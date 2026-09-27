//! Day and night on the table (docs/design.md §11.1, §18).
//!
//! The board is lit: by day a warm sun and bright sky, by night a dim blue
//! moon, a dark sky and the lights of the world (fireflies, braziers,
//! windows, temples, a glow around each champion). The switch eases over a
//! couple of seconds when a round turns. The dice trays have their own
//! light and camera, and the camera-only ambient light keeps night off them.

use bevy::prelude::*;
use necromy_rules::TimeOfDay;

use crate::play::Match;

/// Seconds for day to turn into night.
const TURN_SECS: f32 = 2.5;

// By day the lit board looks about as bright as it did unlit.
const DAY_SUN: f32 = 2_600.0;
const DAY_SKY: f32 = 260.0;
const DAY_SUN_COLOR: Color = Color::srgb(1.0, 0.96, 0.88);
const DAY_SKY_COLOR: Color = Color::srgb(0.92, 0.95, 1.0);
const DAY_BACKGROUND: Color = Color::srgb(0.17, 0.17, 0.19);
// By night the moon only hints at the land; lights carry the scene.
const NIGHT_SUN: f32 = 380.0;
const NIGHT_SKY: f32 = 45.0;
const NIGHT_SUN_COLOR: Color = Color::srgb(0.55, 0.65, 1.0);
const NIGHT_SKY_COLOR: Color = Color::srgb(0.45, 0.52, 0.85);
const NIGHT_BACKGROUND: Color = Color::srgb(0.04, 0.04, 0.08);

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DayNight>()
            .insert_resource(ClearColor(DAY_BACKGROUND))
            .add_systems(crate::MatchBegins, start_at_the_right_hour)
            .add_systems(
                crate::InGame,
                (follow_the_hour, light_the_table, glow).chain(),
            );
    }
}

/// How far into night the table is: 0 is full day, 1 full night.
#[derive(Resource, Default)]
pub struct DayNight {
    pub night: f32,
}

/// The table's sun (the moon by night).
#[derive(Component)]
pub struct Sun;

/// A point light that burns brighter at night: its intensity in lumens by
/// day and by night.
#[derive(Component, Clone, Copy)]
pub struct Glow {
    pub day: f32,
    pub night: f32,
}

impl Glow {
    /// Only lit at night.
    pub const fn night(lumens: f32) -> Glow {
        Glow {
            day: 0.0,
            night: lumens,
        }
    }
}

/// A point light for `glow`, no shadows, starting dark.
pub fn lamp(color: Color, glow: Glow, range: f32) -> impl Bundle {
    (
        PointLight {
            color,
            intensity: 0.0,
            range,
            shadow_maps_enabled: false,
            ..default()
        },
        glow,
    )
}

fn hour(game: &Match) -> f32 {
    match game.game.time() {
        TimeOfDay::Day => 0.0,
        TimeOfDay::Night => 1.0,
    }
}

/// A match joined at night does not fade in from day.
fn start_at_the_right_hour(game: Res<Match>, mut day_night: ResMut<DayNight>) {
    day_night.night = hour(&game);
}

fn follow_the_hour(time: Res<Time>, game: Res<Match>, mut day_night: ResMut<DayNight>) {
    let target = hour(&game);
    let step = time.delta_secs() / TURN_SECS;
    let night = day_night.night;
    let next = if night < target {
        (night + step).min(target)
    } else {
        (night - step).max(target)
    };
    if next != night {
        day_night.night = next;
    }
}

fn light_the_table(
    day_night: Res<DayNight>,
    mut sun: Single<&mut DirectionalLight, With<Sun>>,
    mut sky: Single<&mut AmbientLight, With<crate::TableCamera>>,
    mut background: ResMut<ClearColor>,
) {
    if !day_night.is_changed() {
        return;
    }
    // Ease in and out, so dusk lingers a little.
    let t = day_night.night * day_night.night * (3.0 - 2.0 * day_night.night);
    sun.illuminance = DAY_SUN + (NIGHT_SUN - DAY_SUN) * t;
    sun.color = DAY_SUN_COLOR.mix(&NIGHT_SUN_COLOR, t);
    sky.brightness = DAY_SKY + (NIGHT_SKY - DAY_SKY) * t;
    sky.color = DAY_SKY_COLOR.mix(&NIGHT_SKY_COLOR, t);
    background.0 = DAY_BACKGROUND.mix(&NIGHT_BACKGROUND, t);
}

/// Lamps follow the hour; a new lamp takes it at once.
fn glow(day_night: Res<DayNight>, mut lamps: Query<(Ref<Glow>, &mut PointLight)>) {
    let t = day_night.night;
    for (glow, mut light) in &mut lamps {
        if !day_night.is_changed() && !glow.is_added() {
            continue;
        }
        let intensity = glow.day + (glow.night - glow.day) * t;
        if light.intensity != intensity {
            light.intensity = intensity;
        }
    }
}
