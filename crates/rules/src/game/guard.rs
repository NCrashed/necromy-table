//! The royal guard, trained against necromancy (docs/design.md §6.5).
//!
//! When someone's Threat reaches `GUARD_THRESHOLD`, the guard leaves the
//! Table and walks towards the loudest champion during the world phase. Next
//! to them, it strikes with physical dice. The strike quiets them down.
//! With nobody loud enough, the guard stands down.
//!
//! It can be fought: stepping onto it attacks it, as a rival (§20.4). It
//! has `GUARD_HEALTH` for each time it comes out and does not heal; every
//! hit a champion lands on it counts, in their attack or in its strike.
//! Brought down, it leaves the board, and whoever felled it takes Style and
//! an item from the loot deck. While someone is still loud, a fresh guard
//! comes out of the Table in the next world phase.
//!
//! Trained against necromancy, it hews down an undead next to it on its
//! way whenever its quarry is not yet in reach.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Fighter, Game, PlayerId, WindowKind};
use crate::gods::{Element, God};

/// Hexes the guard walks per world phase.
pub const GUARD_STEPS: u32 = 2;
/// Dice the guard throws.
pub const GUARD_DICE: u8 = 3;
/// Threat a strike takes off its target.
pub const GUARD_RELIEF: i8 = 3;
/// Hits that bring the guard down; it does not heal.
pub const GUARD_HEALTH: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guard {
    pub hex: Hex,
    pub target: PlayerId,
    /// Hits it can still take.
    pub hp: u8,
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
                Guard {
                    hex,
                    target,
                    hp: GUARD_HEALTH,
                }
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
                .filter(|&h| {
                    self.board.contains(h)
                        && self.champion_at(h).is_none()
                        && self.mob_on(h).is_none()
                })
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
        } else if let Some(undead) = self
            .mobs
            .iter()
            .filter(|m| m.is_undead() && m.hex.unsigned_distance_to(guard.hex) <= 1)
            .map(|m| m.id)
            .min()
        {
            self.mobs.retain(|m| m.id != undead);
            events.push(Event::GuardHewed {
                hex: guard.hex,
                undead,
            });
        }
    }

    fn guard_strike(&mut self, target: PlayerId, events: &mut Vec<Event>) {
        events.push(Event::GuardStruck { target });
        self.last_fight = self.round;
        self.battles += 1;
        self.offer(None, God::Trishna, 1, events);
        let dice = self.guard_dice(target);
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
        // What the target lands, the guard takes.
        let back = target_score.hits.saturating_sub(guard_score.shields);
        if back > 0 {
            self.hurt_guard(target, back, events);
        }
        self.add_threat(target, -GUARD_RELIEF, events);
    }

    /// Dice the guard throws against `foe`. Zaga's Sentence: one more (§5.3).
    pub fn guard_dice(&self, foe: PlayerId) -> u8 {
        GUARD_DICE + u8::from(self.law_active(super::Law::Sentence) && !self.chosen(foe, God::Zaga))
    }

    /// `attacker` stepped onto the guard: they pay `cost`, then pick cards
    /// to burn; the guard burns none.
    pub(super) fn start_guard_battle(
        &mut self,
        attacker: PlayerId,
        cost: u32,
        events: &mut Vec<Event>,
    ) {
        self.turns[attacker.0 as usize].move_points -= cost;
        if self.is_hidden(attacker) {
            self.reveal(attacker, super::RevealReason::Attacked, events);
        }
        events.push(Event::GuardAttacked { attacker });
        self.last_fight = self.round;
        // Attacking is loud (§6.5), the crown's iron above all.
        self.add_threat(attacker, 1, events);
        self.record_deed(attacker, super::style::Deed::Attacked);
        self.record_deed(attacker, super::style::Deed::Fought);
        self.open_window(
            attacker,
            WindowKind::GuardBattle { attacker },
            vec![attacker],
            None,
            None,
            events,
        );
    }

    /// The attacker's burned faces and throw against the guard's.
    pub(super) fn resolve_guard_battle(
        &mut self,
        attacker: PlayerId,
        burned: Vec<crate::cards::CardId>,
        events: &mut Vec<Event>,
    ) {
        if self.guard.is_none() {
            self.discard.extend(burned);
            return;
        }
        self.battles += 1;
        self.offer(None, God::Trishna, 1, events);
        let a_faces = self.throw_side(attacker, false, burned, events);
        let dice = self.guard_dice(attacker);
        let g_faces = self.roll(Fighter::Guard, true, dice, Vec::new(), events);
        self.element_breaks_ward(Element::Metal, attacker, &g_faces, events);
        let guard_score = self.score(&g_faces);
        let mut target_score = self.score(&a_faces);
        target_score.shields += self.item_shields(attacker);
        events.push(Event::GuardResolved {
            target: attacker,
            guard_score,
            target_score,
        });
        let hurt = guard_score.hits.saturating_sub(target_score.shields);
        if hurt > 0 {
            self.damage(attacker, hurt, events);
        }
        let dealt = target_score.hits.saturating_sub(guard_score.shields);
        if dealt > 0 {
            self.hurt_guard(attacker, dealt, events);
        }
    }

    /// The guard takes `amount` from `by`; at nothing left it falls, and
    /// `by` takes Style for a won battle, doubled, and an item (§20.3).
    pub(super) fn hurt_guard(&mut self, by: PlayerId, amount: u8, events: &mut Vec<Event>) {
        let Some(mut guard) = self.guard else {
            return;
        };
        guard.hp = guard.hp.saturating_sub(amount);
        events.push(Event::GuardHurt {
            amount,
            hp: guard.hp,
        });
        if guard.hp > 0 {
            self.guard = Some(guard);
            return;
        }
        self.guard = None;
        events.push(Event::GuardFell { hex: guard.hex, by });
        self.record_deed(by, super::style::Deed::Won);
        let style = 2 * i16::from(self.taste.battle);
        self.add_style(by, style, super::StyleReason::Battle, events);
        self.gain_loot(by, events);
    }

    /// The closest hex to `from` with nobody on it.
    pub(super) fn nearest_free(&self, from: Hex) -> Option<Hex> {
        (0..=self.board.radius() * 2)
            .flat_map(|r| from.ring(r).collect::<Vec<_>>())
            .find(|&h| self.board.contains(h) && self.champion_at(h).is_none() && !self.mob_at(h))
    }
}
