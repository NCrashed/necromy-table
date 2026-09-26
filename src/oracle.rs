//! The gods' voice in the game (docs/design.md §7.6, §8.6): the Bevy side of
//! `necromy-oracle`.
//!
//! A probe thread checks every few seconds whether a `llama-server` answers
//! (`scripts/oracle-server.sh`). While it does, the human's wish is written
//! in free words and judged by the model, and gods' replies and story voices
//! are written by it too. While it does not, everything falls back to the
//! prepared wishes and the template voices, so the game never waits on it.
//!
//! Only the human's wish changes the game, and only through an ordinary
//! intent that carries the model's reading. Voices are words on screen.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use necromy_oracle::{Job, Oracle, prompt};
use necromy_rules::{God, Intent};

use crate::play::Match;

/// Seconds between health probes.
const PROBE_SECS: u64 = 5;
/// Cosmetic jobs are skipped while this many are already waiting.
const MAX_QUEUED_VOICES: usize = 2;

pub struct OraclePlugin;

impl Plugin for OraclePlugin {
    fn build(&self, app: &mut App) {
        let addr = necromy_oracle::addr_from_env();
        let online = Arc::new(AtomicBool::new(false));
        {
            let (addr, online) = (addr.clone(), online.clone());
            std::thread::Builder::new()
                .name("oracle-probe".into())
                .spawn(move || {
                    loop {
                        online.store(necromy_oracle::alive(&addr), Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_secs(PROBE_SECS));
                    }
                })
                .expect("spawn the oracle probe");
        }
        info!("gods' voice: llama-server at {addr} (NECROMY_ORACLE to change)");
        app.insert_resource(OracleLink {
            oracle: Oracle::spawn(addr),
            online,
            next_id: 0,
            pending: HashMap::new(),
        })
        .init_resource::<Voices>()
        .init_resource::<Hearing>()
        .init_resource::<OracleOnline>()
        .add_systems(Update, (mirror_online, poll, ask_for_voices));
    }
}

/// What a job was for.
enum Purpose {
    /// The human's own wish, to be judged and then played.
    Wish { god: God, text: String },
    /// A god's words for a wish that came without them.
    WishSpeech { serial: u32 },
    /// A god's words for a story line.
    LineVoice { line: u32 },
}

#[derive(Resource)]
pub struct OracleLink {
    oracle: Oracle,
    online: Arc<AtomicBool>,
    next_id: u64,
    pending: HashMap<u64, Purpose>,
}

impl OracleLink {
    pub fn online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    fn voices_queued(&self) -> usize {
        self.pending
            .values()
            .filter(|p| !matches!(p, Purpose::Wish { .. }))
            .count()
    }

    fn send(&mut self, purpose: Purpose, job: impl FnOnce(u64) -> Job) {
        self.next_id += 1;
        let id = self.next_id;
        self.pending.insert(id, purpose);
        self.oracle.send(job(id));
    }

    /// Hand the human's free-text wish to `god` for judgement.
    pub fn ask_wish(&mut self, game: &Match, god: God, text: &str) {
        let (messages, schema) = prompt::wish(&game.game, game.human, god, text);
        self.send(
            Purpose::Wish {
                god,
                text: text.to_string(),
            },
            |id| Job {
                id,
                messages,
                schema: Some(schema),
                max_tokens: 300,
                temperature: 0.6,
            },
        );
    }

    /// A god is still thinking about the human's wish.
    pub fn judging(&self) -> Option<God> {
        self.pending.values().find_map(|p| match p {
            Purpose::Wish { god, .. } => Some(*god),
            _ => None,
        })
    }
}

/// Words the model wrote for things on screen, keyed by what they belong to.
#[derive(Resource, Default)]
pub struct Voices {
    /// By `Match::wish_serial`.
    pub wishes: HashMap<u32, String>,
    /// By story line id.
    pub lines: HashMap<u32, String>,
}

/// Why the last free-text wish could not be heard, if it could not.
#[derive(Resource, Default)]
pub struct Hearing {
    pub failed: Option<String>,
}

fn poll(
    mut link: ResMut<OracleLink>,
    mut game: ResMut<Match>,
    mut voices: ResMut<Voices>,
    mut hearing: ResMut<Hearing>,
) {
    while let Some(answer) = link.oracle.poll() {
        let Some(purpose) = link.pending.remove(&answer.id) else {
            continue;
        };
        match (purpose, answer.result) {
            (Purpose::Wish { god, text }, Ok(reply)) => {
                let human = game.human;
                match prompt::read_wish(&game.game, human, &text, &reply) {
                    Ok((kind, target, said)) => {
                        if let Err(err) = game.act(
                            human,
                            Intent::Wish {
                                god,
                                kind,
                                target,
                                said: Some(said),
                            },
                        ) {
                            hearing.failed = Some(format!("бог не смог исполнить: {err}"));
                        }
                    }
                    Err(err) => {
                        warn!("wish reply unreadable: {err}");
                        hearing.failed = Some("бог ответил невнятно".into());
                    }
                }
            }
            (Purpose::Wish { .. }, Err(err)) => {
                warn!("wish not heard: {err}");
                hearing.failed = Some("бог не ответил".into());
            }
            (Purpose::WishSpeech { serial }, Ok(text)) => {
                voices.wishes.insert(serial, text.trim().to_string());
            }
            (Purpose::LineVoice { line }, Ok(text)) => {
                voices.lines.insert(line, text.trim().to_string());
            }
            (_, Err(err)) => debug!("voice not written: {err}"),
        }
    }
}

/// New wishes without words and new story lines for the human get a god's
/// voice, while the model is up and not buried in work.
fn ask_for_voices(mut link: ResMut<OracleLink>, game: Res<Match>, mut seen: Local<(u32, u32)>) {
    if !game.is_changed() {
        return;
    }
    let (wish_seen, told_seen) = *seen;
    *seen = (game.wish_serial, game.told_serial);
    if !link.online() || link.voices_queued() >= MAX_QUEUED_VOICES {
        return;
    }
    if game.wish_serial != wish_seen
        && let Some(reply) = &game.wish_reply
        && reply.said.is_none()
        && let Some((god, kind, grade)) = reply.wish
    {
        let messages = prompt::wish_speech(&game.game, reply.player, god, kind, grade);
        let serial = game.wish_serial;
        link.send(Purpose::WishSpeech { serial }, |id| Job {
            id,
            messages,
            schema: None,
            max_tokens: 90,
            temperature: 0.8,
        });
    }
    if game.told_serial != told_seen
        && let Some(line) = game.told
    {
        let messages = prompt::line_voice(&game.game, &line);
        link.send(Purpose::LineVoice { line: line.id }, |id| Job {
            id,
            messages,
            schema: None,
            max_tokens: 90,
            temperature: 0.8,
        });
    }
}

/// Whether the model answers, as a resource that changes only when that does,
/// so panels can redraw on it.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct OracleOnline(pub bool);

fn mirror_online(link: Res<OracleLink>, mut online: ResMut<OracleOnline>) {
    online.set_if_neq(OracleOnline(link.online()));
}
