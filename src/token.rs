//! Champion tokens: pixel-art billboards standing on hexes.
//!
//! Sprites are procedural placeholders until PixelLab sheets land in
//! assets/sprites/; swap `placeholder_sprite` for `asset_server.load(...)`
//! plus a `TextureAtlasLayout` and animate with bevy_spritesheet_animation.

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
        app.add_systems(Startup, spawn_tokens).add_systems(
            Update,
            (queue_steps, move_tokens, sync_guard, face_camera).chain(),
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

fn spawn_tokens(
    mut commands: Commands,
    board: Res<Board>,
    game: Res<Match>,
    mut images: ResMut<Assets<Image>>,
) {
    for player in game.game.players() {
        let champion = game.game.champion(player).expect("seat exists");
        commands.spawn((
            Token {
                player,
                waypoints: VecDeque::new(),
            },
            Billboard,
            // A camera-facing plane casts a paper-thin, swinging shadow; leave shadows
            // to a blob decal or a baked shadow row in the sprite sheet.
            NotShadowCaster,
            NotShadowReceiver,
            Sprite::from_image(images.add(placeholder_sprite(champion.god.accent()))),
            Sprite3d {
                pixels_per_metre: PIXELS_PER_METRE,
                // Feet on the tile, not the sprite centre.
                pivot: Some(Vec2::new(0.5, 0.0)),
                alpha_mode: AlphaMode::Mask(0.5),
                unlit: true,
                ..default()
            },
            Transform::from_translation(board.hex_to_world(champion.hex)),
        ));
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

fn move_tokens(time: Res<Time>, mut tokens: Query<(&mut Token, &mut Transform)>) {
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
