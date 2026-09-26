//! The gods as shared world state (docs/design.md §5).
//!
//! Every god has one stage, shared by all players: 0 is its light face, 2 its
//! dark one. Offerings build pressure; at dusk enough pressure darkens the
//! god and enough relief lightens it. Feeding a god also relieves the god it
//! quenches on the ring, so serving Maya (water) cools Trishna (fire).
//! Trishna gains pressure every dusk on her own: left alone, the world slides
//! towards Devouring.
//!
//! Each player also gathers favour with each god; the favour vector on the
//! pentagram is what victory conditions will read (§10).

use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId};
use crate::gods::{Element, God};

pub const STAGES: u8 = 3;
/// Pressure at which a stage shifts at dusk, either way.
pub const STAGE_THRESHOLD: i8 = 3;
/// Trishna's own pull every dusk.
pub const TRISHNA_DRIFT: i8 = 1;

/// Pressure and stage of the five gods, indexed by `God::index`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pantheon {
    pub stages: [u8; 5],
    pub pressure: [i8; 5],
}

impl Game {
    pub fn stage(&self, god: God) -> u8 {
        self.pantheon.stages[god.index()]
    }

    pub fn pressure(&self, god: God) -> i8 {
        self.pantheon.pressure[god.index()]
    }

    pub fn favor(&self, player: PlayerId, god: God) -> u16 {
        self.favor
            .get(player.0 as usize)
            .map_or(0, |f| f[god.index()])
    }

    /// Where a player stands on the pentagram: the favour-weighted sum of the
    /// five unit vectors, divided by total favour. Its length (0..1) is how
    /// fanatical, its angle whom they serve; serving all equally is the centre.
    /// Floats: for display and conditions, never fed back into the state.
    pub fn favor_vector(&self, player: PlayerId) -> [f32; 2] {
        let total: u16 = God::ALL.iter().map(|&g| self.favor(player, g)).sum();
        if total == 0 {
            return [0.0, 0.0];
        }
        let mut v = [0.0f32; 2];
        for god in God::ALL {
            let angle = (72.0 * god.index() as f32).to_radians();
            let w = f32::from(self.favor(player, god)) / f32::from(total);
            v[0] += w * angle.cos();
            v[1] += w * angle.sin();
        }
        v
    }

    /// How a god's stage bends its own cards' numbers: its light side helps
    /// more and hurts less, its dark side the other way round.
    pub fn stage_shift(&self, element: Option<Element>, harmful: bool) -> i8 {
        let Some(element) = element else {
            return 0;
        };
        match (self.stage(God::from_index(element.index())), harmful) {
            (0, false) | (2, true) => 1,
            (0, true) | (2, false) => -1,
            _ => 0,
        }
    }

    /// Something was given to `god`: by a player, or by the world itself.
    pub(super) fn offer(
        &mut self,
        player: Option<PlayerId>,
        god: God,
        amount: u8,
        events: &mut Vec<Event>,
    ) {
        if amount == 0 {
            return;
        }
        let a = amount as i8;
        let fed = god.index();
        let relieved = god.element().quenches().index();
        self.pantheon.pressure[fed] = self.pantheon.pressure[fed].saturating_add(a);
        self.pantheon.pressure[relieved] = self.pantheon.pressure[relieved].saturating_sub(a);
        if let Some(p) = player {
            self.favor[p.0 as usize][fed] += u16::from(amount);
        }
        events.push(Event::Offered {
            player,
            god,
            amount,
        });
    }

    /// Dusk: Trishna pulls, then every god whose pressure crossed the
    /// threshold moves one stage and starts over.
    pub(super) fn dusk(&mut self, events: &mut Vec<Event>) {
        let t = God::Trishna.index();
        self.pantheon.pressure[t] = self.pantheon.pressure[t].saturating_add(TRISHNA_DRIFT);
        for god in God::ALL {
            let i = god.index();
            let (p, s) = (self.pantheon.pressure[i], self.pantheon.stages[i]);
            let next = if p >= STAGE_THRESHOLD && s + 1 < STAGES {
                s + 1
            } else if p <= -STAGE_THRESHOLD && s > 0 {
                s - 1
            } else {
                continue;
            };
            self.pantheon.stages[i] = next;
            self.pantheon.pressure[i] = 0;
            events.push(Event::StageChanged { god, stage: next });
        }
    }
}

impl Game {
    /// «Пир урожая»: what a feast becomes depends on Trishna's stage.
    pub(super) fn feast(&mut self, caster: PlayerId, amount: u8, events: &mut Vec<Event>) {
        let at = self.hex_of(caster);
        let near: Vec<PlayerId> = self
            .players()
            .filter(|&p| self.hex_of(p).unsigned_distance_to(at) <= 1)
            .collect();
        match self.stage(God::Trishna) {
            // Generosity: everyone at the table eats.
            0 => {
                for p in near {
                    self.heal(p, amount, events);
                }
            }
            // Thirst: the host eats; the guests pay.
            1 => {
                self.heal(caster, amount + 1, events);
                for p in near.into_iter().filter(|&p| p != caster) {
                    if self.pierce(p, Some(Element::Fire), events) {
                        self.damage(p, 1, events);
                    }
                }
            }
            // Devouring: ash instead of gifts.
            _ => {
                let bodies: Vec<_> = self
                    .board
                    .corpses()
                    .map(|(h, _)| h)
                    .filter(|h| h.unsigned_distance_to(at) <= 1)
                    .collect();
                for hex in &bodies {
                    if let Some(tile) = self.board.tile_mut(*hex) {
                        tile.corpse = None;
                    }
                    events.push(Event::CorpseTaken { hex: *hex });
                }
                self.gain_spirit(caster, bodies.len() as u8, events);
                self.damage(caster, 1, events);
            }
        }
    }
}
