//! Victory conditions and the end of the match (docs/design.md §10).
//!
//! Right side, under the gods: the three open conditions and the human's
//! secret one, each with its checks as "have / need" and a mark when met;
//! the explanation shows on hover. When someone wins, a panel over
//! everything says who and how, with a new match or a look at the board.

use bevy::prelude::*;
use necromy_rules::Condition;

use crate::hud::{INK, UiFont};
use crate::names;
use crate::play::Match;
use crate::stats::{self, StatArt};
use crate::ui_skin::{Accent, Frame};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const MET: Color = Color::srgb(0.45, 0.85, 0.40);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);

pub struct VictoryUiPlugin;

impl Plugin for VictoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EndDismissed>()
            .add_systems(Startup, spawn)
            .add_systems(
                crate::InGame,
                (rebuild_conditions, rebuild_end)
                    .run_if(resource_changed::<Match>.or_else(resource_changed::<EndDismissed>)),
            )
            .add_systems(crate::InGame, (condition_tip, end_buttons));
    }
}

#[derive(Component)]
struct Conditions;

/// One condition line, for its hover explanation.
#[derive(Component)]
struct ConditionRow(Condition);

#[derive(Component)]
struct ConditionTip;

#[derive(Component)]
struct EndPanel;

/// The human closed the end panel to look at the board.
#[derive(Resource, Default)]
struct EndDismissed(bool);

#[derive(Component, Clone, Copy)]
enum EndButton {
    NewMatch,
    LookAtBoard,
}

fn spawn(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        Conditions,
        Node {
            position_type: PositionType::Absolute,
            top: px(205.0),
            right: px(10.0),
            ..default()
        },
    ));
    commands.spawn((
        ConditionTip,
        Text::new(""),
        font.text(12.0),
        TextColor(INK),
        Node {
            position_type: PositionType::Absolute,
            top: px(205.0),
            right: px(300.0),
            width: px(260.0),
            padding: UiRect::all(px(8.0)),
            ..default()
        },
        Frame::Tip,
        GlobalZIndex(10),
        Visibility::Hidden,
    ));
    commands.spawn((
        EndPanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(0.0),
            bottom: px(0.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        GlobalZIndex(20),
        Visibility::Hidden,
    ));
}

fn rebuild_conditions(
    mut commands: Commands,
    game: Res<Match>,
    font: Res<UiFont>,
    panel: Single<Entity, With<Conditions>>,
) {
    commands.entity(*panel).despawn_related::<Children>();
    let g = &game.game;
    let sheet = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12.0)),
                row_gap: px(4.0),
                width: px(280.0),
                ..default()
            },
            Frame::Panel,
        ))
        .id();
    let title = stats::label(&mut commands, &font, "Условия победы", 14.0, true);
    commands.entity(sheet).add_child(title);

    let secret = g.secret(game.human);
    let lines = g
        .open_conditions()
        .iter()
        .map(|&c| (c, false))
        .chain(secret.map(|c| (c, true)));
    for (condition, is_secret) in lines {
        let row = condition_row(&mut commands, &font, &game, condition, is_secret);
        commands.entity(sheet).add_child(row);
    }
    let hint = commands
        .spawn((
            Text::new("Наведи на условие — пояснение. Выполнишь любое своё — победа."),
            font.text(11.0),
            TextColor(DIM),
            Node {
                width: px(264.0),
                ..default()
            },
        ))
        .id();
    commands.entity(sheet).add_child(hint);
    commands.entity(*panel).add_child(sheet);
}

/// The condition's name, then each check as "label have/need", green when met.
fn condition_row(
    commands: &mut Commands,
    font: &UiFont,
    m: &Match,
    condition: Condition,
    secret: bool,
) -> Entity {
    let g = &m.game;
    let checks = g.checks(m.human, condition);
    let done = checks.iter().filter(|c| c.met()).count();
    let row = commands
        .spawn((
            ConditionRow(condition),
            Button,
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(8.0), px(6.0)),
                ..default()
            },
            Frame::Tip,
            Accent(if secret {
                Color::srgb(0.62, 0.45, 0.85)
            } else {
                Color::srgb(0.66, 0.47, 0.24)
            }),
        ))
        .id();
    let (name, _) = names::condition(condition);
    let head = if secret {
        format!("{name} · тайное  {done}/{}", checks.len())
    } else {
        format!("{name}  {done}/{}", checks.len())
    };
    let head = stats::label(commands, font, &head, 13.0, true);
    commands.entity(row).add_child(head);
    for check in checks {
        let color = if check.met() { MET } else { DIM };
        let mark = if check.met() { "✓" } else { "·" };
        let line = commands
            .spawn((
                Text::new(format!(
                    "{mark} {}: {}/{}",
                    names::check(check.kind),
                    check.have,
                    check.need
                )),
                font.text(11.0),
                TextColor(color),
                TextLayout::no_wrap(),
            ))
            .id();
        commands.entity(row).add_child(line);
    }
    row
}

fn condition_tip(
    rows: Query<(&Interaction, &ConditionRow)>,
    tip: Single<(&mut Text, &mut Visibility), With<ConditionTip>>,
) {
    let (mut text, mut visibility) = tip.into_inner();
    let Some(condition) = rows
        .iter()
        .find(|(i, _)| **i != Interaction::None)
        .map(|(_, r)| r.0)
    else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let (name, explain) = names::condition(condition);
    let wanted = format!("{name}: {explain}");
    if text.0 != wanted {
        text.0 = wanted;
    }
    visibility.set_if_neq(Visibility::Inherited);
}

/// Who won and how, over everything.
fn rebuild_end(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<EndPanel>>,
    dismissed: Res<EndDismissed>,
) {
    let (panel, mut visibility) = panel.into_inner();
    let Some((winner, condition)) = game.game.winner() else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    if dismissed.0 {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    commands.entity(panel).despawn_related::<Children>();
    visibility.set_if_neq(Visibility::Inherited);

    let won = winner == game.human;
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10.0),
                padding: UiRect::all(px(26.0)),
                width: px(460.0),
                ..default()
            },
            Frame::Plate,
            Accent(if won {
                GOLD
            } else {
                Color::srgb(0.6, 0.6, 0.65)
            }),
        ))
        .id();
    let title = stats::label(
        &mut commands,
        &font,
        if won {
            "Победа!"
        } else {
            "Поражение"
        },
        30.0,
        true,
    );
    let portrait = commands
        .spawn((
            ImageNode::new(art.portraits[winner.0 as usize].clone()),
            Node {
                width: px(64.0),
                height: px(96.0),
                ..default()
            },
        ))
        .id();
    let (name, explain) = names::condition(condition);
    let secret = game.game.secret(winner) == Some(condition)
        && !game.game.open_conditions().contains(&condition);
    let who = stats::label(
        &mut commands,
        &font,
        &format!(
            "{} — {name}{}",
            game.name(winner),
            if secret {
                " (тайное условие)"
            } else {
                ""
            }
        ),
        17.0,
        true,
    );
    let why = commands
        .spawn((
            Text::new(explain),
            font.text(13.0),
            TextColor(DIM),
            TextLayout::justify(Justify::Center),
            Node {
                width: px(400.0),
                ..default()
            },
        ))
        .id();
    let row = stats::row(&mut commands);
    for (button, label) in [
        (EndButton::NewMatch, "Новая партия"),
        (EndButton::LookAtBoard, "Посмотреть доску"),
    ] {
        let b = commands
            .spawn((
                button,
                Button,
                Node {
                    padding: UiRect::axes(px(16.0), px(8.0)),
                    ..default()
                },
                Frame::Button,
            ))
            .id();
        let t = stats::label(&mut commands, &font, label, 14.0, true);
        commands.entity(b).add_child(t);
        commands.entity(row).add_child(b);
    }
    commands
        .entity(frame)
        .add_children(&[title, portrait, who, why, row]);
    commands.entity(panel).add_child(frame);
}

fn end_buttons(
    pressed: Query<(&Interaction, &EndButton), Changed<Interaction>>,
    mut dismissed: ResMut<EndDismissed>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            // A fresh process is the simplest clean slate for now; the same
            // environment is kept, so a pinned NECROMY_SEED replays the match.
            EndButton::NewMatch => {
                if let Ok(exe) = std::env::current_exe()
                    && std::process::Command::new(exe)
                        .args(std::env::args().skip(1))
                        .spawn()
                        .is_ok()
                {
                    exit.write(AppExit::Success);
                }
            }
            EndButton::LookAtBoard => dismissed.0 = true,
        }
    }
}
