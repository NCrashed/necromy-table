//! Roads of the register (docs/design.md §21.8).
//!
//! A road lies on a hex. A step from a road onto a road costs one on any
//! land, and a road over a river is a bridge: no crossing to make. Nobody
//! stays hidden on a road. The mist and still water take the roads they
//! cover. A champion paves the hex underfoot for Spirit; a wish runs the
//! register's road on, from the network about the Table towards the
//! nearest temple not yet on it.

use hexx::Hex;

use super::{Event, Game, PlayerId, RuleError};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::God;

/// Spirit to pave the hex underfoot.
pub const PAVE_SPIRIT: u8 = 1;
/// Hexes of road a wish of power 0 lays; each power adds one.
pub const ROAD_RUN: usize = 2;

impl Game {
    pub fn road(&self, hex: Hex) -> bool {
        self.roads.contains(&(hex.x(), hex.y()))
    }

    pub fn roads(&self) -> impl Iterator<Item = Hex> + '_ {
        self.roads.iter().map(|&(x, y)| Hex::new(x, y))
    }

    /// A step from `from` onto `to` along a road: one, bridges included.
    pub(super) fn on_road(&self, from: Hex, to: Hex) -> bool {
        self.road(to)
            && (self.road(from)
                || self
                    .board
                    .tile(to)
                    .is_some_and(|t| t.terrain == Terrain::River))
    }

    /// Land a road may lie on.
    fn pavable(&self, hex: Hex) -> bool {
        self.board.tile(hex).is_some_and(|t| t.terrain.is_land()) && !self.road(hex)
    }

    pub fn may_pave(&self, player: PlayerId) -> bool {
        self.has(Feature::Roads) && self.pavable(self.hex_of(player))
    }

    pub(super) fn check_pave(&self, player: PlayerId) -> Result<(), RuleError> {
        if !self.may_pave(player) {
            return Err(RuleError::CannotBuild);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < PAVE_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: PAVE_SPIRIT,
                have,
            });
        }
        Ok(())
    }

    /// A road on the hex underfoot.
    pub(super) fn pave(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_pave(player)?;
        let champ = self.champ_mut(player);
        champ.spirit_points -= PAVE_SPIRIT;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
        let hex = self.hex_of(player);
        self.lay_road(hex, events);
        self.first(player, super::Novelty::Paved, events);
        if self.is_hidden(player) {
            self.reveal(player, super::stealth::RevealReason::Crowd, events);
        }
        Ok(())
    }

    pub(super) fn lay_road(&mut self, hex: Hex, events: &mut Vec<Event>) {
        if self.roads.insert((hex.x(), hex.y())) {
            events.push(Event::RoadLaid { hex });
        }
    }

    /// The land on `hex` went into the mist or under still water.
    pub(super) fn wash_road(&mut self, hex: Hex) {
        self.roads.remove(&(hex.x(), hex.y()));
    }

    /// The register's network: the Table and every road and temple joined
    /// to it by road.
    pub fn network(&self) -> Vec<Hex> {
        let joins = |h: Hex| {
            self.road(h)
                || self
                    .board
                    .tile(h)
                    .is_some_and(|t| matches!(t.terrain, Terrain::Temple | Terrain::Table))
        };
        let mut net = vec![Hex::ZERO];
        let mut i = 0;
        while i < net.len() {
            let at = net[i];
            // Only a road leads on; a temple or the Table ends a branch but
            // for the Table itself.
            if i == 0 || self.road(at) {
                for n in at.all_neighbors() {
                    if joins(n) && !net.contains(&n) {
                        net.push(n);
                    }
                }
            }
            i += 1;
        }
        net
    }

    /// Temples on the register's network.
    pub fn temples_linked(&self) -> usize {
        let net = self.network();
        God::ALL
            .iter()
            .filter(|&&g| net.contains(&self.board.temple_of(g)))
            .count()
    }

    /// The register's road runs on by `count` hexes: from the network
    /// towards the temple nearest `near` not on it yet.
    pub(super) fn run_road(&mut self, near: Hex, count: usize, events: &mut Vec<Event>) -> usize {
        let net = self.network();
        let Some(goal) = God::ALL
            .iter()
            .map(|&g| self.board.temple_of(g))
            .filter(|t| !net.contains(t) && self.board.contains(*t))
            .min_by_key(|t| (t.unsigned_distance_to(near), t.x(), t.y()))
        else {
            return 0;
        };
        // Breadth first from the goal over land, to the nearest of the net.
        let mut prev: std::collections::BTreeMap<(i32, i32), Hex> = Default::default();
        let mut queue = std::collections::VecDeque::from([goal]);
        prev.insert((goal.x(), goal.y()), goal);
        let mut reached = None;
        while let Some(at) = queue.pop_front() {
            if net.contains(&at) && (self.road(at) || at == Hex::ZERO) {
                reached = Some(at);
                break;
            }
            let mut next: Vec<Hex> = at
                .all_neighbors()
                .into_iter()
                // Not through another temple: the network ends at one.
                .filter(|&n| {
                    (self.board.contains(n) || n == Hex::ZERO)
                        && (n == Hex::ZERO
                            || self
                                .board
                                .tile(n)
                                .is_none_or(|t| t.terrain != Terrain::Temple))
                })
                .collect();
            next.sort_by_key(|h| (h.x(), h.y()));
            for n in next {
                if let std::collections::btree_map::Entry::Vacant(e) = prev.entry((n.x(), n.y())) {
                    e.insert(at);
                    queue.push_back(n);
                }
            }
        }
        let Some(mut at) = reached else {
            return 0;
        };
        // Walk back towards the goal, paving what lacks a road.
        let mut laid = 0;
        while laid < count && at != goal {
            at = prev[&(at.x(), at.y())];
            if at == goal {
                break;
            }
            if self.pavable(at) {
                self.lay_road(at, events);
                laid += 1;
            }
        }
        laid
    }
}
