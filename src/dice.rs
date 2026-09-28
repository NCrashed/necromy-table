//! Battle throws (docs/design.md §12.2).
//!
//! The rules only send a throw's seed and dice count. The client runs the
//! same `necromy_dice::throw` and plays its frames back, so every screen
//! shows the motion that decided the battle.
//!
//! The two trays live in their own little scene on render layer 1, far from
//! the board. Each has a camera that renders into a texture; the battle
//! panel (`battle_ui.rs`) shows those textures next to the fighters.
//!
//! Dice here are placeholder cubes with a pixel symbol atlas; skins (§12.3)
//! will replace the mesh, material, particles and sounds, never the frames.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use necromy_dice::{DIE_HALF, FACE_AXES, Face, TICK_HZ, TRAY_HALF, Throw};

use crate::play::Match;

/// Render layer of the dice scene.
const DICE_LAYER: usize = 1;
/// Centre of each tray in the dice scene: attacker, defender. Far apart so
/// each camera sees only its own.
const TRAYS: [Vec3; 2] = [Vec3::new(-60.0, 0.0, 0.0), Vec3::new(60.0, 0.0, 0.0)];
/// Size of each tray texture, in pixels; the tray is 10 × 7 units.
pub const TRAY_TEXTURE: [u32; 2] = [480, 336];
/// Pause between a throw and the explosion throw after it.
const BETWEEN_SECS: f32 = 0.4;
/// How long the result stays on screen after the last die settles.
const HOLD_SECS: f32 = 2.2;

pub struct DicePlugin;

impl Plugin for DicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiceShow>()
            .init_resource::<Revealed>()
            .add_systems(Startup, build_dice_scene)
            .add_systems(crate::InGame, (take_throws, play_throws).chain());
    }
}

/// A throw waiting for its tray.
struct Queued {
    seed: u64,
    count: u8,
    faces: Vec<Face>,
}

struct Playing {
    throw: Throw,
    started: f32,
    dice: Vec<Entity>,
    revealed: bool,
}

impl Playing {
    fn done(&self, now: f32) -> bool {
        now - self.started >= self.throw.ticks() as f32 / TICK_HZ as f32
    }
}

#[derive(Resource, Default)]
pub struct DiceShow {
    queue: [VecDeque<Queued>; 2],
    playing: [Option<Playing>; 2],
    /// When everything settled; the panel closes `HOLD_SECS` later.
    settled_at: Option<f32>,
    /// Dice have settled on screen at least once (dev screenshots wait on it).
    pub ever_settled: bool,
    /// The battle is decided and still on screen: the table waits.
    busy: bool,
}

impl DiceShow {
    /// Every queued throw has landed and been revealed.
    pub fn landed(&self) -> bool {
        self.queue.iter().all(VecDeque::is_empty)
            && self.playing.iter().flatten().all(|p| p.revealed)
    }

    /// A decided battle is still on screen; bots and turns wait for it.
    pub fn busy(&self) -> bool {
        self.busy || self.queue.iter().any(|q| !q.is_empty())
    }
}

/// Thrown faces that have settled on screen, per side. Changes only when a
/// throw lands, so the battle panel redraws only then.
#[derive(Resource, Default)]
pub struct Revealed(pub [Vec<Face>; 2]);

/// The textures the tray cameras render into: attacker, defender.
#[derive(Resource)]
pub struct TrayTextures(pub [Handle<Image>; 2]);

#[derive(Component)]
struct TrayCamera;

#[derive(Resource)]
struct DiceAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

fn build_dice_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let layer = RenderLayers::layer(DICE_LAYER);
    let atlas = images.add(face_atlas());
    commands.insert_resource(DiceAssets {
        mesh: meshes.add(die_mesh(DIE_HALF)),
        material: materials.add(StandardMaterial {
            base_color_texture: Some(atlas),
            perceptual_roughness: 0.6,
            ..default()
        }),
    });

    let [hx, hz] = TRAY_HALF;
    let felt = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.10, 0.14),
        perceptual_roughness: 1.0,
        ..default()
    });
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.22, 0.12),
        perceptual_roughness: 0.8,
        ..default()
    });
    let floor = meshes.add(Plane3d::default().mesh().size(hx * 2.0, hz * 2.0));
    let rim_x = meshes.add(Cuboid::new(hx * 2.0 + 0.8, 0.6, 0.4));
    let rim_z = meshes.add(Cuboid::new(0.4, 0.6, hz * 2.0 + 0.8));

    let mut textures = Vec::new();
    for centre in TRAYS {
        commands.spawn((
            Mesh3d(floor.clone()),
            MeshMaterial3d(felt.clone()),
            Transform::from_translation(centre),
            layer.clone(),
        ));
        for (mesh, offset) in [
            (rim_x.clone(), Vec3::new(0.0, 0.3, -hz - 0.2)),
            (rim_x.clone(), Vec3::new(0.0, 0.3, hz + 0.2)),
            (rim_z.clone(), Vec3::new(-hx - 0.2, 0.3, 0.0)),
            (rim_z.clone(), Vec3::new(hx + 0.2, 0.3, 0.0)),
        ] {
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(wood.clone()),
                Transform::from_translation(centre + offset),
                layer.clone(),
            ));
        }

        let image = images.add(Image::new_target_texture(
            TRAY_TEXTURE[0],
            TRAY_TEXTURE[1],
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
        commands.spawn((
            TrayCamera,
            Camera3d::default(),
            Camera {
                // Render before the board camera; idle until a battle.
                order: -1,
                is_active: false,
                clear_color: ClearColorConfig::Custom(Color::srgb(0.07, 0.05, 0.08)),
                ..default()
            },
            RenderTarget::from(image.clone()),
            Transform::from_translation(centre + Vec3::new(0.0, 9.5, 5.0))
                .looking_at(centre + Vec3::new(0.0, 0.0, 0.4), Vec3::Y),
            layer.clone(),
        ));
        textures.push(image);
    }
    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            ..default()
        },
        Transform::from_xyz(3.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        layer,
    ));
    commands.insert_resource(TrayTextures([textures[0].clone(), textures[1].clone()]));
}

/// Moves new throws from the match into the tray queues.
fn take_throws(mut game: ResMut<Match>, mut show: ResMut<DiceShow>) {
    if game.throws.is_empty() {
        return;
    }
    for t in game.throws.drain(..) {
        show.queue[t.side].push_back(Queued {
            seed: t.seed,
            count: t.count,
            faces: t.faces,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn play_throws(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<DiceAssets>,
    mut show: ResMut<DiceShow>,
    mut revealed: ResMut<Revealed>,
    mut game: ResMut<Match>,
    mut cameras: Query<&mut Camera, With<TrayCamera>>,
    mut transforms: Query<&mut Transform>,
) {
    let now = time.elapsed_secs();
    // Read through `Match` without touching it: only closing the panel below
    // may mark it changed.
    let open = game.battle.is_some();
    let decided = game.battle.as_ref().is_some_and(|b| b.scores.is_some());
    for mut camera in &mut cameras {
        if camera.is_active != open {
            camera.is_active = open;
        }
    }
    if !open && show.queue.iter().all(VecDeque::is_empty) {
        return;
    }

    for (side, centre) in TRAYS.into_iter().enumerate() {
        // Reveal a throw's faces once its dice have landed.
        if let Some(p) = show.playing[side].as_mut()
            && !p.revealed
            && p.done(now)
        {
            p.revealed = true;
            let faces = p.throw.faces.clone();
            revealed.0[side].extend(faces);
        }

        // Start the next throw once the previous one settled and paused.
        let ready = match &show.playing[side] {
            None => true,
            Some(p) => {
                p.done(now)
                    && now - p.started > p.throw.ticks() as f32 / TICK_HZ as f32 + BETWEEN_SECS
            }
        };
        if ready && let Some(next) = show.queue[side].pop_front() {
            if let Some(old) = show.playing[side].take() {
                for e in old.dice {
                    commands.entity(e).despawn();
                }
            }
            let throw = necromy_dice::throw(next.seed, next.count);
            if throw.faces != next.faces {
                // Our physics diverged from the server's. Result stays the
                // server's; later the server's trajectory is shown instead.
                warn!(
                    "dice replay diverged: seed {} shows {:?}, server says {:?}",
                    next.seed, throw.faces, next.faces
                );
            }
            let dice = (0..next.count)
                .map(|_| {
                    commands
                        .spawn((
                            Mesh3d(assets.mesh.clone()),
                            MeshMaterial3d(assets.material.clone()),
                            Transform::default(),
                            RenderLayers::layer(DICE_LAYER),
                        ))
                        .id()
                })
                .collect();
            show.playing[side] = Some(Playing {
                throw,
                started: now,
                dice,
                revealed: false,
            });
            show.settled_at = None;
        }

        if let Some(p) = &show.playing[side] {
            let tick = (((now - p.started) * TICK_HZ as f32) as usize).min(p.throw.ticks() - 1);
            for (die, pose) in p.dice.iter().zip(&p.throw.frames[tick]) {
                if let Ok(mut t) = transforms.get_mut(*die) {
                    let [x, y, z] = pose.pos;
                    let [qx, qy, qz, qw] = pose.rot;
                    t.translation = centre + Vec3::new(x, y, z);
                    t.rotation = Quat::from_xyzw(qx, qy, qz, qw);
                }
            }
        }
    }

    let landed = show.queue.iter().all(VecDeque::is_empty)
        && show.playing.iter().flatten().all(|p| p.revealed);
    let busy = decided;
    if show.busy != busy {
        show.busy = busy;
    }
    if !(decided && landed) {
        return;
    }
    let settled = *show.settled_at.get_or_insert(now);
    if !show.ever_settled {
        show.ever_settled = true;
    }
    // The fight on the panel's stage plays out before it closes.
    let fight = game.battle.as_ref().map_or(0.0, crate::fight::show_secs);
    if now - settled < HOLD_SECS.max(fight) {
        return;
    }
    for p in show.playing.iter_mut().filter_map(Option::take) {
        for e in p.dice {
            commands.entity(e).despawn();
        }
    }
    show.settled_at = None;
    show.busy = false;
    revealed.0 = [Vec::new(), Vec::new()];
    game.end_battle_view();
}

/// One face as a 16×16 icon, drawn like the die itself: for the panel.
pub fn face_icon(face: Face) -> Image {
    const N: usize = 16;
    let mut image = Image::new_fill(
        Extent3d {
            width: N as u32,
            height: N as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    for y in 0..N {
        for x in 0..N {
            let edge = x == 0 || y == 0 || x == N - 1 || y == N - 1;
            let px = if edge {
                [28, 22, 30, 255]
            } else {
                [232, 222, 200, 255]
            };
            data[(y * N + x) * 4..(y * N + x) * 4 + 4].copy_from_slice(&px);
        }
    }
    for (y, row) in GLYPHS[atlas_cell(face)].iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            let px = match c {
                b'#' => [28, 22, 30, 255],
                b'o' => [200, 120, 30, 255],
                _ => continue,
            };
            let i = ((y + 2) * N + x + 2) * 4;
            data[i..i + 4].copy_from_slice(&px);
        }
    }
    image
}

/// A cube whose six faces map to cells of the symbol atlas so that each
/// face's outward axis matches `FACE_AXES`.
fn die_mesh(h: f32) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for (face, axis) in FACE_AXES {
        let n = Vec3::from(axis);
        // Tangents with u × v = n, so the quad winds outwards.
        let (u, v) = match axis {
            [0.0, 1.0, 0.0] => (Vec3::X, Vec3::NEG_Z),
            [0.0, -1.0, 0.0] => (Vec3::X, Vec3::Z),
            [1.0, 0.0, 0.0] => (Vec3::NEG_Z, Vec3::Y),
            [-1.0, 0.0, 0.0] => (Vec3::Z, Vec3::Y),
            [0.0, 0.0, 1.0] => (Vec3::X, Vec3::Y),
            _ => (Vec3::NEG_X, Vec3::Y),
        };
        let cell = atlas_cell(face) as f32;
        let (x0, x1) = (cell / 6.0, (cell + 1.0) / 6.0);
        let base = positions.len() as u16;
        for (du, dv, uv) in [
            (-1.0, -1.0, [x0, 1.0]),
            (1.0, -1.0, [x1, 1.0]),
            (1.0, 1.0, [x1, 0.0]),
            (-1.0, 1.0, [x0, 0.0]),
        ] {
            positions.push(((n + u * du + v * dv) * h).to_array());
            normals.push(n.to_array());
            uvs.push(uv);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U16(indices))
}

fn atlas_cell(face: Face) -> usize {
    match face {
        Face::Strike => 0,
        Face::Shield => 1,
        Face::Sun => 2,
        Face::Moon => 3,
        Face::Element => 4,
        Face::Blank => 5,
    }
}

const GLYPH: usize = 12;

/// Placeholder 12×12 symbols, in atlas order. `#` ink, `o` accent.
const GLYPHS: [[&str; GLYPH]; 6] = [
    [
        "............",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        "...######...",
        ".....##.....",
        ".....##.....",
        "............",
        "............",
    ],
    [
        "............",
        "..########..",
        "..#oooooo#..",
        "..#oooooo#..",
        "..#oooooo#..",
        "..#oooooo#..",
        "...#oooo#...",
        "...#oooo#...",
        "....#oo#....",
        ".....##.....",
        "............",
        "............",
    ],
    [
        "............",
        ".o...o...o..",
        "..o.....o...",
        "....ooo.....",
        "...ooooo....",
        "o..ooooo..o.",
        "...ooooo....",
        "....ooo.....",
        "..o.....o...",
        ".o...o...o..",
        "............",
        "............",
    ],
    [
        "............",
        "....####....",
        "...###......",
        "..###.......",
        "..##........",
        "..##........",
        "..###.......",
        "...###......",
        "....####....",
        "............",
        "............",
        "............",
    ],
    [
        "............",
        ".....oo.....",
        "....o..o....",
        "...o.oo.o...",
        "..o.o..o.o..",
        "..o.o.oo.o..",
        "..o..o...o..",
        "...o....o...",
        "....oooo....",
        "............",
        "............",
        "............",
    ],
    [
        "............",
        "............",
        "............",
        "............",
        "............",
        ".....##.....",
        ".....##.....",
        "............",
        "............",
        "............",
        "............",
        "............",
    ],
];

fn face_atlas() -> Image {
    let (w, h) = ((GLYPH * 6) as u32, GLYPH as u32);
    let mut image = Image::new_fill(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        // Bone-white die body.
        &[232, 222, 200, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    for (cell, glyph) in GLYPHS.iter().enumerate() {
        for (y, row) in glyph.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                let px = match c {
                    b'#' => [28, 22, 30, 255],
                    b'o' => [200, 120, 30, 255],
                    _ => continue,
                };
                let i = (y * w as usize + cell * GLYPH + x) * 4;
                data[i..i + 4].copy_from_slice(&px);
            }
        }
    }
    image
}
