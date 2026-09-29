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
        app.add_systems(Startup, make_pennants)
            .add_systems(crate::InGame, (sync_undead, sync_militia));
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

/// Where each settlement's militia stand, a figure a man, as the board
/// shows them (`Match::shown_militia`), with a pennant over them in the
/// colour of what they think of the human.
#[allow(clippy::too_many_arguments)]
fn sync_militia(
    mut commands: Commands,
    time: Res<Time>,
    game: Res<Match>,
    board: Res<Board>,
    art: Res<StatArt>,
    pennants: Res<Pennants>,
    images: Res<Assets<Image>>,
    mut men: Query<(Entity, &MilitiaMan, &mut Transform), Without<Pennant>>,
    mut flags: Query<(Entity, &Pennant, &mut Transform), Without<MilitiaMan>>,
) {
    let g = &game.game;
    // (home, where they stand, men) for every militia standing somewhere.
    let standing: Vec<(Hex, Hex, u8)> = g
        .militias()
        .filter_map(|(home, _)| {
            let m = game.shown_militia(home)?;
            m.at.filter(|_| m.men > 0).map(|at| (home, at, m.men))
        })
        .collect();
    let step = WALK * time.delta_secs();

    // The men: walk to where their militia stands; one figure a man.
    for (entity, man, mut transform) in &mut men {
        let Some(&(_, at, count)) = standing.iter().find(|(h, ..)| *h == man.hex) else {
            commands.entity(entity).despawn();
            continue;
        };
        if man.post >= count as usize {
            commands.entity(entity).despawn();
            continue;
        }
        let goal = board.hex_to_world(at) + POSTS[man.post];
        let delta = goal - transform.translation;
        transform.translation += delta.clamp_length_max(step);
    }
    for &(home, at, count) in &standing {
        for (post, offset) in POSTS.iter().enumerate().take(count as usize) {
            if men.iter().any(|(_, m, _)| m.hex == home && m.post == post) {
                continue;
            }
            // A different man at each post, the same on every client.
            let pick =
                (crate::props::hex_seed(home, 31 + post as i32) as usize) % art.militia.len();
            let image = art.militia[pick].clone();
            if !images.contains(&image) {
                continue;
            }
            let pos = board.hex_to_world(at) + *offset;
            commands.spawn((MilitiaMan { hex: home, post }, figure(image, pos)));
        }
    }

    // The pennant: green for friends, gold when they let the human through,
    // red when the human would have to fight them.
    let standing_with = g.standing(game.human);
    let look = if standing_with >= necromy_rules::FRIENDLY {
        0
    } else if standing_with >= necromy_rules::MILITIA_PASS {
        1
    } else {
        2
    };
    for (entity, flag, mut transform) in &mut flags {
        let Some(&(_, at, _)) = standing.iter().find(|(h, ..)| *h == flag.home) else {
            commands.entity(entity).despawn();
            continue;
        };
        if flag.look != look {
            commands.entity(entity).despawn();
            continue;
        }
        let goal = board.hex_to_world(at) + PENNANT;
        let delta = goal - transform.translation;
        transform.translation += delta.clamp_length_max(step);
    }
    for &(home, at, _) in &standing {
        if flags
            .iter()
            .any(|(_, f, _)| f.home == home && f.look == look)
        {
            continue;
        }
        commands.spawn((
            Pennant { home, look },
            Billboard,
            NotShadowCaster,
            NotShadowReceiver,
            Sprite::from_image(pennants.0[look].clone()),
            Sprite3d {
                pixels_per_metre: TEXELS,
                pivot: Some(Vec2::new(0.5, 0.0)),
                alpha_mode: AlphaMode::Mask(0.5),
                // Read at any hour, like the other markers.
                unlit: true,
                ..default()
            },
            Transform::from_translation(board.hex_to_world(at) + PENNANT),
        ));
    }
}

/// Pennant pictures: friends, let through, against (`look`).
#[derive(Resource)]
struct Pennants([Handle<Image>; 3]);

#[derive(Component)]
struct Pennant {
    home: Hex,
    look: usize,
}

/// Over the militia's heads.
const PENNANT: Vec3 = Vec3::new(0.0, 1.1, 0.45);

fn make_pennants(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    const ROWS: [&str; 15] = [
        "#############",
        "#fFFFFFFFFFf#",
        "#fFfffffffFf#",
        "#fffffffffff#",
        "#fffffffffff#",
        "#fffffffffff#",
        "#fffffffffff#",
        "#fffffffffff#",
        ".#fffffffff#.",
        "..#fffffff#..",
        "...#fffff#...",
        "....#fff#....",
        ".....#f#.....",
        "......#......",
        "......#......",
    ];
    let colours = [[96, 196, 84], [232, 196, 84], [214, 64, 52]];
    let pennants = colours.map(|c| {
        let light = c.map(|v: u8| v.saturating_add(50));
        let (w, h) = (ROWS[0].len() as u32, ROWS.len() as u32);
        let mut image = Image::new_fill(
            bevy::render::render_resource::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            &[0, 0, 0, 0],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::RENDER_WORLD,
        );
        let data = image.data.as_mut().expect("new_fill allocates pixel data");
        for (y, row) in ROWS.iter().enumerate() {
            for (x, ch) in row.bytes().enumerate() {
                let px = match ch {
                    b'#' => [20, 16, 24, 255],
                    b'f' => [c[0], c[1], c[2], 255],
                    b'F' => [light[0], light[1], light[2], 255],
                    _ => continue,
                };
                let i = (y * w as usize + x) * 4;
                data[i..i + 4].copy_from_slice(&px);
            }
        }
        images.add(image)
    });
    commands.insert_resource(Pennants(pennants));
}
