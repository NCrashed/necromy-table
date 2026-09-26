//! The battle panel (docs/design.md §12): who fights whom, their dice and
//! health, the cards being burned, the trays with rolling dice and the faces
//! that came up. It sits over everything from the first blow until the dice
//! show ends.

use bevy::prelude::*;
use necromy_rules::{Face, GUARD_DICE, Intent, PlayerId, WindowKind};

use crate::dice::{DiceShow, Revealed, TRAY_TEXTURE, TrayTextures};
use crate::hud::{INK, PANEL, UiFont};
use crate::icons::StatIcon;
use crate::names;
use crate::play::{Match, Selection};
use crate::stats::{self, HEALTH, StatArt};

/// Width of the trays on screen.
const TRAY_W: f32 = 400.0;
const BURN_FRAME: Color = Color::srgb(0.95, 0.55, 0.2);
const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);

pub struct BattleUiPlugin;

impl Plugin for BattleUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_panel)
            .add_systems(
                crate::InGame,
                rebuild.run_if(
                    resource_changed::<Match>
                        .or_else(resource_changed::<Selection>)
                        .or_else(resource_changed::<Revealed>),
                ),
            )
            .add_systems(crate::InGame, buttons)
            .add_systems(
                crate::InGame,
                debug_sizes.run_if(|| std::env::var_os("NECROMY_DEBUG_UI").is_some()),
            );
    }
}

#[derive(Component)]
struct BattlePanel;

#[derive(Component, Clone, Copy)]
enum BattleButton {
    Throw,
    NoCards,
}

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        BattlePanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(88.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        // Over the board and panels, under tooltips.
        GlobalZIndex(5),
        Visibility::Hidden,
    ));
}

/// Who stands on one side of the battle.
#[derive(Clone, Copy)]
enum Side {
    Champion(PlayerId),
    Guard,
}

#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    game: Res<Match>,
    selection: Res<Selection>,
    revealed: Res<Revealed>,
    dice: Res<DiceShow>,
    art: Res<StatArt>,
    trays: Res<TrayTextures>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<BattlePanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let Some(battle) = game.battle.as_ref() else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);

    let sides = [
        battle.attacker.map_or(Side::Guard, Side::Champion),
        Side::Champion(battle.defender),
    ];
    let landed = battle.scores.is_some() && dice.landed();

    let frame = commands
        .spawn((
            Node {
                padding: UiRect::all(px(12.0)),
                column_gap: px(14.0),
                border: UiRect::all(px(2.0)),
                align_items: AlignItems::FlexStart,
                align_self: AlignSelf::FlexStart,
                ..default()
            },
            BorderColor::all(GOLD),
            BackgroundColor(PANEL.with_alpha(1.0)),
        ))
        .id();

    let mut columns = Vec::new();
    for (i, side) in sides.into_iter().enumerate() {
        // Damage this side dealt and took, once the dice are all down.
        let result = battle.scores.filter(|_| landed).map(|(a, d)| {
            let (mine, theirs) = if i == 0 { (a, d) } else { (d, a) };
            (
                mine,
                mine.hits.saturating_sub(theirs.shields),
                theirs.hits.saturating_sub(mine.shields),
            )
        });
        let column = side_column(
            &mut commands,
            &game,
            &art,
            &font,
            side,
            i == 1,
            trays.0[i].clone(),
            &battle.burned[i],
            &revealed.0[i],
            result,
            // Health shown matches the dice: before the blows until they land.
            battle.hp[i].map(|(before, after)| if landed { after } else { before }),
            landed && battle.fell[i],
        );
        columns.push(column);
    }
    let centre = centre_column(
        &mut commands,
        &game,
        &selection,
        &art,
        &font,
        battle.scores.is_some(),
        landed,
    );
    commands
        .entity(frame)
        .add_children(&[columns[0], centre, columns[1]]);
    commands.entity(panel).add_child(frame);
}

#[allow(clippy::too_many_arguments)]
fn side_column(
    commands: &mut Commands,
    m: &Match,
    art: &StatArt,
    font: &UiFont,
    side: Side,
    defending: bool,
    tray: Handle<Image>,
    burned: &[Face],
    thrown: &[Face],
    result: Option<(necromy_rules::Score, u8, u8)>,
    hp_shown: Option<u8>,
    fell: bool,
) -> Entity {
    let g = &m.game;
    let column = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(6.0),
            width: px(TRAY_W),
            ..default()
        })
        .id();

    // Portrait, name, health and dice.
    let (portrait, name, dice) = match side {
        Side::Champion(p) => (
            art.portraits[p.0 as usize].clone(),
            m.name(p),
            g.dice_for(p, defending),
        ),
        Side::Guard => (
            art.guard.clone(),
            "Королевская гвардия".to_string(),
            GUARD_DICE,
        ),
    };
    let header = stats::row(commands);
    let image = commands
        .spawn((
            ImageNode::new(portrait),
            Node {
                width: px(48.0),
                height: px(72.0),
                ..default()
            },
        ))
        .id();
    let info = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            ..default()
        })
        .id();
    let title = stats::label(
        commands,
        font,
        &format!(
            "{name} — {}",
            if defending {
                "защита"
            } else {
                "нападение"
            }
        ),
        16.0,
        true,
    );
    let numbers = stats::row(commands);
    if let Side::Champion(p) = side
        && let Some(c) = g.champion(p)
    {
        let heart = stats::icon_node(commands, art.icon(StatIcon::Health), 24.0, true);
        let hp_now = hp_shown.unwrap_or(c.hp);
        let hp = stats::bar(commands, hp_now, c.body, HEALTH, None, 9.0, 14.0);
        let text = if fell {
            format!("{hp_now}/{} — пал!", c.body)
        } else {
            format!("{hp_now}/{}", c.body)
        };
        let hp_text = stats::label(commands, font, &text, 13.0, fell);
        commands.entity(numbers).add_children(&[heart, hp, hp_text]);
        if let Some(ward) = c.ward {
            let w = stats::icon_node(commands, art.wards[ward.index()].clone(), 24.0, true);
            commands.entity(numbers).add_child(w);
        }
    }
    let sword = stats::icon_node(commands, art.icon(StatIcon::Might), 24.0, true);
    let dice_text = stats::label(commands, font, &format!("кубиков: {dice}"), 13.0, true);
    commands.entity(numbers).add_children(&[sword, dice_text]);
    commands.entity(info).add_children(&[title, numbers]);
    commands.entity(header).add_children(&[image, info]);

    // The tray, rendered by its own camera.
    let tray_h = TRAY_W * TRAY_TEXTURE[1] as f32 / TRAY_TEXTURE[0] as f32;
    let tray = commands
        .spawn((
            ImageNode::new(tray),
            Node {
                width: px(TRAY_W),
                height: px(tray_h),
                ..default()
            },
        ))
        .id();

    // Faces so far: burned ones framed in orange, then thrown ones as they land.
    let faces = stats::row(commands);
    let faces_label = stats::label(commands, font, "грани:", 12.0, false);
    commands.entity(faces).add_child(faces_label);
    for (face, burned) in burned
        .iter()
        .map(|f| (*f, true))
        .chain(thrown.iter().map(|f| (*f, false)))
    {
        let icon = commands
            .spawn((
                ImageNode::new(art.faces[&face].clone()),
                Node {
                    width: px(24.0),
                    height: px(24.0),
                    border: UiRect::all(px(if burned { 2.0 } else { 0.0 })),
                    ..default()
                },
                BorderColor::all(BURN_FRAME),
            ))
            .id();
        commands.entity(faces).add_child(icon);
    }

    let mut children = vec![header, tray, faces];
    if let Some((score, dealt, taken)) = result {
        let line = stats::label(
            commands,
            font,
            &format!(
                "ударов {} · щитов {} · нанесено {dealt} · получено {taken}",
                score.hits, score.shields
            ),
            13.0,
            true,
        );
        children.push(line);
    }
    commands.entity(column).add_children(&children);
    column
}

/// The middle: what is happening now, and the burn controls for the human.
fn centre_column(
    commands: &mut Commands,
    m: &Match,
    selection: &Selection,
    art: &StatArt,
    font: &UiFont,
    decided: bool,
    landed: bool,
) -> Entity {
    let g = &m.game;
    let column = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(8.0),
            width: px(170.0),
            margin: UiRect::top(px(24.0)),
            ..default()
        })
        .id();
    let vs = stats::label(commands, font, "против", 18.0, true);
    commands.entity(column).add_child(vs);

    let burning = matches!(g.window().map(|w| w.kind), Some(WindowKind::Battle { .. }));
    let my_choice = burning && m.human_awaited();
    let phase = if my_choice {
        let max = g.battle_dice(m.human).unwrap_or(0);
        format!(
            "Сжигание карт.\nКликни карты в руке (до {max}): каждая заменит кубик своей гранью."
        )
    } else if burning {
        "Стороны выбирают карты на сжигание…".to_string()
    } else if !decided {
        String::new()
    } else if !landed {
        "Бросок!".to_string()
    } else {
        "Итог".to_string()
    };
    let text = commands
        .spawn((
            Text::new(phase),
            font.text(13.0),
            TextColor(INK),
            TextLayout::justify(Justify::Center),
        ))
        .id();
    commands.entity(column).add_child(text);

    if my_choice {
        // The chosen cards and the faces they will give.
        for &card in &selection.burn {
            let def = g.def(card);
            let chip = stats::row(commands);
            let icon = commands
                .spawn((
                    ImageNode::new(art.faces[&def.burn_face()].clone()),
                    Node {
                        width: px(20.0),
                        height: px(20.0),
                        ..default()
                    },
                ))
                .id();
            let name = stats::label(
                commands,
                font,
                &format!("{} → {}", def.name, names::face(def.burn_face())),
                12.0,
                false,
            );
            commands.entity(chip).add_children(&[icon, name]);
            commands.entity(column).add_child(chip);
        }
        for (button, text) in [
            (BattleButton::Throw, "Бросить (Enter)"),
            (BattleButton::NoCards, "Без карт (P)"),
        ] {
            let b = commands
                .spawn((
                    button,
                    Button,
                    Node {
                        padding: UiRect::axes(px(10.0), px(5.0)),
                        border: UiRect::all(px(2.0)),
                        ..default()
                    },
                    BorderColor::all(GOLD),
                    BackgroundColor(Color::srgba(0.2, 0.15, 0.1, 0.9)),
                ))
                .id();
            let t = stats::label(commands, font, text, 13.0, true);
            commands.entity(b).add_child(t);
            commands.entity(column).add_child(b);
        }
    }
    column
}

fn buttons(
    pressed: Query<(&Interaction, &BattleButton), Changed<Interaction>>,
    mut game: ResMut<Match>,
    mut selection: ResMut<Selection>,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let human = game.human;
        let intent = match button {
            BattleButton::Throw => Intent::Burn {
                cards: std::mem::take(&mut selection.burn),
            },
            BattleButton::NoCards => {
                selection.burn.clear();
                Intent::Pass
            }
        };
        if let Err(err) = game.act(human, intent) {
            warn!("battle choice rejected: {err}");
        }
    }
}

/// Dev aid (`NECROMY_DEBUG_UI`): logs the panel's computed node sizes.
fn debug_sizes(
    panel: Query<&Children, With<BattlePanel>>,
    nodes: Query<(&ComputedNode, Option<&Children>, Option<&Text>)>,
) {
    fn walk(
        e: Entity,
        depth: usize,
        nodes: &Query<(&ComputedNode, Option<&Children>, Option<&Text>)>,
    ) {
        let Ok((c, children, text)) = nodes.get(e) else {
            return;
        };
        info!(
            "{}{:?} {:?}",
            "  ".repeat(depth),
            c.size(),
            text.map(|t| t.0.chars().take(20).collect::<String>())
        );
        if depth < 4 {
            for ch in children.into_iter().flatten() {
                walk(*ch, depth + 1, nodes);
            }
        }
    }
    for children in &panel {
        for ch in children {
            walk(*ch, 0, &nodes);
        }
    }
}
