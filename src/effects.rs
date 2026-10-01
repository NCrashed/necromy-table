//! Effects on the board: short bursts of pixel particles where a rule
//! touched a champion, and lingering ones while a state lasts. Drawn in
//! code like `ambient.rs`, unlit so they read at any hour. Nothing here
//! reads or changes the rules beyond what the view shows.
//!
//! Poison (§20.1) first: a splash when it is laid, bubbles when it bites or
//! is fed, steam and sparks when it is purged, and a bubble now and then
//! over whoever carries it. Each in the colour of the poison's element, with
//! the sickly green glint of the icon.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_sprite3d::prelude::*;
use necromy_rules::{Element, Event, God, PlayerId};

use crate::board::TEXELS;
use crate::play::Match;
use crate::token::{Billboard, Token};

/// Heights above the feet, in metres: a champion stands about one metre.
const HEAD: f32 = 1.05;
const CHEST: f32 = 0.55;
/// Particles sit this far towards the camera, so the token does not hide them.
const FORWARD: f32 = 0.35;
/// Seconds between two bubbles over a poisoned champion.
const AURA_EVERY: f32 = 0.9;
const GRAVITY: f32 = 5.5;
/// How long a popped bubble shows its burst.
const POP_LIFE: f32 = 0.12;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, make_sprites).add_systems(
            crate::InGame,
            (demo, burst, aura, fly)
                .chain()
                .after(crate::token::move_tokens),
        );
    }
}

#[derive(Resource)]
pub(crate) struct EffectSprites {
    /// Per element (`Element::index`).
    drop: [Handle<Image>; 5],
    bubble: [Handle<Image>; 5],
    pop: [Handle<Image>; 5],
    pub spark: Handle<Image>,
    steam: Handle<Image>,
    /// Lost health over a champion: a heart, an arrow down and the number,
    /// by amount (index 0 is −1, the last stands for anything more).
    harm: Vec<Handle<Image>>,
}

#[derive(Component)]
pub(crate) struct Particle {
    vel: Vec3,
    /// Pulls down (m/s²); negative lifts, like a bubble.
    gravity: f32,
    age: f32,
    life: f32,
    /// Stops and ends on the ground at this height.
    floor: Option<f32>,
    /// Shown for a moment where it ends.
    then: Option<Handle<Image>>,
}

/// Sprite from rows of palette letters; `.` is transparent.
pub(crate) fn pixels(rows: &[&str], palette: &[(u8, [u8; 4])]) -> Image {
    let (w, h) = (rows[0].len() as u32, rows.len() as u32);
    let mut image = Image::new_fill(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    for (y, row) in rows.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            if let Some((_, rgba)) = palette.iter().find(|(k, _)| *k == c) {
                let i = (y * w as usize + x) * 4;
                data[i..i + 4].copy_from_slice(rgba);
            }
        }
    }
    image
}

fn shade([r, g, b]: [u8; 3], k: f32) -> [u8; 4] {
    let f = |v: u8| (v as f32 * k).clamp(0.0, 255.0) as u8;
    [f(r), f(g), f(b), 255]
}

fn mix([r, g, b]: [u8; 3], [r2, g2, b2]: [u8; 3], t: f32) -> [u8; 3] {
    let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    [f(r, r2), f(g, g2), f(b, b2)]
}

const VENOM: [u8; 3] = [150, 230, 90];
pub(crate) const INK: [u8; 4] = [30, 24, 30, 255];

// Rows of pixels read best one under another.
#[rustfmt::skip]
fn make_sprites(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let colour = |e: Element| mix(God::from_index(e.index()).accent(), VENOM, 0.4);
    let drop = Element::ALL.map(|e| {
        let c = colour(e);
        images.add(pixels(
            &[
                "...k...",
                "..kck..",
                ".kcclk.",
                "kccclck",
                "kcccvck",
                ".kcvvk.",
                "..kkk..",
            ],
            &[
                (b'k', INK),
                (b'c', shade(c, 1.0)),
                (b'l', shade(c, 1.6)),
                (b'v', shade(VENOM, 1.0)),
            ],
        ))
    });
    let bubble = Element::ALL.map(|e| {
        let c = colour(e);
        images.add(pixels(
            &[
                "..kkk..",
                ".kccck.",
                "kcww.ck",
                "kcw..ck",
                "kc...ck",
                ".kcvck.",
                "..kkk..",
            ],
            &[
                (b'k', INK),
                (b'c', shade(c, 1.2)),
                (b'w', [245, 250, 235, 255]),
                (b'v', shade(VENOM, 1.0)),
            ],
        ))
    });
    let pop = Element::ALL.map(|e| {
        let c = colour(e);
        images.add(pixels(
            &[
                "c..c..c",
                ".......",
                "..v.v..",
                "c.....c",
                "..v.v..",
                ".......",
                "c..c..c",
            ],
            &[(b'c', shade(c, 1.4)), (b'v', shade(VENOM, 1.2))],
        ))
    });
    let spark = images.add(pixels(
        &["..y..", "..y..", "yywyy", "..y..", "..y.."],
        &[(b'y', [250, 220, 120, 255]), (b'w', [255, 255, 240, 255])],
    ));
    let steam = images.add(pixels(
        &[".ll.", "lwwl", "lwwl", ".ll."],
        &[(b'l', [200, 206, 212, 255]), (b'w', [240, 244, 246, 255])],
    ));
    commands.insert_resource(EffectSprites {
        drop,
        bubble,
        pop,
        spark,
        steam,
        harm: (1..=HARM_SHOWN).map(|n| images.add(harm_badge(n))).collect(),
    });
}

/// The most lost health a badge spells out; more shows as this.
const HARM_SHOWN: u8 = 9;
/// How long a badge rises before it goes.
const HARM_LIFE: f32 = 1.7;

/// "♥▼N": a red heart, an arrow down and the number of health lost, with an
/// ink rim round all of it so it reads over any ground.
fn harm_badge(amount: u8) -> Image {
    const HEART: [&str; 8] = [
        ".rrr.rrr.",
        "rwwrrrrrr",
        "rwrrrrrrr",
        "rrrrrrrrr",
        ".rrrrrrr.",
        "..rrrrr..",
        "...rrr...",
        "....r....",
    ];
    const ARROW: [&str; 4] = ["aaaaaaa", ".aaaaa.", "..aaa..", "...a..."];
    #[rustfmt::skip]
    const DIGITS: [[&str; 7]; 10] = [
        [".nnn.", "nn.nn", "nn.nn", "nn.nn", "nn.nn", "nn.nn", ".nnn."],
        ["..nn.", ".nnn.", "..nn.", "..nn.", "..nn.", "..nn.", ".nnnn"],
        [".nnn.", "nn.nn", "...nn", "..nn.", ".nn..", "nn...", "nnnnn"],
        ["nnnn.", "...nn", "...nn", ".nnn.", "...nn", "...nn", "nnnn."],
        ["nn.nn", "nn.nn", "nn.nn", "nnnnn", "...nn", "...nn", "...nn"],
        ["nnnnn", "nn...", "nnnn.", "...nn", "...nn", "nn.nn", ".nnn."],
        [".nnn.", "nn...", "nnnn.", "nn.nn", "nn.nn", "nn.nn", ".nnn."],
        ["nnnnn", "...nn", "..nn.", "..nn.", ".nn..", ".nn..", ".nn.."],
        [".nnn.", "nn.nn", "nn.nn", ".nnn.", "nn.nn", "nn.nn", ".nnn."],
        [".nnn.", "nn.nn", "nn.nn", ".nnnn", "...nn", "..nn.", ".nn.."],
    ];
    let (w, h) = (27usize, 10usize);
    let mut grid = vec![b'.'; w * h];
    let mut put = |rows: &[&str], x0: usize, y0: usize| {
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                if c != b'.' {
                    grid[(y0 + y) * w + x0 + x] = c;
                }
            }
        }
    };
    put(&HEART, 1, 1);
    put(&ARROW, 11, 3);
    put(&DIGITS[amount.min(9) as usize], 20, 2);
    // The rim: every empty pixel beside a drawn one.
    let drawn = grid.clone();
    for y in 0..h {
        for x in 0..w {
            if drawn[y * w + x] != b'.' {
                continue;
            }
            let near = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dy)| {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    nx >= 0
                        && ny >= 0
                        && (nx as usize) < w
                        && (ny as usize) < h
                        && drawn[ny as usize * w + nx as usize] != b'.'
                });
            if near {
                grid[y * w + x] = b'k';
            }
        }
    }
    let rows: Vec<String> = grid
        .chunks(w)
        .map(|r| String::from_utf8_lossy(r).into_owned())
        .collect();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    pixels(
        &rows,
        &[
            (b'k', INK),
            (b'r', [214, 40, 48, 255]),
            (b'w', [255, 170, 170, 255]),
            (b'a', [236, 70, 60, 255]),
            (b'n', [255, 236, 226, 255]),
        ],
    )
}

pub(crate) fn sprite(image: Handle<Image>) -> impl Bundle {
    (
        Billboard,
        NotShadowCaster,
        NotShadowReceiver,
        Sprite::from_image(image),
        Sprite3d {
            pixels_per_metre: TEXELS,
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            ..default()
        },
    )
}

/// A small generator for looks only; the rules never see it.
#[derive(Default)]
pub(crate) struct Scatter(u32);

impl Scatter {
    /// −1..1
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

/// Where a champion's effects start: their token's feet, pulled towards
/// the camera. `None` for a rival out of sight: the effect would give
/// them away.
fn anchor(
    game: &Match,
    player: PlayerId,
    tokens: &Query<(&Token, &GlobalTransform)>,
    towards: Vec3,
) -> Option<Vec3> {
    if player != game.human && game.game.is_hidden(player) {
        return None;
    }
    tokens
        .iter()
        .find(|(t, _)| t.player == player)
        .map(|(_, at)| at.translation() + towards * FORWARD)
}

/// Bursts for what just happened (`Match::effects`).
#[allow(clippy::too_many_arguments)]
fn burst(
    mut commands: Commands,
    mut game: ResMut<Match>,
    sprites: Res<EffectSprites>,
    tokens: Query<(&Token, &GlobalTransform)>,
    camera: Single<&Transform, With<crate::TableCamera>>,
    mut scatter: Local<Scatter>,
    // The element of each seat's last poison: a bite that takes the last
    // stack leaves none in the view to colour it by.
    mut known: Local<Vec<Option<Element>>>,
    // Health lost while a fight or a trial is on screen: shown once it is
    // over, or the badge would tell the outcome before the dice land.
    mut harm_held: Local<Vec<(PlayerId, u8)>>,
) {
    let fight_up = game.battle.is_some() || game.trial.is_some();
    if game.effects.is_empty() && (harm_held.is_empty() || fight_up) {
        return;
    }
    let events = std::mem::take(&mut game.bypass_change_detection().effects);
    let towards = *camera.back();
    let r = &mut *scatter;
    let mut spawn = |image: &Handle<Image>, at: Vec3, p: Particle| {
        commands.spawn((p, sprite(image.clone()), Transform::from_translation(at)));
    };
    harm_held.extend(events.iter().filter_map(|e| match *e {
        Event::Damaged { player, amount, .. } if amount > 0 => Some((player, amount)),
        _ => None,
    }));
    if !fight_up {
        for (player, amount) in harm_held.drain(..) {
            let Some(feet) = anchor(&game, player, &tokens, towards) else {
                continue;
            };
            let badge = &sprites.harm[(amount.min(HARM_SHOWN) - 1) as usize];
            spawn(
                badge,
                feet + Vec3::Y * (HEAD + 0.55),
                Particle::new(Vec3::Y * 0.6, 0.3, HARM_LIFE),
            );
        }
    }
    for event in &events {
        let player = match *event {
            Event::Poisoned { player, .. }
            | Event::PoisonBit { player, .. }
            | Event::PoisonFed { player, .. }
            | Event::PoisonCured { player, .. } => player,
            _ => continue,
        };
        let seat = player.0 as usize;
        if known.len() <= seat {
            known.resize(seat + 1, None);
        }
        if let Event::Poisoned { element, .. } = *event {
            known[seat] = Some(element);
        }
        let Some(feet) = anchor(&game, player, &tokens, towards) else {
            continue;
        };
        let element = known[seat]
            .or_else(|| {
                game.game
                    .champion(player)
                    .and_then(|c| c.poison)
                    .map(|p| p.element)
            })
            .or_else(|| game.game.champion(player).map(|c| c.god.element()))
            .unwrap_or(Element::Wood);
        let i = element.index();
        match *event {
            // A splash from above: drops fall onto the champion and the
            // ground around, a few bubbles rise.
            Event::Poisoned { .. } => {
                for _ in 0..10 {
                    let vel = Vec3::new(r.next() * 1.3, 1.2 + r.next().abs() * 1.4, r.next() * 1.3);
                    spawn(
                        &sprites.drop[i],
                        feet + Vec3::Y * HEAD,
                        Particle::new(vel, GRAVITY, 1.4).floor(feet.y + 0.02),
                    );
                }
                for k in 0..3 {
                    bubbles(&mut spawn, &sprites, i, feet, r, 0.2 + k as f32 * 0.15);
                }
            }
            // A sting from within: bubbles rise and pop. The last stack
            // leaves with the bite: fewer bubbles.
            Event::PoisonBit { stacks, .. } => {
                let n = if stacks == 0 { 3 } else { 5 };
                for k in 0..n {
                    bubbles(&mut spawn, &sprites, i, feet, r, k as f32 * 0.08);
                }
            }
            // Fed: a boil of bubbles, more and quicker.
            Event::PoisonFed { .. } => {
                for k in 0..9 {
                    bubbles(&mut spawn, &sprites, i, feet, r, k as f32 * 0.04);
                }
            }
            // Purged: steam hisses off and bright sparks rise.
            Event::PoisonCured { .. } => {
                known[seat] = None;
                for _ in 0..6 {
                    let vel = Vec3::new(r.next() * 0.3, 0.6 + r.next().abs() * 0.4, r.next() * 0.3);
                    let at = feet + Vec3::new(r.next() * 0.25, CHEST, r.next() * 0.25);
                    spawn(&sprites.steam, at, Particle::new(vel, -0.3, 0.9));
                }
                for _ in 0..10 {
                    let vel = Vec3::new(r.next() * 0.9, 1.2 + r.next().abs() * 0.8, r.next() * 0.9);
                    spawn(
                        &sprites.spark,
                        feet + Vec3::Y * CHEST,
                        Particle::new(vel, 1.2, 1.0),
                    );
                }
            }
            _ => {}
        }
    }
}

/// Dev aid: `NECROMY_FX=poison` plays the poison effects over the human's
/// token in a loop, for screenshots. Looks only: the rules never hear of it.
fn demo(time: Res<Time>, mut game: ResMut<Match>, mut next: Local<(f32, usize)>) {
    let fx = std::env::var("NECROMY_FX").ok();
    if !matches!(fx.as_deref(), Some("poison" | "harm")) {
        return;
    }
    let (wait, step) = &mut *next;
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = 1.6;
    let player = game.human;
    let element = game
        .game
        .champion(player)
        .map_or(Element::Earth, |c| c.god.element().quenches());
    let event = match *step % 4 {
        // `=harm`: lost health, one more each time.
        n if fx.as_deref() == Some("harm") => Event::Damaged {
            player,
            amount: n as u8 + 1,
            hp: 1,
        },
        0 => Event::Poisoned {
            player,
            element,
            stacks: 2,
        },
        1 => Event::PoisonBit {
            player,
            amount: 1,
            hp: 1,
            stacks: 1,
        },
        2 => Event::PoisonFed { player, stacks: 2 },
        _ => Event::PoisonCured {
            player,
            by: necromy_rules::Cure::Temple,
        },
    };
    *step += 1;
    game.bypass_change_detection().effects.push(event);
}

/// One bubble from the chest that rises and pops after a while.
fn bubbles(
    spawn: &mut impl FnMut(&Handle<Image>, Vec3, Particle),
    sprites: &EffectSprites,
    element: usize,
    feet: Vec3,
    r: &mut Scatter,
    delay: f32,
) {
    let at = feet + Vec3::new(r.next() * 0.35, CHEST + r.next() * 0.25, r.next() * 0.35);
    let vel = Vec3::new(
        r.next() * 0.15,
        0.55 + r.next().abs() * 0.35,
        r.next() * 0.15,
    );
    let mut p =
        Particle::new(vel, -0.4, 0.7 + r.next().abs() * 0.4).pop(sprites.pop[element].clone());
    // A delay is an age below zero: hidden until it starts.
    p.age = -delay;
    spawn(&sprites.bubble[element], at, p);
}

impl Particle {
    pub fn new(vel: Vec3, gravity: f32, life: f32) -> Particle {
        Particle {
            vel,
            gravity,
            age: 0.0,
            life,
            floor: None,
            then: None,
        }
    }

    pub fn floor(self, y: f32) -> Particle {
        Particle {
            floor: Some(y),
            ..self
        }
    }

    fn pop(self, image: Handle<Image>) -> Particle {
        Particle {
            then: Some(image),
            ..self
        }
    }
}

/// A bubble now and then over everyone who carries poison.
#[allow(clippy::too_many_arguments)]
fn aura(
    mut commands: Commands,
    time: Res<Time>,
    game: Res<Match>,
    sprites: Res<EffectSprites>,
    tokens: Query<(&Token, &GlobalTransform)>,
    camera: Single<&Transform, With<crate::TableCamera>>,
    mut next: Local<f32>,
    mut scatter: Local<Scatter>,
) {
    *next -= time.delta_secs();
    if *next > 0.0 {
        return;
    }
    *next = AURA_EVERY;
    let towards = *camera.back();
    for player in game.game.players() {
        let Some(poison) = game.game.champion(player).and_then(|c| c.poison) else {
            continue;
        };
        let Some(feet) = anchor(&game, player, &tokens, towards) else {
            continue;
        };
        let mut spawn = |image: &Handle<Image>, at: Vec3, p: Particle| {
            commands.spawn((p, sprite(image.clone()), Transform::from_translation(at)));
        };
        bubbles(
            &mut spawn,
            &sprites,
            poison.element.index(),
            feet,
            &mut scatter,
            0.0,
        );
    }
}

/// Particles move, land, pop and go.
fn fly(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut p, mut transform) in &mut particles {
        p.age += dt;
        if p.age < 0.0 {
            transform.scale = Vec3::ZERO;
            continue;
        }
        transform.scale = Vec3::ONE;
        let landed = p
            .floor
            .is_some_and(|y| transform.translation.y <= y && p.vel.y < 0.0);
        if p.age >= p.life || landed {
            // `Sprite3d` makes its material on spawn: a new look is a new entity.
            if let Some(image) = p.then.take() {
                commands.spawn((
                    Particle::new(Vec3::ZERO, 0.0, POP_LIFE),
                    sprite(image),
                    Transform::from_translation(transform.translation),
                ));
            }
            commands.entity(entity).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        transform.translation += p.vel * dt;
    }
}
