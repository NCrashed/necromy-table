//! Walking groves (docs/design.md §21.8).
//!
//! The card «Дикий энт» wakes a grove beside its caster: every night the
//! grove takes a step towards Ahamar's Table, leaving woods behind. Water,
//! the mist, fire, the living and anything built stand in its way. Beside
//! the Table it puts down roots, and whoever woke it has made the Walking
//! Forest.

use hexx::Hex;

use super::{Event, Game, PlayerId};
use crate::board::Terrain;
use crate::features::Feature;

impl Game {
    /// Groves awake, and who woke them.
    pub fn walkers(&self) -> impl Iterator<Item = (Hex, PlayerId)> + '_ {
        self.walkers.iter().map(|(&(x, y), &p)| (Hex::new(x, y), p))
    }

    /// `player` rooted a walking grove beside the Table.
    pub fn rooted(&self, player: PlayerId) -> bool {
        self.rooted.get(player.0 as usize).copied().unwrap_or(false)
    }

    /// «Дикий энт» on the grove on `hex`.
    pub(super) fn wake_grove(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        let grove = self
            .board
            .tile(hex)
            .is_some_and(|t| t.terrain == Terrain::Grove);
        if !grove || self.walkers.contains_key(&(hex.x(), hex.y())) {
            return;
        }
        self.walkers.insert((hex.x(), hex.y()), player);
        events.push(Event::GroveWoke { player, hex });
    }

    /// Ground a walking grove may step onto.
    fn open_to_grove(&self, hex: Hex) -> bool {
        self.board.tile(hex).is_some_and(|t| {
            matches!(
                t.terrain,
                Terrain::Plains | Terrain::Forest | Terrain::Swamp | Terrain::Ash | Terrain::Ruins
            )
        }) && self.champion_at(hex).is_none()
            && !self.mob_at(hex)
            && self.trial_at(hex).is_none()
            && self.fire(hex).is_none()
            && !self.walkers.contains_key(&(hex.x(), hex.y()))
    }

    /// Night, world phase: each walking grove steps towards the Table.
    pub(super) fn walk_groves(&mut self, events: &mut Vec<Event>) {
        if !self.has(Feature::WalkingGroves) {
            return;
        }
        let walking: Vec<(Hex, PlayerId)> = self.walkers().collect();
        for (hex, owner) in walking {
            // Burnt, drowned or cut down where it stood: it walks no more.
            if self
                .board
                .tile(hex)
                .is_none_or(|t| t.terrain != Terrain::Grove)
            {
                self.walkers.remove(&(hex.x(), hex.y()));
                continue;
            }
            let next = hex
                .all_neighbors()
                .into_iter()
                .filter(|&n| n.ulength() < hex.ulength() && n != Hex::ZERO)
                .filter(|&n| self.open_to_grove(n))
                .min_by_key(|n| (n.ulength(), n.x(), n.y()));
            let Some(next) = next else {
                continue;
            };
            self.walkers.remove(&(hex.x(), hex.y()));
            let corpse = self.board.tile(hex).and_then(|t| t.corpse);
            if let Some(t) = self.board.tile_mut(hex) {
                t.terrain = Terrain::Forest;
                t.corpse = None;
            }
            if let Some(t) = self.board.tile_mut(next) {
                t.terrain = Terrain::Grove;
                t.corpse = corpse;
            }
            if self.hero_groves.remove(&(hex.x(), hex.y())) {
                self.hero_groves.insert((next.x(), next.y()));
            }
            self.ruins.remove(&(next.x(), next.y()));
            events.push(Event::TerrainChanged {
                hex,
                terrain: Terrain::Forest,
            });
            events.push(Event::TerrainChanged {
                hex: next,
                terrain: Terrain::Grove,
            });
            events.push(Event::GroveWalked {
                from: hex,
                to: next,
            });
            if next.ulength() == 1 {
                self.rooted[owner.0 as usize] = true;
                events.push(Event::GroveRooted {
                    player: owner,
                    hex: next,
                });
            } else {
                self.walkers.insert((next.x(), next.y()), owner);
            }
        }
    }
}
