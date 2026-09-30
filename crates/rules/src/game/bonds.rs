//! Where the world's mechanics meet (docs/design.md §21.8): what each
//! one gives day by day beyond its deed, and what neighbours do to each
//! other. The sandbox lives here more than in any one mechanic.
//!
//! - Stores feed: a hurt champion who wakes at dawn in a settlement of
//!   theirs with food in store eats one: a health and a Spirit.
//! - Roads carry: a burden weighs nothing on a turn begun on a road.
//! - A caravan pays: goods sold at a fair outside their own land are worth
//!   a Style more.
//! - A caravan is waylaid: whoever carries a burden onto a road beside a
//!   rival in hiding loses it to them.
//! - Vassals open the gate: a sworn ruler's militia let their lord pass.
//! - The dead at rest are honoured: a burial raises one's standing with
//!   the militia; a rival who never came to a duel loses it.
//! - A dragon's champion does not burn.
//! - Fire at a fair ends it; the dead and monsters go to fairs, and the
//!   dead to feuding settlements.

use hexx::Hex;

use super::{Event, Game, PlayerId};
use crate::board::Terrain;
use crate::features::Feature;

impl Game {
    /// Dawn: stores feed their owners standing in them.
    pub(super) fn stores_at_dawn(&mut self, events: &mut Vec<Event>) {
        if !self.has(Feature::Fields) {
            return;
        }
        for p in self.players().collect::<Vec<_>>() {
            let at = self.hex_of(p);
            let key = (at.x(), at.y());
            // Only the hurt eat: a feast's stores are not eaten by its host.
            let hurt = self.champion(p).is_some_and(|c| c.hp < c.body);
            if !hurt || !self.own_settlement(p, at) || self.stores.get(&key).is_none_or(|&f| f == 0)
            {
                continue;
            }
            if let Some(food) = self.stores.get_mut(&key) {
                *food -= 1;
            }
            events.push(Event::Ate { player: p, hex: at });
            self.heal(p, 1, events);
            self.gain_spirit(p, 1, events);
        }
    }

    /// A turn begun on a road: the burden weighs nothing.
    pub(super) fn carried_by_road(&self, player: PlayerId) -> bool {
        self.road(self.hex_of(player))
    }

    /// Goods of `god`'s land sold at a fair on `hex` in another land.
    pub(super) fn foreign_goods(&self, hex: Hex, god: crate::gods::God) -> bool {
        self.board
            .tile(hex)
            .and_then(|t| t.region)
            .is_some_and(|here| here != god)
    }

    /// A step onto `to`: a burden carried onto a road beside a rival in
    /// hiding is waylaid.
    pub(super) fn waylay(&mut self, player: PlayerId, to: Hex, events: &mut Vec<Event>) {
        if self.cargo(player).is_none() || !self.road(to) {
            return;
        }
        let robber = self
            .players()
            .filter(|&p| p != player && self.is_hidden(p) && self.cargo(p).is_none())
            .find(|&p| self.hex_of(p).unsigned_distance_to(to) == 1);
        if let Some(r) = robber {
            events.push(Event::Waylaid {
                robber: r,
                victim: player,
                hex: to,
            });
            self.reveal(r, super::stealth::RevealReason::Attacked, events);
            self.seize_cargo(r, player, events);
        }
    }

    /// A sworn ruler's militia open to their lord.
    pub(super) fn sworn_militia(&self, player: PlayerId, home: Hex) -> bool {
        self.ruler(home).is_some_and(|r| r.sworn == Some(player))
    }

    /// Fire reached a fair: it breaks up.
    pub(super) fn fire_at_fair(&mut self, hex: Hex, events: &mut Vec<Event>) {
        if self.fairs.remove(&(hex.x(), hex.y())).is_some() {
            events.push(Event::FairClosed { hex });
        }
    }

    /// A settlement whose ruler feuds, nearest `from` within `sight`: the
    /// dead smell the blood.
    pub(super) fn feud_in_sight(&self, from: Hex, sight: u32) -> Option<Hex> {
        self.rulers()
            .filter(|(h, r)| {
                r.feud.is_some()
                    && h.unsigned_distance_to(from) <= sight
                    && self
                        .board
                        .tile(*h)
                        .is_some_and(|t| t.terrain == Terrain::Settlement)
            })
            .map(|(h, _)| h)
            .min_by_key(|h| (h.unsigned_distance_to(from), h.x(), h.y()))
    }
}
