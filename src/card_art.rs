//! How a card looks (docs/design.md §9): a frame drawn in code in its
//! element's colour, a PixelLab illustration (`assets/cards/`), and the
//! card's facts laid over it: Spirit cost in a gem, element, name, kind and
//! timing, text, how the god's stage bends it, and the die face it gives
//! when burned in battle.
//!
//! The frame is drawn at exactly the card's size, so it stays crisp pixel
//! art; the illustrations are 64 px, shown at 2×.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use necromy_rules::{CardDef, Element, Game, God};

use crate::hud::{INK, UiFont};
use crate::names;
use crate::stats::StatArt;

pub const CARD_W: f32 = 144.0;
pub const CARD_H: f32 = 216.0;
/// The illustration window, in the frame's pixels.
const ART: (u32, u32, u32, u32) = (8, 8, 128, 96);
/// The name ribbon's top and height.
const RIBBON: (u32, u32) = (106, 18);
/// Where the text field starts and the bottom strip starts.
const TEXT_TOP: u32 = 126;
const STRIP_TOP: u32 = 196;

pub struct CardArtPlugin;

impl Plugin for CardArtPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, make_card_art);
    }
}

#[derive(Resource)]
pub struct CardArt {
    /// Per element (`Element::index`), and the last one for cards without.
    frames: [Handle<Image>; 6],
    art: HashMap<&'static str, Handle<Image>>,
    spirit: Handle<Image>,
}

/// The illustration of each card, by its name in the pool.
fn art_file(name: &str) -> Option<&'static str> {
    Some(match name {
        "Побег сквозь камень" => "grow-through-stone",
        "Цепкий корень" => "grasping-root",
        "Живица" => "resin",
        "Шипы чащи" => "thicket-thorns",
        "Семя в мёртвом" => "seed-in-the-dead",
        "Пламя пира" => "feast-flame",
        "Сжечь как топливо" => "burn-as-fuel",
        "Второе блюдо" => "second-course",
        "Жар в крови" => "heat-in-blood",
        "Искра" => "spark",
        "Пир урожая" => "harvest-feast",
        "Тишь" => "hush",
        "Оковы" => "shackles",
        "Упокоить" => "lay-to-rest",
        "Бремя" => "burden",
        "Отзвучавшая нота" => "faded-note",
        "Власяница" => "hair-shirt",
        "Именной оберег" => "named-ward",
        "Вписать в легион" => "write-into-legion",
        "Реестр" => "registry",
        "Приговор порядка" => "sentence-of-order",
        "Присяга" => "oath",
        "Растворить душу" => "dissolve-soul",
        "Туманный шаг" => "mist-step",
        "Морок" => "glamour",
        "Дымная ладонь" => "smoky-palm",
        "Бирюзовый оберег" => "turquoise-ward",
        "Пелена" => "veil",
        "Короткий путь" => "shortcut",
        "Бинт" => "bandage",
        _ => return None,
    })
}

fn rgb(c: Color) -> [u8; 3] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
}

fn scale([r, g, b]: [u8; 3], k: f32) -> [u8; 3] {
    [r, g, b].map(|v| (v as f32 * k).min(255.0) as u8)
}

/// The frame of a card of `element`: a bevelled bronze rim, the art window
/// on the element's dark tone, a ribbon for the name, a field for the text,
/// a strip for the battle face.
fn frame(element: Option<Element>) -> Image {
    let (w, h) = (CARD_W as u32, CARD_H as u32);
    let mut image = Image::new_fill(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let tone = rgb(names::element_color(element));
    let ink = [20, 15, 24];
    let bronze = [[212, 170, 96], [168, 120, 60], [92, 62, 36]];
    let body = [28, 24, 32];
    let field = [40, 35, 44];
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    let mut put = |x: u32, y: u32, c: [u8; 3]| {
        let i = ((y * w + x) * 4) as usize;
        data[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
    };
    let inside = |x: u32, y: u32, (rx, ry, rw, rh): (u32, u32, u32, u32)| {
        x >= rx && x < rx + rw && y >= ry && y < ry + rh
    };
    let (ax, ay, aw, ah) = ART;
    let (ry, rh) = RIBBON;
    for y in 0..h {
        for x in 0..w {
            // Rounded outer corners.
            let corner = (x < 2 || x >= w - 2) && (y < 2 || y >= h - 2);
            if corner && (x.min(w - 1 - x) + y.min(h - 1 - y) < 2) {
                continue;
            }
            let edge = x.min(w - 1 - x).min(y.min(h - 1 - y));
            let lit = x < w - 1 - x && y < h - 1 - y;
            let c = match edge {
                0 => ink,
                // Bevel: light from the top left.
                1 if lit => bronze[0],
                1 => bronze[2],
                2 | 3 => bronze[1],
                4 => ink,
                _ if inside(x, y, (ax - 1, ay - 1, aw + 2, ah + 2)) && !inside(x, y, ART) => ink,
                _ if inside(x, y, ART) => {
                    // The element's dark tone, a touch lighter towards the top.
                    let k = 0.28 + 0.12 * (1.0 - (y - ay) as f32 / ah as f32);
                    scale(tone, k)
                }
                _ if y >= ry && y < ry + rh => {
                    // Ribbon with notched ends.
                    let notch = (y as i32 - (ry + rh / 2) as i32).unsigned_abs();
                    let from_edge = x.min(w - 1 - x);
                    if from_edge < 5 + (rh / 2 - notch.min(rh / 2)) / 3 {
                        body
                    } else if y == ry || y == ry + rh - 1 {
                        ink
                    } else if y == ry + 1 {
                        scale(tone, 1.0)
                    } else {
                        scale(tone, 0.72)
                    }
                }
                _ if (TEXT_TOP..STRIP_TOP).contains(&y) => field,
                _ if y == STRIP_TOP => bronze[2],
                _ => body,
            };
            put(x, y, c);
        }
    }
    // Gold studs at the rim's corners.
    for (cx, cy) in [(3, 3), (w - 5, 3), (3, h - 5), (w - 5, h - 5)] {
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(cx + dx, cy + dy, [250, 214, 120]);
        }
    }
    image
}

fn make_card_art(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    let frames = [
        Some(Element::Wood),
        Some(Element::Fire),
        Some(Element::Earth),
        Some(Element::Metal),
        Some(Element::Water),
        None,
    ]
    .map(|e| images.add(frame(e)));
    let art = necromy_rules::cards::POOL
        .iter()
        .filter_map(|d| art_file(d.name))
        .map(|f| (f, assets.load(format!("cards/{f}.png"))))
        .collect();
    commands.insert_resource(CardArt {
        frames,
        art,
        spirit: assets.load("cards/spirit-crystal.png"),
    });
}

/// How a card is drawn where it is shown.
pub struct CardLook {
    /// Can be played (or burned) right now; otherwise it is dimmed.
    pub usable: bool,
    /// Gold outline: aimed or marked to burn, or the colour of a threat.
    pub outline: Option<Color>,
    /// Lines under the text, e.g. what a card aimed at you did.
    pub extra: Vec<(String, Color)>,
}

fn at(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(left),
        top: px(top),
        width: px(width),
        height: px(height),
        ..default()
    }
}

/// One card as a UI node, `CARD_W`×`CARD_H`.
pub fn card_node(
    commands: &mut Commands,
    font: &UiFont,
    art: &CardArt,
    stats: &StatArt,
    g: &Game,
    def: &CardDef,
    look: CardLook,
) -> Entity {
    let dim = if look.usable { 1.0 } else { 0.5 };
    let tint = Color::srgb(dim, dim, dim);
    let text_alpha = if look.usable { 1.0 } else { 0.6 };
    let frame_index = def.element.map_or(5, |e| e.index());
    let mut card = commands.spawn((
        Node {
            width: px(CARD_W),
            height: px(CARD_H),
            flex_shrink: 0.0,
            ..default()
        },
        ImageNode::new(art.frames[frame_index].clone()).with_color(tint),
    ));
    if let Some(color) = look.outline {
        card.insert(Outline::new(px(3.0), px(1.0), color));
    }
    let card = card.id();
    let mut children = Vec::new();

    // The illustration, 2×, cropped to the window.
    let (ax, ay, aw, ah) = ART;
    let window = commands
        .spawn(Node {
            overflow: Overflow::clip(),
            ..at(ax as f32, ay as f32, aw as f32, ah as f32)
        })
        .id();
    if let Some(image) = art_file(def.name).and_then(|f| art.art.get(f)) {
        let picture = commands
            .spawn((
                ImageNode::new(image.clone()).with_color(tint),
                at(0.0, -20.0, 128.0, 128.0),
            ))
            .id();
        commands.entity(window).add_child(picture);
    }
    children.push(window);

    // Spirit cost in a gem, top left.
    if def.cost > 0 {
        let gem = commands
            .spawn((
                ImageNode::new(art.spirit.clone()).with_color(tint),
                Node {
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..at(-10.0, -10.0, 40.0, 40.0)
                },
            ))
            .id();
        let n = commands
            .spawn((
                Text::new(def.cost.to_string()),
                font.bold(18.0),
                TextColor(Color::WHITE.with_alpha(text_alpha)),
                TextShadow::default(),
            ))
            .id();
        commands.entity(gem).add_child(n);
        children.push(gem);
    }
    // Element, top right.
    if let Some(element) = def.element
        && let Some(god) = God::ALL.iter().find(|g| g.element() == element)
    {
        let icon = commands
            .spawn((
                ImageNode::new(stats.gods[god.index()].clone()).with_color(tint),
                at(CARD_W - 26.0, -6.0, 32.0, 32.0),
            ))
            .id();
        children.push(icon);
    }

    // Name on the ribbon.
    let (ry, rh) = RIBBON;
    let name = commands
        .spawn((
            Node {
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..at(6.0, ry as f32, CARD_W - 12.0, rh as f32)
            },
            children![(
                Text::new(def.name),
                font.bold(12.0),
                TextColor(INK.with_alpha(text_alpha)),
                TextShadow::default(),
                TextLayout::no_wrap(),
            )],
        ))
        .id();
    children.push(name);

    // Kind and timing, the text, the stage note, extras.
    let text = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(3.0),
            overflow: Overflow::clip(),
            ..at(
                8.0,
                TEXT_TOP as f32 + 2.0,
                CARD_W - 16.0,
                (STRIP_TOP - TEXT_TOP) as f32 - 4.0,
            )
        })
        .id();
    let line = |commands: &mut Commands, text: String, size: f32, color: Color, bold: bool| {
        commands
            .spawn((
                Text::new(text),
                if bold {
                    font.bold(size)
                } else {
                    font.text(size)
                },
                TextColor(color.with_alpha(text_alpha)),
                Node {
                    width: px(CARD_W - 16.0),
                    flex_shrink: 0.0,
                    ..default()
                },
            ))
            .id()
    };
    let mut lines = vec![
        line(
            commands,
            format!("{} · {}", names::kind(def.kind), names::timing(def.timing)),
            10.0,
            names::element_color(def.element),
            false,
        ),
        line(
            commands,
            def.text.to_string(),
            11.0,
            Color::srgb(0.86, 0.84, 0.78),
            false,
        ),
    ];
    let note = crate::hud::stage_note(g, def);
    if !note.is_empty() {
        lines.push(line(
            commands,
            note,
            10.0,
            Color::srgb(0.75, 0.70, 0.95),
            false,
        ));
    }
    for (extra, color) in look.extra {
        lines.push(line(commands, extra, 10.0, color, true));
    }
    commands.entity(text).add_children(&lines);
    children.push(text);

    // The face it gives when burned, in the bottom strip.
    let face = def.burn_face();
    let strip = commands
        .spawn(Node {
            column_gap: px(4.0),
            align_items: AlignItems::Center,
            ..at(8.0, STRIP_TOP as f32 + 1.0, CARD_W - 16.0, 16.0)
        })
        .id();
    let face_icon = commands
        .spawn((
            ImageNode::new(stats.faces[&face].clone()).with_color(tint),
            Node {
                width: px(14.0),
                height: px(14.0),
                ..default()
            },
        ))
        .id();
    let face_text = commands
        .spawn((
            Text::new(format!("в бою: {}", names::face(face))),
            font.text(10.0),
            TextColor(Color::srgb(0.95, 0.6, 0.5).with_alpha(text_alpha)),
            TextLayout::no_wrap(),
        ))
        .id();
    commands.entity(strip).add_children(&[face_icon, face_text]);
    children.push(strip);

    commands.entity(card).add_children(&children);
    card
}
