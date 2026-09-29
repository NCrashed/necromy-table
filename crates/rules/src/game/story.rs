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
    /// Bring into the world a mechanic one's Great Deed needs (§21.6).
    Bring,
    /// Break a rival's deed on its eve (§21.6).
    Thwart,
    /// Come to a rival's feast (§21.8).
    Invitation,
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
    /// The world has this mechanic, whoever brought it.
    Bring(crate::features::Feature),
    /// This rival's deed is off its eve.
    Thwart(PlayerId),
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
    /// The darkest god brings something of its own into the world.
    Awakening,
}

impl Game {
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn lines_of(&self, player: PlayerId) -> impl Iterator<Item = &Line> {
        self.lines.iter().filter(move |l| l.owner == player)
    }

    /// How close a player is to winning, 0..=100: their deed, the average
    /// of its checks.
    pub fn nearness(&self, player: PlayerId) -> u16 {
        let Some(deed) = self.deed(player) else {
            return 0;
        };
        let checks = self.checks(player, deed);
        let sum: u32 = checks
            .iter()
            .map(|k| u32::from(k.have.min(k.need)) * 100 / u32::from(k.need.max(1)))
            .sum();
        (sum / checks.len().max(1) as u32) as u16
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
                Goal::Bring(feature) => self.has(feature),
                Goal::Thwart(rival) => !self.on_eve(rival),
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
            // And the story's end changes the world (§21.3).
            self.story_gift(line.owner, line.god, events);
        } else {
            events.push(Event::LineFailed { line });
            // A story failed: its god makes what it likes, near its owner.
            if self.wishes_made() {
                self.god_creates(line.owner, line.god, events);
            }
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

        // A deed on its eve: every rival is told how to break it (§21.6), by
        // the god whose element quenches the deed's patron.
        let eves: Vec<PlayerId> = self.players().filter(|&p| self.on_eve(p)).collect();
        for p in eves {
            let Some(deed) = self.deed(p) else {
                continue;
            };
            let teller = God::from_index(deed.patron().index() + 3);
            let rivals: Vec<PlayerId> = self.players().filter(|&r| r != p).collect();
            for r in rivals {
                let told = self.lines_of(r).any(|l| l.goal == Goal::Thwart(p));
                if !told && self.lines_of(r).count() < MAX_OPEN {
                    self.tell(r, teller, LineKind::Thwart, Goal::Thwart(p), 3, 0, events);
                    // Until the dusk the deed would be done at.
                    if let Some(line) = self.lines.last_mut() {
                        line.deadline = self.round + 2;
                    }
                }
            }
        }

        // Food enough for a feast: the others are asked to it (§21.8).
        let hosts: Vec<(PlayerId, Hex)> = self
            .players()
            .filter_map(|p| self.feast_hall(p).map(|h| (p, h)))
            .collect();
        for (host, hall) in hosts {
            let seats: Vec<Hex> = hall
                .all_neighbors()
                .into_iter()
                .filter(|&h| self.board.contains(h))
                .collect();
            let guests: Vec<PlayerId> = self.players().filter(|&r| r != host).collect();
            for (i, r) in guests.into_iter().enumerate() {
                let asked = self.lines_of(r).any(|l| l.kind == LineKind::Invitation);
                let Some(&seat) = seats.get(i % seats.len().max(1)) else {
                    continue;
                };
                if !asked && self.lines_of(r).count() < MAX_OPEN {
                    self.tell(
                        r,
                        God::Trishna,
                        LineKind::Invitation,
                        Goal::ReachHex(seat),
                        2,
                        0,
                        events,
                    );
                }
            }
        }

        // A board gone still stirs: no fight, nothing made, nobody near a
        // deed for a while (§21.6).
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

    /// An opportunity from the library, told by a god who fits it. What the
    /// one lagging's deed needs of the world comes first (§21.6).
    pub(super) fn opportunity(&mut self, p: PlayerId, events: &mut Vec<Event>) {
        let taken: Vec<LineKind> = self.lines_of(p).map(|l| l.kind).collect();
        let missing = self
            .deed(p)
            .and_then(|d| d.needs().iter().copied().find(|&f| self.can_awaken(f)));
        if let Some(feature) = missing
            && !taken.contains(&LineKind::Bring)
        {
            self.tell(
                p,
                feature.domain(),
                LineKind::Bring,
                Goal::Bring(feature),
                3,
                0,
                events,
            );
            return;
        }
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

    /// The world moves by itself when nobody moves it. The darkest god makes
    /// something of its own first, if the world may still take it (§21.6).
    fn stir(&mut self, events: &mut Vec<Event>) {
        if self.wishes_made() && !self.awakened_tonight() {
            let darkest = *God::ALL
                .iter()
                .max_by_key(|&&g| (self.stage(g), std::cmp::Reverse(g.index())))
                .expect("five gods");
            let own = self
                .awakenable(darkest)
                .into_iter()
                .find(|f| f.domain() == darkest);
            if let Some(feature) = own {
                events.push(Event::WorldStirred {
                    stir: WorldStir::Awakening,
                });
                let near = self.board.temple_of(darkest);
                self.awaken(None, darkest, feature, near, events);
                return;
            }
        }
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
            WorldStir::Awakening => false,
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
                        tile.corpse = Some(Corpse::fresh());
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
            WorldStir::Awakening => {}
        }
    }
}

impl Game {
    /// A story done: its god gives the world something for its owner
    /// (§21.3): what their deed needs, if it can come in, else land of its
    /// own at the rim nearest them.
    fn story_gift(&mut self, owner: PlayerId, god: God, events: &mut Vec<Event>) {
        if !self.wishes_made() {
            return;
        }
        let near = self.hex_of(owner);
        let need = self
            .deed(owner)
            .and_then(|d| d.needs().iter().copied().find(|&f| self.can_awaken(f)));
        if let Some(feature) = need
            && self.awaken(Some(owner), god, feature, near, events)
        {
            return;
        }
        self.rise(god, None, near, 2, events);
    }

    /// Something happened: the board is not still (§21.6). A fight, land
    /// made or unmade, a new rule, a card at a rival, a tribute, a trial
    /// passed, a deed on its eve.
    pub(super) fn note_life(&mut self, events: &[Event]) {
        let alive = events.iter().any(|e| {
            matches!(
                e,
                Event::LandRaised { .. }
                    | Event::WorldGrew { .. }
                    | Event::TerrainChanged { .. }
                    | Event::DeedEve { .. }
                    | Event::TributeGiven { .. }
                    | Event::TrialPassed { .. }
                    | Event::WindowOpened {
                        kind: super::WindowKind::Target { .. },
                        ..
                    }
            )
        });
        if alive {
            self.last_fight = self.round;
        }
    }
}
