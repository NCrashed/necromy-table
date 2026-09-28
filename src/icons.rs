//! Placeholder terrain icons, painted flat on the tiles so the board reads
//! until real tiles come from PixelLab (assets/tiles/). Each icon is built
//! from a few filled shapes on a 16×16 canvas; an ink outline is added
//! around them automatically.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use necromy_rules::Terrain;

const N: usize = 16;

type Rgb = [u8; 3];

const WHITE: Rgb = [240, 240, 232];
const LIGHT_GREY: Rgb = [186, 184, 178];
const GREY: Rgb = [128, 126, 124];
const DARK_GREEN: Rgb = [34, 100, 46];
const GREEN: Rgb = [70, 150, 60];
const LIGHT_GREEN: Rgb = [128, 200, 90];
const BROWN: Rgb = [112, 72, 40];
const DARK_BROWN: Rgb = [66, 42, 26];
const BONE: Rgb = [228, 212, 176];
const ROOF: Rgb = [196, 64, 46];
const GOLD: Rgb = [236, 186, 58];
const DEEP_GOLD: Rgb = [180, 128, 40];
const RUNE: Rgb = [90, 210, 200];
const MURK: Rgb = [64, 104, 96];
const PINK: Rgb = [236, 132, 172];
const INK: [u8; 4] = [22, 18, 26, 255];

struct Canvas([[Option<Rgb>; N]; N]);

impl Canvas {
    fn new() -> Self {
        Canvas([[None; N]; N])
    }

    fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if (0..N as i32).contains(&x) && (0..N as i32).contains(&y) {
            self.0[y as usize][x as usize] = Some(c);
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, c);
            }
        }
    }

    /// Upright triangle from `apex` down to `base_y`, `half` wide at the base.
    /// Pixels left of the axis get `left`, the rest `right`: cheap shading.
    fn tri(&mut self, cx: i32, apex: i32, base_y: i32, half: i32, left: Rgb, right: Rgb) {
        for y in apex..=base_y {
            let t = (y - apex) as f32 / (base_y - apex).max(1) as f32;
            let hw = (t * half as f32).round() as i32;
            for x in cx - hw..=cx + hw {
                self.set(x, y, if x < cx { left } else { right });
            }
        }
    }

    fn disc(&mut self, cx: i32, cy: i32, r: i32, c: Rgb) {
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r + r / 2 {
                    self.set(x, y, c);
                }
            }
        }
    }

    fn clear(&mut self, x: i32, y: i32) {
        if (0..N as i32).contains(&x) && (0..N as i32).contains(&y) {
            self.0[y as usize][x as usize] = None;
        }
    }

    /// Fills a polygon (even-odd rule), sampling pixel centres.
    fn polygon(&mut self, points: &[(f32, f32)], c: Rgb) {
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let mut inside = false;
                let mut j = points.len() - 1;
                for i in 0..points.len() {
                    let ((xi, yi), (xj, yj)) = (points[i], points[j]);
                    if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                        inside = !inside;
                    }
                    j = i;
                }
                if inside {
                    self.set(x, y, c);
                }
            }
        }
    }

    fn into_image(self) -> Image {
        let mut image = Image::new_fill(
            Extent3d {
                width: N as u32,
                height: N as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        let data = image.data.as_mut().expect("new_fill allocates pixel data");
        let filled = |x: i32, y: i32| {
            (0..N as i32).contains(&x)
                && (0..N as i32).contains(&y)
                && self.0[y as usize][x as usize].is_some()
        };
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                let px = match self.0[y as usize][x as usize] {
                    Some([r, g, b]) => [r, g, b, 255],
                    None if [(-1, 0), (1, 0), (0, -1), (0, 1)]
                        .iter()
                        .any(|(dx, dy)| filled(x + dx, y + dy)) =>
                    {
                        INK
                    }
                    None => continue,
                };
                let i = (y as usize * N + x as usize) * 4;
                data[i..i + 4].copy_from_slice(&px);
            }
        }
        image
    }
}

/// The icon for a terrain, or `None` for plain ground.
pub fn terrain_icon(terrain: Terrain) -> Option<Image> {
    let mut c = Canvas::new();
    match terrain {
        Terrain::Plains => return None,
        Terrain::Mountain => {
            c.tri(10, 5, 13, 5, GREY, GREY);
            c.tri(6, 2, 13, 6, LIGHT_GREY, GREY);
            c.tri(6, 2, 4, 2, WHITE, WHITE);
            c.tri(10, 5, 6, 1, WHITE, WHITE);
        }
        Terrain::Forest => {
            for (cx, top) in [(5, 2), (11, 4)] {
                c.tri(cx, top, top + 8, 4, GREEN, DARK_GREEN);
                c.rect(cx, top + 9, cx, top + 10, BROWN);
            }
        }
        Terrain::Grove => {
            for (cx, cy) in [(5, 6), (11, 7)] {
                c.rect(cx, cy + 3, cx, cy + 6, BROWN);
                c.disc(cx, cy, 3, LIGHT_GREEN);
            }
            c.set(4, 5, PINK);
            c.set(12, 6, PINK);
        }
        Terrain::Swamp => {
            c.rect(2, 11, 13, 12, MURK);
            for (x, top) in [(4, 4), (7, 6), (11, 3)] {
                c.rect(x, top + 2, x, 10, DARK_GREEN);
                c.rect(x, top, x, top + 1, BROWN);
            }
        }
        Terrain::Settlement => {
            c.rect(3, 8, 12, 13, BONE);
            c.tri(7, 2, 7, 6, ROOF, ROOF);
            c.rect(7, 10, 8, 13, DARK_BROWN);
            c.rect(4, 9, 5, 10, GOLD);
            c.rect(10, 9, 11, 10, GOLD);
        }
        Terrain::Temple => {
            c.rect(2, 12, 13, 13, BONE);
            for x in [3, 7, 11] {
                c.rect(x, 6, x + 1, 11, BONE);
            }
            c.tri(7, 1, 5, 6, GOLD, DEEP_GOLD);
        }
        Terrain::Ruins => {
            c.rect(2, 13, 13, 13, GREY);
            c.rect(3, 4, 4, 12, LIGHT_GREY);
            c.rect(8, 8, 9, 12, LIGHT_GREY);
            c.rect(12, 10, 13, 12, GREY);
            c.set(6, 12, GREY);
            c.set(10, 11, GREY);
        }
        Terrain::Stones => {
            c.rect(2, 7, 4, 13, GREY);
            c.rect(7, 3, 9, 13, LIGHT_GREY);
            c.rect(12, 6, 14, 13, GREY);
            c.rect(8, 6, 8, 9, RUNE);
        }
        Terrain::Table => {
            // A game board of gold with a crown over it: Ahamar's wager.
            c.rect(2, 7, 13, 13, GOLD);
            for y in 7..=13 {
                for x in 2..=13 {
                    if (x + y) % 2 == 0 && (3..=12).contains(&x) && (8..=12).contains(&y) {
                        c.set(x, y, DEEP_GOLD);
                    }
                }
            }
            c.rect(5, 3, 10, 5, GOLD);
            for x in [5, 7, 8, 10] {
                c.set(x, 2, GOLD);
            }
        }
    }
    Some(c.into_image())
}

const HEART: Rgb = [214, 52, 58];
const HEART_LIGHT: Rgb = [246, 120, 120];
const SPIRIT: Rgb = [120, 110, 236];
const SPIRIT_LIGHT: Rgb = [190, 186, 255];
const THREAT: Rgb = [236, 128, 40];
const DARK: Rgb = [40, 30, 36];

/// Stat and status icons for the HUD (§6, §12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatIcon {
    Health,
    Spirit,
    /// Might: dice in battle.
    Might,
    /// Wits: hand size.
    Wits,
    Cards,
    Moves,
    Style,
    Threat,
    Crown,
    Rooted,
    Day,
    Night,
    /// The table's taste (§6.4): a goblet.
    Taste,
    /// A god's curse from a wish without style (§7.4).
    Curse,
    /// A story line a god gave (§8): a sealed scroll.
    Quest,
}

impl StatIcon {
    pub const ALL: [StatIcon; 15] = [
        StatIcon::Health,
        StatIcon::Spirit,
        StatIcon::Might,
        StatIcon::Wits,
        StatIcon::Cards,
        StatIcon::Moves,
        StatIcon::Style,
        StatIcon::Threat,
        StatIcon::Crown,
        StatIcon::Rooted,
        StatIcon::Day,
        StatIcon::Night,
        StatIcon::Taste,
        StatIcon::Curse,
        StatIcon::Quest,
    ];
}

/// Five-pointed star around a centre.
fn star(cx: f32, cy: f32, outer: f32, inner: f32) -> Vec<(f32, f32)> {
    (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let a = std::f32::consts::PI * (i as f32 / 5.0 - 0.5);
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

pub fn stat_icon(icon: StatIcon) -> Image {
    let mut c = Canvas::new();
    match icon {
        StatIcon::Health => {
            c.disc(5, 6, 3, HEART);
            c.disc(10, 6, 3, HEART);
            c.polygon(&[(1.5, 7.0), (14.5, 7.0), (8.0, 14.5)], HEART);
            c.set(4, 5, HEART_LIGHT);
            c.set(5, 4, HEART_LIGHT);
        }
        StatIcon::Spirit => {
            c.disc(8, 10, 4, SPIRIT);
            c.polygon(&[(4.5, 9.0), (11.5, 9.0), (9.5, 1.0)], SPIRIT);
            c.disc(8, 11, 2, SPIRIT_LIGHT);
        }
        StatIcon::Might => {
            c.rect(7, 1, 8, 10, LIGHT_GREY);
            c.rect(8, 2, 8, 9, WHITE);
            c.rect(4, 10, 11, 11, GOLD);
            c.rect(7, 12, 8, 14, BROWN);
        }
        StatIcon::Wits => {
            c.polygon(
                &[
                    (1.0, 8.0),
                    (5.0, 4.5),
                    (11.0, 4.5),
                    (15.0, 8.0),
                    (11.0, 11.5),
                    (5.0, 11.5),
                ],
                WHITE,
            );
            c.disc(8, 8, 2, RUNE);
            c.set(8, 8, DARK);
        }
        StatIcon::Cards => {
            c.rect(6, 1, 13, 11, LIGHT_GREY);
            c.rect(2, 4, 9, 14, BONE);
            c.clear(5, 3);
            c.rect(5, 8, 6, 10, ROOF);
        }
        StatIcon::Moves => {
            c.rect(5, 2, 9, 11, BROWN);
            c.rect(5, 10, 13, 13, BROWN);
            c.rect(5, 12, 13, 13, DARK_BROWN);
            c.rect(6, 3, 6, 9, [150, 100, 60]);
        }
        StatIcon::Quest => {
            // Rolled ends, the sheet between, lines of writing, a red seal.
            c.rect(3, 3, 12, 12, BONE);
            c.rect(2, 2, 13, 3, BROWN);
            c.rect(2, 12, 13, 13, BROWN);
            for y in [5, 7, 9] {
                c.rect(5, y, 10, y, DARK_BROWN);
            }
            c.disc(11, 11, 2, HEART);
            c.set(11, 11, HEART_LIGHT);
        }
        StatIcon::Style => {
            c.polygon(&star(8.0, 8.5, 7.5, 3.2), GOLD);
            c.disc(8, 8, 1, [255, 230, 140]);
        }
        StatIcon::Threat => {
            c.polygon(&[(8.0, 1.0), (15.0, 14.0), (1.0, 14.0)], THREAT);
            c.rect(7, 5, 8, 9, DARK);
            c.rect(7, 11, 8, 12, DARK);
        }
        StatIcon::Crown => {
            c.rect(2, 9, 13, 12, GOLD);
            c.polygon(&[(2.0, 9.5), (2.0, 3.0), (5.5, 9.5)], GOLD);
            c.polygon(&[(5.0, 9.5), (8.0, 2.0), (11.0, 9.5)], GOLD);
            c.polygon(&[(10.5, 9.5), (14.0, 3.0), (14.0, 9.5)], GOLD);
            c.set(8, 10, ROOF);
            c.set(4, 10, RUNE);
            c.set(11, 10, RUNE);
        }
        StatIcon::Day => {
            for (dx, dy) in [
                (0, -7),
                (0, 7),
                (-7, 0),
                (7, 0),
                (-5, -5),
                (5, -5),
                (-5, 5),
                (5, 5),
            ] {
                c.set(8 + dx, 8 + dy, GOLD);
                c.set(8 + dx * 6 / 7, 8 + dy * 6 / 7, GOLD);
            }
            c.disc(8, 8, 4, GOLD);
            c.disc(7, 7, 1, [255, 230, 140]);
        }
        StatIcon::Night => {
            c.disc(8, 8, 6, [200, 210, 240]);
            for y in 0..16 {
                for x in 0..16 {
                    if (x - 11) * (x - 11) + (y - 6) * (y - 6) <= 26 {
                        c.clear(x, y);
                    }
                }
            }
        }
        StatIcon::Taste => {
            c.polygon(&[(2.0, 2.0), (14.0, 2.0), (11.0, 8.0), (5.0, 8.0)], GOLD);
            c.rect(7, 8, 8, 12, DEEP_GOLD);
            c.rect(4, 13, 11, 14, GOLD);
            c.rect(4, 3, 11, 3, ROOF);
        }
        StatIcon::Curse => {
            c.disc(8, 7, 5, BONE);
            c.rect(6, 11, 10, 14, BONE);
            c.rect(5, 6, 6, 8, DARK);
            c.rect(10, 6, 11, 8, DARK);
            c.set(8, 10, DARK);
            c.set(7, 13, DARK);
            c.set(9, 13, DARK);
        }
        StatIcon::Rooted => {
            for (cx, cy) in [(5, 6), (10, 10)] {
                c.disc(cx, cy, 3, GREY);
                for (x, y) in [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)] {
                    c.clear(cx + x, cy + y);
                }
            }
        }
    }
    c.into_image()
}

/// A ward: a shield in the colour of its element.
pub fn ward_icon(color: [u8; 3]) -> Image {
    let mut c = Canvas::new();
    c.polygon(
        &[
            (2.0, 2.0),
            (14.0, 2.0),
            (14.0, 8.0),
            (8.0, 15.0),
            (2.0, 8.0),
        ],
        color,
    );
    c.rect(7, 4, 8, 11, WHITE);
    c.into_image()
}

/// Poison (§20.1): a drop in the colour of its element with a sickly
/// green glint, so every element's poison still reads as poison.
pub fn poison_icon(color: [u8; 3]) -> Image {
    let mut c = Canvas::new();
    c.polygon(&[(8.0, 1.0), (12.5, 9.0), (3.5, 9.0)], color);
    c.disc(8, 10, 5, color);
    c.disc(8, 11, 2, LIGHT_GREEN);
    c.rect(6, 6, 6, 8, WHITE);
    c.into_image()
}

fn lighten([r, g, b]: Rgb, t: f32) -> Rgb {
    [r, g, b].map(|c| (c as f32 + (255.0 - c as f32) * t) as u8)
}

fn darken([r, g, b]: Rgb, t: f32) -> Rgb {
    [r, g, b].map(|c| (c as f32 * (1.0 - t)) as u8)
}

/// A god's element in the god's colour: sprout, flame, crag, ingot, drop.
pub fn element_icon(element: necromy_rules::Element, accent: Rgb) -> Image {
    use necromy_rules::Element;
    let (light, dark) = (lighten(accent, 0.45), darken(accent, 0.35));
    let mut c = Canvas::new();
    match element {
        Element::Wood => {
            c.rect(7, 8, 8, 14, BROWN);
            c.polygon(&[(8.0, 9.0), (1.5, 6.0), (5.0, 2.5), (8.0, 7.0)], accent);
            c.polygon(&[(8.0, 8.0), (14.5, 4.0), (11.0, 1.5), (8.0, 6.0)], light);
        }
        Element::Fire => {
            c.polygon(
                &[
                    (8.0, 1.0),
                    (13.0, 8.0),
                    (12.0, 13.0),
                    (8.0, 15.0),
                    (4.0, 13.0),
                    (3.0, 8.0),
                    (6.0, 6.0),
                ],
                accent,
            );
            c.polygon(&[(8.0, 6.0), (11.0, 11.0), (8.0, 14.0), (5.0, 11.0)], GOLD);
        }
        Element::Earth => {
            c.polygon(
                &[
                    (1.0, 14.0),
                    (4.0, 7.0),
                    (7.0, 9.0),
                    (10.0, 3.0),
                    (15.0, 14.0),
                ],
                accent,
            );
            c.polygon(&[(10.0, 3.0), (12.5, 8.5), (9.0, 7.0)], light);
            c.rect(1, 13, 14, 14, dark);
        }
        Element::Metal => {
            c.polygon(
                &[(1.0, 12.0), (4.0, 5.0), (12.0, 5.0), (15.0, 12.0)],
                accent,
            );
            c.rect(4, 6, 11, 7, light);
            c.rect(2, 11, 14, 12, dark);
        }
        Element::Water => {
            c.disc(8, 10, 4, accent);
            c.polygon(&[(4.5, 9.0), (11.5, 9.0), (8.0, 1.0)], accent);
            c.rect(6, 9, 6, 11, light);
        }
    }
    c.into_image()
}

/// An offering bowl: a player's favour with a god.
pub fn offering_icon() -> Image {
    let mut c = Canvas::new();
    c.polygon(&[(1.0, 8.0), (15.0, 8.0), (12.0, 13.0), (4.0, 13.0)], BROWN);
    c.rect(5, 14, 10, 14, DARK_BROWN);
    c.disc(6, 6, 2, ROOF);
    c.disc(10, 6, 2, GOLD);
    c.rect(1, 8, 14, 8, [150, 100, 60]);
    c.into_image()
}
