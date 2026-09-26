//! The Dominant's wish (docs/design.md §7).
//!
//! When the human wears the Crown at dawn, a panel in the middle asks for a
//! wish. With the gods' voice up (`oracle.rs`), the human picks a god and
//! writes the wish in their own words; the model reads it and the god
//! answers. Without it, or on request, the prepared wishes are offered as
//! buttons. The grade is not shown in advance: the god's answer tells it.
//!
//! Every wish, the bots' too, is answered in a panel for a few seconds: who
//! asked whom for what, the grade in stars, the god's words and what came of
//! it.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use necromy_rules::{God, Intent, PlayerId, WishKind};

use crate::hud::{INK, PANEL, UiFont};
use crate::names;
use crate::oracle::{Hearing, OracleLink, OracleOnline, Voices};
use crate::play::Match;
use crate::stats::{self, StatArt};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
const VOICE: Color = Color::srgb(0.85, 0.80, 0.95);
/// How long a god's answer stays on screen.
const REPLY_SECS: f32 = 7.0;
/// Longest wish the field accepts, in characters.
const MAX_WISH: usize = 200;

pub struct WishUiPlugin;

impl Plugin for WishUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WishDraft>()
            .add_systems(Startup, spawn)
            .add_systems(Update, (dev_wish, type_wish, buttons))
            .add_systems(
                Update,
                rebuild_panel.after(type_wish).after(buttons).run_if(
                    resource_changed::<Match>
                        .or_else(resource_changed::<WishDraft>)
                        .or_else(resource_changed::<OracleOnline>)
                        .or_else(resource_changed::<OracleLink>)
                        .or_else(resource_changed::<Hearing>),
                ),
            )
            .add_systems(
                Update,
                rebuild_reply.run_if(resource_changed::<Match>.or_else(resource_changed::<Voices>)),
            )
            .add_systems(Update, (expire_reply, listening_dots));
    }
}

/// The wish being put together in the panel.
#[derive(Resource, Default)]
struct WishDraft {
    god: Option<God>,
    kind: Option<WishKind>,
    target: Option<PlayerId>,
    /// Free words, when the gods' voice is up.
    text: String,
    /// The human chose the prepared wishes even with the voice up.
    prepared: bool,
}

#[derive(Component)]
struct WishPanel;

#[derive(Component)]
struct ReplyPanel;

/// "Тришна слушает…", animated while the model thinks.
#[derive(Component)]
struct Listening(God);

#[derive(Component, Clone, Copy)]
enum WishButton {
    God(God),
    Kind(WishKind),
    Target(PlayerId),
    Make,
    Refuse,
    /// Switch between free words and the prepared wishes.
    Prepared(bool),
}

fn spawn(mut commands: Commands) {
    for (wish, z) in [(true, 15), (false, 12)] {
        let node = Node {
            position_type: PositionType::Absolute,
            top: px(90.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        };
        let mut e = commands.spawn((node, GlobalZIndex(z), Visibility::Hidden));
        if wish {
            e.insert(WishPanel);
        } else {
            e.insert(ReplyPanel);
        }
    }
}

fn button(
    commands: &mut Commands,
    font: &UiFont,
    action: WishButton,
    text: &str,
    on: bool,
) -> Entity {
    let b = commands
        .spawn((
            action,
            Button,
            Node {
                padding: UiRect::axes(px(8.0), px(4.0)),
                border: UiRect::all(px(2.0)),
                align_items: AlignItems::Center,
                column_gap: px(4.0),
                ..default()
            },
            BorderColor::all(if on { GOLD } else { GOLD.with_alpha(0.25) }),
            BackgroundColor(if on {
                Color::srgba(0.35, 0.26, 0.1, 0.95)
            } else {
                Color::srgba(0.15, 0.12, 0.15, 0.9)
            }),
        ))
        .id();
    let t = stats::label(commands, font, text, 13.0, on);
    commands.entity(b).add_child(t);
    b
}

fn text_block(
    commands: &mut Commands,
    font: &UiFont,
    text: &str,
    size: f32,
    color: Color,
) -> Entity {
    commands
        .spawn((
            Text::new(text.to_string()),
            font.text(size),
            TextColor(color),
            Node {
                width: px(610.0),
                ..default()
            },
        ))
        .id()
}

/// Is the human writing a wish right now (panel up, free words, not sent)?
fn writing(game: &Match, draft: &WishDraft, link: &OracleLink) -> bool {
    game.game.wish_due() == Some(game.human)
        && !(game.autoplay && !game.paused_for_wish_panel())
        && link.online()
        && !draft.prepared
        && link.judging().is_none()
}

#[allow(clippy::too_many_arguments)]
fn rebuild_panel(
    mut commands: Commands,
    game: Res<Match>,
    draft: Res<WishDraft>,
    link: Res<OracleLink>,
    hearing: Res<Hearing>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<WishPanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let g = &game.game;
    if g.wish_due() != Some(game.human) || (game.autoplay && !game.paused_for_wish_panel()) {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let free = link.online() && !draft.prepared;

    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8.0),
                padding: UiRect::all(px(14.0)),
                border: UiRect::all(px(2.0)),
                width: px(640.0),
                ..default()
            },
            BorderColor::all(GOLD),
            BackgroundColor(PANEL.with_alpha(1.0)),
        ))
        .id();
    let mut rows = Vec::new();

    let title = stats::row(&mut commands);
    let crown = stats::icon_node(
        &mut commands,
        art.icon(crate::icons::StatIcon::Crown),
        28.0,
        true,
    );
    let text = stats::label(
        &mut commands,
        &font,
        "Венец у тебя: загадай желание",
        18.0,
        true,
    );
    commands.entity(title).add_children(&[crown, text]);
    rows.push(title);

    // The god is thinking: show the words and wait.
    if let Some(god) = link.judging() {
        let words = text_block(
            &mut commands,
            &font,
            &format!("«{}»", draft.text),
            14.0,
            INK,
        );
        let listening = commands
            .spawn((
                Listening(god),
                Text::new(format!("{} слушает…", names::god(god))),
                font.bold(14.0),
                TextColor(VOICE),
            ))
            .id();
        rows.extend([words, listening]);
        commands.entity(frame).add_children(&rows);
        commands.entity(panel).add_child(frame);
        return;
    }

    let hint = if free {
        "Выбери бога и напиши желание своими словами. Бог исполнит по-своему: изощрённое вознаградит Стилем, грубое исполнит урезанно и с проклятием."
    } else {
        "Бог исполнит по-своему. Изощрённое желание вознаграждается Стилем, грубое исполнится урезанно и с проклятием."
    };
    rows.push(text_block(&mut commands, &font, hint, 12.0, DIM));
    if let Some(why) = &hearing.failed {
        rows.push(text_block(
            &mut commands,
            &font,
            &format!("Прошлое желание не услышано: {why}. Попробуй ещё раз или выбери заготовку."),
            12.0,
            Color::srgb(0.95, 0.5, 0.4),
        ));
    }

    // Gods.
    let gods = commands
        .spawn(Node {
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(6.0),
            row_gap: px(6.0),
            ..default()
        })
        .id();
    for god in God::ALL {
        let b = button(
            &mut commands,
            &font,
            WishButton::God(god),
            &format!("{} · {}", names::god(god), names::stage(god, g.stage(god))),
            draft.god == Some(god),
        );
        let icon = stats::icon_node(&mut commands, art.gods[god.index()].clone(), 20.0, true);
        commands.entity(b).insert_children(0, &[icon]);
        commands.entity(gods).add_child(b);
    }
    rows.push(gods);
    if let Some(god) = draft.god {
        rows.push(text_block(
            &mut commands,
            &font,
            names::god_likes(god),
            12.0,
            VOICE,
        ));
    }

    if free {
        // The words, typed straight in.
        let field = commands
            .spawn((
                Node {
                    padding: UiRect::all(px(8.0)),
                    border: UiRect::all(px(2.0)),
                    min_height: px(44.0),
                    width: px(610.0),
                    ..default()
                },
                BorderColor::all(GOLD.with_alpha(0.6)),
                BackgroundColor(Color::srgba(0.05, 0.04, 0.07, 1.0)),
            ))
            .id();
        let (shown, color) = if draft.text.is_empty() {
            (
                "Например: «накорми меня досыта перед боем»▏".to_string(),
                DIM,
            )
        } else {
            (format!("{}▏", draft.text), INK)
        };
        let words = commands
            .spawn((
                Text::new(shown),
                font.text(15.0),
                TextColor(color),
                Node {
                    width: px(590.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(field).add_child(words);
        rows.push(field);
    } else {
        // Prepared wishes.
        let kinds = commands
            .spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6.0),
                row_gap: px(6.0),
                ..default()
            })
            .id();
        for kind in WishKind::ALL {
            let text = if kind.is_crude() {
                format!("«{}» · грубое", names::wish(kind))
            } else {
                format!("«{}»", names::wish(kind))
            };
            let b = button(
                &mut commands,
                &font,
                WishButton::Kind(kind),
                &text,
                draft.kind == Some(kind),
            );
            commands.entity(kinds).add_child(b);
        }
        rows.push(kinds);

        if draft.kind.is_some_and(WishKind::needs_target) {
            let rivals = stats::row(&mut commands);
            let label = stats::label(&mut commands, &font, "Кого:", 13.0, false);
            commands.entity(rivals).add_child(label);
            for p in g.players().filter(|&p| p != game.human) {
                let b = button(
                    &mut commands,
                    &font,
                    WishButton::Target(p),
                    &game.name(p),
                    draft.target == Some(p),
                );
                let face = commands
                    .spawn((
                        ImageNode::new(art.portraits[p.0 as usize].clone()),
                        Node {
                            width: px(12.0),
                            height: px(18.0),
                            ..default()
                        },
                    ))
                    .id();
                commands.entity(b).insert_children(0, &[face]);
                commands.entity(rivals).add_child(b);
            }
            rows.push(rivals);
        }
    }

    // Make, switch, refuse.
    let ready = draft.god.is_some()
        && if free {
            !draft.text.trim().is_empty()
        } else {
            draft.kind.is_some()
                && (draft.target.is_some() || !draft.kind.is_some_and(WishKind::needs_target))
        };
    let actions = stats::row(&mut commands);
    let make_label = if free {
        "Загадать (Enter)"
    } else {
        "Загадать"
    };
    let make = button(&mut commands, &font, WishButton::Make, make_label, ready);
    commands.entity(actions).add_child(make);
    if link.online() {
        let (switch, label) = if free {
            (true, "Заготовки")
        } else {
            (false, "Своими словами")
        };
        let b = button(
            &mut commands,
            &font,
            WishButton::Prepared(switch),
            label,
            false,
        );
        commands.entity(actions).add_child(b);
    }
    let refuse = button(
        &mut commands,
        &font,
        WishButton::Refuse,
        "Отказаться от желания",
        false,
    );
    commands.entity(actions).add_child(refuse);
    rows.push(actions);
    if !link.online() {
        rows.push(text_block(
            &mut commands,
            &font,
            "Голос богов не отвечает (scripts/oracle-server.sh): только заготовки.",
            11.0,
            DIM,
        ));
    }
    if matches!(
        g.secret(game.human),
        Some(necromy_rules::Condition::Wager { .. })
    ) {
        let note = stats::label(
            &mut commands,
            &font,
            "Твоё тайное — Пари Ахамара: отказ засчитывается.",
            12.0,
            false,
        );
        rows.push(note);
    }

    commands.entity(frame).add_children(&rows);
    commands.entity(panel).add_child(frame);
}

/// Dev aid: `NECROMY_WISH=<god index>:<words>` writes the human's first wish
/// once the gods' voice is up, e.g. `NECROMY_WISH="1:накорми меня перед боем"`
/// with `NECROMY_AUTOPLAY=1 NECROMY_SCREENSHOT_WHEN=reply`.
fn dev_wish(
    mut done: Local<bool>,
    mut draft: ResMut<WishDraft>,
    mut link: ResMut<OracleLink>,
    game: Res<Match>,
) {
    if *done || !writing(&game, &draft, &link) {
        return;
    }
    *done = true;
    let Some((god, text)) = std::env::var("NECROMY_WISH").ok().and_then(|w| {
        w.split_once(':')
            .map(|(g, t)| (g.to_string(), t.to_string()))
    }) else {
        return;
    };
    let Some(god) = god
        .parse::<usize>()
        .ok()
        .and_then(|i| God::ALL.get(i).copied())
    else {
        return;
    };
    draft.god = Some(god);
    draft.text = text.clone();
    link.ask_wish(&game, god, &text);
}

/// Keys go into the wish while the human writes one; Enter sends it.
fn type_wish(
    mut keys: MessageReader<KeyboardInput>,
    mut draft: ResMut<WishDraft>,
    mut link: ResMut<OracleLink>,
    mut hearing: ResMut<Hearing>,
    game: Res<Match>,
) {
    if !writing(&game, &draft, &link) {
        keys.clear();
        return;
    }
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Backspace => {
                draft.text.pop();
            }
            Key::Enter => {
                if let Some(god) = draft.god
                    && !draft.text.trim().is_empty()
                {
                    hearing.failed = None;
                    let text = draft.text.clone();
                    link.ask_wish(&game, god, &text);
                }
            }
            _ => {
                if let Some(text) = &key.text
                    && text.chars().all(|c| !c.is_control())
                    && draft.text.chars().count() < MAX_WISH
                {
                    draft.text.push_str(text);
                }
            }
        }
    }
}

fn buttons(
    pressed: Query<(&Interaction, &WishButton), Changed<Interaction>>,
    mut draft: ResMut<WishDraft>,
    mut link: ResMut<OracleLink>,
    mut hearing: ResMut<Hearing>,
    mut game: ResMut<Match>,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let human = game.human;
        match *button {
            WishButton::God(god) => draft.god = Some(god),
            WishButton::Kind(kind) => {
                draft.kind = Some(kind);
                if !kind.needs_target() {
                    draft.target = None;
                }
            }
            WishButton::Target(p) => draft.target = Some(p),
            WishButton::Prepared(on) => draft.prepared = on,
            WishButton::Make => {
                let Some(god) = draft.god else { continue };
                if link.online() && !draft.prepared {
                    if !draft.text.trim().is_empty() && link.judging().is_none() {
                        hearing.failed = None;
                        let text = draft.text.clone();
                        link.ask_wish(&game, god, &text);
                    }
                    continue;
                }
                let Some(kind) = draft.kind else { continue };
                if game
                    .act(
                        human,
                        Intent::Wish {
                            god,
                            kind,
                            target: draft.target,
                            said: None,
                        },
                    )
                    .is_ok()
                {
                    *draft = WishDraft::default();
                }
            }
            WishButton::Refuse => {
                if game.act(human, Intent::RefuseWish).is_ok() {
                    *draft = WishDraft::default();
                }
            }
        }
    }
    // A wish that went through clears the draft for the next dawn.
    let done = game.game.wish_due() != Some(game.human)
        && link.judging().is_none()
        && (!draft.text.is_empty() || draft.god.is_some());
    if done {
        *draft = WishDraft::default();
    }
}

fn listening_dots(time: Res<Time>, mut texts: Query<(&Listening, &mut Text)>) {
    let dots = ".".repeat(1 + (time.elapsed_secs() * 2.0) as usize % 3);
    for (listening, mut text) in &mut texts {
        let wanted = format!("{} слушает{dots}", names::god(listening.0));
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

/// Who asked whom for what, the grade, the god's words, what came of it.
fn rebuild_reply(
    mut commands: Commands,
    game: Res<Match>,
    voices: Res<Voices>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<ReplyPanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let Some(reply) = game.wish_reply.as_ref() else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);

    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4.0),
                padding: UiRect::all(px(12.0)),
                border: UiRect::all(px(2.0)),
                width: px(540.0),
                ..default()
            },
            BorderColor::all(GOLD),
            BackgroundColor(PANEL.with_alpha(0.97)),
        ))
        .id();
    let mut rows = Vec::new();
    let block = |commands: &mut Commands, text: String, size: f32, color: Color| {
        commands
            .spawn((
                Text::new(text),
                font.text(size),
                TextColor(color),
                Node {
                    width: px(510.0),
                    ..default()
                },
            ))
            .id()
    };
    match reply.wish {
        Some((god, kind, grade)) => {
            let head = stats::row(&mut commands);
            let icon = stats::icon_node(&mut commands, art.gods[god.index()].clone(), 28.0, true);
            let who = stats::label(
                &mut commands,
                &font,
                &format!(
                    "{} просит {}",
                    game.name(reply.player),
                    names::god_accusative(god)
                ),
                15.0,
                true,
            );
            let stars = stats::label(
                &mut commands,
                &font,
                &format!(
                    "{}{}",
                    "★".repeat(grade as usize),
                    "☆".repeat(3 - grade as usize)
                ),
                16.0,
                true,
            );
            commands.entity(head).add_children(&[icon, who, stars]);
            rows.push(head);
            // The words: the player's own, or the prepared phrase.
            let asked = match &reply.said {
                Some(said) => format!("«{}» — понято как «{}»", said.text, names::wish(kind)),
                None => format!("«{}»", names::wish(kind)),
            };
            rows.push(block(&mut commands, asked, 14.0, INK));
            // The god's answer: the model's, then any written later, then the template.
            let speech = reply
                .said
                .as_ref()
                .map(|s| s.speech.clone())
                .filter(|s| !s.is_empty())
                .or_else(|| voices.wishes.get(&game.wish_serial).cloned())
                .unwrap_or_else(|| names::god_speech(god, grade).to_string());
            rows.push(block(
                &mut commands,
                format!("{}: «{speech}»", names::god(god)),
                13.0,
                VOICE,
            ));
            if let Some(said) = &reply.said
                && !said.reason.is_empty()
            {
                rows.push(block(
                    &mut commands,
                    format!("Оценка: {}", said.reason),
                    12.0,
                    DIM,
                ));
            }
        }
        None => {
            let text = stats::label(
                &mut commands,
                &font,
                &format!("{} отказывается от желания.", game.name(reply.player)),
                15.0,
                true,
            );
            rows.push(text);
        }
    }
    for line in &reply.lines {
        rows.push(block(&mut commands, format!("→ {line}"), 12.0, INK));
    }
    commands.entity(frame).add_children(&rows);
    commands.entity(panel).add_child(frame);
}

fn expire_reply(time: Res<Time>, mut shown: Local<(u32, f32)>, mut game: ResMut<Match>) {
    if game.wish_reply.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if shown.0 != game.wish_serial {
        *shown = (game.wish_serial, now);
    }
    if now - shown.1 > REPLY_SECS {
        game.wish_reply = None;
    }
}
