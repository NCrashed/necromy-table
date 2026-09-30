//! Burial, graveyards and the plague pit (docs/design.md §21.8).
//!
//! A champion consecrates the ground underfoot as a graveyard: a body laid
//! down on it is buried and never rises, nor does one left lying there.
//! A plague pit is dug the same way; bodies thrown in it rot together and
//! poison whoever ends a turn beside it. A pit of five bodies is settled by
//! its digger: laid to rest (it becomes a graveyard) or raised as the dead.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::{Element, God};

/// Spirit to consecrate a graveyard or dig a pit.
pub const CONSECRATE_SPIRIT: u8 = 1;
/// Bodies a pit holds before it is settled.
pub const PIT_BODIES: u8 = 5;
/// Graveyard hexes side by side for the Necropolis, and bodies buried.
pub const NECROPOLIS: usize = 3;
pub const NECROPOLIS_BODIES: usize = 5;

/// A plague pit and who dug it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pit {
    pub owner: PlayerId,
    pub bodies: u8,
}

impl Game {
    pub fn graves(&self, hex: Hex) -> u8 {
        self.graves.get(&(hex.x(), hex.y())).copied().unwrap_or(0)
    }

    pub fn pit(&self, hex: Hex) -> Option<Pit> {
        self.pits.get(&(hex.x(), hex.y())).copied()
    }

    pub fn pits(&self) -> impl Iterator<Item = (Hex, Pit)> + '_ {
        self.pits.iter().map(|(&(x, y), &p)| (Hex::new(x, y), p))
    }

    /// Whether `player` settled a plague pit this match.
    pub fn settled_pit(&self, player: PlayerId) -> bool {
        self.plague_done
            .get(player.0 as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Ground underfoot that may be consecrated or dug.
    pub fn may_consecrate(&self, player: PlayerId) -> bool {
        let at = self.hex_of(player);
        self.has(Feature::Burial)
            && self.board.tile(at).is_some_and(|t| {
                matches!(t.terrain, Terrain::Plains | Terrain::Ash | Terrain::Ruins)
                    && t.corpse.is_none()
            })
            && self.trial_at(at).is_none()
    }

    pub(super) fn check_consecrate(&self, player: PlayerId) -> Result<(), RuleError> {
        if !self.may_consecrate(player) {
            return Err(RuleError::CannotBuild);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < CONSECRATE_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: CONSECRATE_SPIRIT,
                have,
            });
        }
        Ok(())
    }

    fn pay_consecration(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let champ = self.champ_mut(player);
        champ.spirit_points -= CONSECRATE_SPIRIT;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
    }

    pub(super) fn set_ground(&mut self, hex: Hex, terrain: Terrain, events: &mut Vec<Event>) {
        let key = (hex.x(), hex.y());
        self.ruins.remove(&key);
        if let Some(t) = self.board.tile_mut(hex) {
            t.terrain = terrain;
        }
        events.push(Event::TerrainChanged { hex, terrain });
    }

    /// The ground underfoot becomes a graveyard.
    pub(super) fn consecrate(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_consecrate(player)?;
        self.pay_consecration(player, events);
        let hex = self.hex_of(player);
        self.set_ground(hex, Terrain::Graveyard, events);
        self.first(player, super::Novelty::Consecrated, events);
        Ok(())
    }

    /// A plague pit underfoot, theirs.
    pub(super) fn dig_pit(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_consecrate(player)?;
        self.pay_consecration(player, events);
        let hex = self.hex_of(player);
        self.set_ground(hex, Terrain::Pit, events);
        self.pits.insert(
            (hex.x(), hex.y()),
            Pit {
                owner: player,
                bodies: 0,
            },
        );
        Ok(())
    }

    /// A body laid down on a graveyard or in a pit is buried there.
    pub(super) fn bury(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) -> bool {
        let key = (hex.x(), hex.y());
        match self.board.tile(hex).map(|t| t.terrain) {
            Some(Terrain::Graveyard) => {
                let n = self.graves.entry(key).or_insert(0);
                *n = n.saturating_add(1);
            }
            Some(Terrain::Pit) => {
                let Some(pit) = self.pits.get_mut(&key) else {
                    return false;
                };
                pit.bodies = pit.bodies.saturating_add(1);
            }
            _ => return false,
        }
        events.push(Event::Buried { player, hex });
        // The militia think well of whoever buries the dead (§21.8).
        if self.has(Feature::Militia) {
            self.shift_standing(player, 1, events);
        }
        // Zaga's Stillness: a burial is an offering, and it quiets.
        if self.law_active(super::Law::Stillness) {
            self.offer(Some(player), God::Zaga, 1, events);
            if self.threat(player) > 0 {
                self.add_threat(player, -1, events);
            }
        }
        self.first(player, super::Novelty::Buried, events);
        true
    }

    /// The end of a turn beside a pit with bodies in it: poison of earth.
    pub(super) fn pit_fumes(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let at = self.hex_of(player);
        let near = self
            .pits()
            .any(|(h, p)| p.bodies > 0 && h.unsigned_distance_to(at) <= 1);
        if near {
            let stacks = self.pit_stacks();
            self.poison(player, Element::Earth, stacks, events);
        }
    }

    /// `player`'s own pit underfoot, full.
    pub fn may_settle_pit(&self, player: PlayerId) -> bool {
        self.pit(self.hex_of(player))
            .is_some_and(|p| p.owner == player && p.bodies >= PIT_BODIES)
    }

    pub(super) fn check_settle_pit(&self, player: PlayerId) -> Result<(), RuleError> {
        if self.may_settle_pit(player) {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    /// The pit's bodies laid to rest (a graveyard, Maya and Zaga honoured)
    /// or raised as the dead around it.
    pub(super) fn settle_pit(
        &mut self,
        player: PlayerId,
        raise: bool,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_settle_pit(player)?;
        let hex = self.hex_of(player);
        let pit = self.pits.remove(&(hex.x(), hex.y())).expect("checked");
        if raise {
            self.set_ground(hex, Terrain::Plains, events);
            let spots: Vec<Hex> = hex
                .all_neighbors()
                .into_iter()
                .filter(|&h| {
                    self.board.contains(h) && self.champion_at(h).is_none() && !self.mob_at(h)
                })
                .take(usize::from(pit.bodies))
                .collect();
            for h in spots {
                self.next_mob += 1;
                let mob = super::mobs::Mob {
                    id: self.next_mob,
                    kind: super::mobs::MobKind::Undead,
                    hex: h,
                    hp: super::UNDEAD_HEALTH,
                };
                self.mobs.push(mob);
                events.push(Event::MobAppeared { mob });
            }
        } else {
            self.set_ground(hex, Terrain::Graveyard, events);
            self.graves.insert((hex.x(), hex.y()), pit.bodies);
            self.offer(Some(player), God::Maya, 1, events);
            self.offer(Some(player), God::Zaga, 1, events);
        }
        self.plague_done[player.0 as usize] = true;
        events.push(Event::PitSettled {
            player,
            hex,
            raised: raise,
        });
        Ok(())
    }

    /// The greatest graveyard: hexes side by side, and bodies buried in it.
    pub fn best_necropolis(&self) -> (usize, usize) {
        let mut seen: Vec<Hex> = Vec::new();
        let mut best = (0, 0);
        for (hex, t) in self.board.tiles() {
            if t.terrain != Terrain::Graveyard || seen.contains(&hex) {
                continue;
            }
            let mut yard = vec![hex];
            let mut i = 0;
            while i < yard.len() {
                for n in yard[i].all_neighbors() {
                    if self
                        .board
                        .tile(n)
                        .is_some_and(|t| t.terrain == Terrain::Graveyard)
                        && !yard.contains(&n)
                    {
                        yard.push(n);
                    }
                }
                i += 1;
            }
            let buried: usize = yard.iter().map(|&h| usize::from(self.graves(h))).sum();
            let found = (yard.len(), buried);
            let score = |(size, buried): (usize, usize)| {
                size.min(NECROPOLIS) + buried.min(NECROPOLIS_BODIES)
            };
            if score(found) > score(best) {
                best = found;
            }
            seen.extend(yard);
        }
        best
    }

    /// No undead walks Zaga's land.
    pub fn zaga_land_quiet(&self) -> bool {
        !self.mobs.iter().any(|m| {
            m.is_undead() && self.board.tile(m.hex).and_then(|t| t.region) == Some(God::Zaga)
        })
    }
}
