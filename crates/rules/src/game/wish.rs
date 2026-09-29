//! The Dominant's wish (docs/design.md §7), offline.
//!
//! At dawn the Dominant asks one god for one of a few prepared wishes, or
//! refuses to wish at all. The god grades the wish by its own nature and by
//! how new it is (§7.4), grants it through its nature, never quite as asked
//! (§7.2), and the grade turns into Style. Crude wishes always get 0: they
//! come true, cut down, with a curse.
//!
//! This is the stand-in for the LLM: later a free-text wish is interpreted by
//! a model into the same closed set of effects, and the rules stay the same.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::style::StyleReason;
use super::{Event, Game, PlayerId, RuleError};
use crate::board::Terrain;
use crate::cards::{CardId, CardMod};
use crate::gods::{Element, God};

/// What a model made of a free-text wish (§7.1): the words, its grade and
/// the god's answer. Carried in the intent, so a replay needs no model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Said {
    /// The wish as the player wrote it.
    pub text: String,
    /// The model's grade, 0..=3, before the rules' own limits.
    pub grade: u8,
    /// The god's answer, in character.
    pub speech: String,
    /// One line on why the grade.
    pub reason: String,
    /// For a forged card: its name and a line of its own, in the god's
    /// voice (§7.3). Empty when the wish forges nothing.
    pub forged: Option<(String, String)>,
}

/// Prepared wishes: what the Dominant can ask for offline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WishKind {
    /// "Give me strength": health, spirit, a ward.
    Strength,
    /// "Let my rival falter": hurt and hold a rival.
    Weaken,
    /// "Let the land answer me": the god's own ground around you.
    Land,
    /// "Let the dead serve me": bodies around you.
    Dead,
    /// "Quiet the noise around me": Threat falls.
    Peace,
    /// "Give me riches": crude.
    Fortune,
    /// "Give me victory": crude.
    Doom,
    /// "Let me know what they want": a rival's secret condition.
    Secret,
    /// "Show me what they hold": a rival's hand, once.
    Hand,
    /// "Bless what I hold": a card in hand costs less and does more.
    Bless,
    /// "Let their hand betray them": a rival's card costs more, does less.
    Blight,
    /// "Give me a new weapon": a card of the god's own.
    Forge,
    /// "Let there be peace between us": no fighting until dusk.
    Truce,
    /// "Let us trade places".
    Swap,
    /// "Let them pay me their due": every rival gives a card or takes Threat.
    Tribute,
    /// "I bet they will...": a wager on a rival's day, settled at dusk.
    Wager,
    /// "Hallow the deck": cards of an element in the deck grow better.
    Hallow,
    /// "Rot the deck": cards of an element in the deck grow worse.
    Rot,
    /// "Hide a curse in the deck": whoever else draws it is bitten.
    Plant,
    /// "Show me what comes": the top of the deck.
    Foresee,
}

impl WishKind {
    pub const ALL: [WishKind; 20] = [
        WishKind::Strength,
        WishKind::Weaken,
        WishKind::Land,
        WishKind::Dead,
        WishKind::Peace,
        WishKind::Fortune,
        WishKind::Doom,
        WishKind::Secret,
        WishKind::Hand,
        WishKind::Bless,
        WishKind::Blight,
        WishKind::Forge,
        WishKind::Truce,
        WishKind::Swap,
        WishKind::Tribute,
        WishKind::Wager,
        WishKind::Hallow,
        WishKind::Rot,
        WishKind::Plant,
        WishKind::Foresee,
    ];

    /// Asks for the outcome itself instead of going through the world: always
    /// graded 0 (§7.4).
    pub const fn is_crude(self) -> bool {
        matches!(self, WishKind::Fortune | WishKind::Doom)
    }

    pub const fn needs_target(self) -> bool {
        matches!(
            self,
            WishKind::Weaken
                | WishKind::Secret
                | WishKind::Hand
                | WishKind::Blight
                | WishKind::Truce
                | WishKind::Swap
                | WishKind::Wager
        )
    }
}

/// One thing a wish asks for (§7.3): a kind from the vocabulary and what
/// that kind needs to know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Act {
    Strength,
    Weaken {
        target: PlayerId,
    },
    Land,
    Dead,
    Peace,
    Fortune,
    Doom,
    /// Learn the rival's secret condition, for good.
    Secret {
        target: PlayerId,
    },
    /// See the rival's hand, once.
    Hand {
        target: PlayerId,
    },
    /// A card in the asker's hand, the one named or else the dearest:
    /// cheaper and stronger.
    Bless {
        card: Option<CardId>,
    },
    /// A card in the rival's hand: dearer and weaker.
    Blight {
        target: PlayerId,
    },
    /// A new card of the god's element into the asker's hand.
    Forge,
    /// No fighting between the asker and the rival until dusk.
    Truce {
        target: PlayerId,
    },
    /// The asker and the rival change places.
    Swap {
        target: PlayerId,
    },
    /// Every rival gives the asker a card or takes Threat.
    Tribute,
    /// The asker bets that the rival will do `bet` before dusk.
    Wager {
        target: PlayerId,
        bet: Bet,
    },
    /// Cards of `element` in the deck (the god's own if none): better.
    Hallow {
        element: Option<Element>,
    },
    /// Cards of `element` in the deck (the one the god quenches if none):
    /// worse; whoever draws one finds out.
    Rot {
        element: Option<Element>,
    },
    /// A curse of the god hidden near the top of the deck.
    Plant,
    /// The asker sees the top cards of the deck.
    Foresee,
}

impl Act {
    pub const fn kind(self) -> WishKind {
        match self {
            Act::Strength => WishKind::Strength,
            Act::Weaken { .. } => WishKind::Weaken,
            Act::Land => WishKind::Land,
            Act::Dead => WishKind::Dead,
            Act::Peace => WishKind::Peace,
            Act::Fortune => WishKind::Fortune,
            Act::Doom => WishKind::Doom,
            Act::Secret { .. } => WishKind::Secret,
            Act::Hand { .. } => WishKind::Hand,
            Act::Bless { .. } => WishKind::Bless,
            Act::Blight { .. } => WishKind::Blight,
            Act::Forge => WishKind::Forge,
            Act::Truce { .. } => WishKind::Truce,
            Act::Swap { .. } => WishKind::Swap,
            Act::Tribute => WishKind::Tribute,
            Act::Wager { .. } => WishKind::Wager,
            Act::Hallow { .. } => WishKind::Hallow,
            Act::Rot { .. } => WishKind::Rot,
            Act::Plant => WishKind::Plant,
            Act::Foresee => WishKind::Foresee,
        }
    }

    pub const fn target(self) -> Option<PlayerId> {
        match self {
            Act::Weaken { target }
            | Act::Secret { target }
            | Act::Hand { target }
            | Act::Blight { target }
            | Act::Truce { target }
            | Act::Swap { target }
            | Act::Wager { target, .. } => Some(target),
            _ => None,
        }
    }

    /// Budget the act takes (§7.3): a new card and a leap across the
    /// table are worth more.
    pub const fn cost(self) -> u8 {
        match self {
            Act::Forge | Act::Swap { .. } | Act::Tribute | Act::Plant => 2,
            _ => 1,
        }
    }

    /// The act of a prepared `kind`, aimed at `target` if it needs one.
    pub fn of(kind: WishKind, target: Option<PlayerId>) -> Option<Act> {
        Some(match kind {
            WishKind::Strength => Act::Strength,
            WishKind::Weaken => Act::Weaken { target: target? },
            WishKind::Land => Act::Land,
            WishKind::Dead => Act::Dead,
            WishKind::Peace => Act::Peace,
            WishKind::Fortune => Act::Fortune,
            WishKind::Doom => Act::Doom,
            WishKind::Secret => Act::Secret { target: target? },
            WishKind::Hand => Act::Hand { target: target? },
            WishKind::Bless => Act::Bless { card: None },
            WishKind::Blight => Act::Blight { target: target? },
            WishKind::Forge => Act::Forge,
            WishKind::Truce => Act::Truce { target: target? },
            WishKind::Swap => Act::Swap { target: target? },
            WishKind::Tribute => Act::Tribute,
            WishKind::Hallow => Act::Hallow { element: None },
            WishKind::Rot => Act::Rot { element: None },
            WishKind::Plant => Act::Plant,
            WishKind::Foresee => Act::Foresee,
            // A prepared wager bets on a fight, the likeliest thing to happen.
            WishKind::Wager => Act::Wager {
                target: target?,
                bet: Bet::Fight,
            },
        })
    }
}

/// What a wager bets a rival will do before dusk (§7.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bet {
    /// Fight a battle, attacking or attacked.
    Fight,
    /// Take a settlement, temple or the Table.
    Claim,
    /// Fall.
    Fall,
    /// Slip out of sight.
    Hide,
}

impl Bet {
    pub const ALL: [Bet; 4] = [Bet::Fight, Bet::Claim, Bet::Fall, Bet::Hide];
}

/// A wager a wish made: `player` bets `target` does `bet` before the dusk
/// of `until`, with `god` holding the stakes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wager {
    pub player: PlayerId,
    pub target: PlayerId,
    pub bet: Bet,
    pub god: God,
    pub until: u32,
    /// It happened: the bet is won, whatever comes after.
    pub happened: bool,
}

/// Style a won wager pays, Threat a lost one costs (with the god's curse).
pub const WAGER_STAKE: i8 = 2;
/// Threat a rival takes for refusing tribute.
pub const TRIBUTE_THREAT: i8 = 2;

/// A truce a wish made (§7.3): neither fights the other until the dusk of
/// `until`; who breaks it carries the curse of the god who granted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Truce {
    pub a: PlayerId,
    pub b: PlayerId,
    pub god: God,
    /// The round whose dusk ends it.
    pub until: u32,
}

impl Truce {
    pub fn binds(&self, x: PlayerId, y: PlayerId) -> bool {
        (self.a, self.b) == (x, y) || (self.a, self.b) == (y, x)
    }
}

/// What the Dominant gives up for a wish (§7.3): paid at once, before the
/// god answers; it raises the budget and the strength of what is granted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Price {
    /// A card from hand, into the discard.
    Card(CardId),
    /// Health, 1 or 2, never the last point.
    Health(u8),
    /// Style, 1 or 2.
    Style(u8),
    /// A settlement, temple or the Table the Dominant holds, let go.
    Claim(Hex),
}

impl Price {
    /// Budget the sacrifice adds.
    pub const fn value(self) -> u8 {
        match self {
            Price::Card(_) => 1,
            Price::Health(n) | Price::Style(n) => {
                if n > 2 {
                    2
                } else {
                    n
                }
            }
            Price::Claim(_) => 2,
        }
    }
}

/// Acts a wish may ask for at most.
pub const MAX_ACTS: usize = 2;

/// A wish (§7.3): one or two acts, the first the main one, and what is
/// given for them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wish {
    pub acts: Vec<Act>,
    pub price: Option<Price>,
}

impl Wish {
    pub fn one(act: Act) -> Wish {
        Wish {
            acts: vec![act],
            price: None,
        }
    }

    /// Any crude part makes the whole wish crude (§7.4).
    pub fn is_crude(&self) -> bool {
        self.acts.iter().any(|a| a.kind().is_crude())
    }

    /// The act that sets the grade: the first.
    pub fn main(&self) -> Option<Act> {
        self.acts.first().copied()
    }
}

/// Gods that value a stake above the rest (§7.2): the contract, the price.
pub const fn likes_a_stake(god: God) -> bool {
    matches!(god, God::Ahamar | God::Zaga)
}

/// How a god likes being asked for something: +1, 0 or −1 to the grade.
pub const fn taste_for(god: God, kind: WishKind) -> i8 {
    use WishKind::*;
    match (god, kind) {
        // Hunger loves strength, feasts of the dead and a gift that feeds;
        // quiet and truce bore it.
        (God::Trishna, Strength | Dead | Forge | Tribute | Plant) => 1,
        (God::Trishna, Peace | Truce) => -1,
        // Order loves land, judgement, a contract and the registry of
        // secrets; the dead are paperwork, a swap is disorder.
        (God::Ahamar, Land | Weaken | Truce | Secret | Wager | Foresee) => 1,
        (God::Ahamar, Dead | Swap) => -1,
        // Dissolution loves letting go, loosening a grip, seeing through,
        // one thing becoming another; not strength, not a new thing to hold.
        (God::Maya, Peace | Weaken | Swap | Hand | Rot) => 1,
        (God::Maya, Strength | Forge | Tribute) => -1,
        // Renunciation loves quiet, the dead at rest and the price a desire
        // exacts; not strength, not blessing what is held.
        (God::Zaga, Peace | Dead | Blight | Tribute) => 1,
        (God::Zaga, Strength | Bless | Hallow) => -1,
        // Growth loves the land, strength, what grows in the hand; not harm.
        (God::Bhava, Land | Strength | Bless | Forge | Hallow) => 1,
        (God::Bhava, Weaken | Blight | Wager | Rot) => -1,
        _ => 0,
    }
}

/// Ground each god raises when asked for land.
pub const fn god_terrain(god: God) -> Terrain {
    match god {
        God::Bhava => Terrain::Grove,
        God::Trishna => Terrain::Plains,
        God::Zaga => Terrain::Mountain,
        God::Ahamar => Terrain::Stones,
        God::Maya => Terrain::Swamp,
    }
}

impl Game {
    /// The Dominant owes a wish (or a refusal) before play goes on.
    pub fn wish_due(&self) -> Option<PlayerId> {
        self.wish_due
    }

    /// The grade `god` would give `kind` right now (§7.4). The same rule the
    /// god applies, so bots and the UI can reason about it.
    pub fn wish_grade(&self, god: God, kind: WishKind) -> u8 {
        if kind.is_crude() {
            return 0;
        }
        let novelty = if self.asked.contains(&(god, kind)) {
            -1
        } else {
            1
        };
        (1 + taste_for(god, kind) + novelty).clamp(0, 3) as u8
    }

    /// Curses on a champion: one god each, draining health every turn until
    /// lifted by a card of the element that quenches that god.
    pub fn curses(&self, player: PlayerId) -> &[God] {
        self.curses
            .get(player.0 as usize)
            .map_or(&[][..], Vec::as_slice)
    }

    /// The grade `god` would give `wish` offline: its main act's grade
    /// (§7.4), one more with a price to a god that likes a stake; crude
    /// wishes are 0.
    pub fn wish_grade_of(&self, god: God, wish: &Wish) -> u8 {
        let Some(main) = wish.main() else {
            return 0;
        };
        if wish.is_crude() {
            return 0;
        }
        let stake = u8::from(wish.price.is_some() && likes_a_stake(god));
        (self.wish_grade(god, main.kind()) + stake).min(3)
    }

    /// Whether `player` may make `wish`: one or two acts, rivals who exist,
    /// a price they can pay.
    pub fn check_wish(&self, player: PlayerId, wish: &Wish) -> Result<(), RuleError> {
        if wish.acts.is_empty() || wish.acts.len() > MAX_ACTS {
            return Err(RuleError::InvalidWish);
        }
        for act in &wish.acts {
            if let Some(t) = act.target()
                && (t == player || self.champion(t).is_none())
            {
                return Err(RuleError::InvalidWish);
            }
            if let Act::Bless { card: Some(card) } = act
                && !self.hand(player).contains(card)
            {
                return Err(RuleError::InvalidWish);
            }
        }
        let payable = match wish.price {
            None => true,
            Some(Price::Card(card)) => self.hand(player).contains(&card),
            Some(Price::Health(n)) => {
                (1..=2).contains(&n) && self.champion(player).is_some_and(|c| c.hp > n)
            }
            Some(Price::Style(n)) => (1..=2).contains(&n) && self.style(player) >= u16::from(n),
            Some(Price::Claim(hex)) => self.claims().any(|(h, p)| h == hex && p == player),
        };
        if payable {
            Ok(())
        } else {
            Err(RuleError::InvalidWish)
        }
    }

    pub(super) fn grant_wish(
        &mut self,
        player: PlayerId,
        god: God,
        wish: Wish,
        said: Option<Said>,
        events: &mut Vec<Event>,
    ) {
        // A forged card's name and line, as the model wrote them, cut to size.
        let forged = said
            .as_ref()
            .and_then(|s| s.forged.clone())
            .map(|(name, line)| (tidy(&name, FORGED_NAME), tidy(&line, FORGED_LINE)));
        let crude = wish.is_crude();
        // A model judges the words; the rules still hold crude wishes at 0 and
        // make a repeated wish worth less (§7.4, §7.5).
        let grade = match &said {
            Some(_) if crude => 0,
            Some(s) => {
                let repeat = wish
                    .main()
                    .is_some_and(|a| self.asked.contains(&(god, a.kind())));
                s.grade.min(3).saturating_sub(u8::from(repeat))
            }
            None => self.wish_grade_of(god, &wish),
        };
        // The god's Voice lifts a wish; a dark god grants grudgingly (§5.4).
        let grade = if crude {
            grade
        } else {
            let voice = u8::from(self.patronage(player, god) >= super::Patronage::Voice);
            let dark = u8::from(self.stage(god) == 2);
            (grade + voice).min(3).saturating_sub(dark)
        };

        // The price is given first: the god takes it whatever it grants.
        if let Some(price) = wish.price {
            self.pay(player, price, events);
        }
        let price_value = wish.price.map_or(0, Price::value);
        let budget = (grade + price_value).max(1);
        let mut spent = 0;
        let granted: Vec<Act> = wish
            .acts
            .iter()
            .copied()
            .take_while(|a| {
                spent += a.cost();
                spent <= budget
            })
            .collect();
        let dropped = (wish.acts.len() - granted.len()) as u8;

        for act in &granted {
            self.asked.push((god, act.kind()));
        }
        self.wish_due = None;
        self.progress[player.0 as usize].refusals = 0;
        events.push(Event::WishGranted {
            player,
            god,
            wish: Wish {
                acts: granted.clone(),
                price: wish.price,
            },
            dropped,
            grade,
            said,
        });
        // Asking is an offering too.
        self.offer(Some(player), god, 1, events);

        let power = (grade + 1 + u8::from(wish.price.is_some())).min(4);
        for act in granted {
            self.grant_act(player, god, act, power, forged.as_ref(), events);
        }

        // A god in its light stage gives without a twist.
        if self.stage(god) != 0 {
            self.twist(player, god, events);
        }

        // The grade becomes Style; a wish without style also carries a curse.
        // Kept small: a wish is power already, and Style keeps the Crown.
        let style = match grade {
            3 => 1,
            1 | 2 => 0,
            _ => -1,
        };
        if style != 0 {
            self.add_style(player, style, StyleReason::Wish, events);
        }
        if grade == 0 {
            self.curses[player.0 as usize].push(god);
            events.push(Event::CurseLaid { player, god });
        }
    }

    /// The sacrifice goes to the god.
    fn pay(&mut self, player: PlayerId, price: Price, events: &mut Vec<Event>) {
        events.push(Event::PricePaid { player, price });
        match price {
            Price::Card(card) => {
                let hand = &mut self.hands[player.0 as usize];
                if let Some(i) = hand.iter().position(|&c| c == card) {
                    hand.remove(i);
                    self.discard.push(card);
                }
            }
            // Checked to leave them standing.
            Price::Health(n) => self.damage(player, n, events),
            Price::Style(n) => self.add_style(player, -i16::from(n), StyleReason::Wish, events),
            Price::Claim(hex) => {
                self.claims.remove(&(hex.x(), hex.y()));
            }
        }
    }

    /// One act of a granted wish, at `power`.
    fn grant_act(
        &mut self,
        player: PlayerId,
        god: God,
        act: Act,
        power: u8,
        forged: Option<&(String, String)>,
        events: &mut Vec<Event>,
    ) {
        let me = self.hex_of(player);
        // Cards bend by one, by two for a strong wish.
        let boost = 1 + i8::from(power >= 4);
        match act {
            Act::Strength => {
                self.heal(player, power, events);
                self.gain_spirit(player, power, events);
                self.raise_ward(player, god.element(), events);
            }
            Act::Weaken { target } => {
                // A god's hand passes any ward.
                self.damage(target, power.saturating_sub(1).max(1), events);
                self.root(target, events);
            }
            Act::Land => {
                let terrain = god_terrain(god);
                let spots: Vec<Hex> = me
                    .all_neighbors()
                    .into_iter()
                    .chain(std::iter::once(me))
                    .filter(|&h| {
                        self.board.tile(h).is_some_and(|t| {
                            t.terrain.can_grow_grove() || t.terrain == Terrain::Mountain
                        }) && !self.mob_at(h)
                    })
                    .take(power as usize)
                    .collect();
                for hex in spots {
                    if let Some(tile) = self.board.tile_mut(hex) {
                        tile.terrain = terrain;
                    }
                    events.push(Event::TerrainChanged { hex, terrain });
                }
            }
            Act::Dead => {
                let free: Vec<Hex> = (1..=2)
                    .flat_map(|r| me.ring(r).collect::<Vec<_>>())
                    .filter(|&h| {
                        self.has(super::Feature::Bodies)
                            && self.board.tile(h).is_some_and(|t| t.corpse.is_none())
                            && self.occupant(h).is_none()
                    })
                    .take(power as usize)
                    .collect();
                for hex in free {
                    if let Some(tile) = self.board.tile_mut(hex) {
                        tile.corpse = Some(crate::board::Corpse { age: 0 });
                    }
                    events.push(Event::CorpseAppeared { hex });
                }
            }
            Act::Peace => {
                self.add_threat(player, -(2 * power as i8), events);
            }
            Act::Fortune => {
                self.add_style(player, 2, StyleReason::Wish, events);
            }
            Act::Doom => {
                for p in self.players().filter(|&p| p != player).collect::<Vec<_>>() {
                    self.damage(p, 1, events);
                }
            }
            Act::Secret { target } => {
                if !self.known.contains(&(player, target)) {
                    self.known.push((player, target));
                }
                events.push(Event::SecretLearned {
                    player,
                    about: target,
                });
            }
            Act::Hand { target } => {
                let cards = self.hands[target.0 as usize]
                    .iter()
                    .map(|&c| self.def_id(c))
                    .collect();
                events.push(Event::HandSeen {
                    player,
                    about: target,
                    cards,
                });
            }
            Act::Bless { card } => {
                if let Some(card) = card.or_else(|| self.dearest_card(player)) {
                    let change = CardMod {
                        cost: -1,
                        power: boost,
                        ..CardMod::default()
                    };
                    self.change_card(player, card, change, god, true, events);
                }
            }
            Act::Blight { target } => {
                if let Some(card) = self.dearest_card(target) {
                    let change = CardMod {
                        cost: 1,
                        power: -boost,
                        ..CardMod::default()
                    };
                    self.change_card(target, card, change, god, false, events);
                }
            }
            Act::Forge => {
                let template = crate::cards::def_named(forge_template(god))
                    .expect("every god has a card to forge from");
                let card = CardId(self.defs.len() as u32);
                self.defs.push(template);
                self.hands[player.0 as usize].push(card);
                // Named by the god when a model spoke for it (§7.3).
                let (name, flavor) = match forged {
                    Some((n, l)) => (
                        Some(n.clone()).filter(|n| !n.is_empty()),
                        Some(l.clone()).filter(|l| !l.is_empty()),
                    ),
                    None => (None, None),
                };
                self.mods.insert(
                    card,
                    CardMod {
                        cost: -1,
                        power: boost,
                        name,
                        flavor,
                        ..CardMod::default()
                    },
                );
                events.push(Event::CardForged { player, card, god });
            }
            Act::Truce { target } => {
                self.truces.retain(|t| !t.binds(player, target));
                self.truces.push(Truce {
                    a: player,
                    b: target,
                    god,
                    until: self.round,
                });
                events.push(Event::TruceMade {
                    player,
                    other: target,
                    god,
                });
            }
            Act::Hallow { element } => {
                let element = element.unwrap_or(god.element());
                let change = CardMod {
                    cost: -1,
                    power: boost,
                    ..CardMod::default()
                };
                self.temper_deck(player, god, element, change, true, power, events);
            }
            Act::Rot { element } => {
                let element = element.unwrap_or(god.element().quenches());
                let change = CardMod {
                    cost: 1,
                    power: -boost,
                    ..CardMod::default()
                };
                self.temper_deck(player, god, element, change, false, power, events);
            }
            Act::Plant => {
                let template = crate::cards::def_named(forge_template(god))
                    .expect("every god has a card to curse with");
                let card = CardId(self.defs.len() as u32);
                self.defs.push(template);
                // A dead weight in hand even before it bites.
                self.mods.insert(
                    card,
                    CardMod {
                        cost: 3,
                        power: -9,
                        ..CardMod::default()
                    },
                );
                self.planted.insert(card, (player, god));
                // Somewhere among the top few, so nobody can count on it.
                let top = self.deck.len();
                let depth = self.rng.below(top.min(3) as u32 + 1) as usize;
                self.deck.insert(top - depth, card);
                events.push(Event::CursePlanted { player, god });
            }
            Act::Foresee => {
                let cards = self
                    .deck
                    .iter()
                    .rev()
                    .take(FORESEE)
                    .map(|&c| self.def_id(c))
                    .collect();
                events.push(Event::Foreseen { player, cards });
            }
            Act::Tribute => {
                // Every rival not already answering elsewhere owes it.
                let owed: Vec<PlayerId> = self
                    .players()
                    .filter(|&p| {
                        p != player && !self.windows.iter().any(|w| w.eligible.contains(&p))
                    })
                    .collect();
                self.open_window(
                    player,
                    super::WindowKind::Tribute { asker: player },
                    owed,
                    None,
                    None,
                    events,
                );
            }
            Act::Wager { target, bet } => {
                self.wagers.push(Wager {
                    player,
                    target,
                    bet,
                    god,
                    until: self.round,
                    happened: false,
                });
                events.push(Event::WagerMade {
                    player,
                    target,
                    bet,
                    god,
                });
            }
            Act::Swap { target } => {
                let (here, there) = (self.hex_of(player), self.hex_of(target));
                for (who, to) in [(player, there), (target, here)] {
                    let champ = self.champ_mut(who);
                    champ.hex = to;
                    champ.seen_at = to;
                }
                events.push(Event::Swapped {
                    player,
                    to: there,
                    other: target,
                    other_to: here,
                });
            }
        }
    }

    /// Every god grants through its own nature (§7.2).
    fn twist(&mut self, player: PlayerId, god: God, events: &mut Vec<Event>) {
        match god {
            // The gift feeds the hunger.
            God::Trishna => self.offer(None, God::Trishna, 1, events),
            // Written down as a debt in the registry.
            God::Ahamar => self.add_threat(player, 1, events),
            // Not quite as you held it: a card dissolves into spirit.
            God::Maya => {
                let hand = &self.hands[player.0 as usize];
                if !hand.is_empty() {
                    let i = self.rng.below(hand.len() as u32) as usize;
                    let card = self.hands[player.0 as usize].remove(i);
                    self.discard.push(card);
                    events.push(Event::CardDissolved { player, card });
                    self.gain_spirit(player, 1, events);
                }
            }
            // The price of one wish is another.
            God::Zaga => {
                let champ = self.champ_mut(player);
                if champ.spirit_points > 0 {
                    champ.spirit_points -= 1;
                    let spirit = champ.spirit_points;
                    events.push(Event::SpiritChanged { player, spirit });
                }
            }
            // Growth nobody controls: a grove rises by a rival too.
            God::Bhava => {
                let rivals: Vec<PlayerId> = self.players().filter(|&p| p != player).collect();
                if let Some(&rival) = self.rng.pick(&rivals) {
                    let near = self.hex_of(rival).all_neighbors().into_iter().find(|&h| {
                        self.occupant(h).is_none()
                            && self.board.tile(h).is_some_and(|t| {
                                t.terrain.can_grow_grove() && t.terrain != Terrain::Grove
                            })
                    });
                    if let Some(hex) = near {
                        self.grow(hex, events);
                    }
                }
            }
        }
    }

    /// Cursed champions lose health as their turn starts.
    pub(super) fn bite_curses(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let gods = self.curses[player.0 as usize].clone();
        for god in gods {
            events.push(Event::CurseBit { player, god });
            self.damage(player, 1, events);
        }
    }

    /// A card of the element that quenches a cursing god lifts its curse.
    pub(super) fn lift_curse(
        &mut self,
        player: PlayerId,
        element: crate::gods::Element,
        events: &mut Vec<Event>,
    ) {
        let curses = &mut self.curses[player.0 as usize];
        if let Some(i) = curses
            .iter()
            .position(|g| g.element().quenched_by() == element)
        {
            let god = curses.remove(i);
            events.push(Event::CurseLifted { player, god });
        }
    }
}

/// The pool card a god forges a new one from (§7.3): its own element's
/// strongest gift.
pub const fn forge_template(god: God) -> &'static str {
    match god {
        God::Bhava => "Живица",
        God::Trishna => "Пламя пира",
        God::Zaga => "Отзвучавшая нота",
        God::Ahamar => "Приговор порядка",
        God::Maya => "Дымная ладонь",
    }
}

impl Game {
    /// The dearest card in a hand: the one a blessing or a blight weighs on.
    fn dearest_card(&self, player: PlayerId) -> Option<CardId> {
        self.hands[player.0 as usize]
            .iter()
            .copied()
            .max_by_key(|&c| (self.def(c).cost, std::cmp::Reverse(c)))
    }

    /// One copy of a card changes; its owner is told.
    fn change_card(
        &mut self,
        owner: PlayerId,
        card: CardId,
        change: CardMod,
        god: God,
        blessed: bool,
        events: &mut Vec<Event>,
    ) {
        self.mods.entry(card).or_default().stack(&change);
        events.push(Event::CardChanged {
            owner,
            card,
            god,
            blessed,
        });
    }

    /// A fight or a harmful card between the two sides of a truce breaks it:
    /// the breaker carries the curse of the god who made it, and loses face.
    pub(super) fn break_truce(&mut self, by: PlayerId, other: PlayerId, events: &mut Vec<Event>) {
        let Some(i) = self.truces.iter().position(|t| t.binds(by, other)) else {
            return;
        };
        let truce = self.truces.remove(i);
        events.push(Event::TruceBroken {
            player: by,
            other,
            god: truce.god,
        });
        self.curses[by.0 as usize].push(truce.god);
        events.push(Event::CurseLaid {
            player: by,
            god: truce.god,
        });
        self.add_style(by, -2, StyleReason::Wish, events);
    }

    /// Truces end at the dusk of the day they were made.
    pub(super) fn end_truces(&mut self) {
        let round = self.round;
        self.truces.retain(|t| t.until > round);
    }

    /// Truces in force: who with whom, and which god keeps them.
    pub fn truces(&self) -> &[Truce] {
        &self.truces
    }

    /// Whether `viewer` has learned `player`'s secret through a wish.
    pub fn knows_secret(&self, viewer: PlayerId, player: PlayerId) -> bool {
        self.known.contains(&(viewer, player))
    }
}

impl Game {
    /// A harmful card at a rival in a truce breaks it.
    pub(super) fn hostile_play(
        &mut self,
        player: PlayerId,
        card: CardId,
        target: super::Target,
        events: &mut Vec<Event>,
    ) {
        if let super::Target::Champion(other) = target
            && other != player
            && self.def(card).effect.is_harmful()
        {
            self.break_truce(player, other, events);
            self.note_pursuit(player, other);
        }
    }
}

impl Game {
    /// Tribute answered (§7.3): each card given goes to the asker, each
    /// refusal is Threat.
    pub(super) fn settle_tribute(
        &mut self,
        asker: PlayerId,
        window: &super::Window,
        events: &mut Vec<Event>,
    ) {
        for &p in &window.eligible {
            match window.choices.get(&p) {
                Some(super::Choice::Play(card, _)) => {
                    self.hands[asker.0 as usize].push(*card);
                    events.push(Event::TributeGiven {
                        player: p,
                        to: asker,
                        card: *card,
                    });
                }
                _ => {
                    events.push(Event::TributeRefused {
                        player: p,
                        to: asker,
                    });
                    self.add_threat(p, TRIBUTE_THREAT, events);
                }
            }
        }
    }

    /// `player` did `bet` today: wagers on it are won.
    pub(super) fn note_bet(&mut self, player: PlayerId, bet: Bet) {
        for w in &mut self.wagers {
            if w.target == player && w.bet == bet {
                w.happened = true;
            }
        }
    }

    /// Wagers are settled at the dusk of their day: Style for a won one,
    /// Threat and the god's curse (a debt) for a lost one.
    pub(super) fn settle_wagers(&mut self, events: &mut Vec<Event>) {
        let round = self.round;
        let due: Vec<Wager> = self
            .wagers
            .iter()
            .copied()
            .filter(|w| w.until <= round)
            .collect();
        self.wagers.retain(|w| w.until > round);
        for w in due {
            if w.happened {
                events.push(Event::WagerWon {
                    player: w.player,
                    target: w.target,
                    bet: w.bet,
                    god: w.god,
                });
                self.add_style(w.player, i16::from(WAGER_STAKE), StyleReason::Wish, events);
            } else {
                events.push(Event::WagerLost {
                    player: w.player,
                    target: w.target,
                    bet: w.bet,
                    god: w.god,
                });
                self.add_threat(w.player, WAGER_STAKE, events);
                self.curses[w.player.0 as usize].push(w.god);
                events.push(Event::CurseLaid {
                    player: w.player,
                    god: w.god,
                });
            }
        }
    }

    /// Wagers open today.
    pub fn wagers(&self) -> &[Wager] {
        &self.wagers
    }
}

/// Cards a foreseeing wish shows from the top of the deck.
pub const FORESEE: usize = 3;

impl Game {
    /// Up to `power + 1` cards of `element` in the deck, from the top, take
    /// `change` (§7.3). Nobody sees it until one is drawn.
    #[allow(clippy::too_many_arguments)]
    fn temper_deck(
        &mut self,
        player: PlayerId,
        god: God,
        element: Element,
        change: CardMod,
        blessed: bool,
        power: u8,
        events: &mut Vec<Event>,
    ) {
        let cards: Vec<CardId> = self
            .deck
            .iter()
            .rev()
            .copied()
            .filter(|&c| self.def(c).element == Some(element))
            .take(power as usize + 1)
            .collect();
        for &card in &cards {
            self.mods.entry(card).or_default().stack(&change);
        }
        events.push(Event::DeckChanged {
            player,
            god,
            element,
            count: cards.len() as u8,
            blessed,
        });
    }

    /// A planted curse is drawn: it bites anyone but whoever planted it,
    /// then leaves the game.
    pub(super) fn spring_curse(&mut self, player: PlayerId, card: CardId, events: &mut Vec<Event>) {
        let Some((planter, god)) = self.planted.remove(&card) else {
            return;
        };
        self.hands[player.0 as usize].retain(|&c| c != card);
        let bit = player != planter;
        events.push(Event::CurseDrawn {
            player,
            planter,
            god,
            bit,
        });
        if bit {
            self.damage(player, 1, events);
        }
    }
}

/// Letters a forged card's name may have, and its own line.
pub const FORGED_NAME: usize = 28;
pub const FORGED_LINE: usize = 90;

/// A model's words for a card, one line, no quotes, at most `max` letters.
fn tidy(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line
        .trim_matches(|c: char| matches!(c, '«' | '»' | '"' | '\'' | '.' | ' '))
        .chars()
        .take(max)
        .collect()
}
