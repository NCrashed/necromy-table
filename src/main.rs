mod board;
mod hud;
mod names;
mod play;
mod token;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy_sprite3d::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                // Pixel art: nearest-neighbour sampling, no blur.
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Necromy Table".into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(Sprite3dPlugin)
        // PlayPlugin first: it inserts the `Match` the others read at startup.
        .add_plugins((
            play::PlayPlugin,
            board::BoardPlugin,
            token::TokenPlugin,
            hud::HudPlugin,
        ))
        .add_plugins(AutoScreenshotPlugin)
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(mut commands: Commands) {
    // Fixed tilted view over the board, like Armello's table camera.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 18.0, 15.5).looking_at(Vec3::new(0.0, 0.0, 2.6), Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Dev aid: `NECROMY_SCREENSHOT=out.png` saves one frame after a few seconds
/// and quits, so a change can be checked without clicking through the game.
struct AutoScreenshotPlugin;

impl Plugin for AutoScreenshotPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(path) = std::env::var("NECROMY_SCREENSHOT") {
            let after = std::env::var("NECROMY_SCREENSHOT_AFTER")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4.0);
            app.insert_resource(AutoScreenshot {
                path,
                after,
                taken_at: None,
            })
            .add_systems(Update, auto_screenshot);
        }
    }
}

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    /// Seconds to wait (`NECROMY_SCREENSHOT_AFTER`, default 4).
    after: f32,
    taken_at: Option<f32>,
}

/// Time for the capture to reach the disk before quitting.
const SCREENSHOT_SAVE_SECS: f32 = 1.0;

fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut shot: ResMut<AutoScreenshot>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    match shot.taken_at {
        None if now >= shot.after => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(shot.path.clone()));
            shot.taken_at = Some(now);
        }
        Some(at) if now - at >= SCREENSHOT_SAVE_SECS => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}
