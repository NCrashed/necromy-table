//! Champion stats as icons and cell bars instead of text lines.
//!
//! Bottom left: the human's champion. Bottom right: a portrait per seat;
//! hovering one pops up that champion's sheet in the same form. Filled cells
//! are what the champion has now, hollow ones what they could have.

use std::collections::HashMap;

use bevy::prelude::*;
use necromy_rules::{GUARD_THRESHOLD, God, PlayerId};

use crate::board::Hovered;
use crate::hud::{INK, PANEL, UiFont};
use crate::icons::{self, StatIcon};
use crate::names;
use crate::play::Match;
use crate::token;

const ICON: f32 = 32.0;
const CELL_W: f32 = 9.0;
const CELL_H: f32 = 14.0;

pub const HEALTH: Color = Color::srgb(0.84, 0.20, 0.23);
const SPIRIT: Color = Color::srgb(0.47, 0.43, 0.93);
const THREAT: Color = Color::srgb(0.93, 0.50, 0.16);
const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoveredSeat>()
            .add_systems(Startup, (make_art, spawn_panels).chain())
            .add_systems(Update, (track_seat_hover, place_popup).chain())
            .add_systems(
                Update,
                (rebuild_mine, rebuild_seats, rebuild_gods).run_if(resource_changed::<Match>),
            )
            .add_systems(
                Update,
                rebuild_popup
                    .after(track_seat_hover)
                    .run_if(resource_changed::<Match>.or_else(resource_changed::<HoveredSeat>)),
            );
    }
}

#[derive(Resource)]
pub struct StatArt {
    icons: HashMap<StatIcon, Handle<Image>>,
    /// Ward shields per element (`Element::index`).
    pub wards: [Handle<Image>; 5],
    /// Token sprite per seat.
    pub portraits: Vec<Handle<Image>>,
    /// The royal guard's sprite.
    pub guard: Handle<Image>,
    /// Die faces as icons.
    pub faces: HashMap<necromy_rules::Face, Handle<Image>>,
    /// Element icons per god (`God::index`).
    pub gods: [Handle<Image>; 5],
    offering: Handle<Image>,
}

impl StatArt {
    pub fn icon(&self, icon: StatIcon) -> Handle<Image> {
        self.icons[&icon].clone()
    }
}

/// The seat whose portrait is under the mouse.
#[derive(Resource, Default, PartialEq, Eq)]
struct HoveredSeat(Option<(Card, HoverSource)>);

/// Whose sheet the popup shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Card {
    Champion(PlayerId),
    Guard,
}

/// Where the mouse found the champion: their portrait, or their hex.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HoverSource {
    Portrait,
    Board,
}

#[derive(Component)]
struct MyPanel;

#[derive(Component)]
struct Seats;

#[derive(Component)]
struct Seat(PlayerId);

#[derive(Component)]
struct Popup;

#[derive(Component)]
struct GodsPanel;

fn make_art(mut commands: Commands, game: Res<Match>, mut images: ResMut<Assets<Image>>) {
    let icons = StatIcon::ALL
        .into_iter()
        .map(|i| (i, images.add(icons::stat_icon(i))))
        .collect();
    let wards = God::ALL.map(|g| images.add(icons::ward_icon(g.accent())));
    let portraits = game
        .game
        .champions()
        .iter()
        .map(|c| images.add(token::placeholder_sprite(c.god.accent())))
        .collect();
    let gods = God::ALL.map(|g| images.add(icons::element_icon(g.element(), g.accent())));
    let offering = images.add(icons::offering_icon());
    let guard = images.add(token::placeholder_sprite(token::GUARD_COLOR));
    let faces = [
        necromy_rules::Face::Strike,
        necromy_rules::Face::Shield,
        necromy_rules::Face::Sun,
        necromy_rules::Face::Moon,
        necromy_rules::Face::Element,
        necromy_rules::Face::Blank,
    ]
    .into_iter()
    .map(|f| (f, images.add(crate::dice::face_icon(f))))
    .collect();
    commands.insert_resource(StatArt {
        icons,
        wards,
        portraits,
        gods,
        offering,
        guard,
        faces,
    });
}

fn spawn_panels(mut commands: Commands) {
    commands.spawn((
        GodsPanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(10.0),
            right: px(10.0),
            ..default()
        },
    ));
    commands.spawn((
        MyPanel,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(10.0),
            left: px(10.0),
            ..default()
        },
    ));
    commands.spawn((
        Seats,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(10.0),
            right: px(10.0),
            column_gap: px(4.0),
            ..default()
        },
    ));
    commands.spawn((
        Popup,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(100.0),
            right: px(10.0),
            ..default()
        },
        Visibility::Hidden,
        GlobalZIndex(10),
    ));
}

fn rebuild_mine(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<Entity, With<MyPanel>>,
) {
    commands.entity(*panel).despawn_related::<Children>();
    let sheet = stat_sheet(&mut commands, &art, &font, &game, game.human, true);
    commands.entity(*panel).add_child(sheet);
}

fn rebuild_seats(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    seats: Single<Entity, With<Seats>>,
) {
    commands.entity(*seats).despawn_related::<Children>();
    let g = &game.game;
    for p in g.players() {
        let Some(c) = g.champion(p) else { continue };
        // The seat whose move it is gets a gold frame.
        let border = if g.awaiting().contains(&p) {
            GOLD
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.15)
        };
        let seat = commands
            .spawn((
                Seat(p),
                Button,
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(3.0)),
                    border: UiRect::all(px(2.0)),
                    row_gap: px(2.0),
                    ..default()
                },
                BorderColor::all(border),
                BackgroundColor(PANEL),
            ))
            .id();
        let crown = icon_node(
            &mut commands,
            art.icon(StatIcon::Crown),
            16.0,
            g.dominant() == Some(p),
        );
        let portrait = commands
            .spawn((
                ImageNode::new(art.portraits[p.0 as usize].clone()),
                Node {
                    width: px(32.0),
                    height: px(48.0),
                    ..default()
                },
            ))
            .id();
        let hp = bar(&mut commands, c.hp, c.body, HEALTH, None, 5.0, 7.0);
        commands.entity(seat).add_children(&[crown, portrait, hp]);
        commands.entity(*seats).add_child(seat);
    }
}

fn track_seat_hover(
    seats: Query<(&Interaction, &Seat)>,
    board_hover: Res<Hovered>,
    game: Res<Match>,
    mut hovered: ResMut<HoveredSeat>,
) {
    // Dev aid: `NECROMY_SEAT=n` pins the popup on a seat for screenshots.
    let pinned = std::env::var("NECROMY_SEAT")
        .ok()
        .and_then(|s| s.parse().ok())
        .map(|n| (Card::Champion(PlayerId(n)), HoverSource::Portrait));
    let portrait = || {
        seats
            .iter()
            .find(|(i, _)| **i != Interaction::None)
            .map(|(_, s)| (Card::Champion(s.0), HoverSource::Portrait))
    };
    // A champion or the guard standing on the hovered hex.
    let on_board = || {
        let hex = board_hover.0?;
        let g = &game.game;
        let card = match g.occupant(hex) {
            Some(p) => Card::Champion(p),
            None if g.guard().is_some_and(|guard| guard.hex == hex) => Card::Guard,
            None => return None,
        };
        Some((card, HoverSource::Board))
    };
    let seat = pinned.or_else(portrait).or_else(on_board);
    hovered.set_if_neq(HoveredSeat(seat));
}

fn rebuild_popup(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    hovered: Res<HoveredSeat>,
    popup: Single<(Entity, &mut Visibility), With<Popup>>,
) {
    let (popup, mut visibility) = popup.into_inner();
    commands.entity(popup).despawn_related::<Children>();
    let Some((card, _)) = hovered.0 else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    let sheet = match card {
        Card::Champion(p) => stat_sheet(&mut commands, &art, &font, &game, p, false),
        Card::Guard => guard_sheet(&mut commands, &art, &font, &game),
    };
    commands.entity(popup).add_child(sheet);
}

/// Everything about one champion: bars for what runs out, icons with numbers
/// for the rest, badges for states, and their character.
fn stat_sheet(
    commands: &mut Commands,
    art: &StatArt,
    font: &UiFont,
    m: &Match,
    player: PlayerId,
    mine: bool,
) -> Entity {
    let g = &m.game;
    let sheet = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8.0)),
                row_gap: px(2.0),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .id();
    let Some(c) = g.champion(player) else {
        return sheet;
    };

    let mut rows = Vec::new();

    // Header: portrait, name, crown.
    let header = row(commands);
    let portrait = commands
        .spawn((
            ImageNode::new(art.portraits[player.0 as usize].clone()),
            Node {
                width: px(16.0),
                height: px(24.0),
                ..default()
            },
        ))
        .id();
    let name = label(commands, font, &m.name(player), 15.0, true);
    let crown = icon_node(
        commands,
        art.icon(StatIcon::Crown),
        20.0,
        g.dominant() == Some(player),
    );
    commands
        .entity(header)
        .add_children(&[portrait, name, crown]);
    rows.push(header);

    // What runs out: cells.
    let threat = g.threat(player);
    for (icon, now, max, color, warn) in [
        (StatIcon::Health, c.hp, c.body, HEALTH, None),
        (StatIcon::Spirit, c.spirit_points, c.spirit, SPIRIT, None),
        (
            StatIcon::Threat,
            threat,
            threat.max(GUARD_THRESHOLD),
            THREAT,
            Some(GUARD_THRESHOLD),
        ),
    ] {
        let r = row(commands);
        let i = icon_node(commands, art.icon(icon), ICON, true);
        let b = bar(commands, now, max, color, warn, CELL_W, CELL_H);
        let t = label(commands, font, &format!("{now}/{max}"), 13.0, false);
        commands.entity(r).add_children(&[i, b, t]);
        rows.push(r);
    }

    // Numbers.
    let hand = g.hand(player).len();
    let mut numbers = vec![
        (StatIcon::Might, c.might.to_string()),
        (StatIcon::Wits, c.wits.to_string()),
        (StatIcon::Cards, format!("{hand}/{}", c.hand_limit())),
        (StatIcon::Style, g.style(player).to_string()),
    ];
    if g.window().is_none() && g.current_player() == player {
        numbers.push((StatIcon::Moves, g.move_points().to_string()));
    }
    let r = row(commands);
    for (icon, value) in numbers {
        let i = icon_node(commands, art.icon(icon), 24.0, true);
        let t = label(commands, font, &value, 14.0, true);
        commands.entity(r).add_children(&[i, t]);
    }
    rows.push(r);

    // States.
    let curses = g.curses(player);
    if c.ward.is_some() || c.rooted || !curses.is_empty() {
        let r = row(commands);
        if let Some(ward) = c.ward {
            let i = icon_node(commands, art.wards[ward.index()].clone(), 24.0, true);
            let t = label(
                commands,
                font,
                &format!("оберег: {}", names::element(ward)),
                12.0,
                false,
            );
            commands.entity(r).add_children(&[i, t]);
        }
        if c.rooted {
            let i = icon_node(commands, art.icon(StatIcon::Rooted), 24.0, true);
            let t = label(commands, font, "скован", 12.0, false);
            commands.entity(r).add_children(&[i, t]);
        }
        for god in curses {
            let i = icon_node(commands, art.icon(StatIcon::Curse), 24.0, true);
            let t = label(
                commands,
                font,
                &format!("проклятие {}", names::god_genitive(*god)),
                12.0,
                false,
            );
            commands.entity(r).add_children(&[i, t]);
        }
        rows.push(r);
    }

    // Character: what earns and costs Style at dusk (§6.2).
    if let Some(ch) = g.character(player) {
        let who = if mine {
            "Твоя клятва"
        } else {
            "Клятва"
        };
        let text = format!(
            "{who}: не {}.\nМанера: {}.",
            names::deed(ch.oath),
            names::deed(ch.manner)
        );
        let t = commands
            .spawn((
                Text::new(text),
                font.text(11.0),
                TextColor(Color::srgb(0.80, 0.78, 0.72)),
                Node {
                    max_width: px(230.0),
                    margin: UiRect::top(px(4.0)),
                    ..default()
                },
            ))
            .id();
        rows.push(t);
    }

    commands.entity(sheet).add_children(&rows);
    sheet
}

pub fn row(commands: &mut Commands) -> Entity {
    commands
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(6.0),
            ..default()
        })
        .id()
}

pub fn label(commands: &mut Commands, font: &UiFont, text: &str, size: f32, bold: bool) -> Entity {
    let f = if bold {
        font.bold(size)
    } else {
        font.text(size)
    };
    // Labels are one line: never let a narrow min-content pass wrap them, or
    // flex layouts grow tall around the would-be wrapped text.
    commands
        .spawn((
            Text::new(text.to_string()),
            f,
            TextColor(INK),
            TextLayout::no_wrap(),
        ))
        .id()
}

/// A square pixel icon; `shown: false` keeps its space but draws nothing, so
/// rows line up whether or not a badge is present.
pub fn icon_node(commands: &mut Commands, image: Handle<Image>, size: f32, shown: bool) -> Entity {
    commands
        .spawn((
            ImageNode::new(image),
            Node {
                width: px(size),
                height: px(size),
                ..default()
            },
            if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ))
        .id()
}

/// `now` filled cells out of `max`; from `warn` on, hollow cells get a red
/// frame (the guard's threshold on the Threat bar).
pub fn bar(
    commands: &mut Commands,
    now: u8,
    max: u8,
    color: Color,
    warn: Option<u8>,
    w: f32,
    h: f32,
) -> Entity {
    let bar = commands
        .spawn(Node {
            column_gap: px(2.0),
            ..default()
        })
        .id();
    for i in 0..max {
        let filled = i < now;
        let warned = warn.is_some_and(|t| i + 1 >= t);
        let frame = if warned {
            Color::srgb(0.95, 0.2, 0.15)
        } else {
            color
        };
        let cell = commands
            .spawn((
                Node {
                    width: px(w),
                    height: px(h),
                    border: UiRect::all(px(1.0)),
                    ..default()
                },
                BorderColor::all(frame),
                BackgroundColor(if filled { color } else { Color::NONE }),
            ))
            .id();
        commands.entity(bar).add_child(cell);
    }
    bar
}

const RELIEF: Color = Color::srgb(0.50, 0.76, 0.96);
const PRESSURE: Color = Color::srgb(0.92, 0.30, 0.24);
const HOLLOW: Color = Color::srgba(1.0, 1.0, 1.0, 0.3);

/// The five gods (§5): stage as cells from light to dark, pressure as a
/// two-sided bar (a full side shifts the stage at dusk), your favour.
fn rebuild_gods(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<Entity, With<GodsPanel>>,
) {
    commands.entity(*panel).despawn_related::<Children>();
    let g = &game.game;
    let sheet = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8.0)),
                row_gap: px(3.0),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .id();
    let title = label(&mut commands, &font, "Боги", 14.0, true);
    let legend = commands
        .spawn((
            Text::new("стадия · давление: ◀ светлеет | темнеет ▶ · твоя благосклонность"),
            font.text(11.0),
            TextColor(Color::srgb(0.75, 0.73, 0.68)),
        ))
        .id();
    commands.entity(sheet).add_children(&[title, legend]);

    for god in God::ALL {
        let r = row(&mut commands);
        let accent = {
            let [red, green, blue] = god.accent();
            Color::srgb_u8(red, green, blue)
        };
        let icon = icon_node(&mut commands, art.gods[god.index()].clone(), 24.0, true);
        let name = fixed_label(&mut commands, &font, names::god(god), 13.0, true, 62.0);

        let stage = g.stage(god);
        let cells = commands
            .spawn(Node {
                column_gap: px(2.0),
                ..default()
            })
            .id();
        for i in 0..necromy_rules::STAGES {
            let tone = match i {
                0 => accent.mix(&Color::WHITE, 0.45),
                1 => accent,
                _ => accent.mix(&Color::BLACK, 0.55),
            };
            let cell = cell(&mut commands, i <= stage, tone, tone);
            commands.entity(cells).add_child(cell);
        }
        let stage_name = fixed_label(
            &mut commands,
            &font,
            names::stage(god, stage),
            12.0,
            false,
            96.0,
        );

        let p = g.pressure(god);
        let t = necromy_rules::STAGE_THRESHOLD;
        let pressure = commands
            .spawn(Node {
                column_gap: px(2.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .id();
        // Left half fills from the middle outwards as relief builds.
        for i in 0..t {
            let c = cell(&mut commands, p <= -(t - i), RELIEF, HOLLOW);
            commands.entity(pressure).add_child(c);
        }
        let divider = commands
            .spawn((
                Node {
                    width: px(2.0),
                    height: px(18.0),
                    ..default()
                },
                BackgroundColor(INK),
            ))
            .id();
        commands.entity(pressure).add_child(divider);
        for i in 0..t {
            let c = cell(&mut commands, p > i, PRESSURE, HOLLOW);
            commands.entity(pressure).add_child(c);
        }

        let bowl = icon_node(&mut commands, art.offering.clone(), 20.0, true);
        let favor = label(
            &mut commands,
            &font,
            &g.favor(game.human, god).to_string(),
            13.0,
            true,
        );
        commands
            .entity(r)
            .add_children(&[icon, name, cells, stage_name, pressure, bowl, favor]);
        commands.entity(sheet).add_child(r);
    }
    commands.entity(*panel).add_child(sheet);
}

/// One bar cell: filled with `fill`, or hollow in a `frame`.
fn cell(commands: &mut Commands, filled: bool, fill: Color, frame: Color) -> Entity {
    commands
        .spawn((
            Node {
                width: px(CELL_W),
                height: px(CELL_H),
                border: UiRect::all(px(1.0)),
                ..default()
            },
            BorderColor::all(if filled { fill } else { frame }),
            BackgroundColor(if filled { fill } else { Color::NONE }),
        ))
        .id()
}

/// A label of fixed width, so rows line up in columns.
fn fixed_label(
    commands: &mut Commands,
    font: &UiFont,
    text: &str,
    size: f32,
    bold: bool,
    width: f32,
) -> Entity {
    let f = if bold {
        font.bold(size)
    } else {
        font.text(size)
    };
    commands
        .spawn((
            Text::new(text.to_string()),
            f,
            TextColor(INK),
            Node {
                width: px(width),
                ..default()
            },
        ))
        .id()
}

/// Estimated popup size, to keep it on screen next to the cursor.
const POPUP_SIZE: Vec2 = Vec2::new(270.0, 270.0);

/// A portrait's popup sits above the portraits; a champion hovered on the
/// board gets theirs beside the cursor, left of the tile tooltip.
fn place_popup(
    hovered: Res<HoveredSeat>,
    board_hover: Res<Hovered>,
    board: Res<crate::board::Board>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::TableCamera>>,
    mut popup: Single<&mut Node, With<Popup>>,
) {
    // A pinned hover (`NECROMY_HOVER` screenshots) anchors at its hex.
    let (camera, cam_transform) = *camera;
    let anchor = if std::env::var_os("NECROMY_HOVER").is_some() {
        board_hover.0.and_then(|h| {
            camera
                .world_to_viewport(cam_transform, board.hex_to_world(h))
                .ok()
        })
    } else {
        window.cursor_position()
    };
    let place = match (hovered.0, anchor) {
        (Some((_, HoverSource::Board)), Some(cursor)) => {
            let size = window.size();
            // Left of the cursor; right of the tooltip if there is no room.
            let x = if cursor.x - POPUP_SIZE.x - 12.0 >= 10.0 {
                cursor.x - POPUP_SIZE.x - 12.0
            } else {
                cursor.x + 360.0
            };
            let y = (cursor.y - 40.0).clamp(10.0, (size.y - POPUP_SIZE.y - 10.0).max(10.0));
            Some((x, y))
        }
        _ => None,
    };
    let (left, top, right, bottom) = match place {
        Some((x, y)) => (px(x), px(y), Val::Auto, Val::Auto),
        None => (Val::Auto, Val::Auto, px(10.0), px(100.0)),
    };
    if popup.left != left || popup.top != top || popup.right != right || popup.bottom != bottom {
        popup.left = left;
        popup.top = top;
        popup.right = right;
        popup.bottom = bottom;
    }
}

/// The royal guard (§6.5): its dice, its pace, whom it hunts and why.
fn guard_sheet(commands: &mut Commands, art: &StatArt, font: &UiFont, m: &Match) -> Entity {
    let g = &m.game;
    let sheet = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8.0)),
                row_gap: px(4.0),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .id();

    let header = row(commands);
    let portrait = commands
        .spawn((
            ImageNode::new(art.guard.clone()),
            Node {
                width: px(16.0),
                height: px(24.0),
                ..default()
            },
        ))
        .id();
    let name = label(commands, font, "Королевская гвардия", 15.0, true);
    commands.entity(header).add_children(&[portrait, name]);

    let numbers = row(commands);
    let sword = icon_node(commands, art.icon(StatIcon::Might), 24.0, true);
    let dice = label(
        commands,
        font,
        &format!("{} кубика", necromy_rules::GUARD_DICE),
        14.0,
        true,
    );
    let boot = icon_node(commands, art.icon(StatIcon::Moves), 24.0, true);
    let steps = label(
        commands,
        font,
        &format!("{} клетки за ход мира", necromy_rules::GUARD_STEPS),
        13.0,
        false,
    );
    commands
        .entity(numbers)
        .add_children(&[sword, dice, boot, steps]);
    let mut rows = vec![header, numbers];

    // Whom it will walk to next world phase: the loudest champion now, not
    // necessarily the one it came for. Nobody loud enough: it leaves.
    let next = g.hunted();
    if next.is_none() {
        let calm = label(commands, font, "Уходит: на столе тихо.", 13.0, true);
        rows.push(calm);
    }
    if let Some(target) = next {
        let hunt = row(commands);
        let text = label(commands, font, "идёт за:", 13.0, false);
        let face = commands
            .spawn((
                ImageNode::new(art.portraits[target.0 as usize].clone()),
                Node {
                    width: px(16.0),
                    height: px(24.0),
                    ..default()
                },
            ))
            .id();
        let who = label(commands, font, &m.name(target), 13.0, true);
        commands.entity(hunt).add_children(&[text, face, who]);

        let loud = row(commands);
        let threat = g.threat(target);
        let icon = icon_node(commands, art.icon(StatIcon::Threat), 24.0, true);
        let t = necromy_rules::GUARD_THRESHOLD;
        let b = bar(
            commands,
            threat,
            threat.max(t),
            Color::srgb(0.93, 0.50, 0.16),
            Some(t),
            CELL_W,
            CELL_H,
        );
        let n = label(commands, font, &format!("{threat}/{t}"), 13.0, false);
        commands.entity(loud).add_children(&[icon, b, n]);
        rows.push(hunt);
        rows.push(loud);
    }

    let rules = commands
        .spawn((
            Text::new(format!(
                "Выходит, когда у кого-то Угроза {} и больше, и идёт к самому шумному. \
                 Рядом с ним бьёт кубиками; удар снимает {} Угрозы. Грань «Стихия» \
                 гвардии ломает оберег дерева: железо рубит рост.",
                necromy_rules::GUARD_THRESHOLD,
                necromy_rules::GUARD_RELIEF
            )),
            font.text(11.0),
            TextColor(Color::srgb(0.80, 0.78, 0.72)),
            Node {
                width: px(250.0),
                margin: UiRect::top(px(4.0)),
                ..default()
            },
        ))
        .id();
    rows.push(rules);
    commands.entity(sheet).add_children(&rows);
    sheet
}
