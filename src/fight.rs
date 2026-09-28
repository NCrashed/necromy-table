//! The fight on the battle panel's stage (docs/design.md §12): the two
//! sides face each other in profile, and once the dice are down every hit
//! that came up is dealt as a blow. The hitter swings, the other side
//! reels (a red flash, a number, the panel shakes) or blocks with a shield;
//! a side that fell dies at the end and stays down.
//!
//! Sheets come from PixelLab (`assets/sprites/<god>-fight.png`, see
//! `scripts/pixellab-fight.sh`): 96 px cells, rows attack A, attack B, hurt
//! A, hurt B, death, all facing east; the right-hand side is mirrored. The
//! frames in a row are counted from the image. A champion without a sheet,
//! and the royal guard, stand still and only lunge and recoil.
//!
//! Everything is a function of the time since the dice settled, so the
//! panel may rebuild the stage at any moment.

use std::collections::HashMap;

use bevy::prelude::*;
use necromy_rules::{God, PlayerId};

use crate::dice::DiceShow;
use crate::hud::UiFont;
use crate::play::{BattleInfo, Match};
use crate::stats::StatArt;

const CELL: u32 = 96;
const FIGHT_COLUMNS: u32 = 10;
const FIGHT_ROWS: u32 = 5;
const ROW_ATTACK: usize = 0;
const ROW_HURT: usize = 2;
const ROW_DEATH: usize = 4;
/// The idle row facing east in a champion sheet (`token.rs`).
const IDLE_EAST_ROW: usize = 1;
const IDLE_COLUMNS: u32 = 6;
const IDLE_FRAMES: usize = 4;
const IDLE_FRAME_SECS: f32 = 0.16;

/// Frames per second of the fight animations.
const FPS: f32 = 14.0;
/// A breath after the dice settle, before the first blow.
const LEAD_IN: f32 = 0.35;
/// From one blow to the next.
const BLOW_SECS: f32 = 0.72;
/// When a swing lands, from its start.
const IMPACT: f32 = 0.34;
/// A fallen side's death, then a moment to take in the result.
const DEATH_SECS: f32 = 1.1;
const TAIL_SECS: f32 = 0.9;
/// How far a swing carries the hitter forward, a blow throws the other
/// back, and the panel shakes, in pixels.
const LUNGE: f32 = 26.0;
const KNOCK: f32 = 12.0;
const SHAKE: f32 = 5.0;
const SHAKE_SECS: f32 = 0.22;
/// The stage and a fighter's size on it: frames at 2×.
pub const STAGE_W: f32 = 300.0;
pub const STAGE_H: f32 = 176.0;
const FIGHTER: f32 = 192.0;
const POPUP_SECS: f32 = 0.9;

pub struct FightPlugin;

impl Plugin for FightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fight>()
            .init_resource::<Wounds>()
            .add_systems(Startup, load_art)
            .add_systems(
                crate::InGame,
                (direct, pose, shake, popups).chain(),
            );
    }
}

/// One hit that came up on a die: who deals it, and whether it gets
/// through the other side's shields.
#[derive(Clone, Copy)]
struct Blow {
    by: usize,
    lands: bool,
}

/// Blows in the order they are dealt: the attacker's, then the defender's
/// answer; the shielded ones of each side first, the real harm last. A side
/// that falls strikes first, so nobody swings after their death.
fn blows(battle: &BattleInfo) -> Vec<Blow> {
    let Some((a, d)) = battle.scores else {
        return Vec::new();
    };
    let mut sides = [(0, a, d), (1, d, a)];
    if battle.fell[1] && !battle.fell[0] {
        sides.swap(0, 1);
    }
    let mut out = Vec::new();
    for (by, mine, theirs) in sides {
        let blocked = mine.hits.min(theirs.shields);
        for i in 0..mine.hits {
            out.push(Blow {
                by,
                lands: i >= blocked,
            });
        }
    }
    out
}

/// How long the fight takes on screen from the moment the dice settle;
/// the dice show keeps the panel up at least this long.
pub fn show_secs(battle: &BattleInfo) -> f32 {
    if battle.scores.is_none() {
        return 0.0;
    }
    let death = if battle.fell.iter().any(|&f| f) {
        DEATH_SECS
    } else {
        0.0
    };
    LEAD_IN + blows(battle).len() as f32 * BLOW_SECS + death + TAIL_SECS
}

/// What the panel shows so far: harm taken per side, and who lies dead.
#[derive(Resource, Default, PartialEq)]
pub struct Wounds {
    pub taken: [u8; 2],
    pub dead: [bool; 2],
}

#[derive(Resource, Default)]
struct Fight {
    /// When the dice of the current battle settled.
    start: Option<f32>,
    /// Blows whose impact has been shown (popups, shake).
    shown: usize,
    /// When the last blow landed, for the shake.
    shook_at: Option<f32>,
}

#[derive(Resource)]
pub struct FightArt {
    /// A soft oval under a fighter's feet.
    shadow: Handle<Image>,
    /// Per god: the champion sheet (idle) and the fight sheet, if any.
    idle: [Option<Handle<Image>>; 5],
    fight: [Option<Handle<Image>>; 5],
    idle_layout: Handle<TextureAtlasLayout>,
    fight_layout: Handle<TextureAtlasLayout>,
}

fn load_art(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut images: ResMut<Assets<Image>>,
) {
    let load = |kind: &str| {
        God::ALL.map(|god| {
            let path = format!("sprites/{}-{kind}.png", god.name().to_lowercase());
            std::path::Path::new("assets")
                .join(&path)
                .exists()
                .then(|| assets.load(path))
        })
    };
    commands.insert_resource(FightArt {
        shadow: images.add(shadow()),
        idle: load("champion"),
        fight: load("fight"),
        idle_layout: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(CELL),
            IDLE_COLUMNS,
            8,
            None,
            None,
        )),
        fight_layout: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(CELL),
            FIGHT_COLUMNS,
            FIGHT_ROWS,
            None,
            None,
        )),
    });
}

/// A 24×6 oval of translucent ink, darker in the middle.
fn shadow() -> Image {
    let (w, h) = (24u32, 6u32);
    let mut image = Image::new_fill(
        bevy::render::render_resource::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        &[0, 0, 0, 0],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    let data = image.data.as_mut().expect("new_fill allocates pixel data");
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5 - w as f32 / 2.0) / (w as f32 / 2.0);
            let dy = (y as f32 + 0.5 - h as f32 / 2.0) / (h as f32 / 2.0);
            let r = dx * dx + dy * dy;
            if r <= 1.0 {
                let a = if r < 0.45 { 150 } else { 90 };
                let i = ((y * w + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&[10, 8, 14, a]);
            }
        }
    }
    image
}

/// Frames in a row of a sheet: cells from the left until an empty one.
fn frames_in_row(image: &Image, row: usize) -> usize {
    let Some(data) = image.data.as_ref() else {
        return 0;
    };
    let width = image.width() as usize;
    let filled = |col: usize| {
        (0..CELL as usize).any(|y| {
            (0..CELL as usize).any(|x| {
                let (px, py) = (col * CELL as usize + x, row * CELL as usize + y);
                data.get((py * width + px) * 4 + 3).is_some_and(|&a| a > 0)
            })
        })
    };
    (0..FIGHT_COLUMNS as usize)
        .take_while(|&c| filled(c))
        .count()
}

/// A fighter on the stage; 0 stands left facing right, 1 the other way.
#[derive(Component)]
pub struct Fighter(pub usize);

/// Where the stage shakes from.
#[derive(Component)]
pub struct Stage;

/// The stage with its two fighters, for the battle panel.
pub fn stage(commands: &mut Commands, art: &FightArt) -> Entity {
    let stage = commands
        .spawn((
            Stage,
            Node {
                width: px(STAGE_W),
                height: px(STAGE_H),
                ..default()
            },
        ))
        .id();
    for side in 0..2 {
        let left = if side == 0 {
            0.0
        } else {
            STAGE_W - FIGHTER
        };
        // Feet are about 80 px down a 96 px frame.
        let feet = STAGE_H - FIGHTER * 0.86 + FIGHTER * 80.0 / 96.0;
        let shadow = commands
            .spawn((
                ImageNode::new(art.shadow.clone()),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(left + FIGHTER / 2.0 - 24.0),
                    top: px(feet - 6.0),
                    width: px(48.0),
                    height: px(12.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(stage).add_child(shadow);
        let fighter = commands
            .spawn((
                Fighter(side),
                ImageNode::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(left),
                    // The feet (about 80 px down a 96 px frame) near the bottom.
                    top: px(STAGE_H - FIGHTER * 0.86),
                    width: px(FIGHTER),
                    height: px(FIGHTER),
                    ..default()
                },
            ))
            .id();
        commands.entity(stage).add_child(fighter);
    }
    stage
}

/// What a side is doing at a moment, and for how long it has been at it.
#[derive(Clone, Copy)]
enum Act {
    Idle,
    Attack { variant: usize, t: f32 },
    Hurt { variant: usize, t: f32 },
    Block { t: f32 },
    Death { t: f32 },
}

fn act(side: usize, t: f32, blows: &[Blow], fell: bool) -> Act {
    if t < 0.0 {
        return Act::Idle;
    }
    let k = (t / BLOW_SECS) as usize;
    if let Some(blow) = blows.get(k) {
        let bt = t - k as f32 * BLOW_SECS;
        // Alternate the swings of one side, and the ways the other reels.
        let variant = blows[..k].iter().filter(|b| b.by == blow.by).count() % 2;
        return if blow.by == side {
            Act::Attack { variant, t: bt }
        } else if bt < IMPACT {
            Act::Idle
        } else if blow.lands {
            Act::Hurt {
                variant,
                t: bt - IMPACT,
            }
        } else {
            Act::Block { t: bt - IMPACT }
        };
    }
    if fell {
        Act::Death {
            t: t - blows.len() as f32 * BLOW_SECS,
        }
    } else {
        Act::Idle
    }
}

/// Starts the fight when the dice are down, and counts the harm shown.
fn direct(
    time: Res<Time>,
    game: Res<Match>,
    dice: Res<DiceShow>,
    mut fight: ResMut<Fight>,
    mut wounds: ResMut<Wounds>,
) {
    let now = time.elapsed_secs();
    let Some(battle) = game.battle.as_ref() else {
        if fight.start.is_some() {
            *fight = Fight::default();
        }
        wounds.set_if_neq(Wounds::default());
        return;
    };
    if battle.scores.is_none() || !dice.landed() {
        return;
    }
    let start = *fight.start.get_or_insert(now);
    let t = now - start - LEAD_IN;
    let blows = blows(battle);
    let mut next = Wounds::default();
    for (k, blow) in blows.iter().enumerate() {
        if blow.lands && t >= k as f32 * BLOW_SECS + IMPACT {
            let taken = &mut next.taken[1 - blow.by];
            *taken = taken.saturating_add(1);
        }
    }
    let over = t >= blows.len() as f32 * BLOW_SECS;
    next.dead = battle.fell.map(|f| f && over);
    wounds.set_if_neq(next);
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn pose(
    time: Res<Time>,
    game: Res<Match>,
    fight: Res<Fight>,
    art: Res<FightArt>,
    stats: Res<StatArt>,
    images: Res<Assets<Image>>,
    mut fighters: Query<(&Fighter, &mut ImageNode, &mut UiTransform)>,
    mut counts: Local<HashMap<AssetId<Image>, [usize; FIGHT_ROWS as usize]>>,
) {
    let Some(battle) = game.battle.as_ref() else {
        return;
    };
    let now = time.elapsed_secs();
    let t = fight.start.map_or(-1.0, |s| now - s - LEAD_IN);
    let blows = blows(battle);
    for (fighter, mut image, mut transform) in &mut fighters {
        let side = fighter.0;
        let who = if side == 0 {
            battle.attacker
        } else {
            Some(battle.defender)
        };
        let god = who.and_then(|p: PlayerId| game.game.champion(p)).map(|c| c.god);
        let act = act(side, t, &blows, battle.fell[side]);
        let facing = if side == 0 { 1.0 } else { -1.0 };

        // The frame: from the fight sheet when there is one, else idle.
        let fight_sheet = god
            .and_then(|g| art.fight[g.index()].clone())
            .filter(|h| images.contains(h));
        let rows = fight_sheet.as_ref().and_then(|h| {
            let sheet = images.get(h)?;
            Some(
                *counts
                    .entry(h.id())
                    .or_insert_with(|| std::array::from_fn(|row| frames_in_row(sheet, row))),
            )
        });
        let row_frame = |row: usize, t: f32, hold: bool| -> Option<(usize, usize)> {
            let n = rows?[row];
            if n == 0 {
                return None;
            }
            let f = (t * FPS) as usize;
            (hold || f < n).then_some((row, f.min(n - 1)))
        };
        let fight_frame = match act {
            Act::Attack { variant, t } => row_frame(ROW_ATTACK + variant, t, false),
            Act::Hurt { variant, t } => row_frame(ROW_HURT + variant, t, false),
            Act::Death { t } => row_frame(ROW_DEATH, t, true),
            Act::Idle | Act::Block { .. } => None,
        };
        let idle_sheet = god.and_then(|g| art.idle[g.index()].clone());
        let (new_image, atlas) = match (fight_frame, &fight_sheet, &idle_sheet) {
            (Some((row, f)), Some(sheet), _) => (
                sheet.clone(),
                Some(TextureAtlas {
                    layout: art.fight_layout.clone(),
                    index: row * FIGHT_COLUMNS as usize + f,
                }),
            ),
            (_, _, Some(sheet)) => {
                let f = (now / IDLE_FRAME_SECS) as usize % IDLE_FRAMES;
                (
                    sheet.clone(),
                    Some(TextureAtlas {
                        layout: art.idle_layout.clone(),
                        index: IDLE_EAST_ROW * IDLE_COLUMNS as usize + f,
                    }),
                )
            }
            // The royal guard, or a god without a sheet.
            _ => (stats.guard.clone(), None),
        };
        if image.image != new_image {
            image.image = new_image;
        }
        if image.texture_atlas != atlas {
            image.texture_atlas = atlas;
        }
        image.flip_x = side == 1;

        // Movement and colour on top of the frames: a lunge into a swing,
        // a recoil and a red flash from a blow, a blue glint off a shield.
        let (dx, color) = match act {
            Act::Attack { t, .. } => {
                let k = (t / IMPACT).min(1.0);
                let out = if t < IMPACT {
                    k * k
                } else {
                    (1.0 - (t - IMPACT) / (BLOW_SECS - IMPACT)).max(0.0)
                };
                (facing * LUNGE * out, Color::WHITE)
            }
            Act::Hurt { t, .. } => {
                let k = (1.0 - t / 0.3).max(0.0);
                let flash = if t < 0.12 {
                    Color::srgb(1.0, 0.35, 0.3)
                } else {
                    Color::WHITE
                };
                (-facing * KNOCK * k, flash)
            }
            Act::Block { t } => {
                let k = (1.0 - t / 0.2).max(0.0);
                let glint = if t < 0.12 {
                    Color::srgb(0.6, 0.8, 1.0)
                } else {
                    Color::WHITE
                };
                (-facing * KNOCK * 0.4 * k, glint)
            }
            // Without a death animation the fallen side sinks and fades.
            Act::Death { t } if fight_frame.is_none() => {
                let k = (t / 0.8).min(1.0);
                (0.0, Color::srgba(0.6, 0.6, 0.6, 1.0 - 0.6 * k))
            }
            _ => (0.0, Color::WHITE),
        };
        let translation = Val2::px(dx, 0.0);
        if transform.translation != translation {
            transform.translation = translation;
        }
        if image.color != color {
            image.color = color;
        }
    }
}

/// Each impact once: a number over the one hit (or "блок" off a shield),
/// and the stage jolts when a blow gets through.
fn shake(
    time: Res<Time>,
    game: Res<Match>,
    mut fight: ResMut<Fight>,
    mut stage: Query<&mut UiTransform, With<Stage>>,
    mut commands: Commands,
    font: Res<UiFont>,
    fighters: Query<(&Fighter, &ComputedNode, &UiGlobalTransform)>,
) {
    let now = time.elapsed_secs();
    let (Some(battle), Some(start)) = (game.battle.as_ref(), fight.start) else {
        return;
    };
    if battle.scores.is_none() {
        return;
    }
    let t = now - start - LEAD_IN;
    let blows = blows(battle);
    while fight.shown < blows.len() && t >= fight.shown as f32 * BLOW_SECS + IMPACT {
        let blow = blows[fight.shown];
        fight.shown += 1;
        let target = 1 - blow.by;
        if blow.lands {
            fight.shook_at = Some(now);
        }
        let Some((_, node, at)) = fighters.iter().find(|(f, ..)| f.0 == target) else {
            continue;
        };
        let centre = at.affine().translation * node.inverse_scale_factor();
        let (text, color, size) = if blow.lands {
            ("-1", Color::srgb(1.0, 0.3, 0.25), 30.0)
        } else {
            ("блок", Color::srgb(0.65, 0.82, 1.0), 20.0)
        };
        commands.spawn((
            Popup { born: now },
            Text::new(text),
            font.bold(size),
            TextColor(color),
            TextShadow::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(centre.x - 16.0),
                top: px(centre.y - 70.0),
                ..default()
            },
            GlobalZIndex(12),
        ));
    }
    let offset = match fight.shook_at {
        Some(at) if now - at < SHAKE_SECS => {
            let k = 1.0 - (now - at) / SHAKE_SECS;
            let s = now * 90.0;
            Vec2::new(s.sin(), (s * 1.3).cos()) * SHAKE * k
        }
        _ => Vec2::ZERO,
    };
    for mut transform in &mut stage {
        let v = Val2::px(offset.x, offset.y);
        if transform.translation != v {
            transform.translation = v;
        }
    }
}

/// A number or word over a fighter, rising and fading.
#[derive(Component)]
struct Popup {
    born: f32,
}

fn popups(
    mut commands: Commands,
    time: Res<Time>,
    mut popups: Query<(Entity, &Popup, &mut UiTransform, &mut TextColor)>,
) {
    let now = time.elapsed_secs();
    for (entity, popup, mut transform, mut color) in &mut popups {
        let t = (now - popup.born) / POPUP_SECS;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation = Val2::px(0.0, -40.0 * t);
        transform.scale = Vec2::splat(1.0 + 0.4 * (1.0 - t).powi(3));
        color.0.set_alpha(1.0 - t * t);
    }
}
