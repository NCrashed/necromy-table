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
use necromy_rules::{Target, Terrain, Tile as RulesTile};

use crate::play::{Match, Selection};
use crate::token::Billboard;

pub const HEX_SIZE: f32 = 1.0;

pub struct BoardPlugin;

impl Plugin for BoardPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Board {
            layout: HexLayout::flat().with_hex_size(HEX_SIZE),
        })
        .add_systems(Startup, spawn_board)
        .add_systems(
            Update,
            (sync_tiles, sync_markers)
                .run_if(resource_changed::<Match>.or_else(resource_changed::<Selection>)),
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

#[derive(Component)]
struct Marker;

#[derive(Resource)]
struct MarkerSprites {
    corpse: Handle<Image>,
    trap: Handle<Image>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lit {
    No,
    /// The human can walk here.
    Reach,
    /// A legal target for the card being aimed.
    Target,
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
    for (hex, tile) in game.game.board().tiles() {
        // One material per tile, so highlighting can recolour it alone.
        let material = materials.add(StandardMaterial {
            base_color: tile_color(tile, Lit::No),
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
    }
    commands.insert_resource(MarkerSprites {
        corpse: images.add(pixel_sprite(&CORPSE_ROWS)),
        trap: images.add(pixel_sprite(&TRAP_ROWS)),
    });
}

fn sync_tiles(
    game: Res<Match>,
    selection: Res<Selection>,
    tiles: Query<(&Tile, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let reachable = if game.is_human_turn() && selection.card.is_none() {
        game.game.reachable()
    } else {
        Default::default()
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
        } else if reachable.contains_key(&tile.0) {
            Lit::Reach
        } else {
            Lit::No
        };
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.base_color = tile_color(rules_tile, lit);
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
    for (hex, image) in corpses.chain(traps) {
        // Offset towards the camera so a champion on the same hex stands behind it.
        let pos = board.hex_to_world(hex) + Vec3::new(0.0, 0.0, 0.35);
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
fn tile_color(tile: &RulesTile, lit: Lit) -> Color {
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
        match lit {
            Lit::No => c,
            Lit::Reach => c + (1.0 - c) * 0.5,
            // Warm gold, so aiming reads differently from walking.
            Lit::Target => c * 0.3 + [1.0, 0.78, 0.25][i] * 0.7,
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

/// Builds a small sprite from rows of palette letters; `.` is transparent.
fn pixel_sprite(rows: &[&str]) -> Image {
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
                _ => continue,
            };
            let i = (y * w as usize + x) * 4;
            data[i..i + 4].copy_from_slice(&px);
        }
    }
    image
}
