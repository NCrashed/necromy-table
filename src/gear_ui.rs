//! Worn items on the stat sheets (docs/design.md §20.3): three slots with
//! the items' pictures, a tooltip for what each does now and what breaks
//! it, and at a temple a click gives the human's item to its god.

use bevy::prelude::*;
use necromy_rules::{PlayerId, Slot};

use crate::hud::{INK, UiFont};
use crate::names;
use crate::play::Match;
use crate::stats::{self, StatArt};
use crate::ui_skin::Frame;

const SLOT: f32 = 34.0;

pub struct GearUiPlugin;

impl Plugin for GearUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(crate::MatchBegins, spawn_tip)
            .add_systems(crate::InGame, (gear_tip, give));
    }
}

/// The picture of each item: `assets/items/<slug>.png` (PixelLab).
pub fn art_file(name: &str) -> &'static str {
    match name {
        "Тисовый лук" => "yew-bow",
        "Плащ из мха" => "moss-cloak",
        "Посох-корень" => "root-staff",
        "Клинок голода" => "hunger-blade",
        "Жаркий доспех" => "ember-armour",
        "Кубок пира" => "feast-cup",
        "Посох паломника" => "pilgrim-staff",
        "Сандалии пути" => "road-sandals",
        "Чётки тишины" => "silence-beads",
        "Меч присяги" => "oath-sword",
        "Латы переписи" => "census-plate",
        "Печать реестра" => "registry-seal",
        "Кинжал тумана" => "mist-dagger",
        "Покров Майи" => "maya-veil",
        "Бирюзовое зеркало" => "turquoise-mirror",
        _ => "loot-pouch",
    }
}

/// One slot of a champion's sheet.
#[derive(Component, Clone, Copy)]
struct GearSlot {
    player: PlayerId,
    slot: Slot,
}

#[derive(Component)]
struct GearTip;

/// The row of three slots on a champion's sheet.
pub fn gear_row(
    commands: &mut Commands,
    art: &StatArt,
    font: &UiFont,
    m: &Match,
    player: PlayerId,
) -> Entity {
    let row = stats::row(commands);
    let gear = m.game.gear(player);
    for slot in Slot::ALL {
        let cell = commands
            .spawn((
                GearSlot { player, slot },
                Button,
                Node {
                    width: px(SLOT),
                    height: px(SLOT),
                    padding: UiRect::all(px(1.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Frame::Slot,
            ))
            .id();
        let inside = match gear[slot.index()] {
            Some(item) => commands
                .spawn((
                    ImageNode::new(art.items[item.0 as usize].clone()),
                    Node {
                        width: px(32.0),
                        height: px(32.0),
                        ..default()
                    },
                ))
                .id(),
            // An empty slot says what goes there.
            None => commands
                .spawn((
                    Text::new(slot_letter(slot)),
                    font.text(10.0),
                    TextColor(INK.with_alpha(0.45)),
                ))
                .id(),
        };
        commands.entity(cell).add_child(inside);
        commands.entity(row).add_child(cell);
    }
    row
}

fn slot_letter(slot: Slot) -> &'static str {
    match slot {
        Slot::Weapon => "оруж.",
        Slot::Armour => "обл.",
        Slot::Relic => "рел.",
    }
}

fn spawn_tip(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        GearTip,
        Text::new(""),
        font.text(12.0),
        TextColor(INK),
        Node {
            position_type: PositionType::Absolute,
            max_width: px(340.0),
            padding: UiRect::all(px(10.0)),
            ..default()
        },
        Frame::Tip,
        GlobalZIndex(12),
        Visibility::Hidden,
    ));
}

/// What the hovered slot holds, beside the cursor.
fn gear_tip(
    game: Res<Match>,
    slots: Query<(&Interaction, &GearSlot)>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    tip: Single<(&mut Text, &mut Node, &mut Visibility), With<GearTip>>,
) {
    let (mut text, mut node, mut visibility) = tip.into_inner();
    // Dev aid: `NECROMY_GEAR_TIP=slot` pins the tip of the human's slot
    // (0 weapon, 1 armour, 2 relic), for screenshots.
    let pinned = std::env::var("NECROMY_GEAR_TIP")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .map(|i| GearSlot {
            player: game.human,
            slot: Slot::ALL[i.min(2)],
        });
    let Some(hovered) = slots
        .iter()
        .find(|(i, _)| **i != Interaction::None)
        .map(|(_, s)| *s)
        .or(pinned)
    else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let g = &game.game;
    let Some(item) = g.gear(hovered.player)[hovered.slot.index()] else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let mut s = names::item_tip(g, Some(hovered.player), item);
    if hovered.player == game.human && g.can_sacrifice(game.human, hovered.slot) {
        let god = g
            .board()
            .tile(
                g.champion(game.human)
                    .map_or_else(Default::default, |c| c.hex),
            )
            .and_then(|t| t.region);
        if let Some(god) = god {
            s.push_str(&format!(
                "\nклик — отдать {} (+{} подношения)",
                names::god_dative(god),
                necromy_rules::SACRIFICE
            ));
        }
    }
    if text.0 != s {
        text.0 = s;
    }
    let at = window.cursor_position().unwrap_or(Vec2::new(40.0, 400.0));
    node.left = px(at.x + 16.0);
    node.bottom = px((window.height() - at.y + 8.0).max(8.0));
    node.top = Val::Auto;
    visibility.set_if_neq(Visibility::Inherited);
}

/// A click on one of the human's items at a temple gives it to the god.
fn give(slots: Query<(&Interaction, &GearSlot), Changed<Interaction>>, mut game: ResMut<Match>) {
    for (interaction, slot) in &slots {
        if *interaction != Interaction::Pressed || slot.player != game.human {
            continue;
        }
        if game.game.gear(slot.player)[slot.slot.index()].is_none() {
            continue;
        }
        if game.game.can_sacrifice(game.human, slot.slot) {
            game.sacrifice(slot.slot);
        } else {
            game.feed
                .push("Отдать предмет богу можно в его храме, в свой ход.".into());
        }
    }
}

/// Each worn item on its own line, for sheets that cannot be hovered: the
/// picture, the name, what it does now, and a dark god's toll.
pub fn gear_lines(
    commands: &mut Commands,
    art: &StatArt,
    font: &UiFont,
    m: &Match,
    player: PlayerId,
) -> Vec<Entity> {
    let g = &m.game;
    g.gear(player)
        .into_iter()
        .flatten()
        .map(|item| {
            let row = stats::row(commands);
            let icon = commands
                .spawn((
                    ImageNode::new(art.items[item.0 as usize].clone()),
                    Node {
                        width: px(32.0),
                        height: px(32.0),
                        ..default()
                    },
                ))
                .id();
            let mut text = format!("{}: {}", item.def().name, names::item_does(g, item));
            if g.item_tolls(player, item) {
                let god = necromy_rules::God::from_index(item.def().element.index());
                text.push_str(&format!(" · тёмный бог берёт {}", names::item_toll(god)));
            }
            let label = stats::label(commands, font, &text, 12.0, false);
            commands.entity(row).add_children(&[icon, label]);
            row
        })
        .collect()
}
