//! Music and sound effects.
//!
//! Music: one piece at a time, crossfaded.
//!
//! A bed of day or night pieces (`DayNight`) plays, the next one every few
//! minutes. When the camera rests in a god's land, that god's pieces for its
//! present stage take over (light, mid, dark: the more it is fed, the
//! darker). When a god changes stage, its new piece is heard for a while
//! wherever the camera is, so the table hears the god sate or sour. The
//! menu plays the day bed.
//!
//! Pieces are seamless loops in `assets/music/` (made with
//! `scripts/music-set.sh`), sorted by name: `day-*`, `night-*`,
//! `<god>-<light|mid|dark>` with an optional number; each mood goes round
//! its pieces. A mood with none falls back to the bed. `NECROMY_MUSIC=off`
//! mutes it.
//!
//! Effects: any system sends a `Sound` (a name from `assets/sfx/`, made with
//! `scripts/sfx-set.sh`); a variant is picked at random, never the same one
//! twice running, with a slight change of pitch. What the rules report is
//! sounded here from `Match::heard`; what is animated (dice, blows, steps) is
//! sounded by its animation, on the frame it happens. `NECROMY_SFX=off`
//! mutes effects.

use std::path::Path;

use bevy::audio::{AddAudioSource, Volume};
use bevy::prelude::*;
use necromy_rules::{Event, God, Hex, PlayerId};

use crate::board::Board;
use crate::camera::Rig;
use crate::lighting::DayNight;
use crate::play::Match;

/// Loudness of music against everything else, linear.
const MUSIC_GAIN: f32 = 0.7;
/// Seconds for one piece to fade out and the next in.
const FADE_SECS: f32 = 4.0;
/// Seconds the camera must stay in a god's land before its piece starts, so
/// passing over a border does not flip the music.
const SETTLE_SECS: f32 = 3.0;
/// Seconds a piece plays before another of the same mood takes over.
const PIECE_SECS: f32 = 180.0;
/// Seconds a god's new stage is heard after it changes.
const HERALD_SECS: f32 = 40.0;

const STAGES: [&str; 3] = ["light", "mid", "dark"];

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Sound>()
            .add_message::<Tone>()
            .add_audio_source::<Blip>()
            .init_resource::<Speaking>()
            .add_systems(Update, (ui_sounds, speak, play_tones).chain())
            .init_resource::<Effects>()
            .add_systems(crate::InGame, hear_events)
            .add_systems(Update, play_sounds);
        if !off("NECROMY_MUSIC") {
            app.insert_resource(Music::new())
                .add_systems(Update, (conduct, fade).chain());
        }
    }
}

/// What the moment asks the music for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mood {
    Day,
    Night,
    /// A god at a stage: 0 light, 1 mid, 2 dark.
    God(God, u8),
}

impl Mood {
    /// The mood of a piece by its file name: `day-*`, `night-*`, or
    /// `<god>-<stage>` with an optional number (`maya-dark3`).
    fn of(stem: &str) -> Option<Mood> {
        let (head, rest) = stem.split_once('-')?;
        match head {
            "day" => Some(Mood::Day),
            "night" => Some(Mood::Night),
            _ => {
                let god = God::ALL
                    .into_iter()
                    .find(|g| g.name().eq_ignore_ascii_case(head))?;
                let stage = rest.trim_end_matches(|c: char| c.is_ascii_digit());
                let stage = STAGES.iter().position(|&s| s == stage)?;
                Some(Mood::God(god, stage as u8))
            }
        }
    }
}

#[derive(Resource)]
struct Music {
    /// Every piece in `assets/music/`: its mood and file name.
    pieces: Vec<(Mood, String)>,
    /// Where each mood's round of pieces has got to.
    turns: Vec<(Mood, usize)>,
    /// The piece playing (or fading in) and its player.
    playing: Option<(usize, Entity)>,
    /// Seconds the present piece has played.
    played: f32,
    /// Where the rounds start: somewhere different every run.
    start: usize,
    /// The god whose land the camera is in, and for how long.
    land: Option<God>,
    land_secs: f32,
    /// Stages as last seen, to hear them change.
    stages: Option<[u8; 5]>,
    /// Gods whose new stage is to be heard, the one playing first, and
    /// seconds it has had.
    heralds: Vec<(God, u8)>,
    herald_secs: f32,
}

impl Music {
    fn new() -> Music {
        let mut pieces: Vec<(Mood, String)> = std::fs::read_dir(Path::new("assets/music"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|f| {
                let stem = f.strip_suffix(".ogg")?;
                Some((Mood::of(stem)?, f))
            })
            .collect();
        pieces.sort_by(|a, b| a.1.cmp(&b.1));
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos() as usize);
        Music {
            pieces,
            turns: Vec::new(),
            playing: None,
            played: 0.0,
            start,
            land: None,
            land_secs: 0.0,
            stages: None,
            heralds: Vec::new(),
            herald_secs: 0.0,
        }
    }

    fn has(&self, mood: Mood) -> bool {
        self.pieces.iter().any(|(m, _)| *m == mood)
    }

    /// The next piece of a mood, round and round.
    fn next(&mut self, mood: Mood) -> Option<usize> {
        let of: Vec<usize> = (0..self.pieces.len())
            .filter(|&i| self.pieces[i].0 == mood)
            .collect();
        if of.is_empty() {
            return None;
        }
        let turn = match self.turns.iter_mut().find(|(m, _)| *m == mood) {
            Some((_, turn)) => turn,
            None => {
                self.turns.push((mood, self.start));
                &mut self.turns.last_mut().expect("just pushed").1
            }
        };
        let pick = of[*turn % of.len()];
        *turn += 1;
        Some(pick)
    }
}

/// A music player and how loud it should be.
#[derive(Component)]
struct Voice {
    gain: f32,
    target: f32,
}

/// Picks the piece for the moment and starts it when it changes.
#[allow(clippy::too_many_arguments)]
fn conduct(
    mut commands: Commands,
    mut music: ResMut<Music>,
    mut voices: Query<&mut Voice>,
    game: Option<Res<Match>>,
    day_night: Option<Res<DayNight>>,
    rig: Option<Res<Rig>>,
    board: Option<Res<Board>>,
    assets: Res<AssetServer>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    music.played += dt;
    let night = game.is_some() && day_night.is_some_and(|d| d.night > 0.5);

    let mut god_mood = None;
    if let Some(game) = &game {
        let stages = God::ALL.map(|g| game.game.stage(g));
        // A stage that changed is heralded; the first look only remembers.
        if let Some(before) = music.stages {
            for god in God::ALL {
                let stage = stages[god.index()];
                if stage != before[god.index()] {
                    debug!("music: {god:?} stage {} -> {stage}", before[god.index()]);
                    music.heralds.retain(|&(g, _)| g != god);
                    music.heralds.push((god, stage));
                }
            }
        }
        music.stages = Some(stages);

        let land = rig.zip(board).and_then(|(rig, board)| {
            let hex = board.world_to_hex(rig.focus());
            game.game.board().tile(hex).and_then(|t| t.region)
        });
        if land != music.land {
            music.land = land;
            music.land_secs = 0.0;
        }
        music.land_secs += dt;

        if let Some(&(god, stage)) = music.heralds.first() {
            music.herald_secs += dt;
            if music.herald_secs > HERALD_SECS {
                music.heralds.remove(0);
                music.herald_secs = 0.0;
            }
            god_mood = Some(Mood::God(god, stage));
        } else if let Some(god) = music.land
            && music.land_secs > SETTLE_SECS
        {
            god_mood = Some(Mood::God(god, game.game.stage(god)));
        }
    } else {
        music.stages = None;
        music.heralds.clear();
    }

    // A god's piece if there is one for the moment, else the day or night bed.
    let mood =
        god_mood
            .filter(|&m| music.has(m))
            .unwrap_or(if night { Mood::Night } else { Mood::Day });
    let playing = music.playing.map(|(i, _)| i);
    // Keep the piece that plays until its time is up, then the next one of
    // its mood; a mood with one piece keeps it.
    let keep = playing.filter(|&i| music.pieces[i].0 == mood && music.played < PIECE_SECS);
    let wanted = keep.or_else(|| music.next(mood));
    if wanted == playing {
        if music.played >= PIECE_SECS {
            music.played = 0.0;
        }
        return;
    }

    if let Some((_, old)) = music.playing.take()
        && let Ok(mut voice) = voices.get_mut(old)
    {
        voice.target = 0.0;
    }
    music.played = 0.0;
    let Some(piece) = wanted else {
        debug!("music: silence ({mood:?})");
        return;
    };
    let name = &music.pieces[piece].1;
    debug!("music: {name} ({mood:?}, land {:?})", music.land);
    let player = commands
        .spawn((
            AudioPlayer::new(assets.load(format!("music/{name}"))),
            PlaybackSettings::LOOP.with_volume(Volume::SILENT),
            Voice {
                gain: 0.0,
                target: 1.0,
            },
        ))
        .id();
    music.playing = Some((piece, player));
}

/// Eases each player's volume to its target; a silenced one goes away.
fn fade(
    mut commands: Commands,
    mut voices: Query<(Entity, &mut Voice, Option<&mut AudioSink>)>,
    time: Res<Time>,
    settings: Res<crate::settings::Settings>,
) {
    let step = time.delta_secs() / FADE_SECS;
    for (entity, mut voice, sink) in &mut voices {
        voice.gain = if voice.gain < voice.target {
            (voice.gain + step).min(voice.target)
        } else {
            (voice.gain - step).max(voice.target)
        };
        if let Some(mut sink) = sink {
            // Equal-power curve: the crossfade does not dip in the middle.
            let level = (voice.gain * std::f32::consts::FRAC_PI_2).sin();
            sink.set_volume(Volume::Linear(level * MUSIC_GAIN * settings.music_gain()));
        }
        if voice.gain == 0.0 && voice.target == 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn off(var: &str) -> bool {
    std::env::var(var).is_ok_and(|v| v == "off")
}

/// Loudness of effects against everything else, linear.
const SFX_GAIN: f32 = 0.8;
/// The same sound again sooner than this is dropped: a batch of events (a
/// bot's whole action) must not stack one sound into a blare.
const REPEAT_SECS: f32 = 0.08;
/// At most this many effects start in one frame.
const PER_FRAME: usize = 4;
/// Rivals' doings are heard this many hexes from where the camera looks.
pub const NEAR_HEXES: u32 = 4;
/// Rivals' doings are heard one at a time, this far apart.
const ASIDE_GAP_SECS: f32 = 0.5;

/// A sound effect to play: a name from `assets/sfx/` and a loudness, 1 is
/// the sound's usual level.
#[derive(Message, Clone, Copy, Debug)]
pub struct Sound {
    pub name: &'static str,
    pub volume: f32,
}

impl Sound {
    pub const fn new(name: &'static str) -> Sound {
        Sound { name, volume: 1.0 }
    }

    pub const fn at(self, volume: f32) -> Sound {
        Sound { volume, ..self }
    }
}

/// How loud each sound is against the others: takes are all peak-normalised,
/// so busy little sounds are turned down and the rare big ones kept up.
fn level(name: &str) -> f32 {
    match name {
        "step" => 0.25,
        "menu-pick" | "menu-flap" => 0.4,
        "menu-love" => 0.45,
        "menu-squeak" | "menu-thud" => 0.5,
        "menu-thump" | "menu-swallow" => 0.6,
        "card-draw" | "card-flip" | "die-die" => 0.45,
        "die-table" | "dice-shake" | "swing" | "spirit" | "offer" | "guard-march"
        | "poison-bite" => 0.55,
        "card-play" | "chain" | "block" | "hurt" | "heal" | "trap-set" | "hide" | "reveal"
        | "blink" | "haste" | "corpse" | "window" | "poison-laid" | "poison-fed"
        | "poison-cured" => 0.65,
        "dawn" | "dusk" | "turn" => 0.7,
        _ => 0.8,
    }
}

#[derive(Resource)]
struct Effects {
    /// Variants of each sound, as found in `assets/sfx/`.
    bank: Vec<(String, Vec<Handle<AudioSource>>)>,
    /// When each sound last started, and which variant.
    last: Vec<(&'static str, f32, usize)>,
    rng: u64,
    muted: bool,
}

impl FromWorld for Effects {
    fn from_world(world: &mut World) -> Effects {
        let assets = world.resource::<AssetServer>();
        let mut files: Vec<String> = std::fs::read_dir(Path::new("assets/sfx"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|f| f.ends_with(".ogg"))
            .collect();
        files.sort();
        let mut bank: Vec<(String, Vec<Handle<AudioSource>>)> = Vec::new();
        for file in files {
            // NAME-K.ogg
            let Some((name, _)) = file.trim_end_matches(".ogg").rsplit_once('-') else {
                continue;
            };
            let handle = assets.load(format!("sfx/{file}"));
            match bank.iter_mut().find(|(n, _)| n == name) {
                Some((_, variants)) => variants.push(handle),
                None => bank.push((name.to_string(), vec![handle])),
            }
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        Effects {
            bank,
            last: Vec::new(),
            rng: seed | 1,
            muted: off("NECROMY_SFX"),
        }
    }
}

impl Effects {
    /// xorshift: effects only need to vary, not to agree between clients.
    fn roll(&mut self, n: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % n.max(1) as u64) as usize
    }
}

fn play_sounds(
    mut commands: Commands,
    mut sounds: MessageReader<Sound>,
    mut effects: ResMut<Effects>,
    time: Res<Time>,
    settings: Res<crate::settings::Settings>,
) {
    let now = time.elapsed_secs();
    let mut started = 0;
    for sound in sounds.read() {
        if effects.muted || started == PER_FRAME {
            continue;
        }
        let Some(variants) = effects
            .bank
            .iter()
            .find(|(n, _)| n == sound.name)
            .map(|(_, v)| v.clone())
        else {
            warn_once!("no sound effect named {}", sound.name);
            continue;
        };
        let last = effects.last.iter().position(|l| l.0 == sound.name);
        if let Some(i) = last
            && now - effects.last[i].1 < REPEAT_SECS
        {
            continue;
        }
        // A different variant from last time, when there is a choice.
        let mut pick = effects.roll(variants.len());
        if let Some(i) = last
            && variants.len() > 1
            && pick == effects.last[i].2
        {
            pick = (pick + 1) % variants.len();
        }
        match last {
            Some(i) => effects.last[i] = (sound.name, now, pick),
            None => effects.last.push((sound.name, now, pick)),
        }
        debug!("sfx: {} #{pick} at {:.2}", sound.name, sound.volume);
        let speed = 0.94 + effects.roll(13) as f32 * 0.01;
        commands.spawn((
            AudioPlayer::new(variants[pick].clone()),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(
                    sound.volume.clamp(0.0, 1.5)
                        * level(sound.name)
                        * SFX_GAIN
                        * settings.effects_gain(),
                ))
                .with_speed(speed),
        ));
        started += 1;
    }
}

// Procedural blips: clicks, keys and the gods' voices, drawn sample by
// sample on the spot. Each is a few dozen milliseconds, so it is rendered
// whole into a buffer and played like any other source.

const SYNTH_RATE: u32 = 44_100;
/// Loudness of blips against everything else, linear.
const BLIP_GAIN: f32 = 0.35;
/// Letters a god speaks per second when its words are typed out.
const LETTERS_PER_SEC: f32 = 32.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Wave {
    Sine,
    Triangle,
    /// A pulse with this share of the period up.
    Pulse(f32),
    Noise,
}

/// One blip: a wave at a pitch that slides, under a short envelope.
#[derive(Message, Clone, Copy, Debug)]
pub struct Tone {
    pub wave: Wave,
    pub hz: f32,
    /// Pitch at the end against the start: 0.5 falls an octave.
    pub slide: f32,
    pub secs: f32,
    pub volume: f32,
    /// A second partial an octave and a fifth up, for a bell-like ring.
    pub ring: f32,
}

impl Tone {
    const fn new(wave: Wave, hz: f32, secs: f32) -> Tone {
        Tone {
            wave,
            hz,
            slide: 1.0,
            secs,
            volume: 1.0,
            ring: 0.0,
        }
    }

    /// A button pressed.
    pub const fn click() -> Tone {
        Tone {
            slide: 0.6,
            volume: 0.8,
            ..Tone::new(Wave::Pulse(0.5), 900.0, 0.035)
        }
    }

    /// The pointer comes onto a button.
    pub const fn hover() -> Tone {
        Tone {
            volume: 0.25,
            ..Tone::new(Wave::Sine, 1500.0, 0.018)
        }
    }

    /// A key typed into a field; a deletion a little lower.
    pub const fn key(delete: bool) -> Tone {
        Tone {
            volume: 0.45,
            ..Tone::new(Wave::Noise, if delete { 2200.0 } else { 3400.0 }, 0.014)
        }
    }

    /// One letter in a god's voice. Its element sets the timbre and the
    /// register; the letter itself nudges the pitch, so a word sounds the
    /// same every time it is said.
    pub fn voice(god: God, letter: char) -> Tone {
        let base = match god {
            // Wood: a hollow knock, middle voice.
            God::Bhava => Tone::new(Wave::Triangle, 330.0, 0.05),
            // Fire: a bright buzz, quick and high.
            God::Trishna => Tone {
                volume: 0.6,
                ..Tone::new(Wave::Pulse(0.5), 520.0, 0.04)
            },
            // Earth: low and slow.
            God::Zaga => Tone {
                slide: 0.9,
                ..Tone::new(Wave::Triangle, 170.0, 0.07)
            },
            // Metal: a thin pulse that rings.
            God::Ahamar => Tone {
                volume: 0.5,
                ring: 0.35,
                ..Tone::new(Wave::Pulse(0.25), 400.0, 0.05)
            },
            // Water: a pure tone that falls away.
            God::Maya => Tone {
                slide: 0.8,
                ..Tone::new(Wave::Sine, 460.0, 0.06)
            },
        };
        let step = (letter.to_lowercase().next().unwrap_or(letter) as u32 % 7) as f32 - 3.0;
        Tone {
            hz: base.hz * 2f32.powf(step * 2.0 / 12.0),
            ..base
        }
    }

    fn render(self, seed: u32) -> Vec<f32> {
        let n = (self.secs * SYNTH_RATE as f32) as usize;
        let attack = (0.003 * SYNTH_RATE as f32) as usize;
        let mut phase = 0.0f32;
        let mut noise = seed | 1;
        let mut held = 0.0f32;
        (0..n)
            .map(|i| {
                let x = i as f32 / n as f32;
                let hz = self.hz * self.slide.powf(x);
                let before = phase;
                phase = (phase + hz / SYNTH_RATE as f32).fract();
                let v = match self.wave {
                    Wave::Sine => (phase * std::f32::consts::TAU).sin(),
                    Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
                    Wave::Pulse(duty) => {
                        if phase < duty {
                            0.7
                        } else {
                            -0.7
                        }
                    }
                    // Noise held for one period of the pitch: higher is hissier.
                    Wave::Noise => {
                        if phase < before {
                            noise ^= noise << 13;
                            noise ^= noise >> 17;
                            noise ^= noise << 5;
                            held = noise as f32 / u32::MAX as f32 * 2.0 - 1.0;
                        }
                        held
                    }
                };
                let ring = self.ring * (phase * 3.0 * std::f32::consts::TAU).sin();
                // A quick rise, then a fall that ends at silence: no clicks.
                let env = (i as f32 / attack.max(1) as f32).min(1.0) * (1.0 - x).powi(2);
                (v + ring) * env * self.volume
            })
            .collect()
    }
}

/// A rendered blip, played as an audio source.
#[derive(Asset, TypePath, Clone)]
pub struct Blip(std::sync::Arc<[f32]>);

pub struct BlipSamples {
    samples: std::sync::Arc<[f32]>,
    at: usize,
}

impl Iterator for BlipSamples {
    type Item = bevy::audio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let s = self.samples.get(self.at).copied();
        self.at += 1;
        s
    }
}

impl bevy::audio::Source for BlipSamples {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len() - self.at.min(self.samples.len()))
    }

    fn channels(&self) -> bevy::audio::ChannelCount {
        bevy::audio::ChannelCount::MIN
    }

    fn sample_rate(&self) -> bevy::audio::SampleRate {
        bevy::audio::SampleRate::new(SYNTH_RATE).expect("non-zero rate")
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs_f32(
            self.samples.len() as f32 / SYNTH_RATE as f32,
        ))
    }
}

impl bevy::audio::Decodable for Blip {
    type Decoder = BlipSamples;

    fn decoder(&self) -> BlipSamples {
        BlipSamples {
            samples: self.0.clone(),
            at: 0,
        }
    }
}

fn play_tones(
    mut commands: Commands,
    mut tones: MessageReader<Tone>,
    mut blips: ResMut<Assets<Blip>>,
    mut effects: ResMut<Effects>,
    settings: Res<crate::settings::Settings>,
) {
    for tone in tones.read() {
        if effects.muted {
            continue;
        }
        let seed = effects.roll(u32::MAX as usize) as u32;
        let blip = blips.add(Blip(tone.render(seed).into()));
        commands.spawn((
            AudioPlayer(blip),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(BLIP_GAIN * settings.effects_gain())),
        ));
    }
}

/// Buttons click when pressed and tick when the pointer comes onto them;
/// a hand card whispers as it rises.
#[allow(clippy::type_complexity)]
fn ui_sounds(
    buttons: Query<
        (
            &Interaction,
            Option<&crate::ui_skin::Frame>,
            Has<crate::hud::HandCard>,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut tones: MessageWriter<Tone>,
    mut sounds: MessageWriter<Sound>,
) {
    for (interaction, frame, card) in &buttons {
        match interaction {
            Interaction::Pressed => {
                tones.write(Tone::click());
            }
            Interaction::Hovered if card => {
                sounds.write(Sound::new("card-draw").at(0.4));
            }
            Interaction::Hovered if frame == Some(&crate::ui_skin::Frame::Button) => {
                tones.write(Tone::hover());
            }
            _ => {}
        }
    }
}

/// Words a god says, typed out letter by letter in its voice. The first
/// `from` characters (who speaks) show at once. Panels that rebuild often
/// keep the same `key` for the same words, and the typing carries on.
#[derive(Component)]
pub struct Speech {
    pub key: u64,
    pub god: God,
    pub text: String,
    pub from: usize,
}

/// The words being typed out now: their key, when they began, letters voiced.
#[derive(Resource, Default)]
struct Speaking {
    key: Option<u64>,
    since: f32,
    voiced: usize,
}

fn speak(
    time: Res<Time>,
    mut speaking: ResMut<Speaking>,
    mut texts: Query<(&Speech, &mut Text)>,
    mut tones: MessageWriter<Tone>,
) {
    let now = time.elapsed_secs();
    for (speech, mut text) in &mut texts {
        if speaking.key != Some(speech.key) {
            *speaking = Speaking {
                key: Some(speech.key),
                since: now,
                voiced: 0,
            };
        }
        let typed = ((now - speaking.since) * LETTERS_PER_SEC) as usize;
        let shown: String = speech.text.chars().take(speech.from + typed).collect();
        if text.0 != shown {
            text.0 = shown;
        }
        // Every other letter voiced: each one is a babble.
        let letters: Vec<char> = speech.text.chars().skip(speech.from).take(typed).collect();
        while speaking.voiced < letters.len() {
            let letter = letters[speaking.voiced];
            if letter.is_alphabetic() && speaking.voiced.is_multiple_of(2) {
                tones.write(Tone::voice(speech.god, letter));
            }
            speaking.voiced += 1;
        }
    }
}

/// Where a sound belongs, which decides whether the human hears it.
enum Heard {
    /// The human's own doings, what is done to them, the world's turns.
    Always,
    /// Something at a hex, or a rival's doing where their champion stands:
    /// heard only near where the camera looks.
    At(Hex),
    /// A rival's doing with no place (a chain, an answer that fizzled).
    Aside,
}

/// Sounds what the rules report. Battles are left to the fight on the
/// battle panel, which deals their blows and deaths in its own time.
/// Rivals act at the same time as the human (simultaneous turns): only what
/// happens near the camera is heard, and one such sound at a time, so a
/// busy table does not drown the human's own play.
#[allow(clippy::too_many_arguments)]
fn hear_events(
    mut game: ResMut<Match>,
    mut sounds: MessageWriter<Sound>,
    mut stages: Local<Option<[u8; 5]>>,
    mut aside_at: Local<f32>,
    rig: Option<Res<Rig>>,
    board: Option<Res<Board>>,
    time: Res<Time>,
) {
    let stages = stages.get_or_insert_with(|| God::ALL.map(|g| game.game.stage(g)));
    if game.heard.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    let human = game.human;
    let focus = rig
        .zip(board)
        .map(|(rig, board)| board.world_to_hex(rig.focus()));
    // Harm and deaths after a battle resolves still come in its batch.
    let mut in_battle = game.battle.is_some();
    let heard = std::mem::take(&mut game.bypass_change_detection().heard);
    let by = |p: PlayerId| match game.game.champion(p) {
        _ if p == human => Heard::Always,
        Some(c) => Heard::At(c.hex),
        None => Heard::Aside,
    };
    for event in &heard {
        let (sound, heard) = match event {
            Event::Dawn { .. } => (Sound::new("dawn"), Heard::Always),
            Event::Dusk { .. } => (Sound::new("dusk"), Heard::Always),
            Event::TurnStarted { player, .. } if *player == human => {
                (Sound::new("turn"), Heard::Always)
            }
            Event::CardDrawn { player, .. } if *player == human => {
                (Sound::new("card-draw"), Heard::Always)
            }
            Event::CardPlayed { player, .. } => (Sound::new("card-play"), by(*player)),
            Event::Chain { player, .. } => (Sound::new("item-gain"), by(*player)),
            Event::WindowOpened { eligible, .. } if eligible.contains(&human) => {
                (Sound::new("window"), Heard::Always)
            }
            Event::Canceled { .. } | Event::Fizzled { .. } => (Sound::new("fizzle"), Heard::Aside),
            Event::Damaged { player, .. } if !in_battle => (Sound::new("hurt"), by(*player)),
            Event::ChampionFell { player, .. } if !in_battle => (Sound::new("fall"), by(*player)),
            Event::Healed { player, .. } => (Sound::new("heal"), by(*player)),
            Event::WardRaised { player, .. } => (Sound::new("ward-raise"), by(*player)),
            Event::WardBroken { player, .. } => (Sound::new("ward-break"), by(*player)),
            Event::Rooted { player } => (Sound::new("root"), by(*player)),
            Event::Poisoned { player, .. } => (Sound::new("poison-laid"), by(*player)),
            Event::PoisonBit { player, .. } => (Sound::new("poison-bite"), by(*player)),
            // Creation mode (§21.8).
            Event::FireStarted { hex, .. } => (Sound::new("fire-catch").at(0.6), Heard::At(*hex)),
            Event::PiranhasBit { player, .. } => (Sound::new("bite"), by(*player)),
            Event::Moved { player, to, .. }
                if game
                    .game
                    .board()
                    .tile(*to)
                    .is_some_and(|t| t.terrain.is_water()) =>
            {
                (Sound::new("splash"), by(*player))
            }
            Event::Built { hex, .. }
            | Event::QuarterRaised { hex, .. }
            | Event::RoadLaid { hex } => (Sound::new("build"), Heard::At(*hex)),
            Event::FairOpened { hex, .. }
            | Event::GoodsSold { hex, .. }
            | Event::Feasted { hex, .. } => (Sound::new("fair"), Heard::At(*hex)),
            Event::Wedding { a, .. } => (Sound::new("wedding"), Heard::At(*a)),
            Event::GodActed { .. } => (Sound::new("dusk").at(0.5), Heard::Always),
            Event::Waylaid { hex, .. } => (Sound::new("hit"), Heard::At(*hex)),
            Event::GateOpened { hex, .. } => (Sound::new("gate"), Heard::At(*hex)),
            Event::DragonHatched { hex, .. } => (Sound::new("hatch"), Heard::At(*hex)),
            Event::Delved { hex, .. }
            | Event::Buried { hex, .. }
            | Event::CircleDrawn { hex, .. } => (Sound::new("dig"), Heard::At(*hex)),
            Event::PoisonFed { player, .. } => (Sound::new("poison-fed"), by(*player)),
            Event::PoisonCured { player, .. } => (Sound::new("poison-cured"), by(*player)),
            // Items (§20.3).
            Event::ItemGained { player, .. } => (Sound::new("item-gain"), by(*player)),
            Event::ItemBroken { player, .. } => (Sound::new("item-break"), by(*player)),
            Event::ItemSacrificed { player, .. } => (Sound::new("item-offer"), by(*player)),
            Event::ItemToll { player, .. } => (Sound::new("curse-bit").at(0.6), by(*player)),
            // Trials (§20.2).
            Event::TrialSet { trial } => (Sound::new("trial-set").at(0.7), Heard::At(trial.hex)),
            Event::TrialBegun { player, .. } => (Sound::new("trial-begin"), by(*player)),
            Event::TrialPassed { player, .. } => (Sound::new("trial-pass"), by(*player)),
            Event::TrialFailed { player, .. } => (Sound::new("trial-fail"), by(*player)),
            // A step of the path to a deed (docs/storyteller-plan.md).
            Event::StepDone { player } => (Sound::new("trial-pass").at(0.5), by(*player)),
            Event::Patron { player, .. } => (Sound::new("stage-light"), by(*player)),
            Event::Betrayed { player, .. } => (Sound::new("stage-dark"), by(*player)),
            Event::Hasted { player, .. } => (Sound::new("haste"), by(*player)),
            Event::Blinked { player, .. } => (Sound::new("blink"), by(*player)),
            Event::TrapSet { player, .. } => (Sound::new("trap-set"), by(*player)),
            Event::TrapSprung { owner, .. } if *owner == human => {
                (Sound::new("trap-sprung"), Heard::Always)
            }
            Event::TrapSprung { victim, .. } => (Sound::new("trap-sprung"), by(*victim)),
            Event::Hid { player, .. } => (Sound::new("hide"), by(*player)),
            Event::Revealed { player, .. } => (Sound::new("reveal"), by(*player)),
            Event::CorpseAppeared { hex } => (Sound::new("corpse").at(0.7), Heard::At(*hex)),
            Event::CorpseTaken { hex } => (Sound::new("corpse-taken"), Heard::At(*hex)),
            Event::GroveGrew { hex } => (Sound::new("grove"), Heard::At(*hex)),
            Event::TerrainChanged { hex, .. } => (Sound::new("terrain"), Heard::At(*hex)),
            // Nearly every card is an offering too: only the human's own are heard.
            Event::Offered {
                player: Some(player),
                ..
            } if *player == human => (Sound::new("offer"), Heard::Always),
            Event::StageChanged { god, stage } => {
                // A higher stage is a darker one.
                let before = std::mem::replace(&mut stages[god.index()], *stage);
                let name = if *stage > before {
                    "stage-dark"
                } else {
                    "stage-light"
                };
                (Sound::new(name), Heard::Always)
            }
            Event::WishGranted { .. } => (Sound::new("wish-granted"), Heard::Always),
            Event::WishRefused { .. } => (Sound::new("wish-refused"), Heard::Always),
            Event::CurseLaid { player, .. } => (Sound::new("curse-laid"), by(*player)),
            Event::CurseBit { player, .. } => (Sound::new("curse-bit"), by(*player)),
            Event::Crowned { .. } => (Sound::new("crown"), Heard::Always),
            Event::GuardSpawned { .. } => (Sound::new("guard-arrive"), Heard::Always),
            // The human's own battle comes up on screen and its blows sound
            // there (`fight.rs`); another's is heard only near the camera,
            // blows and all, until the human looks at it (`watch_ui.rs`).
            Event::BattleStarted { attacker, defender } => {
                if [*attacker, *defender].contains(&human) {
                    in_battle = true;
                    (Sound::new("battle-start"), Heard::Always)
                } else {
                    (Sound::new("battle-start"), by(*defender))
                }
            }
            Event::GuardAttacked { attacker } => {
                if *attacker == human {
                    in_battle = true;
                    (Sound::new("battle-start"), Heard::Always)
                } else {
                    (Sound::new("battle-start"), by(*attacker))
                }
            }
            Event::GuardFell { by: who, .. } => (Sound::new("guard-fall"), by(*who)),
            // The dead, Bhava's beasts and the militia (§20.4).
            Event::MobAppeared { mob } if mob.is_beast() => {
                (Sound::new("beast-appear"), Heard::At(mob.hex))
            }
            Event::MobAppeared { mob } => (Sound::new("undead-rise"), Heard::At(mob.hex)),
            Event::MobLeft { id } => (
                Sound::new("beast-leave"),
                game.seen_mob(*id)
                    .map_or(Heard::Aside, |m| Heard::At(m.hex)),
            ),
            Event::BeastMauled { beast, .. } => (
                Sound::new("beast-strike"),
                game.seen_mob(*beast)
                    .map_or(Heard::Aside, |m| Heard::At(m.hex)),
            ),
            Event::UndeadHitMilitia { home, .. } => (Sound::new("undead-strike"), Heard::At(*home)),
            Event::MilitiaSwapped { player, .. } => (Sound::new("militia-greet"), by(*player)),
            Event::MilitiaAttacked { attacker, .. } => {
                if *attacker == human {
                    in_battle = true;
                    (Sound::new("militia-alarm"), Heard::Always)
                } else {
                    (Sound::new("militia-alarm"), by(*attacker))
                }
            }
            Event::SettlementRebuilt { player, .. } => {
                (Sound::new("settlement-rebuilt"), by(*player))
            }
            Event::MobStruck { id, target } => {
                let name = if game.mob_kind(*id).is_beast() {
                    "beast-strike"
                } else {
                    "battle-start"
                };
                if *target == human {
                    in_battle = true;
                    (Sound::new(name), Heard::Always)
                } else {
                    (Sound::new(name), by(*target))
                }
            }
            Event::MobAttacked { attacker, .. } => {
                if *attacker == human {
                    in_battle = true;
                    (Sound::new("battle-start"), Heard::Always)
                } else {
                    (Sound::new("battle-start"), by(*attacker))
                }
            }
            Event::MobFell { id, hex, .. } if game.mob_kind(*id).is_beast() => {
                (Sound::new("beast-fall"), Heard::At(*hex))
            }
            Event::MobFell { hex, .. } => (Sound::new("undead-rest"), Heard::At(*hex)),
            Event::MilitiaStruck { hex, .. } => (Sound::new("militia-strike"), Heard::At(*hex)),
            Event::SettlementRuined { hex } => (Sound::new("settlement-ruined"), Heard::At(*hex)),
            Event::MilitiaHelped { player, .. } => (Sound::new("heal"), by(*player)),
            Event::MilitiaBeat { player, .. } => (Sound::new("hurt"), by(*player)),
            Event::MilitiaHit { player, .. } => (Sound::new("militia-strike"), by(*player)),
            Event::GuardHewed { hex, .. } => (Sound::new("guard-hew"), Heard::At(*hex)),
            Event::GuardStruck { target } => {
                if *target == human {
                    in_battle = true;
                    (Sound::new("battle-start"), Heard::Always)
                } else {
                    (Sound::new("battle-start"), by(*target))
                }
            }
            Event::Burned { player, .. } => (Sound::new("card-burn"), by(*player)),
            Event::Victory { player, .. } => {
                let name = if *player == human {
                    "victory"
                } else {
                    "defeat"
                };
                (Sound::new(name), Heard::Always)
            }
            _ => continue,
        };
        let aside = match heard {
            Heard::Always => false,
            Heard::At(hex) => {
                if focus.is_none_or(|f| f.unsigned_distance_to(hex) > NEAR_HEXES) {
                    continue;
                }
                true
            }
            Heard::Aside => true,
        };
        if aside {
            if now - *aside_at < ASIDE_GAP_SECS {
                continue;
            }
            *aside_at = now;
        }
        sounds.write(if aside {
            sound.at(sound.volume * 0.6)
        } else {
            sound
        });
    }
}
