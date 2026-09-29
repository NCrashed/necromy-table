//! Style, the Crown and Threat (docs/design.md §6).
//!
//! Style is this game's prestige. At dawn the player with the most Style
//! wears the Crown and becomes the Dominant, who will make the wish (§7).
//! The Crown is status, not an item: it is lost by being outscored.
//!
//! Where Style comes from, and how much, is the table's taste, drawn per
//! match so that no single source is always the answer (§6.4).

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId};
use crate::board::Terrain;
use crate::cards::Effect;
use crate::gods::{Element, God};
use crate::rng::Rng;

/// Something a champion did today that their character cares about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Deed {
    Played(Element),
    Body(BodyVerb),
    /// Started a battle.
    Attacked,
    /// Took part in a battle, either side.
    Fought,
    /// Won a battle (dealt more than it took).
    Won,
    Prayed,
    Claimed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyVerb {
    Fuel,
    Legion,
    Dissolve,
    Rest,
    Seed,
}

impl BodyVerb {
    pub fn of(effect: Effect) -> Option<BodyVerb> {
        Some(match effect {
            Effect::BodyFuel => BodyVerb::Fuel,
            Effect::BodyLegion => BodyVerb::Legion,
            Effect::BodyDissolve => BodyVerb::Dissolve,
            Effect::BodyRest => BodyVerb::Rest,
            Effect::BodySeed => BodyVerb::Seed,
            _ => return None,
        })
    }
}

/// What a champion swore never to do, and how they like to act (§13). The
/// offline storyteller rewards the manner and punishes a broken oath at dusk;
/// the LLM storyteller will judge richer play later (§6.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Character {
    pub oath: Deed,
    pub manner: Deed,
}

impl Character {
    pub const fn of(god: God) -> Character {
        let (oath, manner) = match god {
            God::Bhava => (Deed::Body(BodyVerb::Fuel), Deed::Played(Element::Wood)),
            God::Trishna => (Deed::Body(BodyVerb::Rest), Deed::Fought),
            God::Zaga => (Deed::Attacked, Deed::Played(Element::Earth)),
            God::Ahamar => (Deed::Body(BodyVerb::Dissolve), Deed::Claimed),
            God::Maya => (Deed::Body(BodyVerb::Legion), Deed::Prayed),
        };
        Character { oath, manner }
    }
}

/// Weights of the Style sources in this match (§6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Taste {
    pub kind: TasteKind,
    pub settlement: u8,
    pub temple: u8,
    pub table: u8,
    pub roleplay: u8,
    pub battle: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TasteKind {
    /// Every source counts.
    Balance,
    /// Land is worth double.
    Land,
    /// Land is worth next to nothing; character is worth double.
    Stage,
    /// Fights are worth double; temples nothing.
    Arena,
}

impl Taste {
    pub const fn of(kind: TasteKind) -> Taste {
        let [settlement, temple, table, roleplay, battle] = match kind {
            TasteKind::Balance => [1, 1, 2, 1, 1],
            TasteKind::Land => [2, 2, 3, 1, 1],
            TasteKind::Stage => [0, 1, 1, 2, 1],
            TasteKind::Arena => [1, 0, 1, 1, 2],
        };
        Taste {
            kind,
            settlement,
            temple,
            table,
            roleplay,
            battle,
        }
    }

    pub fn draw(rng: &mut Rng) -> Taste {
        let kinds = [
            TasteKind::Balance,
            TasteKind::Land,
            TasteKind::Stage,
            TasteKind::Arena,
        ];
        Taste::of(*rng.pick(&kinds).expect("four tastes"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StyleReason {
    Territory,
    Battle,
    Manner,
    Oath,
    /// The wish's grade (§7.4).
    Wish,
    /// A story line done or failed (§8).
    Story,
    /// A trial passed or failed (§20.2).
    Trial,
    /// A dark god's toll on its item (§20.3).
    Item,
}

/// Threat at which the royal guard comes out (§6.5).
pub const GUARD_THRESHOLD: u8 = 4;

impl Game {
    pub fn style(&self, player: PlayerId) -> u16 {
        self.style.get(player.0 as usize).copied().unwrap_or(0)
    }

    pub fn threat(&self, player: PlayerId) -> u8 {
        self.threat.get(player.0 as usize).copied().unwrap_or(0)
    }

    /// Who wears the Crown since the last dawn.
    pub fn dominant(&self) -> Option<PlayerId> {
        self.dominant
    }

    pub fn taste(&self) -> Taste {
        self.taste
    }

    pub fn character(&self, player: PlayerId) -> Option<Character> {
        self.champion(player).map(|c| Character::of(c.god))
    }

    /// Who holds a settlement, temple or the Table.
    pub fn owner(&self, hex: Hex) -> Option<PlayerId> {
        self.claims.get(&(hex.x(), hex.y())).copied()
    }

    pub fn claims(&self) -> impl Iterator<Item = (Hex, PlayerId)> + '_ {
        self.claims.iter().map(|(&(x, y), &p)| (Hex::new(x, y), p))
    }

    pub(super) fn add_style(
        &mut self,
        player: PlayerId,
        delta: i16,
        reason: StyleReason,
        events: &mut Vec<Event>,
    ) {
        let s = &mut self.style[player.0 as usize];
        let before = *s;
        *s = (i32::from(*s) + i32::from(delta)).max(0) as u16;
        let total = *s;
        if total != before {
            events.push(Event::StyleChanged {
                player,
                delta: total as i16 - before as i16,
                total,
                reason,
            });
        }
    }

    pub(super) fn add_threat(&mut self, player: PlayerId, delta: i8, events: &mut Vec<Event>) {
        let t = &mut self.threat[player.0 as usize];
        let before = *t;
        *t = (i16::from(*t) + i16::from(delta)).clamp(0, 99) as u8;
        let total = *t;
        if total != before {
            events.push(Event::ThreatChanged {
                player,
                delta: total as i8 - before as i8,
                total,
            });
        }
    }

    pub(super) fn record_deed(&mut self, player: PlayerId, deed: Deed) {
        self.deeds[player.0 as usize].push(deed);
        self.pending_story.push((player, deed));
    }

    /// The winner of a battle takes Style from the loser; from the Dominant,
    /// twice as much (§6.3).
    pub(super) fn battle_style(
        &mut self,
        winner: PlayerId,
        loser: PlayerId,
        events: &mut Vec<Event>,
    ) {
        self.count_overthrow(winner, loser);
        self.record_deed(winner, Deed::Won);
        let base = i16::from(self.taste.battle);
        let amount = if self.dominant == Some(loser) {
            base * 2
        } else {
            base
        };
        self.add_style(winner, amount, StyleReason::Battle, events);
        self.add_style(loser, -amount, StyleReason::Battle, events);
    }

    /// Entering a settlement, a temple or the Table takes it.
    pub(super) fn claim(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        let claimable = self.board.tile(hex).is_some_and(|t| {
            matches!(
                t.terrain,
                Terrain::Settlement | Terrain::Temple | Terrain::Table
            )
        });
        if !claimable || self.owner(hex) == Some(player) {
            return;
        }
        // The militia keep the unwelcome out of their settlement (§20.4).
        if self.militia(hex).is_some() && self.standing(player) <= super::HOSTILE {
            events.push(Event::MilitiaBarred { player, hex });
            return;
        }
        let from = self.claims.insert((hex.x(), hex.y()), player);
        self.record_deed(player, Deed::Claimed);
        events.push(Event::Claimed { player, hex, from });
        self.note_bet(player, super::wish::Bet::Claim);
    }

    /// Dawn: land pays Style, then the Crown goes to the leader (§6.1).
    pub(super) fn dawn(&mut self, events: &mut Vec<Event>) {
        self.stealth_at_dawn(events);
        self.militia_at_dawn();
        // Ahamar's Crack: land far from its owner pays nothing (§5.3).
        let crack = self.law_active(super::Law::Crack);
        let income: Vec<(PlayerId, u8)> = self
            .claims()
            .filter(|&(hex, p)| {
                !crack
                    || self.chosen(p, crate::gods::God::Ahamar)
                    || self.hex_of(p).unsigned_distance_to(hex) <= super::CRACK_REACH
            })
            .filter_map(|(hex, p)| {
                let w = match self.board.tile(hex)?.terrain {
                    Terrain::Settlement => self.taste.settlement,
                    Terrain::Temple => self.taste.temple,
                    Terrain::Table => self.taste.table,
                    _ => 0,
                };
                Some((p, w))
            })
            .collect();
        for (p, w) in income {
            self.add_style(p, i16::from(w), StyleReason::Territory, events);
        }
        // Ahamar's Mask: whoever holds any land looks respectable.
        if self.law_active(super::Law::Mask) {
            let holders: Vec<PlayerId> = self
                .players()
                .filter(|&p| self.claims().any(|(_, o)| o == p))
                .collect();
            for p in holders {
                events.push(Event::Law {
                    law: super::Law::Mask,
                    player: Some(p),
                    hex: None,
                });
                self.add_style(p, 1, StyleReason::Territory, events);
            }
        }

        let best = self.players().map(|p| self.style(p)).max().unwrap_or(0);
        let leaders: Vec<PlayerId> = self.players().filter(|&p| self.style(p) == best).collect();
        let crowned = if best == 0 {
            None
        } else if leaders.len() == 1 {
            Some(leaders[0])
        } else {
            // A tie keeps the Crown where it was; otherwise the table is contested.
            self.dominant.filter(|d| leaders.contains(d))
        };
        self.dominant = crowned;
        events.push(Event::Crowned { player: crowned });
        self.count_dawn();
        if let Some(d) = crowned {
            self.wish_due = Some(d);
            events.push(Event::WishDue { player: d });
        }
        if let Some(d) = crowned {
            // The Crown draws the guard's eye (§6.1).
            self.add_threat(d, 1, events);
        }
    }

    /// Dusk: the offline storyteller weighs the day against each character.
    pub(super) fn judge_the_day(&mut self, events: &mut Vec<Event>) {
        for p in self.players().collect::<Vec<_>>() {
            let character = Character::of(self.champions[p.0 as usize].god);
            let deeds = std::mem::take(&mut self.deeds[p.0 as usize]);
            if deeds.contains(&character.manner) {
                let w = i16::from(self.taste.roleplay);
                self.add_style(p, w, StyleReason::Manner, events);
            }
            if deeds.contains(&character.oath) {
                self.add_style(p, -1, StyleReason::Oath, events);
            }
        }
    }
}
