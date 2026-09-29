//! The menu's world: what the gods' visits leave behind (`menu_stage.rs`).
//! Zaga lays the ground and raises mountains and rocks, Bhava grows trees
//! and bushes, Ahamar's bolts leave clouds, the sun and the moon, Maya
//! brings spirits, fireflies and glowing caps, Trishna's feasts bring
//! animals that wander the ground. Every kind has a cap: past it the oldest
//! fades away, so a menu left idle for hours stays light.
//!
//! The world is kept between runs in `$XDG_STATE_HOME/necromy-table/menu-world`
//! (a line per thing: kind, variant, x and y as fractions of the window).
//! Positions are fractions, so the world fits any window.
//!
//! Dev aid: `NECROMY_MENU_WORLD=fresh` starts empty and saves nothing,
//! `=full` fills every kind to its cap (not saved either).

use std::collections::HashMap;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::menu_stage::{FLOOR, scale};
use crate::play::Match;

/// What a god left.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Ground,
    Mountain,
    Rock,
    Tree,
    Bush,
    Glowcap,
    Cloud,
    Sun,
    Moon,
    Spirit,
    Firefly,
    Animal,
}

impl Kind {
    const ALL: [Kind; 12] = [
        Kind::Ground,
        Kind::Mountain,
        Kind::Rock,
        Kind::Tree,
        Kind::Bush,
        Kind::Glowcap,
        Kind::Cloud,
        Kind::Sun,
        Kind::Moon,
        Kind::Spirit,
        Kind::Firefly,
        Kind::Animal,
    ];

    /// How many may stand at once; the oldest goes past it.
    fn cap(self) -> usize {
        match self {
            Kind::Ground | Kind::Sun | Kind::Moon => 1,
            Kind::Mountain => 5,
            Kind::Rock => 3,
            Kind::Tree => 6,
            Kind::Bush => 8,
            Kind::Glowcap => 4,
            Kind::Cloud => 4,
            Kind::Spirit => 5,
            Kind::Firefly => 16,
            Kind::Animal => 4,
        }
    }

    /// Pictures to choose from (`variant` indexes them).
    fn files(self) -> &'static [&'static str] {
        match self {
            Kind::Ground | Kind::Firefly => &[],
            Kind::Mountain => &[
                "mountain-1",
                "mountain-2",
                "mountain-3",
                "mountain-4",
                "mountain-5",
                "mountain-6",
            ],
            Kind::Rock => &["rocks"],
            Kind::Tree => &["grow-4", "grow-3", "fir"],
            Kind::Bush => &["bush-1", "bush-2", "bush-3", "bush-4"],
            Kind::Glowcap => &["glowcap"],
            Kind::Cloud => &["cloud-1", "cloud-2", "cloud-3"],
            Kind::Sun => &["sun"],
            Kind::Moon => &["moon"],
            Kind::Spirit => &["spirit-1", "spirit-2", "spirit-3", "spirit-4"],
            Kind::Animal => &[],
        }
    }

    pub fn variants(self) -> usize {
        match self {
            Kind::Animal => ANIMALS.len(),
            Kind::Ground | Kind::Firefly => 1,
            _ => self.files().len(),
        }
    }

    fn word(self) -> &'static str {
        match self {
            Kind::Ground => "ground",
            Kind::Mountain => "mountain",
            Kind::Rock => "rock",
            Kind::Tree => "tree",
            Kind::Bush => "bush",
            Kind::Glowcap => "glowcap",
            Kind::Cloud => "cloud",
            Kind::Sun => "sun",
            Kind::Moon => "moon",
            Kind::Spirit => "spirit",
            Kind::Firefly => "firefly",
            Kind::Animal => "animal",
        }
    }

    /// Stands on the ground line (else it flies or hangs in the sky).
    fn grounded(self) -> bool {
        matches!(
            self,
            Kind::Mountain | Kind::Rock | Kind::Tree | Kind::Bush | Kind::Glowcap | Kind::Animal
        )
    }

    /// Stacking in the world, back to front.
    fn z(self) -> i32 {
        match self {
            Kind::Sun | Kind::Moon => 1,
            Kind::Cloud => 2,
            Kind::Mountain => 3,
            Kind::Ground => 5,
            Kind::Tree => 6,
            Kind::Rock | Kind::Bush | Kind::Glowcap => 7,
            Kind::Animal => 8,
            Kind::Spirit => 4,
            Kind::Firefly => 1,
        }
    }

    /// Texels its base sinks below the ground line.
    fn sink(self) -> f32 {
        match self {
            // Far off: on a horizon above the ground line.
            Kind::Mountain => -14.0,
            Kind::Tree => 2.0,
            Kind::Rock | Kind::Bush | Kind::Glowcap => 3.0,
            _ => 1.0,
        }
    }
}

/// How an animal gets about.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gait {
    /// A hop is the whole row played once while it moves a stride.
    Hop,
    /// The row loops while it moves.
    Walk,
}

/// Trishna's animals: `props/menu/animal-<name>.png`, a row of 32 px frames
/// facing right, frame 0 standing (`scripts/pixellab-strip.sh`); speed in
/// texels a second.
const ANIMALS: [(&str, Gait, f32); 12] = [
    ("hare", Gait::Hop, 14.0),
    ("hare-grey", Gait::Hop, 14.0),
    ("fox", Gait::Walk, 12.0),
    ("robin", Gait::Hop, 6.0),
    ("fawn", Gait::Walk, 10.0),
    ("hedgehog", Gait::Walk, 4.0),
    ("quail", Gait::Walk, 5.0),
    ("crow", Gait::Hop, 7.0),
    ("chipmunk", Gait::Walk, 11.0),
    ("bluebird", Gait::Hop, 6.0),
    ("badger", Gait::Walk, 6.0),
    ("raccoon", Gait::Walk, 7.0),
];
const ANIMAL_CELL: u32 = 32;

#[derive(Clone, Copy, Debug)]
pub struct Thing {
    pub kind: Kind,
    pub variant: usize,
    /// Across the window, 0..1 (the middle of the thing).
    pub x: f32,
    /// Down the window, 0..1, for what flies or hangs in the sky.
    pub y: f32,
    id: u64,
    /// Added this run: it comes in with a show (grows, rises, fades in).
    fresh: bool,
}

#[derive(Resource)]
pub struct MenuWorld {
    things: Vec<Thing>,
    next: u64,
    /// Saved to disk as it changes.
    keep: bool,
}

impl MenuWorld {
    pub fn has(&self, kind: Kind) -> bool {
        self.things.iter().any(|t| t.kind == kind)
    }

    /// Something a god left. Past the kind's cap the oldest goes.
    pub fn add(&mut self, kind: Kind, variant: usize, x: f32, y: f32) {
        self.push(kind, variant, x, y, true);
        self.save();
    }

    fn push(&mut self, kind: Kind, variant: usize, x: f32, y: f32, fresh: bool) {
        self.things.push(Thing {
            kind,
            variant: variant % kind.variants().max(1),
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
            id: self.next,
            fresh,
        });
        self.next += 1;
        let over = self.things.iter().filter(|t| t.kind == kind).count();
        if over > kind.cap()
            && let Some(i) = self.things.iter().position(|t| t.kind == kind)
        {
            self.things.remove(i);
        }
    }

    fn path() -> Option<std::path::PathBuf> {
        Some(crate::state_dir()?.join("menu-world"))
    }

    fn load() -> MenuWorld {
        let mode = std::env::var("NECROMY_MENU_WORLD").ok();
        let mut world = MenuWorld {
            things: Vec::new(),
            next: 0,
            keep: mode.is_none(),
        };
        match mode.as_deref() {
            Some("fresh") => {}
            Some("full") => world.fill(),
            _ => {
                let text = Self::path()
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .unwrap_or_default();
                for line in text.lines() {
                    let mut words = line.split_whitespace();
                    let (Some(kind), Some(v), Some(x), Some(y)) =
                        (words.next(), words.next(), words.next(), words.next())
                    else {
                        continue;
                    };
                    let Some(kind) = Kind::ALL.into_iter().find(|k| k.word() == kind) else {
                        continue;
                    };
                    if let (Ok(v), Ok(x), Ok(y)) = (v.parse(), x.parse(), y.parse()) {
                        world.push(kind, v, x, y, false);
                    }
                }
            }
        }
        world
    }

    /// Every kind to its cap, spread about: to look at a grown world.
    fn fill(&mut self) {
        let mut k = 0usize;
        for kind in Kind::ALL {
            for n in 0..kind.cap() {
                k += 1;
                let x = (k as f32 * 0.618_034).fract();
                let y = (k as f32 * 0.414_213).fract();
                let y = match kind {
                    Kind::Cloud => 0.04 + 0.25 * y,
                    _ => y,
                };
                self.push(kind, n, x, y, false);
            }
        }
    }

    fn save(&self) {
        if !self.keep {
            return;
        }
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self
            .things
            .iter()
            .map(|t| format!("{} {} {:.4} {:.4}\n", t.kind.word(), t.variant, t.x, t.y))
            .collect();
        let _ = std::fs::write(path, text);
    }
}

pub struct MenuWorldPlugin;

impl Plugin for MenuWorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MenuWorld::load())
            .add_systems(Startup, (load_art, spawn_roots))
            .add_systems(
                Update,
                (sync, animate).chain().run_if(not(resource_exists::<Match>)),
            )
            .add_systems(Update, clear.run_if(resource_added::<Match>));
    }
}

/// Behind the menu (it sits at `GlobalZIndex(50)`): paints the menu's
/// ground colour and holds what stands in the world.
#[derive(Component)]
struct Back;

/// Over the menu: fireflies fly across everything.
#[derive(Component)]
struct Front;

/// The menu's background colour, formerly the menu root's.
pub const GROUND: Color = Color::srgb(0.09, 0.08, 0.11);

fn spawn_roots(mut commands: Commands) {
    let full = || Node {
        position_type: PositionType::Absolute,
        width: percent(100.0),
        height: percent(100.0),
        overflow: Overflow::clip(),
        ..default()
    };
    commands.spawn((Back, full(), BackgroundColor(GROUND), GlobalZIndex(49)));
    commands.spawn((Front, full(), Pickable::IGNORE, GlobalZIndex(53)));
}

#[allow(clippy::type_complexity)]
fn clear(mut commands: Commands, roots: Query<Entity, Or<(With<Back>, With<Front>)>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

#[derive(Resource)]
struct WorldArt {
    images: HashMap<&'static str, Handle<Image>>,
    animals: Vec<Handle<Image>>,
    ground_top: Handle<Image>,
    ground_fill: Handle<Image>,
    firefly: Handle<Image>,
    /// Bhava's tree as it grows, for a fresh `grow-4`.
    growth: [Handle<Image>; 3],
}

fn load_art(mut commands: Commands, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    let mut named = HashMap::new();
    for kind in Kind::ALL {
        for &file in kind.files() {
            named.insert(file, assets.load(format!("props/menu/{file}.png")));
        }
    }
    commands.insert_resource(WorldArt {
        images: named,
        animals: ANIMALS
            .iter()
            .map(|(name, _, _)| assets.load(format!("props/menu/animal-{name}.png")))
            .collect(),
        ground_top: assets.load("props/menu/ground-top.png"),
        ground_fill: assets.load("props/menu/ground-fill.png"),
        firefly: images.add(firefly()),
        growth: [1, 2, 3].map(|n| assets.load(format!("props/menu/grow-{n}.png"))),
    });
}

/// A 5×5 firefly: a bright core in a soft green-gold glow.
fn firefly() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut image = Image::new_fill(
        Extent3d {
            width: 5,
            height: 5,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    for y in 0..5i32 {
        for x in 0..5i32 {
            let d = (x - 2).abs() + (y - 2).abs();
            let c: [u8; 4] = match d {
                0 => [255, 255, 220, 255],
                1 => [220, 255, 140, 230],
                2 => [170, 230, 90, 110],
                _ => continue,
            };
            let i = ((y * 5 + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&c);
        }
    }
    image
}

/// A thing on screen.
#[derive(Component)]
struct Shown {
    id: u64,
    thing: Thing,
    /// When it came (a fresh thing plays its entrance from here).
    since: f32,
    /// When it began to go, if it has.
    gone: Option<f32>,
    /// Size of its picture in texels, and empty rows under the art.
    size: Vec2,
    empty: f32,
    /// Frames in an animal's row.
    frames: usize,
    /// An animal's wandering.
    walk: Option<Wander>,
}

struct Wander {
    x: f32,
    to: f32,
    rest_until: f32,
    /// Seconds into the present hop or walk.
    t: f32,
    east: bool,
}

/// The number of empty rows under the lowest opaque pixel of the first
/// `cell`-wide frame (or the whole picture).
fn empty_rows(image: &Image, cell: u32) -> Option<f32> {
    let data = image.data.as_ref()?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let cell = (cell as usize).min(w);
    let lowest = (0..h)
        .rev()
        .find(|&y| (0..cell).any(|x| data.get((y * w + x) * 4 + 3).is_some_and(|&a| a > 0)))?;
    Some((h - 1 - lowest) as f32)
}

#[allow(clippy::too_many_arguments)]
fn sync(
    mut commands: Commands,
    time: Res<Time>,
    world: Res<MenuWorld>,
    art: Res<WorldArt>,
    images: Res<Assets<Image>>,
    back: Single<Entity, With<Back>>,
    front: Single<Entity, With<Front>>,
    mut shown: Query<(Entity, &mut Shown)>,
    mut seen: Local<HashMap<u64, Entity>>,
) {
    let now = time.elapsed_secs();
    // What left the world fades out.
    for (entity, mut s) in &mut shown {
        if s.gone.is_none() && !world.things.iter().any(|t| t.id == s.id) {
            s.gone = Some(now);
        }
        if s.gone.is_some_and(|g| now - g > 1.2) {
            commands.entity(entity).despawn();
            seen.remove(&s.id);
        }
    }
    for thing in &world.things {
        if seen.contains_key(&thing.id) {
            continue;
        }
        let (image, cell) = match thing.kind {
            Kind::Ground => (art.ground_top.clone(), 32),
            Kind::Firefly => (art.firefly.clone(), 5),
            Kind::Animal => (art.animals[thing.variant].clone(), ANIMAL_CELL),
            kind => {
                let file = kind.files()[thing.variant];
                (art.images[file].clone(), 0)
            }
        };
        // Wait for the picture: its size and base are read off it.
        let Some(picture) = images.get(&image) else {
            continue;
        };
        let size = match thing.kind {
            Kind::Animal => Vec2::splat(ANIMAL_CELL as f32),
            _ => picture.size().as_vec2(),
        };
        let empty = if thing.kind.grounded() {
            empty_rows(picture, if cell == 0 { picture.width() } else { cell }).unwrap_or(0.0)
        } else {
            0.0
        };
        let since = if thing.fresh { now } else { f32::NEG_INFINITY };
        let mut node = ImageNode::new(image).with_color(Color::WHITE.with_alpha(0.0));
        if thing.kind == Kind::Animal {
            node.texture_atlas = None;
            node.rect = Some(Rect::new(0.0, 0.0, size.x, size.y));
        }
        let entity = commands
            .spawn((
                Shown {
                    id: thing.id,
                    thing: *thing,
                    since,
                    gone: None,
                    size,
                    empty,
                    frames: (picture.width() / ANIMAL_CELL).max(1) as usize,
                    walk: (thing.kind == Kind::Animal).then_some(Wander {
                        x: thing.x,
                        to: thing.x,
                        rest_until: now + 1.0,
                        t: 0.0,
                        east: thing.id % 2 == 0,
                    }),
                },
                node,
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ZIndex(thing.kind.z()),
                Pickable::IGNORE,
            ))
            .id();
        if thing.kind == Kind::Ground {
            // The earth under the grass, repeated down to the bottom.
            let fill = commands
                .spawn((
                    ImageNode::new(art.ground_fill.clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        width: percent(100.0),
                        top: percent(100.0),
                        height: px(4000.0),
                        ..default()
                    },
                ))
                .id();
            commands.entity(entity).add_child(fill);
        }
        // Fireflies cross the menu itself; spirits keep behind it, or they
        // would cloud its words.
        let root = match thing.kind {
            Kind::Firefly => *front,
            _ => *back,
        };
        commands.entity(root).add_child(entity);
        seen.insert(thing.id, entity);
    }
}

/// A smooth wander in -1..1 from a seed: sums of slow sines.
fn drift(t: f32, seed: f32) -> Vec2 {
    Vec2::new(
        (t * 0.13 + seed * 7.1).sin() * 0.6 + (t * 0.31 + seed * 3.3).sin() * 0.4,
        (t * 0.11 + seed * 5.7).sin() * 0.6 + (t * 0.27 + seed * 1.9).sin() * 0.4,
    )
}

#[allow(clippy::type_complexity)]
fn animate(
    time: Res<Time>,
    art: Res<WorldArt>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut shown: Query<(&mut Shown, &mut ImageNode, &mut Node, Option<&Children>)>,
    mut fills: Query<&mut ImageNode, Without<Shown>>,
) {
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    let (w, h) = (window.width(), window.height());
    let s = scale(h);
    let floor = h - FLOOR;
    for (mut shown, mut image, mut node, children) in &mut shown {
        let thing = shown.thing;
        let kind = thing.kind;
        // One pixel size with the champions: pictures are drawn at the size
        // they should stand (small things are small pictures).
        let k = s;
        let age = now - shown.since;
        // Entrance and exit.
        let fade_in = match kind {
            Kind::Mountain | Kind::Ground => 0.3,
            Kind::Sun | Kind::Moon => 2.0,
            _ => 0.8,
        };
        let mut alpha = (age / fade_in).clamp(0.0, 1.0);
        if let Some(gone) = shown.gone {
            alpha = alpha.min(1.0 - (now - gone) / 1.2);
        }
        let seed = (shown.id as f32 * 0.618_034).fract();
        let size = shown.size * k;
        let (x, mut y);
        match kind {
            Kind::Ground => {
                // Rises from below the window.
                let up = (age / 1.2).clamp(0.0, 1.0);
                let top = floor - 19.0 * s + (1.0 - up * (2.0 - up)) * 40.0 * s;
                let (left, t) = (px(0.0), px(top.round()));
                if node.top != t {
                    node.left = left;
                    node.top = t;
                    node.width = percent(100.0);
                    node.height = px(32.0 * s);
                }
                let tiled = NodeImageMode::Tiled {
                    tile_x: true,
                    tile_y: false,
                    stretch_value: s,
                };
                if image.image_mode != tiled {
                    image.image_mode = tiled;
                }
                for child in children.into_iter().flatten() {
                    if let Ok(mut fill) = fills.get_mut(*child) {
                        let tiled = NodeImageMode::Tiled {
                            tile_x: true,
                            tile_y: true,
                            stretch_value: s,
                        };
                        if fill.image_mode != tiled {
                            fill.image_mode = tiled;
                        }
                        fill.color.set_alpha(alpha);
                    }
                }
                if (image.color.alpha() - alpha).abs() > 0.004 {
                    image.color.set_alpha(alpha);
                }
                continue;
            }
            Kind::Cloud => {
                // With the wind, round the window.
                let span = w + size.x;
                let speed = (4.0 + 3.0 * seed) * s;
                x = (thing.x * span + now * speed).rem_euclid(span) - size.x / 2.0;
                y = thing.y * h + size.y / 2.0;
            }
            Kind::Sun | Kind::Moon => {
                x = thing.x * w;
                y = thing.y * h + size.y / 2.0 + (1.0 - (age / 2.0).clamp(0.0, 1.0)) * 30.0;
            }
            Kind::Spirit => {
                // Across the whole window (behind the menu now and then),
                // each its own way, meandering.
                let d = drift(now * 1.5, seed);
                let way = if shown.id % 2 == 0 { 1.0 } else { -1.0 };
                x = (thing.x + way * now * 0.012 + d.x * 0.05).rem_euclid(1.0) * w;
                y = (0.15 + 0.6 * thing.y + d.y * 0.15) * h
                    + (now * 1.7 + seed * 9.0).sin() * 4.0 * s;
                y += size.y / 2.0;
                // Spirits breathe in and out of sight a little.
                alpha *= 0.75 + 0.25 * (now * 1.1 + seed * 5.0).sin();
            }
            Kind::Firefly => {
                let d = drift(now * 1.3, seed);
                x = (thing.x + d.x * 0.12).rem_euclid(1.0) * w;
                y = (0.2 + 0.7 * thing.y + d.y * 0.1) * h + size.y / 2.0;
                alpha *= ((now * (1.3 + seed) + seed * 11.0).sin() * 0.5 + 0.5).powi(2);
            }
            Kind::Animal => {
                let (_, gait, speed) = ANIMALS[thing.variant];
                // Frame 0 stands; the rest are the hop or the walk.
                let moving = shown.frames.saturating_sub(1).max(1);
                let wander = shown.walk.as_mut().expect("an animal wanders");
                let mut frame = 0usize;
                if now >= wander.rest_until {
                    wander.t += dt;
                    let east = wander.to > wander.x;
                    wander.east = east;
                    let step = speed * s * dt / w;
                    let hop_secs = 0.6;
                    match gait {
                        Gait::Walk => {
                            frame = 1 + (wander.t * 9.0) as usize % moving;
                            wander.x += if east { step } else { -step };
                        }
                        Gait::Hop => {
                            let u = (wander.t % hop_secs) / hop_secs;
                            frame = 1 + ((u * moving as f32) as usize).min(moving - 1);
                            wander.x += if east { step * 1.6 } else { -step * 1.6 };
                        }
                    }
                    if (wander.to - wander.x).abs() <= step * 2.0 {
                        wander.x = wander.to;
                        wander.t = 0.0;
                        frame = 0;
                        wander.rest_until = now + 1.5 + 4.0 * (now * 7.3 + seed).sin().abs();
                        // Next time somewhere else, never off the window.
                        let r = (now * 3.7 + seed * 13.0).sin();
                        wander.to = (wander.x + r * 0.25).clamp(0.04, 0.96);
                    }
                }
                x = wander.x * w;
                y = floor;
                let flip = !wander.east;
                if image.flip_x != flip {
                    image.flip_x = flip;
                }
                // The frame, from the row's width (read when shown).
                image.rect = Some(Rect::new(
                    frame as f32 * ANIMAL_CELL as f32,
                    0.0,
                    (frame + 1) as f32 * ANIMAL_CELL as f32,
                    ANIMAL_CELL as f32,
                ));
            }
            _ => {
                x = thing.x * w;
                y = floor;
            }
        }
        if kind.grounded() {
            // The base on the ground line; a mountain rises into place.
            let rise = if kind == Kind::Mountain {
                let u = (age / 2.5).clamp(0.0, 1.0);
                (1.0 - u * (2.0 - u)) * size.y
            } else {
                0.0
            };
            y = floor + kind.sink() * s + shown.empty * k + rise;
        }
        // A fresh tree grows: sprout, sapling, young tree, then in blossom.
        if kind == Kind::Tree && thing.variant == 0 {
            let want = if age < 1.05 {
                art.growth[((age / 0.35) as usize).min(2)].clone()
            } else {
                art.images["grow-4"].clone()
            };
            if image.image != want {
                image.image = want;
            }
        }
        let left = px((x - size.x / 2.0).round());
        let top = px((y - size.y).round());
        if node.left != left || node.top != top || node.width != px(size.x) {
            node.left = left;
            node.top = top;
            node.width = px(size.x);
            node.height = px(size.y);
        }
        if (image.color.alpha() - alpha).abs() > 0.004 {
            image.color.set_alpha(alpha.clamp(0.0, 1.0));
        }
    }
}
