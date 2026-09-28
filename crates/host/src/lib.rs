//! The authoritative table (docs/design.md §17).
//!
//! A [`Table`] owns one match: the whole `Game`, the bots on their seats,
//! and the gods' voice. Seats send it [`ToTable`] messages and read back
//! [`FromTable`] ones; every seat hears only what its player may know
//! (`Game::view_for`). Nothing here knows about sockets or Bevy: the single
//! player game runs a table in its own process (§17.3), the dedicated
//! server runs the same table behind a transport.
//!
//! Pacing: bots act one step at a time, and only once every watching seat
//! has shown the last change (dice rolled, tokens walked) or a timeout ran
//! out, so a slow screen never holds the table for long.

use std::sync::Arc;

use necromy_oracle::{Job, Oracle, prompt};
use necromy_rules::{Event, Game, God, Intent, Phase, PlayerId, RuleError, Setup, bot};
use serde::{Deserialize, Serialize};

mod clock;

use clock::SeatClock;
pub use clock::{Clock, Decision, Timers};

/// Seconds between two bot steps.
pub const BOT_STEP_SECS: f32 = 0.35;
/// Longest wait for a seat to show a change before bots go on without it.
pub const SHOW_TIMEOUT_SECS: f32 = 12.0;
/// Cosmetic voice jobs are skipped while this many already wait.
const MAX_QUEUED_VOICES: usize = 2;
/// Letters of a wish draft passed on to the others (the panel takes 200).
const MAX_DRAFT: usize = 240;

/// Who plays a seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Seat {
    /// A person, through a client.
    Human,
    Bot,
    /// Dev aid: a bot plays for a client that watches the seat. With
    /// `wish_by_hand` the bot leaves this seat's wish to the client.
    Autoplay {
        wish_by_hand: bool,
    },
    /// A tutorial's rival: stands still and passes. It keeps its turn open
    /// (so a script can play for it, `act_as`) until something waits on it
    /// or everyone else is done.
    Dummy,
}

impl Seat {
    /// A client watches this seat and gets its view.
    pub fn watched(self) -> bool {
        !matches!(self, Seat::Bot | Seat::Dummy)
    }
}

pub struct Config {
    pub seed: u64,
    pub champions: Vec<God>,
    /// One per champion.
    pub seats: Vec<Seat>,
    /// Randomness clients must not know; it hides cards in their views.
    pub salt: u64,
    /// `llama-server` address for the gods' voice, `None` for templates only.
    pub oracle: Option<String>,
    /// Clocks on people's decisions; `None` lets them think forever (alone).
    pub timers: Option<Timers>,
}

/// A seat's message to the table.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToTable {
    Act(Intent),
    /// The Dominant's wish in their own words, for the model to judge.
    Wish {
        god: God,
        text: String,
    },
    /// The Dominant's wish as it is being written, for the others to watch:
    /// the god chosen so far and the words.
    Draft {
        god: Option<God>,
        text: String,
    },
    /// The seat has shown everything up to this update.
    Shown(u32),
}

/// The table's message to a seat.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FromTable {
    /// What changed and the state after it, as this seat sees them.
    Update {
        serial: u32,
        events: Vec<Event>,
        view: Box<Game>,
    },
    /// The seat's intent went through; its update came just before.
    /// Every `Act` gets exactly one `Accepted` or `Rejected`, in order, so a
    /// client can hold its next intent until the last one is answered.
    Accepted,
    /// The seat's intent broke the rules; nothing changed.
    Rejected(RuleError),
    Oracle(OracleNews),
    /// The clock this seat should watch now, sent after every update; `None`
    /// when nothing runs for it.
    Clock(Option<Clock>),
    /// Another seat's wish as it is being written (`ToTable::Draft`).
    Drafting {
        player: PlayerId,
        god: Option<God>,
        text: String,
    },
    /// A clock ran out and the table decided for the seat.
    TimedOut(Decision),
}

/// News of the gods' voice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OracleNews {
    /// Whether the model answers; wishes in free words need it.
    Online(bool),
    /// The main voice is down and a spare answers (`NECROMY_ORACLE` lists
    /// it): the game shows it, so whoever runs the server can react.
    Spare(bool),
    /// A god is thinking about this seat's wish, or stopped.
    Listening(Option<God>),
    /// The wish could not be heard; the seat may try again.
    NotHeard(String),
    /// A god's words for the wish granted in update `serial`.
    WishVoice { serial: u32, text: String },
    /// A god's words for a story line.
    LineVoice { line: u32, text: String },
}

enum Purpose {
    Wish {
        seat: PlayerId,
        god: God,
        text: String,
    },
    WishVoice {
        serial: u32,
    },
    LineVoice {
        owner: PlayerId,
        line: u32,
    },
}

struct Voice {
    oracle: Oracle,
    route: Arc<necromy_oracle::Route>,
    /// The voice's state as last told to the seats.
    told: Option<necromy_oracle::Voice>,
    next_id: u64,
    /// In send order.
    pending: Vec<(u64, Purpose)>,
}

pub struct Table {
    game: Game,
    seats: Vec<Seat>,
    salt: u64,
    serial: u32,
    /// Per seat, the last update it has shown.
    shown: Vec<u32>,
    /// Seconds since the last update.
    since_update: f32,
    /// Seconds since the last bot step.
    since_bot: f32,
    outbox: Vec<Vec<FromTable>>,
    voice: Option<Voice>,
    timers: Option<Timers>,
    clocks: Vec<SeatClock>,
}

impl Table {
    pub fn new(config: Config) -> Table {
        assert_eq!(
            config.seats.len(),
            config.champions.len(),
            "a seat per champion"
        );
        let (game, events) = Game::new(Setup {
            seed: config.seed,
            champions: config.champions,
        });
        Table::from_game(
            game,
            events,
            config.seats,
            config.salt,
            config.oracle,
            config.timers,
        )
    }

    /// A table around a match already set up (a scripted scene).
    pub fn from_game(
        game: Game,
        events: Vec<Event>,
        seats: Vec<Seat>,
        salt: u64,
        oracle: Option<String>,
        timers: Option<Timers>,
    ) -> Table {
        assert_eq!(seats.len(), game.champions().len(), "a seat per champion");
        let config = Config {
            seed: game.seed(),
            champions: Vec::new(),
            seats,
            salt,
            oracle,
            timers,
        };
        let voice = config.oracle.map(|addr| {
            // One probe per address list and process, shared by every table.
            let route = necromy_oracle::Route::shared(&addr);
            Voice {
                oracle: Oracle::spawn(route.clone()),
                route,
                told: None,
                next_id: 0,
                pending: Vec::new(),
            }
        });
        let n = config.seats.len();
        let mut table = Table {
            game,
            seats: config.seats,
            salt: config.salt,
            serial: 0,
            shown: vec![0; n],
            since_update: 0.0,
            since_bot: 0.0,
            outbox: vec![Vec::new(); n],
            voice,
            timers: config.timers,
            clocks: vec![SeatClock::default(); n],
        };
        table.broadcast(&events);
        table
    }

    /// The whole match. Only the host may read it: clients get views.
    pub fn game(&self) -> &Game {
        &self.game
    }

    pub fn seats(&self) -> &[Seat] {
        &self.seats
    }

    /// Hand a seat to someone else: a bot while its player is away (§17.1),
    /// the player again when they come back. A seat that becomes watched
    /// gets a fresh view at once, so a returning client can catch up.
    pub fn set_seat(&mut self, seat: PlayerId, kind: Seat) {
        let i = seat.0 as usize;
        if i >= self.seats.len() {
            return;
        }
        self.seats[i] = kind;
        self.outbox[i].clear();
        if kind.watched() {
            let salt = self.next_salt();
            let view = self.game.view_for(Some(seat), salt);
            self.shown[i] = self.serial;
            let serial = self.serial;
            self.send(
                seat,
                FromTable::Update {
                    serial,
                    events: Vec::new(),
                    view: Box::new(view),
                },
            );
        }
        self.clocks[i].clear();
        self.follow_clock(seat, &[]);
    }

    /// Messages waiting for `seat`, oldest first.
    pub fn drain(&mut self, seat: PlayerId) -> Vec<FromTable> {
        self.outbox
            .get_mut(seat.0 as usize)
            .map(std::mem::take)
            .unwrap_or_default()
    }

    pub fn submit(&mut self, seat: PlayerId, message: ToTable) {
        let Some(kind) = self.seats.get(seat.0 as usize).copied() else {
            return;
        };
        if !kind.watched() {
            return;
        }
        match message {
            ToTable::Act(intent) => {
                let answer = match self.act(seat, intent) {
                    Ok(()) => FromTable::Accepted,
                    Err(err) => FromTable::Rejected(err),
                };
                self.send(seat, answer);
            }
            ToTable::Wish { god, text } => self.ask_wish(seat, god, text),
            ToTable::Draft { god, text } => self.share_draft(seat, god, text),
            ToTable::Shown(serial) => {
                let s = &mut self.shown[seat.0 as usize];
                *s = (*s).max(serial.min(self.serial));
            }
        }
    }

    /// Time passes: the model's answers come in, bots take their steps.
    pub fn tick(&mut self, dt: f32) {
        self.since_update += dt;
        self.since_bot += dt;
        self.hear();
        self.run_clocks(dt);
        self.run_bots();
    }

    /// What a dummy does now, if anything: it answers every window with
    /// nothing and refuses a wish; its own turn it ends only once someone
    /// waits on it or everyone else is done, so a script may still play
    /// for it meanwhile.
    fn dummy_intent(&self, player: PlayerId) -> Option<Intent> {
        let g = &self.game;
        if g.wish_due() == Some(player) {
            return Some(Intent::RefuseWish);
        }
        if g.to_answer(player).is_some() {
            return Some(if g.battle_dice(player).is_some() {
                Intent::Burn { cards: Vec::new() }
            } else {
                Intent::Pass
            });
        }
        if !g.free_to_act(player) {
            return None;
        }
        let waited_on = g.players().any(
            |p| matches!(g.phase(p), Phase::Held { on, .. } if on.is_none_or(|o| o == player)),
        );
        let others_done = g
            .players()
            .filter(|&p| p != player && !matches!(self.seats[p.0 as usize], Seat::Dummy))
            .all(|p| *g.phase(p) == Phase::Done);
        (waited_on || others_done).then_some(Intent::EndTurn)
    }

    /// A scripted move for a seat (a tutorial's rival), as if it played it.
    pub fn act_as(&mut self, player: PlayerId, intent: Intent) -> Result<(), RuleError> {
        self.act(player, intent)
    }

    fn act(&mut self, player: PlayerId, intent: Intent) -> Result<(), RuleError> {
        let events = self.game.apply(player, intent)?;
        self.broadcast(&events);
        self.ask_voices(&events);
        Ok(())
    }

    fn send(&mut self, seat: PlayerId, message: FromTable) {
        if let Some(out) = self.outbox.get_mut(seat.0 as usize) {
            out.push(message);
        }
    }

    fn next_salt(&mut self) -> u64 {
        // SplitMix64 over the table's own salt: fresh for every view.
        self.salt = self.salt.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.salt;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn broadcast(&mut self, events: &[Event]) {
        self.serial += 1;
        self.since_update = 0.0;
        for i in 0..self.seats.len() {
            if !self.seats[i].watched() {
                continue;
            }
            let seat = PlayerId(i as u8);
            let salt = self.next_salt();
            let view = self.game.view_for(Some(seat), salt);
            let seen = events
                .iter()
                .filter_map(|e| Game::event_for(&view, Some(seat), e))
                .collect();
            let serial = self.serial;
            self.send(
                seat,
                FromTable::Update {
                    serial,
                    events: seen,
                    view: Box::new(view),
                },
            );
            self.follow_clock(seat, events);
        }
    }

    /// A person's clocks follow the change, and they hear which one runs.
    fn follow_clock(&mut self, seat: PlayerId, events: &[Event]) {
        let i = seat.0 as usize;
        let Some(timers) = self.timers else {
            return;
        };
        if self.seats[i] != Seat::Human {
            return;
        }
        self.clocks[i].follow(&timers, &self.game, seat, events);
        let shown = self.clocks[i].shown(&self.game, seat);
        self.send(seat, FromTable::Clock(shown));
    }

    /// A clock ran out: decide the plainest way for that seat.
    fn run_clocks(&mut self, dt: f32) {
        if self.timers.is_none() || self.game.winner().is_some() {
            return;
        }
        for i in 0..self.seats.len() {
            if self.seats[i] != Seat::Human {
                continue;
            }
            let seat = PlayerId(i as u8);
            let thinking = self.voice.as_ref().is_some_and(|v| {
                v.pending
                    .iter()
                    .any(|(_, p)| matches!(p, Purpose::Wish { seat: s, .. } if *s == seat))
            });
            let Some(intent) = self.clocks[i].run(dt, &self.game, seat, thinking) else {
                continue;
            };
            let what = self.clocks[i].shown(&self.game, seat).map(|c| c.what);
            let fallback = bot::choose(&self.game, seat);
            if self.act(seat, intent).is_err() {
                // The plain move was not legal after all: let the bot decide
                // rather than hold the table.
                let _ = self.act(seat, fallback);
            }
            if let Some(what) = what {
                self.send(seat, FromTable::TimedOut(what));
            }
        }
    }

    fn everyone_has_shown(&self) -> bool {
        self.since_update >= SHOW_TIMEOUT_SECS
            || self
                .seats
                .iter()
                .zip(&self.shown)
                .all(|(seat, &shown)| !seat.watched() || shown >= self.serial)
    }

    fn run_bots(&mut self) {
        if self.game.winner().is_some()
            || self.since_bot < BOT_STEP_SECS
            || !self.everyone_has_shown()
        {
            return;
        }
        let wish_due = self.game.wish_due();
        // Every bot seat awaited takes one step per tick: they play their
        // turns side by side, as people do (§11.2).
        let bots: Vec<PlayerId> = self
            .game
            .awaiting()
            .into_iter()
            .filter(|&p| match self.seats[p.0 as usize] {
                Seat::Bot => true,
                Seat::Autoplay { wish_by_hand } => !(wish_by_hand && wish_due == Some(p)),
                Seat::Human => false,
                Seat::Dummy => self.dummy_intent(p).is_some(),
            })
            .collect();
        if bots.is_empty() {
            return;
        }
        self.since_bot = 0.0;
        for player in bots {
            // An earlier bot's step may have changed who is awaited.
            if self.game.winner().is_some() || !self.game.awaiting().contains(&player) {
                continue;
            }
            let intent = match self.seats[player.0 as usize] {
                Seat::Dummy => match self.dummy_intent(player) {
                    Some(intent) => intent,
                    None => continue,
                },
                _ => bot::choose(&self.game, player),
            };
            if self.act(player, intent).is_err() {
                // A bot that cannot act must not stall the table.
                let fallback = if self.game.to_answer(player).is_some() {
                    Intent::Pass
                } else {
                    Intent::EndTurn
                };
                let _ = self.act(player, fallback);
            }
        }
    }

    // ---- The gods' voice ----

    fn job(&mut self, purpose: Purpose, job: impl FnOnce(u64) -> Job) {
        let Some(voice) = self.voice.as_mut() else {
            return;
        };
        voice.next_id += 1;
        let id = voice.next_id;
        voice.pending.push((id, purpose));
        voice.oracle.send(job(id));
    }

    fn online(&self) -> bool {
        self.voice
            .as_ref()
            .is_some_and(|v| v.route.voice() != necromy_oracle::Voice::Silent)
    }

    /// The Dominant writes their wish: everyone else sees it as it is typed.
    /// Only the seat whose wish is due may, and only so many letters.
    fn share_draft(&mut self, seat: PlayerId, god: Option<God>, text: String) {
        if self.game.wish_due() != Some(seat) {
            return;
        }
        let text: String = text.chars().take(MAX_DRAFT).collect();
        for i in 0..self.seats.len() {
            let other = PlayerId(i as u8);
            if other != seat && self.seats[i].watched() {
                let text = text.clone();
                self.send(
                    other,
                    FromTable::Drafting {
                        player: seat,
                        god,
                        text,
                    },
                );
            }
        }
    }

    fn ask_wish(&mut self, seat: PlayerId, god: God, text: String) {
        let refuse = if self.game.wish_due() != Some(seat) {
            Some("сейчас не время желаний")
        } else if !self.online() {
            Some("голос богов не отвечает")
        } else if self.voice.as_ref().is_some_and(|v| {
            v.pending
                .iter()
                .any(|(_, p)| matches!(p, Purpose::Wish { .. }))
        }) {
            Some("бог ещё слушает прежнее")
        } else {
            None
        };
        if let Some(why) = refuse {
            self.send(seat, FromTable::Oracle(OracleNews::NotHeard(why.into())));
            return;
        }
        let (messages, schema) = prompt::wish(&self.game, seat, god, &text);
        self.job(Purpose::Wish { seat, god, text }, |id| Job {
            id,
            messages,
            schema: Some(schema),
            max_tokens: 300,
            temperature: 0.6,
        });
        self.send(seat, FromTable::Oracle(OracleNews::Listening(Some(god))));
    }

    /// New wishes without words and new lines for people get a god's voice,
    /// while the model is up and not buried in work.
    fn ask_voices(&mut self, events: &[Event]) {
        if !self.online() {
            return;
        }
        let serial = self.serial;
        for event in events {
            let queued = self.voice.as_ref().map_or(usize::MAX, |v| {
                v.pending
                    .iter()
                    .filter(|(_, p)| !matches!(p, Purpose::Wish { .. }))
                    .count()
            });
            if queued >= MAX_QUEUED_VOICES {
                return;
            }
            match event {
                Event::WishGranted {
                    player,
                    god,
                    kind,
                    grade,
                    said: None,
                    ..
                } => {
                    let messages = prompt::wish_speech(&self.game, *player, *god, *kind, *grade);
                    self.job(Purpose::WishVoice { serial }, |id| voice_job(id, messages));
                }
                Event::LineTold { line } if self.seats[line.owner.0 as usize].watched() => {
                    let messages = prompt::line_voice(&self.game, line);
                    let purpose = Purpose::LineVoice {
                        owner: line.owner,
                        line: line.id,
                    };
                    self.job(purpose, |id| voice_job(id, messages));
                }
                _ => {}
            }
        }
    }

    fn hear(&mut self) {
        let Some(voice) = self.voice.as_mut() else {
            return;
        };
        let now = voice.route.voice();
        let changed = voice.told != Some(now);
        voice.told = Some(now);
        let online = now != necromy_oracle::Voice::Silent;
        let spare = now == necromy_oracle::Voice::Spare;
        let mut answers = Vec::new();
        while let Some(answer) = voice.oracle.poll() {
            if let Some(i) = voice.pending.iter().position(|(id, _)| *id == answer.id) {
                answers.push((voice.pending.remove(i).1, answer.result));
            }
        }
        if changed {
            for i in 0..self.seats.len() {
                self.send(
                    PlayerId(i as u8),
                    FromTable::Oracle(OracleNews::Online(online)),
                );
                self.send(
                    PlayerId(i as u8),
                    FromTable::Oracle(OracleNews::Spare(spare)),
                );
            }
        }
        for (purpose, result) in answers {
            match (purpose, result) {
                (Purpose::Wish { seat, god, text }, result) => {
                    self.send(seat, FromTable::Oracle(OracleNews::Listening(None)));
                    let heard =
                        result
                            .map_err(|_| "бог не ответил".to_string())
                            .and_then(|reply| {
                                prompt::read_wish(&self.game, seat, &text, &reply)
                                    .map_err(|_| "бог ответил невнятно".to_string())
                            });
                    let outcome = heard.and_then(|(kind, target, said)| {
                        let intent = Intent::Wish {
                            god,
                            kind,
                            target,
                            said: Some(said),
                        };
                        self.act(seat, intent)
                            .map_err(|err| format!("бог не смог исполнить: {err}"))
                    });
                    if let Err(why) = outcome {
                        self.send(seat, FromTable::Oracle(OracleNews::NotHeard(why)));
                    }
                }
                (Purpose::WishVoice { serial }, Ok(text)) => {
                    let news = OracleNews::WishVoice {
                        serial,
                        text: text.trim().to_string(),
                    };
                    for i in 0..self.seats.len() {
                        self.send(PlayerId(i as u8), FromTable::Oracle(news.clone()));
                    }
                }
                (Purpose::LineVoice { owner, line }, Ok(text)) => {
                    let news = OracleNews::LineVoice {
                        line,
                        text: text.trim().to_string(),
                    };
                    self.send(owner, FromTable::Oracle(news));
                }
                (_, Err(_)) => {}
            }
        }
    }
}

fn voice_job(id: u64, messages: Vec<necromy_oracle::Message>) -> Job {
    Job {
        id,
        messages,
        schema: None,
        max_tokens: 90,
        temperature: 0.8,
    }
}

#[cfg(test)]
mod tests;
