//! Screen overlay: champions, the incoming card, tooltips and the human's hand
//! (the event feed is `feed.rs`).
//! Rebuilt from `Match` and `Selection` whenever either changes.

use bevy::prelude::*;
use bevy::text::{FontSize, FontSource};
use bevy::window::PrimaryWindow;
use necromy_rules::{CardId, Game, WindowKind};

use crate::board::{Board, Hovered};
use crate::card_art::{CARD_H, CARD_W, CardArt, CardLook, card_node};
use crate::deck::DeckSlot;
use crate::names;
use crate::play::{self, IncomingCountdown, Match, Selection};
use crate::stats::StatArt;
use crate::ui_skin::Frame;

/// How much of a card in hand hides below the screen until it is hovered:
/// the illustration, name and kind stay in sight.
pub const CARD_TUCK: f32 = CARD_H - 142.0;
/// Between the deck and the first card in hand.
const DECK_GAP: f32 = 20.0;
/// Room the hand leaves on each side for the sheet and the portraits.
const HAND_MARGIN: f32 = 280.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_font)
            .add_systems(Startup, spawn_hud)
            .add_systems(
                crate::InGame,
                rebuild_hand.run_if(
                    resource_changed::<Match>
                        .or_else(resource_changed::<Selection>)
                        .or_else(resource_changed::<crate::tutorial::Focus>),
                ),
            )
            .add_systems(
                crate::InGame,
                rebuild_incoming.run_if(
                    resource_changed::<Match>.or_else(resource_changed::<IncomingCountdown>),
                ),
            )
            .add_systems(
                crate::InGame,
                (
                    click_cards,
                    raise_cards,
                    tooltip,
                    skip_incoming,
                    expire_incoming_result,
                ),
            );
    }
}

/// Placeholder UI font with Cyrillic (assets/fonts/README.md).
#[derive(Resource)]
pub struct UiFont {
    regular: Handle<Font>,
    bold: Handle<Font>,
}

impl UiFont {
    pub fn text(&self, size: f32) -> TextFont {
        TextFont {
            font: FontSource::Handle(self.regular.clone()),
            font_size: FontSize::Px(size),
            ..default()
        }
    }

    pub fn bold(&self, size: f32) -> TextFont {
        TextFont {
            font: FontSource::Handle(self.bold.clone()),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

fn load_font(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(UiFont {
        regular: assets.load("fonts/DejaVuSans.ttf"),
        bold: assets.load("fonts/DejaVuSans-Bold.ttf"),
    });
}

#[derive(Component)]
struct Hand;

#[derive(Component)]
struct Tooltip;

/// The card aimed at someone in an open Target window.
#[derive(Component)]
struct Incoming;

#[derive(Component)]
pub struct HandCard(pub CardId);

/// The row of cards in hand, right of the deck.
#[derive(Component)]
struct HandCards;

pub const INK: Color = Color::srgb(0.95, 0.92, 0.85);

fn spawn_hud(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        Incoming,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(170.0),
            left: px(0.0),
            right: px(0.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4.0),
            ..default()
        },
        Visibility::Hidden,
    ));
    commands.spawn((
        Tooltip,
        Text::new(""),
        font.text(13.0),
        TextColor(INK),
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::all(px(8.0)),
            max_width: px(320.0),
            ..default()
        },
        Frame::Tip,
        Visibility::Hidden,
        // Above cards and panels.
        GlobalZIndex(10),
    ));
    commands
        .spawn((
            Hand,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(10.0 - CARD_TUCK),
                left: px(0.0),
                right: px(0.0),
                justify_content: JustifyContent::Center,
                column_gap: px(DECK_GAP),
                ..default()
            },
        ))
        .with_children(|hand| {
            hand.spawn((DeckSlot, Node::default()));
            hand.spawn((HandCards, Node::default()));
        });
}

#[allow(clippy::too_many_arguments)]
fn rebuild_hand(
    mut commands: Commands,
    game: Res<Match>,
    selection: Res<Selection>,
    focus: Res<crate::tutorial::Focus>,
    font: Res<UiFont>,
    card_art: Res<CardArt>,
    stat_art: Res<StatArt>,
    hand: Single<(Entity, &mut Node), With<HandCards>>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    let (hand, mut row) = hand.into_inner();
    commands.entity(hand).despawn_related::<Children>();
    // Cards overlap when they do not fit between the side panels.
    let n = game.game.hand(game.human).len() as f32;
    let room = window.width() - 2.0 * HAND_MARGIN - CARD_W - DECK_GAP;
    let gap = if n > 1.0 {
        ((room - n * CARD_W) / (n - 1.0)).clamp(-CARD_W * 0.6, 8.0)
    } else {
        8.0
    };
    row.column_gap = px(gap);
    let playable = game.game.playable(game.human);
    let burning = game.game.battle_dice(game.human).is_some() && game.human_awaited();
    for &card in game.game.hand(game.human) {
        let def = game.game.def(card);
        let usable = playable.contains(&card) || burning;
        let selected = selection.card == Some(card) || selection.burn.contains(&card);
        let look = CardLook {
            usable: usable || selected,
            // Gold when chosen; the tutorial's violet on the card it asks for.
            outline: selected.then_some(Color::srgb(1.0, 0.82, 0.3)).or_else(|| {
                (focus.card == Some(game.game.def(card).name))
                    .then_some(Color::srgb(0.75, 0.55, 1.0))
            }),
            extra: Vec::new(),
            badge: crate::ring_ui::badge(&game.game, game.human, card),
            cost: Some(game.game.cost_of(game.human, card)),
        };
        let entity = card_node(
            &mut commands,
            &font,
            &card_art,
            &stat_art,
            &game.game,
            def,
            look,
        );
        commands
            .entity(entity)
            .insert((HandCard(card), Button, ZIndex(0)));
        commands.entity(hand).add_child(entity);
    }
}

/// A card aimed at someone, shown whole above the hand: while its Target
/// window is open (who aims it, what it does now, how to answer, how long
/// until it lands), and for a moment after it hit the human (what it did).
fn rebuild_incoming(
    mut commands: Commands,
    game: Res<Match>,
    countdown: Res<IncomingCountdown>,
    font: Res<UiFont>,
    card_art: Res<CardArt>,
    stat_art: Res<StatArt>,
    incoming: Single<(Entity, &mut Visibility), With<Incoming>>,
) {
    let (panel, mut visibility) = incoming.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let g = &game.game;
    let red = Color::srgb(0.95, 0.25, 0.2);

    let (header, card, look, footer, button) = if let Some(WindowKind::Target {
        caster,
        target,
        card,
    }) = g.window_for(game.human).map(|w| w.kind)
    {
        let at_me = target == game.human;
        let header = if at_me {
            format!("{} целит в тебя:", game.name(caster))
        } else {
            format!("{} целит в {}:", game.name(caster), game.name(target))
        };
        let mut extra = Vec::new();
        if let Some(bonus) = g.pending_bonus(game.human).filter(|&b| b > 0) {
            extra.push((format!("цепочка: +{bonus}"), Color::srgb(1.0, 0.82, 0.3)));
        }
        let look = CardLook {
            usable: true,
            outline: at_me.then_some(red),
            extra,
            badge: None,
            cost: None,
        };
        let awaited = game.human_awaited();
        let footer = if !awaited {
            "Ждём ответов…".to_string()
        } else if let Some(left) = countdown.0 {
            format!("Ответить нечем. Сработает через {left:.0} с.")
        } else {
            let answers: Vec<&str> = g
                .playable(game.human)
                .into_iter()
                .map(|c| g.def(c).name)
                .collect();
            format!(
                "Ответить можно: {}. Клик по карте в руке или P — пас.",
                answers.join(", ")
            )
        };
        (header, card, look, footer, awaited && at_me)
    } else if let Some(hit) = game.incoming_result.as_ref() {
        let lines = hit.lines.iter().map(|l| (format!("→ {l}"), red)).collect();
        let look = CardLook {
            usable: true,
            outline: Some(red),
            extra: lines,
            badge: None,
            cost: None,
        };
        (
            format!("{} играет в тебя:", game.name(hit.caster)),
            hit.card,
            look,
            String::new(),
            false,
        )
    } else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);

    let backed = Node {
        padding: UiRect::axes(px(10.0), px(5.0)),
        ..default()
    };
    let header = commands
        .spawn((
            Text::new(header),
            font.bold(14.0),
            TextColor(INK),
            backed.clone(),
            Frame::Tip,
        ))
        .id();
    let card = card_node(
        &mut commands,
        &font,
        &card_art,
        &stat_art,
        g,
        g.def(card),
        look,
    );
    let mut children = vec![header, card];
    if !footer.is_empty() {
        let f = commands
            .spawn((
                Text::new(footer),
                font.text(12.0),
                TextColor(INK),
                backed.clone(),
                Frame::Tip,
            ))
            .id();
        children.push(f);
    }
    if button {
        let b = commands
            .spawn((
                IncomingSkip,
                Button,
                Node {
                    padding: UiRect::axes(px(14.0), px(7.0)),
                    ..default()
                },
                Frame::Button,
            ))
            .id();
        let label = commands
            .spawn((
                Text::new("Дальше (пробел)"),
                font.bold(13.0),
                TextColor(INK),
            ))
            .id();
        commands.entity(b).add_child(label);
        children.push(b);
    }
    commands.entity(panel).add_children(&children);
}

/// "Дальше": let the card aimed at you land now.
#[derive(Component)]
struct IncomingSkip;

fn skip_incoming(
    pressed: Query<&Interaction, (Changed<Interaction>, With<IncomingSkip>)>,
    mut game: ResMut<Match>,
) {
    if pressed.iter().any(|i| *i == Interaction::Pressed) {
        let human = game.human;
        let _ = game.act(human, necromy_rules::Intent::Pass);
    }
}

/// How long a landed card stays on screen after it hit the human.
const SHOW_RESULT_SECS: f32 = 3.0;

fn expire_incoming_result(time: Res<Time>, mut shown: Local<(u32, f32)>, mut game: ResMut<Match>) {
    if game.incoming_result.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if shown.0 != game.incoming_serial {
        *shown = (game.incoming_serial, now);
    }
    if now - shown.1 > SHOW_RESULT_SECS {
        game.incoming_result = None;
    }
}

fn click_cards(
    cards: Query<(&Interaction, &HandCard), Changed<Interaction>>,
    mut game: ResMut<Match>,
    mut selection: ResMut<Selection>,
) {
    for (interaction, card) in &cards {
        if *interaction == Interaction::Pressed {
            play::pick_card(&mut game, &mut selection, card.0);
        }
    }
}

/// How the card's god's stage bends it right now (§5); empty at mid stage.
pub fn stage_note(g: &Game, def: &necromy_rules::CardDef) -> String {
    let Some(element) = def.element.filter(|_| def.effect.scales()) else {
        return String::new();
    };
    let god = necromy_rules::God::ALL[element.index()];
    match g.stage_shift(Some(element), def.effect.is_harmful()) {
        0 => String::new(),
        s => format!("{}: {:+}", names::stage(god, g.stage(god)), s),
    }
}

/// What is on the hovered hex, next to the mouse.
fn tooltip(
    game: Res<Match>,
    hovered: Res<Hovered>,
    board: Res<Board>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::TableCamera>>,
    tip: Single<(&mut Text, &mut Node, &mut Visibility), With<Tooltip>>,
) {
    let (mut text, mut node, mut visibility) = tip.into_inner();
    // A pinned hover (`NECROMY_HOVER` screenshots) sits next to the hex itself,
    // wherever the real mouse happens to be.
    let (camera, cam_transform) = *camera;
    let at_hex = || {
        let hex = hovered.0?;
        camera
            .world_to_viewport(cam_transform, board.hex_to_world(hex))
            .ok()
    };
    let anchor = if std::env::var_os("NECROMY_HOVER").is_some() {
        at_hex()
    } else {
        window.cursor_position()
    };
    let (Some(hex), Some(cursor)) = (hovered.0, anchor) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let Some(tile) = game.game.board().tile(hex) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    node.left = px(cursor.x + 18.0);
    node.top = px(cursor.y + 18.0);
    if !game.is_changed() && !hovered.is_changed() {
        return;
    }

    let g = &game.game;
    let (name, what) = names::terrain(tile.terrain);
    let region = tile.region.map_or_else(
        || "центр доски".to_string(),
        |god| format!("край {}", names::god_genitive(god)),
    );
    let mut lines = vec![format!("{name} · {region}"), what.to_string()];
    lines.push(format!("проход: {}", tile.terrain.move_cost()));
    if let Some(owner) = g.owner(hex) {
        lines.push(format!("владелец: {}", game.name(owner)));
    }
    if let Some(corpse) = tile.corpse {
        let left = necromy_rules::board::GROVE_AGE.saturating_sub(corpse.age);
        lines.push(format!(
            "тело: не тронуть {left} раунд(а) — прорастёт рощей"
        ));
    }
    if g.traps()
        .iter()
        .any(|t| t.hex == hex && t.owner == game.human)
    {
        lines.push("здесь твоя ловушка".into());
    }
    if let Some(p) = g.occupant(hex) {
        lines.push(format!("здесь: {}", game.name(p)));
    }
    if g.guard().is_some_and(|guard| guard.hex == hex) {
        lines.push("здесь королевская гвардия".into());
    }
    if game.is_human_turn() {
        if g.attackable(game.human).contains(&hex) {
            lines.push("клик — напасть".into());
        } else if let Some(cost) = g.reachable(game.human).get(&hex) {
            lines.push(format!("клик — идти ({cost} очк.)"));
        }
    }
    text.0 = lines.join("\n");
}

/// A card in hand rises into full view while hovered, aimed or marked to
/// burn, like drawing it out of the hand.
fn raise_cards(
    selection: Res<Selection>,
    mut cards: Query<(&Interaction, &HandCard, &mut Node, &mut ZIndex)>,
) {
    for (interaction, card, mut node, mut z) in &mut cards {
        let chosen = selection.card == Some(card.0) || selection.burn.contains(&card.0);
        let up = chosen || *interaction != Interaction::None;
        let top = if up { px(-CARD_TUCK) } else { px(0.0) };
        // A raised card comes in front of the ones overlapping it.
        z.set_if_neq(ZIndex(if up { 1 } else { 0 }));
        if node.top != top {
            node.top = top;
        }
    }
}
