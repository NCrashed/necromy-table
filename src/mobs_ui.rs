//! Mobs on the board (docs/design.md §20.4): the undead as billboards that
//! walk to their hex, and each settlement's militia as figures by its gate,
//! one for every man it still has. Pictures from PixelLab
//! (`assets/sprites/undead-N.png`, `militia-N.png`), trimmed to the figure
//! so they stand on their feet.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy_sprite3d::prelude::*;
use necromy_rules::Hex;

use crate::board::{Board, TEXELS};
use crate::play::Match;
use crate::stats::StatArt;
use crate::token::Billboard;

/// How fast the dead walk to their new hex, in metres per second.
const WALK: f32 = 2.5;
/// Where a settlement's men stand, from its centre: by its near edge.
const POSTS: [Vec3; 2] = [Vec3::new(-0.5, 0.0, 0.45), Vec3::new(0.5, 0.0, 0.45)];

pub struct MobsUiPlugin;

impl Plugin for MobsUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(crate::InGame, (sync_undead, sync_militia));
    }
}

#[derive(Component)]
struct UndeadToken(u32);

#[derive(Component)]
struct MilitiaMan {
    hex: Hex,
    post: usize,
}

fn figure(image: Handle<Image>, at: Vec3) -> impl Bundle {
    (
        Billboard,
        NotShadowCaster,
        NotShadowReceiver,
        Sprite::from_image(image),
        Sprite3d {
            pixels_per_metre: TEXELS,
            // Trimmed to the figure: its feet are its bottom row.
            pivot: Some(Vec2::new(0.5, 0.0)),
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: false,
            ..default()
        },
        Transform::from_translation(at),
    )
}

/// One token per undead the board shows, walking to where it stands.
fn sync_undead(
    mut commands: Commands,
    time: Res<Time>,
    game: Res<Match>,
    board: Res<Board>,
    art: Res<StatArt>,
    images: Res<Assets<Image>>,
    mut tokens: Query<(Entity, &UndeadToken, &mut Transform)>,
) {
    let shown = game.shown_undead();
    for (entity, token, mut transform) in &mut tokens {
        let Some(u) = shown.iter().find(|u| u.id == token.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let goal = board.hex_to_world(u.hex);
        let delta = goal - transform.translation;
        transform.translation += delta.clamp_length_max(WALK * time.delta_secs());
    }
    for u in &shown {
        if tokens.iter().any(|(_, t, _)| t.0 == u.id) {
            continue;
        }
        let image = art.undead[u.id as usize % art.undead.len()].clone();
        // `Sprite3d` reads the size on spawn: wait for the picture.
        if !images.contains(&image) {
            continue;
        }
        commands.spawn((UndeadToken(u.id), figure(image, board.hex_to_world(u.hex))));
    }
}

/// A figure by each settlement for every man its militia still has.
fn sync_militia(
    mut commands: Commands,
    game: Res<Match>,
    board: Res<Board>,
    art: Res<StatArt>,
    images: Res<Assets<Image>>,
    men: Query<(Entity, &MilitiaMan)>,
) {
    let g = &game.game;
    let wanted: Vec<(Hex, usize)> = g
        .board()
        .tiles()
        .filter_map(|(hex, _)| g.militia(hex).map(|m| (hex, m)))
        .flat_map(|(hex, m)| (0..(m as usize).min(POSTS.len())).map(move |post| (hex, post)))
        .collect();
    for (entity, man) in &men {
        if !wanted.contains(&(man.hex, man.post)) {
            commands.entity(entity).despawn();
        }
    }
    for (hex, post) in wanted {
        if men.iter().any(|(_, m)| m.hex == hex && m.post == post) {
            continue;
        }
        // A different man at each post, the same on every client.
        let pick = (crate::props::hex_seed(hex, 31 + post as i32) as usize) % art.militia.len();
        let image = art.militia[pick].clone();
        if !images.contains(&image) {
            continue;
        }
        let at = board.hex_to_world(hex) + POSTS[post];
        commands.spawn((MilitiaMan { hex, post }, figure(image, at)));
    }
}
