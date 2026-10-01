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
/// Letters offered at once, from as many gods.
pub const LETTERS: usize = 2;
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
    /// A god's letter asking for a thing done (docs/storyteller-plan.md).
    Errand,
    /// A case: something happening near, with two ways to close it
    /// (`hooks.rs`).
    Case(super::hooks::Hook),
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
    /// A thing done by the line's owner.
    Do(Doing),
}

/// A thing a line can ask to be done, read off the events of a move or the
/// deeds the story hears (letters, docs/storyteller-plan.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Doing {
    Build,
    Kindle,
    Bury,
    Gift,
    Tame,
    Sell,
    OpenFair,
    Rebuild,
    FellMob,
    Hide,
    StoreFood,
    Douse,
    /// Cargo won off a rival.
    Seize,
    /// Work done under the ruins.
    Delve,
    /// A card on a body, by its verb.
    Seed,
    Fuel,
    Rest,
    Dissolve,
    Raise,
}

/// The other way to close a line, and the god it pleases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fork {
    pub goal: Goal,
    pub god: God,
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
    /// Another way to close it, for another god.
    pub fork: Option<Fork>,
    /// Where it is to be done, if one place.
    pub at: Option<Hex>,
    /// Taken from a god's letter (one open at a time).
    pub letter: bool,
    /// The chapter of the god's thread it is (1..=3: Sign, Voice, Chosen);
    /// 0 when it is no letter.
    pub chapter: u8,
    /// A temptation: done, it betrays this god (its favour falls).
    pub betrays: Option<God>,
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

    /// Lines open against the limit (`MAX_OPEN`): a case is a layer of
    /// its own and does not count.
    pub fn open_lines(&self, player: PlayerId) -> usize {
        self.lines_of(player)
            .filter(|l| !matches!(l.kind, LineKind::Case(_)))
            .count()
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

    /// Lines (of `owner`, or anyone's) whose goal, or whose fork, `met`
    /// says is reached: done, for the god of the way taken.
    fn close_met(
        &mut self,
        owner: Option<PlayerId>,
        met: impl Fn(&Game, &Line, Goal) -> bool,
        events: &mut Vec<Event>,
    ) {
        let hits: Vec<(u32, bool)> = self
            .lines
            .iter()
            .filter(|l| owner.is_none_or(|o| l.owner == o))
            .filter_map(|l| {
                if met(self, l, l.goal) {
                    Some((l.id, false))
                } else if l.fork.is_some_and(|f| met(self, l, f.goal)) {
                    Some((l.id, true))
                } else {
                    None
                }
            })
            .collect();
        for (id, forked) in hits {
            if forked && let Some(l) = self.lines.iter_mut().find(|l| l.id == id) {
                let f = l.fork.take().expect("a fork");
                l.goal = f.goal;
                l.god = f.god;
            }
            self.finish_line(id, true, events);
        }
    }

    /// A deed may move a line forward (called from `record_deed`).
    pub(super) fn story_deed(&mut self, player: PlayerId, deed: Deed, events: &mut Vec<Event>) {
        use super::style::BodyVerb;
        self.close_met(
            Some(player),
            |_, _, goal| {
                matches!(
                    (goal, deed),
                    (Goal::WinBattle, Deed::Won)
                        | (Goal::Claim, Deed::Claimed)
                        | (Goal::Body, Deed::Body(_))
                        | (Goal::Do(Doing::Seed), Deed::Body(BodyVerb::Seed))
                        | (Goal::Do(Doing::Fuel), Deed::Body(BodyVerb::Fuel))
                        | (Goal::Do(Doing::Rest), Deed::Body(BodyVerb::Rest))
                        | (Goal::Do(Doing::Dissolve), Deed::Body(BodyVerb::Dissolve))
                        | (Goal::Do(Doing::Raise), Deed::Body(BodyVerb::Legion))
                )
            },
            events,
        );
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

    /// What a move's events did that a line can ask for.
    pub(super) fn story_events(&mut self, events: &mut Vec<Event>) {
        let done: Vec<(PlayerId, Doing)> = events.iter().filter_map(doing).collect();
        for (player, what) in done {
            self.close_met(Some(player), |_, _, goal| goal == Goal::Do(what), events);
        }
    }

    /// A god asked for a trial (a wish) tells `player` to pass the one it
    /// set on `hex`.
    pub(super) fn tell_ordeal(
        &mut self,
        player: PlayerId,
        god: God,
        hex: Hex,
        events: &mut Vec<Event>,
    ) {
        self.tell(
            player,
            god,
            LineKind::Ordeal,
            Goal::PassTrial(hex),
            3,
            0,
            events,
        );
    }

    /// A trial passed may close a line that asked for it.
    pub(super) fn story_trial(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        self.close_met(
            Some(player),
            |_, _, goal| goal == Goal::PassTrial(hex),
            events,
        );
    }

    /// Goals that are states, not deeds: standing somewhere, favour gained.
    pub(super) fn check_lines(&mut self, events: &mut Vec<Event>) {
        self.close_met(
            None,
            |g, l, goal| match goal {
                Goal::ReachHex(hex) => g.hex_of(l.owner) == hex,
                Goal::Offer { god, amount, from } => {
                    g.favor(l.owner, god).saturating_sub(from) >= amount
                }
                Goal::Bring(feature) => g.has(feature),
                Goal::Thwart(rival) => !g.on_eve(rival),
                _ => false,
            },
            events,
        );
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
            // A letter's chapter raises its god's patronage to its step; a
            // temptation done turns the betrayed god away.
            self.letter_done(&line, events);
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
            if streak >= 2 && !has_wager && self.open_lines(d) < MAX_OPEN {
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
                if !told && self.open_lines(r) < MAX_OPEN {
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
                if !asked && self.open_lines(r) < MAX_OPEN {
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

        // Letters from the gods: two each, the one lagging most first.
        self.deal_letters(events);
        // And what happens near: a case each (`hooks.rs`).
        self.deal_cases(events);

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
            fork: None,
            at: None,
            letter: false,
            chapter: 0,
            betrays: None,
        };
        self.lines.push(line);
        events.push(Event::LineTold { line });
    }

    /// The letters waiting for `player`: lines offered, not yet taken.
    pub fn letters(&self, player: PlayerId) -> &[Line] {
        self.letters
            .get(player.0 as usize)
            .map_or(&[][..], Vec::as_slice)
    }

    /// Dusk: last night's letters fade; whoever has no letter's line open
    /// gets two, from two gods, the one lagging most first
    /// (docs/storyteller-plan.md).
    fn deal_letters(&mut self, events: &mut Vec<Event>) {
        for l in &mut self.letters {
            l.clear();
        }
        let mut order: Vec<PlayerId> = self.players().collect();
        order.sort_by_key(|&p| (self.nearness(p), p.0));
        for p in order {
            if self.lines_of(p).any(|l| l.letter) || self.open_lines(p) >= MAX_OPEN {
                continue;
            }
            // The deed's patron writes first, the rest in an order of the
            // table's choosing.
            let mut gods: Vec<God> = God::ALL.to_vec();
            let mut picked: Vec<God> = Vec::new();
            if let Some(d) = self.deed(p) {
                picked.push(d.patron());
                gods.retain(|&g| g != d.patron());
            }
            while !gods.is_empty() {
                let i = self.rng.below(gods.len() as u32) as usize;
                picked.push(gods.remove(i));
            }
            // A god jealous of the last letter taken writes first: a
            // temptation to betray the one it envies.
            let mut letters: Vec<Line> = Vec::new();
            if let Some((jealous, victim)) = self.grudges[p.0 as usize].take()
                && let Some(line) = self.temptation(p, jealous, victim)
            {
                letters.push(line);
            }
            for god in picked {
                if letters.len() >= LETTERS {
                    break;
                }
                if letters.iter().any(|l| l.god == god) {
                    continue;
                }
                if let Some(line) = self.letter(p, god) {
                    letters.push(line);
                }
            }
            if !letters.is_empty() {
                self.letters[p.0 as usize] = letters.clone();
                events.push(Event::LettersCame { player: p, letters });
            }
        }
    }

    /// A letter from `god` to `p`: one of the things the god likes done
    /// that can be done near them now, maybe with another god's way out.
    pub(super) fn letter(&mut self, p: PlayerId, god: God) -> Option<Line> {
        use super::Feature;
        let me = self.hex_of(p);
        let near = |hexes: Vec<Hex>, reach: u32| {
            hexes
                .into_iter()
                .filter(|h| h.unsigned_distance_to(me) <= reach)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        };
        let corpse = near(self.board.corpses().map(|(h, _)| h).collect(), 5);
        let holds =
            |g: &Game, e: crate::cards::Effect| g.hand(p).iter().any(|&c| g.def(c).effect == e);
        let fork = |goal: Goal, god: God| Some(Fork { goal, god });
        let mut options: Vec<(Goal, Option<Fork>, Option<Hex>)> = Vec::new();
        use crate::cards::Effect;
        match god {
            God::Bhava => {
                if self.has(Feature::Companions) {
                    let beast = near(
                        self.mobs
                            .iter()
                            .filter(|m| m.is_beast())
                            .map(|m| m.hex)
                            .collect(),
                        6,
                    );
                    if let Some(h) = beast {
                        options.push((
                            Goal::Do(Doing::Tame),
                            fork(Goal::Do(Doing::FellMob), God::Trishna),
                            Some(h),
                        ));
                    }
                }
                if let Some(h) = corpse
                    && holds(self, Effect::BodySeed)
                {
                    options.push((
                        Goal::Do(Doing::Seed),
                        fork(Goal::Do(Doing::Fuel), God::Trishna),
                        Some(h),
                    ));
                }
            }
            God::Trishna => {
                options.push((
                    Goal::WinBattle,
                    fork(Goal::Do(Doing::FellMob), God::Zaga),
                    None,
                ));
                if self.has(Feature::Fires) {
                    let fuel = near(
                        self.board
                            .land()
                            .filter(|(h, t)| t.terrain.burns() && self.owner(*h) != Some(p))
                            .map(|(h, _)| h)
                            .collect(),
                        4,
                    );
                    if let Some(h) = fuel {
                        options.push((Goal::Do(Doing::Kindle), None, Some(h)));
                    }
                }
                if let Some((h, _)) = self.fairs().next()
                    && (self.has(Feature::Goods))
                {
                    let gift = self.has(Feature::Rulers).then_some(Fork {
                        goal: Goal::Do(Doing::Gift),
                        god: God::Ahamar,
                    });
                    options.push((Goal::Do(Doing::Sell), gift, Some(h)));
                }
                if self.has(Feature::Fields)
                    && self
                        .claims()
                        .any(|(h, o)| o == p && self.own_settlement(p, h))
                {
                    let food = near(
                        self.loads
                            .iter()
                            .filter(|(_, c)| *c == super::Cargo::Food)
                            .map(|(h, _)| *h)
                            .collect(),
                        6,
                    );
                    if let Some(h) = food {
                        options.push((Goal::Do(Doing::StoreFood), None, Some(h)));
                    }
                }
            }
            God::Zaga => {
                let graveyard = self
                    .board
                    .land()
                    .any(|(_, t)| t.terrain == Terrain::Graveyard);
                if let Some(h) = corpse {
                    if self.has(Feature::Burial) && graveyard {
                        options.push((
                            Goal::Do(Doing::Bury),
                            fork(Goal::Do(Doing::Dissolve), God::Maya),
                            Some(h),
                        ));
                    }
                    if holds(self, Effect::BodyRest) {
                        options.push((
                            Goal::Do(Doing::Rest),
                            fork(Goal::Do(Doing::Dissolve), God::Maya),
                            Some(h),
                        ));
                    }
                }
                let trial = near(
                    self.trials()
                        .iter()
                        .map(|t| t.hex)
                        .filter(|&h| self.trial_for(p, h).is_some())
                        .collect(),
                    5,
                );
                if let Some(h) = trial {
                    options.push((Goal::PassTrial(h), None, Some(h)));
                }
                let undead = near(
                    self.mobs
                        .iter()
                        .filter(|m| m.is_undead())
                        .map(|m| m.hex)
                        .collect(),
                    5,
                );
                if let Some(h) = undead {
                    let raise = self.has(Feature::Legion).then_some(Fork {
                        goal: Goal::Do(Doing::Tame),
                        god: God::Maya,
                    });
                    options.push((Goal::Do(Doing::FellMob), raise, Some(h)));
                }
            }
            God::Ahamar => {
                let free = near(
                    self.board
                        .land()
                        .filter(|(h, t)| {
                            t.terrain == Terrain::Settlement && self.owner(*h).is_none()
                        })
                        .map(|(h, _)| h)
                        .collect(),
                    6,
                );
                if let Some(h) = free {
                    options.push((Goal::Claim, None, Some(h)));
                }
                if self.has(Feature::Buildings) {
                    let site = near(
                        self.claims()
                            .filter(|&(h, o)| {
                                o == p && self.own_settlement(p, h) && self.building(h).is_none()
                            })
                            .map(|(h, _)| h)
                            .collect(),
                        6,
                    );
                    if let Some(h) = site {
                        let fair = self.has(Feature::Fairs).then_some(Fork {
                            goal: Goal::Do(Doing::OpenFair),
                            god: God::Trishna,
                        });
                        options.push((Goal::Do(Doing::Build), fair, Some(h)));
                    }
                }
                if self.has(Feature::Rulers) {
                    let ruler = near(self.rulers().map(|(h, _)| h).collect(), 5);
                    if let Some(h) = ruler {
                        options.push((Goal::Do(Doing::Gift), None, Some(h)));
                    }
                }
                let ruins = near(
                    self.board
                        .land()
                        .filter(|(_, t)| t.terrain == Terrain::Ruins)
                        .map(|(h, _)| h)
                        .collect(),
                    5,
                );
                if let Some(h) = ruins {
                    options.push((Goal::Do(Doing::Rebuild), None, Some(h)));
                }
            }
            God::Maya => {
                if self.has(Feature::Stealth) {
                    options.push((Goal::Do(Doing::Hide), None, None));
                }
                if let Some(h) = corpse {
                    if holds(self, Effect::BodyDissolve) {
                        let bury = self.has(Feature::Burial).then_some(Fork {
                            goal: Goal::Do(Doing::Bury),
                            god: God::Zaga,
                        });
                        options.push((Goal::Do(Doing::Dissolve), bury, Some(h)));
                    }
                    if holds(self, Effect::BodyLegion) {
                        options.push((
                            Goal::Do(Doing::Raise),
                            fork(Goal::Do(Doing::Rest), God::Zaga),
                            Some(h),
                        ));
                    }
                }
            }
        }
        // Every god can call one to its temple.
        let temple = self.board.temple_of(god);
        if options.is_empty() && temple.unsigned_distance_to(me) > 1 {
            options.push((Goal::ReachHex(temple), None, Some(temple)));
        }
        // A thing of its own the world lacks: ask for it at dusk, and the
        // thread teaches wishes (§21.2).
        if let Some(&f) = self.awakenable(god).iter().find(|f| f.domain() == god) {
            options.push((Goal::Bring(f), None, None));
        }
        // The god's stage colours what it asks: its light the gentle
        // things, its dark the harsh ones (§8.4).
        let stage = self.stage(god);
        let toned: Vec<_> = options
            .iter()
            .copied()
            .filter(|(goal, ..)| harsh(*goal) == (stage == 2))
            .collect();
        let pool = if stage != 1 && !toned.is_empty() {
            toned
        } else {
            options
        };
        if pool.is_empty() {
            return None;
        }
        let i = self.rng.below(pool.len() as u32) as usize;
        let &(goal, fork, at) = pool.get(i)?;
        // The chapter of its thread: the patronage it leads to.
        let chapter = (self.patronage(p, god) as u8 + 1).min(3);
        Some(self.letter_line(p, god, goal, fork, at, chapter, None))
    }

    /// A letter's line, not told yet.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn letter_line(
        &mut self,
        p: PlayerId,
        god: God,
        goal: Goal,
        fork: Option<Fork>,
        at: Option<Hex>,
        chapter: u8,
        betrays: Option<God>,
    ) -> Line {
        let kind = match goal {
            Goal::ReachHex(_) => LineKind::Pilgrimage,
            Goal::WinBattle => LineKind::Spoils,
            Goal::Claim => LineKind::NewLand,
            Goal::PassTrial(_) => LineKind::Ordeal,
            Goal::Bring(_) => LineKind::Bring,
            _ => LineKind::Errand,
        };
        self.next_line += 1;
        Line {
            id: self.next_line,
            owner: p,
            god,
            kind,
            goal,
            deadline: 0,
            // A betrayal pays twice.
            style: if betrays.is_some() { 4 } else { 2 },
            stake: 0,
            fork,
            at,
            letter: true,
            chapter,
            betrays,
        }
    }

    /// The jealous god's letter: harm what `victim` holds dear, in its
    /// land, for twice the Style; done, `victim` turns away.
    pub(super) fn temptation(&mut self, p: PlayerId, jealous: God, victim: God) -> Option<Line> {
        use super::Feature;
        let me = self.hex_of(p);
        let in_land = |g: &Game, f: &dyn Fn(Hex, &crate::board::Tile) -> bool| {
            g.board
                .land()
                .filter(|(h, t)| {
                    t.region == Some(victim) && h.unsigned_distance_to(me) <= 6 && f(*h, t)
                })
                .map(|(h, _)| h)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        };
        let holds =
            |g: &Game, e: crate::cards::Effect| g.hand(p).iter().any(|&c| g.def(c).effect == e);
        let corpse = in_land(self, &|_, t| t.corpse.is_some());
        let fire = |g: &Game, settlements: bool| {
            g.has(Feature::Fires)
                .then(|| {
                    in_land(g, &|h, t| {
                        t.terrain.burns()
                            && (!settlements || t.terrain == Terrain::Settlement)
                            && g.owner(h) != Some(p)
                    })
                })
                .flatten()
                .map(|h| (Goal::Do(Doing::Kindle), Some(h)))
        };
        use crate::cards::Effect;
        let (goal, at) = match victim {
            // Bhava's woods burnt.
            God::Bhava => fire(self, false),
            // Trishna's own settlements taken from her table.
            God::Trishna => in_land(self, &|h, t| {
                t.terrain == Terrain::Settlement && self.owner(h) != Some(p)
            })
            .map(|h| (Goal::Claim, Some(h)))
            .or_else(|| fire(self, false)),
            // Zaga's dead disturbed.
            God::Zaga => corpse
                .and_then(|h| {
                    if holds(self, Effect::BodyLegion) {
                        Some((Goal::Do(Doing::Raise), Some(h)))
                    } else if holds(self, Effect::BodyFuel) {
                        Some((Goal::Do(Doing::Fuel), Some(h)))
                    } else {
                        None
                    }
                })
                .or_else(|| fire(self, false)),
            // Ahamar's order set alight.
            God::Ahamar => fire(self, true).or_else(|| fire(self, false)),
            // Maya's dead kept from dissolving.
            God::Maya => corpse
                .filter(|_| {
                    self.has(Feature::Burial)
                        && self
                            .board
                            .land()
                            .any(|(_, t)| t.terrain == Terrain::Graveyard)
                })
                .map(|h| (Goal::Do(Doing::Bury), Some(h)))
                .or_else(|| fire(self, false)),
        }?;
        let chapter = (self.patronage(p, jealous) as u8 + 1).min(3);
        Some(self.letter_line(p, jealous, goal, None, at, chapter, Some(victim)))
    }

    /// `player` takes the letter at `index`: its line opens now.
    pub(super) fn take_letter(
        &mut self,
        player: PlayerId,
        index: u8,
        events: &mut Vec<Event>,
    ) -> Result<(), super::RuleError> {
        let Some(mut line) = self.letters(player).get(index as usize).copied() else {
            return Err(super::RuleError::InvalidTarget);
        };
        line.deadline = self.round + LINE_ROUNDS;
        self.letters[player.0 as usize].clear();
        // The god whose element quenches the one served takes it ill: its
        // next letter tempts to betrayal (docs/storyteller-plan.md).
        if line.betrays.is_none() {
            self.grudges[player.0 as usize] = Some((quencher(line.god), line.god));
        }
        self.lines.push(line);
        events.push(Event::LineTold { line });
        Ok(())
    }

    /// `player` lets the letters lie: none taken tonight.
    pub(super) fn decline_letters(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), super::RuleError> {
        if self.letters(player).is_empty() {
            return Err(super::RuleError::InvalidTarget);
        }
        self.letters[player.0 as usize].clear();
        events.push(Event::LettersSetAside { player });
        Ok(())
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
                // Near the one lagging most: a case for them to answer.
                let centre = self
                    .players()
                    .min_by_key(|&p| (self.nearness(p), p.0))
                    .map_or(Hex::ZERO, |p| self.hex_of(p));
                let spots: Vec<Hex> = (1..=2)
                    .flat_map(|r| centre.ring(r).collect::<Vec<_>>())
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

/// Who did what of the things a line can ask for, by an event.
fn doing(e: &Event) -> Option<(PlayerId, Doing)> {
    Some(match *e {
        Event::Built { player, .. } => (player, Doing::Build),
        Event::FireStarted { by: Some(by), .. } => (by, Doing::Kindle),
        Event::Buried { player, .. } => (player, Doing::Bury),
        Event::Gifted { player, .. } => (player, Doing::Gift),
        Event::CompanionJoined { player, .. } => (player, Doing::Tame),
        Event::GoodsSold { player, .. } => (player, Doing::Sell),
        Event::FairOpened { player, .. } => (player, Doing::OpenFair),
        Event::SettlementRebuilt { player, .. } => (player, Doing::Rebuild),
        Event::MobFell { by, .. } => (by, Doing::FellMob),
        Event::Hid { player, .. } => (player, Doing::Hide),
        Event::FoodStored { player, .. } => (player, Doing::StoreFood),
        Event::FireOut { by: Some(by), .. } => (by, Doing::Douse),
        Event::CargoSeized { player, .. } => (player, Doing::Seize),
        Event::Delved { player, .. } => (player, Doing::Delve),
        _ => return None,
    })
}

/// A harsh thing to ask: what a god in its dark stage asks for (§8.4).
pub(super) fn harsh(goal: Goal) -> bool {
    matches!(
        goal,
        Goal::WinBattle
            | Goal::Claim
            | Goal::Do(
                Doing::Kindle | Doing::FellMob | Doing::Fuel | Doing::Dissolve | Doing::Raise
            )
    )
}

/// The god whose element quenches `god`'s: jealous of its favourites.
pub fn quencher(god: God) -> God {
    God::from_index(god.index() + 3)
}

/// Favour a betrayed god takes back.
pub const BETRAYAL: u16 = 3;

impl Game {
    fn letter_done(&mut self, line: &Line, events: &mut Vec<Event>) {
        if !line.letter {
            return;
        }
        let p = line.owner;
        if let Some(victim) = line.betrays {
            let f = &mut self.favor[p.0 as usize][victim.index()];
            *f = f.saturating_sub(BETRAYAL);
            events.push(Event::Betrayed {
                player: p,
                god: victim,
            });
            return;
        }
        let step = [super::SIGN, super::VOICE, super::CHOSEN];
        let Some(&tier) = step.get(usize::from(line.chapter.max(1)) - 1) else {
            return;
        };
        let f = &mut self.favor[p.0 as usize][line.god.index()];
        if *f < tier {
            *f = tier;
            let patronage = self.patronage(p, line.god);
            events.push(Event::Patron {
                player: p,
                god: line.god,
                patronage,
            });
        }
    }
}
