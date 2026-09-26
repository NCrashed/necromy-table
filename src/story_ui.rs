//! The storyteller's lines on screen (docs/design.md §8).
//!
//! Left, above the human's sheet: their open lines, each with the god who
//! told it, what it asks (with progress), the deadline and the reward. When
//! a god tells the human a new line, its voice shows in the middle for a few
//! seconds. A pilgrimage's temple glows on the board (`board.rs`).

use bevy::prelude::*;

use crate::hud::{INK, PANEL, UiFont};
use crate::names;
use crate::play::Match;
use crate::stats::{self, StatArt};

const VOICE: Color = Color::srgb(0.85, 0.80, 0.95);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
/// How long a god's voice stays on screen.
const VOICE_SECS: f32 = 6.0;

pub struct StoryUiPlugin;

impl Plugin for StoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(
                crate::InGame,
                (rebuild_lines, rebuild_voice).run_if(resource_changed::<Match>),
            )
            .add_systems(crate::InGame, expire_voice);
    }
}

#[derive(Component)]
struct StoryPanel;

#[derive(Component)]
struct VoicePanel;

fn spawn(mut commands: Commands) {
    commands.spawn((
        StoryPanel,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(232.0),
            left: px(10.0),
            ..default()
        },
    ));
    commands.spawn((
        VoicePanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(90.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        GlobalZIndex(11),
        Visibility::Hidden,
    ));
}

fn rebuild_lines(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<Entity, With<StoryPanel>>,
) {
    commands.entity(*panel).despawn_related::<Children>();
    let g = &game.game;
    let lines: Vec<_> = g.lines_of(game.human).collect();
    if lines.is_empty() {
        return;
    }
    let sheet = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8.0)),
                row_gap: px(4.0),
                width: px(250.0),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .id();
    let title = stats::label(&mut commands, &font, "Сюжет", 14.0, true);
    commands.entity(sheet).add_child(title);
    for line in lines {
        let head = stats::row(&mut commands);
        let icon = stats::icon_node(
            &mut commands,
            art.gods[line.god.index()].clone(),
            18.0,
            true,
        );
        let name = stats::label(
            &mut commands,
            &font,
            names::line_title(line.kind),
            13.0,
            true,
        );
        commands.entity(head).add_children(&[icon, name]);
        let reward = if line.stake > 0 {
            format!("+{} Стиля, провал −{}", line.style, line.stake)
        } else {
            format!("+{} Стиля", line.style)
        };
        let body = commands
            .spawn((
                Text::new(format!(
                    "{} · до раунда {} · {reward}",
                    names::line_goal(line, g),
                    line.deadline
                )),
                font.text(11.0),
                TextColor(DIM),
                Node {
                    width: px(234.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(sheet).add_children(&[head, body]);
    }
    commands.entity(*panel).add_child(sheet);
}

fn rebuild_voice(
    mut commands: Commands,
    game: Res<Match>,

    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<VoicePanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    // A wish's answer has the same spot; it goes first.
    let Some(line) = game.told.filter(|_| game.wish_reply.is_none()) else {
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
                width: px(480.0),
                ..default()
            },
            BorderColor::all(VOICE),
            BackgroundColor(PANEL.with_alpha(0.97)),
        ))
        .id();
    let head = stats::row(&mut commands);
    let icon = stats::icon_node(
        &mut commands,
        art.gods[line.god.index()].clone(),
        28.0,
        true,
    );
    let who = stats::label(
        &mut commands,
        &font,
        &format!(
            "{} зовёт тебя: «{}»",
            names::god(line.god),
            names::line_title(line.kind)
        ),
        15.0,
        true,
    );
    commands.entity(head).add_children(&[icon, who]);
    let voice = commands
        .spawn((
            // The model's words when they have come, the template until then.
            Text::new(format!(
                "«{}»",
                game.oracle
                    .line_voices
                    .get(&line.id)
                    .map_or(names::line_voice(line.kind), String::as_str)
            )),
            font.text(13.0),
            TextColor(VOICE),
            Node {
                width: px(450.0),
                ..default()
            },
        ))
        .id();
    let reward = if line.stake > 0 {
        format!("+{} Стиля; провал — −{}", line.style, line.stake)
    } else {
        format!(
            "+{} Стиля и благосклонность {}",
            line.style,
            names::god_genitive(line.god)
        )
    };
    let goal = commands
        .spawn((
            Text::new(format!(
                "Цель: {} до раунда {}. Награда: {reward}.",
                names::line_goal(&line, &game.game),
                line.deadline
            )),
            font.text(12.0),
            TextColor(INK),
            Node {
                width: px(450.0),
                ..default()
            },
        ))
        .id();
    commands.entity(frame).add_children(&[head, voice, goal]);
    commands.entity(panel).add_child(frame);
}

fn expire_voice(time: Res<Time>, mut shown: Local<(u32, f32)>, mut game: ResMut<Match>) {
    if game.told.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if shown.0 != game.told_serial {
        *shown = (game.told_serial, now);
    }
    if now - shown.1 > VOICE_SECS {
        game.told = None;
    }
}
