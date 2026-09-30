//! Burdens (docs/design.md §21.8): one thing a champion carries.
//!
//! A body taken off the ground, and later food, goods, an egg: a champion
//! carries one at a time, a step shorter each turn for it. It is laid down
//! where they stand. Whoever wins a battle takes the loser's burden if
//! their own hands are free; the fallen drop theirs where they fall.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError};
use crate::board::Corpse;
use crate::features::Feature;

/// What a champion carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cargo {
    /// A body; a champion's still counts as one (a World Tree). A
    /// poisoned one stays poisoned (§20.1).
    Body {
        hero: bool,
        #[serde(default)]
        tainted: bool,
    },
    /// Food from a field, for a settlement's stores.
    Food,
    /// Goods of a region, for a fair.
    Goods(crate::gods::God),
    /// A dragon's egg, warmed in `warmth` fires, last laid down by `by`.
    Egg { warmth: u8, by: Option<PlayerId> },
}

impl Game {
    pub fn cargo(&self, player: PlayerId) -> Option<Cargo> {
        self.champion(player).and_then(|c| c.cargo)
    }

    /// Burdens lying on the ground.
    pub fn loads(&self) -> &[(Hex, Cargo)] {
        &self.loads
    }

    /// What `player` could take where they stand: a body, or a burden lying
    /// there.
    pub fn takeable(&self, player: PlayerId) -> Option<Cargo> {
        if !self.has(Feature::Cargo) || self.cargo(player).is_some() {
            return None;
        }
        let at = self.hex_of(player);
        self.loads
            .iter()
            .find(|(h, _)| *h == at)
            .map(|&(_, c)| c)
            .or_else(|| {
                self.board
                    .tile(at)
                    .and_then(|t| t.corpse)
                    .map(|c| Cargo::Body {
                        hero: c.hero,
                        tainted: c.tainted,
                    })
            })
    }

    pub(super) fn check_take(&self, player: PlayerId) -> Result<Cargo, RuleError> {
        self.takeable(player).ok_or(RuleError::NoCargo)
    }

    /// The burden underfoot goes on the champion's back.
    pub(super) fn take(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let cargo = self.check_take(player)?;
        let at = self.hex_of(player);
        if let Some(i) = self.loads.iter().position(|(h, _)| *h == at) {
            self.loads.remove(i);
        } else if let Some(tile) = self.board.tile_mut(at) {
            tile.corpse = None;
        }
        self.champ_mut(player).cargo = Some(cargo);
        events.push(Event::CargoTaken {
            player,
            cargo,
            hex: at,
        });
        Ok(())
    }

    pub(super) fn check_lay(&self, player: PlayerId) -> Result<Cargo, RuleError> {
        self.cargo(player).ok_or(RuleError::NoCargo)
    }

    /// The burden is laid down where `player` stands.
    pub(super) fn lay(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_lay(player)?;
        let at = self.hex_of(player);
        self.drop_cargo(player, at, events);
        Ok(())
    }

    /// Whatever `player` carries goes to the ground on `hex`: a body lies
    /// there as a body, if the ground is free of one.
    pub(super) fn drop_cargo(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        let Some(cargo) = self.champ_mut(player).cargo.take() else {
            return;
        };
        match cargo {
            Cargo::Body { .. }
                if self.feed_circle(player, hex, events) || self.bury(player, hex, events) => {}
            Cargo::Egg { warmth, .. } => self.loads.push((
                hex,
                Cargo::Egg {
                    warmth,
                    by: Some(player),
                },
            )),
            Cargo::Body { hero, tainted } => match self.board.tile_mut(hex) {
                Some(tile) if tile.corpse.is_none() && tile.terrain.is_land() => {
                    tile.corpse = Some(Corpse {
                        age: 0,
                        hero,
                        tainted,
                    });
                }
                _ => self.loads.push((hex, cargo)),
            },
            Cargo::Goods(god) => {
                if !self.sell_goods(player, hex, god, events) {
                    self.loads.push((hex, cargo));
                }
            }
            Cargo::Food => {
                if !self.store_food(player, hex, events) {
                    self.loads.push((hex, cargo));
                }
            }
        }
        events.push(Event::CargoLaid { player, cargo, hex });
    }

    /// A battle won: the winner takes the loser's burden if their hands
    /// are free; else it falls where the loser stands.
    pub(super) fn seize_cargo(
        &mut self,
        winner: PlayerId,
        loser: PlayerId,
        events: &mut Vec<Event>,
    ) {
        let Some(cargo) = self.cargo(loser) else {
            return;
        };
        if self.cargo(winner).is_none() {
            self.champ_mut(loser).cargo = None;
            self.champ_mut(winner).cargo = Some(cargo);
            events.push(Event::CargoSeized {
                player: winner,
                from: loser,
                cargo,
            });
        } else {
            let at = self.hex_of(loser);
            self.drop_cargo(loser, at, events);
        }
    }
}
