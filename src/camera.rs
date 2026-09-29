//! The table camera, as in Armello: the table turned with the human's side
//! towards them, close on their champion, following them.
//!
//! The view is orthographic and pixel-true: one texel of any sprite or tile
//! (`board::TEXELS` per world unit) covers a whole number of screen pixels
//! at every zoom step but the overview, and the camera snaps to the pixel
//! grid, so nothing on the table is drawn with bigger or smaller pixels
//! than its neighbours.
//!
//! Wheel zooms step by step between a close look and the whole board; right or middle
//! drag and WASD/arrows pan; Q/E turn the table by a hex side; F goes back
//! to the champion and follows again. The rig holds where the camera
//! should be; the camera eases towards it every frame.
//!
//! Dev aid: `NECROMY_CAMERA=overview` starts over the whole board, as the
//! old fixed camera did (handy for screenshots); `=hover` looks at the
//! pinned `NECROMY_HOVER` hex.

use std::f32::consts::{FRAC_PI_3, TAU};

use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::board::{Board, HEX_SIZE, TEXELS};
use crate::play::{Match, Selection};
use crate::token::Token;

/// Downward tilt of the view.
const PITCH: f32 = 0.95;
/// Screen pixels per texel at each zoom step; the first is the overview,
/// the only one where texels are smaller than a pixel.
const ZOOMS: [f32; 5] = [0.5, 1.0, 2.0, 3.0, 4.0];
/// Where a match starts: close on the champion.
const START: f32 = 2.0;
const OVERVIEW: f32 = 0.5;
/// How far back the eye stands; orthographic, so only clipping cares.
const EYE_DISTANCE: f32 = 60.0;
/// Keyboard panning speed, in screen pixels per second.
const PAN_PIXELS: f32 = 700.0;
/// How quickly the camera catches up with the rig (per second).
const EASE: f32 = 8.0;
/// How far past the followed champion the camera looks, into the table.
const LOOK_AHEAD: f32 = 0.6;

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
    /// Screen pixels per texel, one of `ZOOMS`.
    zoom: f32,
    /// Turn of the table around its centre; 0 looks from +Z.
    yaw: f32,
    /// Keep the human's champion in focus.
    following: bool,
    /// Whom it follows: a rival picked by number key, else the human.
    whom: Option<necromy_rules::PlayerId>,
    /// What the camera shows now, easing towards the fields above.
    shown: (Vec3, f32, f32),
}

impl Rig {
    /// The point on the table the camera looks at.
    pub fn focus(&self) -> Vec3 {
        self.focus
    }

    /// The camera's turn snapped to a hex side: what lies flat on the
    /// table (ground tiles, bodies, traps) turns by this, so its pictures
    /// stay upright from any side while flat-top hexes stay flat-top.
    pub fn table_turn(&self) -> f32 {
        (self.shown.2 / FRAC_PI_3).round() * FRAC_PI_3
    }
}

impl Default for Rig {
    fn default() -> Rig {
        Rig {
            focus: Vec3::ZERO,
            zoom: OVERVIEW,
            yaw: 0.0,
            following: false,
            whom: None,
            shown: (Vec3::ZERO, OVERVIEW, 0.0),
        }
    }
}

/// World units per screen pixel at a zoom.
fn per_pixel(zoom: f32) -> f32 {
    1.0 / (TEXELS * zoom)
}

/// The camera for a look at `focus`, turned by `yaw`, its position snapped
/// to whole screen pixels so still things do not shimmer as it moves.
fn place(focus: Vec3, zoom: f32, yaw: f32) -> Transform {
    let back = Vec3::new(yaw.sin(), 0.0, yaw.cos()) * PITCH.cos();
    let eye = focus + (back + Vec3::Y * PITCH.sin()) * EYE_DISTANCE;
    let mut t = Transform::from_translation(eye).looking_at(focus, Vec3::Y);
    let px = per_pixel(zoom);
    let (right, up) = (t.right().as_vec3(), t.up().as_vec3());
    let snap = |v: f32| (v / px).round() * px - v;
    t.translation += right * snap(eye.dot(right)) + up * snap(eye.dot(up));
    t
}

/// At the start: the human's side of the table towards them, their
/// champion in focus.
fn seat_the_camera(game: Res<Match>, board: Res<Board>, mut rig: ResMut<Rig>) {
    if std::env::var("NECROMY_CAMERA").is_ok_and(|v| v == "overview") {
        *rig = Rig {
            focus: Vec3::ZERO,
            zoom: OVERVIEW,
            yaw: 0.0,
            following: false,
            whom: None,
            shown: (Vec3::ZERO, OVERVIEW, 0.0),
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
        zoom: START,
        yaw,
        following: true,
        whom: None,
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
        // One step per notch, towards the nearest step in that direction.
        let at = ZOOMS
            .iter()
            .position(|&z| z >= rig.zoom)
            .unwrap_or(ZOOMS.len() - 1);
        let next = if notches > 0.0 {
            (at + 1).min(ZOOMS.len() - 1)
        } else if notches < 0.0 {
            at.saturating_sub(1)
        } else {
            at
        };
        rig.zoom = ZOOMS[next];
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
        let px = per_pixel(rig.zoom) * window.scale_factor();
        pan -= right * drag.x * px;
        pan -= towards_me * drag.y * px / PITCH.sin();
    }

    // The keyboard belongs to the wish while the human writes one.
    if game.game.wish_due() != Some(game.human) {
        let step = PAN_PIXELS * per_pixel(rig.zoom) * time.delta_secs();
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
            rig.whom = None;
        }
        // 1..5: the champion of that seat, as the portraits bottom right
        // stand; followed until the camera is moved.
        const SEATS: [(KeyCode, KeyCode); 5] = [
            (KeyCode::Digit1, KeyCode::Numpad1),
            (KeyCode::Digit2, KeyCode::Numpad2),
            (KeyCode::Digit3, KeyCode::Numpad3),
            (KeyCode::Digit4, KeyCode::Numpad4),
            (KeyCode::Digit5, KeyCode::Numpad5),
        ];
        for (i, (a, b)) in SEATS.into_iter().enumerate() {
            let seat = necromy_rules::PlayerId(i as u8);
            if (keys.just_pressed(a) || keys.just_pressed(b)) && game.game.champion(seat).is_some()
            {
                rig.following = true;
                rig.whom = (seat != game.human).then_some(seat);
            }
        }
    }
    if pan != Vec3::ZERO {
        rig.following = false;
        // Keep the focus over the board.
        let reach = (game.game.board().radius() as f32 + 1.0) * HEX_SIZE * 1.75;
        rig.focus = (rig.focus + pan).clamp_length_max(reach);
    }
}

/// While following, the focus rides on the human's champion, or on the one
/// picked by number key.
fn follow(
    game: Res<Match>,
    tokens: Query<(&Token, &Transform)>,
    hovered: Res<crate::board::Hovered>,
    board: Res<Board>,
    mut rig: ResMut<Rig>,
) {
    // Dev aid: `NECROMY_CAMERA=hover` looks at the pinned `NECROMY_HOVER`
    // hex instead (a trial, the guard), for screenshots. Only a pinned one:
    // chasing the mouse's own hex would drag the camera to the board's edge.
    if std::env::var("NECROMY_CAMERA").is_ok_and(|v| v == "hover")
        && std::env::var_os("NECROMY_HOVER").is_some()
        && let Some(hex) = hovered.0
    {
        rig.focus = board.hex_to_world(hex).with_y(0.0);
        return;
    }
    if !rig.following {
        return;
    }
    let whom = rig.whom.unwrap_or(game.human);
    if let Some((_, t)) = tokens.iter().find(|(token, _)| token.player == whom) {
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
    camera: Single<(&mut Transform, &mut Projection), With<crate::TableCamera>>,
) {
    let (mut camera, mut projection) = camera.into_inner();
    let k = 1.0 - (-EASE * time.delta_secs()).exp();
    let (focus, zoom, yaw) = rig.shown;
    // Turn the short way round.
    let mut turn = (rig.yaw - yaw) % TAU;
    if turn > TAU / 2.0 {
        turn -= TAU;
    } else if turn < -TAU / 2.0 {
        turn += TAU;
    }
    let shown = (
        focus.lerp(rig.focus, k),
        // Lands exactly on the step, so texels end up whole pixels.
        if (rig.zoom - zoom).abs() < 0.005 {
            rig.zoom
        } else {
            zoom + (rig.zoom - zoom) * k
        },
        yaw + turn * k,
    );
    // The rig is only read here; writing it back must not look like a change.
    rig.bypass_change_detection().shown = shown;
    let wanted = place(shown.0, shown.1, shown.2);
    if *camera != wanted {
        *camera = wanted;
    }
    if let Projection::Orthographic(ortho) = projection.as_mut() {
        let scale = per_pixel(shown.1);
        if ortho.scale != scale {
            ortho.scale = scale;
        }
    }
}
