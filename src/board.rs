//! Draws the rules board: one tile mesh per hex, coloured by terrain and
//! region, plus corpse markers. Everything is re-read from `Match` whenever
//! the match changes; the client never owns board state.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_sprite3d::prelude::*;
use hexx::{Hex, HexLayout, PlaneMeshBuilder};
use necromy_rules::{God, Target, Terrain, Tile as RulesTile};

use std::collections::HashMap;

use bevy::window::PrimaryWindow;

use crate::icons;
use crate::play::{Match, Selection};
use crate::token::Billboard;

pub const HEX_SIZE: f32 = 1.0;

pub struct BoardPlugin;

impl Plugin for BoardPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Board {
            layout: HexLayout::flat().with_hex_size(HEX_SIZE),
        })
        .init_resource::<Hovered>()
        .add_systems(Startup, spawn_board)
        .add_systems(Update, track_hover)
        .add_systems(
            Update,
            (sync_tiles, sync_markers).after(track_hover).run_if(
                resource_changed::<Match>
                    .or_else(resource_changed::<Selection>)
                    .or_else(resource_changed::<Hovered>),
            ),
        );
    }
}

#[derive(Resource)]
pub struct Board {
    pub layout: HexLayout,
}

impl Board {
    /// Board plane is XZ with Y up; hexx works in 2D, so its Y becomes our Z.
    pub fn hex_to_world(&self, hex: Hex) -> Vec3 {
        let p = self.layout.hex_to_world_pos(hex);
        Vec3::new(p.x, 0.0, p.y)
    }

    pub fn world_to_hex(&self, pos: Vec3) -> Hex {
        self.layout.world_pos_to_hex(Vec2::new(pos.x, pos.z))
    }
}

#[derive(Component)]
pub struct Tile(pub Hex);

/// The terrain icon painted on a tile.
#[derive(Component)]
struct TileIcon(Hex);

#[derive(Resource)]
struct IconMaterials(HashMap<Terrain, Handle<StandardMaterial>>);

/// The hex under the mouse, if it is on the board.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct Hovered(pub Option<Hex>);

#[derive(Component)]
struct Marker;

#[derive(Resource)]
struct MarkerSprites {
    corpse: Handle<Image>,
    trap: Handle<Image>,
    /// Ownership flags, one per god (`God::index`).
    flags: [Handle<Image>; 5],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lit {
    No,
    /// The human can walk here.
    Reach,
    /// A legal target for the card being aimed.
    Target,
    /// A rival the human can attack.
    Attack,
    /// Under the mouse.
    Hover,
}

fn spawn_board(
    mut commands: Commands,
    board: Res<Board>,
    game: Res<Match>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // One shared mesh for every tile, slightly inset so the grid reads.
    let mesh = meshes.add(hex_mesh(&board.layout));
    let icon_quad = meshes.add(Plane3d::default().mesh().size(1.1, 1.1));
    let icons: HashMap<Terrain, Handle<StandardMaterial>> = TERRAINS
        .into_iter()
        .filter_map(|t| {
            let image = icons::terrain_icon(t)?;
            Some((
                t,
                materials.add(StandardMaterial {
                    base_color_texture: Some(images.add(image)),
                    alpha_mode: AlphaMode::Mask(0.5),
                    unlit: true,
                    ..default()
                }),
            ))
        })
        .collect();
    for (hex, tile) in game.game.board().tiles() {
        // One material per tile, so highlighting can recolour it alone.
        let material = materials.add(StandardMaterial {
            base_color: tile_color(tile, Lit::No, tile.region.map(|g| game.game.stage(g))),
            // Flat colour, no shading: the board reads like a painted table.
            unlit: true,
            ..default()
        });
        commands.spawn((
            Tile(hex),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::from_translation(board.hex_to_world(hex)),
        ));
        // Every tile gets an icon entity; plain ground just hides it.
        let icon = icons.get(&tile.terrain).or(icons.values().next()).cloned();
        if let Some(icon) = icon {
            commands.spawn((
                TileIcon(hex),
                Mesh3d(icon_quad.clone()),
                MeshMaterial3d(icon),
                Transform::from_translation(board.hex_to_world(hex) + Vec3::Y * 0.01),
                if icons.contains_key(&tile.terrain) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
        }
    }
    commands.insert_resource(IconMaterials(icons));
    commands.insert_resource(MarkerSprites {
        corpse: images.add(pixel_sprite(&CORPSE_ROWS, [0; 3])),
        trap: images.add(pixel_sprite(&TRAP_ROWS, [0; 3])),
        flags: God::ALL.map(|g| images.add(pixel_sprite(&FLAG_ROWS, g.accent()))),
    });
}

fn sync_tiles(
    game: Res<Match>,
    selection: Res<Selection>,
    hovered: Res<Hovered>,
    icon_materials: Res<IconMaterials>,
    tiles: Query<(&Tile, &MeshMaterial3d<StandardMaterial>)>,
    mut icons: Query<
        (
            &TileIcon,
            &mut MeshMaterial3d<StandardMaterial>,
            &mut Visibility,
        ),
        Without<Tile>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Terrain can change (groves), so icons follow it.
    for (icon, mut material, mut visibility) in &mut icons {
        let terrain = game.game.board().tile(icon.0).map(|t| t.terrain);
        match terrain.and_then(|t| icon_materials.0.get(&t)) {
            Some(m) => {
                if material.0 != *m {
                    material.0 = m.clone();
                }
                visibility.set_if_neq(Visibility::Inherited);
            }
            None => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
    let walking = game.is_human_turn() && selection.card.is_none();
    let reachable = if walking {
        game.game.reachable()
    } else {
        Default::default()
    };
    let attackable = if walking {
        game.game.attackable()
    } else {
        Vec::new()
    };
    // Hexes that hold a legal target of the aimed card.
    let targets: Vec<Hex> = selection
        .card
        .map(|card| game.game.targets(game.human, card))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| match t {
            Target::Champion(p) => game.game.champion(p).map(|c| c.hex),
            Target::Hex(h) => Some(h),
            Target::None => None,
        })
        .collect();
    for (tile, material) in &tiles {
        let Some(rules_tile) = game.game.board().tile(tile.0) else {
            continue;
        };
        let lit = if targets.contains(&tile.0) {
            Lit::Target
        } else if attackable.contains(&tile.0) {
            Lit::Attack
        } else if reachable.contains_key(&tile.0) {
            Lit::Reach
        } else if hovered.0 == Some(tile.0) {
            Lit::Hover
        } else {
            Lit::No
        };
        if let Some(mut m) = materials.get_mut(&material.0) {
            let stage = rules_tile.region.map(|g| game.game.stage(g));
            m.base_color = tile_color(rules_tile, lit, stage);
        }
    }
}

/// Corpses for everyone; traps only for their owner, the human.
fn sync_markers(
    mut commands: Commands,
    game: Res<Match>,
    board: Res<Board>,
    sprites: Res<MarkerSprites>,
    markers: Query<Entity, With<Marker>>,
) {
    for entity in &markers {
        commands.entity(entity).despawn();
    }
    let corpses = game
        .game
        .board()
        .corpses()
        .map(|(hex, _)| (hex, sprites.corpse.clone()));
    let traps = game
        .game
        .traps()
        .iter()
        .filter(|t| t.owner == game.human)
        .map(|t| (t.hex, sprites.trap.clone()));
    // Owner flags stand at the back left of the hex, out of the champion's way.
    let flags = game.game.claims().filter_map(|(hex, p)| {
        let god = game.game.champion(p)?.god;
        Some((
            hex,
            sprites.flags[god.index()].clone(),
            Vec3::new(-0.45, 0.0, -0.2),
        ))
    });
    let front = Vec3::new(0.0, 0.0, 0.35);
    let corpses = corpses.map(|(h, i)| (h, i, front));
    let traps = traps.map(|(h, i)| (h, i, front));
    for (hex, image, offset) in corpses.chain(traps).chain(flags) {
        let pos = board.hex_to_world(hex) + offset;
        commands.spawn((
            Marker,
            Billboard,
            NotShadowCaster,
            NotShadowReceiver,
            Sprite::from_image(image),
            Sprite3d {
                pixels_per_metre: 16.0,
                pivot: Some(Vec2::new(0.5, 0.0)),
                alpha_mode: AlphaMode::Mask(0.5),
                unlit: true,
                ..default()
            },
            Transform::from_translation(pos),
        ));
    }
}

/// Placeholder palette: saturated, Warcraft III-like, until tiles land in
/// assets/tiles/. Region accent tints the ground so wedges read at a glance.
/// `stage` of the region's god recolours the ground: light a touch brighter,
/// dark sunk towards ash (§5).
fn tile_color(tile: &RulesTile, lit: Lit, stage: Option<u8>) -> Color {
    let base = match tile.terrain {
        Terrain::Plains => [0.42, 0.62, 0.26],
        Terrain::Forest => [0.13, 0.42, 0.18],
        Terrain::Mountain => [0.52, 0.48, 0.44],
        Terrain::Swamp => [0.26, 0.36, 0.30],
        Terrain::Settlement => [0.78, 0.56, 0.30],
        Terrain::Temple => [0.92, 0.86, 0.62],
        Terrain::Ruins => [0.44, 0.40, 0.46],
        Terrain::Stones => [0.36, 0.54, 0.70],
        Terrain::Grove => [0.30, 0.78, 0.30],
        Terrain::Table => [0.85, 0.66, 0.24],
    };
    let tint = tile
        .region
        .map_or([0.0; 3], |g| g.accent().map(|c| c as f32 / 255.0));
    let mix = if tile.region.is_some() { 0.22 } else { 0.0 };
    let [r, g, b] = std::array::from_fn(|i| {
        let c = base[i] * (1.0 - mix) + tint[i] * mix;
        let c = match stage {
            Some(0) => c + (1.0 - c) * 0.1,
            Some(2) => c * 0.55 + 0.08,
            _ => c,
        };
        match lit {
            Lit::No => c,
            Lit::Reach => c + (1.0 - c) * 0.5,
            // Warm gold, so aiming reads differently from walking.
            Lit::Target => c * 0.3 + [1.0, 0.78, 0.25][i] * 0.7,
            Lit::Attack => c * 0.3 + [0.95, 0.25, 0.2][i] * 0.7,
            Lit::Hover => c + (1.0 - c) * 0.25,
        }
    });
    Color::srgb(r, g, b)
}

fn hex_mesh(layout: &HexLayout) -> Mesh {
    let info = PlaneMeshBuilder::new(layout)
        .with_scale(Vec3::splat(0.95))
        .center_aligned()
        .build();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, info.vertices)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, info.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, info.uvs)
    .with_inserted_indices(Indices::U16(info.indices))
}

/// A tiny shrouded body lying on its side.
const CORPSE_ROWS: [&str; 5] = [
    "..######....",
    ".#oooooo###.",
    "#oxxxxxooox#",
    "#oxxxxxxxxx#",
    ".##########.",
];

/// Iron teeth of a hidden trap; drawn only for its owner.
const TRAP_ROWS: [&str; 4] = [".r...r...r.", "#r#.#r#.#r#", "#xxxxxxxxx#", ".#########."];

/// A small pennant on a pole; `f` takes the owner's colour.
const FLAG_ROWS: [&str; 9] = [
    "#ffff.", "#fffff", "#ffff.", "#.....", "#.....", "#.....", "#.....", "#.....", "##....",
];

/// Builds a small sprite from rows of palette letters; `.` is transparent and
/// `f` is painted with `fill`.
fn pixel_sprite(rows: &[&str], fill: [u8; 3]) -> Image {
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
            let px = match c {
                b'#' => [20, 16, 24, 255],
                b'o' => [214, 206, 186, 255],
                b'x' => [150, 140, 128, 255],
                b'r' => [214, 62, 44, 255],
                b'f' => [fill[0], fill[1], fill[2], 255],
                _ => continue,
            };
            let i = (y * w as usize + x) * 4;
            data[i..i + 4].copy_from_slice(&px);
        }
    }
    image
}

const TERRAINS: [Terrain; 10] = [
    Terrain::Plains,
    Terrain::Forest,
    Terrain::Mountain,
    Terrain::Swamp,
    Terrain::Settlement,
    Terrain::Temple,
    Terrain::Ruins,
    Terrain::Stones,
    Terrain::Grove,
    Terrain::Table,
];

/// The board hex under the mouse cursor, if any.
pub fn cursor_hex(
    window: &Window,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    board: &Board,
    radius: u32,
) -> Option<Hex> {
    let cursor = window.cursor_position()?;
    let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
    let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    let hex = board.world_to_hex(ray.get_point(dist));
    (hex.ulength() <= radius).then_some(hex)
}

fn track_hover(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::TableCamera>>,
    board: Res<Board>,
    game: Res<Match>,
    mut hovered: ResMut<Hovered>,
) {
    let (camera, transform) = *camera;
    // Dev aid: `NECROMY_HOVER=q,r` pins the hover for screenshots.
    // `NECROMY_HOVER=guard` follows the royal guard.
    let pinned = std::env::var("NECROMY_HOVER").ok().and_then(|s| {
        if s == "guard" {
            return game.game.guard().map(|g| g.hex);
        }
        let (q, r) = s.split_once(',')?;
        Some(Hex::new(q.trim().parse().ok()?, r.trim().parse().ok()?))
    });
    let hex = pinned.or_else(|| {
        cursor_hex(
            &window,
            camera,
            transform,
            &board,
            game.game.board().radius(),
        )
    });
    hovered.set_if_neq(Hovered(hex));
}
