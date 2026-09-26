//! The Dominant's wish (docs/design.md §7), offline.
//!
//! When the human wears the Crown at dawn, a panel in the middle asks for a
//! wish: pick a god (with what it likes), a prepared wish, a rival if the
//! wish needs one; or refuse. The grade is not shown in advance: the god's
//! answer tells it. Every wish, the bots' too, is answered in a panel for a
//! few seconds: who asked whom for what, the grade in stars, the god's words
//! and what came of it.

use bevy::prelude::*;
use necromy_rules::{God, Intent, PlayerId, WishKind};

use crate::hud::{INK, PANEL, UiFont};
use crate::names;
use crate::play::Match;
use crate::stats::{self, StatArt};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
/// How long a god's answer stays on screen.
const REPLY_SECS: f32 = 5.0;

pub struct WishUiPlugin;

impl Plugin for WishUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WishDraft>()
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                rebuild_panel
                    .run_if(resource_changed::<Match>.or_else(resource_changed::<WishDraft>)),
            )
            .add_systems(Update, rebuild_reply.run_if(resource_changed::<Match>))
            .add_systems(Update, (buttons, expire_reply));
    }
}

/// The wish being put together in the panel.
#[derive(Resource, Default)]
struct WishDraft {
    god: Option<God>,
    kind: Option<WishKind>,
    target: Option<PlayerId>,
}

#[derive(Component)]
struct WishPanel;

#[derive(Component)]
struct ReplyPanel;

#[derive(Component, Clone, Copy)]
enum WishButton {
    God(God),
    Kind(WishKind),
    Target(PlayerId),
    Make,
    Refuse,
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        WishPanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(90.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        GlobalZIndex(15),
        Visibility::Hidden,
    ));
    commands.spawn((
        ReplyPanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(90.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        GlobalZIndex(12),
        Visibility::Hidden,
    ));
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

fn rebuild_panel(
    mut commands: Commands,
    game: Res<Match>,
    draft: Res<WishDraft>,
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
    let hint = commands
        .spawn((
            Text::new("Бог исполнит по-своему. Изощрённое желание вознаграждается Стилем, грубое исполнится урезанно и с проклятием."),
            font.text(12.0),
            TextColor(DIM),
            Node {
                width: px(610.0),
                ..default()
            },
        ))
        .id();
    rows.push(hint);

    // Gods.
    let gods = stats::row(&mut commands);
    for god in God::ALL {
        let on = draft.god == Some(god);
        let b = button(
            &mut commands,
            &font,
            WishButton::God(god),
            &format!("{} · {}", names::god(god), names::stage(god, g.stage(god))),
            on,
        );
        let icon = stats::icon_node(&mut commands, art.gods[god.index()].clone(), 20.0, true);
        commands.entity(b).insert_children(0, &[icon]);
        commands.entity(gods).add_child(b);
    }
    commands.entity(gods).insert(Node {
        flex_wrap: FlexWrap::Wrap,
        column_gap: px(6.0),
        row_gap: px(6.0),
        ..default()
    });
    rows.push(gods);
    if let Some(god) = draft.god {
        let likes = commands
            .spawn((
                Text::new(names::god_likes(god)),
                font.text(12.0),
                TextColor(Color::srgb(0.85, 0.80, 0.95)),
                Node {
                    width: px(610.0),
                    ..default()
                },
            ))
            .id();
        rows.push(likes);
    }

    // Wishes.
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

    // A rival, for wishes that need one.
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

    // Make or refuse.
    let ready = draft.god.is_some()
        && draft.kind.is_some()
        && (draft.target.is_some() || !draft.kind.is_some_and(WishKind::needs_target));
    let actions = stats::row(&mut commands);
    let make = button(&mut commands, &font, WishButton::Make, "Загадать", ready);
    let refuse = button(
        &mut commands,
        &font,
        WishButton::Refuse,
        "Отказаться от желания",
        false,
    );
    commands.entity(actions).add_children(&[make, refuse]);
    rows.push(actions);
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

fn buttons(
    pressed: Query<(&Interaction, &WishButton), Changed<Interaction>>,
    mut draft: ResMut<WishDraft>,
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
            WishButton::Make => {
                let (Some(god), Some(kind)) = (draft.god, draft.kind) else {
                    continue;
                };
                if game
                    .act(
                        human,
                        Intent::Wish {
                            god,
                            kind,
                            target: draft.target,
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
}

/// Who asked whom for what, the grade, the god's words, what came of it.
fn rebuild_reply(
    mut commands: Commands,
    game: Res<Match>,
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
                width: px(520.0),
                ..default()
            },
            BorderColor::all(GOLD),
            BackgroundColor(PANEL.with_alpha(0.97)),
        ))
        .id();
    let mut rows = Vec::new();
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
            let asked = stats::label(
                &mut commands,
                &font,
                &format!("«{}»", names::wish(kind)),
                14.0,
                false,
            );
            rows.push(asked);
            let speech = commands
                .spawn((
                    Text::new(format!(
                        "{}: «{}»",
                        names::god(god),
                        names::god_speech(god, grade)
                    )),
                    font.text(13.0),
                    TextColor(Color::srgb(0.85, 0.80, 0.95)),
                    Node {
                        width: px(490.0),
                        ..default()
                    },
                ))
                .id();
            rows.push(speech);
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
        let l = commands
            .spawn((
                Text::new(format!("→ {line}")),
                font.text(12.0),
                TextColor(INK),
                Node {
                    width: px(490.0),
                    ..default()
                },
            ))
            .id();
        rows.push(l);
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
