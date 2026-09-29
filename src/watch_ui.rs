//! Battles and trials the human is not in (§12, §20.2): they do not take
//! the screen. Each is heard near the camera (`audio.rs`) and marked by an
//! icon over its hex; a click on the icon puts it on the panels from its
//! first event, and «Закрыть» or Esc takes it off again. The human's own
//! always go on screen (`Match::route_show`). A finished show keeps its
//! icon for a while, then goes.

use bevy::prelude::*;
use necromy_rules::PlayerId;

use crate::board::Board;
use crate::hud::{INK, UiFont};
use crate::icons::StatIcon;
use crate::play::{Match, Show, ShowKind};
use crate::stats::StatArt;
use crate::ui_skin::Frame;

/// Seconds a finished show stays to be looked at.
const KEEP_SECS: f32 = 30.0;
/// The icon floats this high over its hex, in metres.
const ICON_HEIGHT: f32 = 1.4;
const ICON: f32 = 28.0;

pub struct WatchUiPlugin;

impl Plugin for WatchUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            crate::InGame,
            (expire, icons, clicks).chain().after(crate::camera::apply),
        );
    }
}

/// The icon of show `id` over the board.
#[derive(Component)]
struct WatchIcon(u32);

#[derive(Component)]
struct WatchLabel;

/// «Закрыть» on the battle and trial panels while the human only watches.
#[derive(Component)]
pub struct WatchClose;

/// A close button for a panel showing someone else's show.
pub fn close_button(commands: &mut Commands, font: &UiFont) -> Entity {
    let label = commands
        .spawn((Text::new("Закрыть (Esc)"), font.bold(13.0), TextColor(INK)))
        .id();
    commands
        .spawn((
            WatchClose,
            Button,
            Node {
                padding: UiRect::axes(px(14.0), px(8.0)),
                ..default()
            },
            Frame::Button,
        ))
        .add_child(label)
        .id()
}

/// Finished shows age, and go once nobody looks at them.
fn expire(time: Res<Time>, mut game: ResMut<Match>) {
    let dt = time.delta_secs();
    let on_screen = game.on_screen;
    let m = game.bypass_change_detection();
    for show in m.shows.iter_mut().filter(|s| s.closed) {
        show.since_done += dt;
    }
    let human = m.human;
    m.shows.retain(|s| {
        let mine = s.who.contains(&human);
        Some(s.id) == on_screen || !s.closed || (!mine && s.since_done < KEEP_SECS)
    });
}

/// One icon per show the human may look at, kept over its hex.
#[allow(clippy::too_many_arguments)]
fn icons(
    mut commands: Commands,
    game: Res<Match>,
    board: Res<Board>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::TableCamera>>,
    mut placed: Query<(Entity, &WatchIcon, &mut Node, &mut Visibility, &Interaction)>,
    mut labels: Query<(&ChildOf, &mut Text), With<WatchLabel>>,
) {
    let (camera, cam) = *camera;
    let wanted: Vec<&Show> = game
        .shows
        .iter()
        .filter(|s| !s.who.contains(&game.human) && game.on_screen != Some(s.id))
        .collect();
    for (entity, icon, mut node, mut visibility, interaction) in &mut placed {
        let Some(show) = wanted.iter().find(|s| s.id == icon.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let at = board.hex_to_world(show.hex) + Vec3::Y * ICON_HEIGHT;
        match camera.world_to_viewport(cam, at) {
            Ok(p) => {
                node.left = px(p.x - ICON / 2.0 - 6.0);
                node.top = px(p.y - ICON / 2.0 - 6.0);
                visibility.set_if_neq(Visibility::Inherited);
            }
            Err(_) => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
        // Who and what, while hovered; a word otherwise.
        let text = if *interaction == Interaction::None {
            word(show)
        } else {
            format!("посмотреть: {}", about(&game, show))
        };
        for (parent, mut label) in &mut labels {
            if parent.parent() == entity && label.0 != text {
                label.0 = text.clone();
            }
        }
    }
    for show in wanted {
        if placed.iter().any(|(_, icon, ..)| icon.0 == show.id) {
            continue;
        }
        let image = match show.kind {
            ShowKind::Battle | ShowKind::Guard => art.icon(StatIcon::Might),
            ShowKind::Trial => trial_face(&game, show)
                .map_or_else(|| art.icon(StatIcon::Might), |f| art.faces[&f].clone()),
        };
        let icon = commands
            .spawn((
                ImageNode::new(image),
                Node {
                    width: px(ICON),
                    height: px(ICON),
                    ..default()
                },
            ))
            .id();
        let label = commands
            .spawn((
                WatchLabel,
                Text::new(word(show)),
                font.text(11.0),
                TextColor(INK),
                TextLayout::no_wrap(),
            ))
            .id();
        commands
            .spawn((
                WatchIcon(show.id),
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    padding: UiRect::all(px(6.0)),
                    column_gap: px(6.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                Frame::Tip,
                // Over the board, under the panels.
                GlobalZIndex(3),
                Visibility::Hidden,
            ))
            .add_children(&[icon, label]);
    }
}

/// A click on an icon puts its show on screen; «Закрыть» and Esc take a
/// watched one off. The human's own show on screen is never pushed aside.
fn clicks(
    keys: Res<ButtonInput<KeyCode>>,
    icons: Query<(&Interaction, &WatchIcon), Changed<Interaction>>,
    close: Query<&Interaction, (Changed<Interaction>, With<WatchClose>)>,
    mut game: ResMut<Match>,
    mut clicked_once: Local<bool>,
) {
    let busy_with_own = game.on_screen.is_some() && !game.watching;
    for (interaction, icon) in &icons {
        if *interaction == Interaction::Pressed && !busy_with_own {
            game.put_on_screen(icon.0, true);
        }
    }
    // Dev aid: `NECROMY_WATCH=click` opens the first finished show of
    // others, as a click on its icon would (screenshots of the panel).
    if !*clicked_once
        && game.on_screen.is_none()
        && std::env::var("NECROMY_WATCH").is_ok_and(|v| v == "click")
        && let Some(id) = game
            .shows
            .iter()
            .find(|s| s.closed && !s.who.contains(&game.human))
            .map(|s| s.id)
    {
        *clicked_once = true;
        game.put_on_screen(id, true);
    }
    let closing =
        close.iter().any(|i| *i == Interaction::Pressed) || keys.just_pressed(KeyCode::Escape);
    if closing && game.watching {
        game.close_show();
    }
}

fn word(show: &Show) -> String {
    let what = match show.kind {
        ShowKind::Battle | ShowKind::Guard => "бой",
        ShowKind::Trial => "испытание",
    };
    if show.done {
        what.to_string()
    } else {
        format!("{what}…")
    }
}

fn about(game: &Match, show: &Show) -> String {
    let names: Vec<String> = show.who.iter().map(|&p: &PlayerId| game.name(p)).collect();
    match show.kind {
        ShowKind::Battle => names.join(" против "),
        ShowKind::Guard => match show.events.first() {
            Some(necromy_rules::Event::GuardAttacked { .. }) => {
                format!("{} против гвардии", names.join(""))
            }
            _ => format!("гвардия против {}", names.join("")),
        },
        ShowKind::Trial => format!("{} на испытании", names.join("")),
    }
}

/// The face a trial show asks for, from its outcome or the board.
fn trial_face(game: &Match, show: &Show) -> Option<necromy_rules::Face> {
    let god = show
        .events
        .iter()
        .find_map(|e| match e {
            necromy_rules::Event::TrialPassed { trial, .. }
            | necromy_rules::Event::TrialFailed { trial, .. } => Some(trial.god),
            _ => None,
        })
        .or_else(|| game.game.trial_at(show.hex).map(|t| t.god))?;
    Some(necromy_rules::trial_face(god))
}
