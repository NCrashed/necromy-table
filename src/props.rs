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
use necromy_rules::{God, Hex, Terrain};

use crate::board::Board;
use crate::lighting::{Glow, lamp};
use crate::play::Match;
use crate::token::Billboard;

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_props)
            .add_systems(
                crate::InGame,
                sync_props.run_if(resource_changed::<Match>.or_else(props_just_loaded)),
            )
            .add_systems(
                crate::InGame,
                (own_materials, fade_occluders).after(crate::camera::apply),
            );
    }
}

/// A prop sprite (64×64, from `assets/props/`) and how it stands.
#[derive(Clone, Copy)]
struct PropKind {
    file: &'static str,
    /// Height it was once drawn at, in metres, with its visible pixel
    /// height: lights were placed for that size and are scaled from it.
    /// The prop itself is drawn at `board::TEXELS`.
    height: f32,
    px_high: f32,
    /// Empty rows below its base.
    px_below: f32,
    /// How far from the hex centre it stands.
    radius: f32,
    /// A chimney mouth, in the sprite's pixels, if smoke comes out.
    chimney: Option<(f32, f32)>,
    /// A light it gives, by night or always (`lighting.rs`).
    light: Option<PropLight>,
}

#[derive(Clone, Copy)]
struct PropLight {
    color: Color,
    glow: Glow,
    /// Height above the base, in metres.
    height: f32,
    range: f32,
}

const fn with_light(kind: PropKind, color: Color, glow: Glow, height: f32, range: f32) -> PropKind {
    PropKind {
        light: Some(PropLight {
            color,
            glow,
            height,
            range,
        }),
        ..kind
    }
}

const FIRE: Color = Color::srgb(1.0, 0.55, 0.2);
const WINDOW: Color = Color::srgb(1.0, 0.75, 0.42);
const TURQUOISE: Color = Color::srgb(0.25, 0.95, 0.88);
const GROWTH: Color = Color::srgb(0.55, 1.0, 0.4);
const WHITE_GOLD: Color = Color::srgb(1.0, 0.9, 0.65);
const LANTERN_VIOLET: Color = Color::srgb(0.72, 0.5, 1.0);
/// A fire burns by day too, but only night shows it.
const FIRE_GLOW: Glow = Glow {
    day: 400.0,
    night: 5_500.0,
};

const fn kind(file: &'static str, height: f32, px_high: f32, px_below: f32) -> PropKind {
    PropKind {
        file,
        height,
        px_high,
        px_below,
        radius: SPOT_RADIUS,
        chimney: None,
        light: None,
    }
}

const fn with_chimney(kind: PropKind, x: f32, y: f32) -> PropKind {
    PropKind {
        chimney: Some((x, y)),
        ..kind
    }
}

/// A building that is the hex (a temple, the Table): nearer the centre, so
/// it stays on its own hex; the champion still stands at the centre.
const fn landmark(file: &'static str, height: f32, px_high: f32, px_below: f32) -> PropKind {
    PropKind {
        radius: LANDMARK_RADIUS,
        ..kind(file, height, px_high, px_below)
    }
}

// Visible height and the gap under the base, measured on the sprites.
const OAK: PropKind = kind("oak", 1.2, 60.0, 2.0);
const PINE: PropKind = kind("pine", 1.35, 61.0, 2.0);
const PINE_SMALL: PropKind = kind("pine-small", 0.85, 43.0, 2.0);
const BIRCH: PropKind = kind("birch", 1.2, 62.0, 1.0);
const BUSH: PropKind = kind("bush", 0.45, 31.0, 5.0);
const DEAD_TREE: PropKind = kind("dead-tree", 1.1, 61.0, 1.0);
const COTTAGE_RED: PropKind = with_light(
    kind("cottage-red", 0.95, 60.0, 1.0),
    WINDOW,
    Glow::night(4_000.0),
    0.3,
    1.8,
);
const COTTAGE_THATCH: PropKind = with_light(
    with_chimney(kind("cottage-thatch", 0.95, 61.0, 1.0), 42.0, 4.0),
    WINDOW,
    Glow::night(4_000.0),
    0.3,
    1.8,
);
const WELL: PropKind = kind("well", 0.7, 57.0, 4.0);
const PEAK: PropKind = kind("peak", 1.0, 53.0, 2.0);
const BOULDER: PropKind = kind("boulder", 0.4, 29.0, 9.0);
const MENHIR: PropKind = kind("menhir", 0.8, 59.0, 2.0);
const COLUMN: PropKind = kind("column", 0.7, 52.0, 3.0);
const WALL: PropKind = kind("wall", 0.75, 60.0, 3.0);
const REEDS: PropKind = kind("reeds", 0.6, 58.0, 3.0);
const BONES: PropKind = kind("bones", 0.35, 37.0, 7.0);
const BANNER: PropKind = kind("banner-ahamar", 1.1, 61.0, 1.0);
const BRAZIER: PropKind = with_light(kind("brazier", 0.55, 50.0, 6.0), FIRE, FIRE_GLOW, 0.5, 2.6);
const GRAVESTONE: PropKind = kind("gravestone", 0.45, 41.0, 8.0);
// Each god's temple, two looks (§5): Bhava has no face, only his growth.
const TEMPLE_BHAVA_OAK: PropKind = with_light(
    landmark("temple-bhava-oak", 1.6, 63.0, 1.0),
    GROWTH,
    Glow::night(3_500.0),
    0.4,
    2.2,
);
const TEMPLE_BHAVA_RING: PropKind = with_light(
    landmark("temple-bhava-ring", 1.3, 55.0, 1.0),
    GROWTH,
    Glow::night(3_500.0),
    0.5,
    2.2,
);
const TEMPLE_TRISHNA_ALTAR: PropKind = with_light(
    landmark("temple-trishna-altar", 1.2, 57.0, 2.0),
    FIRE,
    FIRE_GLOW,
    0.4,
    2.6,
);
const TEMPLE_TRISHNA_TENT: PropKind = with_light(
    landmark("temple-trishna-tent", 1.4, 62.0, 1.0),
    FIRE,
    FIRE_GLOW,
    0.35,
    2.6,
);
const TEMPLE_ZAGA_CHAPEL: PropKind = landmark("temple-zaga-chapel", 1.5, 63.0, 0.0);
const TEMPLE_ZAGA_CELL: PropKind = with_light(
    landmark("temple-zaga-cell", 1.3, 55.0, 5.0),
    LANTERN_VIOLET,
    Glow::night(2_500.0),
    0.6,
    2.0,
);
const TEMPLE_AHAMAR_PORTICO: PropKind = with_light(
    landmark("temple-ahamar-portico", 1.4, 61.0, 2.0),
    WHITE_GOLD,
    Glow::night(3_000.0),
    0.5,
    2.2,
);
const TEMPLE_AHAMAR_DOME: PropKind = with_light(
    landmark("temple-ahamar-dome", 1.5, 62.0, 1.0),
    WHITE_GOLD,
    Glow::night(3_000.0),
    0.5,
    2.2,
);
const TEMPLE_MAYA_RUIN: PropKind = with_light(
    landmark("temple-maya-ruin", 1.4, 60.0, 2.0),
    TURQUOISE,
    Glow::night(4_500.0),
    0.5,
    2.4,
);
const TEMPLE_MAYA_ARCH: PropKind = with_light(
    landmark("temple-maya-arch", 1.4, 58.0, 3.0),
    TURQUOISE,
    Glow::night(4_500.0),
    0.6,
    2.4,
);
/// Ahamar's Table, the centre of the board.
const TABLE: PropKind = with_light(
    landmark("table-dais", 1.15, 47.0, 6.0),
    WHITE_GOLD,
    Glow::night(3_500.0),
    0.6,
    2.4,
);
/// Nothing on this spot: a choice that leaves the ground bare.
const BARE: PropKind = kind("", 0.0, 1.0, 0.0);

const ALL: [PropKind; 30] = [
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
    BANNER,
    BRAZIER,
    GRAVESTONE,
    TEMPLE_BHAVA_OAK,
    TEMPLE_BHAVA_RING,
    TEMPLE_TRISHNA_ALTAR,
    TEMPLE_TRISHNA_TENT,
    TEMPLE_ZAGA_CHAPEL,
    TEMPLE_ZAGA_CELL,
    TEMPLE_AHAMAR_PORTICO,
    TEMPLE_AHAMAR_DOME,
    TEMPLE_MAYA_RUIN,
    TEMPLE_MAYA_ARCH,
    TABLE,
];

/// What stands on a hex of this terrain: every entry is one prop, picked
/// from its choices, on one of the six spots around the centre. A temple is
/// its region's god's.
fn layout(terrain: Terrain, region: Option<God>) -> &'static [&'static [PropKind]] {
    match terrain {
        Terrain::Mist | Terrain::River | Terrain::Lake | Terrain::Ash => &[],
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
        Terrain::Temple => match region {
            Some(God::Bhava) => &[&[TEMPLE_BHAVA_OAK, TEMPLE_BHAVA_RING]],
            Some(God::Trishna) => &[&[TEMPLE_TRISHNA_ALTAR, TEMPLE_TRISHNA_TENT], &[BRAZIER]],
            Some(God::Zaga) => &[&[TEMPLE_ZAGA_CHAPEL, TEMPLE_ZAGA_CELL]],
            Some(God::Ahamar) => &[&[TEMPLE_AHAMAR_PORTICO, TEMPLE_AHAMAR_DOME], &[BANNER]],
            Some(God::Maya) => &[&[TEMPLE_MAYA_RUIN, TEMPLE_MAYA_ARCH], &[GRAVESTONE]],
            None => &[],
        },
        // The Table: the dais, Ahamar's banners either side of it.
        Terrain::Table => &[&[TABLE], &[BANNER], &[BANNER]],
    }
}

/// Spots around a hex centre: a ring between the champion and the edge.
const SPOT_RADIUS: f32 = 0.55;
/// Temples and the Table: nearer the centre than the ring.
const LANDMARK_RADIUS: f32 = 0.42;
/// How far from the centre a prop's picture may reach: past the middle
/// of a side (0.87), short of a corner (1.0).
const EDGE_REACH: f32 = 0.95;
/// However wide a prop, it keeps this far from the centre.
const MIN_RADIUS: f32 = 0.22;
/// Spread of the ring, so hexes do not look stamped.
const JITTER: f32 = 0.06;

/// Half the drawn width of a prop picture, in pixels from its middle column
/// (the pivot) to the farthest opaque one.
fn half_width_px(image: &Image) -> f32 {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let Some(data) = image.data.as_ref() else {
        return w as f32 / 2.0;
    };
    let mid = w as f32 / 2.0;
    let mut reach: f32 = 0.0;
    for y in 0..h {
        for x in 0..w {
            if data.get((y * w + x) * 4 + 3).is_some_and(|&a| a > 0) {
                reach = reach.max((x as f32 + 0.5 - mid).abs());
            }
        }
    }
    reach
}

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
pub fn hex_seed(hex: Hex, salt: i32) -> u32 {
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
    pictures: Res<Assets<Image>>,
    mut built: ResMut<Built>,
    props: Query<(Entity, &Prop)>,
    mut reach: Local<HashMap<&'static str, f32>>,
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
        let region = game.game.board().tile(hex).and_then(|t| t.region);
        let entries = layout(terrain, region);
        // Props share the ring evenly: two face each other, three stand a
        // third of a turn apart, and so on.
        let count = entries.len().clamp(1, 6);
        for (i, choices) in entries.iter().enumerate() {
            let pick = choices[hex_seed(hex, 2 + i as i32) as usize % choices.len()];
            let spot = (turn + ((i % 6) * 6 + count / 2) / count) % 6;
            let angle = std::f32::consts::FRAC_PI_3 * spot as f32 + std::f32::consts::FRAC_PI_6;
            // Far enough out to leave the centre to the champion, near enough
            // that the picture stays on its own hex: a wide one comes in.
            let half = *reach.entry(pick.file).or_insert_with(|| {
                let px = images
                    .0
                    .get(pick.file)
                    .and_then(|h| pictures.get(h))
                    .map_or(32.0, half_width_px);
                px / crate::board::TEXELS
            });
            let room = (EDGE_REACH - half).max(MIN_RADIUS);
            let jitter = (hex_seed(hex, 20 + i as i32) % 100) as f32 / 100.0 * JITTER;
            let radius = (pick.radius + jitter).min(room);
            let at = centre + Vec3::new(angle.cos(), 0.0, angle.sin()) * radius;
            spawn_prop(&mut commands, &images, pick, hex, at);
        }
    }
}

fn spawn_prop(commands: &mut Commands, images: &PropImages, kind: PropKind, hex: Hex, at: Vec3) {
    let Some(image) = images.0.get(kind.file) else {
        return;
    };
    // Every prop at the table's one texel density: its size is its pixels.
    let pixels_per_metre = crate::board::TEXELS;
    // Lights were placed for the height the prop used to be drawn at.
    let shrink = kind.px_high / pixels_per_metre / kind.height;
    let mut prop = commands.spawn((
        Prop(hex),
        Billboard,
        NotShadowCaster,
        NotShadowReceiver,
        Sprite::from_image(image.clone()),
        Sprite3d {
            pixels_per_metre,
            // Stand on the base, not on the empty rows under it.
            pivot: Some(Vec2::new(0.5, kind.px_below / 64.0)),
            alpha_mode: AlphaMode::Mask(0.5),
            // Lit: night falls on props too (`lighting.rs`).
            unlit: false,
            ..default()
        },
        Transform::from_translation(at),
    ));
    // Smoke comes out of the chimney mouth, which turns with the billboard.
    if let Some((x, y)) = kind.chimney {
        let local = Vec3::new(
            (x - 32.0) / pixels_per_metre,
            (64.0 - y - kind.px_below) / pixels_per_metre,
            0.01,
        );
        let seed = (hex.x * 31 + hex.y * 17).unsigned_abs();
        prop.with_child((
            crate::ambient::Chimney::new(seed),
            Transform::from_translation(local),
        ));
    }
    // A light a little in front of the sprite, so it lights its own face.
    if let Some(light) = kind.light {
        prop.with_child((
            lamp(light.color, light.glow, light.range),
            Transform::from_xyz(0.0, light.height * shrink, 0.25),
        ));
    }
}

/// Props still sharing the library's material.
type SharedProps<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut MeshMaterial3d<StandardMaterial>),
    (With<Prop>, Without<Fade>),
>;

/// A prop with its own material, so it alone can fade.
#[derive(Component)]
struct Fade {
    /// How opaque the prop is now; eases towards its target.
    alpha: f32,
}

/// `bevy_sprite3d` shares one material per image; a prop that may fade
/// takes its own copy once built. Props never change their `Sprite`, so the
/// library does not swap it back.
fn own_materials(
    mut commands: Commands,
    mut props: SharedProps,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut material) in &mut props {
        let Some(mut copy) = materials.get(&material.0).cloned() else {
            continue;
        };
        // Matte: painted props have no glints.
        copy.perceptual_roughness = 1.0;
        copy.reflectance = 0.0;
        material.0 = materials.add(copy);
        commands.entity(entity).insert(Fade { alpha: 1.0 });
    }
}

/// How close in front of a champion a prop must stand to be seen through.
const FADE_REACH: f32 = 0.95;
const FADE_ALPHA: f32 = 0.35;
/// Opacity per second a prop gains or loses: a walker passing behind a
/// row of trees makes them ease, not blink.
const FADE_SPEED: f32 = 3.0;

/// A prop between the camera and a champion goes see-through: trees must
/// not swallow the pieces.
fn fade_occluders(
    time: Res<Time>,
    camera: Single<&Transform, With<crate::TableCamera>>,
    tokens: Query<(&Transform, &Visibility), With<crate::token::Token>>,
    mut props: Query<(&Transform, &MeshMaterial3d<StandardMaterial>, &mut Fade)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let towards_camera = camera.back().with_y(0.0).normalize_or_zero();
    let across = Vec3::Y.cross(towards_camera);
    let pieces: Vec<Vec3> = tokens
        .iter()
        .filter(|(_, v)| **v != Visibility::Hidden)
        .map(|(t, _)| t.translation)
        .collect();
    for (transform, material, mut fade) in &mut props {
        let covers = pieces.iter().any(|piece| {
            let d = (transform.translation - *piece).with_y(0.0);
            let ahead = d.dot(towards_camera);
            ahead > 0.0 && ahead < FADE_REACH && d.dot(across).abs() < 0.6
        });
        let target = if covers { FADE_ALPHA } else { 1.0 };
        if fade.alpha == target {
            continue;
        }
        let step = FADE_SPEED * time.delta_secs();
        fade.alpha = if fade.alpha < target {
            (fade.alpha + step).min(target)
        } else {
            (fade.alpha - step).max(target)
        };
        if let Some(mut m) = materials.get_mut(&material.0) {
            // Blend only while see-through: masked props sort and depth-test
            // like the rest of the board.
            if fade.alpha < 1.0 {
                m.alpha_mode = AlphaMode::Blend;
                m.base_color = Color::srgba(1.0, 1.0, 1.0, fade.alpha);
            } else {
                m.alpha_mode = AlphaMode::Mask(0.5);
                m.base_color = Color::WHITE;
            }
        }
    }
}
