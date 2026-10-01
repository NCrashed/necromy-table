//! Where the human is and where to go: a golden ring pulses under the
//! human's champion, an arrow bobs over its head (always in the tutorial,
//! else for a few seconds as the human's turn begins), and over every hex
//! the tutorial points at (`tutorial::Focus`) an arrow bobs and a ring
//! pulses on the ground.
//!
//! Everything is drawn in code at the table's texel size (`board::TEXELS`),
//! animated by frames and moved by whole texels.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_sprite3d::Sprite3d;
use hexx::Hex;

use crate::board::{Board, TEXELS};
use crate::play::Match;
use crate::token::{Billboard, Token};

pub struct BeaconPlugin;

impl Plugin for BeaconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, make_art).add_systems(
            crate::InGame,
            (spawn_hero_beacons, sync_focus, animate, on_top).chain(),
        );
    }
}

/// Side of a ring's frame, and its frames.
const RING: u32 = 56;
const RING_FRAMES: u32 = 8;
/// How long the arrow stays over the human as their turn begins.
const HERO_ARROW_SECS: f32 = 6.0;
/// Height of an arrow's tip over the hex (world units): over the head of a
/// champion standing there.
const ARROW_HEIGHT: f32 = 1.65;

#[derive(Resource)]
struct BeaconArt {
    ring: Handle<Image>,
    target_ring: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
    arrow: Handle<Image>,
    hero_arrow: Handle<Image>,
}

#[derive(Component)]
enum Beacon {
    /// Under the human's champion.
    HeroRing,
    /// Over the human's head.
    HeroArrow,
    /// On a hex the tutorial points at.
    TargetRing(Hex),
    TargetArrow(Hex),
}

fn make_art(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    commands.insert_resource(BeaconArt {
        ring: images.add(ring_frames([250, 206, 90], [255, 240, 180])),
        target_ring: images.add(ring_frames([255, 150, 60], [255, 220, 150])),
        layout: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(RING),
            RING_FRAMES,
            1,
            None,
            None,
        )),
        arrow: images.add(arrow([255, 170, 60])),
        hero_arrow: images.add(arrow([250, 214, 120])),
    });
}

fn canvas(width: u32, height: u32) -> Image {
    Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// A ring lying on the ground: a steady band with a dark rim, and a wave
/// that leaves it outwards, thinning to nothing, once a loop.
fn ring_frames(band: [u8; 3], wave: [u8; 3]) -> Image {
    let mut image = canvas(RING * RING_FRAMES, RING);
    let width = (RING * RING_FRAMES) as usize;
    let data = image.data.as_mut().expect("canvas allocates pixel data");
    let c = (RING as f32 - 1.0) / 2.0;
    for f in 0..RING_FRAMES {
        let u = f as f32 / RING_FRAMES as f32;
        let wave_r = 21.0 + u * 6.5;
        for y in 0..RING {
            for x in 0..RING {
                let r = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
                let px: [u8; 4] = if (17.0..20.0).contains(&r) {
                    [band[0], band[1], band[2], 255]
                } else if (16.0..17.0).contains(&r) || (20.0..21.0).contains(&r) {
                    [40, 26, 12, 255]
                } else if (r - wave_r).abs() < 0.75 {
                    // Thinner as it goes: every pixel, then every other,
                    // then every fourth.
                    let keep = match f {
                        0..=2 => 1,
                        3..=5 => 2,
                        _ => 4,
                    };
                    if (x + y) % keep != 0 {
                        continue;
                    }
                    [wave[0], wave[1], wave[2], 255]
                } else {
                    continue;
                };
                let i = (y as usize * width + (f * RING + x) as usize) * 4;
                data[i..i + 4].copy_from_slice(&px);
            }
        }
    }
    image
}

/// An arrow pointing down, in a dark outline.
fn arrow(fill: [u8; 3]) -> Image {
    const ROWS: [&str; 22] = [
        "......ooooo......",
        ".....o#####o.....",
        ".....o#FFf#o.....",
        ".....o#Ffd#o.....",
        ".....o#Ffd#o.....",
        ".....o#Ffd#o.....",
        ".....o#Ffd#o.....",
        ".....o#Ffd#o.....",
        ".oooo##Ffd##oooo.",
        "o#####FFfdd#####o",
        "o#FFFFFffdddddd#o",
        ".o#FFFFffddddd#o.",
        "..o#FFFffdddd#o..",
        "...o#FFffddd#o...",
        "....o#Fffdd#o....",
        ".....o#ffd#o.....",
        "......o#d#o......",
        ".......o#o.......",
        "........o........",
        ".................",
        ".................",
        ".................",
    ];
    let (w, h) = (ROWS[0].len() as u32, ROWS.len() as u32);
    let mut image = canvas(w, h);
    let data = image.data.as_mut().expect("canvas allocates pixel data");
    for (y, row) in ROWS.iter().enumerate() {
        for (x, ch) in row.bytes().enumerate() {
            let px = match ch {
                b'#' => [30, 20, 12, 255],
                // A pale halo: reads on dark ground and on grass alike.
                b'o' => [255, 246, 220, 255],
                b'f' => [fill[0], fill[1], fill[2], 255],
                b'F' => {
                    let l = fill.map(|c| (c as u16 + (255 - c as u16) / 2) as u8);
                    [l[0], l[1], l[2], 255]
                }
                b'd' => {
                    let d = fill.map(|c| (c as u16 * 3 / 5) as u8);
                    [d[0], d[1], d[2], 255]
                }
                _ => continue,
            };
            let i = (y * w as usize + x) * 4;
            data[i..i + 4].copy_from_slice(&px);
        }
    }
    image
}

fn ring_bundle(art: &BeaconArt, image: Handle<Image>) -> impl Bundle {
    (
        NotShadowCaster,
        NotShadowReceiver,
        Sprite::from_atlas_image(
            image,
            TextureAtlas {
                layout: art.layout.clone(),
                index: 0,
            },
        ),
        Sprite3d {
            pixels_per_metre: TEXELS,
            pivot: Some(Vec2::splat(0.5)),
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            emissive: MARK,
            ..default()
        },
        // Flat on the ground; a ring reads the same from every side.
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
    )
}

fn arrow_bundle(image: Handle<Image>) -> impl Bundle {
    (
        NotShadowCaster,
        NotShadowReceiver,
        Billboard,
        Sprite::from_image(image),
        Sprite3d {
            pixels_per_metre: TEXELS,
            // The tip at the entity's place.
            pivot: Some(Vec2::new(0.5, 3.0 / 22.0)),
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            emissive: MARK,
            ..default()
        },
        Transform::default(),
        Visibility::Hidden,
    )
}

fn spawn_hero_beacons(mut commands: Commands, art: Res<BeaconArt>, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    commands.spawn((Beacon::HeroRing, ring_bundle(&art, art.ring.clone())));
    commands.spawn((Beacon::HeroArrow, arrow_bundle(art.hero_arrow.clone())));
}

/// A ring and an arrow on every hex the tutorial points at.
fn sync_focus(
    mut commands: Commands,
    focus: Option<Res<crate::tutorial::Focus>>,
    art: Res<BeaconArt>,
    beacons: Query<(Entity, &Beacon)>,
) {
    let hexes: &[Hex] = focus.as_ref().map_or(&[], |f| &f.hexes);
    for (entity, beacon) in &beacons {
        if let Beacon::TargetRing(h) | Beacon::TargetArrow(h) = beacon
            && !hexes.contains(h)
        {
            commands.entity(entity).despawn();
        }
    }
    for &hex in hexes {
        let shown = beacons
            .iter()
            .any(|(_, b)| matches!(b, Beacon::TargetArrow(h) if *h == hex));
        if !shown {
            commands.spawn((
                Beacon::TargetRing(hex),
                ring_bundle(&art, art.target_ring.clone()),
            ));
            commands.spawn((Beacon::TargetArrow(hex), arrow_bundle(art.arrow.clone())));
        }
    }
}

/// Rings pulse, arrows bob by whole texels; the hero's beacons follow the
/// token as it walks.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn animate(
    time: Res<Time>,
    game: Res<Match>,
    board: Res<Board>,
    lesson: Option<Res<crate::tutorial::Lesson>>,
    tokens: Query<(&Token, &Transform), Without<Beacon>>,
    mut beacons: Query<(&Beacon, &mut Transform, &mut Sprite, &mut Visibility)>,
    mut turn_began: Local<Option<f32>>,
    mut was_turn: Local<bool>,
) {
    let now = time.elapsed_secs();
    // The arrow over the human as their turn begins.
    let turn = game.is_human_turn();
    if turn && !*was_turn {
        *turn_began = Some(now);
    }
    *was_turn = turn;
    let hero = tokens
        .iter()
        .find(|(t, _)| t.player == game.human)
        .map(|(_, at)| at.translation);
    let hero_arrow = lesson.is_some() || turn_began.is_some_and(|t| now - t < HERO_ARROW_SECS);
    let frame = (now * 10.0) as usize % RING_FRAMES as usize;
    let bob = |phase: f32| ((now * 4.5 + phase).sin() * 3.0).round() / TEXELS;
    for (beacon, mut transform, mut sprite, mut visibility) in &mut beacons {
        let (at, show) = match *beacon {
            Beacon::HeroRing => (hero.map(|h| h + Vec3::Y * 0.035), true),
            Beacon::HeroArrow => (
                hero.map(|h| h + Vec3::Y * (ARROW_HEIGHT + bob(0.0))),
                hero_arrow,
            ),
            Beacon::TargetRing(hex) => (Some(board.hex_to_world(hex) + Vec3::Y * 0.04), true),
            Beacon::TargetArrow(hex) => (
                Some(board.hex_to_world(hex) + Vec3::Y * (ARROW_HEIGHT + 0.25 + bob(1.5))),
                true,
            ),
        };
        let Some(at) = at.filter(|_| show) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        if transform.translation != at {
            transform.translation = at;
        }
        if let Some(atlas) = sprite.texture_atlas.as_mut()
            && atlas.index != frame
        {
            atlas.index = frame;
        }
    }
}

/// Beacons' materials are told apart by this emissive (`bevy_sprite3d`
/// caches one material per image and look, and puts it back whenever the
/// sprite changes): too faint to see, it gives them materials of their own.
const MARK: LinearRgba = LinearRgba::rgb(0.0, 0.0, 1.0 / 255.0);
/// Pulls beacons in front of the trees and figures between them and the
/// camera: they must never be lost behind the scenery.
const ON_TOP: f32 = 1.0e7;

fn on_top(
    beacons: Query<(&Beacon, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // The ring under the champion stays under its feet.
    for (_, material) in beacons
        .iter()
        .filter(|(b, _)| !matches!(b, Beacon::HeroRing))
    {
        if materials
            .get(&material.0)
            .is_some_and(|m| m.depth_bias != ON_TOP)
            && let Some(mut m) = materials.get_mut(&material.0)
        {
            m.depth_bias = ON_TOP;
        }
    }
}
