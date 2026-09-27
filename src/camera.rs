//! The table camera, as in Armello: the table turned with the human's side
//! towards them, close on their champion, following them.
//!
//! Wheel zooms between a close look and the whole board; right or middle
//! drag and WASD/arrows pan; Q/E turn the table by a hex side; F goes back
//! to the champion and follows again. The rig holds where the camera
//! should be; the camera eases towards it every frame.
//!
//! Dev aid: `NECROMY_CAMERA=overview` starts over the whole board, as the
//! old fixed camera did (handy for screenshots).

use std::f32::consts::{FRAC_PI_3, TAU};

use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::board::{Board, HEX_SIZE};
use crate::play::{Match, Selection};
use crate::token::Token;

/// Downward tilt of the view.
const PITCH: f32 = 0.95;
const NEAR: f32 = 6.0;
const FAR: f32 = 25.0;
/// Where a match starts: the champion and a ring of hexes around them.
const START: f32 = 11.0;
/// How quickly the camera catches up with the rig (per second).
const EASE: f32 = 8.0;
/// How far past the followed champion the camera looks, into the table.
const LOOK_AHEAD: f32 = 2.5;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Rig>()
            .add_systems(crate::MatchBegins, seat_the_camera)
            .add_systems(
                crate::InGame,
                (steer, follow, apply)
                    .chain()
                    .after(crate::token::move_tokens),
            );
    }
}

/// Where the camera should look from.
#[derive(Resource)]
pub struct Rig {
    focus: Vec3,
    distance: f32,
    /// Turn of the table around its centre; 0 looks from +Z.
    yaw: f32,
    /// Keep the human's champion in focus.
    following: bool,
    /// What the camera shows now, easing towards the fields above.
    shown: (Vec3, f32, f32),
}

impl Default for Rig {
    fn default() -> Rig {
        Rig {
            focus: Vec3::ZERO,
            distance: FAR,
            yaw: 0.0,
            following: false,
            shown: (Vec3::ZERO, FAR, 0.0),
        }
    }
}

/// The camera for a look at `focus` from `distance`, turned by `yaw`.
fn place(focus: Vec3, distance: f32, yaw: f32) -> Transform {
    let back = Vec3::new(yaw.sin(), 0.0, yaw.cos()) * PITCH.cos();
    let eye = focus + (back + Vec3::Y * PITCH.sin()) * distance;
    Transform::from_translation(eye).looking_at(focus, Vec3::Y)
}

/// At the start: the human's side of the table towards them, their
/// champion in focus.
fn seat_the_camera(game: Res<Match>, board: Res<Board>, mut rig: ResMut<Rig>) {
    if std::env::var("NECROMY_CAMERA").is_ok_and(|v| v == "overview") {
        *rig = Rig {
            focus: Vec3::new(0.0, 0.0, 1.5),
            distance: 22.0,
            yaw: 0.0,
            following: false,
            shown: (Vec3::new(0.0, 0.0, 1.5), 22.0, 0.0),
        };
        return;
    }
    let Some(champion) = game.game.champion(game.human) else {
        return;
    };
    let home = board.hex_to_world(game.game.board().start_of(champion.god));
    // Sit on the side of the table the champion starts from.
    let yaw = if home.length_squared() > 0.01 {
        home.x.atan2(home.z)
    } else {
        0.0
    };
    let focus =
        board.hex_to_world(champion.hex) - Vec3::new(yaw.sin(), 0.0, yaw.cos()) * LOOK_AHEAD;
    *rig = Rig {
        focus,
        distance: START,
        yaw,
        following: true,
        shown: (focus, START, yaw),
    };
}

#[allow(clippy::too_many_arguments)]
fn steer(
    time: Res<Time>,
    mut wheel: MessageReader<MouseWheel>,
    mut motion: MessageReader<MouseMotion>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    game: Res<Match>,
    selection: Res<Selection>,
    window: Single<&Window>,
    mut rig: ResMut<Rig>,
) {
    for w in wheel.read() {
        let notches = match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 40.0,
        };
        rig.distance = (rig.distance * 0.88f32.powf(notches)).clamp(NEAR, FAR);
    }

    // Pan in the table's own directions, faster when far away.
    let right = Vec3::new(rig.yaw.cos(), 0.0, -rig.yaw.sin());
    let towards_me = Vec3::new(rig.yaw.sin(), 0.0, rig.yaw.cos());
    let mut pan = Vec3::ZERO;

    // Right drag is also "cancel the card": it pans only with nothing aimed.
    let dragging = mouse.pressed(MouseButton::Middle)
        || (mouse.pressed(MouseButton::Right) && selection.card.is_none());
    let drag: Vec2 = motion.read().map(|m| m.delta).sum();
    if dragging && drag != Vec2::ZERO {
        // A pixel of drag moves the board about a pixel under the cursor.
        let per_pixel = rig.distance * 1.1 / window.height().max(1.0);
        pan -= right * drag.x * per_pixel;
        pan -= towards_me * drag.y * per_pixel / PITCH.sin();
    }

    // The keyboard belongs to the wish while the human writes one.
    if game.game.wish_due() != Some(game.human) {
        let step = rig.distance * 0.9 * time.delta_secs();
        let held = |a: KeyCode, b: KeyCode| keys.pressed(a) || keys.pressed(b);
        if held(KeyCode::KeyA, KeyCode::ArrowLeft) {
            pan -= right * step;
        }
        if held(KeyCode::KeyD, KeyCode::ArrowRight) {
            pan += right * step;
        }
        if held(KeyCode::KeyW, KeyCode::ArrowUp) {
            pan -= towards_me * step;
        }
        if held(KeyCode::KeyS, KeyCode::ArrowDown) {
            pan += towards_me * step;
        }
        if keys.just_pressed(KeyCode::KeyQ) {
            rig.yaw -= FRAC_PI_3;
        }
        if keys.just_pressed(KeyCode::KeyE) {
            rig.yaw += FRAC_PI_3;
        }
        if keys.just_pressed(KeyCode::KeyF) {
            rig.following = true;
        }
    }
    if pan != Vec3::ZERO {
        rig.following = false;
        // Keep the focus over the board.
        let reach = (game.game.board().radius() as f32 + 1.0) * HEX_SIZE * 1.75;
        rig.focus = (rig.focus + pan).clamp_length_max(reach);
    }
}

/// While following, the focus rides on the human's champion.
fn follow(game: Res<Match>, tokens: Query<(&Token, &Transform)>, mut rig: ResMut<Rig>) {
    if !rig.following {
        return;
    }
    if let Some((_, t)) = tokens.iter().find(|(token, _)| token.player == game.human) {
        // Look a little ahead, into the table: the champion stands low on the
        // screen with the board in front of them, not the void behind.
        let ahead = -Vec3::new(rig.yaw.sin(), 0.0, rig.yaw.cos()) * LOOK_AHEAD;
        rig.focus = t.translation.with_y(0.0) + ahead;
    }
}

/// Ease the camera towards the rig.
pub fn apply(
    time: Res<Time>,
    mut rig: ResMut<Rig>,
    mut camera: Single<&mut Transform, With<crate::TableCamera>>,
) {
    let k = 1.0 - (-EASE * time.delta_secs()).exp();
    let (focus, distance, yaw) = rig.shown;
    // Turn the short way round.
    let mut turn = (rig.yaw - yaw) % TAU;
    if turn > TAU / 2.0 {
        turn -= TAU;
    } else if turn < -TAU / 2.0 {
        turn += TAU;
    }
    let shown = (
        focus.lerp(rig.focus, k),
        distance + (rig.distance - distance) * k,
        yaw + turn * k,
    );
    // The rig is only read here; writing it back must not look like a change.
    rig.bypass_change_detection().shown = shown;
    let wanted = place(shown.0, shown.1, shown.2);
    if **camera != wanted {
        **camera = wanted;
    }
}
