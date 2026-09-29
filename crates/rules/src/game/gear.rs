//! Worn items and the loot deck (docs/design.md §20.3).
//!
//! A champion wears one item per slot. A new one in a taken slot pushes the
//! old one off: it drops to the ground where they stand, for anyone who
//! steps there. A fallen champion drops one of theirs by their body. The
//! element that quenches an item's element breaks it: an Element face in
//! battle from a patron of that element, or a harmful card of it that gets
//! through. At a temple an item can be given to the temple's god.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError, TimeOfDay};
use crate::board::Terrain;
use crate::gods::{Element, God};
use crate::items::{ITEMS, ItemEffect, ItemId, Slot, When};
use crate::rng::Rng;

/// Separates the loot deck's shuffle from everything else drawn from the seed.
const LOOT_STREAM: u64 = 0x0100_7d3c;
/// What a god takes as an item is given at its temple.
pub const SACRIFICE: u8 = 3;

/// Where a champion got an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gain {
    /// Drawn from the loot deck (a trial, a story line).
    Loot,
    /// Picked up from the ground.
    Ground,
}

impl Game {
    /// The match's loot deck, top last, shuffled from the seed apart from
    /// the rules' own generator.
    pub(super) fn loot_deck(seed: u64) -> Vec<ItemId> {
        let mut deck: Vec<ItemId> = (0..ITEMS.len() as u16).map(ItemId).collect();
        Rng::derived(seed, &[LOOT_STREAM]).shuffle(&mut deck);
        deck
    }

    pub fn loot_len(&self) -> usize {
        self.loot.len()
    }

    /// Items lying on the board.
    pub fn ground_items(&self) -> &[(Hex, ItemId)] {
        &self.ground
    }

    pub fn gear(&self, player: PlayerId) -> [Option<ItemId>; 3] {
        self.champions[player.0 as usize].gear
    }

    fn worn(&self, player: PlayerId) -> impl Iterator<Item = ItemId> + '_ {
        self.champions[player.0 as usize].gear.into_iter().flatten()
    }

    fn god_of(item: ItemId) -> God {
        God::from_index(item.def().element.index())
    }

    /// An item's number now: one more while its god is in the light.
    pub fn item_power(&self, item: ItemId) -> u8 {
        let light = self.stage(Self::god_of(item)) == 0;
        if item.def().effect.scales() {
            1 + u8::from(light)
        } else {
            0
        }
    }

    /// The god of `item` is dark and takes its toll from the wearer.
    pub fn item_tolls(&self, player: PlayerId, item: ItemId) -> bool {
        let god = Self::god_of(item);
        self.stage(god) >= 2 && !self.chosen(player, god)
    }

    fn power_of(&self, player: PlayerId, effect: ItemEffect) -> u8 {
        self.worn(player)
            .filter(|i| i.def().effect == effect)
            .map(|i| self.item_power(i))
            .sum()
    }

    /// Dice a champion's weapon adds in battle.
    pub fn item_dice(&self, player: PlayerId, defending: bool) -> u8 {
        let day = self.time == TimeOfDay::Day;
        self.worn(player)
            .filter(|i| match i.def().effect {
                ItemEffect::Dice(When::Always) => true,
                ItemEffect::Dice(When::Attacking) => !defending,
                ItemEffect::Dice(When::Defending) => defending,
                ItemEffect::Dice(When::Day) => day,
                ItemEffect::Dice(When::Night) => !day,
                _ => false,
            })
            .map(|i| self.item_power(i))
            .sum()
    }

    pub fn item_shields(&self, player: PlayerId) -> u8 {
        self.power_of(player, ItemEffect::Shields)
    }

    pub fn item_trial_dice(&self, player: PlayerId) -> u8 {
        self.power_of(player, ItemEffect::TrialDice)
    }

    /// Cards a champion may hold: their Wits, and a seal.
    pub fn hand_limit(&self, player: PlayerId) -> usize {
        self.champions[player.0 as usize].hand_limit()
            + usize::from(self.power_of(player, ItemEffect::Hands))
    }

    pub(super) fn stride(&self, player: PlayerId) -> u32 {
        u32::from(self.power_of(player, ItemEffect::Stride))
    }

    /// Night hides them anywhere, as Maya's Manipulation does.
    pub(super) fn shaded(&self, player: PlayerId) -> bool {
        self.worn(player)
            .any(|i| i.def().effect == ItemEffect::Shade)
    }

    /// As the turn starts: what the items give, then what dark gods take.
    pub(super) fn gear_at_turn_start(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let items: Vec<ItemId> = self.worn(player).collect();
        for item in items {
            let n = self.item_power(item);
            let def = item.def();
            match def.effect {
                ItemEffect::Mend => {
                    let c = &self.champions[player.0 as usize];
                    if c.hp < c.body {
                        events.push(Event::ItemWorked { player, item });
                        self.heal(player, n, events);
                    }
                }
                ItemEffect::Wellspring => {
                    let c = &self.champions[player.0 as usize];
                    if c.spirit_points < c.spirit {
                        events.push(Event::ItemWorked { player, item });
                        self.gain_spirit(player, n, events);
                    }
                }
                ItemEffect::Hush => {
                    if self.threat(player) > 0 {
                        events.push(Event::ItemWorked { player, item });
                        self.add_threat(player, -(n as i8), events);
                    }
                }
                ItemEffect::Ward => {
                    events.push(Event::ItemWorked { player, item });
                    self.raise_ward(player, def.element, events);
                }
                _ => {}
            }
            if self.item_tolls(player, item) {
                self.take_toll(player, item, events);
            }
        }
    }

    /// A dark god takes its own from whoever wears its item, never a life.
    fn take_toll(&mut self, player: PlayerId, item: ItemId, events: &mut Vec<Event>) {
        events.push(Event::ItemToll { player, item });
        match Self::god_of(item) {
            God::Bhava => self.poison(player, Element::Wood, 1, events),
            God::Trishna => {
                if self.champions[player.0 as usize].hp > 1 {
                    self.damage(player, 1, events);
                }
            }
            God::Zaga => self.add_threat(player, 1, events),
            God::Ahamar => self.add_style(player, -1, super::StyleReason::Item, events),
            God::Maya => {
                let c = self.champ_mut(player);
                if c.spirit_points > 0 {
                    c.spirit_points -= 1;
                    let spirit = c.spirit_points;
                    events.push(Event::SpiritChanged { player, spirit });
                }
            }
        }
    }

    /// The top of the loot deck, worn at once. An empty deck gives nothing.
    pub(super) fn gain_loot(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        if !self.has(super::Feature::Loot) {
            return;
        }
        if let Some(item) = self.loot.pop() {
            self.equip(player, item, Gain::Loot, events);
        }
    }

    /// `item` goes into its slot; what was there drops where they stand.
    pub(super) fn equip(
        &mut self,
        player: PlayerId,
        item: ItemId,
        from: Gain,
        events: &mut Vec<Event>,
    ) {
        let slot = item.def().slot.index();
        let old = self.champ_mut(player).gear[slot].replace(item);
        events.push(Event::ItemGained { player, item, from });
        if let Some(old) = old {
            let hex = self.hex_of(player);
            self.ground.push((hex, old));
            events.push(Event::ItemDropped {
                player,
                item: old,
                hex,
            });
        }
    }

    /// A blow of `by` breaks the first worn item it quenches.
    pub(super) fn crack_item(&mut self, target: PlayerId, by: Element, events: &mut Vec<Event>) {
        let gear = self.champions[target.0 as usize].gear;
        let Some(slot) = gear
            .iter()
            .position(|i| i.is_some_and(|i| i.def().element.quenched_by() == by))
        else {
            return;
        };
        let item = self.champ_mut(target).gear[slot]
            .take()
            .expect("found in that slot");
        // Broken things go back under the loot deck, whole again one day.
        self.loot.insert(0, item);
        events.push(Event::ItemBroken {
            player: target,
            item,
            by,
        });
    }

    /// A fallen champion drops one of their items by their body.
    pub(super) fn drop_on_fall(&mut self, player: PlayerId, at: Hex, events: &mut Vec<Event>) {
        let worn: Vec<usize> = (0..3)
            .filter(|&s| self.champions[player.0 as usize].gear[s].is_some())
            .collect();
        let Some(&slot) = self.rng.pick(&worn) else {
            return;
        };
        let item = self.champ_mut(player).gear[slot]
            .take()
            .expect("picked a worn slot");
        self.ground.push((at, item));
        events.push(Event::ItemDropped {
            player,
            item,
            hex: at,
        });
    }

    /// Stepping onto items on the ground takes the first, and leaves what it
    /// pushed off in its place.
    pub(super) fn pick_up(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        if let Some(i) = self.ground.iter().position(|(h, _)| *h == hex) {
            let (_, item) = self.ground.remove(i);
            self.equip(player, item, Gain::Ground, events);
        }
    }

    /// The god of the temple `player` stands on, if they may give it `slot`.
    pub(super) fn check_sacrifice(&self, player: PlayerId, slot: Slot) -> Result<God, RuleError> {
        let tile = self
            .board
            .tile(self.hex_of(player))
            .ok_or(RuleError::OffBoard)?;
        let god = match (tile.terrain, tile.region) {
            (Terrain::Temple, Some(god)) => god,
            _ => return Err(RuleError::NotAtTemple),
        };
        if self.champions[player.0 as usize].gear[slot.index()].is_none() {
            return Err(RuleError::NothingWorn);
        }
        Ok(god)
    }

    /// Whether `player` could give the item in `slot` to a god right now.
    pub fn can_sacrifice(&self, player: PlayerId, slot: Slot) -> bool {
        self.free_to_act(player) && self.check_sacrifice(player, slot).is_ok()
    }

    /// Gives the item in `slot` to the temple's god: a great offering.
    pub(super) fn sacrifice(
        &mut self,
        player: PlayerId,
        slot: Slot,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let god = self.check_sacrifice(player, slot)?;
        let item = self.champ_mut(player).gear[slot.index()]
            .take()
            .expect("checked");
        self.loot.insert(0, item);
        events.push(Event::ItemSacrificed { player, item, god });
        self.first(player, super::Novelty::Sacrificed, events);
        self.offer(Some(player), god, SACRIFICE, events);
        Ok(())
    }
}
