//! The main menu's stage: now and then a champion walks in from a screen
//! edge, greets the player or does its god's trick, and walks on. Ahamar's
//! knight calls lightning up, Zaga's penitent raises a stone, Trishna's cook
//! spreads a feast, Maya's mourner leaps onto the menu panel; anyone may
//! wave. Click a champion to be greeted. Decoration only: nothing here
//! touches the menu's state.
//!
//! Walks come from the champion sheets (`sprites/<god>-champion.png`), the
//! rest from `sprites/<god>-menu.png` (`scripts/pixellab-menu.sh`): row 0
//! waves, row 1 is the god's own act, all facing south. Props lie in
//! `props/menu/`. Frames in a row and where the feet are are read from the
//! images, like the fight sheets.
//!
//! Dev aid: `NECROMY_MENU_ACT=<god>[:wave]` sends that champion first, at
//! once.

use std::collections::{HashMap, VecDeque};

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use necromy_rules::God;

use crate::audio::{Sound, Speech};
use crate::hud::{INK, UiFont};
use crate::menu_world::{Kind, MenuWorld};
use crate::play::Match;
use crate::ui_skin::{Accent, Frame};

const CELL: u32 = 96;
const WALK_COLUMNS: u32 = 6;
const WALK_ROWS: u32 = 8;
const ACT_COLUMNS: u32 = 12;
const ACT_ROWS: u32 = 3;
const ROW_IDLE: usize = 0;
const ROW_WALK_E: usize = 5;
const ROW_WALK_W: usize = 7;
const ROW_WAVE: usize = 0;
const ROW_ACT: usize = 1;
/// A second act, for those that have one (Maya's summoning).
const ROW_ALT: usize = 2;
/// Texels a second a champion walks.
const WALK_SPEED: f32 = 34.0;
const WALK_FPS: f32 = 9.0;
const ACT_FPS: f32 = 8.0;
/// Seconds between visits, the first one sooner.
const FIRST_VISIT: f32 = 2.5;
const BETWEEN: (f32, f32) = (5.0, 12.0);
/// Logical pixels between the feet and the bottom of the window.
pub(crate) const FLOOR: f32 = 26.0;

pub struct MenuStagePlugin;

impl Plugin for MenuStagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (load_art, spawn_root))
            .init_resource::<Visits>()
            .add_systems(
                Update,
                (begin_visit, clicked, walk_on, place, effects, bubbles)
                    .chain()
                    .run_if(not(resource_exists::<Match>)),
            )
            .add_systems(Update, clear.run_if(resource_added::<Match>));
    }
}

/// The menu's panel (the plate with the title): Maya leaps onto it.
#[derive(Component)]
pub struct MenuPanel;

#[derive(Component)]
struct StageRoot;

#[derive(Resource)]
struct MenuArt {
    walk: [Option<Handle<Image>>; 5],
    acts: [Option<Handle<Image>>; 5],
    walk_layout: Handle<TextureAtlasLayout>,
    act_layout: Handle<TextureAtlasLayout>,
    stones: [Handle<Image>; 3],
    feasts: [Handle<Image>; 3],
    /// Flowers to spring around Bhava's tree.
    flowers: Vec<Handle<Image>>,
    dust: Handle<Image>,
    splash: Handle<Image>,
}

fn load_art(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let sheets = |kind: &str| {
        God::ALL.map(|god| {
            let path = format!("sprites/{}-{kind}.png", god.name().to_lowercase());
            std::path::Path::new("assets")
                .join(&path)
                .exists()
                .then(|| assets.load(path))
        })
    };
    commands.insert_resource(MenuArt {
        walk: sheets("champion"),
        acts: sheets("menu"),
        walk_layout: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(CELL),
            WALK_COLUMNS,
            WALK_ROWS,
            None,
            None,
        )),
        act_layout: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(CELL),
            ACT_COLUMNS,
            ACT_ROWS,
            None,
            None,
        )),
        stones: [1, 2, 3].map(|n| assets.load(format!("props/menu/stone-{n}.png"))),
        feasts: [1, 2, 3].map(|n| assets.load(format!("props/menu/feast-{n}.png"))),
        flowers: (1..=FLOWERS)
            .map(|n| assets.load(format!("props/menu/flower-{n}.png")))
            .collect(),
        dust: assets.load("props/menu/dust.png"),
        splash: assets.load("props/menu/splash.png"),
    });
}

/// Over the menu (`GlobalZIndex(50)`), under the settings (60); lets the
/// pointer through to the menu everywhere but on a champion.
fn spawn_root(mut commands: Commands) {
    commands.spawn((
        StageRoot,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            height: percent(100.0),
            ..default()
        },
        Pickable::IGNORE,
        GlobalZIndex(52),
    ));
}

fn clear(mut commands: Commands, root: Query<Entity, With<StageRoot>>) {
    for root in &root {
        commands.entity(root).despawn();
    }
}

/// A sheet row: how many frames, and the row of pixels the feet stand on
/// in its first frame.
/// Rows read so far, by sheet and row.
type Rows = HashMap<(AssetId<Image>, usize), Option<Row>>;

#[derive(Clone, Copy)]
struct Row {
    frames: usize,
    feet: usize,
}

fn opaque(image: &Image, row: usize, col: usize, x: usize, y: usize) -> bool {
    let (Some(data), width) = (image.data.as_ref(), image.width() as usize) else {
        return false;
    };
    let (px, py) = (col * CELL as usize + x, row * CELL as usize + y);
    data.get((py * width + px) * 4 + 3).is_some_and(|&a| a > 0)
}

fn read_row(image: &Image, row: usize, columns: u32) -> Option<Row> {
    image.data.as_ref()?;
    if (row as u32 + 1) * CELL > image.height() {
        return None;
    }
    let c = CELL as usize;
    let filled = |col: usize| (0..c).any(|y| (0..c).any(|x| opaque(image, row, col, x, y)));
    let frames = (0..columns as usize).take_while(|&col| filled(col)).count();
    let feet = (0..c)
        .rev()
        .find(|&y| (0..c).any(|x| opaque(image, row, 0, x, y)))?;
    (frames > 0).then_some(Row { frames, feet })
}

/// Which sheet a pose comes from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Sheet {
    Walk,
    Act,
}

/// Something that happens once at a point of a visit.
#[derive(Clone, Copy, Debug)]
enum Cue {
    Bolt,
    Stone,
    Feast,
    /// Bhava's tree grows where he planted, flowers spring around.
    Grove,
    /// Maya's spirits and fireflies rise.
    Summon,
    Splash,
    /// The champion's greeting, in a bubble over its head.
    Hello,
    /// One of Ahamar's sayings (`SAYINGS`), in a bubble.
    Saying(usize),
}

/// Ahamar's knight speaks in wisdom of a sort: sayings collected from
/// people, one per line.
const SAYINGS: &str = include_str!("ahamar_sayings.txt");

fn saying(n: usize) -> &'static str {
    SAYINGS
        .lines()
        .filter(|l| !l.trim().is_empty())
        .nth(n)
        .unwrap_or("")
}

fn sayings() -> usize {
    SAYINGS.lines().filter(|l| !l.trim().is_empty()).count()
}

/// Seconds to type words out (`audio::Speech`) and let them be read.
fn reading_secs(text: &str) -> f32 {
    text.chars().count() as f32 / 32.0 + 1.5 + text.chars().count() as f32 / 40.0
}

/// Where a step goes, found against the window and the panel as they are
/// at that moment: the window may be resized mid-visit (it is maximized
/// just after start), and the panel moves with it.
#[derive(Clone, Copy, Debug)]
enum Mark {
    /// Just off a screen edge: the one walked in from, or the far one.
    Edge { far: bool },
    /// On the ground between the edge walked in from and the panel, 0..1
    /// of the way.
    Spot(f32),
    /// On the ground beside the panel's near or far side, `texels` out.
    Beside { far: bool, texels: f32 },
    /// On top of the panel, `texels` in from its near or far side.
    Atop { far: bool, texels: f32 },
}

/// The window and the panel, to find marks in.
struct Geometry {
    w: f32,
    h: f32,
    s: f32,
    panel: Rect,
}

impl Geometry {
    fn floor(&self) -> f32 {
        self.h - FLOOR
    }

    /// A mark's point for a visit heading east (or west).
    fn at(&self, mark: Mark, east: bool) -> Vec2 {
        let (s, half) = (self.s, CELL as f32 * self.s / 2.0);
        let dir = if east { 1.0 } else { -1.0 };
        let (near, far) = if east {
            (self.panel.min.x, self.panel.max.x)
        } else {
            (self.panel.max.x, self.panel.min.x)
        };
        let floor = self.floor();
        match mark {
            Mark::Edge { far } => {
                let west = east != far;
                Vec2::new(if west { -half } else { self.w + half }, floor)
            }
            Mark::Spot(u) => {
                // Room for a stone or a feast between the champion and the panel.
                let edge = if east { half } else { self.w - half };
                let inner = near - dir * 80.0 * s;
                let inner = if east { inner.max(edge) } else { inner.min(edge) };
                Vec2::new(edge + (inner - edge) * u, floor)
            }
            Mark::Beside { far: false, texels } => Vec2::new(near - dir * texels * s, floor),
            Mark::Beside { far: true, texels } => Vec2::new(far + dir * texels * s, floor),
            Mark::Atop { far: false, texels } => {
                Vec2::new(near + dir * texels * s, self.panel.min.y + 6.0)
            }
            Mark::Atop { far: true, texels } => {
                Vec2::new(far - dir * texels * s, self.panel.min.y + 6.0)
            }
        }
    }
}

fn geometry(
    window: &Window,
    panel: &Query<(&ComputedNode, &UiGlobalTransform), With<MenuPanel>>,
) -> Option<Geometry> {
    let (w, h) = (window.width(), window.height());
    Some(Geometry {
        w,
        h,
        s: scale(h),
        panel: panel_rect(panel)?,
    })
}

#[derive(Clone, Copy, Debug)]
enum Step {
    /// Walk to a mark, on its ground.
    Walk { to: Mark },
    /// Frames `from..to` of a row of the act sheet (backwards if `from >
    /// to`), then the last one held for `hold` seconds.
    Play {
        row: usize,
        from: usize,
        to: usize,
        hold: f32,
    },
    /// Stand facing the player.
    Idle { secs: f32 },
    /// Leap to a point: the jump row's frames before `air` in place, `air`
    /// held along the arc, the rest where it lands.
    Leap { to: Mark, air: usize, secs: f32 },
    Cue(Cue),
}

/// A champion on the stage and what it has left to do.
#[derive(Component)]
struct Visitor {
    god: God,
    feet: Vec2,
    plan: VecDeque<Step>,
    /// Seconds into the step at the front.
    t: f32,
    /// Where a leap started.
    from: Vec2,
    /// Facing east while walking.
    east: bool,
    /// Where the visit heads: east from the west edge, or west.
    heading: bool,
    /// The mark last reached: where the champion stands.
    ground: Mark,
    /// The pose shown: sheet, row, frame.
    pose: (Sheet, usize, usize),
    /// Greeted after a click already.
    greeted: bool,
}

#[derive(Resource)]
struct Visits {
    next_at: f32,
    count: u64,
    seed: u64,
    /// A dev visit: the god, only a wave, the second act.
    forced: Option<(God, bool, bool)>,
    /// Bubbles so far: each types out under its own key.
    said: u64,
    /// The saying last said, not to be said twice running.
    last_saying: Option<usize>,
}

impl Default for Visits {
    fn default() -> Self {
        let forced = std::env::var("NECROMY_MENU_ACT").ok().and_then(|s| {
            let (name, rest) = match s.split_once(':') {
                Some((name, rest)) => (name.to_string(), rest.to_string()),
                None => (s, String::new()),
            };
            God::ALL
                .into_iter()
                .find(|g| g.name().eq_ignore_ascii_case(&name))
                .map(|g| (g, rest == "wave", rest == "summon"))
        });
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(7, |d| d.as_nanos() as u64);
        Visits {
            next_at: if forced.is_some() { 0.3 } else { FIRST_VISIT },
            count: 0,
            seed,
            forced,
            said: 0,
            last_saying: None,
        }
    }
}

impl Visits {
    /// SplitMix64: decoration needs no better.
    fn roll(&mut self) -> f32 {
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A saying other than the last one, and how long it holds the knight.
    fn saying(&mut self) -> (Step, Step) {
        let n = sayings().max(1);
        let mut pick = (self.roll() * n as f32) as usize % n;
        if n > 1 && self.last_saying == Some(pick) {
            pick = (pick + 1) % n;
        }
        self.last_saying = Some(pick);
        (
            Step::Cue(Cue::Saying(pick)),
            Step::Idle {
                secs: reading_secs(saying(pick)),
            },
        )
    }
}

/// Screen pixels per texel: 3 on a 1080-line window.
pub(crate) fn scale(height: f32) -> f32 {
    (height / 360.0).round().clamp(2.0, 4.0)
}

/// The panel's rectangle on screen, in logical pixels.
fn panel_rect(panel: &Query<(&ComputedNode, &UiGlobalTransform), With<MenuPanel>>) -> Option<Rect> {
    let (node, at) = panel.iter().next()?;
    let centre = at.affine().translation * node.inverse_scale_factor();
    let size = node.size() * node.inverse_scale_factor();
    (size.x > 0.0).then(|| Rect::from_center_size(centre, size))
}

/// What a god's champion says on greeting.
fn hello(god: God) -> &'static str {
    match god {
        God::Bhava => "Тише… здесь всё растёт.",
        God::Trishna => "Садись, накормлю!",
        God::Zaga => "Камень помнит всех.",
        God::Ahamar => "Записан. Проходи.",
        God::Maya => "Ты меня видишь?",
    }
}

#[allow(clippy::too_many_arguments)]
fn begin_visit(
    mut commands: Commands,
    time: Res<Time>,
    mut visits: ResMut<Visits>,
    art: Res<MenuArt>,
    images: Res<Assets<Image>>,
    window: Single<&Window, With<PrimaryWindow>>,
    panel: Query<(&ComputedNode, &UiGlobalTransform), With<MenuPanel>>,
    root: Single<Entity, With<StageRoot>>,
    visitors: Query<(), With<Visitor>>,
) {
    let now = time.elapsed_secs();
    if !visitors.is_empty() || now < visits.next_at {
        return;
    }
    let Some(geo) = geometry(&window, &panel) else {
        return;
    };
    let s = geo.s;
    let forced = visits.forced.is_some();
    let (god, wave_only, alt) = match visits.forced.take() {
        Some(f) => f,
        None => {
            let god = God::ALL[(visits.roll() * 5.0) as usize % 5];
            (god, visits.roll() < 0.3, visits.roll() < 0.55)
        }
    };
    let (Some(walk), act) = (&art.walk[god.index()], &art.acts[god.index()]) else {
        visits.next_at = now + 1.0;
        return;
    };
    if images.get(walk).is_none() || act.as_ref().is_some_and(|a| images.get(a).is_none()) {
        return;
    }
    let act_rows = act
        .as_ref()
        .and_then(|a| images.get(a))
        .map(|image| {
            (
                read_row(image, ROW_WAVE, ACT_COLUMNS),
                read_row(image, ROW_ACT, ACT_COLUMNS),
            )
        })
        .unwrap_or((None, None));

    // A dev visit goes the same way every run, for screenshots by time.
    let east = forced || visits.roll() < 0.5;
    let start = geo.at(Mark::Edge { far: false }, east);
    let spot = Mark::Spot(if forced {
        0.6
    } else {
        0.35 + 0.5 * visits.roll()
    });

    let mut plan = VecDeque::new();
    let wave = |plan: &mut VecDeque<Step>| {
        if let (Some(row), _) = act_rows {
            plan.push_back(Step::Play {
                row: ROW_WAVE,
                from: 0,
                to: row.frames,
                hold: 0.2,
            });
        } else {
            plan.push_back(Step::Idle { secs: 1.2 });
        }
    };
    let alt_row = act
        .as_ref()
        .and_then(|a| images.get(a))
        .and_then(|image| read_row(image, ROW_ALT, ACT_COLUMNS));
    match (god, act_rows.1) {
        (God::Maya, _) if !wave_only && alt && alt_row.is_some() => {
            // Spirits and fireflies rise from her hands and stay.
            let n = alt_row.map_or(1, |r| r.frames);
            plan.push_back(Step::Walk { to: spot });
            plan.push_back(Step::Play {
                row: ROW_ALT,
                from: 0,
                to: 4.min(n),
                hold: 0.0,
            });
            plan.push_back(Step::Cue(Cue::Summon));
            plan.push_back(Step::Play {
                row: ROW_ALT,
                from: 4.min(n),
                to: n,
                hold: 0.5,
            });
            plan.push_back(Step::Cue(Cue::Hello));
            wave(&mut plan);
        }
        (God::Maya, Some(jump)) if !wave_only => {
            // Up onto the panel from beside it, along its top, down the far side.
            let air = jump.frames * 3 / 5;
            plan.push_back(Step::Walk {
                to: Mark::Beside {
                    far: false,
                    texels: 36.0,
                },
            });
            plan.push_back(Step::Leap {
                to: Mark::Atop {
                    far: false,
                    texels: 30.0,
                },
                air,
                secs: 0.9,
            });
            plan.push_back(Step::Cue(Cue::Splash));
            plan.push_back(Step::Cue(Cue::Hello));
            wave(&mut plan);
            plan.push_back(Step::Walk {
                to: Mark::Atop {
                    far: true,
                    texels: 30.0,
                },
            });
            plan.push_back(Step::Leap {
                to: Mark::Beside {
                    far: true,
                    texels: 36.0,
                },
                air,
                secs: 0.8,
            });
            plan.push_back(Step::Cue(Cue::Splash));
        }
        (God::Ahamar | God::Zaga | God::Trishna | God::Bhava, Some(row)) if !wave_only => {
            plan.push_back(Step::Walk { to: spot });
            let n = row.frames;
            // The frame the act lands on: an arm up, the stone called, the
            // cloth down.
            let (mid, cue, hold) = match god {
                God::Ahamar => (n * 2 / 3, Cue::Bolt, 0.9),
                God::Zaga => (n / 3, Cue::Stone, 0.4),
                God::Bhava => (n * 2 / 5, Cue::Grove, 0.8),
                _ => (n * 3 / 5, Cue::Feast, 1.6),
            };
            plan.push_back(Step::Play {
                row: ROW_ACT,
                from: 0,
                to: mid,
                hold: 0.0,
            });
            plan.push_back(Step::Cue(cue));
            plan.push_back(Step::Play {
                row: ROW_ACT,
                from: mid,
                to: n,
                hold,
            });
            if god == God::Trishna {
                // She sat down to it; up again.
                plan.push_back(Step::Play {
                    row: ROW_ACT,
                    from: n - 1,
                    to: mid,
                    hold: 0.0,
                });
            }
            if god == God::Ahamar {
                // The bolt, then a word of wisdom, and on his way.
                let (say, hold) = visits.saying();
                plan.push_back(say);
                plan.push_back(hold);
            } else {
                plan.push_back(Step::Cue(Cue::Hello));
                wave(&mut plan);
            }
        }
        _ => {
            plan.push_back(Step::Walk { to: spot });
            plan.push_back(Step::Cue(Cue::Hello));
            wave(&mut plan);
            plan.push_back(Step::Idle { secs: 0.6 });
        }
    }
    plan.push_back(Step::Walk {
        to: Mark::Edge { far: true },
    });

    visits.count += 1;
    let size = CELL as f32 * s;
    let visitor = commands
        .spawn((
            Visitor {
                god,
                feet: start,
                plan,
                t: 0.0,
                from: start,
                east,
                heading: east,
                ground: Mark::Edge { far: false },
                pose: (Sheet::Walk, if east { ROW_WALK_E } else { ROW_WALK_W }, 0),
                greeted: false,
            },
            Button,
            ImageNode::from_atlas_image(
                walk.clone(),
                TextureAtlas {
                    layout: art.walk_layout.clone(),
                    index: 0,
                },
            ),
            Node {
                position_type: PositionType::Absolute,
                left: px(start.x - size / 2.0),
                top: px(start.y - size),
                width: px(size),
                height: px(size),
                ..default()
            },
            ZIndex(2),
        ))
        .id();
    commands.entity(*root).add_child(visitor);
}

/// A click on a champion: it stops and greets the player (once a visit).
fn clicked(
    mut visitors: Query<(&Interaction, &mut Visitor), Changed<Interaction>>,
    mut visits: ResMut<Visits>,
    mut sounds: MessageWriter<Sound>,
) {
    for (interaction, mut v) in &mut visitors {
        if *interaction != Interaction::Pressed || v.greeted {
            continue;
        }
        // Only on the way: an act or a leap under way plays out.
        match v.plan.front() {
            Some(Step::Walk { .. }) => {}
            Some(Step::Idle { .. }) => {
                v.plan.pop_front();
            }
            _ => continue,
        }
        v.greeted = true;
        if v.god == God::Ahamar {
            // The knight answers with a saying, whatever he was about.
            let (say, hold) = visits.saying();
            v.plan.retain(|s| !matches!(s, Step::Cue(Cue::Saying(_))));
            v.plan.push_front(hold);
            v.plan.push_front(say);
            v.t = 0.0;
            sounds.write(Sound::new("reveal").at(0.6));
            continue;
        }
        // Greeted now, not again later.
        v.plan.retain(|s| !matches!(s, Step::Cue(Cue::Hello)));
        let wave = Step::Play {
            row: ROW_WAVE,
            from: 0,
            to: ACT_COLUMNS as usize,
            hold: 0.2,
        };
        // A walk resumes from where it stopped.
        v.plan.push_front(wave);
        v.plan.push_front(Step::Cue(Cue::Hello));
        v.t = 0.0;
        sounds.write(Sound::new("reveal").at(0.6));
    }
}

/// Moves every champion along its plan, firing its cues.
#[allow(clippy::too_many_arguments)]
fn walk_on(
    mut commands: Commands,
    time: Res<Time>,
    mut visits: ResMut<Visits>,
    art: Res<MenuArt>,
    mut images: ResMut<Assets<Image>>,
    mut rows: Local<Rows>,
    font: Res<UiFont>,
    window: Single<&Window, With<PrimaryWindow>>,
    panel: Query<(&ComputedNode, &UiGlobalTransform), With<MenuPanel>>,
    root: Single<Entity, With<StageRoot>>,
    mut visitors: Query<(Entity, &mut Visitor)>,
    mut sounds: MessageWriter<Sound>,
    mut world: ResMut<MenuWorld>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    // The panel is rebuilt now and then: wait a frame for its layout.
    let Some(geo) = geometry(&window, &panel) else {
        return;
    };
    let s = geo.s;
    for (entity, mut v) in &mut visitors {
        let god = v.god;
        let mut row_of = |sheet: Sheet, row: usize| -> Option<Row> {
            let handle = match sheet {
                Sheet::Walk => art.walk[god.index()].as_ref()?,
                Sheet::Act => art.acts[god.index()].as_ref()?,
            };
            let columns = match sheet {
                Sheet::Walk => WALK_COLUMNS,
                Sheet::Act => ACT_COLUMNS,
            };
            *rows
                .entry((handle.id(), row))
                .or_insert_with(|| images.get(handle).and_then(|i| read_row(i, row, columns)))
        };
        v.t += dt;
        let Some(step) = v.plan.front().copied() else {
            commands.entity(entity).despawn();
            visits.next_at = now + BETWEEN.0 + (BETWEEN.1 - BETWEEN.0) * visits.roll();
            continue;
        };
        let done = match step {
            Step::Walk { to: mark } => {
                let to = geo.at(mark, v.heading);
                v.feet.y = to.y;
                let to = to.x;
                let east = to > v.feet.x;
                v.east = east;
                let row = if east { ROW_WALK_E } else { ROW_WALK_W };
                let frames = row_of(Sheet::Walk, row).map_or(1, |r| r.frames);
                let frame = (v.t * WALK_FPS) as usize % frames;
                if frame != v.pose.2 && (frame == 0 || frame == 3) {
                    sounds.write(Sound::new("step").at(0.5));
                }
                v.pose = (Sheet::Walk, row, frame);
                let stride = WALK_SPEED * s * dt;
                if (to - v.feet.x).abs() <= stride {
                    v.feet.x = to;
                    v.ground = mark;
                    true
                } else {
                    v.feet.x += stride * if east { 1.0 } else { -1.0 };
                    false
                }
            }
            Step::Play {
                row,
                from,
                to,
                hold,
            } => {
                v.feet.y = geo.at(v.ground, v.heading).y;
                let frames = row_of(Sheet::Act, row).map_or(1, |r| r.frames);
                let (from, to) = (from.min(frames), to.min(frames));
                let span = from.abs_diff(to).max(1);
                let k = ((v.t * ACT_FPS) as usize).min(span - 1);
                let frame = if to >= from { from + k } else { from - k };
                v.pose = (Sheet::Act, row, frame.min(frames - 1));
                v.t >= span as f32 / ACT_FPS + hold
            }
            Step::Idle { secs } => {
                v.feet.y = geo.at(v.ground, v.heading).y;
                let frames = row_of(Sheet::Walk, ROW_IDLE).map_or(1, |r| r.frames);
                v.pose = (Sheet::Walk, ROW_IDLE, (v.t * 4.0) as usize % frames);
                v.t >= secs
            }
            Step::Leap { to: mark, air, secs } => {
                if v.t <= dt {
                    v.from = v.feet;
                }
                let to = geo.at(mark, v.heading);
                let frames = row_of(Sheet::Act, ROW_ACT).map_or(1, |r| r.frames);
                let air = air.min(frames - 1);
                let before = air as f32 / ACT_FPS;
                let after = (frames - air - 1) as f32 / ACT_FPS;
                let frame = if v.t < before {
                    (v.t * ACT_FPS) as usize
                } else if v.t < before + secs {
                    let u = (v.t - before) / secs;
                    let from = v.from;
                    let lift = 30.0 * s + (from.y - to.y).abs() * 0.4;
                    v.feet = from.lerp(to, u) - Vec2::Y * 4.0 * lift * u * (1.0 - u);
                    air
                } else {
                    v.feet = to;
                    v.ground = mark;
                    (air + 1 + ((v.t - before - secs) * ACT_FPS) as usize).min(frames - 1)
                };
                v.pose = (Sheet::Act, ROW_ACT, frame);
                v.t >= before + secs + after
            }
            Step::Cue(cue) => {
                let facing = if v.east { 1.0 } else { -1.0 };
                cue_effect(
                    &mut commands,
                    &art,
                    &mut images,
                    &font,
                    *root,
                    cue,
                    god,
                    v.feet,
                    facing,
                    s,
                    now,
                    &mut visits,
                    entity,
                    &mut sounds,
                    &mut world,
                    geo.w,
                );
                true
            }
        };
        if done {
            v.plan.pop_front();
            v.t = 0.0;
        }
    }
}

/// A prop or a flash with a life of its own: it fades in, holds and fades
/// out, and may rise out of the ground (and sink back) inside its clip.
#[derive(Component)]
struct Effect {
    born: f32,
    life: f32,
    fade_in: f32,
    fade_out: f32,
    /// Alpha at full strength.
    peak: f32,
    /// Seconds to rise out of the ground and to sink back; the image is the
    /// only child of a clipping node `depth` pixels tall.
    rise: Option<(f32, f32)>,
    /// Images to flicker between, a new one every `FLICKER` seconds.
    flicker: Vec<Handle<Image>>,
    /// With a step in seconds, `flicker` is a sequence shown once, each
    /// image for that long, the last held: a tree growing.
    grow: f32,
}

const FLICKER: f32 = 0.07;
/// Flowers in `props/menu/flower-N.png`.
const FLOWERS: usize = 6;

/// Words over a champion's head, following it.
#[derive(Component)]
struct Bubble {
    of: Entity,
    until: f32,
}

#[allow(clippy::too_many_arguments)]
fn cue_effect(
    commands: &mut Commands,
    art: &MenuArt,
    images: &mut Assets<Image>,
    font: &UiFont,
    root: Entity,
    cue: Cue,
    god: God,
    feet: Vec2,
    facing: f32,
    s: f32,
    now: f32,
    visits: &mut Visits,
    visitor: Entity,
    sounds: &mut MessageWriter<Sound>,
    world: &mut MenuWorld,
    w: f32,
) {
    let pick = |visits: &mut Visits| (visits.roll() * 3.0) as usize % 3;
    let prop = 64.0 * s;
    match cue {
        Cue::Hello | Cue::Saying(_) => {
            let text = match cue {
                Cue::Saying(n) => saying(n),
                _ => hello(god),
            };
            visits.said += 1;
            let bubble = commands
                .spawn((
                    Bubble {
                        of: visitor,
                        until: now + reading_secs(text),
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        padding: UiRect::axes(px(12.0), px(8.0)),
                        max_width: px(420.0),
                        ..default()
                    },
                    Frame::Tip,
                    Accent(god_color(god)),
                    Pickable::IGNORE,
                    ZIndex(4),
                ))
                .with_child((
                    Text::new(""),
                    Speech {
                        key: 0x6d65_6e75_0000 + visits.said,
                        god,
                        text: text.to_string(),
                        from: 0,
                    },
                    font.text(15.0),
                    TextColor(INK),
                ))
                .id();
            commands.entity(root).add_child(bubble);
        }
        Cue::Bolt => {
            // From the raised hand up off the top of the window.
            let hand = Vec2::new(feet.x + 11.0 * s, feet.y - 60.0 * s);
            let tall = (hand.y / s).ceil().max(8.0) as u32;
            let seed = visits.count;
            let frames: Vec<Handle<Image>> = (0..3)
                .map(|k| images.add(bolt(tall, seed * 3 + k)))
                .collect();
            let width = BOLT_W as f32 * s;
            let bolt = commands
                .spawn((
                    Effect {
                        born: now,
                        life: 0.75,
                        fade_in: 0.0,
                        fade_out: 0.25,
                        peak: 1.0,
                        rise: None,
                        flicker: frames.clone(),
                        grow: 0.0,
                    },
                    ImageNode::new(frames[0].clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px((hand.x - width / 2.0).round()),
                        top: px(hand.y - tall as f32 * s),
                        width: px(width),
                        height: px(tall as f32 * s),
                        ..default()
                    },
                    Pickable::IGNORE,
                    ZIndex(3),
                ))
                .id();
            let flash = commands
                .spawn((
                    Effect {
                        born: now,
                        life: 0.35,
                        fade_in: 0.0,
                        fade_out: 0.3,
                        peak: 0.22,
                        rise: None,
                        flicker: Vec::new(),
                        grow: 0.0,
                    },
                    // A plain fill: an image node would keep its image's
                    // square shape and light only the middle of the window.
                    BackgroundColor(Color::srgba(0.85, 0.92, 1.0, 0.0)),
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100.0),
                        height: percent(100.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                    ZIndex(1),
                ))
                .id();
            commands.entity(root).add_children(&[flash, bolt]);
            sounds.write(Sound::new("ward-break"));
            // The sky answers: the sun, the moon, or a cloud where it struck.
            let r = visits.roll();
            if !world.has(Kind::Sun) && r < 0.3 {
                world.add(Kind::Sun, 0, 0.09, 0.05);
            } else if !world.has(Kind::Moon) && r < 0.55 {
                world.add(Kind::Moon, 0, 0.91, 0.06);
            } else {
                let v = (visits.roll() * 3.0) as usize;
                world.add(Kind::Cloud, v, hand.x / w, 0.03 + 0.22 * visits.roll());
            }
        }
        Cue::Stone => {
            // Beside the champion, towards the middle of the window.
            let x = feet.x + 44.0 * s * facing;
            let depth = prop;
            let clip = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px((x - prop / 2.0).round()),
                        top: px(feet.y - depth + 3.0 * s),
                        width: px(prop),
                        height: px(depth),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    Pickable::IGNORE,
                    ZIndex(1),
                ))
                .id();
            let stone = commands
                .spawn((
                    Effect {
                        born: now,
                        life: 9.0,
                        fade_in: 0.0,
                        fade_out: 0.0,
                        peak: 1.0,
                        rise: Some((0.9, 0.8)),
                        flicker: Vec::new(),
                        grow: 0.0,
                    },
                    ImageNode::new(art.stones[pick(visits)].clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        top: px(depth),
                        width: px(prop),
                        height: px(prop),
                        ..default()
                    },
                ))
                .id();
            commands.entity(clip).add_child(stone);
            let dust = puff(commands, &art.dust, Vec2::new(x, feet.y), s, now, 1.1);
            commands.entity(root).add_children(&[clip, dust]);
            sounds.write(Sound::new("terrain"));
            // The earth first; then mountains on the horizon, or rocks here.
            if !world.has(Kind::Ground) {
                world.add(Kind::Ground, 0, 0.5, 0.0);
            } else if visits.roll() < 0.7 {
                let v = (visits.roll() * Kind::Mountain.variants() as f32) as usize;
                world.add(Kind::Mountain, v, visits.roll(), 0.0);
            } else {
                world.add(Kind::Rock, 0, x / w, 0.0);
            }
        }
        Cue::Feast => {
            // On the ground beside her, where she was heading: left for the player.
            let cloth = 48.0 * s;
            let feast = commands
                .spawn((
                    Effect {
                        born: now,
                        life: 10.0,
                        fade_in: 0.35,
                        fade_out: 1.0,
                        peak: 1.0,
                        rise: None,
                        flicker: Vec::new(),
                        grow: 0.0,
                    },
                    ImageNode::new(art.feasts[pick(visits)].clone())
                        .with_color(Color::WHITE.with_alpha(0.0)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px((feet.x + 46.0 * s * facing - cloth / 2.0).round()),
                        top: px((feet.y + 4.0 * s - cloth).round()),
                        width: px(cloth),
                        height: px(cloth),
                        ..default()
                    },
                    Pickable::IGNORE,
                    ZIndex(3),
                ))
                .id();
            commands.entity(root).add_child(feast);
            sounds.write(Sound::new("offer"));
            // Someone comes to the feast, and stays about.
            let v = (visits.roll() * 12.0) as usize;
            // Beside the cloth, not under it.
            world.add(Kind::Animal, v, (feet.x + 84.0 * s * facing) / w, 0.0);
        }
        Cue::Grove => {
            let x = feet.x + 44.0 * s * facing;
            // The tree stays in the menu's world, growing there from a
            // sprout; a bush springs up somewhere too.
            world.add(Kind::Tree, 0, x / w, 0.0);
            let v = (visits.roll() * 4.0) as usize;
            world.add(Kind::Bush, v, visits.roll(), 0.0);
            // Flowers spring up one after another around him and the tree.
            let small = 30.0 * s;
            let n = art.flowers.len();
            for k in 0..5.min(n) {
                let dx = (visits.roll() - 0.5) * 150.0 * s + (x - feet.x) * 0.5;
                let flower = commands
                    .spawn((
                        Effect {
                            born: now + 0.3 + 0.18 * k as f32,
                            life: 10.0 - 0.18 * k as f32,
                            fade_in: 0.25,
                            fade_out: 1.2,
                            peak: 1.0,
                            rise: None,
                            flicker: Vec::new(),
                            grow: 0.0,
                        },
                        ImageNode::new(art.flowers[(visits.roll() * n as f32) as usize % n].clone())
                            .with_color(Color::WHITE.with_alpha(0.0)),
                        Node {
                            position_type: PositionType::Absolute,
                            left: px((feet.x + dx - small / 2.0).round()),
                            top: px((feet.y + (2.0 + 6.0 * visits.roll()) * s - small).round()),
                            width: px(small),
                            height: px(small),
                            ..default()
                        },
                        Pickable::IGNORE,
                        ZIndex(3),
                    ))
                    .id();
                commands.entity(root).add_child(flower);
            }
            sounds.write(Sound::new("grove"));
        }
        Cue::Summon => {
            // A spirit and a handful of fireflies rise from her and stay;
            // now and then a glowing cap comes up at her feet.
            let at = feet.x / w;
            let v = (visits.roll() * 4.0) as usize;
            world.add(Kind::Spirit, v, at, 0.1 + 0.5 * visits.roll());
            for _ in 0..3 {
                let x = at + (visits.roll() - 0.5) * 0.2;
                world.add(Kind::Firefly, 0, x, visits.roll());
            }
            if visits.roll() < 0.35 {
                world.add(Kind::Glowcap, 0, at + (visits.roll() - 0.5) * 0.1, 0.0);
            }
            let splash = puff(commands, &art.splash, feet - Vec2::Y * 30.0 * s, s, now, 0.9);
            commands.entity(root).add_child(splash);
            sounds.write(Sound::new("spirit"));
        }
        Cue::Splash => {
            let splash = puff(commands, &art.splash, feet, s, now, 0.6);
            commands.entity(root).add_child(splash);
            sounds.write(Sound::new("blink").at(0.7));
        }
    }
}

/// A puff at a point on the ground that fades.
fn puff(commands: &mut Commands, image: &Handle<Image>, at: Vec2, s: f32, now: f32, life: f32) -> Entity {
    let size = 64.0 * s * 0.75;
    commands
        .spawn((
            Effect {
                born: now,
                life,
                fade_in: 0.0,
                fade_out: life * 0.6,
                peak: 1.0,
                rise: None,
                flicker: Vec::new(),
                grow: 0.0,
            },
            ImageNode::new(image.clone()),
            Node {
                position_type: PositionType::Absolute,
                left: px((at.x - size / 2.0).round()),
                top: px((at.y - size * 0.8).round()),
                width: px(size),
                height: px(size),
                ..default()
            },
            Pickable::IGNORE,
            ZIndex(3),
        ))
        .id()
}

const BOLT_W: u32 = 24;

/// A jagged bolt `tall` texels high in a `BOLT_W`-wide image: a white core
/// with a blue glow, a fork or two, drawn from the bottom (the hand) up.
fn bolt(tall: u32, seed: u64) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let (w, h) = (BOLT_W, tall);
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
    let mut rng = Visits {
        next_at: 0.0,
        count: 0,
        seed: seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xB017,
        forced: None,
        said: 0,
        last_saying: None,
    };
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    let mut put = |x: i32, y: i32, c: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            if data[i + 3] < c[3] || c[3] == 255 {
                data[i..i + 4].copy_from_slice(&c);
            }
        }
    };
    const GLOW: [u8; 4] = [90, 160, 255, 150];
    const EDGE: [u8; 4] = [170, 215, 255, 255];
    const CORE: [u8; 4] = [255, 255, 255, 255];
    let mut stroke = |rng: &mut Visits, mut x: f32, from: i32, to: i32, thick: bool| {
        let mut y = from;
        while y > to {
            let run = 3 + (rng.roll() * 7.0) as i32;
            let dx = (rng.roll() - 0.5) * 7.0;
            for k in 0..run {
                let xi = (x + dx * k as f32 / run as f32).round() as i32;
                let yy = y - k;
                for gx in -2..=2 {
                    put(xi + gx, yy, GLOW);
                }
                put(xi - 1, yy, EDGE);
                put(xi + 1, yy, EDGE);
                put(xi, yy, CORE);
                if thick {
                    put(xi + 1, yy, CORE);
                }
            }
            x = (x + dx).clamp(4.0, w as f32 - 5.0);
            y -= run;
        }
        x
    };
    let mid = w as f32 / 2.0;
    stroke(&mut rng, mid, h as i32 - 1, 0, true);
    // A fork or two off the main stroke, short.
    for _ in 0..2 {
        let at = (h as f32 * (0.25 + 0.5 * rng.roll())) as i32;
        let x = mid + (rng.roll() - 0.5) * 8.0;
        let end = at - 10 - (rng.roll() * 14.0) as i32;
        stroke(&mut rng, x, at, end, false);
    }
    image
}

fn god_color(god: God) -> Color {
    let [r, g, b] = god.accent();
    Color::srgb_u8(r, g, b)
}

/// Shows each champion's pose where its feet are.
fn place(
    art: Res<MenuArt>,
    images: Res<Assets<Image>>,
    mut feet_rows: Local<HashMap<(AssetId<Image>, usize), usize>>,
    window: Single<&Window, With<PrimaryWindow>>,
    world: Res<MenuWorld>,
    mut visitors: Query<(&Visitor, &mut ImageNode, &mut Node)>,
) {
    let s = scale(window.height());
    let size = CELL as f32 * s;
    // Night in the menu's world dims them a little too.
    let tint = crate::menu_world::champion_tint(&world);
    for (v, mut image, mut node) in &mut visitors {
        let (sheet, row, frame) = v.pose;
        let (handle, layout, columns) = match sheet {
            Sheet::Walk => (&art.walk[v.god.index()], &art.walk_layout, WALK_COLUMNS),
            Sheet::Act => (&art.acts[v.god.index()], &art.act_layout, ACT_COLUMNS),
        };
        let Some(handle) = handle else { continue };
        let feet = *feet_rows.entry((handle.id(), row)).or_insert_with(|| {
            images
                .get(handle)
                .and_then(|i| read_row(i, row, columns))
                .map_or(CELL as usize - 16, |r| r.feet)
        });
        if image.image != *handle {
            image.image = handle.clone();
        }
        let atlas = Some(TextureAtlas {
            layout: layout.clone(),
            index: row * columns as usize + frame,
        });
        if image.texture_atlas != atlas {
            image.texture_atlas = atlas;
        }
        if image.color != tint {
            image.color = tint;
        }
        let left = px((v.feet.x - size / 2.0).round());
        let top = px((v.feet.y - (feet as f32 + 1.0) * s).round());
        if node.left != left || node.top != top || node.width != px(size) {
            node.left = left;
            node.top = top;
            node.width = px(size);
            node.height = px(size);
        }
    }
}

#[allow(clippy::type_complexity)]
fn effects(
    mut commands: Commands,
    time: Res<Time>,
    mut effects: Query<(
        Entity,
        &Effect,
        Option<&mut ImageNode>,
        Option<&mut BackgroundColor>,
        &mut Node,
        Option<&ChildOf>,
    )>,
) {
    let now = time.elapsed_secs();
    for (entity, effect, image, fill, mut node, parent) in &mut effects {
        let age = now - effect.born;
        if age >= effect.life {
            // A rising prop goes with its clip.
            match (effect.rise, parent) {
                (Some(_), Some(parent)) => commands.entity(parent.parent()).despawn(),
                _ => commands.entity(entity).despawn(),
            }
            continue;
        }
        let mut alpha = 1.0f32;
        if effect.fade_in > 0.0 {
            alpha = alpha.min(age / effect.fade_in);
        }
        if effect.fade_out > 0.0 {
            alpha = alpha.min((effect.life - age) / effect.fade_out);
        }
        // A negative age is an effect that has not begun yet.
        let want = alpha.clamp(0.0, 1.0) * effect.peak;
        // Every node has a (transparent) background: fade it only where
        // there is no image, or the image gets a black box.
        let Some(mut image) = image else {
            if let Some(mut fill) = fill
                && (fill.0.alpha() - want).abs() > 0.004
            {
                fill.0.set_alpha(want);
            }
            continue;
        };
        if (image.color.alpha() - want).abs() > 0.004 {
            image.color.set_alpha(want);
        }
        if !effect.flicker.is_empty() {
            let n = effect.flicker.len();
            let k = if effect.grow > 0.0 {
                ((age.max(0.0) / effect.grow) as usize).min(n - 1)
            } else {
                (age.max(0.0) / FLICKER) as usize % n
            };
            if image.image != effect.flicker[k] {
                image.image = effect.flicker[k].clone();
            }
        }
        if let Some((rise, sink)) = effect.rise {
            let Val::Px(size) = node.height else { continue };
            let up = if age < rise {
                1.0 - (1.0 - age / rise).powi(2)
            } else if age > effect.life - sink {
                ((effect.life - age) / sink).max(0.0)
            } else {
                1.0
            };
            let top = px(((1.0 - up) * size).round());
            if node.top != top {
                node.top = top;
            }
        }
    }
}

fn bubbles(
    mut commands: Commands,
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    visitors: Query<&Visitor>,
    mut bubbles: Query<(Entity, &Bubble, &mut Node, &ComputedNode)>,
) {
    let now = time.elapsed_secs();
    let s = scale(window.height());
    for (entity, bubble, mut node, computed) in &mut bubbles {
        let Ok(v) = visitors.get(bubble.of) else {
            commands.entity(entity).despawn();
            continue;
        };
        if now >= bubble.until {
            commands.entity(entity).despawn();
            continue;
        }
        let size = computed.size() * computed.inverse_scale_factor();
        let x = (v.feet.x - size.x / 2.0).clamp(8.0, window.width() - size.x - 8.0);
        let left = px(x.round());
        let top = px((v.feet.y - 64.0 * s - size.y).round().max(8.0));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
    }
}
