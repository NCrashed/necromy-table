//! Great Deeds (docs/design.md §21.7): how a match is won.
//!
//! At the start everyone is offered three deeds and picks one; all are open
//! to the table. A deed builds or changes the world and needs mechanics a
//! new world does not have, so whoever wants it must bring them in. When
//! every step of a deed holds, its eve begins and the table hears of it;
//! the deed is done at the next dusk if everything still holds then. The
//! others have that day to break it.
//!
//! A deed is a list of checks with a number to reach, so the client can
//! show everyone's progress without knowing the rules behind it.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::God;
use crate::rng::Rng;

/// A late-game goal that makes the world (§21.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum GreatDeed {
    /// A grove grown from a champion's body, ringed by woods, with beasts
    /// near, standing through two dusks.
    WorldTree,
    /// Land of seven hexes or more cut off from the Table by the mist, with a
    /// settlement of yours on it, and you there.
    Island,
    /// A whole region of another god gone into the mist, but its temple and
    /// the champions' homes, and fifteen hexes of it at least.
    DissolvedLand,
}

impl GreatDeed {
    pub const ALL: [GreatDeed; 3] = [
        GreatDeed::WorldTree,
        GreatDeed::Island,
        GreatDeed::DissolvedLand,
    ];

    /// The god whose deed it is: its card's colour, its voice.
    pub const fn patron(self) -> God {
        match self {
            GreatDeed::WorldTree => God::Bhava,
            GreatDeed::Island | GreatDeed::DissolvedLand => God::Maya,
        }
    }

    /// What the world must have for it: on the card, so it says what to
    /// bring in.
    pub const fn needs(self) -> &'static [Feature] {
        match self {
            GreatDeed::WorldTree => &[Feature::Bodies, Feature::Groves, Feature::Beasts],
            GreatDeed::Island => &[Feature::Settlements],
            GreatDeed::DissolvedLand => &[],
        }
    }
}

/// What a check measures; the client names and draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckKind {
    /// A grove grown from a champion's body stands.
    HeroGrove,
    /// Woods and groves round it, of six.
    WoodsAround,
    /// A beast within two hexes of it.
    BeastsNear,
    /// Dusks in a row it has stood so.
    Dusks,
    /// Hexes of the land cut off from the Table that you stand on.
    IslandSize,
    /// A settlement of yours on it.
    IslandSettled,
    /// Hexes of another god's region gone into the mist.
    RegionInMist,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub have: u16,
    pub need: u16,
}

impl Check {
    pub fn met(&self) -> bool {
        self.have >= self.need
    }
}

/// Hexes an Island needs.
pub const ISLAND: usize = 7;
/// Hexes of a region the mist must take at least: all of it, and a small
/// one must first grow.
pub const DISSOLVED: usize = 15;

/// Deeds offered to each player to pick from.
pub const OFFERED: usize = 3;

/// Threat a refused wish costs the Crown: turning the table down is loud.
pub const REFUSAL_THREAT: i8 = 2;

/// Per-player counters the checks need.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Dusks the Crown was theirs in a row (the storyteller's boredom).
    pub crown_streak: u8,
    /// Dusks in a row their World Tree has stood whole.
    pub tree_dusks: u8,
}

/// Three deeds for each player: of different patrons where it can, and none
/// offered twice at the table while the pool lasts.
pub fn deal(rng: &mut Rng, players: usize) -> Vec<Vec<GreatDeed>> {
    let mut pool = GreatDeed::ALL.to_vec();
    rng.shuffle(&mut pool);
    let mut dealt: Vec<GreatDeed> = Vec::new();
    (0..players)
        .map(|_| {
            let mut hand: Vec<GreatDeed> = Vec::new();
            // Fresh ones of new patrons first, then fresh ones, then any.
            type Pass<'a> = &'a dyn Fn(&GreatDeed, &[GreatDeed]) -> bool;
            let passes: [Pass; 3] = [
                &|d, hand| !dealt.contains(d) && !hand.iter().any(|h| h.patron() == d.patron()),
                &|d, _| !dealt.contains(d),
                &|_, _| true,
            ];
            for pass in passes {
                for &d in &pool {
                    if hand.len() < OFFERED && !hand.contains(&d) && pass(&d, &hand) {
                        hand.push(d);
                    }
                }
            }
            dealt.extend(hand.iter().copied());
            hand
        })
        .collect()
}

impl Game {
    /// The deeds `player` was offered to pick from.
    pub fn offers(&self, player: PlayerId) -> &[GreatDeed] {
        self.offers
            .get(player.0 as usize)
            .map_or(&[][..], Vec::as_slice)
    }

    /// The deed `player` chose, once they have.
    pub fn deed(&self, player: PlayerId) -> Option<GreatDeed> {
        self.chosen.get(player.0 as usize).copied().flatten()
    }

    /// `player`'s deed holds and waits for dusk to be done.
    pub fn on_eve(&self, player: PlayerId) -> bool {
        self.eves
            .get(player.0 as usize)
            .is_some_and(Option::is_some)
    }

    /// The winner and their deed, once the match is over.
    pub fn winner(&self) -> Option<(PlayerId, GreatDeed)> {
        self.winner
    }

    /// Those who still have to pick their deed.
    pub fn choosing(&self) -> Vec<PlayerId> {
        self.order
            .iter()
            .copied()
            .filter(|&p| self.deed(p).is_none() && !self.offers(p).is_empty())
            .collect()
    }

    /// `player` picks the deed of the match, one of their offers.
    pub(super) fn choose_deed(
        &mut self,
        player: PlayerId,
        deed: GreatDeed,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        if self.deed(player).is_some() || !self.offers(player).contains(&deed) {
            return Err(RuleError::InvalidDeed);
        }
        self.chosen[player.0 as usize] = Some(deed);
        events.push(Event::DeedChosen { player, deed });
        Ok(())
    }

    /// Where `player` stands on `deed`.
    pub fn checks(&self, player: PlayerId, deed: GreatDeed) -> Vec<Check> {
        let check = |kind, have: usize, need: usize| Check {
            kind,
            have: have.min(u16::MAX as usize) as u16,
            need: need as u16,
        };
        match deed {
            GreatDeed::WorldTree => {
                let (grove, woods, beasts) = self.best_tree();
                vec![
                    check(CheckKind::HeroGrove, usize::from(grove.is_some()), 1),
                    check(CheckKind::WoodsAround, woods, 6),
                    check(CheckKind::BeastsNear, usize::from(beasts), 1),
                    check(
                        CheckKind::Dusks,
                        usize::from(self.progress[player.0 as usize].tree_dusks),
                        2,
                    ),
                ]
            }
            GreatDeed::Island => {
                let island = self.island_of(player);
                let settled = island.iter().any(|&h| {
                    self.owner(h) == Some(player)
                        && self
                            .board
                            .tile(h)
                            .is_some_and(|t| t.terrain == Terrain::Settlement)
                });
                vec![
                    check(CheckKind::IslandSize, island.len(), ISLAND),
                    check(CheckKind::IslandSettled, usize::from(settled), 1),
                ]
            }
            GreatDeed::DissolvedLand => {
                // A land worth the name: a small region must grow before it goes.
                let (gone, all) = self.dissolved_for(player);
                vec![check(CheckKind::RegionInMist, gone, all.max(DISSOLVED))]
            }
        }
    }

    /// The grove grown from a champion that is nearest a World Tree.
    pub fn hero_grove(&self) -> Option<Hex> {
        self.best_tree().0
    }

    /// The best grove grown from a champion: woods round it, a beast near.
    fn best_tree(&self) -> (Option<Hex>, usize, bool) {
        self.hero_groves
            .iter()
            .map(|&(x, y)| Hex::new(x, y))
            .filter(|&h| {
                self.board
                    .tile(h)
                    .is_some_and(|t| t.terrain == Terrain::Grove)
            })
            .map(|h| {
                let woods = h
                    .all_neighbors()
                    .iter()
                    .filter(|&&n| {
                        self.board
                            .tile(n)
                            .is_some_and(|t| matches!(t.terrain, Terrain::Forest | Terrain::Grove))
                    })
                    .count();
                let beasts = self
                    .mobs
                    .iter()
                    .any(|m| m.is_beast() && m.hex.unsigned_distance_to(h) <= 2);
                (Some(h), woods, beasts)
            })
            .max_by_key(|&(h, woods, beasts)| {
                (woods + 6 * usize::from(beasts), h.map(|h| (-h.x(), -h.y())))
            })
            .unwrap_or((None, 0, false))
    }

    /// The land `player` stands on, if the mist cuts it off from the Table.
    fn island_of(&self, player: PlayerId) -> Vec<Hex> {
        let start = self.hex_of(player);
        let mut seen = vec![start];
        let mut i = 0;
        while i < seen.len() {
            for n in seen[i].all_neighbors() {
                if self.board.contains(n) && !seen.contains(&n) {
                    seen.push(n);
                }
            }
            i += 1;
        }
        if seen.contains(&Hex::ZERO) {
            Vec::new()
        } else {
            seen
        }
    }

    /// Of another god's region, the one most gone: hexes in the mist, and
    /// all that can go (its temple and the champions' homes stay).
    fn dissolved_for(&self, player: PlayerId) -> (usize, usize) {
        let (_, gone, all) = self.dissolving(player);
        (gone, all)
    }

    /// The land of the region `player` dissolves nearest them, still out of
    /// the mist.
    pub fn left_to_dissolve(&self, player: PlayerId) -> Option<Hex> {
        let (god, ..) = self.dissolving(player);
        let me = self.hex_of(player);
        self.board
            .land()
            .filter(|(h, t)| {
                t.region == Some(god)
                    && *h != self.board.temple_of(god)
                    && !God::ALL.iter().any(|&o| self.board.start_of(o) == *h)
            })
            .map(|(h, _)| h)
            .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
    }

    /// The region `player` is dissolving best (another god's): its god, hexes
    /// in the mist, and all that can go.
    pub fn dissolving(&self, player: PlayerId) -> (God, usize, usize) {
        let own = self.champions[player.0 as usize].god;
        God::ALL
            .into_iter()
            .filter(|&g| g != own)
            .map(|g| {
                let mut gone = 0;
                let mut all = 0;
                for (h, t) in self.board.tiles() {
                    let home = God::ALL.iter().any(|&o| self.board.start_of(o) == h);
                    if t.region != Some(g) || h == self.board.temple_of(g) || home {
                        continue;
                    }
                    all += 1;
                    if !t.terrain.is_land() {
                        gone += 1;
                    }
                }
                (g, gone, all)
            })
            .max_by_key(|&(g, gone, all)| {
                (gone * 1000 / all.max(1), gone, std::cmp::Reverse(g.index()))
            })
            .expect("four other gods")
    }

    fn deed_holds(&self, player: PlayerId) -> bool {
        self.deed(player)
            .is_some_and(|d| self.checks(player, d).iter().all(Check::met))
    }

    /// Dusk: the Crown's streak.
    pub(super) fn count_crown(&mut self) {
        for p in self.players().collect::<Vec<_>>() {
            let crowned = self.dominant == Some(p);
            let progress = &mut self.progress[p.0 as usize];
            progress.crown_streak = if crowned {
                progress.crown_streak.saturating_add(1)
            } else {
                0
            };
        }
    }

    /// Dusk: a World Tree that stood whole counts another dusk.
    fn count_trees(&mut self) {
        for p in self.players().collect::<Vec<_>>() {
            if self.deed(p) != Some(GreatDeed::WorldTree) {
                continue;
            }
            let standing = self
                .checks(p, GreatDeed::WorldTree)
                .iter()
                .filter(|c| c.kind != CheckKind::Dusks)
                .all(Check::met);
            let dusks = &mut self.progress[p.0 as usize].tree_dusks;
            *dusks = if standing { dusks.saturating_add(1) } else { 0 };
        }
    }

    /// After every intent: a deed that holds begins its eve, one that no
    /// longer holds loses it.
    pub(super) fn check_victory(&mut self, events: &mut Vec<Event>) {
        if self.winner.is_some() {
            return;
        }
        for p in self.order.clone() {
            let Some(deed) = self.deed(p) else {
                continue;
            };
            let holds = self.deed_holds(p);
            let eve = &mut self.eves[p.0 as usize];
            match (holds, *eve) {
                (true, None) => {
                    *eve = Some(self.dusks);
                    events.push(Event::DeedEve { player: p, deed });
                }
                (false, Some(_)) => {
                    *eve = None;
                    events.push(Event::EveBroken { player: p, deed });
                }
                _ => {}
            }
        }
    }

    /// Dusk falls: trees count their dusk, and a deed whose eve began before
    /// this dusk and still holds is done. Initiative settles a tie.
    pub(super) fn dusk_of_deeds(&mut self, events: &mut Vec<Event>) {
        self.dusks += 1;
        self.count_trees();
        self.check_victory(events);
        for p in self.order.clone() {
            let (Some(deed), Some(since)) = (self.deed(p), self.eves[p.0 as usize]) else {
                continue;
            };
            if since < self.dusks && self.deed_holds(p) {
                self.winner = Some((p, deed));
                events.push(Event::Victory { player: p, deed });
                return;
            }
        }
    }
}
