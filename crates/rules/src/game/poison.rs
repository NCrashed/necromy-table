//! Poison (docs/design.md §20.1).
//!
//! Poison lies in stacks and has an element: that of the card that laid it.
//! Each stack costs one health as its bearer's turn starts, but poison never
//! takes the last one: only a battle or a damaging card finishes. A heal of
//! the element that quenches the poison takes it off; a heal of the element
//! that generates it feeds it a stack instead. A temple cleanses, and so
//! does death; but a body that fell poisoned grows no grove, it rises.
//! Under Zaga's Sentence poison is catching: each bite passes a stack to
//! every neighbour free of it, her Chosen aside. Wounds from the undead
//! and beasts fester (`MOB_POISON`).

use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId};
use crate::gods::Element;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poison {
    pub element: Element,
    pub stacks: u8,
}

/// What took a poison off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cure {
    /// A heal of the element that quenches it.
    Heal(Element),
    /// Ending the turn on a temple.
    Temple,
    /// A passed trial's Mending boon (§20.2).
    Trial,
}

impl Game {
    /// New poison adds to the old one and takes its element.
    pub(super) fn poison(
        &mut self,
        player: PlayerId,
        element: Element,
        stacks: u8,
        events: &mut Vec<Event>,
    ) {
        if !self.has(super::Feature::Poison) {
            return;
        }
        let champ = self.champ_mut(player);
        let stacks = champ.poison.map_or(0, |p| p.stacks).saturating_add(stacks);
        champ.poison = Some(Poison { element, stacks });
        events.push(Event::Poisoned {
            player,
            element,
            stacks,
        });
    }

    /// A heal of `element` meets the poison before it heals: the quenching
    /// element cures, the generating one feeds.
    pub(super) fn tend(
        &mut self,
        player: PlayerId,
        element: Option<Element>,
        events: &mut Vec<Event>,
    ) {
        let (Some(element), Some(poison)) = (element, self.champions[player.0 as usize].poison)
        else {
            return;
        };
        if element == poison.element.quenched_by() {
            self.cure(player, Cure::Heal(element), events);
        } else if element == poison.element.generated_by() {
            let stacks = poison.stacks.saturating_add(1);
            self.champ_mut(player).poison = Some(Poison { stacks, ..poison });
            events.push(Event::PoisonFed { player, stacks });
        }
    }

    /// A card heal: the poison first, then the health.
    pub(super) fn mend(
        &mut self,
        player: PlayerId,
        amount: u8,
        element: Option<Element>,
        events: &mut Vec<Event>,
    ) {
        self.tend(player, element, events);
        self.heal(player, amount, events);
    }

    pub(super) fn cure(&mut self, player: PlayerId, by: Cure, events: &mut Vec<Event>) {
        if self.champ_mut(player).poison.take().is_some() {
            events.push(Event::PoisonCured { player, by });
        }
    }

    /// As the turn starts: one health and one stack, never the last health.
    pub(super) fn bite_poison(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let champ = self.champ_mut(player);
        let Some(poison) = champ.poison else {
            return;
        };
        let bite = u8::from(champ.hp > 1);
        champ.hp -= bite;
        let stacks = poison.stacks - 1;
        champ.poison = (stacks > 0).then_some(Poison { stacks, ..poison });
        let hp = champ.hp;
        events.push(Event::PoisonBit {
            player,
            amount: bite,
            hp,
            stacks,
        });
        // Zaga's Sentence: the sick are shunned for a reason (§20.1).
        if self.law_active(super::Law::Sentence) {
            let at = self.hex_of(player);
            let near: Vec<PlayerId> = self
                .players()
                .filter(|&p| p != player && self.hex_of(p).unsigned_distance_to(at) == 1)
                .filter(|&p| self.champions[p.0 as usize].poison.is_none())
                .filter(|&p| !self.chosen(p, crate::gods::God::Zaga))
                .collect();
            for p in near {
                events.push(Event::Law {
                    law: super::Law::Sentence,
                    player: Some(p),
                    hex: None,
                });
                self.poison(p, poison.element, 1, events);
            }
        }
    }
}
