//! The gods' layer made visible (docs/design.md §5): every offering flies
//! to its god in the gods panel as "+N" (and the god it cools shows "−N"),
//! so a player sees what their card or prayer fed; and dusk is a scene:
//! which gods shifted, from what to what, and the law each now lays on the
//! table. The scene goes on «Понятно», when the human ends their turn,
//! or after `DUSK_SECS`.

use bevy::prelude::*;
use necromy_rules::{God, Law, Phase};

use crate::hud::{INK, UiFont};
use crate::names;
use crate::play::Match;
use crate::stats::{self, GodRow, StatArt};
use crate::token::Token;
use crate::ui_skin::{Accent, Frame};

/// Seconds an offering takes to reach its god.
const FLY: f32 = 0.9;
/// Seconds between offerings made at once, so they read one by one.
const STAGGER: f32 = 0.25;
/// The dusk scene closes by itself after this long.
const DUSK_SECS: f32 = 25.0;

pub struct GodsUiPlugin;

impl Plugin for GodsUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DuskScene>()
            .add_systems(Startup, spawn)
            .add_systems(
                crate::InGame,
                (
                    launch,
                    fly,
                    take_dusk_news,
                    rebuild_dusk.run_if(resource_changed::<DuskScene>),
                    close_dusk,
                )
                    .chain(),
            );
    }
}

/// An offering on its way from `from` (screen, logical px) to its god.
#[derive(Component)]
struct Offering {
    god: God,
    from: Vec2,
    start: f32,
    /// `false`: a "−N" rising at the cooled god's row.
    fed: bool,
}

#[derive(Resource, Default, PartialEq)]
struct DuskScene {
    shifts: Vec<(God, u8, u8)>,
    /// Mechanics come into the world (§21.4): what, by which god, for whom.
    grown: Vec<(necromy_rules::Feature, God, Option<String>)>,
    since: f32,
    /// The human has been free to act since the scene opened: once they
    /// end that turn, the scene has been read (or ignored) and goes.
    acted: bool,
}

#[derive(Component)]
pub(crate) struct DuskPanel;

#[derive(Component)]
struct CloseDusk;

fn spawn(mut commands: Commands) {
    commands.spawn((
        DuskPanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(140.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        GlobalZIndex(11),
        Visibility::Hidden,
    ));
}

pub(crate) fn god_color(god: God) -> Color {
    let [r, g, b] = god.accent();
    Color::srgb_u8(r, g, b)
}

/// Offerings the table told about become little flights.
#[allow(clippy::too_many_arguments)]
fn launch(
    mut commands: Commands,
    time: Res<Time>,
    mut game: ResMut<Match>,
    font: Res<UiFont>,
    art: Res<StatArt>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::TableCamera>>,
    tokens: Query<(&Token, &GlobalTransform)>,
    window: Single<&Window>,
) {
    if game.offerings.is_empty() {
        return;
    }
    let offerings = std::mem::take(&mut game.bypass_change_detection().offerings);
    let (camera, eye) = *camera;
    let now = time.elapsed_secs();
    // The world's own offerings rise from the middle of the table.
    let middle = Vec2::new(window.width() / 2.0, window.height() / 2.0);
    for (i, (player, god, amount)) in offerings.into_iter().enumerate() {
        let from = player
            .and_then(|p| tokens.iter().find(|(t, _)| t.player == p))
            .and_then(|(_, at)| {
                camera
                    .world_to_viewport(eye, at.translation() + Vec3::Y * 0.9)
                    .ok()
            })
            .unwrap_or(middle);
        let start = now + i as f32 * STAGGER;
        let cooled = God::ALL[god.element().quenches().index()];
        for (target, fed, sign) in [(god, true, "+"), (cooled, false, "−")] {
            let label = commands
                .spawn((
                    Text::new(format!("{sign}{amount}")),
                    font.bold(if fed { 15.0 } else { 13.0 }),
                    TextColor(if fed {
                        god_color(target).lighter(0.2)
                    } else {
                        Color::srgb(0.6, 0.78, 0.95)
                    }),
                    TextShadow::default(),
                ))
                .id();
            let icon = stats::icon_node(
                &mut commands,
                art.gods[target.index()].clone(),
                if fed { 20.0 } else { 16.0 },
                true,
            );
            commands
                .spawn((
                    Offering {
                        god: target,
                        from,
                        start,
                        fed,
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(from.x),
                        top: px(from.y),
                        align_items: AlignItems::Center,
                        column_gap: px(2.0),
                        ..default()
                    },
                    GlobalZIndex(12),
                    Visibility::Hidden,
                ))
                .add_children(&[icon, label]);
        }
    }
}

fn fly(
    mut commands: Commands,
    time: Res<Time>,
    rows: Query<(&GodRow, &ComputedNode, &UiGlobalTransform)>,
    mut flights: Query<(
        Entity,
        &Offering,
        &mut Node,
        &mut Visibility,
        &mut UiTransform,
    )>,
) {
    let now = time.elapsed_secs();
    for (entity, flight, mut node, mut visibility, mut transform) in &mut flights {
        let t = (now - flight.start) / FLY;
        if t < 0.0 {
            continue;
        }
        let Some(row) = rows
            .iter()
            .find(|(r, ..)| r.0 == flight.god)
            .map(|(_, n, at)| {
                // The right end of the god's row: its pressure and favour.
                let c = at.affine().translation * n.inverse_scale_factor();
                c + Vec2::new(n.size().x * n.inverse_scale_factor() * 0.3, -8.0)
            })
        else {
            commands.entity(entity).despawn();
            continue;
        };
        if t >= 1.4 {
            commands.entity(entity).despawn();
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        let at = if flight.fed {
            // An arc from the champion to the god.
            let k = 1.0 - (1.0 - t.min(1.0)).powi(3);
            flight.from.lerp(row, k) - Vec2::Y * 60.0 * (std::f32::consts::PI * t.min(1.0)).sin()
        } else {
            // The cooled god: a sign that rises at its row once the other lands.
            let k = ((t - 0.6) / 0.8).clamp(0.0, 1.0);
            if t < 0.6 {
                *visibility = Visibility::Hidden;
            }
            row - Vec2::Y * 16.0 * k
        };
        node.left = px(at.x);
        node.top = px(at.y);
        // Lands, lingers, fades by shrinking (text keeps its colour).
        let fade = if t > 1.0 { 1.0 - (t - 1.0) / 0.4 } else { 1.0 };
        transform.scale = Vec2::splat(fade.max(0.0));
    }
}

fn take_dusk_news(time: Res<Time>, mut game: ResMut<Match>, mut scene: ResMut<DuskScene>) {
    if game.dusk_news.is_empty() && game.world_news.is_empty() {
        return;
    }
    let m = game.bypass_change_detection();
    let shifts = std::mem::take(&mut m.dusk_news);
    let grown = std::mem::take(&mut m.world_news);
    *scene = DuskScene {
        shifts,
        grown,
        since: time.elapsed_secs(),
        acted: false,
    };
}

fn rebuild_dusk(
    mut commands: Commands,
    scene: Res<DuskScene>,
    game: Res<crate::play::Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<DuskPanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    if scene.shifts.is_empty() && scene.grown.is_empty() {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8.0),
                padding: UiRect::all(px(20.0)),
                width: px(500.0),
                ..default()
            },
            Frame::Plate,
            Accent(Color::srgb(0.85, 0.55, 0.35)),
        ))
        .id();
    let heading = if scene.shifts.is_empty() {
        "Закат: в мире новое"
    } else {
        "Закат: боги меняются"
    };
    let title = stats::label(&mut commands, &font, heading, 17.0, true);
    commands.entity(frame).add_child(title);
    for &(god, from, to) in &scene.shifts {
        let entry = commands
            .spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(2.0),
                ..default()
            })
            .id();
        let head = stats::row(&mut commands);
        let icon = stats::icon_node(&mut commands, art.gods[god.index()].clone(), 24.0, true);
        let darker = to > from;
        let what = format!(
            "{} {}: {} → {}",
            names::god(god),
            if darker {
                "темнеет"
            } else {
                "светлеет"
            },
            names::stage(god, from),
            names::stage(god, to)
        );
        let name = commands
            .spawn((
                Text::new(what),
                font.bold(14.0),
                TextColor(god_color(god).lighter(0.2)),
            ))
            .id();
        commands.entity(head).add_children(&[icon, name]);
        let law = commands
            .spawn((
                Text::new(format!(
                    "Новый закон «{}»: {}.",
                    names::law_name(Law::of(god, to)),
                    names::law_line(Law::of(god, to), &game.game)
                )),
                font.text(12.0),
                TextColor(INK),
                Node {
                    width: px(460.0),
                    margin: UiRect::left(px(30.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(entry).add_children(&[head, law]);
        commands.entity(frame).add_child(entry);
    }
    for (feature, god, who) in &scene.grown {
        let (name, what) = names::feature(*feature);
        let head = stats::row(&mut commands);
        let icon = stats::icon_node(&mut commands, art.gods[god.index()].clone(), 24.0, true);
        let title = commands
            .spawn((
                Text::new(format!("Новое в мире: {name}")),
                font.bold(14.0),
                TextColor(god_color(*god).lighter(0.2)),
            ))
            .id();
        commands.entity(head).add_children(&[icon, title]);
        let by = match who {
            Some(who) => format!(
                "{}: {what}. Принёс {} по желанию {who}.",
                capitalized(what),
                names::god(*god)
            ),
            None => format!("{}.", capitalized(what)),
        };
        let text = commands
            .spawn((
                Text::new(by),
                font.text(12.0),
                TextColor(INK),
                Node {
                    width: px(460.0),
                    margin: UiRect::left(px(30.0)),
                    ..default()
                },
            ))
            .id();
        let entry = commands
            .spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(2.0),
                ..default()
            })
            .add_children(&[head, text])
            .id();
        commands.entity(frame).add_child(entry);
    }
    let label = stats::label(&mut commands, &font, "Понятно", 13.0, true);
    let ok = commands
        .spawn((
            CloseDusk,
            Button,
            Frame::Button,
            Node {
                padding: UiRect::axes(px(14.0), px(7.0)),
                align_self: AlignSelf::FlexEnd,
                ..default()
            },
        ))
        .add_child(label)
        .id();
    commands.entity(frame).add_child(ok);
    commands.entity(panel).add_child(frame);
}

fn close_dusk(
    time: Res<Time>,
    game: Res<Match>,
    ok: Query<&Interaction, (Changed<Interaction>, With<CloseDusk>)>,
    mut scene: ResMut<DuskScene>,
) {
    if scene.shifts.is_empty() && scene.grown.is_empty() {
        return;
    }
    let done = *game.game.phase(game.human) == Phase::Done;
    if !done && !scene.acted {
        scene.bypass_change_detection().acted = true;
    }
    let clicked = ok.iter().any(|i| *i == Interaction::Pressed);
    if clicked || (done && scene.acted) || time.elapsed_secs() - scene.since > DUSK_SECS {
        *scene = DuskScene::default();
    }
}

/// The first letter up.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
