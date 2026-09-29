//! Fires (docs/design.md §21.8).
//!
//! Fire takes woods, groves and settlements. In each world phase a fire
//! burns its hex out, leaving ash (a settlement is left in ruins), and
//! catches on each such neighbour half the time; water, swamps and the
//! mist stop it, and a flood puts it out. Whoever stands in it or walks
//! into it is burnt, never to the last health; the dead and beasts in it
//! perish. A champion kindles a hex beside them for Spirit, or douses one;
//! a fire remembers who began it, and so does every fire it lights.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError};
use crate::board::Terrain;
use crate::features::Feature;

/// Spirit to kindle a fire or to douse one.
pub const KINDLE_SPIRIT: u8 = 1;
pub const DOUSE_SPIRIT: u8 = 1;
/// Regions a fire of one's own must pass for the Great Fire.
pub const GREAT_FIRE: usize = 4;

/// A fire on a hex, and whose it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fire {
    pub by: Option<PlayerId>,
}

impl Terrain {
    /// What fire takes.
    pub const fn burns(self) -> bool {
        matches!(self, Terrain::Forest | Terrain::Grove | Terrain::Settlement)
    }
}

impl Game {
    pub fn fire(&self, hex: Hex) -> Option<Fire> {
        self.fires.get(&(hex.x(), hex.y())).copied()
    }

    pub fn fires(&self) -> impl Iterator<Item = (Hex, Fire)> + '_ {
        self.fires.iter().map(|(&(x, y), &f)| (Hex::new(x, y), f))
    }

    /// Regions a fire of `player`'s has burnt through.
    pub fn regions_burnt(&self, player: PlayerId) -> usize {
        self.burnt
            .get(player.0 as usize)
            .map_or(0, |m| m.count_ones() as usize)
    }

    /// Whether a fire of `player`'s has burnt through `god`'s region.
    pub fn burnt_by(&self, player: PlayerId, god: crate::gods::God) -> bool {
        self.burnt
            .get(player.0 as usize)
            .is_some_and(|m| m & (1 << god.index()) != 0)
    }

    /// The hexes beside `player` (and underfoot) they could set alight.
    pub fn kindleable(&self, player: PlayerId) -> Vec<Hex> {
        if !self.has(Feature::Fires) {
            return Vec::new();
        }
        let at = self.hex_of(player);
        at.all_neighbors()
            .into_iter()
            .filter(|&h| self.catches(h))
            .collect()
    }

    /// The fires beside `player` (and underfoot) they could put out.
    pub fn dousable(&self, player: PlayerId) -> Vec<Hex> {
        let at = self.hex_of(player);
        std::iter::once(at)
            .chain(at.all_neighbors())
            .filter(|&h| self.fire(h).is_some())
            .collect()
    }

    fn catches(&self, hex: Hex) -> bool {
        self.fire(hex).is_none() && self.board.tile(hex).is_some_and(|t| t.terrain.burns())
    }

    fn spend(
        &mut self,
        player: PlayerId,
        amount: u8,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let have = self.champions[player.0 as usize].spirit_points;
        if have < amount {
            return Err(RuleError::NotEnoughSpirit { need: amount, have });
        }
        let champ = self.champ_mut(player);
        champ.spirit_points -= amount;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
        Ok(())
    }

    pub(super) fn check_kindle(&self, player: PlayerId, hex: Hex) -> Result<(), RuleError> {
        if !self.kindleable(player).contains(&hex) {
            return Err(RuleError::InvalidTarget);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < KINDLE_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: KINDLE_SPIRIT,
                have,
            });
        }
        Ok(())
    }

    pub(super) fn kindle(
        &mut self,
        player: PlayerId,
        hex: Hex,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_kindle(player, hex)?;
        self.spend(player, KINDLE_SPIRIT, events)?;
        self.set_fire(hex, Some(player), events);
        self.first(player, super::Novelty::Kindled, events);
        Ok(())
    }

    pub(super) fn check_douse(&self, player: PlayerId, hex: Hex) -> Result<(), RuleError> {
        if !self.dousable(player).contains(&hex) {
            return Err(RuleError::InvalidTarget);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < DOUSE_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: DOUSE_SPIRIT,
                have,
            });
        }
        Ok(())
    }

    pub(super) fn douse(
        &mut self,
        player: PlayerId,
        hex: Hex,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_douse(player, hex)?;
        self.spend(player, DOUSE_SPIRIT, events)?;
        self.put_out(hex, events);
        self.first(player, super::Novelty::Doused, events);
        Ok(())
    }

    /// A fire on `hex`, begun by `by`.
    pub(super) fn set_fire(&mut self, hex: Hex, by: Option<PlayerId>, events: &mut Vec<Event>) {
        if !self.catches(hex) {
            return;
        }
        self.fires.insert((hex.x(), hex.y()), Fire { by });
        events.push(Event::FireStarted { hex, by });
    }

    /// Water, the mist or a champion's hand: the fire on `hex` goes out.
    pub(super) fn put_out(&mut self, hex: Hex, events: &mut Vec<Event>) {
        if self.fires.remove(&(hex.x(), hex.y())).is_some() {
            events.push(Event::FireOut { hex });
        }
    }

    /// The fuel nearest `near`, not underfoot: where a wish sets its fire.
    pub(super) fn fire_near(
        &mut self,
        player: PlayerId,
        near: Hex,
        count: usize,
        events: &mut Vec<Event>,
    ) {
        let mut spots: Vec<Hex> = self
            .board
            .land()
            .map(|(h, _)| h)
            .filter(|&h| h != near && self.catches(h) && self.champion_at(h).is_none())
            .collect();
        spots.sort_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()));
        for hex in spots.into_iter().take(count) {
            self.set_fire(hex, Some(player), events);
        }
    }

    /// A champion walking into a fire is burnt.
    pub(super) fn scorch(&mut self, player: PlayerId, at: Hex, events: &mut Vec<Event>) {
        if self.fire(at).is_none() {
            return;
        }
        let champ = self.champ_mut(player);
        let burn = u8::from(champ.hp > 1);
        champ.hp -= burn;
        let hp = champ.hp;
        events.push(Event::Scorched {
            player,
            amount: burn,
            hp,
        });
    }

    /// World phase: every fire burns its hex out and catches beside it.
    pub(super) fn fire_phase(&mut self, events: &mut Vec<Event>) {
        let burning: Vec<(Hex, Fire)> = self.fires().collect();
        let mut caught: Vec<(Hex, Fire)> = Vec::new();
        for &(hex, fire) in &burning {
            for n in hex.all_neighbors() {
                if self.catches(n)
                    && !burning.iter().any(|(h, _)| *h == n)
                    && !caught.iter().any(|(h, _)| *h == n)
                    && self.rng.below(2) == 0
                {
                    caught.push((n, fire));
                }
            }
        }
        for (hex, fire) in burning {
            self.burn_out(hex, fire, events);
        }
        for (hex, fire) in caught {
            self.set_fire(hex, fire.by, events);
            if let Some(p) = self.champion_at(hex) {
                self.scorch(p, hex, events);
            }
        }
    }

    /// The fire on `hex` has burnt it: ash, or ruins where people lived.
    fn burn_out(&mut self, hex: Hex, fire: Fire, events: &mut Vec<Event>) {
        self.fires.remove(&(hex.x(), hex.y()));
        let Some(tile) = self.board.tile(hex).cloned() else {
            return;
        };
        if let (Some(by), Some(region)) = (fire.by, tile.region)
            && let Some(mask) = self.burnt.get_mut(by.0 as usize)
        {
            *mask |= 1 << region.index();
        }
        let dead: Vec<u32> = self
            .mobs
            .iter()
            .filter(|m| m.hex == hex)
            .map(|m| m.id)
            .collect();
        for id in dead {
            self.mobs.retain(|m| m.id != id);
            events.push(Event::MobLeft { id });
        }
        if tile.terrain == Terrain::Settlement && self.has(Feature::Ruins) {
            self.ruin(hex, events);
        } else {
            let t = self.board.tile_mut(hex).expect("checked above");
            t.terrain = Terrain::Ash;
            t.corpse = None;
            let key = (hex.x(), hex.y());
            self.claims.remove(&key);
            self.buildings.remove(&key);
            self.militia.remove(&key);
            events.push(Event::TerrainChanged {
                hex,
                terrain: Terrain::Ash,
            });
        }
        events.push(Event::FireOut { hex });
    }
}
