//! The look of the interface: frames for panels, plates, tooltips, fields
//! and buttons, drawn in code as small pixel images in the cards' bronze
//! and shown as nine-slices at 2×, so the rims stay crisp at any size.
//!
//! A node asks for a look with a `Frame` (and an `Accent` for a coloured
//! rim); `dress` turns that into an `ImageNode` and swaps a button's image
//! as the mouse hovers and presses it. Accented images are made once per
//! colour and kept.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Screen pixels per frame pixel.
const PIXEL: f32 = 2.0;

pub struct UiSkinPlugin;

impl Plugin for UiSkinPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Skin>().add_systems(PostUpdate, dress.before(bevy::ui::UiSystems::Prepare));
    }
}

/// How a node is framed.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Frame {
    /// A panel over the board: bronze rim, dark body the board shows
    /// through a little.
    Panel,
    /// A panel that asks for attention (battle, wish, end of match): a
    /// double rim with gold corner brackets, opaque.
    Plate,
    /// A tooltip or a small label: a thin rim.
    Tip,
    /// A sunken field: text input, a tray, a slot.
    Inset,
    /// A button; its image follows `Interaction`.
    Button,
    /// A button that cannot be pressed now.
    ButtonOff,
    /// One filled cell of a stat bar, a small gem in the accent colour.
    Cell,
    /// An empty cell, its rim in the accent colour.
    Slot,
}

/// The bronze of an unaccented rim, for an `Accent` that switches back.
pub const BRONZE_RIM: Color = Color::srgb(0.66, 0.47, 0.24);

/// The rim in this colour instead of bronze.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct Accent(pub Color);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Look {
    Normal,
    Hover,
    Pressed,
}

/// A frame image is known by its frame, its look and its rim colour.
type Key = (Frame, Look, Option<[u8; 3]>);

#[derive(Resource, Default)]
pub struct Skin {
    images: HashMap<Key, Handle<Image>>,
}

type Rgb = [u8; 3];

const INK: Rgb = [20, 15, 24];
const BRONZE: [Rgb; 3] = [[212, 170, 96], [168, 120, 60], [92, 62, 36]];
const GOLD: Rgb = [250, 214, 120];
const BODY: Rgb = [24, 21, 29];

fn rgb(c: Color) -> Rgb {
    let s = c.to_srgba();
    [s.red, s.green, s.blue].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t) as u8)
}

fn grey(c: Rgb) -> Rgb {
    let l = ((c[0] as u32 * 3 + c[1] as u32 * 6 + c[2] as u32) / 10) as u8;
    mix(c, [l, l, l], 0.8)
}

/// Light, middle and shadow tones of a rim in `accent`.
fn rim(accent: Option<Rgb>) -> [Rgb; 3] {
    match accent {
        None => BRONZE,
        Some(c) => [mix(c, [255, 255, 255], 0.35), c, mix(c, INK, 0.55)],
    }
}

/// Where the nine-slice cuts, in frame pixels, and the image's side.
fn geometry(frame: Frame) -> (u32, u32) {
    match frame {
        Frame::Panel => (16, 6),
        Frame::Plate => (24, 10),
        Frame::Tip => (10, 4),
        Frame::Inset => (10, 4),
        Frame::Button | Frame::ButtonOff => (12, 5),
        // Whole images, drawn at 2× by the bars' sizes.
        Frame::Cell | Frame::Slot => (0, 0),
    }
}

/// Draws one frame image. `edge` is the distance to the nearest side,
/// `lit` says the pixel is on the top or left half (light from there).
fn draw(frame: Frame, look: Look, accent: Option<Rgb>) -> Image {
    if matches!(frame, Frame::Cell | Frame::Slot) {
        return cell(frame == Frame::Cell, accent.unwrap_or(BRONZE[1]));
    }
    let (n, _) = geometry(frame);
    let mut image = Image::new_fill(
        Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let [light, mid, dark] = rim(accent);
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    for y in 0..n {
        for x in 0..n {
            let (ex, ey) = (x.min(n - 1 - x), y.min(n - 1 - y));
            let edge = ex.min(ey);
            // Rounded outer corners: the very corner pixel is empty.
            if ex == 0 && ey == 0 {
                continue;
            }
            let lit = if ex == edge {
                x < n / 2
            } else {
                y < n / 2
            };
            let corner = |d: u32| ex <= d && ey <= d;
            let (c, a): (Rgb, u8) = match frame {
                Frame::Panel => match edge {
                    0 => (INK, 255),
                    1 if lit => (light, 255),
                    1 => (dark, 255),
                    2 => (mid, 255),
                    3 => (INK, 255),
                    4 => (mix(BODY, mid, 0.12), 245),
                    _ => (BODY, 242),
                },
                Frame::Plate => match edge {
                    0 => (INK, 255),
                    1 if lit => (light, 255),
                    1 => (dark, 255),
                    2 | 3 => (mid, 255),
                    4 => (INK, 255),
                    // A gold bracket in each corner, a thin line along the sides.
                    6 if corner(9) => (GOLD, 255),
                    7 if corner(9) => (dark, 255),
                    8 if corner(8) => (GOLD, 255),
                    6 => (mix(BODY, mid, 0.45), 255),
                    _ => (BODY, 255),
                },
                Frame::Tip => match edge {
                    0 => (INK, 255),
                    1 => (mid, 255),
                    2 => (INK, 255),
                    _ => (mix(BODY, INK, 0.3), 252),
                },
                Frame::Inset => match edge {
                    // Sunken: shadow on top and left, light below and right.
                    0 if lit => (dark, 255),
                    0 => (light, 255),
                    1 => (INK, 255),
                    2 if lit => ([8, 6, 11], 255),
                    _ => ([12, 10, 16], 255),
                },
                Frame::Button | Frame::ButtonOff => {
                    let off = frame == Frame::ButtonOff;
                    let (light, _, dark) = match look {
                        Look::Hover => (mix(light, GOLD, 0.6), mid, mid),
                        _ => (light, mid, dark),
                    };
                    // Pressed turns the bevel around.
                    let lit = lit != (look == Look::Pressed);
                    let face = match look {
                        Look::Normal => [72, 50, 32],
                        Look::Hover => [98, 68, 38],
                        Look::Pressed => [52, 36, 24],
                    };
                    let (c, a) = match edge {
                        0 => (INK, 255),
                        1 if lit => (light, 255),
                        1 => (dark, 255),
                        2 if lit => (mix(face, light, 0.3), 255),
                        2 => (mix(face, INK, 0.3), 255),
                        // Top half of the face a touch lighter.
                        _ if y < n / 2 && look != Look::Pressed => (mix(face, light, 0.12), 255),
                        _ => (face, 255),
                    };
                    if off { (grey(mix(c, INK, 0.35)), a) } else { (c, a) }
                }
                Frame::Cell | Frame::Slot => unreachable!("cells are drawn by `cell`"),
            };
            let i = ((y * n + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], a]);
        }
    }
    // Gold studs in a panel's corners, on the rim.
    if frame == Frame::Panel || frame == Frame::Plate {
        for (sx, sy) in [(1, 1), (n - 3, 1), (1, n - 3), (n - 3, n - 3)] {
            for (dx, dy, c) in [(0, 0, GOLD), (1, 0, GOLD), (0, 1, GOLD), (1, 1, dark)] {
                let i = (((sy + dy) * n + sx + dx) * 4) as usize;
                data[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    image
}

impl Skin {
    fn image(
        &mut self,
        images: &mut Assets<Image>,
        frame: Frame,
        look: Look,
        accent: Option<Rgb>,
    ) -> Handle<Image> {
        self.images
            .entry((frame, look, accent))
            .or_insert_with(|| images.add(draw(frame, look, accent)))
            .clone()
    }
}

/// A 5×7 bar cell: a gem with a lit top and a dark foot, or an empty
/// slot rimmed in `tone`.
fn cell(filled: bool, tone: Rgb) -> Image {
    let (w, h) = (5u32, 7u32);
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
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    for y in 0..h {
        for x in 0..w {
            let rim = x == 0 || y == 0 || x == w - 1 || y == h - 1;
            let (c, a): (Rgb, u8) = match (filled, rim) {
                (true, true) => (INK, 255),
                (true, false) if (x, y) == (1, 1) => (mix(tone, [255, 255, 255], 0.7), 255),
                (true, false) if y == 1 => (mix(tone, [255, 255, 255], 0.3), 255),
                (true, false) if y == h - 2 => (mix(tone, INK, 0.4), 255),
                (true, false) => (tone, 255),
                (false, true) => (mix(tone, INK, 0.35), 230),
                (false, false) => ([12, 10, 16], 170),
            };
            let i = ((y * w + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], a]);
        }
    }
    image
}

fn node_image(image: Handle<Image>, frame: Frame) -> ImageNode {
    let (_, cut) = geometry(frame);
    if cut == 0 {
        return ImageNode {
            image,
            image_mode: NodeImageMode::Stretch,
            visual_box: VisualBox::BorderBox,
            ..default()
        };
    }
    ImageNode {
        image,
        image_mode: NodeImageMode::Sliced(TextureSlicer {
            border: BorderRect::all(cut as f32),
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
            max_corner_scale: PIXEL,
        }),
        visual_box: VisualBox::BorderBox,
        ..default()
    }
}

/// Gives framed nodes their image, and buttons the one for their state.
#[allow(clippy::type_complexity)]
fn dress(
    mut commands: Commands,
    mut skin: ResMut<Skin>,
    mut images: ResMut<Assets<Image>>,
    nodes: Query<
        (Entity, &Frame, Option<&Accent>, Option<&Interaction>),
        Or<(Changed<Frame>, Changed<Accent>, Changed<Interaction>)>,
    >,
) {
    for (entity, &frame, accent, interaction) in &nodes {
        let look = match (frame, interaction) {
            (Frame::Button, Some(Interaction::Hovered)) => Look::Hover,
            (Frame::Button, Some(Interaction::Pressed)) => Look::Pressed,
            _ => Look::Normal,
        };
        let image = skin.image(&mut images, frame, look, accent.map(|a| rgb(a.0)));
        commands.entity(entity).insert(node_image(image, frame));
    }
}
