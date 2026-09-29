//! The trial panel (docs/design.md §20.2): who stands the trial, their dice
//! in the first tray, and what the god asks of them: the faces it counts,
//! the boon and the price. The human picks cards to burn here, as in battle;
//! the result shows once the dice are down.

use bevy::prelude::*;
use necromy_rules::{Intent, WindowKind};

use crate::dice::{DiceShow, Revealed, TRAY_TEXTURE, TrayTextures};
use crate::hud::{INK, UiFont};
use crate::icons::StatIcon;
use crate::names;
use crate::play::{Match, Selection};
use crate::stats::{self, StatArt};
use crate::ui_skin::{Accent, Frame};

/// Width of the tray on screen, as in battle.
const TRAY_W: f32 = 340.0;
const INFO_W: f32 = 300.0;
const BURN_FRAME: Color = Color::srgb(0.95, 0.55, 0.2);
const PASSED: Color = Color::srgb(0.35, 0.62, 0.25);
const FAILED: Color = Color::srgb(0.72, 0.2, 0.16);

pub struct TrialUiPlugin;

impl Plugin for TrialUiPlugin {
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
            .add_systems(crate::InGame, buttons);
    }
}

#[derive(Component)]
struct TrialPanel;

#[derive(Component, Clone, Copy)]
enum TrialButton {
    Throw,
    NoCards,
}

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        TrialPanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(88.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        // Over the board and panels, under tooltips, as the battle panel.
        GlobalZIndex(5),
        Visibility::Hidden,
    ));
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
    panel: Single<(Entity, &mut Visibility), With<TrialPanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let Some(info) = game.trial.as_ref() else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let Some(trial) = info.trial.as_ref() else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    let g = &game.game;
    let landed = info.result.is_some() && dice.landed();

    let frame = commands
        .spawn((
            Node {
                padding: UiRect::all(px(20.0)),
                column_gap: px(18.0),
                align_items: AlignItems::FlexStart,
                align_self: AlignSelf::FlexStart,
                ..default()
            },
            Frame::Plate,
            Accent(god_color(trial.god)),
        ))
        .id();

    // Left: the challenger and their tray.
    let left = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(6.0),
            width: px(TRAY_W),
            ..default()
        })
        .id();
    let header = stats::row(&mut commands);
    let portrait = commands
        .spawn((
            ImageNode::new(art.portraits[info.player.0 as usize].clone()),
            Node {
                width: px(48.0),
                height: px(72.0),
                ..default()
            },
        ))
        .id();
    let who = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            ..default()
        })
        .id();
    let name = stats::label(&mut commands, &font, &game.name(info.player), 16.0, true);
    let numbers = stats::row(&mut commands);
    let sword = stats::icon_node(&mut commands, art.icon(StatIcon::Might), 24.0, true);
    let might = g.champion(info.player).map_or(0, |c| c.might);
    let dice_text = stats::label(
        &mut commands,
        &font,
        &format!("кубиков: {might}"),
        13.0,
        true,
    );
    commands.entity(numbers).add_children(&[sword, dice_text]);
    commands.entity(who).add_children(&[name, numbers]);
    commands.entity(header).add_children(&[portrait, who]);

    let tray_h = TRAY_W * TRAY_TEXTURE[1] as f32 / TRAY_TEXTURE[0] as f32;
    let tray_image = commands
        .spawn((
            ImageNode::new(trays.0[0].clone()),
            Node {
                width: px(TRAY_W),
                height: px(tray_h),
                ..default()
            },
        ))
        .id();
    let tray = commands
        .spawn((
            Frame::Inset,
            Node {
                padding: UiRect::all(px(6.0)),
                ..default()
            },
        ))
        .add_child(tray_image)
        .id();

    // Faces so far; the ones that count for the trial stand out.
    let faces = stats::row(&mut commands);
    let faces_label = stats::label(&mut commands, &font, "грани:", 12.0, false);
    commands.entity(faces).add_child(faces_label);
    let mut counted = 0;
    for (face, burned) in info
        .burned
        .iter()
        .map(|f| (*f, true))
        .chain(revealed.0[0].iter().map(|f| (*f, false)))
    {
        let counts = g.trial_counts(trial, face);
        counted += u8::from(counts);
        let icon = commands
            .spawn((
                ImageNode::new(art.faces[&face].clone()).with_color(if counts {
                    Color::WHITE
                } else {
                    Color::srgba(1.0, 1.0, 1.0, 0.35)
                }),
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
    let need = info
        .result
        .map_or_else(|| g.trial_need(trial), |(_, n, _)| n);
    let tally = stats::label(
        &mut commands,
        &font,
        &format!("в зачёт: {counted} из {need}"),
        13.0,
        true,
    );
    commands
        .entity(left)
        .add_children(&[header, tray, faces, tally]);

    // Right: what the god asks, and what is happening now.
    let right = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(8.0),
            width: px(INFO_W),
            ..default()
        })
        .id();
    let title_row = stats::row(&mut commands);
    let god_icon = stats::icon_node(
        &mut commands,
        art.gods[trial.god.index()].clone(),
        28.0,
        true,
    );
    let title = stats::label(
        &mut commands,
        &font,
        &capitalise(names::trial_name(trial.god)),
        18.0,
        true,
    );
    commands.entity(title_row).add_children(&[god_icon, title]);
    let place = g
        .board()
        .tile(info.hex)
        .map(|t| names::terrain(t.terrain).0);
    let whose = stats::label(
        &mut commands,
        &font,
        &format!(
            "{} · {} · {}",
            names::god(trial.god),
            names::stage(trial.god, g.stage(trial.god)),
            place.unwrap_or("")
        ),
        12.0,
        false,
    );
    let ask_row = stats::row(&mut commands);
    let face_icon = stats::icon_node(&mut commands, art.faces[&trial.face()].clone(), 24.0, true);
    let ask = stats::label(
        &mut commands,
        &font,
        &format!("нужно: {}", names::trial_ask(g, trial)),
        13.0,
        true,
    );
    commands.entity(ask_row).add_children(&[face_icon, ask]);
    let boon = stats::label(
        &mut commands,
        &font,
        &format!("награда: {}", names::boon(g, trial)),
        13.0,
        false,
    );
    let price = stats::label(
        &mut commands,
        &font,
        &format!("цена провала: {}", names::trial_price(g, trial.god)),
        13.0,
        false,
    );
    commands
        .entity(right)
        .add_children(&[title_row, whose, ask_row, boon, price]);

    let mine = matches!(
        game.human_window(),
        Some(WindowKind::Trial { player, .. }) if player == game.human
    );
    let phase = if mine {
        let max = g.battle_dice(game.human).unwrap_or(0);
        format!(
            "Сжигание карт.\nКликни карты в руке (до {max}): каждая заменит кубик своей гранью."
        )
    } else if info.result.is_none() {
        format!("{} выбирает карты на сжигание…", game.name(info.player))
    } else if !landed {
        "Бросок!".to_string()
    } else {
        String::new()
    };
    if !phase.is_empty() {
        let text = commands
            .spawn((
                Text::new(phase),
                font.text(13.0),
                TextColor(INK),
                TextLayout::justify(Justify::Left),
            ))
            .id();
        commands.entity(right).add_child(text);
    }
    if mine {
        for &card in &selection.burn {
            let def = g.def(card);
            let chip = stats::row(&mut commands);
            let icon = stats::icon_node(
                &mut commands,
                art.faces[&def.burn_face()].clone(),
                20.0,
                true,
            );
            let counts = if g.trial_counts(trial, def.burn_face()) {
                ""
            } else {
                " (не в зачёт)"
            };
            let text = stats::label(
                &mut commands,
                &font,
                &format!("{} → {}{counts}", def.name, names::face(def.burn_face())),
                12.0,
                false,
            );
            commands.entity(chip).add_children(&[icon, text]);
            commands.entity(right).add_child(chip);
        }
        for (button, text) in [
            (TrialButton::Throw, "Бросить (Enter)"),
            (TrialButton::NoCards, "Без карт (пробел)"),
        ] {
            let b = commands
                .spawn((
                    button,
                    Button,
                    Node {
                        padding: UiRect::axes(px(14.0), px(8.0)),
                        ..default()
                    },
                    Frame::Button,
                ))
                .id();
            let t = stats::label(&mut commands, &font, text, 13.0, true);
            commands.entity(b).add_child(t);
            commands.entity(right).add_child(b);
        }
    }
    if landed && let Some((got, need, passed)) = info.result {
        let verdict = if passed {
            format!(
                "Пройдено: {got} из {need}. {}",
                capitalise(&names::boon(g, trial))
            )
        } else {
            format!(
                "Не выдержано: {got} из {need}. {}",
                capitalise(names::trial_price(g, trial.god))
            )
        };
        let text = commands
            .spawn((
                Text::new(verdict),
                font.bold(15.0),
                TextColor(if passed { PASSED } else { FAILED }),
            ))
            .id();
        commands.entity(right).add_child(text);
    }

    commands.entity(frame).add_children(&[left, right]);
    commands.entity(panel).add_child(frame);
}

fn buttons(
    pressed: Query<(&Interaction, &TrialButton), Changed<Interaction>>,
    mut game: ResMut<Match>,
    mut selection: ResMut<Selection>,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let human = game.human;
        let intent = match button {
            TrialButton::Throw => Intent::Burn {
                cards: std::mem::take(&mut selection.burn),
            },
            TrialButton::NoCards => {
                selection.burn.clear();
                Intent::Pass
            }
        };
        if let Err(err) = game.act(human, intent) {
            warn!("trial choice rejected: {err}");
        }
    }
}

fn god_color(god: necromy_rules::God) -> Color {
    let [r, g, b] = god.accent();
    Color::srgb_u8(r, g, b)
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
