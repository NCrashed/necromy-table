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
                        .or_else(resource_changed::<Hovered>)
                        .or_else(resource_changed::<crate::lighting::DayNight>)
                        .or_else(resource_changed::<crate::tutorial::Focus>)
                        .or_else(painted_markers_loading),
                ),
        )
        .add_systems(
            crate::InGame,
            turn_flats
                .after(sync_markers)
                .after(sync_ground)
                .after(crate::camera::apply),
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

/// Ground textures in `assets/tiles/`, `<name>-<n>.png`: bare ground under
/// the terrain's billboard props (`props.rs`), temples and the Table
/// included. A terrain not listed keeps its flat colour and icon.
const TILE_ART: [(Terrain, &str, usize); 10] = [
    (Terrain::Plains, "ground/meadow", 2),
    (Terrain::Forest, "ground/forest", 1),
    (Terrain::Grove, "ground/forest", 1),
    (Terrain::Mountain, "ground/rock", 2),
    (Terrain::Swamp, "ground/swamp", 2),
    (Terrain::Settlement, "ground/village", 2),
    (Terrain::Temple, "ground/paving", 2),
    (Terrain::Table, "ground/paving", 2),
    (Terrain::Ruins, "ground/ruins", 2),
    (Terrain::Stones, "ground/moss", 2),
];

/// Tile images are `TILE_PX` square with the hexagon a little above the
/// middle (measured on the 128 px set): the quad is shifted so the hexagon,
/// not the image, sits on the hex centre.
const ART_HEX_CENTRE_PX: f32 = 61.0;

/// Lies flat on the table and turns with it (`Rig::table_turn`), so its
/// picture reads upright from the side the camera sits on: at
/// `centre + turn * offset`, rotated `turn * base`.
#[derive(Component, Clone, Copy)]
struct Flat {
    centre: Vec3,
    offset: Vec3,
    base: Quat,
}

impl Flat {
    fn transform(&self, turn: Quat) -> Transform {
        Transform::from_translation(self.centre + turn * self.offset)
            .with_rotation(turn * self.base)
    }
}

/// Every flat thing follows the table's turn; a new one takes it at once.
fn turn_flats(
    rig: Res<crate::camera::Rig>,
    mut flats: Query<(Ref<Flat>, &mut Transform)>,
    mut shown: Local<Option<f32>>,
) {
    let turn = rig.table_turn();
    let turned = *shown != Some(turn);
    *shown = Some(turn);
    let q = Quat::from_rotation_y(turn);
    for (flat, mut transform) in &mut flats {
        if turned || flat.is_changed() {
            *transform = flat.transform(q);
        }
    }
}

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

/// A marker on the board: what it shows and where, so a redraw keeps the
/// ones still true and touches only what changed.
#[derive(Component, Clone, PartialEq)]
struct Marker {
    hex: Hex,
    image: AssetId<Image>,
}

#[derive(Resource)]
struct MarkerSprites {
    corpse: Handle<Image>,
    /// Painted corpses by how long they have lain: fresh, rotting, sprouting
    /// (`assets/props/corpse-<stage>-<n>.png`). The drawn `corpse` stands in
    /// until they load.
    corpse_art: [Vec<Handle<Image>>; 3],
    /// Painted traps by the element of their card, the last for neutral
    /// ones (`assets/props/trap-<element>-<n>.png`); `trap` stands in.
    trap_art: [Vec<Handle<Image>>; 6],
    trap: Handle<Image>,
    /// Ownership flags, one per god (`God::index`).
    flags: [Handle<Image>; 5],
    /// Where a hidden rival was last seen, one per god (§11.6).
    trails: [Handle<Image>; 5],
    /// Over the goal of the human's quest (§8).
    quest: Handle<Image>,
    /// A trial (§20.2), per god: a rune circle on the ground in its colour,
    /// and the die face it asks for, framed in that colour, floating above.
    trial_rings: [Handle<Image>; 5],
    trial_faces: [Handle<Image>; 5],
    /// Something lying on the ground (§20.3): a pouch, drawn flat.
    pouch: Handle<Image>,
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
    /// Militia who let the human through: a step there trades places (§20.4).
    Swap,
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
    // A tile image spans the hex's width (two sizes), inset like the mesh.
    let art_size = 2.0 * HEX_SIZE * TILE_INSET;
    let art_quad = meshes.add(Plane3d::default().mesh().size(art_size, art_size));
    let art_shift = Vec3::Z * (TILE_PX / 2.0 - ART_HEX_CENTRE_PX) / TILE_PX * art_size;
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
            // Lit, so night falls on it (`lighting.rs`); matte, no glints on
            // painted ground.
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            ..default()
        });
        let (ground_mesh, ground_shift) = if texture.is_some() {
            (art_quad.clone(), art_shift)
        } else {
            (mesh.clone(), Vec3::ZERO)
        };
        let flat = Flat {
            centre: at,
            offset: ground_shift,
            base: Quat::IDENTITY,
        };
        commands.spawn((
            TileArt {
                hex,
                terrain: tile.terrain,
            },
            Mesh3d(ground_mesh),
            MeshMaterial3d(ground),
            flat.transform(Quat::IDENTITY),
            flat,
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
            let flat = Flat {
                centre: at,
                offset: Vec3::Y * 0.01,
                base: Quat::IDENTITY,
            };
            commands.spawn((
                TileIcon(hex),
                Mesh3d(icon_quad.clone()),
                MeshMaterial3d(icon),
                flat.transform(Quat::IDENTITY),
                flat,
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
        corpse_art: ["fresh", "rot", "sprout"].map(|stage| {
            (1..)
                .map(|n| format!("props/corpse-{stage}-{n}.png"))
                .take_while(|path| std::path::Path::new("assets").join(path).exists())
                .map(|path| assets.load(path))
                .collect()
        }),
        trap_art: ["wood", "fire", "earth", "metal", "water", "neutral"].map(|element| {
            (1..)
                .map(|n| format!("props/trap-{element}-{n}.png"))
                .take_while(|path| std::path::Path::new("assets").join(path).exists())
                .map(|path| assets.load(path))
                .collect()
        }),
        trap: images.add(pixel_sprite(&TRAP_ROWS, [0; 3])),
        flags: God::ALL.map(|g| images.add(pixel_sprite(&FLAG_ROWS, g.accent()))),
        trails: God::ALL.map(|g| images.add(pixel_sprite(&TRAIL_ROWS, g.accent()))),
        quest: images.add(pixel_sprite(&QUEST_ROWS, [250, 214, 120])),
        pouch: assets.load("items/ground-pouch.png"),
        trial_rings: God::ALL.map(|g| images.add(rune_ring(g.accent()))),
        trial_faces: God::ALL.map(|g| images.add(trial_badge(g))),
    });
}

#[allow(clippy::too_many_arguments)]
fn sync_tiles(
    game: Res<Match>,
    focus: Res<crate::tutorial::Focus>,
    day_night: Res<crate::lighting::DayNight>,
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
        game.game.reachable(game.human)
    } else {
        Default::default()
    };
    let attackable = if walking {
        game.game.attackable(game.human)
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
            necromy_rules::Goal::ReachHex(h) | necromy_rules::Goal::PassTrial(h) => Some(h),
            _ => None,
        })
        .chain(focus.hexes.iter().copied())
        .collect();
    for (tile, material) in &tiles {
        let lit = if targets.contains(&tile.0) {
            Lit::Target
        } else if attackable.contains(&tile.0) {
            Lit::Attack
        } else if quests.contains(&tile.0) {
            Lit::Quest
        } else if reachable.contains_key(&tile.0) && game.game.lets_pass(game.human, tile.0) {
            Lit::Swap
        } else if reachable.contains_key(&tile.0) {
            Lit::Reach
        } else if hovered.0 == Some(tile.0) {
            Lit::Hover
        } else {
            Lit::No
        };
        if let Some(mut m) = materials.get_mut(&material.0) {
            let region = game.game.board().tile(tile.0).and_then(|t| t.region);
            m.base_color = highlight_color(lit, region, day_night.night);
        }
    }
}

/// True while painted corpses and traps are loading, and once more when
/// they are all in, so the markers swap the stand-ins for them.
fn painted_markers_loading(
    sprites: Option<Res<MarkerSprites>>,
    images: Res<Assets<Image>>,
    mut done: Local<bool>,
) -> bool {
    if *done {
        return false;
    }
    let Some(sprites) = sprites else {
        return false;
    };
    *done = sprites
        .corpse_art
        .iter()
        .flatten()
        .all(|h| images.contains(h));
    true
}

/// Corpses for everyone; traps only for their owner, the human.
#[allow(clippy::too_many_arguments)]
fn sync_markers(
    mut commands: Commands,
    game: Res<Match>,
    focus: Res<crate::tutorial::Focus>,
    board: Res<Board>,
    sprites: Res<MarkerSprites>,
    images: Res<Assets<Image>>,
    markers: Query<(Entity, &Marker)>,
    rig: Res<crate::camera::Rig>,
) {
    let turn = rig.table_turn();
    // A corpse rots as it lies, and sprouts before it becomes a grove where
    // a grove can grow; each hex keeps its own body.
    // A body the dice on screen have not told of yet stays unseen.
    let held: Vec<Hex> = game.held_falls.iter().map(|&(_, at, _)| at).collect();
    let corpses = game
        .game
        .board()
        .corpses()
        .filter(|(hex, _)| !held.contains(hex))
        .map(|(hex, corpse)| {
            let grows = game
                .game
                .board()
                .tile(hex)
                .is_some_and(|t| t.terrain.can_grow_grove());
            let stage = match corpse.age {
                0 | 1 => 0,
                a if a + 1 >= necromy_rules::board::GROVE_AGE && grows => 2,
                _ => 1,
            };
            let art = &sprites.corpse_art[stage];
            let painted = (!art.is_empty())
                .then(|| &art[crate::props::hex_seed(hex, 7) as usize % art.len()])
                .filter(|h| images.contains(*h));
            match painted {
                Some(image) => (hex, image.clone(), CORPSE_PPM),
                None => (hex, sprites.corpse.clone(), TEXELS),
            }
        });
    let traps = game
        .game
        .traps()
        .iter()
        .filter(|t| t.owner == game.human)
        .map(|t| {
            // Drawn after the element of the card that set it.
            let element = game.game.def(t.card).element.map_or(5, |e| e.index());
            let art = &sprites.trap_art[element];
            let painted = (!art.is_empty())
                .then(|| &art[crate::props::hex_seed(t.hex, 11) as usize % art.len()])
                .filter(|h| images.contains(*h));
            match painted {
                Some(image) => (t.hex, image.clone(), CORPSE_PPM),
                None => (t.hex, sprites.trap.clone(), TEXELS),
            }
        });
    // Owner flags stand at the back left of the hex, out of the champion's way.
    let flags = game.game.claims().filter_map(|(hex, p)| {
        let god = game.game.champion(p)?.god;
        Some((
            hex,
            sprites.flags[god.index()].clone(),
            Vec3::new(-0.45, 0.0, -0.2),
            TEXELS,
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
                TEXELS,
            ))
        });
    // Bodies and traps are drawn from above: they lie flat on the ground,
    // under whoever stands there, instead of rising in front of them.
    let corpses = corpses.map(|(h, i, ppm)| (h, i, Vec3::ZERO, ppm, true));
    let traps = traps.map(|(h, i, ppm)| (h, i, Vec3::ZERO, ppm, true));
    let flags = flags.map(|(h, i, o, ppm)| (h, i, o, ppm, false));
    // The goal of a quest the human carries: a golden mark over the hex.
    let quest_marks: Vec<_> = game
        .game
        .lines_of(game.human)
        .filter_map(|l| match l.goal {
            necromy_rules::Goal::ReachHex(h) | necromy_rules::Goal::PassTrial(h) => Some(h),
            _ => None,
        })
        .chain(focus.hexes.iter().copied())
        .map(|h| {
            (
                h,
                sprites.quest.clone(),
                Vec3::new(0.3, 0.9, -0.1),
                TEXELS,
                false,
            )
        })
        .collect();
    // Items on the ground: a pouch lying by the hex's edge, once loaded.
    let ground: Vec<_> = game
        .game
        .ground_items()
        .iter()
        .map(|(hex, _)| *hex)
        .filter(|hex| !held.contains(hex))
        .filter(|_| images.contains(&sprites.pouch))
        .map(|hex| {
            (
                hex,
                sprites.pouch.clone(),
                Vec3::new(0.3, 0.0, 0.3),
                TEXELS,
                true,
            )
        })
        .collect();
    // Trials: the god's stone, and the face it asks for above it.
    let trial_marks: Vec<_> = game
        .game
        .trials()
        .iter()
        .flat_map(|t| {
            let g = t.god.index();
            [
                (
                    t.hex,
                    sprites.trial_rings[g].clone(),
                    Vec3::ZERO,
                    TEXELS,
                    true,
                ),
                (
                    t.hex,
                    sprites.trial_faces[g].clone(),
                    Vec3::new(0.0, 0.95, 0.0),
                    TEXELS,
                    false,
                ),
            ]
        })
        .collect();
    let trails = trails.map(|(h, i, o, ppm)| (h, i, o, ppm, false));
    let wanted: Vec<_> = corpses
        .chain(traps)
        .chain(flags)
        .chain(trails)
        .chain(quest_marks)
        .chain(trial_marks)
        .chain(ground)
        .collect();
    // Despawning and respawning everything would blink every marker for a
    // frame (this runs on each hover): keep what is still wanted.
    let mut kept = Vec::new();
    for (entity, marker) in &markers {
        if wanted
            .iter()
            .any(|(h, i, ..)| *h == marker.hex && i.id() == marker.image)
        {
            kept.push(marker.clone());
        } else {
            commands.entity(entity).despawn();
        }
    }
    for (hex, image, offset, pixels_per_metre, flat) in wanted {
        let marker = Marker {
            hex,
            image: image.id(),
        };
        if kept.contains(&marker) {
            continue;
        }
        let pos = board.hex_to_world(hex) + offset;
        let mut entity = commands.spawn((
            marker,
            NotShadowCaster,
            NotShadowReceiver,
            Sprite::from_image(image),
            Sprite3d {
                pixels_per_metre,
                pivot: Some(if flat {
                    Vec2::splat(0.5)
                } else {
                    Vec2::new(0.5, 0.0)
                }),
                alpha_mode: AlphaMode::Mask(0.5),
                unlit: true,
                ..default()
            },
        ));
        if flat {
            // Just above the region's veil, the top of the picture away from
            // the table's near edge, whichever side the camera sits on.
            let flat = Flat {
                centre: pos,
                offset: Vec3::Y * 0.03,
                base: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            };
            entity.insert((flat.transform(Quat::from_rotation_y(turn)), flat));
        } else {
            entity.insert((Billboard, Transform::from_translation(pos)));
        }
    }
}

/// Tiles are drawn at this share of their size so the grid reads.
const TILE_INSET: f32 = 0.95;

/// Texels per world unit, the same for every sprite and tile on the table,
/// so none is drawn with bigger pixels than another: a 128 px tile spans a
/// hex (`camera.rs` makes a texel a whole number of screen pixels).
pub const TEXELS: f32 = TILE_PX / (2.0 * HEX_SIZE * TILE_INSET);
/// Width of a ground tile picture.
const TILE_PX: f32 = 128.0;

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
/// `night` (0..1) darkens the region wash: the veil is unlit, and a bright
/// wash on dark ground would glow like stained glass. Highlights stay bright
/// at any hour.
fn highlight_color(lit: Lit, region: Option<God>, night: f32) -> Color {
    match lit {
        Lit::No => region.map_or(Color::NONE, |g| {
            let dim = 1.0 - 0.8 * night;
            let [r, g, b] = g.accent().map(|c| c as f32 / 255.0 * dim);
            Color::srgba(r, g, b, 46.0 / 255.0)
        }),
        Lit::Reach => Color::srgba(1.0, 0.97, 0.85, 0.3),
        Lit::Hover => Color::srgba(1.0, 1.0, 1.0, 0.15),
        // Warm gold, so aiming reads differently from walking.
        Lit::Target => Color::srgba(1.0, 0.78, 0.25, 0.6),
        Lit::Attack => Color::srgba(0.95, 0.25, 0.2, 0.6),
        Lit::Quest => Color::srgba(0.75, 0.55, 1.0, 0.55),
        // Friendly green: not a fight, a trade of places.
        Lit::Swap => Color::srgba(0.4, 0.9, 0.45, 0.5),
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

/// Painted corpses and traps: at the one texel density.
const CORPSE_PPM: f32 = TEXELS;

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

/// A golden exclamation mark: the goal of a quest.
const QUEST_ROWS: [&str; 22] = [
    "..#####..",
    ".#FFFFF#.",
    "#FFfffFF#",
    "#Ffffffd#",
    "#fffffdd#",
    ".#fffdd#.",
    ".#fffdd#.",
    ".#fffdd#.",
    "..#fdd#..",
    "..#fdd#..",
    "..#fdd#..",
    "...#d#...",
    "...#d#...",
    "....#....",
    ".........",
    ".........",
    "..#####..",
    ".#FFFFd#.",
    ".#Fffdd#.",
    ".#fffdd#.",
    "..#ddd#..",
    "...###...",
];

/// A question mark in the hidden rival's colour: last seen here.
const TRAIL_ROWS: [&str; 14] = [
    "..#####..",
    ".#FFFFF#.",
    "#Ff###fF#",
    "#f#...#f#",
    ".#....#f#",
    "....##ff#",
    "...#ff##.",
    "...#f#...",
    "...#f#...",
    "....#....",
    ".........",
    "...###...",
    "...#F#...",
    "...###...",
];

/// A pennant on a pole; `f` takes the owner's colour, `F` and `d` its
/// light and shade.
const FLAG_ROWS: [&str; 24] = [
    ".###.........",
    ".#x#.........",
    ".#w##########",
    ".#w#FFFFFFFF#",
    ".#w#Fffffff#.",
    ".#w#fffffff#.",
    ".#w#ffffff#..",
    ".#w#fffffff#.",
    ".#w#fdddddd#.",
    ".#w##########",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    ".#w#.........",
    "##w##........",
    "#####........",
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
                b'F' => {
                    let l = fill.map(|c| (c as u16 + (255 - c as u16) / 2) as u8);
                    [l[0], l[1], l[2], 255]
                }
                b'd' => {
                    let d = fill.map(|c| (c as u16 * 3 / 5) as u8);
                    [d[0], d[1], d[2], 255]
                }
                b'w' => [122, 86, 52, 255],
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
    // `NECROMY_HOVER=guard` follows the royal guard, `=trial` the trial
    // nearest the human.
    let pinned = std::env::var("NECROMY_HOVER").ok().and_then(|s| {
        if s == "guard" {
            return game.game.guard().map(|g| g.hex);
        }
        // The militia standing nearest the human, and the first ruins (§20.4).
        if s == "militia" {
            let me = game.game.champion(game.human)?.hex;
            return game
                .game
                .militias()
                .filter_map(|(_, m)| m.at.filter(|_| m.men > 0))
                .min_by_key(|h| (me.unsigned_distance_to(*h), h.x(), h.y()));
        }
        if s == "ruins" {
            return game
                .game
                .board()
                .tiles()
                .map(|(h, _)| h)
                .find(|&h| game.game.is_ruined_settlement(h));
        }
        // The first undead or beast on the board (§20.4).
        if s == "undead" || s == "beast" {
            let beast = s == "beast";
            return game
                .game
                .mobs()
                .iter()
                .find(|m| m.is_beast() == beast)
                .map(|m| m.hex);
        }
        // The latest battle or trial of others (`watch_ui.rs`).
        if s == "show" {
            return game
                .shows
                .iter()
                .rev()
                .find(|show| !show.who.contains(&game.human))
                .map(|show| show.hex);
        }
        if s == "trial" {
            let me = game.game.champion(game.human)?.hex;
            return game
                .game
                .trials()
                .iter()
                .map(|t| t.hex)
                .min_by_key(|h| (me.unsigned_distance_to(*h), h.x(), h.y()));
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
    mut grounds: Query<(
        &mut TileArt,
        &MeshMaterial3d<StandardMaterial>,
        &mut Mesh3d,
        &mut Flat,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (mut art, material, mut mesh, mut flat) in &mut grounds {
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
        let (new_mesh, shift) = match texture.is_some() {
            true => (meshes.painted.clone(), meshes.shift),
            false => (meshes.plain.clone(), Vec3::ZERO),
        };
        m.base_color_texture = texture;
        mesh.0 = new_mesh;
        // `turn_flats` places it anew.
        flat.offset = shift;
    }
}

/// A circle of runes on the ground in a god's colour: a trial stands here
/// (§20.2). Drawn from above, like bodies and traps.
fn rune_ring(color: [u8; 3]) -> Image {
    const N: u32 = 44;
    let mut image = Image::new_fill(
        Extent3d {
            width: N,
            height: N,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let light = color.map(|c| (c as u16 + (255 - c as u16) / 2) as u8);
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    let c = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            let r = (dx * dx + dy * dy).sqrt();
            // Five runes on the ring, one for each god, the ink rim around.
            let angle = dy.atan2(dx).rem_euclid(std::f32::consts::TAU);
            let rune = (angle / std::f32::consts::TAU * 5.0).fract();
            let px = if (19.5..21.5).contains(&r) {
                [light[0], light[1], light[2], 255]
            } else if (21.5..22.5).contains(&r) || (18.5..19.5).contains(&r) {
                [20, 16, 24, 255]
            } else if (14.5..17.5).contains(&r) && (0.42..0.58).contains(&rune) {
                [245, 240, 225, 255]
            } else {
                continue;
            };
            let i = ((y * N + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&px);
        }
    }
    image
}

/// The die face a god's trials ask for, in a frame of the god's colour.
fn trial_badge(god: God) -> Image {
    const N: u32 = 20;
    let face = crate::dice::face_icon(necromy_rules::trial_face(god));
    let [r, g, b] = god.accent();
    let mut image = Image::new_fill(
        Extent3d {
            width: N,
            height: N,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[r, g, b, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    // An ink rim outside the colour.
    for y in 0..N {
        for x in 0..N {
            if x == 0 || y == 0 || x == N - 1 || y == N - 1 {
                let i = ((y * N + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&[20, 16, 24, 255]);
            }
        }
    }
    let src = face.data.as_ref().expect("face icons have pixel data");
    for y in 0..16 {
        for x in 0..16 {
            let s = ((y * 16 + x) * 4) as usize;
            let d = (((y + 2) * N + x + 2) * 4) as usize;
            data[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    image
}
