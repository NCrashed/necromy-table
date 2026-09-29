//! Great Deeds and the end of the match (docs/design.md §21.7).
//!
//! Right side, under the gods: everyone's deed, open to all, each with its
//! steps as "have / need" and a mark when met, and the eve when all hold;
//! the explanation and what the world must have for it show on hover. At
//! the start a panel in the middle offers the human three deeds to pick
//! from. When someone wins, a panel over everything says who and how, with
//! a new match or a look at the board.

use bevy::prelude::*;
use necromy_rules::{GreatDeed, Intent, PlayerId};

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
                (rebuild_conditions, rebuild_end, rebuild_choice)
                    .run_if(resource_changed::<Match>.or_else(resource_changed::<EndDismissed>)),
            )
            .add_systems(crate::InGame, (condition_tip, end_buttons, choose_deed));
    }
}

/// Where the deeds panel goes: under the gods, in the right column
/// (`stats.rs` spawns it there).
#[derive(Component)]
pub struct Conditions;

/// One deed line, for its hover explanation.
#[derive(Component)]
struct ConditionRow(GreatDeed);

#[derive(Component)]
struct ConditionTip;

#[derive(Component)]
struct EndPanel;

/// The three deeds offered to the human, in the middle, until one is picked.
#[derive(Component)]
struct ChoicePanel;

#[derive(Component, Clone, Copy)]
struct PickDeed(GreatDeed);

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
    commands.spawn((
        ChoicePanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(120.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        GlobalZIndex(16),
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
    let title = stats::label(&mut commands, &font, "Великие деяния", 14.0, true);
    commands.entity(sheet).add_child(title);
    // The human's first, then the rivals': all are open (§21.7).
    let mut seats: Vec<PlayerId> = g.players().collect();
    seats.sort_by_key(|&p| p != game.human);
    for p in seats {
        let row = match g.deed(p) {
            Some(deed) => condition_row(&mut commands, &font, &game, p, deed),
            None if g.offers(p).is_empty() => continue,
            None => stats::label(
                &mut commands,
                &font,
                &format!("{}: выбирает деяние…", game.name(p)),
                12.0,
                false,
            ),
        };
        commands.entity(sheet).add_child(row);
    }
    let hint = commands
        .spawn((
            Text::new("Наведи на деяние — пояснение. Всё выполнено — канун; свершится на закате, если его не сорвут."),
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

/// The deed's name, then each step as "label have/need", green when met.
fn condition_row(
    commands: &mut Commands,
    font: &UiFont,
    m: &Match,
    owner: PlayerId,
    deed: GreatDeed,
) -> Entity {
    let g = &m.game;
    let rival = owner != m.human;
    let checks = g.checks(owner, deed);
    let done = checks.iter().filter(|c| c.met()).count();
    let eve = g.on_eve(owner);
    let row = commands
        .spawn((
            ConditionRow(deed),
            Button,
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(8.0), px(6.0)),
                ..default()
            },
            Frame::Tip,
            Accent(if eve {
                GOLD
            } else if rival {
                Color::srgb(0.85, 0.35, 0.3)
            } else {
                Color::srgb(0.66, 0.47, 0.24)
            }),
        ))
        .id();
    let (name, _) = names::great_deed(deed);
    let who = if rival {
        format!("{}: ", m.name(owner))
    } else {
        String::new()
    };
    let eve_mark = if eve { " · канун!" } else { "" };
    let head = format!("{who}{name}  {done}/{}{eve_mark}", checks.len());
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

/// What the world must have for `deed`, marked where it has it already.
fn needs_line(m: &Match, deed: GreatDeed) -> String {
    let needs: Vec<String> = deed
        .needs()
        .iter()
        .map(|&f| {
            let mark = if m.game.has(f) { "✓" } else { "·" };
            format!("{mark} {}", names::feature(f).0.to_lowercase())
        })
        .collect();
    if needs.is_empty() {
        "Всё нужное в мире есть.".into()
    } else {
        format!("Нужно в мире: {}.", needs.join(", "))
    }
}

fn condition_tip(
    game: Res<Match>,
    rows: Query<(&Interaction, &ConditionRow)>,
    tip: Single<(&mut Text, &mut Visibility), With<ConditionTip>>,
) {
    let (mut text, mut visibility) = tip.into_inner();
    let Some(deed) = rows
        .iter()
        .find(|(i, _)| **i != Interaction::None)
        .map(|(_, r)| r.0)
    else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let (name, explain) = names::great_deed(deed);
    let wanted = format!(
        "{name} ({}): {explain}\n{}",
        names::god(deed.patron()),
        needs_line(&game, deed)
    );
    if text.0 != wanted {
        text.0 = wanted;
    }
    visibility.set_if_neq(Visibility::Inherited);
}

/// The deeds offered to the human, in the middle, until they pick one.
fn rebuild_choice(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<ChoicePanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let g = &game.game;
    if !g.choosing().contains(&game.human) || game.autoplay {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(10.0),
                padding: UiRect::all(px(22.0)),
                width: px(760.0),
                ..default()
            },
            Frame::Plate,
            Accent(GOLD),
        ))
        .id();
    let title = stats::label(
        &mut commands,
        &font,
        "Выбери Великое деяние: цель твоей партии",
        18.0,
        true,
    );
    let hint = commands
        .spawn((
            Text::new(
                "Деяние видят все. Чтобы его совершить, придётся принести в мир то, чего в нём ещё нет: \
                 желаниями, историями, рисками. Когда всё выполнено — канун, и деяние свершится на закате, \
                 если соперники его не сорвут.",
            ),
            font.text(12.0),
            TextColor(DIM),
            Node {
                width: px(716.0),
                ..default()
            },
        ))
        .id();
    let cards = commands
        .spawn(Node {
            column_gap: px(10.0),
            ..default()
        })
        .id();
    for &deed in g.offers(game.human) {
        let (name, explain) = names::great_deed(deed);
        let card = commands
            .spawn((
                PickDeed(deed),
                Button,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6.0),
                    padding: UiRect::all(px(12.0)),
                    width: px(232.0),
                    ..default()
                },
                Frame::Button,
                Accent(crate::gods_ui::god_color(deed.patron())),
            ))
            .id();
        let head = stats::row(&mut commands);
        let icon = stats::icon_node(
            &mut commands,
            art.gods[deed.patron().index()].clone(),
            22.0,
            true,
        );
        let label = stats::label(&mut commands, &font, name, 15.0, true);
        commands.entity(head).add_children(&[icon, label]);
        let text = |commands: &mut Commands, s: String, color: Color| {
            commands
                .spawn((
                    Text::new(s),
                    font.text(12.0),
                    TextColor(color),
                    Node {
                        width: px(208.0),
                        ..default()
                    },
                ))
                .id()
        };
        let what = text(&mut commands, explain, INK);
        let needs = text(&mut commands, needs_line(&game, deed), DIM);
        commands.entity(card).add_children(&[head, what, needs]);
        commands.entity(cards).add_child(card);
    }
    commands.entity(frame).add_children(&[title, hint, cards]);
    commands.entity(panel).add_child(frame);
}

fn choose_deed(
    pressed: Query<(&Interaction, &PickDeed), Changed<Interaction>>,
    mut game: ResMut<Match>,
) {
    for (interaction, pick) in &pressed {
        if *interaction == Interaction::Pressed {
            let human = game.human;
            let _ = game.act(human, Intent::ChooseDeed { deed: pick.0 });
        }
    }
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
    let Some((winner, deed)) = game.game.winner() else {
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
    let (name, explain) = names::great_deed(deed);
    // Who, then how, each on its own line and wrapping inside the plate.
    let who = commands
        .spawn((
            Text::new(format!("{}\nВеликое деяние: {name}", game.name(winner))),
            font.bold(17.0),
            TextColor(INK),
            TextLayout::justify(Justify::Center),
            Node {
                max_width: px(400.0),
                ..default()
            },
        ))
        .id();
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
