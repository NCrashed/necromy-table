mod ambient;
mod audio;
mod battle_ui;
mod board;
mod camera;
mod card_art;
mod deck;
mod dice;
mod effects;
mod feed;
mod fight;
mod god_pick;
mod gods_ui;
mod gpu;
mod hud;
mod icon;
mod icons;
mod lighting;
mod lobby;
mod names;
mod play;
mod props;
mod ring_ui;
mod settings;
mod stats;
mod story_ui;
mod token;
mod trial_ui;
mod turn_ui;
mod tutorial;
mod ui_skin;
mod victory_ui;
mod watch_ui;
mod wish_ui;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy_sprite3d::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                // Pixel art: nearest-neighbour sampling, no blur.
                .set(ImagePlugin::default_nearest())
                // Which GPU backend and DX12 shader compiler; see `gpu.rs`.
                .set(gpu::render_plugin())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Necromy Table".into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((Sprite3dPlugin, icon::IconPlugin))
        .init_schedule(InGame)
        .init_schedule(MatchBegins)
        .add_systems(Update, run_game)
        .add_plugins((
            lobby::LobbyPlugin,
            god_pick::GodPickPlugin,
            camera::CameraPlugin,
            play::PlayPlugin,
            board::BoardPlugin,
            props::PropsPlugin,
            ambient::AmbientPlugin,
            effects::EffectsPlugin,
            lighting::LightingPlugin,
            token::TokenPlugin,
            hud::HudPlugin,
            card_art::CardArtPlugin,
            ui_skin::UiSkinPlugin,
            dice::DicePlugin,
            deck::DeckPlugin,
        ))
        // The screens: more plugins than one tuple holds.
        .add_plugins((
            stats::StatsPlugin,
            battle_ui::BattleUiPlugin,
            trial_ui::TrialUiPlugin,
            watch_ui::WatchUiPlugin,
            fight::FightPlugin,
            turn_ui::TurnUiPlugin,
            victory_ui::VictoryUiPlugin,
            wish_ui::WishUiPlugin,
            story_ui::StoryUiPlugin,
            gods_ui::GodsUiPlugin,
            feed::FeedPlugin,
            tutorial::TutorialPlugin,
            ring_ui::RingUiPlugin,
            audio::SoundPlugin,
        ))
        .add_plugins(settings::SettingsPlugin)
        .add_plugins(AutoScreenshotPlugin)
        .add_systems(Startup, setup_scene)
        .run();
}

/// Where the game keeps what outlives a run (the ticket, tutorial progress):
/// `$XDG_STATE_HOME/necromy-table`, `~/.local/state/necromy-table`, or on
/// Windows (no `HOME`) `%LOCALAPPDATA%\necromy-table`.
pub fn state_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/state")))
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from))?;
    Some(base.join("necromy-table"))
}

fn setup_scene(mut commands: Commands) {
    // Fixed tilted view over the board, like Armello's table camera.
    commands.spawn((
        TableCamera,
        Camera3d::default(),
        // Orthographic and pixel-true; `camera.rs` sets the scale.
        Projection::Orthographic(OrthographicProjection {
            far: 200.0,
            ..OrthographicProjection::default_3d()
        }),
        // The table's own sky light; `lighting.rs` turns it to night.
        AmbientLight::default(),
        Transform::from_xyz(0.0, 18.0, 15.5).looking_at(Vec3::new(0.0, 0.0, 2.6), Vec3::Y),
    ));

    commands.spawn((
        lighting::Sun,
        DirectionalLight {
            illuminance: 2_600.0,
            // Nothing on the board casts a shadow: billboards would cast
            // paper-thin ones.
            shadow_maps_enabled: false,
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
                ready_at: None,
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
    /// When the awaited moment came; the capture waits a little after it.
    ready_at: Option<f32>,
    taken_at: Option<f32>,
}

/// The screen catches up with a change a frame or two after it arrives.
const SCREENSHOT_SETTLE_SECS: f32 = 0.3;

/// Time for the capture to reach the disk before quitting.
const SCREENSHOT_SAVE_SECS: f32 = 1.0;

#[allow(clippy::too_many_arguments)]
fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    dice: Res<dice::DiceShow>,
    game: Option<Res<play::Match>>,
    front: Res<lobby::Front>,
    day_night: Res<lighting::DayNight>,
    wounds: Res<fight::Wounds>,
    mut shot: ResMut<AutoScreenshot>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    // `NECROMY_SCREENSHOT_WHEN=dice` waits for the first settled throw,
    // `=guard` for the royal guard on the board with no dice in the air,
    // `=lobby` for the lobby with everyone `NECROMY_START_AT` expects.
    let when = std::env::var("NECROMY_SCREENSHOT_WHEN").ok();
    let ready = match (when.as_deref(), game.as_deref()) {
        (Some("lobby"), _) => front.lobby_full(),
        // The menu: only a plain timed capture.
        (None, None) => now >= shot.after,
        (Some(_), None) => false,
        (Some("dice"), Some(_)) => dice.ever_settled,
        // The first blow that got through on the battle stage, and a death.
        (Some("blow"), Some(_)) => wounds.taken.iter().any(|&t| t > 0),
        (Some("death"), Some(_)) => wounds.dead.iter().any(|&d| d),
        (Some("guard"), Some(game)) => game.game.guard().is_some() && !dice.busy(),
        (Some("hit"), Some(game)) => game.incoming_result.is_some(),
        (Some("myturn"), Some(game)) => game.is_human_turn(),
        (Some("victory"), Some(game)) => game.game.winner().is_some() && !dice.busy(),
        (Some("reply"), Some(game)) => game.wish_reply.is_some() && !dice.busy(),
        // The human's own words, judged by the model (`NECROMY_WISH`).
        (Some("heard"), Some(game)) => game.wish_reply.as_ref().is_some_and(|r| r.said.is_some()),
        (Some("told"), Some(game)) => {
            game.told.is_some() && game.wish_reply.is_none() && !dice.busy()
        }
        (Some("wishpanel"), Some(game)) => game.game.wish_due() == Some(game.human),
        // Another seat's wish is being written where the human can see it.
        (Some("drafting"), Some(game)) => game
            .drafting
            .as_ref()
            .is_some_and(|(_, _, text)| text.chars().count() >= 12),
        // Full night: the lights of the world are on.
        (Some("night"), Some(_)) => day_night.night >= 1.0,
        // Someone slipped out of sight (§11.6).
        // A god shifted at dusk: its scene is up (§5).
        // The view has no log: count the shifts the feed told.
        (Some("dusk"), Some(game)) => game.stage_shifts > 0,
        // The human's action waits for a rival still acting (§11.2).
        (Some("held"), Some(game)) => {
            matches!(
                game.game.phase(game.human),
                necromy_rules::Phase::Held { .. }
            )
        }
        // The human has a trap on the board.
        (Some("trap"), Some(game)) => game.game.traps().iter().any(|t| t.owner == game.human),
        (Some("hidden"), Some(game)) => game.game.players().any(|p| game.game.is_hidden(p)),
        // The human carries poison and nothing covers their token: bubbles
        // rise over it.
        (Some("bubbles"), Some(game)) => {
            game.game
                .champion(game.human)
                .is_some_and(|c| c.poison.is_some())
                && game.battle.is_none()
                && game.incoming_result.is_none()
                && game.told.is_none()
                && !dice.busy()
        }
        // Someone else's battle or trial waits behind its icon.
        (Some("watch"), Some(game)) => {
            game.on_screen.is_none()
                && game.told.is_none()
                && game.shows.iter().any(|s| !s.who.contains(&game.human))
        }
        // A show of others on the panels by choice, its dice at rest
        // (with `NECROMY_WATCH=click`).
        (Some("watching"), Some(game)) => game.watching && dice.settled(),
        // Trials on the board, nothing over it: their stones and faces.
        (Some("trials"), Some(game)) => {
            !game.game.trials().is_empty()
                && game.trial.is_none()
                && game.battle.is_none()
                && game.told.is_none()
                && !dice.busy()
        }
        // A trial's result on its panel, the dice down (§20.2).
        (Some("trial"), Some(game)) => {
            game.trial.as_ref().is_some_and(|t| t.result.is_some()) && dice.settled()
        }
        // The human is poisoned: the drop on their sheet.
        (Some("poison"), Some(game)) => game
            .game
            .champion(game.human)
            .is_some_and(|c| c.poison.is_some()),
        (Some("incoming"), Some(game)) => matches!(
            game.human_window(),
            Some(necromy_rules::WindowKind::Target { target, .. }) if target == game.human
        ),
        _ => now >= shot.after,
    };
    if ready && shot.ready_at.is_none() {
        shot.ready_at = Some(now);
    }
    // `NECROMY_SCREENSHOT_SETTLE=S` waits longer after the moment (the end of
    // a death animation, say).
    let settle = std::env::var("NECROMY_SCREENSHOT_SETTLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(SCREENSHOT_SETTLE_SECS);
    let settled = shot.ready_at.is_some_and(|at| now - at >= settle);
    match shot.taken_at {
        None if settled => {
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

/// The camera over the board. Other cameras (the dice trays) render into
/// textures; anything that picks, hovers or faces "the camera" means this one.
#[derive(Component)]
pub struct TableCamera;

/// Systems of the match itself. They run only while there is a match: before
/// it, the menu and the lobby own the screen (`lobby.rs`).
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct InGame;

/// Runs once, when the match arrives: whatever is built from its board and
/// its seats (tiles, tokens, portraits).
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MatchBegins;

fn run_game(world: &mut World, mut begun: Local<bool>) {
    if !world.contains_resource::<play::Match>() {
        return;
    }
    if !*begun {
        *begun = true;
        world.run_schedule(MatchBegins);
    }
    world.run_schedule(InGame);
}
