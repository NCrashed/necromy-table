//! Physical dice throws (docs/design.md §12.2).
//!
//! A throw is a rigid-body simulation in a fixed tray, started from initial
//! conditions derived only from a seed. The face each die settles on is the
//! result: there is no hidden roll behind the animation. The server runs the
//! throw to decide the battle; clients run the same function with the same
//! seed to show the same motion.
//!
//! Every die is the same canonical body. Skins (§12.3) only draw over the
//! frames this returns and never feed back into the simulation.

use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

/// Simulation steps per second.
pub const TICK_HZ: u32 = 60;
/// Hard stop for a throw that will not settle (a die leaning on a wall).
pub const MAX_TICKS: u32 = TICK_HZ * 8;
/// Half the edge of a die, in tray units.
pub const DIE_HALF: f32 = 0.5;
/// Half the inner size of the tray floor along x and z.
pub const TRAY_HALF: [f32; 2] = [5.0, 3.5];

/// Die faces (§12.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Face {
    Strike,
    Shield,
    Sun,
    Moon,
    Element,
    Blank,
}

/// Each face and the die-local axis pointing out of it.
pub const FACE_AXES: [(Face, [f32; 3]); 6] = [
    (Face::Strike, [0.0, 1.0, 0.0]),
    (Face::Blank, [0.0, -1.0, 0.0]),
    (Face::Shield, [1.0, 0.0, 0.0]),
    (Face::Element, [-1.0, 0.0, 0.0]),
    (Face::Sun, [0.0, 0.0, 1.0]),
    (Face::Moon, [0.0, 0.0, -1.0]),
];

/// One die at one tick: position and rotation quaternion (x, y, z, w).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
}

/// A die hit the tray or another die hard enough to hear. Skins hang their
/// sounds and particles on these; they happen at the same tick everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Impact {
    pub tick: u32,
    pub die: u8,
    /// Change of speed in that tick, tray units per second.
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Throw {
    /// Face up of each die once it settled.
    pub faces: Vec<Face>,
    /// `frames[tick][die]`, starting with the initial pose.
    pub frames: Vec<Vec<Pose>>,
    pub impacts: Vec<Impact>,
}

impl Throw {
    pub fn ticks(&self) -> usize {
        self.frames.len()
    }
}

/// Throws `count` dice. Same seed and count, same throw, on every platform.
pub fn throw(seed: u64, count: u8) -> Throw {
    let mut rng = SplitMix(seed ^ 0x6469_6365_7468_726f);
    let mut world = PhysicsWorld::default();
    world.integration_parameters.dt = 1.0 / TICK_HZ as f32;
    build_tray(&mut world);

    let dice: Vec<RigidBodyHandle> = (0..count)
        .map(|i| {
            // Launched from the left edge towards the right wall, spread in z.
            let z_slot = (i as f32 - (count as f32 - 1.0) / 2.0) * 1.3;
            let body = RigidBodyBuilder::dynamic()
                .translation(Vector::new(
                    -TRAY_HALF[0] + 1.2 + rng.range(0.0, 0.6),
                    1.6 + rng.range(0.0, 1.2),
                    (z_slot + rng.range(-0.3, 0.3)).clamp(-TRAY_HALF[1] + 0.8, TRAY_HALF[1] - 0.8),
                ))
                .rotation(Vector::new(
                    rng.range(-3.1, 3.1),
                    rng.range(-3.1, 3.1),
                    rng.range(-3.1, 3.1),
                ))
                .linvel(Vector::new(
                    rng.range(6.0, 9.0),
                    rng.range(-1.0, 2.0),
                    rng.range(-1.5, 1.5),
                ))
                .angvel(Vector::new(
                    rng.range(-18.0, 18.0),
                    rng.range(-18.0, 18.0),
                    rng.range(-18.0, 18.0),
                ))
                .linear_damping(0.1)
                .angular_damping(0.4)
                .ccd_enabled(true);
            let collider = ColliderBuilder::cuboid(DIE_HALF, DIE_HALF, DIE_HALF)
                .density(1.0)
                .friction(0.5)
                .restitution(0.35);
            world.insert(body, collider).0
        })
        .collect();

    let pose = |world: &PhysicsWorld, h: RigidBodyHandle| {
        let b = &world.bodies[h];
        let t = b.translation();
        let r = b.rotation();
        Pose {
            pos: [t.x, t.y, t.z],
            rot: [r.x, r.y, r.z, r.w],
        }
    };

    let mut frames = vec![dice.iter().map(|&h| pose(&world, h)).collect::<Vec<_>>()];
    let mut impacts = Vec::new();
    let mut last_vel: Vec<Vector> = dice.iter().map(|&h| world.bodies[h].linvel()).collect();
    let mut cooldown = vec![0u32; dice.len()];
    let mut still_ticks = 0;

    for tick in 1..=MAX_TICKS {
        world.step();
        frames.push(dice.iter().map(|&h| pose(&world, h)).collect());

        let mut all_still = true;
        for (i, &h) in dice.iter().enumerate() {
            let body = &world.bodies[h];
            let vel = body.linvel();
            let jolt = (vel - last_vel[i]).length();
            last_vel[i] = vel;
            cooldown[i] = cooldown[i].saturating_sub(1);
            if jolt > 1.5 && cooldown[i] == 0 {
                impacts.push(Impact {
                    tick,
                    die: i as u8,
                    strength: jolt,
                });
                cooldown[i] = 6;
            }
            if vel.length() > 0.05 || body.angvel().length() > 0.05 {
                all_still = false;
            }
        }
        still_ticks = if all_still { still_ticks + 1 } else { 0 };
        if still_ticks >= 15 {
            break;
        }
    }

    let faces = dice
        .iter()
        .map(|&h| face_up(*world.bodies[h].rotation()))
        .collect();
    Throw {
        faces,
        frames,
        impacts,
    }
}

/// The face whose outward axis points most nearly up.
fn face_up(rot: Rotation) -> Face {
    FACE_AXES
        .iter()
        .map(|&(face, [x, y, z])| (face, (rot * Vector::new(x, y, z)).y))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(face, _)| face)
        .expect("six faces")
}

fn build_tray(world: &mut PhysicsWorld) {
    let [hx, hz] = TRAY_HALF;
    let wall_h = 3.0;
    let t = 0.2;
    let walls = [
        // floor
        (Vector::new(0.0, -t, 0.0), [hx + t, t, hz + t]),
        // left, right
        (Vector::new(-hx - t, wall_h, 0.0), [t, wall_h, hz + t]),
        (Vector::new(hx + t, wall_h, 0.0), [t, wall_h, hz + t]),
        // back, front
        (Vector::new(0.0, wall_h, -hz - t), [hx + t, wall_h, t]),
        (Vector::new(0.0, wall_h, hz + t), [hx + t, wall_h, t]),
    ];
    for (at, [x, y, z]) in walls {
        world.insert(
            RigidBodyBuilder::fixed().translation(at),
            ColliderBuilder::cuboid(x, y, z)
                .friction(0.6)
                .restitution(0.3),
        );
    }
}

/// SplitMix64, the same generator the rules use; kept local so this crate
/// stands alone.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..hi`, built from 24 integer bits so it is exact.
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        let unit = (self.next() >> 40) as f32 / (1u32 << 24) as f32;
        lo + (hi - lo) * unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_throw() {
        for seed in 0..20 {
            let a = throw(seed, 4);
            let b = throw(seed, 4);
            assert_eq!(a, b, "seed {seed}");
        }
    }

    #[test]
    fn dice_settle_inside_the_tray() {
        for seed in 0..50 {
            let t = throw(seed, 5);
            assert!(t.ticks() < MAX_TICKS as usize, "seed {seed} never settled");
            for p in t.frames.last().unwrap() {
                assert!(p.pos[0].abs() < TRAY_HALF[0], "seed {seed}: {p:?}");
                assert!(p.pos[2].abs() < TRAY_HALF[1], "seed {seed}: {p:?}");
                assert!(
                    p.pos[1] < DIE_HALF * 4.0,
                    "seed {seed}: die still in the air"
                );
            }
        }
    }

    #[test]
    fn every_face_turns_up() {
        let mut seen = std::collections::HashSet::new();
        for seed in 0..60 {
            seen.extend(throw(seed, 3).faces);
        }
        assert_eq!(seen.len(), 6, "{seen:?}");
    }

    #[test]
    fn dice_rest_flat_enough_to_read() {
        let mut cocked = 0;
        let mut total = 0;
        for seed in 0..100 {
            for p in throw(seed, 4).frames.last().unwrap() {
                let [x, y, z, w] = p.rot;
                let rot = Rotation::from_xyzw(x, y, z, w);
                let best = FACE_AXES
                    .iter()
                    .map(|&(_, [ax, ay, az])| (rot * Vector::new(ax, ay, az)).y)
                    .fold(f32::MIN, f32::max);
                total += 1;
                if best < 0.95 {
                    cocked += 1;
                }
            }
        }
        assert!(cocked * 50 <= total, "{cocked} of {total} dice cocked");
    }

    #[test]
    fn throws_make_noise() {
        assert!(!throw(3, 2).impacts.is_empty());
    }

    /// Golden results. If this breaks on another platform, dice diverge
    /// between server and clients there (§12.2). Update only on purpose.
    #[test]
    fn golden_faces() {
        let got: Vec<Vec<Face>> = (0..8).map(|seed| throw(seed, 3).faces).collect();
        assert_eq!(
            got,
            GOLDEN.map(Vec::from),
            "record the new GOLDEN if intended"
        );
    }

    const GOLDEN: [[Face; 3]; 8] = {
        use Face::*;
        [
            [Moon, Sun, Shield],
            [Shield, Moon, Element],
            [Sun, Sun, Strike],
            [Strike, Shield, Shield],
            [Strike, Sun, Moon],
            [Shield, Element, Strike],
            [Shield, Moon, Moon],
            [Blank, Shield, Element],
        ]
    };
}
