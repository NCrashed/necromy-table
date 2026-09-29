//! Buildings and cities (docs/design.md §21.8).
//!
//! On a settlement they hold, a champion may build one building: a tavern
//! (the hand goes through there at no loss), a forge (a metal ward at the
//! start of a turn there), a shrine of the land's god or of two gods whose
//! lands meet there (prayer as at a temple), a wall (its militia hold three).
//! With cities in the world, a settlement grows a new quarter next to it:
//! settlements side by side are one city.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::God;

/// One building on a settlement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Building {
    Tavern,
    Forge,
    /// A shrine of one god (both the same) or of two whose lands meet.
    Shrine([God; 2]),
    Wall,
}

/// Spirit a building costs.
pub const BUILD_SPIRIT: u8 = 2;
/// Spirit a new quarter of a city costs.
pub const QUARTER_SPIRIT: u8 = 3;
/// Men a walled settlement's militia hold.
pub const WALLED_MILITIA: u8 = 3;

impl Game {
    /// What stands on the settlement on `hex`, if anything.
    pub fn building(&self, hex: Hex) -> Option<Building> {
        self.buildings.get(&(hex.x(), hex.y())).copied()
    }

    pub fn buildings(&self) -> impl Iterator<Item = (Hex, Building)> + '_ {
        self.buildings
            .iter()
            .map(|(&(x, y), &b)| (Hex::new(x, y), b))
    }

    /// What `player` could build where they stand: on a settlement of theirs
    /// with nothing on it yet.
    pub fn may_build(&self, player: PlayerId) -> Vec<Building> {
        let at = self.hex_of(player);
        let theirs = self.owner(at) == Some(player)
            && self
                .board
                .tile(at)
                .is_some_and(|t| t.terrain == Terrain::Settlement);
        if !self.has(Feature::Buildings) || !theirs || self.building(at).is_some() {
            return Vec::new();
        }
        let Some(own) = self.board.tile(at).and_then(|t| t.region) else {
            return Vec::new();
        };
        let mut all = vec![Building::Tavern, Building::Forge, Building::Wall];
        all.push(Building::Shrine([own, own]));
        // A shrine of two where the land touches another god's.
        let mut others: Vec<God> = at
            .all_neighbors()
            .iter()
            .filter_map(|&n| self.board.tile(n).and_then(|t| t.region))
            .filter(|&g| g != own)
            .collect();
        others.sort_by_key(|g| g.index());
        others.dedup();
        all.extend(others.into_iter().map(|g| Building::Shrine([own, g])));
        all
    }

    pub(super) fn check_build(
        &self,
        player: PlayerId,
        building: Building,
    ) -> Result<Hex, RuleError> {
        if !self.may_build(player).contains(&building) {
            return Err(RuleError::CannotBuild);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < BUILD_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: BUILD_SPIRIT,
                have,
            });
        }
        Ok(self.hex_of(player))
    }

    /// `building` rises on the settlement underfoot; the turn's walk ends.
    pub(super) fn build(
        &mut self,
        player: PlayerId,
        building: Building,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let hex = self.check_build(player, building)?;
        self.spend_spirit(player, BUILD_SPIRIT, events);
        self.buildings.insert((hex.x(), hex.y()), building);
        self.turns[player.0 as usize].move_points = 0;
        events.push(Event::Built {
            player,
            hex,
            building,
        });
        self.first(player, super::Novelty::Built, events);
        Ok(())
    }

    /// Hexes where `player` could raise a new quarter of their city: free
    /// open land next to a settlement they hold where they stand.
    pub fn quarters(&self, player: PlayerId) -> Vec<Hex> {
        let at = self.hex_of(player);
        let theirs = self.owner(at) == Some(player)
            && self
                .board
                .tile(at)
                .is_some_and(|t| t.terrain == Terrain::Settlement);
        if !self.has(Feature::City) || !theirs {
            return Vec::new();
        }
        at.all_neighbors()
            .into_iter()
            .filter(|&h| {
                self.board.tile(h).is_some_and(|t| {
                    matches!(
                        t.terrain,
                        Terrain::Plains | Terrain::Forest | Terrain::Grove | Terrain::Ruins
                    ) && t.corpse.is_none()
                }) && self.champion_at(h).is_none()
                    && !self.mob_at(h)
                    && self.trial_at(h).is_none()
            })
            .collect()
    }

    pub(super) fn check_quarter(&self, player: PlayerId, hex: Hex) -> Result<(), RuleError> {
        if !self.quarters(player).contains(&hex) {
            return Err(RuleError::CannotBuild);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < QUARTER_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: QUARTER_SPIRIT,
                have,
            });
        }
        Ok(())
    }

    /// A new quarter of the city on `hex`, theirs, with a man of its own if
    /// the world has militia; the turn's walk ends.
    pub(super) fn raise_quarter(
        &mut self,
        player: PlayerId,
        hex: Hex,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_quarter(player, hex)?;
        self.spend_spirit(player, QUARTER_SPIRIT, events);
        let key = (hex.x(), hex.y());
        self.board.tile_mut(hex).expect("checked").terrain = Terrain::Settlement;
        self.ruins.remove(&key);
        self.claims.insert(key, player);
        if self.has(Feature::Militia) {
            self.militia.insert(
                key,
                super::Militia {
                    men: 1,
                    at: Some(hex),
                },
            );
        }
        self.turns[player.0 as usize].move_points = 0;
        events.push(Event::QuarterRaised { player, hex });
        Ok(())
    }

    fn spend_spirit(&mut self, player: PlayerId, amount: u8, events: &mut Vec<Event>) {
        let champ = self.champ_mut(player);
        champ.spirit_points -= amount;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
    }

    /// The settlements side by side with the one on `hex`: its city.
    pub fn city_of(&self, hex: Hex) -> Vec<Hex> {
        let settled = |h: Hex| {
            self.board
                .tile(h)
                .is_some_and(|t| t.terrain == Terrain::Settlement)
        };
        if !settled(hex) {
            return Vec::new();
        }
        let mut city = vec![hex];
        let mut i = 0;
        while i < city.len() {
            for n in city[i].all_neighbors() {
                if settled(n) && !city.contains(&n) {
                    city.push(n);
                }
            }
            i += 1;
        }
        city
    }

    /// The gods a shrine on `hex` serves; prayer there reaches them.
    pub(super) fn shrine_gods(&self, hex: Hex) -> Option<[God; 2]> {
        match self.building(hex)? {
            Building::Shrine(gods) => Some(gods),
            _ => None,
        }
    }

    /// The start of a turn on a forge of one's own: a metal ward.
    pub(super) fn buildings_at_turn_start(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let at = self.hex_of(player);
        if self.building(at) == Some(Building::Forge) && self.owner(at) == Some(player) {
            self.raise_ward(player, crate::gods::Element::Metal, events);
        }
    }

    /// The most men the militia of `home` hold: more behind a wall.
    pub(super) fn militia_cap(&self, home: (i32, i32)) -> u8 {
        if self.buildings.get(&home) == Some(&Building::Wall) {
            WALLED_MILITIA
        } else {
            super::MILITIA
        }
    }
}
