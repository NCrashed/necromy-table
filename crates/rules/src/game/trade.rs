//! Goods, caravans and fairs (docs/design.md §21.8).
//!
//! At dusk every settlement someone holds makes a load of its region's
//! goods (wood, wine, salt, iron, pearls), lying there for anyone to take on
//! their back; a burden is won in battle, so caravans get robbed. A
//! champion opens a fair in a settlement of theirs for a few days: goods
//! laid down there are sold, each kind once, for Style and a card to the
//! one who brought them. A fair is loud: the dead within sight walk to it
//! first, and one that comes up to it eats the feast, which ends the fair.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError, StyleReason};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::God;

/// Spirit to open a fair.
pub const FAIR_SPIRIT: u8 = 2;
/// Dusks a fair stays open.
pub const FAIR_DUSKS: u32 = 3;
/// Fairs eaten by the dead for the Feast for the Dead.
pub const DEAD_FEASTS: usize = 3;

/// A fair in a settlement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fair {
    pub host: PlayerId,
    /// Kinds of goods sold, by `God::index` of the region they came from.
    pub goods: u8,
    /// The dusk it closes at.
    pub until: u32,
}

impl Fair {
    pub fn kinds(&self) -> usize {
        self.goods.count_ones() as usize
    }

    pub fn has(&self, god: God) -> bool {
        self.goods & (1 << god.index()) != 0
    }
}

impl Game {
    pub fn fair(&self, hex: Hex) -> Option<Fair> {
        self.fairs.get(&(hex.x(), hex.y())).copied()
    }

    pub fn fairs(&self) -> impl Iterator<Item = (Hex, Fair)> + '_ {
        self.fairs.iter().map(|(&(x, y), &f)| (Hex::new(x, y), f))
    }

    /// Fairs of `player`'s the dead have eaten.
    pub fn dead_feasts(&self, player: PlayerId) -> usize {
        self.dead_feasts
            .get(player.0 as usize)
            .copied()
            .unwrap_or(0) as usize
    }

    /// Dusk: each settlement someone holds makes its region's goods.
    pub(super) fn make_goods(&mut self, events: &mut Vec<Event>) {
        if !self.has(Feature::Goods) {
            return;
        }
        let towns: Vec<(Hex, God)> = self
            .claims()
            .filter_map(|(h, _)| {
                let t = self.board.tile(h)?;
                (t.terrain == Terrain::Settlement).then_some((h, t.region?))
            })
            .collect();
        for (hex, god) in towns {
            if !self.loads.iter().any(|(h, _)| *h == hex) {
                self.loads.push((hex, super::Cargo::Goods(god)));
                events.push(Event::GoodsMade { hex, god });
            }
        }
    }

    pub fn may_open_fair(&self, player: PlayerId) -> bool {
        let at = self.hex_of(player);
        self.has(Feature::Fairs) && self.own_settlement(player, at) && self.fair(at).is_none()
    }

    pub(super) fn check_fair(&self, player: PlayerId) -> Result<(), RuleError> {
        if !self.may_open_fair(player) {
            return Err(RuleError::CannotBuild);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < FAIR_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: FAIR_SPIRIT,
                have,
            });
        }
        Ok(())
    }

    /// A fair opens in the settlement underfoot.
    pub(super) fn open_fair(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_fair(player)?;
        let champ = self.champ_mut(player);
        champ.spirit_points -= FAIR_SPIRIT;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
        let hex = self.hex_of(player);
        self.fairs.insert(
            (hex.x(), hex.y()),
            Fair {
                host: player,
                goods: 0,
                until: self.dusks + FAIR_DUSKS,
            },
        );
        events.push(Event::FairOpened { player, hex });
        self.first(player, super::Novelty::HeldFair, events);
        Ok(())
    }

    /// Goods laid down at a fair are sold there, a kind once: Style and a
    /// card for whoever brought them.
    pub(super) fn sell_goods(
        &mut self,
        player: PlayerId,
        hex: Hex,
        god: God,
        events: &mut Vec<Event>,
    ) -> bool {
        let Some(fair) = self.fairs.get_mut(&(hex.x(), hex.y())) else {
            return false;
        };
        if fair.has(god) {
            return false;
        }
        fair.goods |= 1 << god.index();
        let kinds = fair.kinds() as u8;
        events.push(Event::GoodsSold {
            player,
            hex,
            god,
            kinds,
        });
        // Brought from another land, it sells for more (§21.8).
        let worth = 1 + i16::from(self.foreign_goods(hex, god));
        self.add_style(player, worth, StyleReason::Fair, events);
        self.draw(player, 1, events);
        // Trishna's Thirst: the market pays in Spirit too.
        if self.law_active(super::Law::Thirst) {
            self.gain_spirit(player, 1, events);
        }
        true
    }

    /// Dusk: fairs whose days are out close.
    pub(super) fn close_fairs(&mut self, events: &mut Vec<Event>) {
        let done: Vec<(i32, i32)> = self
            .fairs
            .iter()
            .filter(|(_, f)| f.until <= self.dusks)
            .map(|(&k, _)| k)
            .collect();
        for (x, y) in done {
            self.fairs.remove(&(x, y));
            events.push(Event::FairClosed {
                hex: Hex::new(x, y),
            });
        }
    }

    /// The dead that came up to a fair eat the feast: it ends, and its host
    /// has fed the dead once more.
    pub(super) fn devour_fairs(&mut self, events: &mut Vec<Event>) {
        let eaten: Vec<(Hex, Fair)> = self
            .fairs()
            .filter(|(h, _)| {
                self.mobs
                    .iter()
                    .any(|m| m.is_undead() && m.hex.unsigned_distance_to(*h) <= 1)
            })
            .collect();
        for (hex, fair) in eaten {
            self.fairs.remove(&(hex.x(), hex.y()));
            if let Some(n) = self.dead_feasts.get_mut(fair.host.0 as usize) {
                *n = n.saturating_add(1);
            }
            events.push(Event::FairDevoured {
                hex,
                host: fair.host,
            });
        }
    }

    /// The fair nearest `from` within `sight`, which the dead walk to first.
    pub(super) fn fair_in_sight(&self, from: Hex, sight: u32) -> Option<Hex> {
        self.fairs()
            .map(|(h, _)| h)
            .filter(|h| h.unsigned_distance_to(from) <= sight)
            .min_by_key(|h| (h.unsigned_distance_to(from), h.x(), h.y()))
    }
}
