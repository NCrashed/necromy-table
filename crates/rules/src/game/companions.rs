//! Companions (docs/design.md §21.8): who follows a champion.
//!
//! With companions in the world, a champion next to a beast may tame it
//! for Spirit; with the legion, an undead next to them may be written into
//! it. A companion has no hex of its own: it walks with its champion and
//! adds a die to their battles (at most `COMPANION_DICE`). Whoever wins a
//! battle takes one of the loser's; a champion who falls loses them all,
//! the undead rising again where they fell.

use serde::{Deserialize, Serialize};

use super::mobs::{Mob, MobKind};
use super::{Event, Game, PlayerId, RuleError};
use crate::features::Feature;

/// Who follows a champion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Companion {
    /// A tamed beast of Bhava's woods.
    Beast,
    /// An undead of the legion.
    Undead,
}

/// Most companions one champion leads.
pub const RETINUE: usize = 5;
/// Most dice companions add to a battle.
pub const COMPANION_DICE: u8 = 2;
/// Spirit to tame a beast.
pub const TAME_SPIRIT: u8 = 2;
/// Spirit to write an undead into the legion.
pub const ENLIST_SPIRIT: u8 = 2;

impl Game {
    pub fn companions(&self, player: PlayerId) -> &[Companion] {
        self.champion(player)
            .map_or(&[][..], |c| c.companions.as_slice())
    }

    /// Dice the companions add to `player`'s battles.
    pub(super) fn companion_dice(&self, player: PlayerId) -> u8 {
        (self.companions(player).len() as u8).min(COMPANION_DICE)
    }

    /// What `mob` would become following `player`, and its price, if it
    /// may: next to them, of a kind the world lets follow.
    pub fn recruit(&self, player: PlayerId, mob: &Mob) -> Option<(Companion, u8)> {
        let near = self.hex_of(player).unsigned_distance_to(mob.hex) <= 1;
        if !near || self.companions(player).len() >= RETINUE {
            return None;
        }
        match mob.kind {
            MobKind::Beast { .. } if self.has(Feature::Companions) => {
                Some((Companion::Beast, TAME_SPIRIT))
            }
            MobKind::Undead if self.has(Feature::Legion) => {
                Some((Companion::Undead, ENLIST_SPIRIT))
            }
            _ => None,
        }
    }

    /// The mobs `player` could take into their retinue now.
    pub fn recruitable(&self, player: PlayerId) -> Vec<u32> {
        let spirit = self.champion(player).map_or(0, |c| c.spirit_points);
        self.mobs
            .iter()
            .filter(|m| {
                self.recruit(player, m)
                    .is_some_and(|(_, cost)| cost <= spirit)
            })
            .map(|m| m.id)
            .collect()
    }

    pub(super) fn check_recruit(
        &self,
        player: PlayerId,
        id: u32,
    ) -> Result<(Companion, u8), RuleError> {
        let mob = self
            .mobs
            .iter()
            .find(|m| m.id == id)
            .ok_or(RuleError::CannotRecruit)?;
        let (companion, cost) = self.recruit(player, mob).ok_or(RuleError::CannotRecruit)?;
        let have = self.champions[player.0 as usize].spirit_points;
        if have < cost {
            return Err(RuleError::NotEnoughSpirit { need: cost, have });
        }
        Ok((companion, cost))
    }

    /// The mob leaves the board and follows `player`.
    pub(super) fn take_companion(
        &mut self,
        player: PlayerId,
        id: u32,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let (companion, cost) = self.check_recruit(player, id)?;
        let champ = self.champ_mut(player);
        champ.spirit_points -= cost;
        let spirit = champ.spirit_points;
        champ.companions.push(companion);
        events.push(Event::SpiritChanged { player, spirit });
        self.mobs.retain(|m| m.id != id);
        events.push(Event::MobLeft { id });
        events.push(Event::CompanionJoined { player, companion });
        let novelty = match companion {
            Companion::Beast => super::Novelty::Tamed,
            Companion::Undead => super::Novelty::Enlisted,
        };
        self.first(player, novelty, events);
        Ok(())
    }

    /// A battle won: the winner takes one of the loser's companions, if
    /// there is room in their retinue.
    pub(super) fn seize_companion(
        &mut self,
        winner: PlayerId,
        loser: PlayerId,
        events: &mut Vec<Event>,
    ) {
        if self.companions(winner).len() >= RETINUE {
            return;
        }
        let Some(companion) = self.champ_mut(loser).companions.pop() else {
            return;
        };
        self.champ_mut(winner).companions.push(companion);
        events.push(Event::CompanionSeized {
            player: winner,
            from: loser,
            companion,
        });
    }

    /// A champion fell on `at`: the companions scatter, the undead rising
    /// there again if the hex is free.
    pub(super) fn scatter_companions(
        &mut self,
        player: PlayerId,
        at: hexx::Hex,
        events: &mut Vec<Event>,
    ) {
        let gone = std::mem::take(&mut self.champ_mut(player).companions);
        if gone.is_empty() {
            return;
        }
        events.push(Event::CompanionsScattered {
            player,
            count: gone.len() as u8,
        });
        if gone.contains(&Companion::Undead) && self.has(Feature::Undead) && !self.mob_at(at) {
            self.next_mob += 1;
            let mob = Mob {
                id: self.next_mob,
                kind: MobKind::Undead,
                hex: at,
                hp: MobKind::Undead.health(),
            };
            self.mobs.push(mob);
            events.push(Event::MobAppeared { mob });
        }
    }
}
