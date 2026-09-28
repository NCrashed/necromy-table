//! Victory conditions (docs/design.md §10).
//!
//! Each match draws three open conditions from a pool, known to everyone,
//! and one secret condition per player, known only to its owner. The first
//! player to meet any of their conditions wins and the match stops.
//!
//! A condition is a list of checks with a number to reach, so the client can
//! show progress without knowing the rules behind it.

use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId};
use crate::board::Terrain;
use crate::gods::God;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    /// Hold a settlement or temple in this many regions.
    Registry { regions: u8 },
    /// Serve one god fanatically while that god is at its darkest.
    GodLimit,
    /// Serve two gods next to each other on the ring of generation, neither
    /// of them in its light stage.
    Fusion,
    /// Keep your favour balanced near the centre with no god dark, for this
    /// many dawns in a row.
    MiddlePath { dawns: u8 },
    /// Beat the Dominant in battle this many times.
    Overthrow { wins: u8 },
    /// Most Style at the dawn of this round.
    FirstAtTable { round: u32 },
    /// As the Dominant, refuse the wish this many dawns in a row (§6, §7):
    /// keep winning the table and turn down what it pays. Each refusal
    /// costs `REFUSAL_THREAT`, and falling in between starts it over.
    Wager { refusals: u8 },
}

/// What a check measures; the client names and draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckKind {
    RegionsHeld,
    /// Favour with your most favoured god.
    TopFavor,
    /// Favour with your second god, next to the first on the ring.
    SecondFavor,
    /// Share of all your favour held by the pair, in percent.
    PairShare,
    TotalFavor,
    /// Length of your favour vector, in hundredths (§5).
    Fanaticism,
    /// Closeness to the centre: 100 minus the vector length in hundredths.
    Balance,
    /// Stage of your most favoured god (2 is dark).
    GodStage,
    /// Both gods of the pair past their light stage (0 or 1 of them).
    PairNotLight,
    /// Gods not in their dark stage, out of five.
    GodsNotDark,
    Streak,
    Overthrows,
    /// Rounds reached.
    Round,
    /// 1 if you lead in Style right now.
    StyleLead,
    /// Wishes refused in a row as the Dominant.
    Refusals,
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

const OPEN_POOL: [Condition; 6] = [
    Condition::Registry { regions: 4 },
    Condition::GodLimit,
    Condition::Fusion,
    Condition::MiddlePath { dawns: 2 },
    Condition::Overthrow { wins: 3 },
    Condition::FirstAtTable { round: 19 },
];

/// Secret conditions are a fallback, not a shortcut (§10): nobody else
/// can see them coming, so they ask for more than any open one and count
/// only from `SECRET_FROM_ROUND`, once the open race has had its chance.
const SECRET_POOL: [Condition; 3] = [
    Condition::Wager { refusals: 3 },
    Condition::Overthrow { wins: 4 },
    Condition::Registry { regions: 5 },
];

/// The round from which a secret condition can win.
pub const SECRET_FROM_ROUND: u32 = 8;

/// Threat a refused wish costs: turning the table down is loud.
pub const REFUSAL_THREAT: i8 = 2;

pub const OPEN_COUNT: usize = 3;

/// Same kind of condition, whatever its numbers.
fn same_kind(a: Condition, b: Condition) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

/// Per-player counters the checks need.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub crown_streak: u8,
    pub middle_streak: u8,
    pub overthrows: u8,
    /// Wishes refused in a row as the Dominant (the Wager, §10); a wish
    /// made, a dawn without the Crown, or a fall starts it over.
    pub refusals: u8,
}

/// Draws this match's conditions: open ones, then one secret per player of
/// a kind not already open.
pub fn draw(rng: &mut Rng, players: usize) -> (Vec<Condition>, Vec<Condition>) {
    let mut open = OPEN_POOL.to_vec();
    rng.shuffle(&mut open);
    open.truncate(OPEN_COUNT);
    let allowed: Vec<Condition> = SECRET_POOL
        .into_iter()
        .filter(|s| !open.iter().any(|o| same_kind(*o, *s)))
        .collect();
    let secrets = (0..players)
        .map(|_| *rng.pick(&allowed).unwrap_or(&SECRET_POOL[0]))
        .collect();
    (open, secrets)
}

impl Game {
    pub fn open_conditions(&self) -> &[Condition] {
        &self.open
    }

    /// A player's secret condition; `None` in a view that hides it.
    pub fn secret(&self, player: PlayerId) -> Option<Condition> {
        self.secrets.get(player.0 as usize).copied().flatten()
    }

    /// The winner and the condition they met, once the match is over.
    pub fn winner(&self) -> Option<(PlayerId, Condition)> {
        self.winner
    }

    /// Where `player` stands on `condition`.
    pub fn checks(&self, player: PlayerId, condition: Condition) -> Vec<Check> {
        let progress = &self.progress[player.0 as usize];
        let favor = |g: God| self.favor(player, g);
        // Gods by favour, most first; ties by ring order.
        let mut ranked: Vec<God> = God::ALL.to_vec();
        ranked.sort_by_key(|&g| (std::cmp::Reverse(favor(g)), g.index()));
        let top = ranked[0];
        let [x, y] = self.favor_vector(player);
        let length = ((x * x + y * y).sqrt() * 100.0).round() as u16;
        let total: u16 = God::ALL.iter().map(|&g| favor(g)).sum();
        let not_dark = God::ALL.iter().filter(|&&g| self.stage(g) < 2).count() as u16;
        let check = |kind, have, need| Check { kind, have, need };

        match condition {
            Condition::Registry { regions } => {
                let held = God::ALL
                    .iter()
                    .filter(|&&god| {
                        self.claims().any(|(hex, p)| {
                            p == player
                                && self.board.tile(hex).is_some_and(|t| {
                                    t.region == Some(god)
                                        && matches!(
                                            t.terrain,
                                            Terrain::Settlement | Terrain::Temple
                                        )
                                })
                        })
                    })
                    .count() as u16;
                vec![check(CheckKind::RegionsHeld, held, u16::from(regions))]
            }
            Condition::GodLimit => vec![
                check(CheckKind::TopFavor, favor(top), 8),
                check(CheckKind::Fanaticism, length, 60),
                check(CheckKind::GodStage, u16::from(self.stage(top)), 2),
            ],
            Condition::Fusion => {
                // The best adjacent pair on the ring: by the weaker of the two.
                let (a, b) = God::ALL
                    .iter()
                    .map(|&g| (g, God::from_index(g.index() + 1)))
                    .max_by_key(|&(a, b)| (favor(a).min(favor(b)), favor(a).max(favor(b))))
                    .expect("five pairs");
                let weaker = favor(a).min(favor(b));
                let past_light = u16::from(self.stage(a) >= 1) + u16::from(self.stage(b) >= 1);
                let share = ((favor(a) + favor(b)) * 100)
                    .checked_div(total)
                    .unwrap_or(0);
                vec![
                    check(CheckKind::SecondFavor, weaker, 8),
                    check(CheckKind::PairShare, share, 60),
                    check(CheckKind::PairNotLight, past_light, 2),
                ]
            }
            Condition::MiddlePath { dawns } => vec![
                check(CheckKind::TotalFavor, total, 10),
                check(CheckKind::Balance, 100u16.saturating_sub(length), 75),
                check(CheckKind::GodsNotDark, not_dark, 5),
                check(
                    CheckKind::Streak,
                    u16::from(progress.middle_streak),
                    u16::from(dawns),
                ),
            ],
            Condition::Overthrow { wins } => vec![check(
                CheckKind::Overthrows,
                u16::from(progress.overthrows),
                u16::from(wins),
            )],
            Condition::FirstAtTable { round } => {
                let best = self.players().map(|p| self.style(p)).max().unwrap_or(0);
                let sole = self.players().filter(|&p| self.style(p) == best).count() == 1;
                let leads = u16::from(best > 0 && sole && self.style(player) == best);
                vec![
                    check(CheckKind::Round, self.round.min(round) as u16, round as u16),
                    check(CheckKind::StyleLead, leads, 1),
                ]
            }
            Condition::Wager { refusals } => vec![check(
                CheckKind::Refusals,
                u16::from(progress.refusals),
                u16::from(refusals),
            )],
        }
    }

    pub fn meets(&self, player: PlayerId, condition: Condition) -> bool {
        self.checks(player, condition).iter().all(Check::met)
    }

    /// Where `player` stands on their secret `condition`: its own checks,
    /// then the round from which a secret may win.
    pub fn secret_checks(&self, player: PlayerId, condition: Condition) -> Vec<Check> {
        let mut checks = self.checks(player, condition);
        checks.push(Check {
            kind: CheckKind::Round,
            have: self.round.min(SECRET_FROM_ROUND) as u16,
            need: SECRET_FROM_ROUND as u16,
        });
        checks
    }

    /// `player`'s checks on `condition`, secret or open.
    pub fn checks_as(&self, player: PlayerId, condition: Condition, secret: bool) -> Vec<Check> {
        if secret {
            self.secret_checks(player, condition)
        } else {
            self.checks(player, condition)
        }
    }

    /// Dawn streaks: the Crown and the middle path.
    pub(super) fn count_dawn(&mut self) {
        for p in self.players().collect::<Vec<_>>() {
            let crowned = self.dominant == Some(p);
            let centred = {
                let checks = self.checks(p, Condition::MiddlePath { dawns: 0 });
                checks
                    .iter()
                    .filter(|c| c.kind != CheckKind::Streak)
                    .all(Check::met)
            };
            let progress = &mut self.progress[p.0 as usize];
            progress.crown_streak = if crowned {
                progress.crown_streak + 1
            } else {
                0
            };
            if !crowned {
                progress.refusals = 0;
            }
            progress.middle_streak = if centred {
                progress.middle_streak + 1
            } else {
                0
            };
        }
    }

    pub(super) fn count_overthrow(&mut self, winner: PlayerId, loser: PlayerId) {
        if self.dominant == Some(loser) {
            self.progress[winner.0 as usize].overthrows += 1;
        }
    }

    /// Ends the match if someone meets a condition. Checked in initiative
    /// order, so simultaneous wins go to whoever acts first.
    pub(super) fn check_victory(&mut self, events: &mut Vec<Event>) {
        if self.winner.is_some() {
            return;
        }
        let order = self.order.clone();
        for p in order {
            let mine = self
                .open
                .iter()
                .copied()
                .map(|c| (c, false))
                .chain(self.secret(p).map(|c| (c, true)))
                .find(|&(c, secret)| self.checks_as(p, c, secret).iter().all(Check::met))
                .map(|(c, _)| c);
            if let Some(condition) = mine {
                self.winner = Some((p, condition));
                events.push(Event::Victory {
                    player: p,
                    condition,
                });
                return;
            }
        }
    }
}
