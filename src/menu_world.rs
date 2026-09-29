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
//! Days and nights turn once there is a sun or a moon (`clock`): the sky
//! lightens, what does not shine dims at night. Things can be picked up
//! and moved (`drag`).
//!
//! Dev aids: `NECROMY_MENU_WORLD=fresh` starts empty and saves nothing,
//! `=full` fills every kind to its cap (not saved either);
//! `NECROMY_MENU_CLOCK=0.25` starts at noon (0.75 midnight).

use std::collections::HashMap;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};

use crate::audio::Sound;
use crate::menu_stage::{FLOOR, MenuPanel, scale};
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
    /// The time of day, 0..1 round the clock: the first half is day (the
    /// sun crosses the sky), the second night (the moon). It runs once
    /// Ahamar has made either.
    clock: f32,
}

impl MenuWorld {
    pub fn has(&self, kind: Kind) -> bool {
        self.things.iter().any(|t| t.kind == kind)
    }

    /// Days and nights pass once there is a sun or a moon.
    fn cycling(&self) -> bool {
        self.has(Kind::Sun) || self.has(Kind::Moon)
    }

    /// A thing dragged somewhere else.
    fn set_pos(&mut self, id: u64, x: f32, y: f32) {
        if let Some(t) = self.things.iter_mut().find(|t| t.id == id) {
            t.x = x.clamp(0.0, 1.0);
            t.y = y.clamp(0.0, 1.0);
        }
        self.save();
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
            // Starts at dusk: the menu's first look.
            clock: 0.55,
        };
        match mode.as_deref() {
            Some("fresh") => {}
            Some("full") => world.fill(),
            _ => {
                let text = Self::path()
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .unwrap_or_default();
                for line in text.lines() {
                    if let Some(c) = line.strip_prefix("clock ") {
                        world.clock = c.trim().parse().unwrap_or(world.clock);
                        continue;
                    }
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
        // `NECROMY_MENU_CLOCK=0.25`: noon, 0.75 midnight, for screenshots.
        if let Some(c) = std::env::var("NECROMY_MENU_CLOCK")
            .ok()
            .and_then(|c| c.parse().ok())
        {
            world.clock = c;
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
                    // On their ground line, where a god leaves them.
                    kind if kind.grounded() => 0.0,
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
        let mut text: String = self
            .things
            .iter()
            .map(|t| format!("{} {} {:.4} {:.4}\n", t.kind.word(), t.variant, t.x, t.y))
            .collect();
        text.push_str(&format!("clock {:.4}\n", self.clock));
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
                (sync, drag, sky, animate)
                    .chain()
                    .run_if(not(resource_exists::<Match>)),
            )
            .init_resource::<Drag>()
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
fn clear(
    mut commands: Commands,
    roots: Query<Entity, Or<(With<Back>, With<Front>)>>,
    window: Single<Entity, With<PrimaryWindow>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    commands.entity(*window).insert(CursorIcon::default());
}

#[derive(Resource)]
struct WorldArt {
    images: HashMap<&'static str, Handle<Image>>,
    animals: Vec<Handle<Image>>,
    /// An animal carried by the mouse, wriggling: `animal-<name>-struggle.png`,
    /// frame 0 still, the rest a loop.
    struggles: Vec<Option<Handle<Image>>>,
    ground_top: Handle<Image>,
    ground_fill: Handle<Image>,
    firefly: Handle<Image>,
    /// Bhava's tree as it grows, for a fresh `grow-4`.
    growth: [Handle<Image>; 3],
    /// A tree's leaves in the wind: `tree-<file>.png`, a row of frames the
    /// size of its still picture, frame 0 the still.
    rustle: HashMap<&'static str, Handle<Image>>,
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
        struggles: ANIMALS
            .iter()
            .map(|(name, _, _)| {
                let path = format!("props/menu/animal-{name}-struggle.png");
                std::path::Path::new("assets")
                    .join(&path)
                    .exists()
                    .then(|| assets.load(path))
            })
            .collect(),
        ground_top: assets.load("props/menu/ground-top.png"),
        ground_fill: assets.load("props/menu/ground-fill.png"),
        firefly: images.add(firefly()),
        growth: [1, 2, 3].map(|n| assets.load(format!("props/menu/grow-{n}.png"))),
        rustle: Kind::Tree
            .files()
            .iter()
            .filter_map(|&file| {
                let path = format!("props/menu/tree-{file}.png");
                std::path::Path::new("assets")
                    .join(&path)
                    .exists()
                    .then(|| (file, assets.load(path)))
            })
            .collect(),
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
    /// Dropped above its ground: where its base is and how fast it falls.
    fall: Option<(f32, f32)>,
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
                    fall: None,
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

/// Seconds for a whole day and night.
const DAY_SECS: f32 = 240.0;
/// Pixels a second squared a dropped thing falls with.
const GRAVITY: f32 = 3000.0;
/// Below this speed (pixels a second) a landing thing stays down.
const SETTLE: f32 = 260.0;

/// A thing's own number in 0..1: its phase in every wander and bob.
fn seed_of(id: u64) -> f32 {
    (id as f32 * 0.618_034).fract()
}

/// The sky's light, 0 at night to 1 at noon, or `None` while nobody has
/// made the sun or the moon: the menu keeps its first, timeless dusk.
fn daylight(world: &MenuWorld) -> Option<f32> {
    world.cycling().then(|| {
        let c = world.clock.rem_euclid(1.0);
        if c < 0.5 {
            (c * std::f32::consts::TAU).sin().max(0.0)
        } else {
            0.0
        }
    })
}

/// How far across the sky the sun (the first half of the clock) or the
/// moon (the second) has come, 0..1, if it is up.
fn sky_way(kind: Kind, clock: f32) -> Option<f32> {
    let c = clock.rem_euclid(1.0);
    match kind {
        Kind::Sun if c < 0.5 => Some(c * 2.0),
        Kind::Moon if c >= 0.5 => Some((c - 0.5) * 2.0),
        _ => None,
    }
}

/// The middle of the sun or the moon `u` of the way across: up from behind
/// the horizon on the left, over the top, down on the right.
fn sky_point(u: f32, w: f32, h: f32, s: f32, size: Vec2) -> Vec2 {
    let horizon = h - FLOOR - 20.0 * s + size.y * 0.6;
    let top = 0.1 * h + size.y / 2.0;
    let lift = (u * std::f32::consts::PI).sin();
    Vec2::new((0.06 + 0.88 * u) * w, horizon - (horizon - top) * lift)
}

/// How much of the day's light there is, 0 at night to 1 by day (1 while
/// no day turns), eased.
fn day_of(world: &MenuWorld) -> f32 {
    daylight(world).map_or(1.0, |l| {
        let d = (l / 0.45).clamp(0.0, 1.0);
        d * d * (3.0 - 2.0 * d)
    })
}

/// The colour the night lays over what does not shine by itself.
fn tint_of(world: &MenuWorld) -> Color {
    Color::srgb(0.55, 0.58, 0.78).mix(&Color::WHITE, day_of(world))
}

/// The night's colour over the champions walking by: half the world's, so
/// they still read against it.
pub fn champion_tint(world: &MenuWorld) -> Color {
    tint_of(world).mix(&Color::WHITE, 0.5)
}

/// Things that shine by their own light and keep it at night.
fn glows(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Sun | Kind::Moon | Kind::Spirit | Kind::Firefly | Kind::Glowcap
    )
}

/// Can be picked up with the mouse.
fn draggable(kind: Kind) -> bool {
    !matches!(kind, Kind::Ground | Kind::Firefly)
}

/// The thing under the mouse being carried, if any.
#[derive(Resource, Default)]
struct Drag {
    id: Option<u64>,
    /// The cursor's offset from the thing's top left corner.
    grab: Vec2,
    /// Where the top left corner goes this frame.
    at: Vec2,
}

/// Whether the picture has an opaque pixel under a point of the node.
fn opaque_at(image: &Image, node: &ImageNode, local: Vec2, k: f32, size: Vec2) -> bool {
    let Some(data) = image.data.as_ref() else {
        return false;
    };
    let mut t = (local / k).floor();
    if t.x < 0.0 || t.y < 0.0 || t.x >= size.x || t.y >= size.y {
        return false;
    }
    if node.flip_x {
        t.x = size.x - 1.0 - t.x;
    }
    let origin = node.rect.map_or(Vec2::ZERO, |r| r.min);
    let (px, py) = ((origin.x + t.x) as usize, (origin.y + t.y) as usize);
    let width = image.width() as usize;
    data.get((py * width + px) * 4 + 3).is_some_and(|&a| a > 0)
}

/// Picks things up and puts them down. Mountains stay where they are put,
/// what stands on the ground falls back to it, clouds and spirits hang
/// where they are let go; the sun and the moon only slide along their way
/// through the sky, and the day turns with them.
#[allow(clippy::too_many_arguments)]
fn drag(
    mut commands: Commands,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<(Entity, &Window), With<PrimaryWindow>>,
    buttons: Query<&Interaction>,
    panel: Query<(&ComputedNode, &UiGlobalTransform), With<MenuPanel>>,
    images: Res<Assets<Image>>,
    mut world: ResMut<MenuWorld>,
    mut carried: ResMut<Drag>,
    mut shown: Query<(Entity, &mut Shown, &ImageNode, &Node)>,
    mut cursor_state: Local<u8>,
    mut sounds: MessageWriter<Sound>,
) {
    let (window_entity, window) = *window;
    let (w, h) = (window.width(), window.height());
    let s = scale(h);
    let now = time.elapsed_secs();
    let cursor = window.cursor_position();
    let px_of = |v: Val| if let Val::Px(p) = v { p } else { 0.0 };

    // What is under the mouse: the frontmost opaque pixel.
    let over_menu = buttons.iter().any(|i| *i != Interaction::None)
        || cursor.is_some_and(|c| {
            panel.iter().any(|(node, at)| {
                let centre = at.affine().translation * node.inverse_scale_factor();
                let size = node.size() * node.inverse_scale_factor();
                Rect::from_center_size(centre, size).contains(c)
            })
        });
    let under = cursor.filter(|_| !over_menu && carried.id.is_none()).and_then(|c| {
        shown
            .iter()
            .filter(|(_, sh, _, _)| sh.gone.is_none() && draggable(sh.thing.kind))
            .filter(|(_, sh, image, node)| {
                let top_left = Vec2::new(px_of(node.left), px_of(node.top));
                images
                    .get(&image.image)
                    .is_some_and(|picture| opaque_at(picture, image, c - top_left, s, sh.size))
            })
            .max_by_key(|(_, sh, _, _)| (sh.thing.kind.z(), sh.id))
            .map(|(e, sh, _, node)| (e, sh.id, Vec2::new(px_of(node.left), px_of(node.top))))
    });

    if mouse.just_pressed(MouseButton::Left)
        && let (Some((entity, id, top_left)), Some(c)) = (under, cursor)
    {
        carried.id = Some(id);
        carried.grab = c - top_left;
        carried.at = top_left;
        sounds.write(Sound::new("menu-pick"));
        // An animal minds being picked up.
        if shown.get(entity).is_ok_and(|(_, sh, _, _)| sh.thing.kind == Kind::Animal) {
            sounds.write(Sound::new("menu-squeak"));
        }
        // Over the menu while carried, so it is never lost behind it.
        commands.entity(entity).insert(GlobalZIndex(54));
    }

    if let (Some(id), Some(c)) = (carried.id, cursor) {
        carried.at = c - carried.grab;
        let kind = shown
            .iter()
            .find(|(_, sh, _, _)| sh.id == id)
            .map(|(_, sh, _, _)| sh.thing.kind);
        // The sun and the moon wind the clock as they go.
        if let Some(kind @ (Kind::Sun | Kind::Moon)) = kind {
            let u = ((c.x / w - 0.06) / 0.88).clamp(0.0, 1.0);
            world.clock = if kind == Kind::Sun { u / 2.0 } else { 0.5 + u / 2.0 };
        }
    }

    if mouse.just_released(MouseButton::Left)
        && let Some(id) = carried.id.take()
    {
        let at = carried.at;
        if let Some((entity, mut sh, _, _)) = shown.iter_mut().find(|(_, sh, _, _)| sh.id == id)
        {
            commands.entity(entity).remove::<GlobalZIndex>();
            let size = sh.size * s;
            let centre = at + size / 2.0;
            let seed = seed_of(sh.id);
            let mut thing = sh.thing;
            match thing.kind {
                Kind::Sun | Kind::Moon => {}
                Kind::Mountain => {
                    // Its base stays where it was let go.
                    thing.x = centre.x / w;
                    thing.y = (at.y + size.y - sh.empty * s) / h;
                    sounds.write(Sound::new("menu-thump").at(0.7));
                }
                Kind::Cloud => {
                    let span = w + size.x;
                    let speed = (4.0 + 3.0 * seed) * s;
                    thing.x = ((centre.x + size.x / 2.0 - now * speed) / span).rem_euclid(1.0);
                    thing.y = (at.y + size.y / 2.0) / h;
                }
                Kind::Spirit => {
                    let d = drift(now * 1.5, seed);
                    let way = if sh.id % 2 == 0 { 1.0 } else { -1.0 };
                    let bob = (now * 1.7 + seed * 9.0).sin() * 4.0 * s;
                    thing.x = (centre.x / w - way * now * 0.012 - d.x * 0.05).rem_euclid(1.0);
                    thing.y = (((at.y + size.y / 2.0 - bob) / h - 0.15 - 0.15 * d.y) / 0.6)
                        .clamp(0.0, 1.0);
                }
                _ => {
                    // Down to the ground from where it was let go.
                    thing.x = (centre.x / w).clamp(0.02, 0.98);
                    sh.fall = Some((at.y + size.y, 0.0));
                    if let Some(wander) = sh.walk.as_mut() {
                        wander.x = thing.x;
                        wander.to = thing.x;
                        wander.t = 0.0;
                        wander.rest_until = now + 1.5;
                    }
                }
            }
            sh.thing = thing;
            world.set_pos(id, thing.x, thing.y);
        }
    }

    // The hand: open over what can be taken, closed while carrying.
    let state = if carried.id.is_some() {
        2
    } else if under.is_some() {
        1
    } else {
        0
    };
    if *cursor_state != state {
        *cursor_state = state;
        let icon = match state {
            2 => SystemCursorIcon::Grabbing,
            1 => SystemCursorIcon::Grab,
            _ => SystemCursorIcon::Default,
        };
        commands.entity(window_entity).insert(CursorIcon::from(icon));
    }
}

/// The day turns (once there is a sun or a moon) and the sky follows it:
/// night blue-black, a warm dusk, a slate day.
fn sky(
    time: Res<Time>,
    mut world: ResMut<MenuWorld>,
    carried: Res<Drag>,
    mut back: Single<&mut BackgroundColor, With<Back>>,
    mut since_save: Local<f32>,
) {
    let dt = time.delta_secs();
    let winding = carried.id.is_some_and(|id| {
        world
            .things
            .iter()
            .any(|t| t.id == id && matches!(t.kind, Kind::Sun | Kind::Moon))
    });
    if world.cycling() && !winding {
        world.clock = (world.clock + dt / DAY_SECS).rem_euclid(1.0);
        *since_save += dt;
        if *since_save > 20.0 {
            *since_save = 0.0;
            world.save();
        }
    }
    let colour = match daylight(&world) {
        None => GROUND,
        Some(light) => {
            let day = Color::srgb(0.22, 0.30, 0.44);
            let dusk = Color::srgb(0.34, 0.18, 0.24);
            let d = (light / 0.45).clamp(0.0, 1.0);
            let base = GROUND.mix(&day, d * d * (3.0 - 2.0 * d));
            // Warm at sunrise and sunset, while the sun is low.
            let c = world.clock.rem_euclid(1.0);
            let low = if c < 0.5 {
                (1.0 - (light - 0.1).abs() / 0.18).clamp(0.0, 1.0)
            } else {
                0.0
            };
            base.mix(&dusk, low * 0.55)
        }
    };
    if back.0 != colour {
        back.0 = colour;
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn animate(
    time: Res<Time>,
    art: Res<WorldArt>,
    images: Res<Assets<Image>>,
    world: Res<MenuWorld>,
    carried: Res<Drag>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut shown: Query<(&mut Shown, &mut ImageNode, &mut Node, Option<&Children>)>,
    mut fills: Query<&mut ImageNode, Without<Shown>>,
    mut sounds: MessageWriter<Sound>,
) {
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    let (w, h) = (window.width(), window.height());
    let s = scale(h);
    let floor = h - FLOOR;
    // Night dims and cools what does not shine by itself.
    let light = daylight(&world);
    let day = day_of(&world);
    let tint = tint_of(&world);
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
        let seed = seed_of(shown.id);
        let size = shown.size * k;
        let colour = if glows(kind) { Color::WHITE } else { tint };
        let carried_here = carried.id == Some(shown.id);
        let (mut x, mut y);
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
                let want = colour.with_alpha(alpha);
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
                        if fill.color != want {
                            fill.color = want;
                        }
                    }
                }
                if image.color != want {
                    image.color = want;
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
                // Along its way over the sky, by the clock; below the
                // horizon while it is the other one's turn.
                match sky_way(kind, world.clock) {
                    Some(u) => {
                        let p = sky_point(u, w, h, s, size);
                        x = p.x;
                        y = p.y + size.y / 2.0;
                        alpha *= ((u * std::f32::consts::PI).sin() * 5.0).min(1.0);
                    }
                    None => {
                        x = -size.x;
                        y = h + size.y;
                        alpha = 0.0;
                    }
                }
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
                // Faint by day.
                alpha *= 1.0 - 0.7 * day * f32::from(light.is_some());
            }
            Kind::Animal => {
                let (_, gait, speed) = ANIMALS[thing.variant];
                // Frame 0 stands; the rest are the hop or the walk.
                let moving = shown.frames.saturating_sub(1).max(1);
                let falling = shown.fall.is_some() || carried_here;
                let wander = shown.walk.as_mut().expect("an animal wanders");
                let mut frame = 0usize;
                if now >= wander.rest_until && !falling {
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
                // Held up by the mouse, it wriggles and kicks.
                let struggle = art.struggles[thing.variant]
                    .as_ref()
                    .filter(|_| carried_here)
                    .and_then(|strip| Some((strip, images.get(strip)?)));
                let sheet = match struggle {
                    Some((strip, picture)) => {
                        let loops = ((picture.width() / ANIMAL_CELL) as usize)
                            .saturating_sub(1)
                            .max(1);
                        frame = 1 + (now * 12.0) as usize % loops;
                        strip.clone()
                    }
                    None => art.animals[thing.variant].clone(),
                };
                if image.image != sheet {
                    image.image = sheet;
                }
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
            // The node's bottom: the art's base on its line, a mountain on
            // the horizon or where it was put, rising into place when new.
            let base = if kind == Kind::Mountain && thing.y > 0.0 {
                thing.y * h
            } else {
                floor + kind.sink() * s
            };
            let rest = base + shown.empty * k;
            let rise = if kind == Kind::Mountain {
                let u = (age / 2.5).clamp(0.0, 1.0);
                (1.0 - u * (2.0 - u)) * size.y
            } else {
                0.0
            };
            y = rest + rise;
            // A dropped thing falls back down to its ground and bounces:
            // an animal springs up again, a tree or a stone barely.
            if let Some((bottom, speed)) = shown.fall {
                let speed = speed + GRAVITY * dt;
                let bottom = bottom + speed * dt;
                if bottom >= rest {
                    let give = if kind == Kind::Animal { 0.38 } else { 0.18 };
                    shown.fall = (speed > SETTLE).then_some((rest, -speed * give));
                    // Every touch of the ground is heard, softer as it
                    // settles; heavy things thump.
                    if speed > SETTLE * 0.5 {
                        let name = match kind {
                            Kind::Tree | Kind::Rock => "menu-thump",
                            _ => "menu-thud",
                        };
                        sounds.write(Sound::new(name).at((speed / 1600.0).clamp(0.2, 1.0)));
                    }
                } else {
                    shown.fall = Some((bottom, speed));
                    y = bottom;
                }
            }
        }
        // Trees: a fresh `grow-4` grows from a sprout, then every tree
        // rustles in the wind.
        if kind == Kind::Tree {
            let file = kind.files()[thing.variant];
            let (want, rect) = if thing.variant == 0 && age < 1.05 {
                (art.growth[((age / 0.35) as usize).min(2)].clone(), None)
            } else if let Some(strip) = art.rustle.get(file)
                && let Some(picture) = images.get(strip)
            {
                let cell = shown.size.x;
                let frames = ((picture.width() as f32 / cell) as usize).max(1);
                let frame = (now * 5.0 + seed * 17.0) as usize % frames;
                let r = Rect::new(frame as f32 * cell, 0.0, (frame + 1) as f32 * cell, shown.size.y);
                (strip.clone(), Some(r))
            } else {
                (art.images[file].clone(), None)
            };
            if image.image != want {
                image.image = want;
            }
            if image.rect != rect {
                image.rect = rect;
            }
        }
        if carried_here {
            x = carried.at.x + size.x / 2.0;
            y = carried.at.y + size.y;
            alpha = alpha.max(0.9);
            // The sun and the moon keep to their way: drawn as the clock says.
            if let Some(u) = sky_way(kind, world.clock) {
                let p = sky_point(u, w, h, s, size);
                x = p.x;
                y = p.y + size.y / 2.0;
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
        let want = colour.with_alpha(alpha.clamp(0.0, 1.0));
        if image.color != want {
            image.color = want;
        }
    }
}
