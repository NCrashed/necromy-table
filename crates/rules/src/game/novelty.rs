//! Style for what is new (docs/design.md §21.5).
//!
//! Style rewards the one who tries things, not the one who repeats the best
//! of them. Whoever does something first at the table (the first battle
//! won, the first undead laid to rest, the first of each kind of wish, the
//! first to bring a mechanic in or play one of its cards) earns Style for
//! it; nobody earns it twice. Battles won again are worth less each time,
//! and a day of many different deeds is worth more than one of the same.

use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, StyleReason};
use crate::features::Feature;
use crate::game::wish::WishKind;

/// Something done for the first time at this table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Novelty {
    WonBattle,
    FelledGuard,
    /// An undead laid to rest.
    LaidToRest,
    SlewBeast,
    PassedTrial,
    TookSettlement,
    TookTable,
    Rebuilt,
    /// An item given to a god at its temple.
    Sacrificed,
    Hid,
    /// A story line brought to its end.
    FinishedLine,
    /// A building raised on a settlement.
    Built,
    /// A beast tamed, an undead enlisted.
    Tamed,
    Enlisted,
    /// A road laid by one's own hand.
    Paved,
    /// A fire set, a fire put out.
    Kindled,
    Doused,
    /// A field sown, a feast held.
    Sowed,
    Feasted,
    /// A fair opened.
    HeldFair,
    /// A gift to a ruler.
    Gifted,
    /// Ground consecrated, a body buried.
    Consecrated,
    Buried,
    /// A wish of this kind, granted.
    Wished(WishKind),
    /// A mechanic brought into the world.
    Brought(Feature),
    /// A card of this mechanic played.
    PlayedFor(Feature),
}

/// Style for a first.
pub const FIRST_STYLE: i16 = 1;
/// Different deeds a day that earn a Style at dusk.
pub const VARIETY: usize = 3;

impl Game {
    /// Who did what first at this table.
    pub fn firsts(&self) -> impl Iterator<Item = (Novelty, PlayerId)> + '_ {
        self.firsts.iter().map(|(&n, &p)| (n, p))
    }

    /// `player` did `novelty`: if nobody had yet, it is theirs, with Style.
    pub(super) fn first(&mut self, player: PlayerId, novelty: Novelty, events: &mut Vec<Event>) {
        if self.firsts.contains_key(&novelty) {
            return;
        }
        self.firsts.insert(novelty, player);
        events.push(Event::First { player, novelty });
        self.add_style(player, FIRST_STYLE, StyleReason::First, events);
    }

    /// Style for a battle won: less for each won before (§21.5).
    pub(super) fn repeated(&mut self, player: PlayerId, base: i16) -> i16 {
        let won = &mut self.won[player.0 as usize];
        let worth = base - i16::from(*won / 2);
        *won = won.saturating_add(1);
        worth.max(0)
    }

    /// Dusk: a day of many different deeds is worth a Style.
    pub(super) fn judge_variety(&mut self, events: &mut Vec<Event>) {
        for p in self.players().collect::<Vec<_>>() {
            let mut kinds: Vec<std::mem::Discriminant<super::style::Deed>> = Vec::new();
            for d in &self.deeds[p.0 as usize] {
                let kind = match d {
                    // Each element played is its own deed.
                    super::style::Deed::Played(_) => None,
                    other => Some(std::mem::discriminant(other)),
                };
                if let Some(k) = kind
                    && !kinds.contains(&k)
                {
                    kinds.push(k);
                }
            }
            let mut elements: Vec<crate::gods::Element> = self.deeds[p.0 as usize]
                .iter()
                .filter_map(|d| match d {
                    super::style::Deed::Played(e) => Some(*e),
                    _ => None,
                })
                .collect();
            elements.sort_by_key(|e| e.index());
            elements.dedup();
            if kinds.len() + elements.len() >= VARIETY {
                self.add_style(p, 1, StyleReason::Variety, events);
            }
        }
    }
}
