//! Dusk (docs/design.md §21.4): every player wishes once a day.
//!
//! All day a player may seal a wish (or a refusal); what they asked stays
//! hidden from rivals until the god answers. At dusk the Crown goes to the
//! leader in Style, the table waits for everyone still to decide, and then
//! the gods answer in order of Style, least first, the Crown last: each
//! later wish sees the world as the earlier ones left it. When the tributes
//! they demanded are in, the gods shift, and the night begins.

use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, REFUSAL_THREAT, TimeOfDay};
use crate::game::wish::{Said, Wish};
use crate::gods::God;

/// A player's wish for tonight's dusk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Seal {
    /// Not decided yet.
    #[default]
    Open,
    /// Sealed. `None` in a rival's view: what was asked stays hidden until
    /// the god answers.
    Wish(Option<SealedWish>),
    /// They want nothing tonight.
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedWish {
    pub god: God,
    pub wish: Wish,
    /// A model's reading of the words; `None` for a prepared wish.
    pub said: Option<Said>,
}

/// Where dusk stands, while it waits on people.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuskStep {
    /// The Crown is given; the gods wait for everyone's wish.
    Sealing,
    /// The gods answered; tributes they demanded are still coming in.
    Answering,
}

impl Game {
    /// `player`'s wish for tonight.
    pub fn seal(&self, player: PlayerId) -> &Seal {
        &self.seals[player.0 as usize]
    }

    /// Whether wishes are made in this match at all (a scripted scene may
    /// do without them).
    fn wishes_made(&self) -> bool {
        self.scripted.is_none_or(|s| s.dawn)
    }

    /// `player` may seal a wish now: by day, or at dusk while the gods wait,
    /// and only once.
    pub fn may_wish(&self, player: PlayerId) -> bool {
        self.winner.is_none()
            && self.wishes_made()
            && (self.time == TimeOfDay::Day || self.dusk.is_some())
            && self.dusk != Some(DuskStep::Answering)
            && self.seals.get(player.0 as usize) == Some(&Seal::Open)
    }

    /// Dusk is under way and waits on someone: a wish, or a tribute.
    pub fn at_dusk(&self) -> Option<DuskStep> {
        self.dusk
    }

    /// Those dusk waits for: everyone who has not sealed a wish yet.
    pub fn wishing(&self) -> Vec<PlayerId> {
        if self.dusk != Some(DuskStep::Sealing) {
            return Vec::new();
        }
        self.order
            .iter()
            .copied()
            .filter(|&p| self.seals[p.0 as usize] == Seal::Open)
            .collect()
    }

    /// A wish or a refusal, sealed until dusk.
    pub(super) fn seal_wish(
        &mut self,
        player: PlayerId,
        seal: Seal,
        events: &mut Vec<Event>,
    ) -> Result<(), super::RuleError> {
        if !self.may_wish(player) {
            return Err(super::RuleError::InvalidWish);
        }
        if let Seal::Wish(Some(w)) = &seal {
            self.check_wish(player, &w.wish)?;
        }
        self.seals[player.0 as usize] = seal;
        events.push(Event::WishSealed { player });
        Ok(())
    }

    /// Dusk has fallen: the Crown goes to the leader, and the gods wait.
    pub(super) fn begin_dusk(&mut self, events: &mut Vec<Event>) {
        if !self.wishes_made() {
            return;
        }
        self.crown(events);
        // Maya's fog lifts; what a god set aside last dusk comes in first.
        self.fog.clear();
        self.awaken_deferred(events);
        self.dusk = Some(DuskStep::Sealing);
        for p in self.wishing() {
            events.push(Event::WishDue { player: p });
        }
    }

    /// Once everyone has sealed: the gods answer, least Style first, the
    /// Crown last (§21.4).
    pub(super) fn answer_wishes(&mut self, events: &mut Vec<Event>) {
        // What a wish told of a rival's wish lasts from dusk to dusk.
        self.known.clear();
        let mut order: Vec<PlayerId> = self.order.clone();
        order.sort_by_key(|&p| (self.dominant == Some(p), self.style(p)));
        for p in order {
            self.answer_wish(p, events);
        }
    }

    /// The god answers `player`'s sealed wish (or notes the refusal).
    pub(super) fn answer_wish(&mut self, p: PlayerId, events: &mut Vec<Event>) {
        {
            let seal = std::mem::take(&mut self.seals[p.0 as usize]);
            match seal {
                Seal::Wish(Some(SealedWish { god, wish, said })) => {
                    // What was sealed at noon may no longer hold at dusk: a
                    // card gone from the hand, a price no longer there. The
                    // god takes what still can be given.
                    let acts: Vec<_> = wish
                        .acts
                        .iter()
                        .copied()
                        .filter(|&a| {
                            self.check_wish(
                                p,
                                &Wish {
                                    acts: vec![a],
                                    price: None,
                                },
                            )
                            .is_ok()
                        })
                        .collect();
                    let price = wish.price.filter(|_| {
                        self.check_wish(
                            p,
                            &Wish {
                                acts: acts.clone(),
                                price: wish.price,
                            },
                        )
                        .is_ok()
                    });
                    if acts.is_empty() {
                        events.push(Event::WishLost { player: p, god });
                    } else {
                        self.grant_wish(p, god, Wish { acts, price }, said, events);
                    }
                }
                Seal::Refused | Seal::Open | Seal::Wish(None) => {
                    events.push(Event::WishRefused { player: p });
                    // The Crown turning down what the table pays is loud.
                    if self.dominant == Some(p) {
                        self.add_threat(p, REFUSAL_THREAT, events);
                    }
                }
            }
        }
    }

    /// The Crown for the leader in Style; a tie keeps it where it was, and
    /// with none ahead the table is contested (§6.1).
    pub(super) fn crown(&mut self, events: &mut Vec<Event>) {
        let best = self.players().map(|p| self.style(p)).max().unwrap_or(0);
        let leaders: Vec<PlayerId> = self.players().filter(|&p| self.style(p) == best).collect();
        let crowned = if best == 0 {
            None
        } else if leaders.len() == 1 {
            Some(leaders[0])
        } else {
            self.dominant.filter(|d| leaders.contains(d))
        };
        self.dominant = crowned;
        events.push(Event::Crowned { player: crowned });
        self.count_crown();
        if let Some(d) = crowned {
            // The Crown draws the guard's eye (§6.1).
            self.add_threat(d, 1, events);
        }
    }

    /// The Crown answers for its wishes (§21.4): a bad one costs it double.
    pub(super) fn crown_wish_style(&self, player: PlayerId, style: i16) -> i16 {
        if self.dominant == Some(player) && style < 0 {
            style * 2
        } else {
            style
        }
    }
}

/// Style a wish's grade is worth, before the Crown's weight.
pub(super) const fn grade_style(grade: u8) -> i16 {
    match grade {
        3 => 1,
        1 | 2 => 0,
        _ => -1,
    }
}
