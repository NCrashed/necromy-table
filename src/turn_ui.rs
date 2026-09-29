//! Where the match stands and what the human should do now.
//!
//! Top left, information only: day or night and the round, the turn order as
//! portraits, the table's taste (its explanation on hover). Centre, what
//! needs the human: a "your turn" splash when their turn begins, and an
//! action bar above the hand with the one thing to do, a hint apart from it,
//! and the button for it. The battle panel and the incoming card take the
//! centre when they are up; the action bar steps aside for them.

use bevy::prelude::*;
use necromy_rules::{Intent, TimeOfDay, WindowKind};

use crate::hud::{INK, UiFont};
use crate::icons::StatIcon;
use crate::names;
use crate::play::{self, Match, Selection};
use crate::stats::{self, StatArt};
use crate::ui_skin::{Accent, Frame};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const HINT: Color = Color::srgb(0.72, 0.70, 0.64);
/// How long the "your turn" splash stays, fading out.
const SPLASH_SECS: f32 = 1.6;

pub struct TurnUiPlugin;

impl Plugin for TurnUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn, spawn_clock))
            .add_systems(
                crate::InGame,
                (rebuild_status, rebuild_action)
                    .run_if(resource_changed::<Match>.or_else(resource_changed::<Selection>)),
            )
            .add_systems(
                crate::InGame,
                (taste_tip, action_buttons, splash, show_clock),
            );
    }
}

#[derive(Component)]
struct Status;

#[derive(Component)]
struct TasteChip;

#[derive(Component)]
struct TasteTip;

#[derive(Component)]
struct ActionBar;

#[derive(Component)]
struct Splash;

/// Seconds left on the human's clock (server matches only, §17.1).
#[derive(Component)]
struct ClockChip;

#[derive(Component, Clone, Copy)]
enum ActionButton {
    EndTurn,
    Pass,
    /// Build the ruins underfoot again (§20.4).
    Rebuild,
    /// Let the marked cards go and draw anew (§21.2).
    Cycle,
    /// Take the burden underfoot, lay down the one carried (§21.8).
    Take,
    Lay,
    /// Build on the settlement underfoot, or grow its city (§21.8).
    Build(necromy_rules::Building),
    Quarter(hexx::Hex),
}

fn spawn(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        Status,
        Node {
            position_type: PositionType::Absolute,
            top: px(10.0),
            left: px(10.0),
            ..default()
        },
    ));
    commands.spawn((
        TasteTip,
        Text::new(""),
        font.text(12.0),
        TextColor(INK),
        Node {
            position_type: PositionType::Absolute,
            top: px(92.0),
            left: px(10.0),
            max_width: px(320.0),
            padding: UiRect::all(px(8.0)),
            ..default()
        },
        Frame::Tip,
        GlobalZIndex(10),
        Visibility::Hidden,
    ));
    commands.spawn((
        ActionBar,
        // Top centre: between the status and the gods, clear of the board.
        Node {
            position_type: PositionType::Absolute,
            top: px(8.0),
            // Centred in the gap between the status panel and the gods panel.
            left: px(345.0),
            right: px(425.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Visibility::Hidden,
    ));
    commands.spawn((
        Splash,
        Text::new("Твой ход"),
        font.bold(44.0),
        TextColor(GOLD.with_alpha(0.0)),
        TextShadow::default(),
        // Hidden outright between splashes: the shadow has its own colour and
        // would stay on screen if only the text faded.
        Visibility::Hidden,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(36.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        TextLayout::justify(Justify::Center),
        GlobalZIndex(8),
    ));
}

/// Day or night with the round, the turn order, the taste chip.
fn rebuild_status(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    status: Single<Entity, With<Status>>,
) {
    commands.entity(*status).despawn_related::<Children>();
    let g = &game.game;
    let panel = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12.0)),
                row_gap: px(6.0),
                ..default()
            },
            Frame::Panel,
        ))
        .id();

    // Day or night, round, taste.
    let top = stats::row(&mut commands);
    let time_icon = match g.time() {
        TimeOfDay::Day => StatIcon::Day,
        TimeOfDay::Night => StatIcon::Night,
    };
    let sun = stats::icon_node(&mut commands, art.icon(time_icon), 28.0, true);
    let round = stats::label(
        &mut commands,
        &font,
        &format!("Раунд {} · {}", g.round(), play::time_name(g.time())),
        16.0,
        true,
    );
    let (taste, _) = names::taste_parts(g.taste().kind);
    let chip = commands
        .spawn((
            TasteChip,
            Button,
            Node {
                align_items: AlignItems::Center,
                column_gap: px(4.0),
                padding: UiRect::axes(px(8.0), px(4.0)),
                margin: UiRect::left(px(10.0)),
                ..default()
            },
            Frame::Button,
        ))
        .id();
    let goblet = stats::icon_node(&mut commands, art.icon(StatIcon::Taste), 20.0, true);
    let taste_label = stats::label(&mut commands, &font, &format!("вкус: {taste}"), 13.0, false);
    commands.entity(chip).add_children(&[goblet, taste_label]);
    commands.entity(top).add_children(&[sun, round, chip]);

    // Everyone plays at once (§11.2): each seat in initiative order with
    // what it is doing, the ones the table waits on framed in gold.
    let order = stats::row(&mut commands);
    let awaited = g.awaiting();
    for &p in g.order() {
        let current = g.phase(p) == &necromy_rules::Phase::Acting;
        let seat = commands
            .spawn((Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(5.0)),
                ..default()
            },))
            .id();
        // The one we wait on framed in gold, the one whose turn it is in bronze.
        if awaited.contains(&p) {
            commands.entity(seat).insert((Frame::Tip, Accent(GOLD)));
        } else if current {
            commands.entity(seat).insert(Frame::Tip);
        }
        let portrait = commands
            .spawn((
                ImageNode::new(art.portraits[p.0 as usize].clone()),
                Node {
                    width: px(24.0),
                    height: px(36.0),
                    ..default()
                },
            ))
            .id();
        let name = g.champion(p).map_or("?", |c| names::god(c.god));
        let tag = if p == game.human { "ты" } else { name };
        let tag = stats::label(&mut commands, &font, tag, 10.0, p == game.human);
        let (state, color) = match g.phase(p) {
            necromy_rules::Phase::Acting => ("ходит", GOLD),
            necromy_rules::Phase::Held { .. } => ("ждёт", Color::srgb(0.95, 0.6, 0.35)),
            necromy_rules::Phase::Done => ("готов", Color::srgb(0.6, 0.6, 0.6)),
        };
        let state = commands
            .spawn((Text::new(state), font.text(9.0), TextColor(color)))
            .id();
        commands.entity(seat).add_children(&[portrait, tag, state]);
        commands.entity(order).add_child(seat);
    }

    commands.entity(panel).add_children(&[top, order]);
    commands.entity(*status).add_child(panel);
}

fn taste_tip(
    game: Res<Match>,
    chip: Query<&Interaction, With<TasteChip>>,
    tip: Single<(&mut Text, &mut Visibility), With<TasteTip>>,
) {
    let (mut text, mut visibility) = tip.into_inner();
    let hovered = chip.iter().any(|i| *i != Interaction::None);
    if !hovered {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    let (name, explain) = names::taste_parts(game.game.taste().kind);
    let wanted = format!(
        "Вкус стола «{name}»: {explain}.\nОн задаёт, за что на этот матч дают Стиль, а Стиль решает, кто носит Венец."
    );
    if text.0 != wanted {
        text.0 = wanted;
    }
    visibility.set_if_neq(Visibility::Inherited);
}

/// The one thing the human should do now, a hint apart, and its button.
fn rebuild_action(
    mut commands: Commands,
    game: Res<Match>,
    selection: Res<Selection>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    bar: Single<(Entity, &mut Visibility), With<ActionBar>>,
) {
    let (bar, mut visibility) = bar.into_inner();
    commands.entity(bar).despawn_related::<Children>();
    let g = &game.game;
    let human = game.human;
    let kind = game.human_window();

    let (title, hint, button, moves) = if let Some(marked) = &selection.sift {
        (
            format!("Перебор руки: отмечено {}", marked.len()),
            "клик по карте — отметить; отмеченные уйдут в сброс, доберёшь на одну меньше (в храме — столько же) · Esc — отмена",
            (!marked.is_empty()).then_some((ActionButton::Cycle, "Сбросить\nотмеченные")),
            false,
        )
    } else if let Some(card) = selection.card {
        (
            format!("Цель для «{}»", g.def(card).name),
            "клик по золотой клетке · правый клик или Esc — отмена",
            None,
            false,
        )
    } else {
        match kind {
            // The battle panel and the incoming card have the centre.
            Some(WindowKind::Battle { .. }) => (String::new(), "", None, false),
            Some(WindowKind::Target { target, .. }) if target == human => {
                (String::new(), "", None, false)
            }
            // Tribute (§7.3): a card given, or Threat taken.
            Some(WindowKind::Tribute { asker })
                if asker != human && g.to_answer(human).is_some() =>
            {
                (
                    format!("Дань для {}", game.name(asker)),
                    "кликни карту — отдай её; пробел — откажи и возьми +2 Угрозы",
                    Some((ActionButton::Pass, "Отказать\n(пробел)")),
                    false,
                )
            }
            Some(WindowKind::Tribute { asker }) => (
                if asker == human {
                    "Ждём дань".to_string()
                } else {
                    "Дань отдана · ждём остальных".to_string()
                },
                "каждый соперник отдаёт карту или берёт +2 Угрозы",
                None,
                false,
            ),
            Some(k) if !g.playable(human).is_empty() => {
                let what = if matches!(k, WindowKind::Target { .. }) {
                    "сыграй карту «ответ» из руки или пропусти"
                } else {
                    "сыграй «мгновенную» карту из руки или пропусти"
                };
                (
                    format!("Можно ответить: {}", play::window_name(&game, k)),
                    what,
                    Some((ActionButton::Pass, "Пас\n(пробел)")),
                    false,
                )
            }
            // Ruins underfoot: the one thing only this spot offers (§20.4).
            None if game.is_human_turn() && g.can_rebuild(human) => (
                "Ты на руинах поселения".to_string(),
                "восстанови: 2 Духа, движение на этом ход кончается; пробел — конец хода",
                Some((ActionButton::Rebuild, "Восстановить")),
                true,
            ),
            None if game.is_human_turn() => (
                "Твой ход".to_string(),
                "светлая клетка — идти, зелёная — ополчение пропустит, красная — напасть, карта из руки — сыграть",
                Some((ActionButton::EndTurn, "Конец хода\n(пробел)")),
                true,
            ),
            // Simultaneous turns (§11.2): the action waits for a rival nearby.
            None if matches!(g.phase(human), necromy_rules::Phase::Held { .. }) => {
                let title = match g.phase(human) {
                    necromy_rules::Phase::Held { on: Some(on), .. } => {
                        format!("Ждём: {} ещё ходит", game.name(*on))
                    }
                    _ => "Ждём: рядом кто-то занят".to_string(),
                };
                (
                    title,
                    "твоё действие сыграется, когда он закончит ход",
                    None,
                    false,
                )
            }
            // Dusk waits for wishes (§21.4).
            None if g.at_dusk() == Some(necromy_rules::DuskStep::Sealing) => {
                let names: Vec<String> = g.wishing().iter().map(|&p| game.name(p)).collect();
                if g.wishing().contains(&human) {
                    (
                        "Закат: загадай желание".to_string(),
                        "боги ответят по очереди, Венцу последним",
                        None,
                        true,
                    )
                } else {
                    (
                        format!("Закат · боги ждут желаний: {}", names.join(", ")),
                        "твоё запечатано; ответы придут по очереди",
                        None,
                        false,
                    )
                }
            }
            None if g.at_dusk().is_some() => (
                "Закат: боги отвечают".to_string(),
                "дань, которую просят боги, ещё собирают",
                None,
                false,
            ),
            None if g.phase(human) == &necromy_rules::Phase::Done => {
                let names: Vec<String> = g
                    .order()
                    .iter()
                    .filter(|&&p| g.phase(p) != &necromy_rules::Phase::Done)
                    .map(|&p| game.name(p))
                    .collect();
                (
                    format!("Ход сделан · ждём: {}", names.join(", ")),
                    "все ходят одновременно; раунд кончится, когда закончат все",
                    None,
                    false,
                )
            }
            _ => (String::new(), "", None, false),
        }
    };
    if title.is_empty() {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);

    let panel = commands
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(12.0),
                padding: UiRect::axes(px(14.0), px(10.0)),
                ..default()
            },
            Frame::Panel,
            GlobalZIndex(6),
        ))
        .id();
    let texts = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(2.0),
            ..default()
        })
        .id();
    let main = stats::row(&mut commands);
    let title = stats::label(&mut commands, &font, &title, 18.0, true);
    commands.entity(main).add_child(title);
    if moves {
        let boot = stats::icon_node(&mut commands, art.icon(StatIcon::Moves), 24.0, true);
        let mp = stats::label(
            &mut commands,
            &font,
            &format!("{} очк. движения", g.move_points(human)),
            15.0,
            true,
        );
        commands.entity(main).add_children(&[boot, mp]);
    }
    let hint = commands
        .spawn((
            Text::new(hint),
            font.text(12.0),
            TextColor(HINT),
            // A fixed width lets the hint wrap without inflating the bar.
            Node {
                width: px(250.0),
                ..default()
            },
        ))
        .id();
    commands.entity(texts).add_children(&[main, hint]);
    commands.entity(panel).add_child(texts);

    // What this spot offers besides (§21.8): a burden to take or lay down.
    let mut buttons: Vec<(ActionButton, String)> = button
        .map(|(a, l)| (a, l.to_string()))
        .into_iter()
        .collect();
    if game.is_human_turn() && selection.card.is_none() && selection.sift.is_none() {
        if let Some(cargo) = g.takeable(human) {
            buttons.push((
                ActionButton::Take,
                format!("Взять\n{}", names::cargo(cargo)),
            ));
        }
        if g.cargo(human).is_some() {
            buttons.push((ActionButton::Lay, "Положить\nношу".to_string()));
        }
        let spirit = g.champion(human).map_or(0, |c| c.spirit_points);
        if spirit >= necromy_rules::BUILD_SPIRIT {
            for b in g.may_build(human) {
                buttons.push((
                    ActionButton::Build(b),
                    format!(
                        "Построить ({} Духа)\n{}",
                        necromy_rules::BUILD_SPIRIT,
                        names::building(b)
                    ),
                ));
            }
        }
        if spirit >= necromy_rules::QUARTER_SPIRIT
            && let Some(&hex) = g.quarters(human).first()
        {
            buttons.push((
                ActionButton::Quarter(hex),
                format!("Новый квартал\n{} Духа", necromy_rules::QUARTER_SPIRIT),
            ));
        }
    }
    for (action, label) in buttons {
        let b = commands
            .spawn((
                action,
                Button,
                Node {
                    padding: UiRect::axes(px(14.0), px(8.0)),
                    ..default()
                },
                Frame::Button,
            ))
            .id();
        // Two short lines: the action, then its key.
        let t = commands
            .spawn((
                Text::new(label),
                font.bold(13.0),
                TextColor(INK),
                TextLayout::new(Justify::Center, LineBreak::NoWrap),
            ))
            .id();
        commands.entity(b).add_child(t);
        commands.entity(panel).add_child(b);
    }
    commands.entity(bar).add_child(panel);
}

fn action_buttons(
    pressed: Query<(&Interaction, &ActionButton), Changed<Interaction>>,
    mut game: ResMut<Match>,
    mut selection: ResMut<Selection>,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let human = game.human;
        let intent = match button {
            ActionButton::EndTurn => Intent::EndTurn,
            ActionButton::Rebuild => Intent::Rebuild,
            ActionButton::Take => Intent::Take,
            ActionButton::Lay => Intent::Lay,
            ActionButton::Build(building) => Intent::Build {
                building: *building,
            },
            ActionButton::Quarter(hex) => Intent::Quarter { hex: *hex },
            ActionButton::Cycle => Intent::Cycle {
                cards: selection.sift.take().unwrap_or_default(),
            },
            ActionButton::Pass => {
                selection.card = None;
                Intent::Pass
            }
        };
        if let Err(err) = game.act(human, intent) {
            warn!("action rejected: {err}");
        }
    }
}

/// "Твой ход" in the middle of the screen when the human's turn begins:
/// once a round, the first moment they are free to act (after a dawn wish,
/// say), not every time a window or a wait lets them go again.
fn splash(
    time: Res<Time>,
    game: Res<Match>,
    mut state: Local<(u32, Option<f32>)>,
    splash: Single<(&mut TextColor, &mut TextShadow, &mut Visibility), With<Splash>>,
) {
    let now = time.elapsed_secs();
    let round = game.game.round();
    let (shown_in, started) = &mut *state;
    if game.is_human_turn() && *shown_in != round {
        *shown_in = round;
        *started = Some(now);
    }
    let alpha = started.map_or(0.0, |s| {
        let t = (now - s) / SPLASH_SECS;
        if t >= 1.0 { 0.0 } else { 1.0 - t * t }
    });
    let (mut color, mut shadow, mut visibility) = splash.into_inner();
    if alpha <= 0.0 {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    if color.0.alpha() != alpha {
        color.0 = GOLD.with_alpha(alpha);
        shadow.color = Color::linear_rgba(0.0, 0.0, 0.0, 0.75 * alpha);
    }
}

/// Seconds below which the clock turns red.
const CLOCK_URGENT: f32 = 10.0;

fn spawn_clock(mut commands: Commands, font: Res<UiFont>) {
    let chip = commands
        .spawn((
            ClockChip,
            Text::new(""),
            font.bold(14.0),
            TextColor(INK),
            Node {
                padding: UiRect::axes(px(14.0), px(6.0)),
                ..default()
            },
            Frame::Tip,
        ))
        .id();
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(80.0),
                left: px(0.0),
                right: px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            // Over the battle panel and the wish: the clock runs there too.
            GlobalZIndex(40),
            Visibility::Hidden,
        ))
        .add_child(chip);
}

/// Every frame: the countdown moves even when nothing else changes. Reads
/// `Match` without touching it, so it redraws nothing else.
fn show_clock(
    game: Res<Match>,
    mut chip: Query<(&mut Text, &mut TextColor, &ChildOf), With<ClockChip>>,
    mut holders: Query<&mut Visibility>,
) {
    let Ok((mut text, mut color, parent)) = chip.single_mut() else {
        return;
    };
    let left = game
        .clock
        .map(|c| (c.what, (c.left - game.clock_since).max(0.0)));
    let Ok(mut visibility) = holders.get_mut(parent.parent()) else {
        return;
    };
    // A window with nothing to answer passes by itself at once: no clock.
    let idle_window = |what| {
        what == necromy_host::Decision::Window
            && game.game.playable(game.human).is_empty()
            && game.game.battle_dice(game.human).is_none()
    };
    let Some((what, left)) = left.filter(|(what, _)| !idle_window(*what)) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    let what = match what {
        necromy_host::Decision::Turn => "на ход",
        necromy_host::Decision::Window => "на ответ",
        necromy_host::Decision::Wish => "на желание",
    };
    let wanted = format!("{} с {what}", left.ceil() as u32);
    if text.0 != wanted {
        text.0 = wanted;
    }
    let tint = if left <= CLOCK_URGENT {
        Color::srgb(1.0, 0.45, 0.35)
    } else {
        INK
    };
    color.set_if_neq(TextColor(tint));
}
