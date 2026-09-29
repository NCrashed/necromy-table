//! Items and the loot deck (docs/design.md §20.3).
//!
//! An item is worn in one of three slots and does one thing, named by a
//! keyword like a card. It has an element: the one element that quenches it
//! breaks it, as it breaks a ward (§4), so no item goes unanswered. It reads
//! its god's stage: stronger in the light one, and in the dark one the god
//! takes a toll from whoever wears it, every turn.
//!
//! Items never enter the card deck. They come from the loot deck: for a
//! passed trial, a story line done, later for monsters killed and gods'
//! gifts. Each item exists once per match.

use serde::{Deserialize, Serialize};

use crate::gods::Element;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ItemId(pub u16);

impl ItemId {
    pub fn def(self) -> &'static ItemDef {
        &ITEMS[self.0 as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Slot {
    Weapon,
    Armour,
    Relic,
}

impl Slot {
    pub const ALL: [Slot; 3] = [Slot::Weapon, Slot::Armour, Slot::Relic];

    pub const fn index(self) -> usize {
        match self {
            Slot::Weapon => 0,
            Slot::Armour => 1,
            Slot::Relic => 2,
        }
    }
}

/// When a weapon's die counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum When {
    Always,
    Attacking,
    Defending,
    Day,
    Night,
}

/// What an item does; numbers are the middle stage's, one more in the light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemEffect {
    /// More dice in battle, when it applies.
    Dice(When),
    /// Shields added to every battle's score.
    Shields,
    /// Health back as the turn starts.
    Mend,
    /// Spirit back as the turn starts.
    Wellspring,
    /// Threat off as the turn starts.
    Hush,
    /// A bigger hand.
    Hands,
    /// More move points each turn.
    Stride,
    /// More dice in trials (§20.2).
    TrialDice,
    /// Night hides the wearer on any ground, as Maya's Manipulation does.
    Shade,
    /// A ward of the item's element as the turn starts.
    Ward,
}

impl ItemEffect {
    /// Has a number the god's stage bends.
    pub const fn scales(self) -> bool {
        !matches!(self, ItemEffect::Shade | ItemEffect::Ward)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemDef {
    pub name: &'static str,
    pub text: &'static str,
    pub element: Element,
    pub slot: Slot,
    pub effect: ItemEffect,
}

const fn item(
    name: &'static str,
    text: &'static str,
    element: Element,
    slot: Slot,
    effect: ItemEffect,
) -> ItemDef {
    ItemDef {
        name,
        text,
        element,
        slot,
        effect,
    }
}

use Element::*;
use ItemEffect::*;
use Slot::*;

/// Every item; a match's loot deck holds each once.
#[rustfmt::skip]
pub const ITEMS: &[ItemDef] = &[
    // Wood — Bhava
    item("Тисовый лук", "+1 кубик, когда нападаешь.", Wood, Weapon, Dice(When::Attacking)),
    item("Плащ из мха", "Ночью скрываешься на любой клетке.", Wood, Armour, Shade),
    item("Посох-корень", "В начале хода +1 здоровья.", Wood, Relic, Mend),
    // Fire — Trishna
    item("Клинок голода", "+1 кубик днём.", Fire, Weapon, Dice(When::Day)),
    item("Жаркий доспех", "+1 щит в каждом бою.", Fire, Armour, Shields),
    item("Кубок пира", "В начале хода +1 Духа.", Fire, Relic, Wellspring),
    // Earth — Zaga
    item("Посох паломника", "+1 кубик в защите.", Earth, Weapon, Dice(When::Defending)),
    item("Сандалии пути", "+1 очко движения каждый ход.", Earth, Armour, Stride),
    item("Чётки тишины", "В начале хода −1 Угрозы.", Earth, Relic, Hush),
    // Metal — Ahamar
    item("Меч присяги", "+1 кубик в бою.", Metal, Weapon, Dice(When::Always)),
    item("Латы переписи", "+1 щит в каждом бою.", Metal, Armour, Shields),
    item("Печать реестра", "+1 карта к пределу руки.", Metal, Relic, Hands),
    // Water — Maya
    item("Кинжал тумана", "+1 кубик ночью.", Water, Weapon, Dice(When::Night)),
    item("Покров Майи", "В начале хода оберег воды.", Water, Armour, Ward),
    item("Бирюзовое зеркало", "+1 кубик в испытаниях.", Water, Relic, TrialDice),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_god_has_one_of_each_slot() {
        for element in Element::ALL {
            for slot in Slot::ALL {
                let n = ITEMS
                    .iter()
                    .filter(|d| d.element == element && d.slot == slot)
                    .count();
                assert_eq!(n, 1, "{element:?} {slot:?}");
            }
        }
    }
}
