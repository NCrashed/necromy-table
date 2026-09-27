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
        .add_systems(crate::MatchBegins, spawn_board)
        .add_systems(crate::InGame, track_hover)
        .add_systems(
            crate::InGame,
            (sync_tiles, sync_markers, sync_ground)
                .after(track_hover)
                .run_if(
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

/// The highlight over a hex: clear until the hex is lit (reach, target...).
#[derive(Component)]
pub struct Tile(pub Hex);

/// The painted ground of a hex: its terrain tile, tinted by region and stage.
#[derive(Component)]
struct TileArt {
    hex: Hex,
    terrain: Terrain,
}

/// Painted terrain tiles in `assets/tiles/`, `<name>-<n>.png`. A terrain not
/// listed keeps its flat colour and icon.
const TILE_ART: [(Terrain, &str, usize); 9] = [
    (Terrain::Plains, "plains", 3),
    (Terrain::Forest, "forest", 4),
    (Terrain::Mountain, "mountain", 3),
    (Terrain::Swamp, "swamp", 2),
    (Terrain::Settlement, "settlement", 3),
    (Terrain::Temple, "temple", 4),
    (Terrain::Ruins, "ruins", 4),
    (Terrain::Stones, "stones", 4),
    (Terrain::Grove, "grove", 4),
];

/// Tile images are 64×64 with the hexagon in the top 55.4 rows: the quad is
/// shifted so the hexagon, not the image, sits on the hex centre.
const ART_HEX_CENTRE_PX: f32 = 64.0 * 0.866_025_4 / 2.0;

/// The meshes a hex's ground switches between when its terrain changes.
#[derive(Resource)]
struct GroundMeshes {
    plain: Handle<Mesh>,
    painted: Handle<Mesh>,
    /// From the hex centre to the painted quad's centre.
    shift: Vec3,
}

#[derive(Resource)]
struct TileImages(HashMap<Terrain, Vec<Handle<Image>>>);

impl TileImages {
    /// The same variant for a hex every time and on every client.
    fn pick(&self, terrain: Terrain, hex: Hex) -> Option<Handle<Image>> {
        let variants = self.0.get(&terrain)?;
        let mix = hex.x.wrapping_mul(73_856_093) ^ hex.y.wrapping_mul(19_349_663);
        Some(variants[mix.rem_euclid(variants.len() as i32) as usize].clone())
    }
}

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
    /// Where a hidden rival was last seen, one per god (§11.6).
    trails: [Handle<Image>; 5],
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
    /// The goal of one of the human's story lines.
    Quest,
}

fn spawn_board(
    mut commands: Commands,
    board: Res<Board>,
    game: Res<Match>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
) {
    // One shared mesh for every tile, slightly inset so the grid reads.
    let mesh = meshes.add(hex_mesh(&board.layout));
    let icon_quad = meshes.add(Plane3d::default().mesh().size(1.1, 1.1));
    let tile_images = TileImages(
        TILE_ART
            .iter()
            .map(|&(terrain, name, n)| {
                let variants = (0..n)
                    .map(|i| assets.load(format!("tiles/{name}-{i}.png")))
                    .collect();
                (terrain, variants)
            })
            .collect(),
    );
    // A 64 px image spans the hex's width (two sizes), inset like the mesh.
    let art_size = 2.0 * HEX_SIZE * TILE_INSET;
    let art_quad = meshes.add(Plane3d::default().mesh().size(art_size, art_size));
    let art_shift = Vec3::Z * (32.0 - ART_HEX_CENTRE_PX) / 64.0 * art_size;
    // Terrains with painted tiles need no icon.
    let icons: HashMap<Terrain, Handle<StandardMaterial>> = TERRAINS
        .into_iter()
        .filter(|t| !tile_images.0.contains_key(t))
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
        let at = board.hex_to_world(hex);
        let stage = tile.region.map(|g| game.game.stage(g));
        // The ground: a painted tile where there is one, flat colour on the
        // hex mesh where there is not. One material per hex, so terrain and
        // region can recolour it alone.
        let texture = tile_images.pick(tile.terrain, hex);
        let ground = materials.add(StandardMaterial {
            base_color: ground_color(tile, texture.is_some(), stage),
            base_color_texture: texture.clone(),
            alpha_mode: AlphaMode::Mask(0.5),
            // Flat colour, no shading: the board reads like a painted table.
            unlit: true,
            ..default()
        });
        let (ground_mesh, ground_at) = if texture.is_some() {
            (art_quad.clone(), at + art_shift)
        } else {
            (mesh.clone(), at)
        };
        commands.spawn((
            TileArt {
                hex,
                terrain: tile.terrain,
            },
            Mesh3d(ground_mesh),
            MeshMaterial3d(ground),
            Transform::from_translation(ground_at),
        ));
        // The highlight, clear until the hex is lit.
        let highlight = materials.add(StandardMaterial {
            base_color: Color::NONE,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        commands.spawn((
            Tile(hex),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(highlight),
            Transform::from_translation(at + Vec3::Y * 0.02),
            NotShadowCaster,
            NotShadowReceiver,
        ));
        // Every tile gets an icon entity; painted and plain ground hide it.
        let icon = icons.get(&tile.terrain).or(icons.values().next()).cloned();
        if let Some(icon) = icon {
            commands.spawn((
                TileIcon(hex),
                Mesh3d(icon_quad.clone()),
                MeshMaterial3d(icon),
                Transform::from_translation(at + Vec3::Y * 0.01),
                if icons.contains_key(&tile.terrain) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
        }
    }
    commands.insert_resource(IconMaterials(icons));
    commands.insert_resource(tile_images);
    commands.insert_resource(GroundMeshes {
        plain: mesh,
        painted: art_quad,
        shift: art_shift,
    });
    commands.insert_resource(MarkerSprites {
        corpse: images.add(pixel_sprite(&CORPSE_ROWS, [0; 3])),
        trap: images.add(pixel_sprite(&TRAP_ROWS, [0; 3])),
        flags: God::ALL.map(|g| images.add(pixel_sprite(&FLAG_ROWS, g.accent()))),
        trails: God::ALL.map(|g| images.add(pixel_sprite(&TRAIL_ROWS, g.accent()))),
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
    // Pilgrimage temples of the human's lines glow (§8).
    let quests: Vec<Hex> = game
        .game
        .lines_of(game.human)
        .filter_map(|l| match l.goal {
            necromy_rules::Goal::ReachHex(h) => Some(h),
            _ => None,
        })
        .collect();
    for (tile, material) in &tiles {
        let lit = if targets.contains(&tile.0) {
            Lit::Target
        } else if attackable.contains(&tile.0) {
            Lit::Attack
        } else if quests.contains(&tile.0) {
            Lit::Quest
        } else if reachable.contains_key(&tile.0) {
            Lit::Reach
        } else if hovered.0 == Some(tile.0) {
            Lit::Hover
        } else {
            Lit::No
        };
        if let Some(mut m) = materials.get_mut(&material.0) {
            let region = game.game.board().tile(tile.0).and_then(|t| t.region);
            m.base_color = highlight_color(lit, region);
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
            16.0,
        ))
    });
    // Hidden rivals: a mark where they were last seen, which is where the
    // view keeps them.
    let trails = game
        .game
        .players()
        .filter(|&p| p != game.human && game.game.is_hidden(p))
        .filter_map(|p| {
            let c = game.game.champion(p)?;
            Some((
                c.hex,
                sprites.trails[c.god.index()].clone(),
                Vec3::ZERO,
                8.0,
            ))
        });
    let front = Vec3::new(0.0, 0.0, 0.35);
    // Pixels per metre: markers are small, the trail is drawn twice as big.
    let corpses = corpses.map(|(h, i)| (h, i, front, 16.0));
    let traps = traps.map(|(h, i)| (h, i, front, 16.0));
    for (hex, image, offset, pixels_per_metre) in corpses.chain(traps).chain(flags).chain(trails) {
        let pos = board.hex_to_world(hex) + offset;
        commands.spawn((
            Marker,
            Billboard,
            NotShadowCaster,
            NotShadowReceiver,
            Sprite::from_image(image),
            Sprite3d {
                pixels_per_metre,
                pivot: Some(Vec2::new(0.5, 0.0)),
                alpha_mode: AlphaMode::Mask(0.5),
                unlit: true,
                ..default()
            },
            Transform::from_translation(pos),
        ));
    }
}

/// Tiles are drawn at this share of their size so the grid reads.
const TILE_INSET: f32 = 0.95;

/// The ground of a hex: a painted tile as painted, or a flat terrain colour
/// tinted towards its region's god where there is no tile. `stage` of the region's god recolours both: light a touch
/// brighter, dark sunk towards ash (§5).
fn ground_color(tile: &RulesTile, painted: bool, stage: Option<u8>) -> Color {
    let base = if painted {
        [1.0; 3]
    } else {
        match tile.terrain {
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
        }
    };
    let tint = tile
        .region
        .map_or([0.0; 3], |g| g.accent().map(|c| c as f32 / 255.0));
    let mix = match (tile.region.is_some(), painted) {
        (false, _) => 0.0,
        // A painted tile gets its region from the veil (`highlight_color`):
        // multiplying green grass by violet only darkens it.
        (true, true) => 0.0,
        (true, false) => 0.22,
    };
    let [r, g, b] = std::array::from_fn(|i| {
        let c = base[i] * (1.0 - mix) + tint[i] * mix;
        match stage {
            // A painted tile is already at full brightness: light leaves it be.
            Some(0) if !painted => c + (1.0 - c) * 0.1,
            Some(2) => c * 0.55 + 0.08,
            _ => c,
        }
    });
    Color::srgb(r, g, b)
}

/// The veil over a hex: the highlight when it is lit, else a wash of its
/// region's god so wedges read at a glance.
fn highlight_color(lit: Lit, region: Option<God>) -> Color {
    match lit {
        Lit::No => region.map_or(Color::NONE, |g| {
            let [r, g, b] = g.accent();
            Color::srgba_u8(r, g, b, 46)
        }),
        Lit::Reach => Color::srgba(1.0, 0.97, 0.85, 0.3),
        Lit::Hover => Color::srgba(1.0, 1.0, 1.0, 0.15),
        // Warm gold, so aiming reads differently from walking.
        Lit::Target => Color::srgba(1.0, 0.78, 0.25, 0.6),
        Lit::Attack => Color::srgba(0.95, 0.25, 0.2, 0.6),
        Lit::Quest => Color::srgba(0.75, 0.55, 1.0, 0.55),
    }
}

fn hex_mesh(layout: &HexLayout) -> Mesh {
    let info = PlaneMeshBuilder::new(layout)
        .with_scale(Vec3::splat(TILE_INSET))
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

/// A question mark in the hidden rival's colour: last seen here.
const TRAIL_ROWS: [&str; 9] = [
    ".###.", "#fff#", "#f#f#", "..#f#", ".#f#.", ".#f#.", "..#..", ".#f#.", "..#..",
];

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

/// A hex's ground follows its terrain (a body grows into a grove) and its
/// god's stage.
fn sync_ground(
    game: Res<Match>,
    tile_images: Res<TileImages>,
    meshes: Res<GroundMeshes>,
    board: Res<Board>,
    mut grounds: Query<(
        &mut TileArt,
        &MeshMaterial3d<StandardMaterial>,
        &mut Mesh3d,
        &mut Transform,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (mut art, material, mut mesh, mut transform) in &mut grounds {
        let Some(tile) = game.game.board().tile(art.hex) else {
            continue;
        };
        let texture = tile_images.pick(tile.terrain, art.hex);
        let color = ground_color(
            tile,
            texture.is_some(),
            tile.region.map(|g| game.game.stage(g)),
        );
        let same_color = materials
            .get(&material.0)
            .is_some_and(|m| m.base_color == color);
        if same_color && art.terrain == tile.terrain {
            // Touching the material would re-upload it: leave it be.
            continue;
        }
        let Some(mut m) = materials.get_mut(&material.0) else {
            continue;
        };
        m.base_color = color;
        if art.terrain == tile.terrain {
            continue;
        }
        art.terrain = tile.terrain;
        let at = board.hex_to_world(art.hex);
        let (new_mesh, new_at) = match texture.is_some() {
            true => (meshes.painted.clone(), at + meshes.shift),
            false => (meshes.plain.clone(), at),
        };
        m.base_color_texture = texture;
        mesh.0 = new_mesh;
        transform.translation = new_at;
    }
}
