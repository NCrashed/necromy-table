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
            (burst, aura, fly).chain().after(crate::token::move_tokens),
        );
    }
}

#[derive(Resource)]
struct EffectSprites {
    /// Per element (`Element::index`).
    drop: [Handle<Image>; 5],
    bubble: [Handle<Image>; 5],
    pop: [Handle<Image>; 5],
    spark: Handle<Image>,
    steam: Handle<Image>,
}

#[derive(Component)]
struct Particle {
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
fn pixels(rows: &[&str], palette: &[(u8, [u8; 4])]) -> Image {
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
const INK: [u8; 4] = [30, 24, 30, 255];

fn make_sprites(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let colour = |e: Element| mix(God::from_index(e.index()).accent(), VENOM, 0.2);
    let drop = Element::ALL.map(|e| {
        let c = colour(e);
        images.add(pixels(
            &["..k..", ".kck.", "kcclk", "kcvck", ".kkk."],
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
            &[".kkk.", "kcwck", "kc.ck", "kcvck", ".kkk."],
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
            &["c...c", ".....", "..v..", ".....", "c...c"],
            &[(b'c', shade(c, 1.4)), (b'v', shade(VENOM, 1.2))],
        ))
    });
    let spark = images.add(pixels(
        &[".y.", "ywy", ".y."],
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
    });
}

fn sprite(image: Handle<Image>) -> impl Bundle {
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
struct Scatter(u32);

impl Scatter {
    /// −1..1
    fn next(&mut self) -> f32 {
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
fn burst(
    mut commands: Commands,
    mut game: ResMut<Match>,
    sprites: Res<EffectSprites>,
    tokens: Query<(&Token, &GlobalTransform)>,
    camera: Single<&Transform, With<crate::TableCamera>>,
    mut scatter: Local<Scatter>,
) {
    if game.effects.is_empty() {
        return;
    }
    let events = std::mem::take(&mut game.bypass_change_detection().effects);
    let towards = *camera.back();
    let r = &mut *scatter;
    let mut spawn = |image: &Handle<Image>, at: Vec3, p: Particle| {
        commands.spawn((p, sprite(image.clone()), Transform::from_translation(at)));
    };
    for event in &events {
        match *event {
            // A splash from above: drops fall onto the champion and the
            // ground around, a few bubbles rise.
            Event::Poisoned {
                player, element, ..
            } => {
                let Some(feet) = anchor(&game, player, &tokens, towards) else {
                    continue;
                };
                let i = element.index();
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
            // A sting from within: bubbles rise and pop.
            Event::PoisonBit { player, stacks, .. } => {
                let Some(feet) = anchor(&game, player, &tokens, towards) else {
                    continue;
                };
                let Some(element) = game
                    .game
                    .champion(player)
                    .map(|c| c.poison.map_or(c.god.element(), |p| p.element))
                else {
                    continue;
                };
                // The last stack leaves with the bite: fewer bubbles.
                let n = if stacks == 0 { 3 } else { 5 };
                for k in 0..n {
                    bubbles(
                        &mut spawn,
                        &sprites,
                        element.index(),
                        feet,
                        r,
                        k as f32 * 0.08,
                    );
                }
            }
            // Fed: a boil of bubbles, bigger and faster.
            Event::PoisonFed { player, .. } => {
                let Some(feet) = anchor(&game, player, &tokens, towards) else {
                    continue;
                };
                let Some(element) = game
                    .game
                    .champion(player)
                    .and_then(|c| c.poison.map(|p| p.element))
                else {
                    continue;
                };
                for k in 0..9 {
                    bubbles(
                        &mut spawn,
                        &sprites,
                        element.index(),
                        feet,
                        r,
                        k as f32 * 0.04,
                    );
                }
            }
            // Purged: steam hisses off and bright sparks rise.
            Event::PoisonCured { player, .. } => {
                let Some(feet) = anchor(&game, player, &tokens, towards) else {
                    continue;
                };
                for _ in 0..6 {
                    let vel = Vec3::new(r.next() * 0.3, 0.6 + r.next().abs() * 0.4, r.next() * 0.3);
                    spawn(
                        &sprites.steam,
                        feet + Vec3::new(r.next() * 0.25, CHEST, r.next() * 0.25),
                        Particle::new(vel, -0.3, 0.9),
                    );
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

/// One bubble from the chest that rises and pops after a while.
fn bubbles(
    spawn: &mut impl FnMut(&Handle<Image>, Vec3, Particle),
    sprites: &EffectSprites,
    element: usize,
    feet: Vec3,
    r: &mut Scatter,
    delay: f32,
) {
    let at = feet + Vec3::new(r.next() * 0.25, CHEST + r.next() * 0.2, r.next() * 0.25);
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
    fn new(vel: Vec3, gravity: f32, life: f32) -> Particle {
        Particle {
            vel,
            gravity,
            age: 0.0,
            life,
            floor: None,
            then: None,
        }
    }

    fn floor(self, y: f32) -> Particle {
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
