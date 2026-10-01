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
const STEP: Color = Color::srgb(1.0, 0.80, 0.35);
/// How long the "your turn" splash stays, fading out.
const SPLASH_SECS: f32 = 1.6;

pub struct TurnUiPlugin;

impl Plugin for TurnUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlaceMenu>()
            .add_systems(Startup, (spawn, spawn_clock))
            .add_systems(
                crate::InGame,
                (rebuild_status, rebuild_action).run_if(
                    resource_changed::<Match>
                        .or_else(resource_changed::<Selection>)
                        .or_else(resource_changed::<PlaceMenu>),
                ),
            )
            .add_systems(
                crate::InGame,
                (taste_tip, action_buttons, splash, show_clock),
            );
    }
}

/// Whether the list of what this spot offers is open under the bar
/// (`NECROMY_PLACE_MENU=open` opens it from the start, for screenshots).
#[derive(Resource)]
struct PlaceMenu(bool);

impl Default for PlaceMenu {
    fn default() -> Self {
        PlaceMenu(std::env::var("NECROMY_PLACE_MENU").is_ok_and(|v| v == "open"))
    }
}

#[derive(Component)]
pub(crate) struct Status;

#[derive(Component)]
struct TasteChip;

#[derive(Component)]
struct TasteTip;

#[derive(Component)]
pub(crate) struct ActionBar;

#[derive(Component)]
struct Splash;

/// Seconds left on the human's clock (server matches only, §17.1).
#[derive(Component)]
pub(crate) struct ClockChip;

#[derive(Component, Clone, Copy, PartialEq)]
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
    /// Open or close the list of what this spot offers.
    PlaceMenu,
    /// Build on the settlement underfoot, or grow its city (§21.8).
    Build(necromy_rules::Building),
    /// Tame the beast or enlist the undead next to you (§21.8).
    Recruit(u32),
    /// A road on the hex underfoot (§21.8).
    Pave,
    /// Sow the plains underfoot; hold a feast.
    Sow,
    Feast,
    /// Open a fair in the settlement underfoot.
    Fair,
    /// A circle on the stones underfoot.
    DrawCircle,
    /// Work at a way down.
    Delve(necromy_rules::DelveWork),
    /// Arena and debts: a challenge, a bet, a debt paid.
    Challenge(necromy_rules::PlayerId),
    BetOn(necromy_rules::PlayerId),
    PayDebt(necromy_rules::PlayerId),
    /// Tether a beast in one's pen; untie one in a rival's.
    Tether(necromy_rules::Element),
    Untether(necromy_rules::Element),
    /// Burial: a graveyard, a pit, a pit settled.
    Consecrate,
    DigPit,
    SettlePit(bool),
    /// Rulers: a gift, a betrothal, the crown, discord.
    Gift(hexx::Hex),
    Betroth(hexx::Hex, hexx::Hex),
    Coronation,
    Discord,
    /// Set a hex beside you alight, or put a fire out.
    Kindle(hexx::Hex),
    Douse(hexx::Hex),
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
            // The bar, and under it what this spot offers.
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6.0),
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
    place: Res<PlaceMenu>,
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
            Some(k) if crate::battle_ui::is_fight(&k) => (String::new(), "", None, false),
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
    // On the human's own turn, the next step of their deed (the path).
    let step = (moves && game.gate.is_none())
        .then(|| g.next_step(human))
        .flatten();
    if let Some(step) = &step {
        let (what, _) = names::step(g, human, step);
        let next = commands
            .spawn((
                Text::new(format!("Дальше: {what}")),
                font.bold(12.0),
                TextColor(STEP),
                Node {
                    width: px(250.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(texts).add_children(&[main, next, hint]);
    } else {
        commands.entity(texts).add_children(&[main, hint]);
    }
    commands.entity(panel).add_child(texts);

    // What this spot offers besides (§21.8): a burden to take or lay down,
    // a building, a gift... A row of their own under the bar, so the bar
    // keeps its size. Not in the tutorial: it lets only its step through.
    let primary = button.map(|(a, l)| (a, l.to_string()));
    let mut buttons: Vec<(ActionButton, String)> = Vec::new();
    if game.is_human_turn()
        && selection.card.is_none()
        && selection.sift.is_none()
        && game.gate.is_none()
    {
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
        for id in g.recruitable(human) {
            let Some(m) = g.mobs().iter().find(|m| m.id == id) else {
                continue;
            };
            let label = if m.is_undead() {
                format!("В легион\n{} Духа", necromy_rules::ENLIST_SPIRIT)
            } else {
                format!("Приручить\n{} Духа", necromy_rules::TAME_SPIRIT)
            };
            buttons.push((ActionButton::Recruit(id), label));
        }
        if g.may_pave(human) && spirit >= necromy_rules::PAVE_SPIRIT {
            buttons.push((
                ActionButton::Pave,
                format!("Замостить\n{} Духа", necromy_rules::PAVE_SPIRIT),
            ));
        }
        if spirit >= necromy_rules::DOUSE_SPIRIT
            && let Some(&hex) = g.dousable(human).first()
        {
            buttons.push((
                ActionButton::Douse(hex),
                format!("Потушить\n{} Духа", necromy_rules::DOUSE_SPIRIT),
            ));
        }
        if spirit >= necromy_rules::KINDLE_SPIRIT
            && let Some(&hex) = g.kindleable(human).first()
        {
            buttons.push((
                ActionButton::Kindle(hex),
                format!("Поджечь\n{} Духа", necromy_rules::KINDLE_SPIRIT),
            ));
        }
        for w in g.delve_work(human) {
            buttons.push((ActionButton::Delve(w), names::delve_work(w).to_string()));
        }
        if let Some(&rival) = g.challengeable(human).first() {
            buttons.push((
                ActionButton::Challenge(rival),
                format!(
                    "Вызвать\n{}",
                    names::god_accusative(
                        g.champion(rival)
                            .map_or(necromy_rules::God::Ahamar, |c| c.god)
                    )
                ),
            ));
        }
        if let Some(d) = g.debts().iter().find(|d| d.debtor == human) {
            buttons.push((
                ActionButton::PayDebt(d.creditor),
                format!("Отдать долг\n{} Стиля", d.amount),
            ));
        }
        if let Some(&rival) = g.bettable(human).first() {
            buttons.push((
                ActionButton::BetOn(rival),
                format!(
                    "Пари: {}\nвступит в бой",
                    names::god(
                        g.champion(rival)
                            .map_or(necromy_rules::God::Ahamar, |c| c.god)
                    )
                ),
            ));
        }
        if let Some(&e) = g.tetherable(human).first() {
            buttons.push((ActionButton::Tether(e), "Привязать\nзверя".to_string()));
        }
        if let Some(&e) = g.untetherable(human).first() {
            buttons.push((ActionButton::Untether(e), "Увести\nзверя".to_string()));
        }
        if g.may_draw_circle(human) && spirit >= necromy_rules::CIRCLE_SPIRIT {
            buttons.push((
                ActionButton::DrawCircle,
                format!("Начертить круг\n{} Духа", necromy_rules::CIRCLE_SPIRIT),
            ));
        }
        if g.may_settle_pit(human) {
            buttons.push((ActionButton::SettlePit(false), "Упокоить\nяму".to_string()));
            buttons.push((
                ActionButton::SettlePit(true),
                "Поднять\nмертвецов".to_string(),
            ));
        }
        if g.may_consecrate(human) && spirit >= necromy_rules::CONSECRATE_SPIRIT {
            buttons.push((ActionButton::Consecrate, "Освятить\nкладбище".to_string()));
            buttons.push((ActionButton::DigPit, "Вырыть\nяму".to_string()));
        }
        if g.may_crown(human) {
            buttons.push((ActionButton::Coronation, "Коронация\nна Столе".to_string()));
        }
        if g.may_sow_discord(human) {
            buttons.push((ActionButton::Discord, "Посеять\nраздор".to_string()));
        }
        if let Some(&(a, b)) = g.matches(human).first() {
            buttons.push((
                ActionButton::Betroth(a, b),
                "Сосватать\nправителей".to_string(),
            ));
        }
        let carries = matches!(
            g.cargo(human),
            Some(necromy_rules::Cargo::Food | necromy_rules::Cargo::Goods(_))
        );
        if (spirit >= 1 || carries)
            && let Some(&hex) = g.giftable(human).first()
        {
            let what = if carries { "ношу" } else { "1 Дух" };
            buttons.push((ActionButton::Gift(hex), format!("Дар правителю\n{what}")));
        }
        if g.may_open_fair(human) && spirit >= necromy_rules::FAIR_SPIRIT {
            buttons.push((
                ActionButton::Fair,
                format!("Ярмарка\n{} Духа", necromy_rules::FAIR_SPIRIT),
            ));
        }
        if g.may_feast(human) {
            let guests = g.guests(human).len();
            buttons.push((ActionButton::Feast, format!("Пир\nгостей: {guests}")));
        }
        if g.may_sow(human) && spirit >= necromy_rules::SOW_SPIRIT {
            buttons.push((
                ActionButton::Sow,
                format!("Засеять\n{} Духа", necromy_rules::SOW_SPIRIT),
            ));
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
    // What this spot offers stays behind one button; the list opens under
    // the bar, over what lies there, until an action or the button again.
    let mut in_bar: Vec<(ActionButton, String)> = Vec::new();
    // The move the path asks for here comes out of the list, first.
    if let Some(necromy_rules::path::StepWhat::Do(intent)) = step.as_ref().map(|s| &s.what)
        && let Some(i) = buttons
            .iter()
            .position(|(b, _)| button_of(intent) == Some(*b))
    {
        in_bar.push(buttons.remove(i));
    }
    in_bar.extend(primary);
    if !buttons.is_empty() {
        let arrow = if place.0 { "▴" } else { "▾" };
        in_bar.push((
            ActionButton::PlaceMenu,
            format!("Здесь: {} {arrow}\nдействия клетки", buttons.len()),
        ));
    }
    for (action, label) in in_bar {
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
    if buttons.is_empty() || !place.0 {
        return;
    }
    let extras = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                row_gap: px(4.0),
                padding: UiRect::all(px(10.0)),
                ..default()
            },
            Frame::Panel,
            // Over the wish strip and the deed cards while open.
            GlobalZIndex(30),
        ))
        .id();
    for (action, label) in buttons {
        let b = commands
            .spawn((
                action,
                Button,
                Node {
                    padding: UiRect::axes(px(10.0), px(5.0)),
                    // Wrap to the next line rather than squeeze.
                    flex_shrink: 0.0,
                    ..default()
                },
                Frame::Button,
            ))
            .id();
        // One line, small: many may stand here at once.
        let t = commands
            .spawn((
                Text::new(label.replace('\n', " · ")),
                font.bold(12.0),
                TextColor(INK),
                TextLayout::new(Justify::Center, LineBreak::NoWrap),
            ))
            .id();
        commands.entity(b).add_child(t);
        commands.entity(extras).add_child(b);
    }
    commands.entity(bar).add_child(extras);
}

fn action_buttons(
    pressed: Query<(&Interaction, &ActionButton), Changed<Interaction>>,
    mut game: ResMut<Match>,
    mut selection: ResMut<Selection>,
    mut place: ResMut<PlaceMenu>,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let ActionButton::PlaceMenu = button {
            place.0 = !place.0;
            continue;
        }
        if place.0 {
            place.0 = false;
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
            ActionButton::Recruit(mob) => Intent::Recruit { mob: *mob },
            ActionButton::Pave => Intent::Pave,
            ActionButton::Sow => Intent::Sow,
            ActionButton::Feast => Intent::Feast,
            ActionButton::Fair => Intent::Fair,
            ActionButton::DrawCircle => Intent::DrawCircle,
            ActionButton::Delve(work) => Intent::Delve { work: *work },
            ActionButton::Challenge(rival) => Intent::Challenge { rival: *rival },
            ActionButton::BetOn(rival) => Intent::BetOn {
                rival: *rival,
                bet: necromy_rules::Bet::Fight,
            },
            ActionButton::PayDebt(creditor) => Intent::PayDebt {
                creditor: *creditor,
            },
            ActionButton::Tether(element) => Intent::Tether { element: *element },
            ActionButton::Untether(element) => Intent::Untether { element: *element },
            ActionButton::Consecrate => Intent::Consecrate,
            ActionButton::DigPit => Intent::DigPit,
            ActionButton::SettlePit(raise) => Intent::SettlePit { raise: *raise },
            ActionButton::Gift(hex) => Intent::Gift { hex: *hex },
            ActionButton::Betroth(a, b) => Intent::Betroth { a: *a, b: *b },
            ActionButton::Coronation => Intent::Coronation,
            ActionButton::Discord => Intent::Discord,
            ActionButton::Kindle(hex) => Intent::Kindle { hex: *hex },
            ActionButton::Douse(hex) => Intent::Douse { hex: *hex },
            ActionButton::Cycle => Intent::Cycle {
                cards: selection.sift.take().unwrap_or_default(),
            },
            ActionButton::Pass => {
                selection.card = None;
                Intent::Pass
            }
            ActionButton::PlaceMenu => continue,
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

/// The button of the action bar that sends `intent`, if one does.
fn button_of(intent: &Intent) -> Option<ActionButton> {
    Some(match *intent {
        Intent::Rebuild => ActionButton::Rebuild,
        Intent::Take => ActionButton::Take,
        Intent::Lay => ActionButton::Lay,
        Intent::Build { building } => ActionButton::Build(building),
        Intent::Quarter { hex } => ActionButton::Quarter(hex),
        Intent::Recruit { mob } => ActionButton::Recruit(mob),
        Intent::Pave => ActionButton::Pave,
        Intent::Sow => ActionButton::Sow,
        Intent::Feast => ActionButton::Feast,
        Intent::Fair => ActionButton::Fair,
        Intent::DrawCircle => ActionButton::DrawCircle,
        Intent::Delve { work } => ActionButton::Delve(work),
        Intent::Challenge { rival } => ActionButton::Challenge(rival),
        Intent::BetOn { rival, .. } => ActionButton::BetOn(rival),
        Intent::Tether { element } => ActionButton::Tether(element),
        Intent::Consecrate => ActionButton::Consecrate,
        Intent::DigPit => ActionButton::DigPit,
        Intent::SettlePit { raise } => ActionButton::SettlePit(raise),
        Intent::Gift { hex } => ActionButton::Gift(hex),
        Intent::Betroth { a, b } => ActionButton::Betroth(a, b),
        Intent::Coronation => ActionButton::Coronation,
        Intent::Discord => ActionButton::Discord,
        Intent::Kindle { hex } => ActionButton::Kindle(hex),
        Intent::Douse { hex } => ActionButton::Douse(hex),
        _ => return None,
    })
}
