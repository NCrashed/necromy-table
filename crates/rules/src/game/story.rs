//! The gods as storytellers (docs/design.md §8), offline.
//!
//! At dusk the rules measure the table: who lags behind, whether the
//! Dominant has had it too easy, whether the board has gone quiet. From
//! that they pick who gets a story line and what kind: an opportunity for
//! the one lagging (it asks for a move, a risk or a sacrifice, it does not
//! hand out points), a wager for a bored Dominant, a world event for a
//! quiet board. Lines come from a small library of templates, told in a
//! god's voice; the LLM storyteller will write them later from the same
//! building blocks.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::style::{Deed, StyleReason};
use super::{Event, Game, PlayerId};
use crate::board::{Corpse, Terrain};
use crate::gods::God;

/// Open lines a player may carry at once (§8.5).
pub const MAX_OPEN: usize = 2;
/// Rounds a line stays open.
pub const LINE_ROUNDS: u32 = 4;
/// Rounds without a fight before the world stirs by itself.
pub const CALM_ROUNDS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    /// Go to a god's temple.
    Pilgrimage,
    /// Give a god offerings.
    Tithe,
    /// Win a battle.
    Spoils,
    /// Take a settlement or a temple.
    NewLand,
    /// Put the dead to use: a card on a body.
    TheDeadCall,
    /// Ahamar's wager for a bored Dominant: keep the Crown without a fight.
    QuietCrown,
    /// Ahamar's trial for a bored Dominant: prove the Crown in battle.
    Trial,
    /// Pass a trial a god set near the one lagging (§20.2).
    Ordeal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Goal {
    ReachHex(Hex),
    /// Favour with the god to gain from here.
    Offer {
        god: God,
        amount: u16,
        from: u16,
    },
    WinBattle,
    Claim,
    Body,
    /// Stay out of battles until the deadline.
    AvoidBattle,
    /// Pass the trial on this hex.
    PassTrial(Hex),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub id: u32,
    pub owner: PlayerId,
    /// The god telling it.
    pub god: God,
    pub kind: LineKind,
    pub goal: Goal,
    /// Last round it can be done in.
    pub deadline: u32,
    pub style: u8,
    /// Style lost on failure; only wagers carry one.
    pub stake: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldStir {
    /// The dead rise near the Table.
    RisingDead,
    /// Groves spread where nobody looks.
    Overgrowth,
    /// Unrest: everyone is a little louder.
    Unrest,
}

impl Game {
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn lines_of(&self, player: PlayerId) -> impl Iterator<Item = &Line> {
        self.lines.iter().filter(move |l| l.owner == player)
    }

    /// How close a player is to winning, 0..=100: the best of their
    /// conditions, each the average of its checks.
    pub fn nearness(&self, player: PlayerId) -> u16 {
        self.open
            .iter()
            .copied()
            .map(|c| (c, false))
            .chain(self.secret(player).map(|c| (c, true)))
            .map(|(c, secret)| {
                let checks = self.checks_as(player, c, secret);
                let sum: u32 = checks
                    .iter()
                    .map(|k| u32::from(k.have.min(k.need)) * 100 / u32::from(k.need.max(1)))
                    .sum();
                (sum / checks.len().max(1) as u32) as u16
            })
            .max()
            .unwrap_or(0)
    }

    /// A deed may move a line forward (called from `record_deed`).
    pub(super) fn story_deed(&mut self, player: PlayerId, deed: Deed, events: &mut Vec<Event>) {
        let hits: Vec<u32> = self
            .lines
            .iter()
            .filter(|l| l.owner == player)
            .filter(|l| {
                matches!(
                    (l.goal, deed),
                    (Goal::WinBattle, Deed::Won)
                        | (Goal::Claim, Deed::Claimed)
                        | (Goal::Body, Deed::Body(_))
                )
            })
            .map(|l| l.id)
            .collect();
        for id in hits {
            self.finish_line(id, true, events);
        }
        // Any fight breaks a quiet-crown wager.
        if matches!(deed, Deed::Fought) {
            let broken: Vec<u32> = self
                .lines
                .iter()
                .filter(|l| l.owner == player && l.goal == Goal::AvoidBattle)
                .map(|l| l.id)
                .collect();
            for id in broken {
                self.finish_line(id, false, events);
            }
        }
    }

    /// A trial passed may close a line that asked for it.
    pub(super) fn story_trial(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        let done: Vec<u32> = self
            .lines
            .iter()
            .filter(|l| l.owner == player && l.goal == Goal::PassTrial(hex))
            .map(|l| l.id)
            .collect();
        for id in done {
            self.finish_line(id, true, events);
        }
    }

    /// Goals that are states, not deeds: standing somewhere, favour gained.
    pub(super) fn check_lines(&mut self, events: &mut Vec<Event>) {
        let done: Vec<u32> = self
            .lines
            .iter()
            .filter(|l| match l.goal {
                Goal::ReachHex(hex) => self.hex_of(l.owner) == hex,
                Goal::Offer { god, amount, from } => {
                    self.favor(l.owner, god).saturating_sub(from) >= amount
                }
                _ => false,
            })
            .map(|l| l.id)
            .collect();
        for id in done {
            self.finish_line(id, true, events);
        }
    }

    fn finish_line(&mut self, id: u32, won: bool, events: &mut Vec<Event>) {
        let Some(i) = self.lines.iter().position(|l| l.id == id) else {
            return;
        };
        let line = self.lines.remove(i);
        if won {
            events.push(Event::LineDone { line });
            self.first(line.owner, super::Novelty::FinishedLine, events);
            self.add_style(
                line.owner,
                i16::from(line.style),
                StyleReason::Story,
                events,
            );
            // The god remembers who answered, and gives from the loot (§20.3).
            self.offer(Some(line.owner), line.god, 1, events);
            self.gain_loot(line.owner, events);
        } else {
            events.push(Event::LineFailed { line });
            if line.stake > 0 {
                self.add_style(
                    line.owner,
                    -i16::from(line.stake),
                    StyleReason::Story,
                    events,
                );
            }
        }
    }

    /// Dusk: close lines past their deadline, then tell new ones.
    pub(super) fn storyteller(&mut self, events: &mut Vec<Event>) {
        // Deadlines. A quiet crown kept to the end is won; the rest are lost.
        let due: Vec<(u32, bool)> = self
            .lines
            .iter()
            .filter(|l| self.round >= l.deadline)
            .map(|l| (l.id, l.goal == Goal::AvoidBattle))
            .collect();
        for (id, won) in due {
            self.finish_line(id, won, events);
        }

        // The Dominant who has had it easy gets a wager from Ahamar.
        if let Some(d) = self.dominant {
            let streak = self.progress[d.0 as usize].crown_streak;
            let has_wager = self
                .lines_of(d)
                .any(|l| matches!(l.kind, LineKind::QuietCrown | LineKind::Trial));
            if streak >= 2 && !has_wager && self.lines_of(d).count() < MAX_OPEN {
                let kind = if self.rng.below(2) == 0 {
                    LineKind::QuietCrown
                } else {
                    LineKind::Trial
                };
                let goal = match kind {
                    LineKind::QuietCrown => Goal::AvoidBattle,
                    _ => Goal::WinBattle,
                };
                self.tell(d, God::Ahamar, kind, goal, 3, 2, events);
            }
        }

        // Those lagging get an opportunity, most lagging first, one each.
        let best = self.players().map(|p| self.nearness(p)).max().unwrap_or(0);
        let mut lagging: Vec<PlayerId> = self
            .players()
            .filter(|&p| Some(p) != self.dominant)
            .filter(|&p| self.nearness(p) + 25 <= best || self.nearness(p) == 0)
            .collect();
        lagging.sort_by_key(|&p| (self.nearness(p), p.0));
        for p in lagging {
            if self.lines_of(p).count() >= MAX_OPEN {
                continue;
            }
            self.opportunity(p, events);
        }

        // A quiet board stirs.
        if self.round.saturating_sub(self.last_fight) >= CALM_ROUNDS {
            self.last_fight = self.round;
            self.stir(events);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tell(
        &mut self,
        owner: PlayerId,
        god: God,
        kind: LineKind,
        goal: Goal,
        style: u8,
        stake: u8,
        events: &mut Vec<Event>,
    ) {
        self.next_line += 1;
        let line = Line {
            id: self.next_line,
            owner,
            god,
            kind,
            goal,
            deadline: self.round + LINE_ROUNDS,
            style,
            stake,
        };
        self.lines.push(line);
        events.push(Event::LineTold { line });
    }

    /// An opportunity from the library, told by a god who fits it.
    fn opportunity(&mut self, p: PlayerId, events: &mut Vec<Event>) {
        let taken: Vec<LineKind> = self.lines_of(p).map(|l| l.kind).collect();
        let choices: Vec<LineKind> = [
            LineKind::Pilgrimage,
            LineKind::Tithe,
            LineKind::Spoils,
            LineKind::NewLand,
            LineKind::TheDeadCall,
            LineKind::Ordeal,
        ]
        .into_iter()
        .filter(|k| !taken.contains(k))
        .filter(|k| match k {
            LineKind::Ordeal => self.has(super::Feature::Trials),
            LineKind::TheDeadCall => self.has(super::Feature::Bodies),
            _ => true,
        })
        .collect();
        let Some(&kind) = self.rng.pick(&choices) else {
            return;
        };
        // A trial within reach, set for them by the god of its land.
        if kind == LineKind::Ordeal {
            let at = self.hex_of(p);
            if let Some(hex) = self.trial_spot(Some((at, 2, 4)))
                && let Some(god) = self.set_trial(hex, events)
            {
                self.tell(p, god, kind, Goal::PassTrial(hex), 3, 0, events);
            }
            return;
        }
        // The god whose favour the player most lacks calls them.
        let god = *God::ALL
            .iter()
            .min_by_key(|&&g| (self.favor(p, g), g.index()))
            .expect("five gods");
        let (goal, god, style) = match kind {
            LineKind::Pilgrimage => (Goal::ReachHex(self.board.temple_of(god)), god, 2),
            LineKind::Tithe => (
                Goal::Offer {
                    god,
                    amount: 3,
                    from: self.favor(p, god),
                },
                god,
                2,
            ),
            LineKind::Spoils => (Goal::WinBattle, God::Trishna, 3),
            LineKind::NewLand => (Goal::Claim, God::Ahamar, 2),
            _ => (Goal::Body, God::Maya, 2),
        };
        self.tell(p, god, kind, goal, style, 0, events);
    }

    /// The world moves by itself when nobody moves it.
    fn stir(&mut self, events: &mut Vec<Event>) {
        let stirs: Vec<WorldStir> = [
            WorldStir::RisingDead,
            WorldStir::Overgrowth,
            WorldStir::Unrest,
        ]
        .into_iter()
        .filter(|s| match s {
            WorldStir::RisingDead => self.has(super::Feature::Bodies),
            WorldStir::Overgrowth => self.has(super::Feature::Groves),
            WorldStir::Unrest => true,
        })
        .collect();
        let stir = *self.rng.pick(&stirs).expect("unrest is always there");
        events.push(Event::WorldStirred { stir });
        match stir {
            WorldStir::RisingDead => {
                let spots: Vec<Hex> = (1..=2)
                    .flat_map(|r| Hex::ZERO.ring(r).collect::<Vec<_>>())
                    .filter(|&h| {
                        self.board.contains(h)
                            && self.board.tile(h).is_some_and(|t| t.corpse.is_none())
                            && self.occupant(h).is_none()
                    })
                    .collect();
                for _ in 0..3 {
                    if let Some(&hex) = self.rng.pick(&spots)
                        && let Some(tile) = self.board.tile_mut(hex)
                        && tile.corpse.is_none()
                    {
                        tile.corpse = Some(Corpse { age: 0 });
                        events.push(Event::CorpseAppeared { hex });
                    }
                }
            }
            WorldStir::Overgrowth => {
                let spots: Vec<Hex> = self
                    .board
                    .tiles()
                    .filter(|(h, t)| t.terrain == Terrain::Plains && self.occupant(*h).is_none())
                    .map(|(h, _)| h)
                    .collect();
                for _ in 0..2 {
                    if let Some(&hex) = self.rng.pick(&spots) {
                        self.grow(hex, events);
                    }
                }
                self.offer(None, God::Bhava, 1, events);
            }
            WorldStir::Unrest => {
                for p in self.players().collect::<Vec<_>>() {
                    self.add_threat(p, 1, events);
                }
            }
        }
    }
}
