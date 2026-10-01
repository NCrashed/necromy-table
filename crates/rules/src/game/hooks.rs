//! Cases: the storyteller reads the world (docs/storyteller-plan.md, stage
//! 4). A detector finds something happening near a champion (a fire by a
//! settlement, the dead piling up, a rival's caravan, a settlement with no
//! master); each comes with two ways to close it, for two gods. At dusk
//! whoever has no case open is given the best one near them: the one
//! lagging most first, scored by nearness, by what their deed's patron
//! cares for, and by whether a rival is in it.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::story::{Doing, Fork, Goal, Line, LineKind};
use super::{Event, Feature, Game, PlayerId};
use crate::board::Terrain;
use crate::gods::God;

/// Rounds a case stays open: what happens near does not wait.
pub const CASE_ROUNDS: u32 = 2;
/// How far from a champion a case may be found for them.
pub const CASE_REACH: u32 = 4;

/// What the storyteller saw happening.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hook {
    /// A fire by a settlement.
    Blaze,
    /// A beast by a settlement.
    Prowler,
    /// Bodies piling up, about to rise.
    RestlessDead,
    /// A rival carrying goods or food.
    Caravan,
    /// A settlement with a ruler and no master.
    Masterless,
    /// The stranger out of the mist.
    Stranger,
    /// An undead by a settlement or a fair.
    Marauder,
    /// Ruins waiting.
    Ruins,
}

/// A case as found: where, what it asks, the other way, and who is in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Case {
    pub hook: Hook,
    pub at: Hex,
    pub goal: Goal,
    pub god: God,
    pub fork: Option<Fork>,
    /// The rival it is about, if one.
    pub rival: Option<PlayerId>,
}

impl Game {
    /// Everything happening on the board a case could be made of.
    pub fn cases(&self) -> Vec<Case> {
        let mut out = Vec::new();
        let towns: Vec<Hex> = self
            .board
            .land()
            .filter(|(_, t)| t.terrain == Terrain::Settlement)
            .map(|(h, _)| h)
            .collect();
        let by_town = |h: Hex| towns.iter().any(|t| t.unsigned_distance_to(h) <= 2);
        let fork = |goal: Goal, god: God| Some(Fork { goal, god });
        // Fire by a settlement: put it out, or let it eat.
        for (h, _) in self.fires() {
            if by_town(h) {
                out.push(Case {
                    hook: Hook::Blaze,
                    at: h,
                    goal: Goal::Do(Doing::Douse),
                    god: God::Maya,
                    fork: fork(Goal::Do(Doing::Kindle), God::Trishna),
                    rival: None,
                });
            }
        }
        for m in &self.mobs {
            match m.kind {
                // A beast at the gates: tame it, or fell it.
                super::mobs::MobKind::Beast { .. } if by_town(m.hex) => out.push(Case {
                    hook: Hook::Prowler,
                    at: m.hex,
                    goal: if self.has(Feature::Companions) {
                        Goal::Do(Doing::Tame)
                    } else {
                        Goal::Do(Doing::FellMob)
                    },
                    god: God::Bhava,
                    fork: self.has(Feature::Companions).then_some(Fork {
                        goal: Goal::Do(Doing::FellMob),
                        god: God::Ahamar,
                    }),
                    rival: None,
                }),
                // The dead at a settlement or a fair: put it down, or enlist it.
                super::mobs::MobKind::Undead
                    if by_town(m.hex)
                        || self
                            .fairs()
                            .any(|(f, _)| f.unsigned_distance_to(m.hex) <= 2) =>
                {
                    out.push(Case {
                        hook: Hook::Marauder,
                        at: m.hex,
                        goal: Goal::Do(Doing::FellMob),
                        god: God::Ahamar,
                        fork: self.has(Feature::Legion).then_some(Fork {
                            goal: Goal::Do(Doing::Tame),
                            god: God::Maya,
                        }),
                        rival: None,
                    })
                }
                // The stranger: lead them.
                super::mobs::MobKind::Guest => out.push(Case {
                    hook: Hook::Stranger,
                    at: m.hex,
                    goal: Goal::Do(Doing::Tame),
                    god: God::Maya,
                    fork: None,
                    rival: None,
                }),
                _ => {}
            }
        }
        // Bodies piling up: three within two of one.
        let corpses: Vec<Hex> = self.board.corpses().map(|(h, _)| h).collect();
        let graveyard = self.has(Feature::Burial)
            && self
                .board
                .land()
                .any(|(_, t)| t.terrain == Terrain::Graveyard);
        let mut piles: Vec<Hex> = Vec::new();
        for &c in &corpses {
            let near = corpses
                .iter()
                .filter(|o| o.unsigned_distance_to(c) <= 2)
                .count();
            if near >= 3 && !piles.iter().any(|p| p.unsigned_distance_to(c) <= 2) {
                piles.push(c);
                out.push(Case {
                    hook: Hook::RestlessDead,
                    at: c,
                    goal: if graveyard {
                        Goal::Do(Doing::Bury)
                    } else {
                        Goal::Body
                    },
                    god: God::Zaga,
                    fork: fork(Goal::Do(Doing::Raise), God::Maya),
                    rival: None,
                });
            }
        }
        // A rival's burden of goods or food: take it.
        if self.has(Feature::Cargo) {
            for p in self.players() {
                let Some(cargo) = self.cargo(p) else { continue };
                if matches!(cargo, super::Cargo::Goods(_) | super::Cargo::Food) {
                    out.push(Case {
                        hook: Hook::Caravan,
                        at: self.hex_of(p),
                        goal: Goal::Do(Doing::Seize),
                        god: God::Trishna,
                        fork: None,
                        rival: Some(p),
                    });
                }
            }
        }
        // A settlement with a ruler and no master: win the ruler, or take it.
        for (h, _) in self.rulers() {
            if self.owner(h).is_none() {
                out.push(Case {
                    hook: Hook::Masterless,
                    at: h,
                    goal: Goal::Do(Doing::Gift),
                    god: God::Ahamar,
                    fork: fork(Goal::Claim, God::Trishna),
                    rival: None,
                });
            }
        }
        // Ruins: build them again, or dig under them.
        for (h, t) in self.board.land() {
            if t.terrain == Terrain::Ruins {
                out.push(Case {
                    hook: Hook::Ruins,
                    at: h,
                    goal: Goal::Do(Doing::Rebuild),
                    god: God::Ahamar,
                    fork: self.has(Feature::Underworld).then_some(Fork {
                        goal: Goal::Do(Doing::Delve),
                        god: God::Zaga,
                    }),
                    rival: None,
                });
            }
        }
        out
    }

    /// Dusk: whoever has no case open gets the best one near them, the one
    /// lagging most first; one case to a place.
    pub(super) fn deal_cases(&mut self, events: &mut Vec<Event>) {
        let cases = self.cases();
        if cases.is_empty() {
            return;
        }
        let mut order: Vec<PlayerId> = self.players().collect();
        order.sort_by_key(|&p| (self.nearness(p), p.0));
        let mut taken: Vec<Hex> = self
            .lines
            .iter()
            .filter(|l| matches!(l.kind, LineKind::Case(_)))
            .filter_map(|l| l.at)
            .collect();
        for p in order {
            if self
                .lines_of(p)
                .any(|l| matches!(l.kind, LineKind::Case(_)))
            {
                continue;
            }
            let me = self.hex_of(p);
            let patron = self.deed(p).map(|d| d.patron());
            let best = cases
                .iter()
                .filter(|c| c.rival != Some(p) && !taken.contains(&c.at))
                .filter(|c| c.at.unsigned_distance_to(me) <= CASE_REACH)
                .max_by_key(|c| {
                    let near = CASE_REACH - c.at.unsigned_distance_to(me);
                    let cares =
                        Some(c.god) == patron || c.fork.is_some_and(|f| Some(f.god) == patron);
                    // What happens now beats what only lies there.
                    let lively = !matches!(c.hook, Hook::Ruins | Hook::Masterless);
                    (
                        near + 3 * u32::from(cares)
                            + 2 * u32::from(c.rival.is_some())
                            + 3 * u32::from(lively),
                        c.at.x(),
                        c.at.y(),
                    )
                })
                .copied();
            let Some(case) = best else { continue };
            taken.push(case.at);
            self.next_line += 1;
            let line = Line {
                id: self.next_line,
                owner: p,
                god: case.god,
                kind: LineKind::Case(case.hook),
                goal: case.goal,
                deadline: self.round + CASE_ROUNDS,
                style: 3,
                stake: 0,
                fork: case.fork,
                at: Some(case.at),
                letter: false,
                chapter: 0,
                betrays: None,
            };
            self.lines.push(line);
            events.push(Event::LineTold { line });
        }
    }
}
