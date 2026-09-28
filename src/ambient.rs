//! Life on the board: fireflies over the standing stones by night, each a
//! little light, and smoke from the chimneys. Pure decoration, drawn procedurally as tiny pixel billboards:
//! nothing here reads or changes the rules.
//!
//! `bevy_sprite3d` shares one material per image and ignores
//! `Sprite::color`, so blinking and fading are done by scale.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_sprite3d::prelude::*;
use necromy_rules::Terrain;

use crate::board::Board;
use crate::lighting::{DayNight, Glow, lamp};
use crate::play::Match;
use crate::token::Billboard;

/// Fireflies over each hex of standing stones.
const FIREFLIES_PER_HEX: u32 = 6;
const FIREFLY_LIGHT: Color = Color::srgb(1.0, 0.85, 0.35);
/// Seconds between two puffs from a chimney.
const PUFF_EVERY: f32 = 0.55;
/// How long a puff lives, and how fast it rises (m/s).
const PUFF_LIFE: f32 = 2.6;
const PUFF_RISE: f32 = 0.32;
/// The wind carries the smoke a little, the same way everywhere.
const WIND: Vec3 = Vec3::new(0.12, 0.0, -0.05);

pub struct AmbientPlugin;

impl Plugin for AmbientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, make_sprites).add_systems(
            crate::InGame,
            (
                spawn_fireflies.run_if(resource_changed::<Match>),
                fly,
                smoke,
                drift,
            ),
        );
    }
}

#[derive(Resource)]
struct AmbientSprites {
    firefly: Handle<Image>,
    puff: Handle<Image>,
}

/// A point on a prop where smoke comes out: a child of the prop, so it
/// turns with its billboard.
#[derive(Component)]
pub struct Chimney {
    next: f32,
}

impl Chimney {
    pub fn new(seed: u32) -> Chimney {
        Chimney {
            next: (seed % 100) as f32 / 100.0 * PUFF_EVERY,
        }
    }
}

#[derive(Component)]
struct Puff {
    age: f32,
}

#[derive(Component)]
struct Firefly {
    centre: Vec3,
    /// Phases and speeds, different for every firefly.
    phase: [f32; 4],
}

/// The hexes fireflies already hover over.
#[derive(Component)]
struct Swarm(necromy_rules::Hex);

/// Draws a sprite from rows of palette letters; `.` is transparent.
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

fn make_sprites(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // A warm yellow spark, white at the heart, that reads on grass by day.
    let firefly = pixels(
        &["..y..", ".yoy.", "yowoy", ".yoy.", "..y.."],
        &[
            (b'y', [235, 190, 40, 255]),
            (b'o', [255, 236, 110, 255]),
            (b'w', [255, 255, 235, 255]),
        ],
    );
    // A small grey puff with a darker rim, pixel-art smoke.
    let puff = pixels(
        &[
            "..ddd..", ".dlllld", "dlllwld", "dllllld", "dllllld", ".dllld.", "..ddd..",
        ],
        &[
            (b'd', [120, 116, 124, 255]),
            (b'l', [196, 192, 198, 255]),
            (b'w', [236, 234, 238, 255]),
        ],
    );
    commands.insert_resource(AmbientSprites {
        firefly: images.add(firefly),
        puff: images.add(puff),
    });
}

fn seed(hex: necromy_rules::Hex, n: u32) -> f32 {
    let mix = hex.x.wrapping_mul(73_856_093)
        ^ hex.y.wrapping_mul(19_349_663)
        ^ (n as i32).wrapping_mul(83_492_791);
    (mix.unsigned_abs() % 10_000) as f32 / 10_000.0
}

/// A pixel billboard; `glows` keeps it bright in the dark (unlit), else
/// night falls on it like on the rest of the board.
fn billboard(image: Handle<Image>, pixels_per_metre: f32, glows: bool) -> impl Bundle {
    (
        Billboard,
        NotShadowCaster,
        NotShadowReceiver,
        Sprite::from_image(image),
        Sprite3d {
            pixels_per_metre,
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: glows,
            ..default()
        },
    )
}

/// Fireflies hover over every hex of standing stones; a hex that turns into
/// something else loses them, a new one gains them.
fn spawn_fireflies(
    mut commands: Commands,
    game: Res<Match>,
    board: Res<Board>,
    sprites: Res<AmbientSprites>,
    swarms: Query<(Entity, &Swarm)>,
) {
    let stones: Vec<necromy_rules::Hex> = game
        .game
        .board()
        .tiles()
        .filter(|(_, t)| t.terrain == Terrain::Stones)
        .map(|(h, _)| h)
        .collect();
    for (entity, swarm) in &swarms {
        if !stones.contains(&swarm.0) {
            commands.entity(entity).despawn();
        }
    }
    for hex in stones {
        if swarms.iter().any(|(_, s)| s.0 == hex) {
            continue;
        }
        let centre = board.hex_to_world(hex);
        commands
            .spawn((Swarm(hex), Transform::default(), Visibility::default()))
            .with_children(|swarm| {
                for i in 0..FIREFLIES_PER_HEX {
                    let phase = std::array::from_fn(|k| {
                        seed(hex, i * 4 + k as u32) * std::f32::consts::TAU
                    });
                    swarm
                        .spawn((
                            Firefly { centre, phase },
                            billboard(sprites.firefly.clone(), crate::board::TEXELS, true),
                            Transform::from_translation(centre),
                        ))
                        // Each firefly lights the stones and grass around it.
                        .with_child(lamp(FIREFLY_LIGHT, Glow::night(700.0), 1.1));
                }
            });
    }
}

/// Lazy loops over the stones, each firefly blinking on its own beat.
/// Fireflies come out at dusk and leave at dawn.
fn fly(time: Res<Time>, day_night: Res<DayNight>, mut flies: Query<(&Firefly, &mut Transform)>) {
    let t = time.elapsed_secs();
    let out = day_night.night;
    for (fly, mut transform) in &mut flies {
        let [a, b, c, d] = fly.phase;
        let r = 0.45 + 0.2 * (t * 0.3 + a).sin();
        let offset = Vec3::new(
            (t * (0.35 + a * 0.05) + a).sin() * r,
            0.35 + 0.3 * (t * (0.6 + b * 0.05) + b).sin().abs(),
            (t * (0.3 + c * 0.05) + c).cos() * r,
        );
        transform.translation = fly.centre + offset;
        // Glow and fade: out now and then, for a moment.
        let glow = ((t * (1.2 + d * 0.2) + d).sin() * 0.5 + 0.5).powf(0.6);
        let s = if glow < 0.15 { 0.0 } else { 0.6 + 0.4 * glow } * out;
        transform.scale = Vec3::splat(s);
    }
}

/// Chimneys breathe out a puff now and then.
fn smoke(
    mut commands: Commands,
    time: Res<Time>,
    sprites: Res<AmbientSprites>,
    mut chimneys: Query<(&mut Chimney, &GlobalTransform)>,
) {
    for (mut chimney, at) in &mut chimneys {
        chimney.next -= time.delta_secs();
        if chimney.next > 0.0 {
            continue;
        }
        chimney.next += PUFF_EVERY;
        commands.spawn((
            Puff { age: 0.0 },
            billboard(sprites.puff.clone(), crate::board::TEXELS, false),
            Transform::from_translation(at.translation()).with_scale(Vec3::splat(0.4)),
        ));
    }
}

/// Puffs rise with the wind, swell, then thin out to nothing.
fn drift(
    mut commands: Commands,
    time: Res<Time>,
    mut puffs: Query<(Entity, &mut Puff, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut puff, mut transform) in &mut puffs {
        puff.age += dt;
        if puff.age >= PUFF_LIFE {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += (Vec3::Y * PUFF_RISE + WIND * puff.age) * dt;
        let k = puff.age / PUFF_LIFE;
        let size = if k < 0.35 {
            0.4 + k / 0.35 * 0.8
        } else {
            1.2 * (1.0 - (k - 0.35) / 0.65)
        };
        transform.scale = Vec3::splat(size.max(0.0));
    }
}
