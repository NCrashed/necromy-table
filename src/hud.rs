//! Screen overlay: status line, champions, event feed and the human's hand.
//! Rebuilt from `Match` and `Selection` whenever either changes.

use bevy::prelude::*;
use bevy::text::{FontSize, FontSource};
use bevy::window::PrimaryWindow;
use necromy_rules::{CardDef, CardId, Game, WindowKind};

use crate::board::{Board, Hovered};
use crate::names;
use crate::play::{self, IncomingCountdown, Match, Selection};

const CARD_WIDTH: f32 = 150.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_font)
            .add_systems(Startup, spawn_hud)
            .add_systems(
                crate::InGame,
                (update_text, rebuild_hand)
                    .run_if(resource_changed::<Match>.or_else(resource_changed::<Selection>)),
            )
            .add_systems(
                crate::InGame,
                rebuild_incoming.run_if(
                    resource_changed::<Match>.or_else(resource_changed::<IncomingCountdown>),
                ),
            )
            .add_systems(
                crate::InGame,
                (click_cards, tooltip, skip_incoming, expire_incoming_result),
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

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Feed,
}

#[derive(Component)]
struct Hand;

#[derive(Component)]
struct Tooltip;

/// The card aimed at someone in an open Target window.
#[derive(Component)]
struct Incoming;

#[derive(Component)]
struct HandCard(CardId);

pub const INK: Color = Color::srgb(0.95, 0.92, 0.85);
pub const PANEL: Color = Color::srgba(0.08, 0.07, 0.10, 0.82);

fn spawn_hud(mut commands: Commands, font: Res<UiFont>) {
    let panel = |top: Option<f32>, left: Option<f32>, right: Option<f32>| Node {
        position_type: PositionType::Absolute,
        top: top.map_or(Val::Auto, px),
        left: left.map_or(Val::Auto, px),
        right: right.map_or(Val::Auto, px),
        padding: UiRect::all(px(8.0)),
        max_width: px(620.0),
        ..default()
    };
    commands.spawn((
        Panel::Feed,
        Text::new(""),
        font.text(12.0),
        TextColor(Color::srgb(0.82, 0.80, 0.74)),
        Node {
            max_width: px(360.0),
            ..panel(Some(140.0), Some(10.0), None)
        },
        BackgroundColor(PANEL),
    ));
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
            padding: UiRect::all(px(6.0)),
            max_width: px(320.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.04, 0.07, 0.92)),
        Visibility::Hidden,
        // Above cards and panels.
        GlobalZIndex(10),
    ));
    commands.spawn((
        Hand,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(10.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            column_gap: px(8.0),
            ..default()
        },
    ));
}

fn update_text(game: Res<Match>, mut panels: Query<(&Panel, &mut Text)>) {
    for (panel, mut text) in &mut panels {
        text.0 = match panel {
            Panel::Feed => game.feed.join("\n"),
        };
    }
}

/// How a card is drawn: frame colour and width, opacity, extra lines.
struct CardLook {
    border: Color,
    border_px: f32,
    alpha: f32,
    extra: Vec<(String, Color)>,
}

/// One card as a UI node: name, element and timing, text, stage note, battle
/// face and any extra lines.
fn spawn_card(
    commands: &mut Commands,
    font: &UiFont,
    g: &Game,
    def: &CardDef,
    look: CardLook,
) -> Entity {
    let alpha = look.alpha;
    let element = def.element.map_or("без стихии", names::element);
    let cost = if def.cost > 0 {
        format!(" · дух {}", def.cost)
    } else {
        String::new()
    };
    let lines = [
        (def.name.to_string(), font.bold(14.0), INK),
        (
            format!(
                "{element} · {} · {}{cost}",
                names::kind(def.kind),
                names::timing(def.timing)
            ),
            font.text(11.0),
            names::element_color(def.element),
        ),
        (
            def.text.to_string(),
            font.text(12.0),
            Color::srgb(0.84, 0.82, 0.76),
        ),
        (
            stage_note(g, def),
            font.text(11.0),
            Color::srgb(0.75, 0.70, 0.95),
        ),
        (
            format!("в бою: {}", names::face(def.burn_face())),
            font.text(11.0),
            Color::srgb(0.95, 0.55, 0.45),
        ),
    ];
    let card = commands
        .spawn((
            Node {
                width: px(CARD_WIDTH),
                min_height: px(120.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(6.0)),
                row_gap: px(4.0),
                border: UiRect::all(px(look.border_px)),
                ..default()
            },
            BorderColor::all(look.border),
            BackgroundColor(Color::srgba(0.10, 0.09, 0.12, alpha)),
        ))
        .id();
    let extra = look.extra.into_iter().map(|(t, c)| (t, font.text(11.0), c));
    for (text, font, color) in lines.into_iter().chain(extra) {
        if text.is_empty() {
            continue;
        }
        let line = commands
            .spawn((Text::new(text), font, TextColor(color.with_alpha(alpha))))
            .id();
        commands.entity(card).add_child(line);
    }
    card
}

fn rebuild_hand(
    mut commands: Commands,
    game: Res<Match>,
    selection: Res<Selection>,
    font: Res<UiFont>,
    hand: Single<Entity, With<Hand>>,
) {
    commands.entity(*hand).despawn_related::<Children>();
    let playable = game.game.playable(game.human);
    let burning = game.game.battle_dice(game.human).is_some() && game.human_awaited();
    for &card in game.game.hand(game.human) {
        let def = game.game.def(card);
        let usable = playable.contains(&card) || burning;
        let selected = selection.card == Some(card) || selection.burn.contains(&card);
        let look = CardLook {
            border: if selected {
                Color::srgb(1.0, 0.82, 0.3)
            } else {
                names::element_color(def.element)
            },
            border_px: if selected { 3.0 } else { 2.0 },
            alpha: if usable || selected { 0.95 } else { 0.55 },
            extra: Vec::new(),
        };
        let entity = spawn_card(&mut commands, &font, &game.game, def, look);
        commands.entity(entity).insert((HandCard(card), Button));
        commands.entity(*hand).add_child(entity);
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
    }) = g.window().map(|w| w.kind)
    {
        let at_me = target == game.human;
        let header = if at_me {
            format!("{} целит в тебя:", game.name(caster))
        } else {
            format!("{} целит в {}:", game.name(caster), game.name(target))
        };
        let mut extra = Vec::new();
        if let Some(bonus) = g.pending_bonus().filter(|&b| b > 0) {
            extra.push((format!("цепочка: +{bonus}"), Color::srgb(1.0, 0.82, 0.3)));
        }
        let look = CardLook {
            border: if at_me {
                red
            } else {
                names::element_color(g.def(card).element)
            },
            border_px: 3.0,
            alpha: 0.97,
            extra,
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
            border: red,
            border_px: 3.0,
            alpha: 0.97,
            extra: lines,
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
        padding: UiRect::axes(px(8.0), px(3.0)),
        ..default()
    };
    let header = commands
        .spawn((
            Text::new(header),
            font.bold(14.0),
            TextColor(INK),
            backed.clone(),
            BackgroundColor(PANEL),
        ))
        .id();
    let card = spawn_card(&mut commands, &font, g, g.def(card), look);
    let mut children = vec![header, card];
    if !footer.is_empty() {
        let f = commands
            .spawn((
                Text::new(footer),
                font.text(12.0),
                TextColor(INK),
                backed.clone(),
                BackgroundColor(PANEL),
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
                    padding: UiRect::axes(px(10.0), px(4.0)),
                    border: UiRect::all(px(2.0)),
                    ..default()
                },
                BorderColor::all(Color::srgb(1.0, 0.82, 0.3)),
                BackgroundColor(Color::srgba(0.2, 0.15, 0.1, 0.9)),
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
fn stage_note(g: &Game, def: &necromy_rules::CardDef) -> String {
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
        if g.attackable().contains(&hex) {
            lines.push("клик — напасть".into());
        } else if let Some(cost) = g.reachable().get(&hex) {
            lines.push(format!("клик — идти ({cost} очк.)"));
        }
    }
    text.0 = lines.join("\n");
}
