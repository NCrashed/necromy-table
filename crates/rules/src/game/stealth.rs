//! Stealth (docs/design.md §11.6).
//!
//! A hidden champion is invisible to rivals: `view_for` shows them where
//! they were last seen and their steps never reach rivals. The rules below
//! decide when they slip out of sight and when they are seen again; the
//! rest of the game treats them as not there (`occupant` skips them), so
//! nobody can target, attack or window them, and whoever walks into one
//! stumbles into an ambush.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, Law, PlayerId, TimeOfDay};
use crate::board::Terrain;

/// Why a hidden champion was seen again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevealReason {
    /// They struck from the shadow.
    Attacked,
    /// They aimed a card at a rival.
    Aimed,
    /// A settlement, temple or the Table: there are people there.
    Crowd,
    /// A turn ended with them next to a rival.
    Spotted,
    /// The royal guard came by.
    Guard,
    /// Dawn found them out of cover.
    Dawn,
    /// Someone walked into them.
    Stumbled,
    /// They stepped onto a trial: the gods watch it in the open.
    Trial,
}

impl Terrain {
    /// Night here hides whoever ends their turn in it.
    pub const fn gives_cover(self) -> bool {
        matches!(self, Terrain::Forest | Terrain::Grove | Terrain::Swamp)
    }

    /// People live or gather here: nobody stays hidden on it.
    pub const fn crowded(self) -> bool {
        matches!(self, Terrain::Settlement | Terrain::Temple | Terrain::Table)
    }
}

impl Game {
    pub fn is_hidden(&self, player: PlayerId) -> bool {
        self.champion(player).is_some_and(|c| c.hidden)
    }

    /// Anyone on `hex`, hidden or not. Only the server's placement uses it:
    /// two champions must never share a hex.
    pub(super) fn champion_at(&self, hex: Hex) -> Option<PlayerId> {
        self.champions
            .iter()
            .position(|c| c.hex == hex)
            .map(|i| PlayerId(i as u8))
    }

    /// A hidden champion on `hex`, if one lies there.
    pub(super) fn hidden_at(&self, hex: Hex) -> Option<PlayerId> {
        self.champion_at(hex).filter(|&p| self.is_hidden(p))
    }

    pub(super) fn hide(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        if !self.has(super::Feature::Stealth) {
            return;
        }
        // Ahamar's Exposure: in daylight nobody slips away (§5.3).
        if self.time == TimeOfDay::Day
            && self.law_active(Law::Exposure)
            && !self.chosen(player, crate::gods::God::Ahamar)
        {
            if !self.is_hidden(player) {
                events.push(Event::Law {
                    law: Law::Exposure,
                    player: Some(player),
                    hex: None,
                });
            }
            return;
        }
        let c = self.champ_mut(player);
        if c.hidden {
            return;
        }
        c.hidden = true;
        c.seen_at = c.hex;
        let hex = c.hex;
        events.push(Event::Hid { player, hex });
        self.first(player, super::Novelty::Hid, events);
        self.note_bet(player, super::wish::Bet::Hide);
    }

    pub(super) fn reveal(&mut self, player: PlayerId, why: RevealReason, events: &mut Vec<Event>) {
        let c = self.champ_mut(player);
        if !c.hidden {
            return;
        }
        c.hidden = false;
        c.seen_at = c.hex;
        let hex = c.hex;
        events.push(Event::Revealed { player, hex, why });
    }

    /// The end of `player`'s turn: night cover hides them; anyone hidden
    /// next to a rival is seen.
    pub(super) fn stealth_at_turn_end(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        if !self.has(super::Feature::Stealth) {
            return;
        }
        let at = self.hex_of(player);
        let terrain = self.board.tile(at).map(|t| t.terrain);
        let cover = terrain.is_some_and(|t| t.gives_cover());
        // At night under cover; Maya's Manipulation hides anywhere at night,
        // Bhava's Thicket keeps the woods dark by day (§5.3).
        let may_hide = match self.time {
            // A moss cloak does as much (§20.3).
            TimeOfDay::Night => cover || self.law_active(Law::Manipulation) || self.shaded(player),
            TimeOfDay::Day => {
                self.law_active(Law::Thicket)
                    && matches!(terrain, Some(Terrain::Forest | Terrain::Grove))
            }
        };
        if may_hide && !self.rival_near(player, at) {
            self.hide(player, events);
        }
        let spotted: Vec<PlayerId> = self
            .players()
            .filter(|&p| self.is_hidden(p) && self.rival_near(p, self.hex_of(p)))
            .collect();
        for p in spotted {
            self.reveal(p, RevealReason::Spotted, events);
        }
    }

    /// A visible rival of `player` stands next to `at`.
    fn rival_near(&self, player: PlayerId, at: Hex) -> bool {
        self.players().any(|q| {
            q != player && !self.is_hidden(q) && self.hex_of(q).unsigned_distance_to(at) <= 1
        })
    }

    /// Dawn drives out whoever is hidden out of cover.
    pub(super) fn stealth_at_dawn(&mut self, events: &mut Vec<Event>) {
        // Under Ahamar's Exposure the dawn finds everyone.
        let exposure = self.law_active(Law::Exposure);
        let exposed: Vec<PlayerId> = self
            .players()
            .filter(|&p| {
                self.is_hidden(p)
                    && (exposure
                        || !self
                            .board
                            .tile(self.hex_of(p))
                            .is_some_and(|t| t.terrain.gives_cover()))
            })
            .collect();
        for p in exposed {
            self.reveal(p, RevealReason::Dawn, events);
        }
    }

    /// The guard sees whoever hides next to it.
    pub(super) fn stealth_near_guard(&mut self, guard: Hex, events: &mut Vec<Event>) {
        let seen: Vec<PlayerId> = self
            .players()
            .filter(|&p| self.is_hidden(p) && self.hex_of(p).unsigned_distance_to(guard) <= 1)
            .collect();
        for p in seen {
            self.reveal(p, RevealReason::Guard, events);
        }
    }

    /// `mover` stepped or blinked towards `hex` where `hidden` lies: the move
    /// ends there, the hidden one is seen and strikes first (§11.6).
    pub(super) fn stumble(
        &mut self,
        mover: PlayerId,
        hidden: PlayerId,
        hex: Hex,
        events: &mut Vec<Event>,
    ) {
        events.push(Event::Stumbled { mover, hidden, hex });
        self.turns[mover.0 as usize].move_points = 0;
        self.reveal(hidden, RevealReason::Stumbled, events);
        // The one in hiding strikes first, even in the middle of their own
        // turn (§11.2): the mover waits for the battle.
        self.start_battle(mover, hidden, mover, 0, Some(hidden), events);
    }
}

impl Game {
    /// Rivals who may react to something `player` does at `at`: within
    /// reaction range and not lying hidden, in initiative after `player`.
    pub(super) fn watchers(&self, player: PlayerId, at: Hex) -> Vec<PlayerId> {
        self.initiative_after(player)
            .into_iter()
            .filter(|&p| {
                !self.is_hidden(p)
                    && self.hex_of(p).unsigned_distance_to(at) <= self.reaction_range()
            })
            .collect()
    }
}
