//! Rulers, gifts, oaths, marriages and feuds (docs/design.md §21.8).
//!
//! Every settlement has a ruler of its land's house. A gift (a Spirit, or
//! the food or goods on one's back, worth more) moves the ruler towards the
//! giver and ends any feud they are in; a ruler who favours one champion
//! above all and enough swears to them, and so does one whose militia a
//! champion has beaten. Two rulers of different lands who both favour a
//! champion may be betrothed by them; at dusk they wed if they still do.
//! Marriages bind lands into one house.
//!
//! One with three sworn rulers may be crowned on Ahamar's Table; an
//! emperor may sow discord among their vassals: the oaths are broken and
//! the former vassals feud, their militia fighting each other in the
//! world phase until someone makes peace with a gift.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::God;

/// A settlement's place in the maps.
type Key = (i32, i32);

/// Regard a ruler must have for someone, above all others, to swear.
pub const OATH_REGARD: i8 = 4;
/// Regard for a betrothal.
pub const MATCH_REGARD: i8 = 2;
/// Sworn rulers for a coronation.
pub const CROWN_VASSALS: usize = 3;
/// Lands bound by one's marriages for the Triple Union.
pub const UNION_LANDS: usize = 3;
/// Feuding former vassals for the Fallen Empire.
pub const FEUDING: usize = 3;

/// The ruler of a settlement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ruler {
    /// Regard for each player, by seat.
    pub regard: Vec<i8>,
    pub sworn: Option<PlayerId>,
    /// Married to the ruler on this hex, the match made by this player.
    pub spouse: Option<((i32, i32), PlayerId)>,
    /// Betrothed, to wed at dusk.
    pub betrothed: Option<((i32, i32), PlayerId)>,
    /// Feuding since this player sowed discord among their vassals.
    pub feud: Option<PlayerId>,
}

impl Ruler {
    /// The one this ruler favours above all others, if anyone.
    pub fn favourite(&self) -> Option<PlayerId> {
        let best = *self.regard.iter().max()?;
        let mut top = self.regard.iter().enumerate().filter(|&(_, &r)| r == best);
        let (i, _) = top.next()?;
        (best > 0 && top.next().is_none()).then_some(PlayerId(i as u8))
    }

    pub fn regard_for(&self, player: PlayerId) -> i8 {
        self.regard.get(player.0 as usize).copied().unwrap_or(0)
    }
}

impl Game {
    pub fn ruler(&self, hex: Hex) -> Option<&Ruler> {
        self.rulers.get(&(hex.x(), hex.y()))
    }

    pub fn rulers(&self) -> impl Iterator<Item = (Hex, &Ruler)> + '_ {
        self.rulers.iter().map(|(&(x, y), r)| (Hex::new(x, y), r))
    }

    pub fn emperor(&self) -> Option<PlayerId> {
        self.emperor
    }

    /// Whether `player` has been crowned this match.
    pub fn crowned(&self, player: PlayerId) -> bool {
        self.crowned
            .get(player.0 as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Rulers sworn to `player`.
    pub fn vassals(&self, player: PlayerId) -> Vec<Hex> {
        self.rulers()
            .filter(|(_, r)| r.sworn == Some(player))
            .map(|(h, _)| h)
            .collect()
    }

    /// Every settlement has its ruler once the world has rulers.
    pub(super) fn seat_rulers(&mut self) {
        if !self.has(Feature::Rulers) {
            return;
        }
        let seats = self.champions.len();
        let towns: Vec<(i32, i32)> = self
            .board
            .land()
            .filter(|(_, t)| t.terrain == Terrain::Settlement)
            .map(|(h, _)| (h.x(), h.y()))
            .collect();
        for key in towns {
            self.rulers.entry(key).or_insert_with(|| Ruler {
                regard: vec![0; seats],
                ..Ruler::default()
            });
        }
        // A ruler whose settlement is gone is dead.
        let gone: Vec<(i32, i32)> = self
            .rulers
            .keys()
            .copied()
            .filter(|&(x, y)| {
                self.board
                    .tile(Hex::new(x, y))
                    .is_none_or(|t| t.terrain != Terrain::Settlement)
            })
            .collect();
        for key in gone {
            self.rulers.remove(&key);
            for r in self.rulers.values_mut() {
                if r.spouse.is_some_and(|(k, _)| k == key) {
                    r.spouse = None;
                }
                if r.betrothed.is_some_and(|(k, _)| k == key) {
                    r.betrothed = None;
                }
            }
        }
    }

    /// Settlements beside `player` (or underfoot) with a ruler to give to.
    pub fn giftable(&self, player: PlayerId) -> Vec<Hex> {
        let at = self.hex_of(player);
        std::iter::once(at)
            .chain(at.all_neighbors())
            .filter(|&h| self.ruler(h).is_some())
            .collect()
    }

    pub(super) fn check_gift(&self, player: PlayerId, hex: Hex) -> Result<(), RuleError> {
        if !self.giftable(player).contains(&hex) {
            return Err(RuleError::InvalidTarget);
        }
        let carries = matches!(
            self.cargo(player),
            Some(super::Cargo::Food | super::Cargo::Goods(_))
        );
        let have = self.champions[player.0 as usize].spirit_points;
        if !carries && have < 1 {
            return Err(RuleError::NotEnoughSpirit { need: 1, have });
        }
        Ok(())
    }

    /// A gift to the ruler on `hex`: what is carried, else a Spirit.
    pub(super) fn gift(
        &mut self,
        player: PlayerId,
        hex: Hex,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_gift(player, hex)?;
        let worth = if matches!(
            self.cargo(player),
            Some(super::Cargo::Food | super::Cargo::Goods(_))
        ) {
            self.champ_mut(player).cargo = None;
            2
        } else {
            let champ = self.champ_mut(player);
            champ.spirit_points -= 1;
            let spirit = champ.spirit_points;
            events.push(Event::SpiritChanged { player, spirit });
            1
        };
        let ruler = self.rulers.get_mut(&(hex.x(), hex.y())).expect("checked");
        if let Some(r) = ruler.regard.get_mut(player.0 as usize) {
            *r = r.saturating_add(worth);
        }
        // A gift makes peace.
        ruler.feud = None;
        events.push(Event::Gifted { player, hex, worth });
        self.first(player, super::Novelty::Gifted, events);
        self.settle_oath(hex, events);
        Ok(())
    }

    /// The ruler on `hex` swears to their favourite, if they favour them
    /// enough; an oath to anyone else is broken.
    fn settle_oath(&mut self, hex: Hex, events: &mut Vec<Event>) {
        let Some(ruler) = self.rulers.get_mut(&(hex.x(), hex.y())) else {
            return;
        };
        let lord = ruler
            .favourite()
            .filter(|&p| ruler.regard_for(p) >= OATH_REGARD);
        if let Some(player) = lord
            && lord != ruler.sworn
        {
            ruler.sworn = lord;
            events.push(Event::Sworn { hex, player });
        }
    }

    /// A champion beat the militia of `home`: its ruler swears to them.
    pub(super) fn conquer_ruler(&mut self, player: PlayerId, home: Hex, events: &mut Vec<Event>) {
        let Some(ruler) = self.rulers.get_mut(&(home.x(), home.y())) else {
            return;
        };
        let top = ruler.regard.iter().copied().max().unwrap_or(0);
        if let Some(r) = ruler.regard.get_mut(player.0 as usize) {
            *r = (*r).max(OATH_REGARD).max(top.saturating_add(1));
        }
        self.settle_oath(home, events);
    }

    /// Pairs of rulers `player` could betroth now: both favour them enough,
    /// of different lands, neither wed nor promised, one of them beside them.
    pub fn matches(&self, player: PlayerId) -> Vec<(Hex, Hex)> {
        if !self.has(Feature::Rulers) {
            return Vec::new();
        }
        let near = self.giftable(player);
        let free: Vec<(Hex, God)> = self
            .rulers()
            .filter(|(_, r)| {
                r.spouse.is_none()
                    && r.betrothed.is_none()
                    && r.favourite() == Some(player)
                    && r.regard_for(player) >= MATCH_REGARD
            })
            .filter_map(|(h, _)| Some((h, self.board.tile(h)?.region?)))
            .collect();
        let mut out = Vec::new();
        for (i, &(a, ga)) in free.iter().enumerate() {
            for &(b, gb) in &free[i + 1..] {
                if ga != gb && (near.contains(&a) || near.contains(&b)) {
                    out.push((a, b));
                }
            }
        }
        out
    }

    pub(super) fn check_betroth(&self, player: PlayerId, a: Hex, b: Hex) -> Result<(), RuleError> {
        let ok = self
            .matches(player)
            .iter()
            .any(|&(x, y)| (x, y) == (a, b) || (y, x) == (a, b));
        if ok {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    pub(super) fn betroth(
        &mut self,
        player: PlayerId,
        a: Hex,
        b: Hex,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_betroth(player, a, b)?;
        let (ka, kb) = ((a.x(), a.y()), (b.x(), b.y()));
        self.rulers.get_mut(&ka).expect("checked").betrothed = Some((kb, player));
        self.rulers.get_mut(&kb).expect("checked").betrothed = Some((ka, player));
        events.push(Event::Betrothed { player, a, b });
        Ok(())
    }

    /// Dusk: the betrothed wed if both still favour their matchmaker, else
    /// the match is off.
    pub(super) fn weddings(&mut self, events: &mut Vec<Event>) {
        self.seat_rulers();
        let promised: Vec<(Key, Key, PlayerId)> = self
            .rulers
            .iter()
            .filter_map(|(&k, r)| r.betrothed.map(|(o, p)| (k, o, p)))
            .filter(|(k, o, _)| k < o)
            .collect();
        for (ka, kb, p) in promised {
            let holds = [ka, kb]
                .iter()
                .all(|k| self.rulers.get(k).is_some_and(|r| r.favourite() == Some(p)));
            for (k, o) in [(ka, kb), (kb, ka)] {
                if let Some(r) = self.rulers.get_mut(&k) {
                    r.betrothed = None;
                    if holds {
                        r.spouse = Some((o, p));
                    }
                }
            }
            let (a, b) = (Hex::new(ka.0, ka.1), Hex::new(kb.0, kb.1));
            if holds {
                events.push(Event::Wedding { player: p, a, b });
            } else {
                events.push(Event::MatchBroken { player: p, a, b });
            }
        }
    }

    /// Lands bound into one house by marriages `player` made, the most.
    pub fn union_lands(&self, player: PlayerId) -> usize {
        let links: Vec<(God, God)> = self
            .rulers()
            .filter_map(|(h, r)| {
                let ((x, y), by) = r.spouse?;
                let other = Hex::new(x, y);
                (by == player && (h.x(), h.y()) < (x, y)).then_some(())?;
                Some((self.board.tile(h)?.region?, self.board.tile(other)?.region?))
            })
            .collect();
        let mut best = 0;
        for start in God::ALL {
            let mut house = vec![start];
            let mut i = 0;
            while i < house.len() {
                for &(a, b) in &links {
                    for (x, y) in [(a, b), (b, a)] {
                        if x == house[i] && !house.contains(&y) {
                            house.push(y);
                        }
                    }
                }
                i += 1;
            }
            if house.len() > 1 {
                best = best.max(house.len());
            }
        }
        best
    }

    pub fn may_crown(&self, player: PlayerId) -> bool {
        self.has(Feature::Rulers)
            && self.emperor != Some(player)
            && self.hex_of(player) == Hex::ZERO
            && self.vassals(player).len() >= CROWN_VASSALS
    }

    pub(super) fn check_crown(&self, player: PlayerId) -> Result<(), RuleError> {
        if self.may_crown(player) {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    /// Crowned on the Table, emperor over three vassals or more.
    pub(super) fn coronation(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_crown(player)?;
        self.emperor = Some(player);
        self.crowned[player.0 as usize] = true;
        events.push(Event::Emperor { player });
        Ok(())
    }

    pub fn may_sow_discord(&self, player: PlayerId) -> bool {
        self.emperor == Some(player)
            && self
                .giftable(player)
                .iter()
                .any(|&h| self.ruler(h).is_some_and(|r| r.sworn == Some(player)))
    }

    pub(super) fn check_discord(&self, player: PlayerId) -> Result<(), RuleError> {
        if self.may_sow_discord(player) {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    /// The emperor sets their vassals against each other: the oaths break,
    /// the empire is gone, the former vassals feud.
    pub(super) fn sow_discord(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_discord(player)?;
        let mut feuding = 0usize;
        for r in self.rulers.values_mut() {
            if r.sworn == Some(player) {
                r.sworn = None;
                r.feud = Some(player);
                if let Some(x) = r.regard.get_mut(player.0 as usize) {
                    *x = 0;
                }
                feuding += 1;
            }
        }
        self.emperor = None;
        events.push(Event::Discord {
            player,
            feuding: feuding as u8,
        });
        Ok(())
    }

    /// Former vassals of `player`'s still feuding.
    pub fn feuding(&self, player: PlayerId) -> usize {
        self.rulers()
            .filter(|(_, r)| r.feud == Some(player))
            .count()
    }

    /// World phase: feuding settlements' militia strike each other.
    pub(super) fn feud_phase(&mut self, events: &mut Vec<Event>) {
        let feuding: Vec<Hex> = self
            .rulers()
            .filter(|(_, r)| r.feud.is_some())
            .map(|(h, _)| h)
            .collect();
        if feuding.len() < 2 {
            return;
        }
        for home in feuding {
            if self.militia(home).is_some_and(|m| m > 0) {
                events.push(Event::FeudStruck { home });
                self.hurt_militia(home, 1, events);
            }
        }
    }
}
