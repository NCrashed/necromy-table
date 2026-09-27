//! Billboard props on the hexes: trees, houses, peaks, stones (docs/design.md
//! §18). The ground is a flat texture (`board.rs`); everything that stands
//! up is a camera-facing sprite, so the table can turn without houses
//! lying on their side.
//!
//! Each terrain has a set of props and the spots they may take, a ring
//! around the hex centre, which stays free for the champion. Which props and
//! which spots come from the hex's coordinates, so every client sees the
//! same board. When a hex's terrain changes (a body grows into a grove),
//! its props are built again.

use std::collections::HashMap;

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy_sprite3d::prelude::*;
use necromy_rules::{Hex, Terrain};

use crate::board::Board;
use crate::play::Match;
use crate::token::Billboard;

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_props).add_systems(
            crate::InGame,
            sync_props.run_if(resource_changed::<Match>.or_else(props_just_loaded)),
        );
    }
}

/// A prop sprite (64×64, from `assets/props/`) and how it stands.
#[derive(Clone, Copy)]
struct PropKind {
    file: &'static str,
    /// Height of the drawn part, in metres on the table.
    height: f32,
    /// Its visible pixels: height and the empty rows below its base.
    px_high: f32,
    px_below: f32,
}

const fn kind(file: &'static str, height: f32, px_high: f32, px_below: f32) -> PropKind {
    PropKind {
        file,
        height,
        px_high,
        px_below,
    }
}

// Visible height and the gap under the base, measured on the sprites.
const OAK: PropKind = kind("oak", 1.2, 60.0, 2.0);
const PINE: PropKind = kind("pine", 1.35, 61.0, 2.0);
const PINE_SMALL: PropKind = kind("pine-small", 0.85, 43.0, 2.0);
const BIRCH: PropKind = kind("birch", 1.2, 62.0, 1.0);
const BUSH: PropKind = kind("bush", 0.45, 31.0, 5.0);
const DEAD_TREE: PropKind = kind("dead-tree", 1.1, 61.0, 1.0);
const COTTAGE_RED: PropKind = kind("cottage-red", 0.95, 60.0, 1.0);
const COTTAGE_THATCH: PropKind = kind("cottage-thatch", 0.95, 61.0, 1.0);
const WELL: PropKind = kind("well", 0.7, 57.0, 4.0);
const PEAK: PropKind = kind("peak", 1.3, 53.0, 2.0);
const BOULDER: PropKind = kind("boulder", 0.4, 29.0, 9.0);
const MENHIR: PropKind = kind("menhir", 0.8, 59.0, 2.0);
const COLUMN: PropKind = kind("column", 0.7, 52.0, 3.0);
const WALL: PropKind = kind("wall", 0.75, 60.0, 3.0);
const REEDS: PropKind = kind("reeds", 0.6, 58.0, 3.0);
const BONES: PropKind = kind("bones", 0.35, 37.0, 7.0);
/// Nothing on this spot: a choice that leaves the ground bare.
const BARE: PropKind = kind("", 0.0, 1.0, 0.0);

const ALL: [PropKind; 16] = [
    OAK,
    PINE,
    PINE_SMALL,
    BIRCH,
    BUSH,
    DEAD_TREE,
    COTTAGE_RED,
    COTTAGE_THATCH,
    WELL,
    PEAK,
    BOULDER,
    MENHIR,
    COLUMN,
    WALL,
    REEDS,
    BONES,
];

/// What stands on a hex of this terrain: every entry is one prop, picked
/// from its choices, on one of the six spots around the centre.
fn layout(terrain: Terrain) -> &'static [&'static [PropKind]] {
    match terrain {
        // Mostly open meadow: a bush or a stone on about a third of it.
        Terrain::Plains => &[&[BUSH, BOULDER, BUSH, BARE, BARE, BARE, BARE, BARE, BARE]],
        Terrain::Forest => &[
            &[OAK, PINE, BIRCH],
            &[PINE, PINE_SMALL],
            &[OAK, PINE],
            &[PINE_SMALL, BUSH, BIRCH],
        ],
        Terrain::Grove => &[&[BIRCH, OAK], &[BIRCH, PINE_SMALL], &[BONES]],
        Terrain::Mountain => &[&[PEAK], &[PEAK], &[BOULDER]],
        Terrain::Swamp => &[&[REEDS], &[REEDS], &[DEAD_TREE, REEDS]],
        Terrain::Settlement => &[&[COTTAGE_RED], &[COTTAGE_THATCH], &[WELL]],
        Terrain::Ruins => &[&[WALL], &[COLUMN], &[BOULDER, COLUMN]],
        Terrain::Stones => &[&[MENHIR], &[MENHIR], &[MENHIR], &[MENHIR]],
        Terrain::Temple | Terrain::Table => &[],
    }
}

/// Spots around a hex centre: a ring between the champion and the edge.
const SPOT_RADIUS: f32 = 0.58;

#[derive(Resource)]
struct PropImages(HashMap<&'static str, Handle<Image>>);

/// Every prop standing on `hex`, and the terrain they were built for.
#[derive(Component)]
struct Prop(Hex);

/// The terrain each hex's props were built for.
#[derive(Resource, Default)]
struct Built(HashMap<Hex, Terrain>);

fn load_props(mut commands: Commands, assets: Res<AssetServer>) {
    let images = ALL
        .iter()
        .filter(|k| !k.file.is_empty())
        .map(|k| (k.file, assets.load(format!("props/{}.png", k.file))))
        .collect();
    commands.insert_resource(PropImages(images));
    commands.init_resource::<Built>();
}

/// `Sprite3d` reads the image size when spawned: props wait for their
/// images, then build once they are all in.
fn props_just_loaded(
    images: Option<Res<PropImages>>,
    assets: Res<AssetServer>,
    mut done: Local<bool>,
) -> bool {
    if *done {
        return false;
    }
    let Some(images) = images else {
        return false;
    };
    *done = images
        .0
        .values()
        .all(|h| assets.is_loaded_with_dependencies(h));
    *done
}

/// A number from the hex's coordinates, the same on every client.
fn hex_seed(hex: Hex, salt: i32) -> u32 {
    let mix = hex.x.wrapping_mul(73_856_093)
        ^ hex.y.wrapping_mul(19_349_663)
        ^ salt.wrapping_mul(83_492_791);
    mix.unsigned_abs()
}

#[allow(clippy::too_many_arguments)]
fn sync_props(
    mut commands: Commands,
    game: Res<Match>,
    board: Res<Board>,
    images: Res<PropImages>,
    assets: Res<AssetServer>,
    mut built: ResMut<Built>,
    props: Query<(Entity, &Prop)>,
) {
    if !images
        .0
        .values()
        .all(|h| assets.is_loaded_with_dependencies(h))
    {
        return;
    }
    let changed: Vec<(Hex, Terrain)> = game
        .game
        .board()
        .tiles()
        .filter(|(hex, tile)| built.0.get(hex) != Some(&tile.terrain))
        .map(|(hex, tile)| (hex, tile.terrain))
        .collect();
    if changed.is_empty() {
        return;
    }
    for (entity, prop) in &props {
        if changed.iter().any(|(h, _)| *h == prop.0) {
            commands.entity(entity).despawn();
        }
    }
    for (hex, terrain) in changed {
        built.0.insert(hex, terrain);
        let centre = board.hex_to_world(hex);
        // Spots are taken in a turned order, so hexes differ.
        let turn = hex_seed(hex, 1) as usize % 6;
        for (i, choices) in layout(terrain).iter().enumerate() {
            let pick = choices[hex_seed(hex, 2 + i as i32) as usize % choices.len()];
            let spot = (turn + i * 5) % 6;
            let angle = std::f32::consts::FRAC_PI_3 * spot as f32 + std::f32::consts::FRAC_PI_6;
            let jitter = (hex_seed(hex, 20 + i as i32) % 100) as f32 / 100.0 * 0.12;
            let at = centre + Vec3::new(angle.cos(), 0.0, angle.sin()) * (SPOT_RADIUS + jitter);
            spawn_prop(&mut commands, &images, pick, hex, at);
        }
    }
}

fn spawn_prop(commands: &mut Commands, images: &PropImages, kind: PropKind, hex: Hex, at: Vec3) {
    let Some(image) = images.0.get(kind.file) else {
        return;
    };
    commands.spawn((
        Prop(hex),
        Billboard,
        NotShadowCaster,
        NotShadowReceiver,
        Sprite::from_image(image.clone()),
        Sprite3d {
            pixels_per_metre: kind.px_high / kind.height,
            // Stand on the base, not on the empty rows under it.
            pivot: Some(Vec2::new(0.5, kind.px_below / 64.0)),
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            ..default()
        },
        Transform::from_translation(at),
    ));
}
