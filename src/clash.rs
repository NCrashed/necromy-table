//! Fights on the board between mobs, the militia and the guard
//! (docs/design.md §20.4). The rules settle them without dice, in the world
//! phase; here each one plays out where it happens: the attacker lunges at
//! its victim, a slash and sparks land, the victim shakes, and one that
//! falls stands a moment as a ghost of its figure, then breaks into bones
//! or tufts of fur
//! (a monster from a gate goes out in sparks). Clashes of one batch play one after another, so a
//! trade of blows at a gate reads in order. Looks only: nothing here
//! touches the rules.

use bevy::prelude::*;
use necromy_rules::{Event, Hex, MobKind, PlayerId};

use crate::board::Board;
use crate::effects::{INK, Particle, Scatter, pixels, sprite};
use crate::mobs_ui::{Figures, MilitiaMan, UndeadToken, figure};
use crate::play::Match;
use crate::stats::StatArt;
use crate::token::{GuardToken, Token};

/// Seconds between the starts of two clashes of one batch.
const GAP: f32 = 0.45;
/// From the start of the lunge to the blow landing.
const HIT: f32 = 0.18;
const LUNGE_SECS: f32 = 0.4;
/// How far a lunge reaches towards the victim, in metres.
const REACH: f32 = 0.35;
const SHAKE_SECS: f32 = 0.3;
/// How long a fallen mob stands after the blow before it breaks.
const DYING: f32 = 0.35;
/// Heights above the feet, and the pull towards the camera so the figure
/// does not hide what lands on it.
const CHEST: f32 = 0.55;
const FORWARD: f32 = 0.35;
const GRAVITY: f32 = 5.5;

pub struct ClashPlugin;

impl Plugin for ClashPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clashes>()
            .add_systems(Startup, make_sprites)
            .add_systems(
                crate::InGame,
                (queue_clashes, play_clashes, lunge)
                    .chain()
                    .after(Figures)
                    .after(crate::token::Tokens),
            );
    }
}

/// A figure moved by a clash; the systems that place figures leave it
/// alone meanwhile.
#[derive(Component)]
pub(crate) struct Lunge {
    base: Vec3,
    /// Towards the victim; zero for a shake.
    dir: Vec3,
    start: f32,
}

/// Who takes part in a clash.
#[derive(Clone, Copy, Debug)]
enum Side {
    Mob(u32),
    /// A settlement's militia, by its home.
    Men(Hex),
    Guard,
    Champion(PlayerId),
}

struct Clash {
    attacker: Side,
    from: Hex,
    victim: Side,
    to: Hex,
    /// The victim falls: its kind, and the ghost standing in for its figure.
    dead: Option<(MobKind, Option<Entity>)>,
    start: f32,
    lunged: bool,
    struck: bool,
}

#[derive(Resource, Default)]
struct Clashes {
    playing: Vec<Clash>,
    /// When the next clash may start.
    next: f32,
    scatter: Scatter,
}

#[derive(Resource)]
struct ClashSprites {
    slash: Handle<Image>,
    spark: Handle<Image>,
    bone: Handle<Image>,
    fur: Handle<Image>,
}

#[rustfmt::skip]
fn make_sprites(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let white = [250, 248, 236, 255];
    let pale = [196, 200, 210, 255];
    let slash = images.add(pixels(
        &[
            "...........ww",
            "..........wwl",
            ".........wwl.",
            "........wwl..",
            ".......wwl...",
            "......wwl....",
            ".....wwl.....",
            "....wwl......",
            "...wwl.......",
            "..wwl........",
            ".wwl.........",
            "wwl..........",
            "wl...........",
        ],
        &[(b'w', white), (b'l', pale)],
    ));
    let spark = images.add(pixels(
        &["..y..", ".yyy.", "yywyy", ".yyy.", "..y.."],
        &[(b'y', [250, 210, 110, 255]), (b'w', [255, 255, 240, 255])],
    ));
    let bone = images.add(pixels(
        &["kk..", "kwwk", ".kwk", "..kk"],
        &[(b'k', INK), (b'w', [232, 224, 200, 255])],
    ));
    let fur = images.add(pixels(
        &[".b.", "bBb", ".b."],
        &[(b'b', [112, 80, 52, 255]), (b'B', [164, 122, 78, 255])],
    ));
    commands.insert_resource(ClashSprites { slash, spark, bone, fur });
}

/// Takes the clashes the rules just reported (`Match::clashes`) and lines
/// them up; a mob that falls gets its ghost at once, where its figure was.
fn queue_clashes(
    mut commands: Commands,
    time: Res<Time>,
    mut game: ResMut<Match>,
    mut clashes: ResMut<Clashes>,
    board: Res<Board>,
    art: Res<StatArt>,
    images: Res<Assets<Image>>,
) {
    if game.clashes.is_empty() {
        return;
    }
    let events = std::mem::take(&mut game.bypass_change_detection().clashes);
    let g = &game.game;
    let mob_hex = |id: u32| game.seen_mob(id).map(|m| m.hex);
    // Where a militia stands now, or its home.
    let post = |home: Hex| g.militia_unit(home).and_then(|m| m.at).unwrap_or(home);
    // The militia that struck from `hex`: standing there, or at home there,
    // or the nearest home next to it (they may have gone home since).
    let home_of = |hex: Hex| {
        g.militias()
            .find(|(home, m)| m.at == Some(hex) || *home == hex)
            .or_else(|| {
                g.militias()
                    .find(|(home, _)| home.unsigned_distance_to(hex) <= 1)
            })
            .map_or(hex, |(home, _)| home)
    };
    let now = time.elapsed_secs();
    for event in &events {
        let parts = match *event {
            Event::MilitiaStruck { hex, mob, fell } => mob_hex(mob).map(|to| {
                let dead = fell.then(|| game.mob_kind(mob));
                (Side::Men(home_of(hex)), hex, Side::Mob(mob), to, dead, mob)
            }),
            Event::MilitiaHit { home, player, .. } => g.champion(player).map(|c| {
                (
                    Side::Men(home),
                    post(home),
                    Side::Champion(player),
                    c.hex,
                    None,
                    0,
                )
            }),
            Event::UndeadHitMilitia { id, home } => {
                mob_hex(id).map(|from| (Side::Mob(id), from, Side::Men(home), post(home), None, 0))
            }
            Event::BeastMauled { beast, undead } => {
                mob_hex(beast).zip(mob_hex(undead)).map(|(from, to)| {
                    let dead = Some(MobKind::Undead);
                    (Side::Mob(beast), from, Side::Mob(undead), to, dead, undead)
                })
            }
            Event::GuardHewed { hex, undead } => mob_hex(undead).map(|to| {
                let dead = Some(MobKind::Undead);
                (Side::Guard, hex, Side::Mob(undead), to, dead, undead)
            }),
            _ => None,
        };
        debug!("clash {event:?}: {parts:?}");
        let Some((attacker, from, victim, to, dead, id)) = parts else {
            continue;
        };
        // Dev aid: with `NECROMY_CLASH` set they wait for the camera to get
        // there (`NECROMY_HOVER=clash`), for screenshots.
        let wait = if std::env::var_os("NECROMY_CLASH").is_some() {
            1.2
        } else {
            0.0
        };
        let start = (now + wait).max(clashes.next);
        clashes.next = start + GAP;
        let dead = dead.map(|kind| {
            let image = art.mob(kind, id);
            // `Sprite3d` reads the size on spawn: no ghost before the picture.
            let ghost = images
                .contains(&image)
                .then(|| commands.spawn(figure(image, board.hex_to_world(to))).id());
            (kind, ghost)
        });
        clashes.playing.push(Clash {
            attacker,
            from,
            victim,
            to,
            dead,
            start,
            lunged: false,
            struck: false,
        });
    }
}

/// Plays the lined-up clashes: the lunge, the blow, the fall.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play_clashes(
    mut commands: Commands,
    time: Res<Time>,
    mut clashes: ResMut<Clashes>,
    sprites: Res<ClashSprites>,
    board: Res<Board>,
    camera: Single<&Transform, With<crate::TableCamera>>,
    figures: Query<
        (
            Entity,
            &Transform,
            Option<&UndeadToken>,
            Option<&MilitiaMan>,
            Has<GuardToken>,
            Has<Lunge>,
        ),
        Or<(With<UndeadToken>, With<MilitiaMan>, With<GuardToken>)>,
    >,
    tokens: Query<(&Token, &GlobalTransform)>,
) {
    if clashes.playing.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    let towards = *camera.back();
    let Clashes {
        playing, scatter, ..
    } = &mut *clashes;
    // The figures standing for a side.
    let of = |side: Side| -> Vec<(Entity, Vec3, bool)> {
        figures
            .iter()
            .filter(|(_, _, mob, man, guard, _)| match side {
                Side::Mob(id) => mob.is_some_and(|m| m.0 == id),
                Side::Men(home) => man.is_some_and(|m| m.hex == home),
                Side::Guard => *guard,
                Side::Champion(_) => false,
            })
            .map(|(e, t, .., busy)| (e, t.translation, busy))
            .collect()
    };
    let move_figure = |commands: &mut Commands, entity: Entity, base: Vec3, dir: Vec3| {
        commands.entity(entity).try_insert(Lunge {
            base,
            dir,
            start: now,
        });
    };
    for clash in playing.iter_mut() {
        if now < clash.start {
            continue;
        }
        let from = board.hex_to_world(clash.from);
        let to = board.hex_to_world(clash.to);
        if !clash.lunged {
            clash.lunged = true;
            let dir = (to - from).with_y(0.0).normalize_or_zero();
            for (entity, at, busy) in of(clash.attacker) {
                if !busy {
                    move_figure(&mut commands, entity, at, dir);
                }
            }
        }
        if !clash.struck && now >= clash.start + HIT {
            clash.struck = true;
            // Where the blow lands: on the victim's figure, or its hex.
            let feet = match clash.victim {
                Side::Champion(p) => tokens
                    .iter()
                    .find(|(t, _)| t.player == p)
                    .map(|(_, at)| at.translation()),
                side => of(side).first().map(|(_, at, _)| *at),
            }
            .unwrap_or(to);
            let at = feet + Vec3::Y * CHEST + towards * FORWARD;
            commands.spawn((
                Particle::new(Vec3::ZERO, 0.0, 0.16),
                sprite(sprites.slash.clone()),
                Transform::from_translation(at),
            ));
            for _ in 0..6 {
                let vel = Vec3::new(
                    scatter.next() * 1.4,
                    0.6 + scatter.next().abs() * 1.2,
                    scatter.next() * 1.4,
                );
                commands.spawn((
                    Particle::new(vel, GRAVITY, 0.35),
                    sprite(sprites.spark.clone()),
                    Transform::from_translation(at),
                ));
            }
            // The victim shakes, and so does a ghost.
            for (entity, at, busy) in of(clash.victim) {
                if !busy {
                    move_figure(&mut commands, entity, at, Vec3::ZERO);
                }
            }
            if let Some((_, Some(ghost))) = clash.dead {
                move_figure(&mut commands, ghost, to, Vec3::ZERO);
            }
        }
        // The fallen breaks apart: bones for the dead, fur for a beast.
        if let Some((kind, ghost)) = clash.dead
            && now >= clash.start + HIT + DYING
        {
            clash.dead = None;
            if let Some(ghost) = ghost {
                commands.entity(ghost).despawn();
            }
            let image = match kind {
                MobKind::Undead => &sprites.bone,
                MobKind::Beast { .. } => &sprites.fur,
                // What came from beyond goes out in sparks.
                MobKind::Monster { .. } | MobKind::Guest => &sprites.spark,
            };
            let at = to + towards * FORWARD;
            for k in 0..12 {
                let vel = Vec3::new(
                    scatter.next() * 1.2,
                    0.8 + scatter.next().abs() * 1.6,
                    scatter.next() * 1.2,
                );
                let y = 0.15 + 0.07 * (k % 8) as f32;
                commands.spawn((
                    Particle::new(vel, GRAVITY, 1.4).floor(at.y + 0.02),
                    sprite(image.clone()),
                    Transform::from_translation(at + Vec3::Y * y),
                ));
            }
        }
    }
    playing.retain(|c| now < c.start + HIT + DYING + 0.1 || c.dead.is_some());
}

/// Figures in a clash: out towards the victim and back, or a shake that
/// dies down; then they stand where they were.
fn lunge(
    mut commands: Commands,
    time: Res<Time>,
    camera: Single<&Transform, (With<crate::TableCamera>, Without<Lunge>)>,
    mut figures: Query<(Entity, &Lunge, &mut Transform)>,
) {
    let now = time.elapsed_secs();
    let right = camera.right().with_y(0.0).normalize_or_zero();
    for (entity, lunge, mut transform) in &mut figures {
        let t = now - lunge.start;
        let shake = lunge.dir == Vec3::ZERO;
        let secs = if shake { SHAKE_SECS } else { LUNGE_SECS };
        if t >= secs {
            transform.translation = lunge.base;
            commands.entity(entity).try_remove::<Lunge>();
            continue;
        }
        let offset = if shake {
            right * 0.05 * (t * 55.0).sin() * (1.0 - t / secs)
        } else {
            lunge.dir * REACH * (std::f32::consts::PI * t / secs).sin()
        };
        transform.translation = lunge.base + offset;
    }
}
