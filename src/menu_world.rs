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
//! and moved (`drag`); while one is carried a black hole opens in the top
//! right corner (`hole`), and what is dropped into it is gone for good.
//!
//! The animals mind each other (`behave`): the fox stalks, hares, hedgehogs
//! and the small ones run from it, two of a kind meet, face each other and
//! hearts rise over them. Birds fly up to the top of the menu panel and to
//! the tops of trees (`Perch`), and flutter down when let go.
//!
//! Dev aids: `NECROMY_MENU_WORLD=fresh` starts empty and saves nothing,
//! `=full` fills every kind to its cap (not saved either);
//! `NECROMY_MENU_CLOCK=0.25` starts at noon (0.75 midnight).
//! `NECROMY_MENU_HOLE=1` keeps the black hole open.

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
            Kind::Animal => 6,
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
            // New ones go last: saved worlds keep their variants.
            Kind::Tree => &[
                "grow-4", "grow-3", "fir", "oak", "maple", "willow", "birch", "cherry",
            ],
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

/// One of Trishna's animals.
struct Beast {
    /// `props/menu/animal-<name>.png`: a row of 32 px frames facing right,
    /// frame 0 standing (`scripts/pixellab-strip.sh`).
    name: &'static str,
    gait: Gait,
    /// Texels a second.
    speed: f32,
    /// Two of the same kin fall in love (the grey hare and the brown one).
    kin: &'static str,
    /// Runs from the fox.
    prey: bool,
    /// Stalks the prey.
    hunts: bool,
    /// Flies to perches (`animal-<name>-fly.png`).
    flies: bool,
}

impl Beast {
    const fn new(name: &'static str, gait: Gait, speed: f32) -> Beast {
        Beast {
            name,
            gait,
            speed,
            kin: name,
            prey: false,
            hunts: false,
            flies: false,
        }
    }

    const fn kin(mut self, kin: &'static str) -> Beast {
        self.kin = kin;
        self
    }

    const fn prey(mut self) -> Beast {
        self.prey = true;
        self
    }

    const fn hunts(mut self) -> Beast {
        self.hunts = true;
        self
    }

    const fn flies(mut self) -> Beast {
        self.flies = true;
        self
    }
}

const ANIMALS: [Beast; 12] = [
    Beast::new("hare", Gait::Hop, 14.0).prey(),
    Beast::new("hare-grey", Gait::Hop, 14.0).kin("hare").prey(),
    Beast::new("fox", Gait::Walk, 12.0).hunts(),
    Beast::new("robin", Gait::Hop, 6.0).prey().flies(),
    Beast::new("fawn", Gait::Walk, 10.0),
    Beast::new("hedgehog", Gait::Walk, 4.0).prey(),
    Beast::new("quail", Gait::Walk, 5.0).prey(),
    Beast::new("crow", Gait::Hop, 7.0).flies(),
    Beast::new("chipmunk", Gait::Walk, 11.0).prey(),
    Beast::new("bluebird", Gait::Hop, 6.0).prey().flies(),
    Beast::new("badger", Gait::Walk, 6.0),
    Beast::new("raccoon", Gait::Walk, 7.0),
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

    /// A thing dropped into the black hole.
    fn remove(&mut self, id: u64) {
        self.things.retain(|t| t.id != id);
        self.save();
    }

    /// The animal Trishna's feast brings, by a roll in 0..1: often one of
    /// the kin of an animal already here, so that pairs meet.
    pub fn animal_for(&self, roll: f32) -> usize {
        let here: Vec<usize> = self
            .things
            .iter()
            .filter(|t| t.kind == Kind::Animal)
            .map(|t| t.variant)
            .collect();
        if roll < 0.4 && !here.is_empty() {
            return here[((roll / 0.4) * here.len() as f32) as usize % here.len()];
        }
        ((roll * ANIMALS.len() as f32) as usize).min(ANIMALS.len() - 1)
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
                (bake_spirits, sync, drag, hole, sky, behave, animate, hearts)
                    .chain()
                    .run_if(not(resource_exists::<Match>)),
            )
            .init_resource::<Drag>()
            .init_resource::<Perches>()
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

fn spawn_roots(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let full = || Node {
        position_type: PositionType::Absolute,
        width: percent(100.0),
        height: percent(100.0),
        overflow: Overflow::clip(),
        ..default()
    };
    commands.spawn((Back, full(), BackgroundColor(GROUND), GlobalZIndex(49)));
    let front = commands
        .spawn((Front, full(), Pickable::IGNORE, GlobalZIndex(53)))
        .id();
    // The black hole waits unseen until something is carried.
    let mut node =
        ImageNode::new(images.add(hole_frames())).with_color(Color::WHITE.with_alpha(0.0));
    node.rect = Some(Rect::new(0.0, 0.0, HOLE as f32, HOLE as f32));
    let hole = commands
        .spawn((
            Hole { open: 0.0 },
            node,
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(front).add_child(hole);
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
    /// A bird in flight: `animal-<name>-fly.png`, frame 0 still, the rest
    /// a loop of wingbeats.
    flights: Vec<Option<Handle<Image>>>,
    /// Over two animals in love.
    heart: Handle<Image>,
    /// Each spirit's motion, a row of `SPIRIT_FRAMES` frames baked from its
    /// still picture once it loads (`bake_spirits`).
    spirits: Vec<Option<Handle<Image>>>,
}

/// How each spirit (`Kind::Spirit` files, in order) moves, and how fast
/// (frames a second).
#[derive(Clone, Copy)]
enum Motion {
    /// The tail waves: rows sway, more towards the bottom.
    Tail,
    /// The bell swells and the tentacles sway.
    Pulse,
    /// The wings beat: everything off the body narrows and widens.
    Wings,
    /// A flame licks: rows sway, more towards the top.
    Flicker,
}

const SPIRIT_MOTION: [(Motion, f32); 4] = [
    (Motion::Tail, 7.0),
    (Motion::Pulse, 6.0),
    (Motion::Wings, 14.0),
    (Motion::Flicker, 11.0),
];
const SPIRIT_FRAMES: u32 = 8;

/// The spirits' frames, made from their pictures by moving whole pixels
/// (no new colours, the outline kept): the art stays the art.
fn bake_spirits(
    mut art: ResMut<WorldArt>,
    mut images: ResMut<Assets<Image>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let files = Kind::Spirit.files();
    let mut all = true;
    for (i, file) in files.iter().enumerate() {
        if art.spirits.get(i).is_some_and(Option::is_some) {
            continue;
        }
        let Some(still) = images.get(&art.images[file]) else {
            all = false;
            continue;
        };
        let Some(strip) = spirit_strip(still, SPIRIT_MOTION[i % SPIRIT_MOTION.len()].0) else {
            continue;
        };
        art.spirits[i] = Some(images.add(strip));
    }
    *done = all;
}

/// One picture moved through `SPIRIT_FRAMES` frames: each pixel of a frame
/// is read from the still somewhere near (nearest pixel, never blended).
fn spirit_strip(still: &Image, motion: Motion) -> Option<Image> {
    use std::f32::consts::TAU;
    let data = still.data.as_ref()?;
    let (w, h) = (still.width() as i32, still.height() as i32);
    let mut strip = canvas(still.width() * SPIRIT_FRAMES, still.height());
    let width = (still.width() * SPIRIT_FRAMES) as usize;
    let out = strip.data.as_mut()?;
    // The middle column of what is drawn: the body of a moth.
    let opaque = |x: i32, y: i32| data[((y * w + x) * 4 + 3) as usize] > 0;
    let cols: Vec<i32> = (0..w).filter(|&x| (0..h).any(|y| opaque(x, y))).collect();
    let cx = (cols.first()? + cols.last()?) as f32 / 2.0;
    let rows: Vec<i32> = (0..h).filter(|&y| (0..w).any(|x| opaque(x, y))).collect();
    let (top, bottom) = (*rows.first()? as f32, *rows.last()? as f32);
    let tall = (bottom - top).max(1.0);
    for f in 0..SPIRIT_FRAMES as i32 {
        let phase = TAU * f as f32 / SPIRIT_FRAMES as f32;
        for y in 0..h {
            // How far down the drawing this row is, 0 at its top.
            let down = ((y as f32 - top) / tall).clamp(0.0, 1.0);
            for x in 0..w {
                let (sx, sy) = match motion {
                    Motion::Tail => {
                        let sway = (phase + y as f32 * 0.4).sin() * 2.0 * down * down;
                        (x as f32 - sway, y as f32)
                    }
                    Motion::Flicker => {
                        let up = 1.0 - down;
                        let sway = (phase + y as f32 * 0.7).sin() * 1.6 * up * up;
                        // The tip stretches up and settles.
                        let lift = (phase.sin() * 0.5 + 0.5) * 1.5 * up;
                        (x as f32 - sway, y as f32 + lift)
                    }
                    Motion::Pulse => {
                        if down < 0.45 {
                            // The bell: wider and narrower about the middle.
                            let k = 1.0 + 0.12 * phase.sin();
                            (cx + (x as f32 - cx) / k, y as f32)
                        } else {
                            let sway = (phase + y as f32 * 0.22).sin() * 1.2 * down;
                            (x as f32 - sway, y as f32 - phase.sin() * 1.0)
                        }
                    }
                    Motion::Wings => {
                        // Seen from above: the wings fold up and open again.
                        let k = 1.0 - 0.7 * (phase / 2.0).sin().abs();
                        let off = x as f32 - cx;
                        if off.abs() <= 1.0 {
                            (x as f32, y as f32)
                        } else {
                            (cx + off.signum() + (off - off.signum()) / k, y as f32)
                        }
                    }
                };
                let (sx, sy) = (sx.round() as i32, sy.round() as i32);
                if sx < 0 || sy < 0 || sx >= w || sy >= h {
                    continue;
                }
                let from = ((sy * w + sx) * 4) as usize;
                let to = (y as usize * width + (f * w + x) as usize) * 4;
                out[to..to + 4].copy_from_slice(&data[from..from + 4]);
            }
        }
    }
    Some(strip)
}

/// An optional picture of the menu's world: loaded if its file is there.
fn maybe(assets: &AssetServer, path: String) -> Option<Handle<Image>> {
    std::path::Path::new("assets")
        .join(&path)
        .exists()
        .then(|| assets.load(path))
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
            .map(|b| assets.load(format!("props/menu/animal-{}.png", b.name)))
            .collect(),
        struggles: ANIMALS
            .iter()
            .map(|b| {
                maybe(
                    &assets,
                    format!("props/menu/animal-{}-struggle.png", b.name),
                )
            })
            .collect(),
        flights: ANIMALS
            .iter()
            .map(|b| maybe(&assets, format!("props/menu/animal-{}-fly.png", b.name)))
            .collect(),
        heart: images.add(heart()),
        spirits: vec![None; Kind::Spirit.files().len()],
        ground_top: assets.load("props/menu/ground-top.png"),
        ground_fill: assets.load("props/menu/ground-fill.png"),
        firefly: images.add(firefly()),
        growth: [1, 2, 3].map(|n| assets.load(format!("props/menu/grow-{n}.png"))),
        rustle: Kind::Tree
            .files()
            .iter()
            .filter_map(|&file| {
                Some((file, maybe(&assets, format!("props/menu/tree-{file}.png"))?))
            })
            .collect(),
    });
}

/// A picture drawn in code, transparent to begin with.
fn canvas(width: u32, height: u32) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        // Kept on the CPU too: the drag reads its pixels.
        RenderAssetUsages::default(),
    )
}

/// A 7×6 heart: a deep rim, a pink fill, a glint.
fn heart() -> Image {
    const ROWS: [&str; 6] = [
        ".##.##.", //
        "#pp#ww#", //
        "#ppppp#", //
        ".#ppp#.", //
        "..#p#..", //
        "...#...",
    ];
    let mut image = canvas(7, 6);
    let data = image.data.as_mut().expect("canvas allocates pixel data");
    for (y, row) in ROWS.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            let rgba: [u8; 4] = match c {
                b'#' => [150, 20, 60, 255],
                b'p' => [245, 80, 120, 255],
                b'w' => [255, 200, 215, 255],
                _ => continue,
            };
            let i = (y * 7 + x) * 4;
            data[i..i + 4].copy_from_slice(&rgba);
        }
    }
    image
}

/// The black hole's side in texels, and its frames.
const HOLE: u32 = 40;
const HOLE_FRAMES: u32 = 8;

/// The black hole: a black core in a bright ring, a disc of two arms
/// winding in, violet outside and hot near the core. A row of frames, the
/// arms turned a little more in each (half a turn over the row: two arms
/// make it seamless).
fn hole_frames() -> Image {
    use std::f32::consts::PI;
    let n = HOLE as i32;
    let mut image = canvas(HOLE * HOLE_FRAMES, HOLE);
    let width = (HOLE * HOLE_FRAMES) as usize;
    let data = image.data.as_mut().expect("canvas allocates pixel data");
    for f in 0..HOLE_FRAMES {
        let phase = PI * f as f32 / HOLE_FRAMES as f32;
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 - 19.5, y as f32 - 19.5);
                let r = (dx * dx + dy * dy).sqrt();
                let theta = dy.atan2(dx);
                let rgba: [u8; 4] = if r < 7.0 {
                    [6, 2, 12, 255]
                } else if r < 8.6 {
                    [255, 226, 170, 255]
                } else if r < 19.5 {
                    let arm = (2.0 * (theta - phase) + 4.5 * (r / 8.0).ln()).cos();
                    let near = 1.0 - (r - 8.6) / 10.9;
                    let v = 0.55 * (0.5 + 0.5 * arm) + 0.6 * near;
                    match v {
                        v if v > 0.95 => [255, 180, 100, 255],
                        v if v > 0.75 => [230, 90, 150, 255],
                        v if v > 0.55 => [130, 55, 180, 255],
                        v if v > 0.38 => [60, 30, 110, 220],
                        _ => continue,
                    }
                } else {
                    continue;
                };
                let i = (y as usize * width + (f as i32 * n + x) as usize) * 4;
                data[i..i + 4].copy_from_slice(&rgba);
            }
        }
    }
    image
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
    /// A tree's empty rows over its crown (where birds sit).
    crown: f32,
    /// Dropped into the black hole: when, and from where (its middle).
    swallowed: Option<(f32, Vec2)>,
    /// Drawn over the menu panel (a bird up in the air or sitting on it).
    lifted: bool,
}

struct Wander {
    x: f32,
    to: f32,
    rest_until: f32,
    /// Seconds into the present hop or walk.
    t: f32,
    east: bool,
    /// Speed times this: running from the fox.
    hurry: f32,
    mood: Mood,
    /// Not before this does it fall in love again.
    love_after: f32,
    /// Where a bird sits when not on the ground, and its flight.
    perch: Perch,
    flight: Option<Flight>,
}

impl Wander {
    fn new(x: f32, now: f32, east: bool) -> Wander {
        Wander {
            x,
            to: x,
            rest_until: now + 1.0,
            t: 0.0,
            east,
            hurry: 1.0,
            mood: Mood::Roam,
            love_after: now + 8.0,
            perch: Perch::Ground(x),
            flight: None,
        }
    }

    /// Standing on the ground, not on the way anywhere.
    fn idle_on_ground(&self) -> bool {
        self.flight.is_none() && matches!(self.perch, Perch::Ground(_)) && self.to == self.x
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mood {
    Roam,
    /// Running from the fox to `to`.
    Flee,
    /// Going to meet `with`; once both are there (`met`), hearts rise.
    Love {
        with: u64,
        until: f32,
        met: Option<f32>,
        next_heart: f32,
    },
}

/// Where a bird lands.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Perch {
    /// On the ground, so far across the window.
    Ground(f32),
    /// On the menu panel's top edge, so far along it.
    Panel(f32),
    /// On the crown of a tree, so many texels off its middle.
    Tree(u64, f32),
}

struct Flight {
    /// Where its feet left from (pixels).
    from: Vec2,
    to: Perch,
    t: f32,
    secs: f32,
}

/// Where birds may land this frame: the panel's box, the top of every tree
/// (pixels; filled by `behave`, read by `animate` and `drag`).
#[derive(Resource, Default)]
struct Perches {
    panel: Option<Rect>,
    trees: Vec<(u64, Vec2)>,
    /// Where an animal's feet stand on the ground line.
    ground: f32,
    w: f32,
    s: f32,
}

impl Perches {
    /// Where the feet go on a perch, if it is still there.
    fn point(&self, perch: Perch) -> Option<Vec2> {
        match perch {
            Perch::Ground(x) => Some(Vec2::new(x * self.w, self.ground)),
            Perch::Panel(u) => self
                .panel
                .map(|r| Vec2::new(r.min.x + u * r.width(), r.min.y + 2.0 * self.s)),
            Perch::Tree(id, dx) => self
                .trees
                .iter()
                .find(|(t, _)| *t == id)
                .map(|(_, top)| *top + Vec2::new(dx * self.s, 3.0 * self.s)),
        }
    }

    /// Somewhere to fly to from `x` (across the window), by two rolls:
    /// the panel, a tree, or the ground a way off. `high` leaves the
    /// ground out while there is anywhere else.
    fn choose(&self, x: f32, a: f32, b: f32, high: bool) -> Perch {
        let mut options: Vec<Perch> = Vec::new();
        if self.panel.is_some() {
            options.push(Perch::Panel(0.12 + 0.76 * b));
        }
        for (id, _) in &self.trees {
            options.push(Perch::Tree(*id, (b - 0.5) * 8.0));
        }
        if !high || options.is_empty() {
            let way = if b < 0.5 { -1.0 } else { 1.0 };
            let mut to = x + way * (0.2 + 0.25 * a);
            if !(0.04..=0.96).contains(&to) {
                to = x - way * (0.2 + 0.25 * a);
            }
            options.push(Perch::Ground(to.clamp(0.04, 0.96)));
        }
        options[((a * options.len() as f32) as usize).min(options.len() - 1)]
    }

    /// A flight from `from` to `to`: longer for a longer way.
    fn flight(&self, from: Vec2, to: Perch) -> Flight {
        let there = self.point(to).unwrap_or(from);
        let secs = (from.distance(there) / (110.0 * self.s)).clamp(0.7, 3.5);
        Flight {
            from,
            to,
            t: 0.0,
            secs,
        }
    }
}

/// A number in 0..1 from the time, a thing's seed and a salt: the menu's
/// dice (nothing here needs to be fair, only to look free).
fn roll(now: f32, seed: f32, salt: f32) -> f32 {
    ((now * 12.989_8 + seed * 78.233 + salt * 37.719).sin() * 43_758.547)
        .fract()
        .abs()
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

/// The number of empty rows over the highest opaque pixel.
fn empty_rows_above(image: &Image) -> Option<f32> {
    let data = image.data.as_ref()?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let highest =
        (0..h).find(|&y| (0..w).any(|x| data.get((y * w + x) * 4 + 3).is_some_and(|&a| a > 0)))?;
    Some(highest as f32)
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
                    walk: (thing.kind == Kind::Animal)
                        .then(|| Wander::new(thing.x, now, thing.id % 2 == 0)),
                    fall: None,
                    crown: if thing.kind == Kind::Tree {
                        empty_rows_above(picture).unwrap_or(0.0)
                    } else {
                        0.0
                    },
                    swallowed: None,
                    lifted: false,
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
    /// The cursor is over the black hole.
    hungry: bool,
    /// When the hole last swallowed something (it stays open to finish).
    fed: f32,
}

/// The black hole's middle (pixels), in the top right corner.
fn hole_centre(w: f32, s: f32) -> Vec2 {
    Vec2::new(
        w - (HOLE as f32 / 2.0 + 14.0) * s,
        (HOLE as f32 / 2.0 + 14.0) * s,
    )
}

/// How near its middle the cursor lets a thing fall in.
fn hole_reach(s: f32) -> f32 {
    (HOLE as f32 / 2.0 + 10.0) * s
}

/// Seconds a swallowed thing takes to wind into the hole.
const SWALLOW_SECS: f32 = 0.7;

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
    perches: Res<Perches>,
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
    let under = cursor
        .filter(|_| !over_menu && carried.id.is_none())
        .and_then(|c| {
            shown
                .iter()
                .filter(|(_, sh, _, _)| {
                    sh.gone.is_none() && sh.swallowed.is_none() && draggable(sh.thing.kind)
                })
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
        if shown
            .get(entity)
            .is_ok_and(|(_, sh, _, _)| sh.thing.kind == Kind::Animal)
        {
            sounds.write(Sound::new("menu-squeak"));
        }
        // Over the menu while carried, so it is never lost behind it.
        commands.entity(entity).insert(GlobalZIndex(54));
    }

    let hole = hole_centre(w, s);
    carried.hungry =
        carried.id.is_some() && cursor.is_some_and(|c| c.distance(hole) < hole_reach(s));
    if let (Some(id), Some(c)) = (carried.id, cursor) {
        carried.at = c - carried.grab;
        // Over the hole the thing is drawn in a little.
        if carried.hungry {
            carried.at += (hole - c) * 0.3;
        }
        let kind = shown
            .iter()
            .find(|(_, sh, _, _)| sh.id == id)
            .map(|(_, sh, _, _)| sh.thing.kind);
        // The sun and the moon wind the clock as they go.
        if let Some(kind @ (Kind::Sun | Kind::Moon)) = kind {
            let u = ((c.x / w - 0.06) / 0.88).clamp(0.0, 1.0);
            world.clock = if kind == Kind::Sun {
                u / 2.0
            } else {
                0.5 + u / 2.0
            };
        }
    }

    if mouse.just_released(MouseButton::Left)
        && let Some(id) = carried.id.take()
    {
        let at = carried.at;
        let fed = carried.hungry;
        carried.hungry = false;
        if let Some((entity, mut sh, _, _)) = shown.iter_mut().find(|(_, sh, _, _)| sh.id == id) {
            let size = sh.size * s;
            let centre = at + size / 2.0;
            if fed {
                // Into the black hole, and out of the world for good. It
                // stays over the menu while it winds in.
                sh.swallowed = Some((now, centre));
                carried.fed = now;
                world.remove(id);
                sounds.write(Sound::new("menu-swallow"));
                if sh.thing.kind == Kind::Animal {
                    sounds.write(Sound::new("menu-squeak").at(0.6));
                }
                return;
            }
            commands.entity(entity).remove::<GlobalZIndex>();
            sh.lifted = false;
            let seed = seed_of(sh.id);
            let mut thing = sh.thing;
            let flies = thing.kind == Kind::Animal && ANIMALS[thing.variant].flies;
            match thing.kind {
                // A bird let go flutters down to the ground below.
                _ if flies => {
                    let feet = Vec2::new(centre.x, at.y + size.y - sh.empty * s);
                    thing.x = (centre.x / w).clamp(0.04, 0.96);
                    if let Some(wander) = sh.walk.as_mut() {
                        wander.flight = Some(perches.flight(feet, Perch::Ground(thing.x)));
                        wander.mood = Mood::Roam;
                        wander.hurry = 1.0;
                    }
                    sounds.write(Sound::new("menu-flap").at(0.6));
                }
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
                        wander.mood = Mood::Roam;
                        wander.hurry = 1.0;
                        wander.perch = Perch::Ground(thing.x);
                        wander.flight = None;
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
        commands
            .entity(window_entity)
            .insert(CursorIcon::from(icon));
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

/// The black hole in the top right corner.
#[derive(Component)]
struct Hole {
    /// 0 unseen .. 1 open.
    open: f32,
}

/// The hole opens while something is carried (and while it swallows),
/// turning faster when the thing is over it.
fn hole(
    time: Res<Time>,
    carried: Res<Drag>,
    window: Single<&Window, With<PrimaryWindow>>,
    hole: Single<(&mut Hole, &mut ImageNode, &mut Node)>,
) {
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    let (w, s) = (window.width(), scale(window.height()));
    let (mut hole, mut image, mut node) = hole.into_inner();
    // `NECROMY_MENU_HOLE=1` keeps it open, for screenshots.
    let pinned = std::env::var_os("NECROMY_MENU_HOLE").is_some();
    let wanted = pinned || carried.id.is_some() || now - carried.fed < SWALLOW_SECS + 0.3;
    hole.open = if wanted {
        (hole.open + dt * 4.0).min(1.0)
    } else {
        (hole.open - dt * 2.5).max(0.0)
    };
    let alpha = hole.open * hole.open * (3.0 - 2.0 * hole.open);
    let want = Color::WHITE.with_alpha(alpha);
    if image.color != want {
        image.color = want;
    }
    if hole.open == 0.0 {
        return;
    }
    let speed = if carried.hungry { 22.0 } else { 9.0 };
    let frame = ((now * speed) as u32 % HOLE_FRAMES) as f32;
    let cell = HOLE as f32;
    image.rect = Some(Rect::new(frame * cell, 0.0, (frame + 1.0) * cell, cell));
    let size = cell * s;
    let centre = hole_centre(w, s);
    let (left, top) = (
        px((centre.x - size / 2.0).round()),
        px((centre.y - size / 2.0).round()),
    );
    if node.left != left || node.top != top || node.width != px(size) {
        node.left = left;
        node.top = top;
        node.width = px(size);
        node.height = px(size);
    }
}

/// A heart rising over two animals in love.
#[derive(Component)]
struct Heart {
    born: f32,
    /// Where it rose from (pixels: its middle).
    from: Vec2,
    seed: f32,
}

const HEART_SECS: f32 = 1.8;

/// Hearts rise, sway and fade.
fn hearts(
    mut commands: Commands,
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut hearts: Query<(Entity, &Heart, &mut ImageNode, &mut Node)>,
) {
    let now = time.elapsed_secs();
    let s = scale(window.height());
    for (entity, heart, mut image, mut node) in &mut hearts {
        let age = now - heart.born;
        if age > HEART_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        let u = age / HEART_SECS;
        let x = heart.from.x + (age * 4.0 + heart.seed * 6.0).sin() * 2.5 * s;
        let y = heart.from.y - age * 16.0 * s;
        node.left = px((x - 3.5 * s).round());
        node.top = px((y - 3.0 * s).round());
        node.width = px(7.0 * s);
        node.height = px(6.0 * s);
        image.color = Color::WHITE.with_alpha((1.0 - u * u).clamp(0.0, 1.0));
    }
}

/// How near the fox (a share of the window's width) the prey bolts.
const FEAR: f32 = 0.14;
/// How far the fox looks for prey.
const SCENT: f32 = 0.45;
/// How far apart two of a kind notice each other.
const LONGING: f32 = 0.3;

/// What one animal is up to, as the others see it.
#[derive(Clone, Copy)]
struct Seen {
    id: u64,
    variant: usize,
    x: f32,
    /// On the ground, free (not carried, falling, flying or swallowed).
    down: bool,
    idle: bool,
    mood: Mood,
    love_after: f32,
}

/// The animals mind each other: the fox stalks, the prey runs or flies up,
/// two of a kind meet and love, birds take wing. Also finds the perches.
#[allow(clippy::too_many_arguments)]
fn behave(
    mut commands: Commands,
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    panel: Query<(&ComputedNode, &UiGlobalTransform), With<MenuPanel>>,
    art: Res<WorldArt>,
    back: Single<Entity, With<Back>>,
    carried: Res<Drag>,
    mut perches: ResMut<Perches>,
    mut shown: Query<(&mut Shown, &Node)>,
    mut sounds: MessageWriter<Sound>,
) {
    let now = time.elapsed_secs();
    let (w, h) = (window.width(), window.height());
    let s = scale(h);
    let px_of = |v: Val| if let Val::Px(p) = v { p } else { 0.0 };

    // Where birds may land: the panel's top, every standing tree's crown.
    perches.w = w;
    perches.s = s;
    perches.ground = h - FLOOR + Kind::Animal.sink() * s;
    perches.panel = crate::menu_stage::panel_rect(&panel);
    perches.trees = shown
        .iter()
        .filter(|(sh, _)| {
            sh.thing.kind == Kind::Tree
                && sh.gone.is_none()
                && sh.swallowed.is_none()
                && now - sh.since > 1.2
        })
        .map(|(sh, node)| {
            let middle = px_of(node.left) + px_of(node.width) / 2.0;
            (sh.id, Vec2::new(middle, px_of(node.top) + sh.crown * s))
        })
        .collect();

    let seen: Vec<Seen> = shown
        .iter()
        .filter_map(|(sh, _)| {
            let wander = sh.walk.as_ref()?;
            let free = sh.gone.is_none()
                && sh.swallowed.is_none()
                && sh.fall.is_none()
                && carried.id != Some(sh.id);
            Some(Seen {
                id: sh.id,
                variant: sh.thing.variant,
                x: wander.x,
                down: free && wander.flight.is_none() && matches!(wander.perch, Perch::Ground(_)),
                idle: wander.idle_on_ground(),
                mood: wander.mood,
                love_after: wander.love_after,
            })
        })
        .collect();
    let hunters: Vec<f32> = seen
        .iter()
        .filter(|a| a.down && ANIMALS[a.variant].hunts)
        .map(|a| a.x)
        .collect();

    for (mut sh, _) in &mut shown {
        let id = sh.id;
        let Some(me) = seen.iter().find(|a| a.id == id).copied() else {
            continue;
        };
        if !me.down {
            continue;
        }
        let beast = &ANIMALS[me.variant];
        let seed = seed_of(id);
        let wander = sh.walk.as_mut().expect("an animal wanders");

        // The prey bolts from a fox near it: away along the ground, or up.
        let fox = hunters
            .iter()
            .copied()
            .filter(|f| (f - me.x).abs() < FEAR)
            .min_by(|a, b| (a - me.x).abs().total_cmp(&(b - me.x).abs()));
        if beast.prey
            && let Some(fox) = fox
            && wander.mood != Mood::Flee
        {
            wander.mood = Mood::Flee;
            wander.rest_until = now;
            wander.t = 0.0;
            if beast.flies {
                let from = Vec2::new(me.x * w, perches.ground);
                let to = perches.choose(me.x, roll(now, seed, 1.0), roll(now, seed, 2.0), true);
                wander.flight = Some(perches.flight(from, to));
                wander.mood = Mood::Roam;
                sounds.write(Sound::new("menu-flap").at(0.7));
            } else {
                let away = if me.x >= fox { 1.0 } else { -1.0 };
                let mut to = me.x + away * 0.3;
                // Cornered against the edge: dash past the fox instead.
                if !(0.04..=0.96).contains(&to) && (to.clamp(0.04, 0.96) - me.x).abs() < 0.08 {
                    to = fox - away * 0.2;
                }
                wander.to = to.clamp(0.04, 0.96);
                wander.hurry = 2.4;
                debug!("the {} runs from the fox", beast.name);
                sounds.write(Sound::new("menu-squeak").at(0.5));
            }
            continue;
        }

        // The fox, resting, picks the nearest prey and goes after it (not
        // every time: it also just wanders).
        if beast.hunts && me.idle {
            let prey = seen
                .iter()
                .filter(|a| a.down && a.id != id && ANIMALS[a.variant].prey)
                .filter(|a| (a.x - me.x).abs() < SCENT)
                .min_by(|a, b| (a.x - me.x).abs().total_cmp(&(b.x - me.x).abs()));
            if let Some(prey) = prey
                && roll(wander.rest_until, seed, 3.0) < 0.6
            {
                let side = if prey.x > me.x { -1.0 } else { 1.0 };
                wander.to = (prey.x + side * 0.05).clamp(0.04, 0.96);
            }
        }

        match wander.mood {
            Mood::Love {
                with,
                until,
                met,
                next_heart,
            } => {
                let partner = seen.iter().find(|a| a.id == with).copied();
                let faithful = partner.is_some_and(|p| {
                    p.down && matches!(p.mood, Mood::Love { with, .. } if with == id)
                });
                let over = met.is_some_and(|m| now - m > 4.5) || now > until;
                if !faithful || over {
                    wander.mood = Mood::Roam;
                    wander.love_after = now + if over { 40.0 } else { 10.0 };
                    wander.rest_until = wander.rest_until.max(now + 1.5);
                    continue;
                }
                let p = partner.expect("faithful means there");
                if me.idle && p.idle {
                    // Face each other.
                    wander.east = p.x > me.x;
                    wander.rest_until = now + 1.0;
                    let met = met.unwrap_or(now);
                    let mut next = next_heart;
                    // The one of the pair with the smaller id keeps the hearts.
                    if id < with && now >= next_heart {
                        let mid = Vec2::new((me.x + p.x) / 2.0 * w, perches.ground - 26.0 * s);
                        let k = (now * 3.0) as i32 % 3 - 1;
                        let heart = commands
                            .spawn((
                                Heart {
                                    born: now,
                                    from: mid + Vec2::new(k as f32 * 5.0 * s, 0.0),
                                    seed: roll(now, seed, 4.0),
                                },
                                ImageNode::new(art.heart.clone())
                                    .with_color(Color::WHITE.with_alpha(0.0)),
                                Node {
                                    position_type: PositionType::Absolute,
                                    ..default()
                                },
                                ZIndex(9),
                                Pickable::IGNORE,
                            ))
                            .id();
                        commands.entity(*back).add_child(heart);
                        if next_heart == 0.0 {
                            sounds.write(Sound::new("menu-love"));
                            debug!("{} and {} meet", beast.name, ANIMALS[p.variant].name);
                        }
                        next = now + 0.45;
                    }
                    wander.mood = Mood::Love {
                        with,
                        until,
                        met: Some(met),
                        next_heart: next,
                    };
                }
            }
            Mood::Flee => {}
            Mood::Roam => {
                // Two of a kind, both free and calm, no fox about: they go
                // to meet halfway.
                if !me.idle || me.love_after > now || fox.is_some() {
                    continue;
                }
                let Some(other) = seen.iter().find(|a| {
                    a.id != id
                        && a.idle
                        && a.mood == Mood::Roam
                        && a.love_after <= now
                        && ANIMALS[a.variant].kin == beast.kin
                        && (a.x - me.x).abs() < LONGING
                }) else {
                    continue;
                };
                let mid = (me.x + other.x) / 2.0;
                let gap = 24.0 * s / w;
                let side = if me.x < other.x { -1.0 } else { 1.0 };
                wander.to = (mid + side * gap / 2.0).clamp(0.02, 0.98);
                wander.rest_until = now;
                wander.mood = Mood::Love {
                    with: other.id,
                    until: now + 15.0,
                    met: None,
                    next_heart: 0.0,
                };
            }
        }
    }

    // The other half of each new pair answers in the same frame.
    let pairs: Vec<(u64, u64, f32)> = shown
        .iter()
        .filter_map(|(sh, _)| match sh.walk.as_ref()?.mood {
            Mood::Love {
                with,
                met: None,
                until,
                ..
            } => Some((sh.id, with, until)),
            _ => None,
        })
        .collect();
    for (mut sh, _) in &mut shown {
        let id = sh.id;
        let Some(&(from, _, until)) = pairs.iter().find(|(_, with, _)| *with == id) else {
            continue;
        };
        let Some(wander) = sh.walk.as_mut() else {
            continue;
        };
        if wander.mood != Mood::Roam {
            continue;
        }
        let Some(other) = seen.iter().find(|a| a.id == from) else {
            continue;
        };
        let mid = (wander.x + other.x) / 2.0;
        let gap = 24.0 * s / w;
        let side = if wander.x < other.x { -1.0 } else { 1.0 };
        wander.to = (mid + side * gap / 2.0).clamp(0.02, 0.98);
        wander.rest_until = now;
        wander.mood = Mood::Love {
            with: from,
            until,
            met: None,
            next_heart: 0.0,
        };
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn animate(
    time: Res<Time>,
    art: Res<WorldArt>,
    images: Res<Assets<Image>>,
    world: Res<MenuWorld>,
    carried: Res<Drag>,
    perches: Res<Perches>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    mut shown: Query<(
        Entity,
        &mut Shown,
        &mut ImageNode,
        &mut Node,
        Option<&Children>,
    )>,
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
    for (entity, mut shown, mut image, mut node, children) in &mut shown {
        let thing = shown.thing;
        let kind = thing.kind;
        // One pixel size with the champions: pictures are drawn at the size
        // they should stand (small things are small pictures).
        let k = s;
        // Into the black hole: round and round, smaller and smaller.
        if let Some((t0, from)) = shown.swallowed {
            let u = ((now - t0) / SWALLOW_SECS).clamp(0.0, 1.0);
            let hole = hole_centre(w, s);
            let turn = u * u * 6.0;
            let off = (from - hole) * (1.0 - u).powf(1.3);
            let off = Vec2::new(
                off.x * turn.cos() - off.y * turn.sin(),
                off.x * turn.sin() + off.y * turn.cos(),
            );
            let centre = hole + off;
            let size = shown.size * k * (1.0 - u).powf(1.5);
            node.left = px((centre.x - size.x / 2.0).round());
            node.top = px((centre.y - size.y / 2.0).round());
            node.width = px(size.x);
            node.height = px(size.y);
            image.color = Color::WHITE.with_alpha(1.0 - 0.4 * u);
            continue;
        }
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
        let colour = match kind {
            // The caps' light swells and ebbs, each in its own time.
            Kind::Glowcap => {
                let pulse = 0.5 + 0.5 * (now * 1.6 + seed * 9.0).sin();
                Color::srgb(0.7, 0.78, 0.8).mix(&Color::WHITE, pulse * pulse)
            }
            kind if glows(kind) => Color::WHITE,
            _ => tint,
        };
        let carried_here = carried.id == Some(shown.id);
        let (mut x, mut y);
        // A bird's feet off the ground: in flight or on a perch.
        let mut aloft: Option<Vec2> = None;
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
                // And move: wings beat, a tail waves, a flame flickers.
                if let Some(strip) = art.spirits[thing.variant].as_ref() {
                    let fps = SPIRIT_MOTION[thing.variant].1;
                    let frame = (now * fps + seed * 8.0) as u32 % SPIRIT_FRAMES;
                    let cell = shown.size.x;
                    if image.image != *strip {
                        image.image = strip.clone();
                    }
                    image.rect = Some(Rect::new(
                        frame as f32 * cell,
                        0.0,
                        (frame + 1) as f32 * cell,
                        shown.size.y,
                    ));
                }
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
                let beast = &ANIMALS[thing.variant];
                // Frame 0 stands; the rest are the hop or the walk.
                let moving = shown.frames.saturating_sub(1).max(1);
                let falling = shown.fall.is_some() || carried_here;
                let wander = shown.walk.as_mut().expect("an animal wanders");
                let mut frame = 0usize;
                let mut sheet = art.animals[thing.variant].clone();
                // A perch gone from under a bird (its tree taken away), or a
                // flight to one: down to the ground below.
                if wander.flight.is_none() && perches.point(wander.perch).is_none() {
                    let here = Vec2::new(wander.x * w, perches.ground - 40.0 * s);
                    wander.flight = Some(perches.flight(here, Perch::Ground(wander.x)));
                }
                if let Some(flight) = wander.flight.as_mut()
                    && perches.point(flight.to).is_none()
                {
                    flight.to = Perch::Ground((flight.from.x / w).clamp(0.04, 0.96));
                }
                if let Some(flight) = wander.flight.as_mut() {
                    if !carried_here {
                        flight.t += dt;
                    }
                    let u = (flight.t / flight.secs).clamp(0.0, 1.0);
                    let there = perches.point(flight.to).unwrap_or(flight.from);
                    let lift = (flight.from.distance(there) * 0.25).min(70.0 * s) + 14.0 * s;
                    let eased = u * u * (3.0 - 2.0 * u);
                    let feet = flight.from.lerp(there, eased)
                        - Vec2::Y * lift * (u * std::f32::consts::PI).sin();
                    if (there.x - flight.from.x).abs() > 1.0 {
                        wander.east = there.x > flight.from.x;
                    }
                    // Wingbeats; frame 0 (standing) only as it touches down.
                    let strip = art.flights[thing.variant]
                        .as_ref()
                        .or(art.struggles[thing.variant].as_ref());
                    if let Some(strip) = strip
                        && let Some(picture) = images.get(strip)
                        && u < 0.94
                    {
                        let loops = ((picture.width() / ANIMAL_CELL) as usize)
                            .saturating_sub(1)
                            .max(1);
                        frame = 1 + (now * 14.0 + seed * 7.0) as usize % loops;
                        sheet = strip.clone();
                    }
                    aloft = Some(feet);
                    if u >= 1.0 {
                        wander.perch = flight.to;
                        wander.flight = None;
                        wander.t = 0.0;
                        let r = roll(now, seed, 5.0);
                        match wander.perch {
                            Perch::Ground(gx) => {
                                wander.x = gx;
                                wander.to = gx;
                                wander.rest_until = now + 1.0 + 3.0 * r;
                            }
                            // Sits a while up there.
                            _ => wander.rest_until = now + 4.0 + 9.0 * r,
                        }
                    }
                } else if !matches!(wander.perch, Perch::Ground(_)) {
                    // Sitting up high; now and then it looks the other way,
                    // and when rested it flies on.
                    aloft = perches.point(wander.perch);
                    if (now * 0.7 + seed * 5.0).sin() > 0.97 {
                        wander.east = (now * 0.3 + seed).sin() > 0.0;
                    }
                    if now >= wander.rest_until && !falling {
                        let feet = aloft.unwrap_or(Vec2::new(wander.x * w, perches.ground));
                        let x_now = feet.x / w;
                        let high = roll(now, seed, 6.0) < 0.4;
                        let to =
                            perches.choose(x_now, roll(now, seed, 7.0), roll(now, seed, 8.0), high);
                        let to = if to == wander.perch {
                            Perch::Ground(x_now.clamp(0.04, 0.96))
                        } else {
                            to
                        };
                        wander.x = x_now.clamp(0.0, 1.0);
                        wander.flight = Some(perches.flight(feet, to));
                        sounds.write(Sound::new("menu-flap").at(0.5));
                    }
                } else if now >= wander.rest_until && !falling {
                    if wander.to == wander.x && matches!(wander.mood, Mood::Roam) {
                        // Rested: a bird may fly up; anyone goes somewhere
                        // else, never off the window.
                        if beast.flies && roll(now, seed, 9.0) < 0.4 {
                            let from = Vec2::new(wander.x * w, perches.ground);
                            let to = perches.choose(
                                wander.x,
                                roll(now, seed, 10.0),
                                roll(now, seed, 11.0),
                                true,
                            );
                            wander.flight = Some(perches.flight(from, to));
                            sounds.write(Sound::new("menu-flap").at(0.5));
                        } else {
                            let r = (now * 3.7 + seed * 13.0).sin();
                            wander.to = (wander.x + r * 0.25).clamp(0.04, 0.96);
                        }
                    }
                    if wander.to != wander.x && wander.flight.is_none() {
                        wander.t += dt;
                        let east = wander.to > wander.x;
                        wander.east = east;
                        let step = beast.speed * wander.hurry * s * dt / w;
                        let hop_secs = 0.6 / wander.hurry.sqrt();
                        match beast.gait {
                            Gait::Walk => {
                                frame =
                                    1 + (wander.t * 9.0 * wander.hurry.sqrt()) as usize % moving;
                                wander.x += if east { step } else { -step };
                            }
                            Gait::Hop => {
                                let u = (wander.t % hop_secs) / hop_secs;
                                frame = 1 + ((u * moving as f32) as usize).min(moving - 1);
                                wander.x += if east { step * 1.6 } else { -step * 1.6 };
                            }
                        }
                        let past = if east {
                            wander.x >= wander.to
                        } else {
                            wander.x <= wander.to
                        };
                        if past {
                            wander.x = wander.to;
                            wander.t = 0.0;
                            frame = 0;
                            wander.rest_until = now + 1.5 + 4.0 * (now * 7.3 + seed).sin().abs();
                            if wander.mood == Mood::Flee {
                                wander.mood = Mood::Roam;
                            }
                            wander.hurry = 1.0;
                        }
                    }
                }
                if wander.flight.is_none() && matches!(wander.perch, Perch::Ground(_)) {
                    wander.perch = Perch::Ground(wander.x);
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
                if let Some((strip, picture)) = struggle {
                    let loops = ((picture.width() / ANIMAL_CELL) as usize)
                        .saturating_sub(1)
                        .max(1);
                    frame = 1 + (now * 12.0) as usize % loops;
                    sheet = strip.clone();
                }
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
        if let Some(feet) = aloft {
            x = feet.x;
            y = feet.y + shown.empty * k;
        } else if kind.grounded() {
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
                let r = Rect::new(
                    frame as f32 * cell,
                    0.0,
                    (frame + 1) as f32 * cell,
                    shown.size.y,
                );
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
        } else {
            // A bird up in the air or on the panel is drawn over the menu.
            let lift = aloft.is_some();
            if shown.lifted != lift {
                shown.lifted = lift;
                if lift {
                    commands.entity(entity).insert(GlobalZIndex(51));
                } else {
                    commands.entity(entity).remove::<GlobalZIndex>();
                }
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
