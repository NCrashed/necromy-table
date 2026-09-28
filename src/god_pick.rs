//! Choosing a god before a match (docs/design.md §3, §13): a card per god
//! with its champion breathing in place, the element and its side of the
//! yin-yang rhythm, the land, the champion's stats, the god's stages from
//! light to dark, and the champion's oath and manner. The single player
//! setup and the network lobby both show these cards; later each god will
//! offer several champions here.

use bevy::prelude::*;
use necromy_rules::{Champion, Character, God};

use crate::hud::{INK, UiFont};
use crate::icons::{self, StatIcon};
use crate::names;
use crate::ui_skin::{Accent, Frame};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
pub const CARD_W: f32 = 212.0;
/// Champion sheets (`token.rs`): 96 px cells, six columns, idle south first.
const CELL: u32 = 96;
const SHEET_COLUMNS: u32 = 6;
const IDLE_FRAMES: usize = 4;
const IDLE_FRAME_SECS: f32 = 0.16;

pub struct GodPickPlugin;

impl Plugin for GodPickPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_art)
            .add_systems(Update, breathe);
    }
}

#[derive(Resource)]
pub struct GodPickArt {
    sheets: [Option<Handle<Image>>; 5],
    layout: Handle<TextureAtlasLayout>,
    elements: [Handle<Image>; 5],
    stats: [Handle<Image>; 4],
}

fn load_art(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let sheets = God::ALL.map(|god| {
        let path = format!("sprites/{}-champion.png", god.name().to_lowercase());
        std::path::Path::new("assets")
            .join(&path)
            .exists()
            .then(|| assets.load(path))
    });
    commands.insert_resource(GodPickArt {
        sheets,
        layout: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(CELL),
            SHEET_COLUMNS,
            8,
            None,
            None,
        )),
        elements: God::ALL.map(|g| images.add(icons::element_icon(g.element(), g.accent()))),
        stats: [
            StatIcon::Might,
            StatIcon::Health,
            StatIcon::Wits,
            StatIcon::Spirit,
        ]
        .map(|i| images.add(icons::stat_icon(i))),
    });
}

/// Whose god it is at the table.
pub enum Holder {
    Free,
    Mine,
    Taken(String),
}

/// A champion idling on a god card.
#[derive(Component)]
struct Breathing;

fn breathe(time: Res<Time>, mut portraits: Query<&mut ImageNode, With<Breathing>>) {
    let frame = (time.elapsed_secs() / IDLE_FRAME_SECS) as usize % IDLE_FRAMES;
    for mut image in &mut portraits {
        if let Some(atlas) = image.texture_atlas.as_mut()
            && atlas.index != frame
        {
            atlas.index = frame;
        }
    }
}

fn god_color(god: God) -> Color {
    let [r, g, b] = god.accent();
    Color::srgb_u8(r, g, b)
}

fn text(commands: &mut Commands, font: TextFont, s: String, color: Color) -> Entity {
    commands
        .spawn((
            Text::new(s),
            font,
            TextColor(color),
            Node {
                max_width: px(CARD_W - 28.0),
                ..default()
            },
        ))
        .id()
}

/// One god's card. `action` makes it a button (for a god one may take).
pub fn god_card(
    commands: &mut Commands,
    font: &UiFont,
    art: &GodPickArt,
    god: God,
    holder: Holder,
    action: Option<impl Bundle>,
) -> Entity {
    let taken = matches!(holder, Holder::Taken(_));
    let card = commands
        .spawn(Node {
            width: px(CARD_W),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(5.0),
            padding: UiRect::all(px(14.0)),
            ..default()
        })
        .id();
    match holder {
        Holder::Mine => commands.entity(card).insert((Frame::Plate, Accent(GOLD))),
        _ => commands
            .entity(card)
            .insert((Frame::Tip, Accent(god_color(god)))),
    };
    if let Some(action) = action {
        commands.entity(card).insert((action, Button));
    }
    let dim = if taken { 0.45 } else { 1.0 };

    // The champion, 2×, cropped to the figure.
    let window = commands
        .spawn(Node {
            width: px(150.0),
            height: px(124.0),
            overflow: Overflow::clip(),
            ..default()
        })
        .id();
    if let Some(sheet) = &art.sheets[god.index()] {
        let figure = commands
            .spawn((
                Breathing,
                ImageNode::from_atlas_image(
                    sheet.clone(),
                    TextureAtlas {
                        layout: art.layout.clone(),
                        index: 0,
                    },
                )
                .with_color(Color::srgb(dim, dim, dim)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(-21.0),
                    top: px(-42.0),
                    width: px(192.0),
                    height: px(192.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(window).add_child(figure);
    }

    let name = text(
        commands,
        font.bold(20.0),
        names::god(god).to_string(),
        god_color(god).lighter(0.1),
    );
    let title = text(
        commands,
        font.text(13.0),
        names::champion_title(god).to_string(),
        INK,
    );

    // Element, rhythm, land.
    let element = god.element();
    let origin = commands
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(6.0),
            ..default()
        })
        .id();
    let icon = commands
        .spawn((
            ImageNode::new(art.elements[god.index()].clone()),
            Node {
                width: px(20.0),
                height: px(20.0),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let where_ = text(
        commands,
        font.text(12.0),
        format!("{} · {}", names::element(element), names::yin_yang(element)),
        DIM,
    );
    commands.entity(origin).add_children(&[icon, where_]);
    let land = text(commands, font.text(12.0), names::land(god).to_string(), DIM);

    // Might, body, wits, spirit.
    let stats = commands
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(4.0),
            ..default()
        })
        .id();
    for (icon, value) in art.stats.iter().zip(Champion::stats_of(god)) {
        let i = commands
            .spawn((
                ImageNode::new(icon.clone()),
                Node {
                    width: px(20.0),
                    height: px(20.0),
                    ..default()
                },
            ))
            .id();
        let v = text(commands, font.bold(14.0), value.to_string(), INK);
        commands.entity(v).insert(Node {
            margin: UiRect::right(px(6.0)),
            ..default()
        });
        commands.entity(stats).add_children(&[i, v]);
    }

    let stages = text(
        commands,
        font.text(11.0),
        format!(
            "{} → {} → {}",
            names::stage(god, 0),
            names::stage(god, 1),
            names::stage(god, 2)
        ),
        god_color(god).lighter(0.2),
    );
    let ch = Character::of(god);
    let oath = text(
        commands,
        font.text(11.0),
        format!(
            "Клятва: не {}.\nМанера: {}.",
            names::deed(ch.oath),
            names::deed(ch.manner)
        ),
        DIM,
    );

    let (state, color) = match holder {
        Holder::Free => ("выбрать".to_string(), DIM),
        Holder::Mine => ("твой бог".to_string(), GOLD),
        Holder::Taken(who) => (format!("занят: {who}"), DIM),
    };
    let state = text(commands, font.bold(13.0), state, color);
    commands.entity(state).insert(Node {
        margin: UiRect::top(px(4.0)),
        ..default()
    });

    commands.entity(card).add_children(&[
        window, name, title, origin, land, stats, stages, oath, state,
    ]);
    card
}

/// A row of the five cards.
pub fn row(commands: &mut Commands) -> Entity {
    commands
        .spawn(Node {
            column_gap: px(10.0),
            align_items: AlignItems::Stretch,
            ..default()
        })
        .id()
}
