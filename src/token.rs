//! Champion tokens: pixel-art billboards standing on hexes.
//!
//! A champion with a PixelLab sheet in `assets/sprites/` walks and breathes
//! in four directions (`animate_tokens`); one without keeps a procedural
//! placeholder figure.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_sprite3d::prelude::*;
use necromy_rules::PlayerId;

use crate::board::Board;
use crate::play::Match;

const SPRITE_W: u32 = 16;
const SPRITE_H: u32 = 24;
const PIXELS_PER_METRE: f32 = 16.0;
const MOVE_SPEED: f32 = 6.0;

pub struct TokenPlugin;

impl Plugin for TokenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sheets)
            .add_systems(
                crate::InGame,
                (
                    spawn_tokens,
                    queue_steps,
                    move_tokens,
                    sync_guard,
                    animate_tokens,
                )
                    .chain(),
            )
            // Billboards face the camera where it is this frame.
            .add_systems(crate::InGame, face_camera.after(crate::camera::apply))
            .add_systems(
                crate::InGame,
                shade_tokens.run_if(resource_changed::<Match>),
            );
    }
}

#[derive(Component)]
pub struct Token {
    pub player: PlayerId,
    /// Hexes still to walk through, so multi-step moves follow the path.
    waypoints: VecDeque<Vec3>,
}

impl Token {
    pub fn is_walking(&self) -> bool {
        !self.waypoints.is_empty()
    }
}

/// Kingdom steel, darker than any patron's colour.
pub const GUARD_COLOR: [u8; 3] = [120, 132, 150];

/// The royal guard's token; follows `Game::guard`.
#[derive(Component)]
pub struct GuardToken;

/// Keeps a sprite plane turned towards the camera.
#[derive(Component)]
pub struct Billboard;

/// Champion sheets in `assets/sprites/<god>-champion.png` (PixelLab): 96×96
/// frames; rows are idle south, east, north, west, then walk in the same
/// order. A god without a sheet keeps the placeholder figure.
const SHEET_FRAME: u32 = 96;
const IDLE_FRAMES: u32 = 4;
const WALK_FRAMES: u32 = 6;
const SHEET_COLUMNS: u32 = if IDLE_FRAMES > WALK_FRAMES {
    IDLE_FRAMES
} else {
    WALK_FRAMES
};
/// A champion on the table: about 1.6 m, the drawn figure ~58 px tall.
const SHEET_PIXELS_PER_METRE: f32 = 36.0;
/// Empty rows under the feet in a frame.
const SHEET_FEET_PX: f32 = 17.0;
const FRAME_SECS: f32 = 0.14;

#[derive(Resource)]
struct ChampionSheets {
    /// Per god (`God::index`), when its sheet exists.
    sheets: [Option<Handle<Image>>; 5],
    layout: Handle<TextureAtlasLayout>,
}

fn load_sheets(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let sheets = necromy_rules::God::ALL.map(|god| {
        let path = format!("sprites/{}-champion.png", god.name().to_lowercase());
        std::path::Path::new("assets")
            .join(&path)
            .exists()
            .then(|| assets.load(path))
    });
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(SHEET_FRAME),
        SHEET_COLUMNS,
        8,
        None,
        None,
    ));
    commands.insert_resource(ChampionSheets { sheets, layout });
}

/// Which way a token looks, as the camera sees it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Facing {
    /// Towards the viewer.
    #[default]
    South,
    East,
    North,
    West,
}

impl Facing {
    fn row(self) -> u32 {
        match self {
            Facing::South => 0,
            Facing::East => 1,
            Facing::North => 2,
            Facing::West => 3,
        }
    }
}

/// A token drawn from a sheet: its frame clock and where it looks.
#[derive(Component, Default)]
struct Animated {
    facing: Facing,
    clock: f32,
}

/// Tokens appear once the champion sheets are loaded: `Sprite3d` reads the
/// image size when spawned.
fn spawn_tokens(
    mut commands: Commands,
    board: Res<Board>,
    game: Res<Match>,
    sheets: Res<ChampionSheets>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let loaded = sheets
        .sheets
        .iter()
        .flatten()
        .all(|h| assets.is_loaded_with_dependencies(h));
    if !loaded {
        return;
    }
    *done = true;
    for player in game.game.players() {
        let champion = game.game.champion(player).expect("seat exists");
        let at = board.hex_to_world(champion.hex);
        let token = Token {
            player,
            waypoints: VecDeque::new(),
        };
        // A camera-facing plane casts a paper-thin, swinging shadow; leave
        // shadows to a blob decal or a baked shadow row in the sheet.
        let common = (token, Billboard, NotShadowCaster, NotShadowReceiver);
        // `bevy_sprite3d` ignores `Sprite::color` (its materials are cached
        // per image), so the shaded look is a darker copy of the image.
        let (normal, sprite3d, atlas) = match &sheets.sheets[champion.god.index()] {
            Some(sheet) => (
                sheet.clone(),
                Sprite3d {
                    pixels_per_metre: SHEET_PIXELS_PER_METRE,
                    pivot: Some(Vec2::new(0.5, SHEET_FEET_PX / SHEET_FRAME as f32)),
                    alpha_mode: AlphaMode::Mask(0.5),
                    unlit: true,
                    ..default()
                },
                Some(TextureAtlas {
                    layout: sheets.layout.clone(),
                    index: 0,
                }),
            ),
            None => (
                images.add(placeholder_sprite(champion.god.accent())),
                Sprite3d {
                    pixels_per_metre: PIXELS_PER_METRE,
                    // Feet on the tile, not the sprite centre.
                    pivot: Some(Vec2::new(0.5, 0.0)),
                    alpha_mode: AlphaMode::Mask(0.5),
                    unlit: true,
                    ..default()
                },
                None,
            ),
        };
        let shaded = images
            .get(&normal)
            .map(shade)
            .map_or_else(|| normal.clone(), |img| images.add(img));
        let mut entity = commands.spawn((
            common,
            Looks {
                normal: normal.clone(),
                shaded,
            },
            sprite3d,
            Transform::from_translation(at),
        ));
        match atlas {
            Some(atlas) => {
                entity.insert((Animated::default(), Sprite::from_atlas_image(normal, atlas)));
            }
            None => {
                entity.insert(Sprite::from_image(normal));
            }
        }
    }
}

/// A token's two looks: in plain sight, and shaded while its own player
/// hides (§11.6).
#[derive(Component)]
struct Looks {
    normal: Handle<Image>,
    shaded: Handle<Image>,
}

/// A darker, bluer copy: the champion in shadow.
fn shade(image: &Image) -> Image {
    let mut out = image.clone();
    if let Some(data) = out.data.as_mut() {
        for px in data.as_chunks_mut::<4>().0 {
            px[0] = (px[0] as f32 * 0.45) as u8;
            px[1] = (px[1] as f32 * 0.5) as u8;
            px[2] = (px[2] as f32 * 0.75) as u8;
        }
    }
    out
}

/// Walk while there are waypoints, breathe otherwise; face the way of the
/// walk as the camera sees it, so a turned table turns the champion too.
fn animate_tokens(
    time: Res<Time>,
    camera: Single<&Transform, (With<crate::TableCamera>, Without<Token>)>,
    mut tokens: Query<(&Token, &Transform, &mut Animated, &mut Sprite)>,
) {
    let right = camera.right().with_y(0.0).normalize_or_zero();
    let away = camera.forward().with_y(0.0).normalize_or_zero();
    for (token, transform, mut anim, mut sprite) in &mut tokens {
        let walking = token.is_walking();
        if let Some(target) = token.waypoints.front() {
            let way = (*target - transform.translation).with_y(0.0);
            if way.length_squared() > 1e-4 {
                let (x, y) = (way.dot(right), way.dot(away));
                anim.facing = if x.abs() > y.abs() {
                    if x > 0.0 { Facing::East } else { Facing::West }
                } else if y > 0.0 {
                    Facing::North
                } else {
                    Facing::South
                };
            }
        }
        anim.clock += time.delta_secs();
        let (first_row, frames) = if walking {
            (4, WALK_FRAMES)
        } else {
            (0, IDLE_FRAMES)
        };
        // Idle breathes slower than a step.
        let pace = if walking {
            FRAME_SECS
        } else {
            FRAME_SECS * 2.0
        };
        let frame = (anim.clock / pace) as u32 % frames;
        let index = ((first_row + anim.facing.row()) * SHEET_COLUMNS + frame) as usize;
        if let Some(atlas) = sprite.texture_atlas.as_mut()
            && atlas.index != index
        {
            atlas.index = index;
        }
    }
}

/// A chunky hooded figure: dark outline, body in the patron's colour.
pub fn placeholder_sprite(color: [u8; 3]) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: SPRITE_W,
            height: SPRITE_H,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let data = image.data.as_mut().expect("new_fill allocates pixel data");

    let inside = |x: i32, y: i32| -> bool {
        let cx = SPRITE_W as i32 / 2;
        let dx = (x - cx).abs();
        match y {
            2..=8 => (x - cx) * (x - cx) + (y - 5) * (y - 5) <= 10, // head
            9..=21 => dx <= 2 + (y - 9) / 3,                        // robe widening down
            _ => false,
        }
    };

    for y in 0..SPRITE_H as i32 {
        for x in 0..SPRITE_W as i32 {
            let px = if inside(x, y) {
                let shade = if x < SPRITE_W as i32 / 2 { 1.0 } else { 0.75 };
                let [r, g, b] = color.map(|c| (c as f32 * shade) as u8);
                [r, g, b, 255]
            } else if [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|(dx, dy)| inside(x + dx, y + dy))
            {
                [20, 16, 24, 255]
            } else {
                continue;
            };
            let i = ((y as u32 * SPRITE_W + x as u32) * 4) as usize;
            data[i..i + 4].copy_from_slice(&px);
        }
    }
    image
}

/// Turns the steps the rules accepted into waypoints for the matching token.
fn queue_steps(mut game: ResMut<Match>, board: Res<Board>, mut tokens: Query<&mut Token>) {
    // Read first: taking the steps marks `Match` changed and triggers a redraw.
    if game.steps.is_empty() {
        return;
    }
    for (player, hex) in game.steps.drain(..) {
        if let Some(mut token) = tokens.iter_mut().find(|t| t.player == player) {
            token.waypoints.push_back(board.hex_to_world(hex));
        }
    }
}

pub fn move_tokens(time: Res<Time>, mut tokens: Query<(&mut Token, &mut Transform)>) {
    for (mut token, mut transform) in &mut tokens {
        let mut budget = MOVE_SPEED * time.delta_secs();
        while let Some(&target) = token.waypoints.front() {
            let delta = target - transform.translation;
            let dist = delta.length();
            if dist > budget {
                transform.translation += delta / dist * budget;
                break;
            }
            transform.translation = target;
            budget -= dist;
            token.waypoints.pop_front();
        }
    }
}

fn face_camera(
    camera: Single<&Transform, (With<crate::TableCamera>, Without<Billboard>)>,
    mut sprites: Query<&mut Transform, With<Billboard>>,
) {
    for mut transform in &mut sprites {
        transform.rotation = camera.rotation;
    }
}

/// Spawns, walks and removes the guard token to match the rules.
fn sync_guard(
    mut commands: Commands,
    time: Res<Time>,
    game: Res<Match>,
    board: Res<Board>,
    mut images: ResMut<Assets<Image>>,
    mut guards: Query<(Entity, &mut Transform), With<GuardToken>>,
) {
    match (game.game.guard(), guards.single_mut()) {
        (Some(guard), Ok((_, mut transform))) => {
            let target = board.hex_to_world(guard.hex);
            let delta = target - transform.translation;
            transform.translation += delta.clamp_length_max(MOVE_SPEED * 0.5 * time.delta_secs());
        }
        (Some(guard), Err(_)) => {
            commands.spawn((
                GuardToken,
                Billboard,
                NotShadowCaster,
                NotShadowReceiver,
                Sprite::from_image(images.add(placeholder_sprite(GUARD_COLOR))),
                Sprite3d {
                    pixels_per_metre: PIXELS_PER_METRE,
                    pivot: Some(Vec2::new(0.5, 0.0)),
                    alpha_mode: AlphaMode::Mask(0.5),
                    unlit: true,
                    ..default()
                },
                Transform::from_translation(board.hex_to_world(guard.hex)),
            ));
        }
        (None, Ok((entity, _))) => commands.entity(entity).despawn(),
        (None, Err(_)) => {}
    }
}

/// Stealth on the board (§11.6): a hidden rival's token is gone (a trail
/// marks where they were seen, `board.rs`); the human's own, when hidden,
/// stands in shadow.
fn shade_tokens(
    game: Res<Match>,
    mut tokens: Query<(&Token, &Looks, &mut Sprite, &mut Visibility)>,
) {
    for (token, looks, mut sprite, mut visibility) in &mut tokens {
        let hidden = game.game.is_hidden(token.player);
        let own = token.player == game.human;
        visibility.set_if_neq(if hidden && !own {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        let look = if hidden && own {
            &looks.shaded
        } else {
            &looks.normal
        };
        if sprite.image != *look {
            sprite.image = look.clone();
        }
    }
}
