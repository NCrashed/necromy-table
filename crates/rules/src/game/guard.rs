//! The royal guard, trained against necromancy (docs/design.md §6.5).
//!
//! When someone's Threat reaches `GUARD_THRESHOLD`, the guard leaves the
//! Table and walks towards the loudest champion during the world phase. Next
//! to them, it strikes with physical dice. The strike quiets them down.
//! With nobody loud enough, the guard stands down.
//!
//! The guard only blocks its hex for now; fighting back is a later step.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Fighter, Game, PlayerId};
use crate::gods::{Element, God};

/// Hexes the guard walks per world phase.
pub const GUARD_STEPS: u32 = 2;
/// Dice the guard throws.
pub const GUARD_DICE: u8 = 3;
/// Threat a strike takes off its target.
pub const GUARD_RELIEF: i8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guard {
    pub hex: Hex,
    pub target: PlayerId,
}

impl Game {
    pub fn guard(&self) -> Option<Guard> {
        self.guard
    }

    pub(super) fn guard_at(&self, hex: Hex) -> bool {
        self.guard.is_some_and(|g| g.hex == hex)
    }

    /// The loudest champion at or over the threshold; ties go by initiative.
    pub fn hunted(&self) -> Option<PlayerId> {
        let loudest = self.players().map(|p| self.threat(p)).max()?;
        if loudest < self.guard_threshold() {
            return None;
        }
        self.order
            .iter()
            .copied()
            .find(|&p| self.threat(p) == loudest)
    }

    pub(super) fn guard_phase(&mut self, events: &mut Vec<Event>) {
        let Some(target) = self.hunted() else {
            if let Some(g) = self.guard.take() {
                events.push(Event::GuardLeft { hex: g.hex });
            }
            return;
        };
        let mut guard = match self.guard {
            Some(g) => g,
            None => {
                let Some(hex) = self.nearest_free(Hex::ZERO) else {
                    return;
                };
                events.push(Event::GuardSpawned { hex, target });
                Guard { hex, target }
            }
        };
        guard.target = target;

        let goal = self.hex_of(target);
        for _ in 0..GUARD_STEPS {
            let here = guard.hex.unsigned_distance_to(goal);
            if here <= 1 {
                break;
            }
            let next = guard
                .hex
                .all_neighbors()
                .into_iter()
                .filter(|&h| self.board.contains(h) && self.champion_at(h).is_none())
                .filter(|&h| h.unsigned_distance_to(goal) < here)
                .min_by_key(|&h| (h.unsigned_distance_to(goal), h.x(), h.y()));
            let Some(next) = next else { break };
            events.push(Event::GuardMoved {
                from: guard.hex,
                to: next,
            });
            guard.hex = next;
        }
        self.guard = Some(guard);
        self.stealth_near_guard(guard.hex, events);

        if guard.hex.unsigned_distance_to(goal) <= 1 {
            self.guard_strike(target, events);
        }
    }

    fn guard_strike(&mut self, target: PlayerId, events: &mut Vec<Event>) {
        events.push(Event::GuardStruck { target });
        self.last_fight = self.round;
        self.battles += 1;
        self.offer(None, God::Trishna, 1, events);
        // Zaga's Sentence: the guard strikes harder (§5.3).
        let dice = GUARD_DICE
            + u8::from(self.law_active(super::Law::Sentence) && !self.chosen(target, God::Zaga));
        let g_faces = self.roll(Fighter::Guard, false, dice, Vec::new(), events);
        let count = self.dice_for(target, true);
        let t_faces = self.roll(Fighter::Champion(target), true, count, Vec::new(), events);
        // The guard fights with the kingdom's iron.
        self.element_breaks_ward(Element::Metal, target, &g_faces, events);
        let guard_score = self.score(&g_faces);
        let mut target_score = self.score(&t_faces);
        target_score.shields += self.item_shields(target);
        events.push(Event::GuardResolved {
            target,
            guard_score,
            target_score,
        });
        let hurt = guard_score.hits.saturating_sub(target_score.shields);
        if hurt > 0 {
            self.damage(target, hurt, events);
        }
        self.add_threat(target, -GUARD_RELIEF, events);
    }

    /// The closest hex to `from` with nobody on it.
    pub(super) fn nearest_free(&self, from: Hex) -> Option<Hex> {
        (0..=self.board.radius() * 2)
            .flat_map(|r| from.ring(r).collect::<Vec<_>>())
            .find(|&h| self.board.contains(h) && self.champion_at(h).is_none() && !self.guard_at(h))
    }
}
