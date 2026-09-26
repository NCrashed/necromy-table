//! Screen overlay: status line, champions, event feed and the human's hand.
//! Rebuilt from `Match` and `Selection` whenever either changes.

use bevy::prelude::*;
use bevy::text::{FontSize, FontSource};
use necromy_rules::{CardId, Game};

use crate::names;
use crate::play::{self, Match, Selection};

const CARD_WIDTH: f32 = 150.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_font)
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (update_text, rebuild_hand)
                    .run_if(resource_changed::<Match>.or_else(resource_changed::<Selection>)),
            )
            .add_systems(Update, click_cards);
    }
}

/// Placeholder UI font with Cyrillic (assets/fonts/README.md).
#[derive(Resource)]
pub struct UiFont {
    regular: Handle<Font>,
    bold: Handle<Font>,
}

impl UiFont {
    fn text(&self, size: f32) -> TextFont {
        TextFont {
            font: FontSource::Handle(self.regular.clone()),
            font_size: FontSize::Px(size),
            ..default()
        }
    }

    fn bold(&self, size: f32) -> TextFont {
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
    Status,
    Champions,
    Feed,
}

#[derive(Component)]
struct Hand;

#[derive(Component)]
struct HandCard(CardId);

const INK: Color = Color::srgb(0.95, 0.92, 0.85);
const PANEL: Color = Color::srgba(0.08, 0.07, 0.10, 0.82);

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
        Panel::Status,
        Text::new(""),
        font.text(16.0),
        TextColor(INK),
        panel(Some(10.0), Some(10.0), None),
        BackgroundColor(PANEL),
    ));
    commands.spawn((
        Panel::Champions,
        Text::new(""),
        font.text(15.0),
        TextColor(INK),
        panel(Some(10.0), None, Some(10.0)),
        BackgroundColor(PANEL),
    ));
    commands.spawn((
        Panel::Feed,
        Text::new(""),
        font.text(13.0),
        TextColor(Color::srgb(0.82, 0.80, 0.74)),
        panel(Some(120.0), Some(10.0), None),
        BackgroundColor(PANEL),
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

fn update_text(
    game: Res<Match>,
    selection: Res<Selection>,
    mut panels: Query<(&Panel, &mut Text)>,
) {
    let g = &game.game;
    let current = g.current_player();
    let order = g
        .order()
        .iter()
        .map(|&p| {
            let name = g.champion(p).map_or("?", |c| names::god(c.god));
            if p == current {
                format!("[{name}]")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" › ");

    let hint = if let Some(card) = selection.card {
        format!(
            "Цель для «{}»: клик по золотой клетке. Правый клик — отмена.",
            g.def(card).name
        )
    } else if let Some(window) = g.window() {
        if game.human_awaited() {
            format!(
                "Окно реакции: {}.\nСыграй подходящую карту или нажми P — пас.",
                play::window_name(&game, window.kind)
            )
        } else {
            format!("Окно реакции: {}…", play::window_name(&game, window.kind))
        }
    } else if game.is_human_turn() {
        format!(
            "Твой ход: {} очк. движения. Клик по светлой клетке — идти,\nкарта — сыграть, пробел — конец хода.",
            g.move_points()
        )
    } else {
        format!("{} думает…", game.name(current))
    };
    for (panel, mut text) in &mut panels {
        text.0 = match panel {
            Panel::Status => format!(
                "Раунд {} — {}\n{order}\n{hint}",
                g.round(),
                play::time_name(g.time())
            ),
            Panel::Champions => champion_lines(&game, g),
            Panel::Feed => game.feed.join("\n"),
        };
    }
}

fn champion_lines(m: &Match, g: &Game) -> String {
    g.players()
        .filter_map(|p| {
            let c = g.champion(p)?;
            let mut line = format!(
                "{}  здоровье {}/{}  дух {}/{}  карт {}",
                m.name(p),
                c.hp,
                c.body,
                c.spirit_points,
                c.spirit,
                g.hand(p).len()
            );
            if let Some(ward) = c.ward {
                line += &format!("  оберег: {}", names::element(ward));
            }
            if c.rooted {
                line += "  скован";
            }
            Some(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    for &card in game.game.hand(game.human) {
        let def = game.game.def(card);
        let usable = playable.contains(&card);
        let selected = selection.card == Some(card);
        let border = if selected {
            Color::srgb(1.0, 0.82, 0.3)
        } else {
            names::element_color(def.element)
        };
        let alpha = if usable || selected { 0.95 } else { 0.55 };
        let element = def.element.map_or("без стихии", names::element);
        let cost = if def.cost > 0 {
            format!(" · дух {}", def.cost)
        } else {
            String::new()
        };
        let card_entity = commands
            .spawn((
                HandCard(card),
                Button,
                Node {
                    width: px(CARD_WIDTH),
                    min_height: px(120.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(6.0)),
                    row_gap: px(4.0),
                    border: UiRect::all(px(if selected { 3.0 } else { 2.0 })),
                    ..default()
                },
                BorderColor::all(border),
                BackgroundColor(Color::srgba(0.10, 0.09, 0.12, alpha)),
                children![
                    (
                        Text::new(def.name),
                        font.bold(14.0),
                        TextColor(INK.with_alpha(alpha)),
                    ),
                    (
                        Text::new(format!(
                            "{element} · {} · {}{cost}",
                            names::kind(def.kind),
                            names::timing(def.timing)
                        )),
                        font.text(11.0),
                        TextColor(names::element_color(def.element).with_alpha(alpha)),
                    ),
                    (
                        Text::new(def.text),
                        font.text(12.0),
                        TextColor(Color::srgb(0.84, 0.82, 0.76).with_alpha(alpha)),
                    ),
                ],
            ))
            .id();
        commands.entity(*hand).add_child(card_entity);
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
