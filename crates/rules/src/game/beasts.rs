//! The beasts of Bhava's forests (docs/design.md §20.4).
//!
//! While Bhava is not in the light, his woods breed beasts: a night brings
//! one out of a forest or grove of his land, up to `beasts_allowed` (more in
//! the dark stage). Each keeps to the land about its lair
//! (`BEAST_RANGE`): it strikes a champion next to it there, goes after one
//! who walks in, tears apart the undead that wander in, and goes back to
//! its lair when nothing stirs. It leaves the living outside alone, and
//! Bhava's Chosen anywhere. When Bhava turns light the beasts go back into
//! the woods.
//!
//! A beast is fought like an undead (`WindowKind::MobBattle`): a step onto
//! it, only the champion burns. It is stronger, and carries loot half the
//! time.

use hexx::Hex;

use super::mobs::{Mob, MobKind};
use super::{Event, Game, PlayerId};
use crate::board::Terrain;
use crate::gods::God;

pub const BEAST_HEALTH: u8 = 3;
pub const BEAST_DICE: u8 = 3;
/// How far from its lair a beast keeps its land.
pub const BEAST_RANGE: u32 = 2;
/// A new beast comes out at least this far from every champion.
const BEAST_CLEARANCE: u32 = 3;

impl Game {
    /// How many beasts Bhava's woods hold at his stage: none in the light,
    /// a few in the thicket, more in the overgrowth.
    pub fn beasts_allowed(&self) -> usize {
        match self.stage(God::Bhava) {
            0 => 0,
            1 => 2,
            _ => 4,
        }
    }

    /// The land a beast keeps: within `BEAST_RANGE` of its lair.
    pub fn beast_land(mob: &Mob, hex: Hex) -> bool {
        match mob.kind {
            MobKind::Beast { lair } => lair.unsigned_distance_to(hex) <= BEAST_RANGE,
            MobKind::Undead => false,
        }
    }

    /// Whom beasts leave alone: the hidden, and Bhava's Chosen.
    fn beasts_see(&self, player: PlayerId) -> bool {
        !self.is_hidden(player) && !self.chosen(player, God::Bhava)
    }

    /// World phase, at night: a beast comes out of Bhava's woods.
    pub(super) fn beasts_at_night(&mut self, events: &mut Vec<Event>) {
        let beasts = self.mobs.iter().filter(|m| m.is_beast()).count();
        if beasts >= self.beasts_allowed() {
            return;
        }
        let spots: Vec<Hex> = self
            .board
            .tiles()
            .filter(|(h, t)| {
                t.region == Some(God::Bhava)
                    && matches!(t.terrain, Terrain::Forest | Terrain::Grove)
                    && self.champion_at(*h).is_none()
                    && !self.mob_at(*h)
                    && self.trial_at(*h).is_none()
                    && self.players().all(|p| {
                        self.champions[p.0 as usize].hex.unsigned_distance_to(*h) >= BEAST_CLEARANCE
                    })
            })
            .map(|(h, _)| h)
            .collect();
        let Some(&lair) = self.rng.pick(&spots) else {
            return;
        };
        self.next_mob += 1;
        let mob = Mob {
            id: self.next_mob,
            kind: MobKind::Beast { lair },
            hex: lair,
            hp: BEAST_HEALTH,
        };
        self.mobs.push(mob);
        events.push(Event::MobAppeared { mob });
    }

    /// World phase: what each beast does on its land; in Bhava's light
    /// they all go back into the woods.
    pub(super) fn beast_phase(&mut self, events: &mut Vec<Event>) {
        if self.beasts_allowed() == 0 {
            let gone: Vec<u32> = self
                .mobs
                .iter()
                .filter(|m| m.is_beast())
                .map(|m| m.id)
                .collect();
            self.mobs.retain(|m| !m.is_beast());
            for id in gone {
                events.push(Event::MobLeft { id });
            }
            return;
        }
        let ids: Vec<u32> = self
            .mobs
            .iter()
            .filter(|m| m.is_beast())
            .map(|m| m.id)
            .collect();
        for id in ids {
            let Some(beast) = self.mobs.iter().find(|m| m.id == id).copied() else {
                continue;
            };
            let MobKind::Beast { lair } = beast.kind else {
                continue;
            };
            // A champion on its land, next to it: it strikes.
            let prey = self
                .players()
                .filter(|&p| self.beasts_see(p))
                .filter(|&p| {
                    let at = self.hex_of(p);
                    at.unsigned_distance_to(beast.hex) <= 1 && Self::beast_land(&beast, at)
                })
                .min_by_key(|&p| (self.champions[p.0 as usize].hp, p.0));
            if let Some(p) = prey {
                self.mob_strike(id, p, events);
                continue;
            }
            // The dead on its land next to it: torn apart.
            let dead = self
                .mobs
                .iter()
                .filter(|m| m.is_undead())
                .filter(|m| {
                    m.hex.unsigned_distance_to(beast.hex) <= 1 && Self::beast_land(&beast, m.hex)
                })
                .map(|m| m.id)
                .min();
            if let Some(undead) = dead {
                self.mobs.retain(|m| m.id != undead);
                events.push(Event::BeastMauled { beast: id, undead });
                continue;
            }
            // Someone on its land: it goes for them; else home to its lair.
            let intruder = self
                .players()
                .filter(|&p| self.beasts_see(p))
                .map(|p| self.hex_of(p))
                .filter(|&at| Self::beast_land(&beast, at))
                .min_by_key(|h| (h.unsigned_distance_to(beast.hex), h.x(), h.y()));
            let goal = intruder.unwrap_or(lair);
            if goal != beast.hex {
                self.beast_step(beast, goal, events);
            }
        }
    }

    /// One step towards `goal`, never off its land.
    fn beast_step(&mut self, beast: Mob, goal: Hex, events: &mut Vec<Event>) {
        let here = beast.hex.unsigned_distance_to(goal);
        let next = beast
            .hex
            .all_neighbors()
            .into_iter()
            .filter(|&h| self.board.contains(h) && Self::beast_land(&beast, h))
            .filter(|&h| self.champion_at(h).is_none() && !self.mob_at(h))
            .filter(|&h| h.unsigned_distance_to(goal) < here)
            .min_by_key(|&h| (h.unsigned_distance_to(goal), h.x(), h.y()));
        let Some(next) = next else {
            return;
        };
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == beast.id) {
            m.hex = next;
        }
        events.push(Event::MobMoved {
            id: beast.id,
            from: beast.hex,
            to: next,
        });
    }
}
