//! Mobs and factions (docs/design.md §20.4): the undead and the militia.
//!
//! A body nobody tends is a clock with three ends: a grove where one can
//! grow (Bhava), what a champion's card makes of it, or a restless spirit
//! where no grove takes: it rises as one of the undead. The lands of the
//! yin gods, Maya and Zaga, keep their dead restless on any ground unless
//! their god is in the light, and sooner when it is dark.
//!
//! The undead walk in the world phase, a hex at a time, towards the living
//! near them: a champion they strike, a settlement they lay waste unless its
//! militia holds. Every settlement keeps a militia; it kills an undead next
//! to it, losing a man each time, and fills its ranks again at dawn.
//!
//! The militia remember what each champion does near them (`standing`):
//! laying the dead to rest and cutting the undead down wins them over;
//! burning the dead, pressing them into a legion, fighting at their gates
//! turns them away. Friends are healed in their settlements; the unwelcome
//! cannot take one and are beaten when they linger.
//!
//! Undead are fought like the guard: a step onto one attacks it, and it
//! throws its own dice. In the world phase it throws first, unasked.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::style::Deed;
use super::{Event, Fighter, Game, PlayerId, RevealReason, WindowKind};
use crate::cards::CardId;
use crate::gods::{Element, God};

/// Rounds a body lies before it rises where no grove can grow.
pub const UNDEAD_AGE: u8 = 2;
/// In the land of a dark Maya or Zaga the dead do not wait as long.
pub const UNDEAD_AGE_DARK: u8 = 1;
pub const UNDEAD_HEALTH: u8 = 2;
pub const UNDEAD_DICE: u8 = 2;
/// Hexes an undead sees the living from.
pub const UNDEAD_SIGHT: u32 = 4;
/// More stay in their graves: the world phase must stay short (§20.4).
pub const MAX_UNDEAD: usize = 6;
/// Separates undead throws from battle and trial throws.
const MOB_STREAM: u64 = 0x0000_dead;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Undead {
    pub id: u32,
    pub hex: Hex,
    pub hp: u8,
}

impl Game {
    pub fn undead(&self) -> &[Undead] {
        &self.undead
    }

    pub fn undead_at(&self, hex: Hex) -> Option<&Undead> {
        self.undead.iter().find(|u| u.hex == hex)
    }

    /// The royal guard, an undead or a settlement's militia holds `hex`.
    pub(super) fn mob_at(&self, hex: Hex) -> bool {
        self.guard_at(hex) || self.undead_at(hex).is_some() || self.militia_at(hex).is_some()
    }

    /// The age at which the body on `hex` rises, if it ever does.
    fn rising_age(&self, hex: Hex) -> Option<u8> {
        let tile = self.board.tile(hex)?;
        // The yin gods' lands keep their dead restless unless their god is
        // in the light: sooner still when it is dark.
        let yin_stage = tile
            .region
            .filter(|g| matches!(g, God::Maya | God::Zaga))
            .map(|g| self.stage(g));
        match yin_stage {
            Some(2..) => Some(UNDEAD_AGE_DARK),
            Some(1) => Some(UNDEAD_AGE),
            _ if !tile.terrain.can_grow_grove() => Some(UNDEAD_AGE),
            _ => None,
        }
    }

    /// World phase, as the bodies age: those that are due rise.
    pub(super) fn raise_dead(&mut self, events: &mut Vec<Event>) {
        let due: Vec<Hex> = self
            .board
            .corpses()
            .filter(|&(hex, c)| self.rising_age(hex).is_some_and(|age| c.age >= age))
            .map(|(hex, _)| hex)
            .collect();
        for hex in due {
            if self.undead.len() >= MAX_UNDEAD
                || self.champion_at(hex).is_some()
                || self.mob_at(hex)
            {
                continue;
            }
            if let Some(tile) = self.board.tile_mut(hex) {
                tile.corpse = None;
            }
            self.next_mob += 1;
            let undead = Undead {
                id: self.next_mob,
                hex,
                hp: UNDEAD_HEALTH,
            };
            self.undead.push(undead);
            events.push(Event::UndeadRose { undead });
        }
    }

    /// World phase: the undead act, the militia strike back, then militia
    /// away from home go back.
    ///
    /// At a gate the two sides trade blows: each undead next to militia
    /// knocks a man down, then the militia wound one undead that did not
    /// just come up (an undead takes two). A lone undead left alone takes a
    /// settlement in a few phases unless a dawn brings a man back in time
    /// or a champion comes to help; two at once are faster.
    pub(super) fn mob_phase(&mut self, events: &mut Vec<Event>) {
        // Those that come up to a gate this phase are not reached by its
        // militia until the next.
        let mut arrived: Vec<u32> = Vec::new();
        let ids: Vec<u32> = self.undead.iter().map(|u| u.id).collect();
        for id in ids {
            let Some(u) = self.undead.iter().find(|u| u.id == id).copied() else {
                continue;
            };
            // A champion next to it, in sight: it strikes.
            let prey = self
                .players()
                .filter(|&p| !self.is_hidden(p))
                .filter(|&p| self.hex_of(p).unsigned_distance_to(u.hex) <= 1)
                .min_by_key(|&p| (self.champions[p.0 as usize].hp, p.0));
            if let Some(p) = prey {
                self.undead_strike(id, p, events);
                continue;
            }
            // Militia next to it: it knocks a man down.
            let guards = u
                .hex
                .all_neighbors()
                .into_iter()
                .find_map(|h| self.militia_at(h));
            if let Some(home) = guards {
                events.push(Event::UndeadHitMilitia { id, home });
                self.hurt_militia(home, 1, events);
                continue;
            }
            // Standing in a settlement no militia stands on (none left, or
            // the men that came back at dawn could not get past it): it lays
            // it waste.
            if self.militia(u.hex).is_some() && self.militia_at(u.hex).is_none() {
                self.ruin(u.hex, events);
                continue;
            }
            self.undead_walk(id, events);
            arrived.push(id);
        }

        let posts: Vec<(Hex, Hex)> = self
            .militias()
            .filter_map(|(home, m)| m.at.filter(|_| m.men > 0).map(|at| (home, at)))
            .collect();
        for (_, at) in posts {
            let Some(id) = self
                .undead
                .iter()
                .filter(|u| u.hex.unsigned_distance_to(at) <= 1 && !arrived.contains(&u.id))
                .min_by_key(|u| (u.hp, u.id))
                .map(|u| u.id)
            else {
                continue;
            };
            let Some(u) = self.undead.iter_mut().find(|u| u.id == id) else {
                continue;
            };
            u.hp = u.hp.saturating_sub(1);
            let hp = u.hp;
            events.push(Event::UndeadHurt { id, amount: 1, hp });
            if hp == 0 {
                self.undead.retain(|u| u.id != id);
                events.push(Event::MilitiaStruck {
                    hex: at,
                    undead: id,
                });
            }
        }

        self.militia_go_home(events);
    }

    /// One step towards the nearest living thing it sees: a champion or a
    /// settlement.
    fn undead_walk(&mut self, id: u32, events: &mut Vec<Event>) {
        let Some(u) = self.undead.iter().find(|u| u.id == id).copied() else {
            return;
        };
        // The dead are drawn to where the living gather: a settlement in
        // sight first, a champion only when none is near.
        let nearest = |hexes: &mut dyn Iterator<Item = Hex>| {
            hexes
                .filter(|h| h.unsigned_distance_to(u.hex) <= UNDEAD_SIGHT)
                .min_by_key(|h| (h.unsigned_distance_to(u.hex), h.x(), h.y()))
        };
        let settlement = nearest(&mut self.militias().map(|(home, _)| home));
        let champion = nearest(
            &mut self
                .players()
                .filter(|&p| !self.is_hidden(p))
                .map(|p| self.hex_of(p)),
        );
        let Some(goal) = settlement.or(champion) else {
            return;
        };
        let here = u.hex.unsigned_distance_to(goal);
        let next = u
            .hex
            .all_neighbors()
            .into_iter()
            .filter(|&h| self.board.contains(h))
            .filter(|&h| self.champion_at(h).is_none() && !self.mob_at(h))
            .filter(|&h| h.unsigned_distance_to(goal) < here)
            .min_by_key(|&h| (h.unsigned_distance_to(goal), h.x(), h.y()));
        if let Some(next) = next {
            if let Some(u) = self.undead.iter_mut().find(|u| u.id == id) {
                u.hex = next;
            }
            events.push(Event::UndeadMoved {
                id,
                from: u.hex,
                to: next,
            });
            // The living near them notice the dead walking by.
            let near: Vec<PlayerId> = self
                .players()
                .filter(|&p| self.is_hidden(p) && self.hex_of(p).unsigned_distance_to(next) <= 1)
                .collect();
            for p in near {
                self.reveal(p, RevealReason::Stumbled, events);
            }
        }
    }

    fn undead_dice_label(&mut self, id: u32, defending: bool) -> [u64; 4] {
        self.mob_throws += 1;
        [
            MOB_STREAM,
            u64::from(id),
            self.mob_throws,
            u64::from(defending),
        ]
    }

    /// World phase: an undead strikes a champion next to it, unasked.
    fn undead_strike(&mut self, id: u32, target: PlayerId, events: &mut Vec<Event>) {
        events.push(Event::UndeadStruck { id, target });
        self.last_fight = self.round;
        let label = self.undead_dice_label(id, false);
        let u_faces = self.roll_with(Fighter::Undead(id), &label, UNDEAD_DICE, Vec::new(), events);
        let count = self.dice_for(target, true);
        let label = self.undead_dice_label(id, true);
        let t_faces = self.roll_with(Fighter::Champion(target), &label, count, Vec::new(), events);
        self.settle_undead_fight(id, target, &u_faces, &t_faces, false, events);
    }

    /// `attacker` stepped onto an undead: they pay `cost`, then pick cards
    /// to burn; the dead burn none.
    pub(super) fn start_undead_battle(
        &mut self,
        attacker: PlayerId,
        id: u32,
        cost: u32,
        events: &mut Vec<Event>,
    ) {
        self.turns[attacker.0 as usize].move_points -= cost;
        if self.is_hidden(attacker) {
            self.reveal(attacker, RevealReason::Attacked, events);
        }
        events.push(Event::UndeadAttacked { attacker, id });
        self.last_fight = self.round;
        self.record_deed(attacker, Deed::Fought);
        self.open_window(
            attacker,
            WindowKind::UndeadBattle { attacker, id },
            vec![attacker],
            None,
            None,
            events,
        );
    }

    pub(super) fn resolve_undead_battle(
        &mut self,
        attacker: PlayerId,
        id: u32,
        burned: Vec<CardId>,
        events: &mut Vec<Event>,
    ) {
        if self.undead.iter().all(|u| u.id != id) {
            self.discard.extend(burned);
            return;
        }
        self.battles += 1;
        let a_faces = self.throw_side(attacker, false, burned, events);
        let label = self.undead_dice_label(id, true);
        let u_faces = self.roll_with(Fighter::Undead(id), &label, UNDEAD_DICE, Vec::new(), events);
        self.settle_undead_fight(id, attacker, &u_faces, &a_faces, true, events);
    }

    /// Both sides take what got past the other's shields.
    fn settle_undead_fight(
        &mut self,
        id: u32,
        champion: PlayerId,
        u_faces: &[necromy_dice::Face],
        c_faces: &[necromy_dice::Face],
        champion_attacked: bool,
        events: &mut Vec<Event>,
    ) {
        // The dead are Maya's: their Element face is water's.
        self.element_breaks_ward(Element::Water, champion, u_faces, events);
        let undead_score = self.score(u_faces);
        let mut champion_score = self.score(c_faces);
        champion_score.shields += self.item_shields(champion);
        events.push(Event::UndeadResolved {
            id,
            champion,
            champion_attacked,
            undead_score,
            champion_score,
        });
        let hurt = undead_score.hits.saturating_sub(champion_score.shields);
        if hurt > 0 {
            self.damage(champion, hurt, events);
        }
        let dealt = champion_score.hits.saturating_sub(undead_score.shields);
        if dealt > 0 {
            self.hurt_undead(id, champion, dealt, events);
        }
    }

    /// An undead takes `amount`; at nothing left it is laid to rest, and
    /// whoever did it rises in the militia's eyes, now and then with loot.
    fn hurt_undead(&mut self, id: u32, by: PlayerId, amount: u8, events: &mut Vec<Event>) {
        let Some(u) = self.undead.iter_mut().find(|u| u.id == id) else {
            return;
        };
        u.hp = u.hp.saturating_sub(amount);
        let (hp, hex) = (u.hp, u.hex);
        events.push(Event::UndeadHurt { id, amount, hp });
        if hp > 0 {
            return;
        }
        self.undead.retain(|u| u.id != id);
        events.push(Event::UndeadFell { id, hex, by });
        self.record_deed(by, Deed::Won);
        self.shift_standing(by, 1, events);
        // One in three carries something worth taking (§20.3).
        if self.rng.below(3) == 0 {
            self.gain_loot(by, events);
        }
    }
}
