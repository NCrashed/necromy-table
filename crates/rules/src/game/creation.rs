//! The world as players make it (docs/design.md §21): land rising at the
//! rim, land going into the mist and coming back out.

use hexx::Hex;

use super::{Event, Game};
use crate::board::Terrain;

#[cfg_attr(not(test), expect(dead_code, reason = "creation wishes come next"))]
impl Game {
    /// New land on `hex`, off the board and next to it.
    pub(super) fn raise_land(
        &mut self,
        hex: Hex,
        terrain: Terrain,
        events: &mut Vec<Event>,
    ) -> bool {
        if !self.board.raise(hex, terrain) {
            return false;
        }
        events.push(Event::LandRaised { hex, terrain });
        true
    }

    /// The land on `hex` goes into the mist, with whatever lay on it: a
    /// body, traps, a trial, items, a claim, the militia at home there.
    /// Never where someone stands, a mob walks or a temple is.
    pub(super) fn veil(&mut self, hex: Hex, events: &mut Vec<Event>) -> bool {
        if self.champion_at(hex).is_some() || self.mob_at(hex) || !self.board.veil(hex) {
            return false;
        }
        let key = (hex.x(), hex.y());
        self.traps.retain(|t| t.hex != hex);
        self.trials.retain(|t| t.hex != hex);
        self.ground.retain(|(h, _)| *h != hex);
        self.claims.remove(&key);
        self.militia.remove(&key);
        self.ruins.remove(&key);
        events.push(Event::TerrainChanged {
            hex,
            terrain: Terrain::Mist,
        });
        true
    }

    /// The mist on `hex` lifts, and the land is as it was.
    pub(super) fn unveil(&mut self, hex: Hex, events: &mut Vec<Event>) -> bool {
        let Some(terrain) = self.board.unveil(hex) else {
            return false;
        };
        events.push(Event::TerrainChanged { hex, terrain });
        true
    }
}
