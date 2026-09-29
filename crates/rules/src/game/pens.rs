//! Pens and tethered beasts (docs/design.md §21.8).
//!
//! With pens in the world a settlement may take a pen for its building. A
//! champion in a pen of theirs tethers a tamed beast there: it no longer
//! follows them, and it stays whatever befalls them. A rival in the pen
//! may untie one and lead it away.

use hexx::Hex;

use super::buildings::Building;
use super::{Companion, Event, Game, PlayerId, RuleError};
use crate::gods::Element;

/// Elements of beasts in a pen for the Ark.
pub const ARK: usize = 5;

impl Game {
    /// Elements of the beasts tethered in the pen on `hex`, as a mask by
    /// `Element::index`.
    pub fn penned(&self, hex: Hex) -> u8 {
        self.pens.get(&(hex.x(), hex.y())).copied().unwrap_or(0)
    }

    fn in_pen(&self, player: PlayerId) -> Option<Hex> {
        let at = self.hex_of(player);
        (self.building(at) == Some(Building::Pen)).then_some(at)
    }

    /// Beasts `player` could tether where they stand: in a pen of theirs,
    /// of an element not there yet.
    pub fn tetherable(&self, player: PlayerId) -> Vec<Element> {
        let Some(pen) = self
            .in_pen(player)
            .filter(|&h| self.owner(h) == Some(player))
        else {
            return Vec::new();
        };
        let mask = self.penned(pen);
        let mut out: Vec<Element> = self
            .companions(player)
            .iter()
            .filter_map(|c| match c {
                Companion::Beast(e) if mask & (1 << e.index()) == 0 => Some(*e),
                _ => None,
            })
            .collect();
        out.dedup();
        out
    }

    /// Beasts `player` could untie and lead off: in a rival's pen.
    pub fn untetherable(&self, player: PlayerId) -> Vec<Element> {
        let Some(pen) = self
            .in_pen(player)
            .filter(|&h| self.owner(h) != Some(player))
        else {
            return Vec::new();
        };
        if self.companions(player).len() >= super::RETINUE {
            return Vec::new();
        }
        let mask = self.penned(pen);
        Element::ALL
            .into_iter()
            .filter(|e| mask & (1 << e.index()) != 0)
            .collect()
    }

    pub(super) fn check_tether(&self, player: PlayerId, element: Element) -> Result<(), RuleError> {
        if self.tetherable(player).contains(&element) {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    pub(super) fn tether(
        &mut self,
        player: PlayerId,
        element: Element,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_tether(player, element)?;
        let hex = self.hex_of(player);
        let champ = self.champ_mut(player);
        let i = champ
            .companions
            .iter()
            .position(|&c| c == Companion::Beast(element))
            .expect("checked");
        champ.companions.remove(i);
        *self.pens.entry((hex.x(), hex.y())).or_insert(0) |= 1 << element.index();
        events.push(Event::Tethered {
            player,
            hex,
            element,
        });
        Ok(())
    }

    pub(super) fn check_untether(
        &self,
        player: PlayerId,
        element: Element,
    ) -> Result<(), RuleError> {
        if self.untetherable(player).contains(&element) {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    pub(super) fn untether(
        &mut self,
        player: PlayerId,
        element: Element,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_untether(player, element)?;
        let hex = self.hex_of(player);
        if let Some(mask) = self.pens.get_mut(&(hex.x(), hex.y())) {
            *mask &= !(1 << element.index());
        }
        self.champ_mut(player)
            .companions
            .push(Companion::Beast(element));
        events.push(Event::Untethered {
            player,
            hex,
            element,
        });
        Ok(())
    }

    /// Of `player`'s pens, the fullest: its elements, and whether a shrine of
    /// theirs stands in its city or within two hexes.
    pub fn best_ark(&self, player: PlayerId) -> (usize, bool) {
        self.buildings()
            .filter(|&(h, b)| b == Building::Pen && self.owner(h) == Some(player))
            .map(|(h, _)| {
                let near: Vec<Hex> = self.city_of(h).into_iter().chain(h.range(2)).collect();
                let shrine = near.iter().any(|&c| {
                    matches!(self.building(c), Some(Building::Shrine(_)))
                        && self.owner(c) == Some(player)
                });
                (self.penned(h).count_ones() as usize, shrine)
            })
            .max()
            .unwrap_or((0, false))
    }
}
