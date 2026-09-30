//! Runs a local match: one human seat, bots on the rest (docs/design.md §19).
//!
//! The match lives on a table (`necromy-host`) in this same process, as it
//! will on the dedicated server (§17.3): the client sends it intents and
//! redraws from the view it sends back, which holds only what the human may
//! know. Intents are checked against that view first, so a refusal comes at
//! once and in words.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use necromy_host::{Clock, Config, Decision, FromTable, OracleNews, Seat, Table, ToTable};
use necromy_net::{ClientConn, ClientMsg, ServerMsg};
use necromy_rules::{
    CardId, Cure, Event, Fighter, Game, God, Hex, Intent, PlayerId, RuleError, Score, Target,
    Terrain, TimeOfDay, WindowKind,
};

use crate::board::{self, Board};
use crate::dice::DiceShow;
use crate::names;
use crate::token::Token;

/// The god a single player match starts on unless another is chosen:
/// `NECROMY_GOD=<name>` (bhava, trishna, zaga, ahamar, maya) for dev runs,
/// else Trishna, whom the dev seeds were written for.
pub fn default_god() -> God {
    std::env::var("NECROMY_GOD")
        .ok()
        .and_then(|name| {
            God::ALL
                .into_iter()
                .find(|g| g.name().eq_ignore_ascii_case(&name))
        })
        .unwrap_or(God::Trishna)
}
/// The world a match begins with: `NECROMY_MODE=creation|full` for dev runs,
/// which play a full world by default; the menu picks its own.
pub fn default_mode() -> necromy_rules::Mode {
    match std::env::var("NECROMY_MODE").as_deref() {
        Ok("creation") => necromy_rules::Mode::Creation,
        _ => necromy_rules::Mode::Full,
    }
}
const FEED_LINES: usize = 9;

pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        // Dev runs go straight to a single player match; otherwise the menu
        // (`lobby.rs`) inserts the `Match` when one begins.
        if crate::lobby::skip_menu() {
            app.insert_resource(Match::local(default_god(), default_mode()));
        }
        app.init_resource::<Selection>()
            .init_resource::<IncomingCountdown>()
            .add_systems(
                crate::InGame,
                (
                    drive_table,
                    click_board,
                    keys,
                    auto_pass,
                    drop_stale_selection,
                    remote_autoplay,
                ),
            );
    }
}

#[derive(Resource)]
pub struct Match {
    /// The match as the human sees it (`Game::view_for`), after the last update.
    pub game: Game,
    link: Link,
    /// Keeps a single player match on disk as it plays (`saves.rs`).
    saver: Option<necromy_host::save::Saver>,
    /// The table's last update, and the last one the screen has shown.
    serial: u32,
    shown: u32,
    /// Steps of a walk still to send, the next one last.
    walk: Vec<Hex>,
    /// An intent went to the table and its answer has not come yet.
    answer_due: bool,
    /// Intents made meanwhile, sent one by one as answers come.
    queued: VecDeque<Intent>,
    /// The clock the table runs on the human now (§17.1), and seconds since
    /// it said so.
    pub clock: Option<Clock>,
    pub clock_since: f32,
    /// The gods' voice as the table reports it.
    pub oracle: OracleState,
    pub human: PlayerId,
    /// Accepted steps not yet picked up by the token animation.
    pub steps: Vec<(PlayerId, necromy_rules::Hex)>,
    pub feed: Vec<String>,
    /// Dev aid (`NECROMY_AUTOPLAY`): a bot plays the human seat too.
    pub autoplay: bool,
    /// Throws not yet picked up by the dice show.
    pub throws: Vec<ThrowView>,
    /// The battle on screen, from its start until the dice show ends.
    pub battle: Option<BattleInfo>,
    /// The trial on screen (§20.2), from its first step to the end of its dice.
    pub trial: Option<TrialInfo>,
    /// Battles and trials on the table, each with the events that tell it.
    /// The human's own go on screen at once; the others wait behind an icon
    /// over their hex (`watch_ui.rs`) until the human looks.
    pub shows: Vec<Show>,
    next_show: u32,
    /// The show on the panels now, if any.
    pub on_screen: Option<u32>,
    /// The human chose to look at it: it is not theirs, it may be closed.
    pub watching: bool,
    /// Bumped whenever the show on screen changes: the dice trays start over.
    pub show_serial: u32,
    /// Falls in the fight on screen, told only when its panel closes: who,
    /// where the body lies, where they wake. Until then the token stands and
    /// the board shows no body there.
    pub held_falls: Vec<(PlayerId, Hex, Hex)>,
    /// A guard felled in the fight on screen stands until its panel closes.
    pub held_guard: Option<necromy_rules::Guard>,
    /// Mob laid to rest in the fight on screen, standing until it closes.
    pub held_mobs: Vec<necromy_rules::Mob>,
    /// Every mob seen so far, as last seen: for panels, lines and sounds
    /// about one that has already left the board.
    seen_mobs: std::collections::BTreeMap<u32, necromy_rules::Mob>,
    /// A settlement's militia as they stood when the fight on screen began:
    /// their figures keep to it until its dice are down.
    pub held_militia: Option<(Hex, necromy_rules::Militia)>,
    /// The last card that hit the human and what it did, shown for a moment
    /// after its Target window closed.
    pub incoming_result: Option<IncomingResult>,
    /// Bumped for every new `incoming_result`.
    pub incoming_serial: u32,
    /// Offerings since the UI last looked: who gave, to whom, how much
    /// (§5.1). `gods_ui` drains it into little flights.
    pub offerings: Vec<(Option<PlayerId>, God, u8)>,
    /// Stages that shifted at the last dusk, for its scene: god, from, to.
    pub dusk_news: Vec<(God, u8, u8)>,
    /// Mechanics that came into the world, for the same scene (§21.4): the
    /// mechanic, the god, and who asked, named.
    pub world_news: Vec<(necromy_rules::Feature, God, Option<String>)>,
    /// Each god's stage as last told, to say where a shift came from.
    stages_seen: [u8; 5],
    /// Stage shifts told so far (the `=dusk` screenshot waits for one).
    pub stage_shifts: u32,
    /// Events not yet sounded; `audio::hear_events` takes them.
    pub heard: Vec<Event>,
    /// Events for `effects.rs` to show on the board, drained there.
    pub effects: Vec<Event>,
    /// Fights between mobs, the militia and the guard, for `clash.rs`.
    pub clashes: Vec<Event>,
    /// Where the first clash took place (`NECROMY_SCREENSHOT_WHEN=clash`).
    pub last_clash: Option<Hex>,
    /// Rivals' wishes as they write them: the god and the words (§21.4).
    pub drafting: std::collections::BTreeMap<PlayerId, (Option<God>, String)>,
    /// The wish and the god's answer on screen now, shown for a moment (§7).
    pub wish_reply: Option<WishReply>,
    /// Answers still to show: at dusk the gods answer everyone at once.
    pub wish_queue: VecDeque<WishReply>,
    /// Bumped for every new `wish_reply`.
    pub wish_serial: u32,
    /// The last story line told to the human, for its voice popup (§8).
    pub told: Option<necromy_rules::Line>,
    pub told_serial: u32,
    /// Feed lines that would spoil dice still rolling on screen.
    held: Vec<String>,
    holding: bool,
    /// A tutorial chapter runs (`tutorial.rs`): what the table told, for its
    /// steps to watch. `None` outside the tutorial.
    pub lesson_events: Option<Vec<Event>>,
    /// What the human may do now; anything else is turned away (a tutorial
    /// step leads by the hand). `refused` counts the turned-away intents.
    pub gate: Option<Gate>,
    pub refused: u32,
}

/// Whether the human may make this intent now.
pub type Gate = std::sync::Arc<dyn Fn(&Game, PlayerId, &Intent) -> bool + Send + Sync>;

/// What the table told about the gods' voice.
#[derive(Default)]
pub struct OracleState {
    /// The model answers: wishes can be written in free words.
    pub online: bool,
    /// The main voice is down and a spare answers (the server's
    /// `NECROMY_ORACLE` lists it).
    pub spare: bool,
    /// The god thinking about the human's wish.
    pub listening: Option<God>,
    /// Why the last free-words wish was not heard.
    pub failed: Option<String>,
    /// The model's words for wishes, by the table's update serial and who
    /// asked.
    pub wish_voices: HashMap<(u32, PlayerId), String>,
    /// The model's words for story lines, by line id.
    pub line_voices: HashMap<u32, String>,
}

/// A wish and what came of it.
pub struct WishReply {
    pub player: PlayerId,
    /// The table's update it came in; its god's words are keyed by it.
    pub serial: u32,
    /// `None` when the Dominant refused to wish.
    /// God, what it granted, the grade and how many acts it left out.
    pub wish: Option<(God, necromy_rules::Wish, u8, u8)>,
    /// The model's reading, for a wish written in free words.
    pub said: Option<necromy_rules::Said>,
    pub lines: Vec<String>,
}

/// A card that was aimed at the human, and its outcome in words.
pub struct IncomingResult {
    pub caster: PlayerId,
    pub card: CardId,
    pub lines: Vec<String>,
}

/// A trial being thrown for, as the panel shows it.
#[derive(Clone, Debug)]
pub struct TrialInfo {
    pub player: PlayerId,
    pub hex: Hex,
    /// As it stood when stepped onto; later from its outcome.
    pub trial: Option<necromy_rules::Trial>,
    /// Faces from burned cards.
    pub burned: Vec<necromy_rules::Face>,
    /// Counting faces, how many it asked, and whether that was enough.
    pub result: Option<(u8, u8, bool)>,
}

/// What the battle panel shows about the current fight.
pub struct BattleInfo {
    /// Attacker, then defender: champions, the royal guard or an undead,
    /// who may strike or be struck (§20.4).
    pub sides: [Fighter; 2],
    /// Faces from burned cards: attacker, defender.
    pub burned: [Vec<necromy_rules::Face>; 2],
    /// Attacker and defender scores once the battle resolved.
    pub scores: Option<(Score, Score)>,
    /// Health before and after the blows, per side, for sides that were hit.
    pub hp: [Option<(u8, u8)>; 2],
    /// The side fell and woke at home.
    pub fell: [bool; 2],
}

impl BattleInfo {
    fn new(sides: [Fighter; 2]) -> Self {
        BattleInfo {
            sides,
            burned: [Vec::new(), Vec::new()],
            scores: None,
            hp: [None, None],
            fell: [false, false],
        }
    }

    /// 0 for the attacker, 1 for the defender, `None` for bystanders.
    fn side_of(&self, player: PlayerId) -> Option<usize> {
        self.sides
            .iter()
            .position(|s| *s == Fighter::Champion(player))
    }

    /// The side that is no champion: the guard or an undead.
    pub fn foe_side(&self) -> Option<usize> {
        self.sides
            .iter()
            .position(|s| !matches!(s, Fighter::Champion(_)))
    }

    /// The champion on `side`, if a champion stands there.
    pub fn champion(&self, side: usize) -> Option<PlayerId> {
        match self.sides[side] {
            Fighter::Champion(p) => Some(p),
            _ => None,
        }
    }

    fn side(&self, player: PlayerId) -> usize {
        self.side_of(player).unwrap_or(0)
    }
}

/// A battle or a trial on the table, as the events told it (§12, §20.2).
pub struct Show {
    pub id: u32,
    pub hex: Hex,
    pub kind: ShowKind,
    /// Champions in it: attacker and defender, the guard's target, the one
    /// on trial.
    pub who: Vec<PlayerId>,
    /// Its events so far, from the one that began it.
    pub events: Vec<Event>,
    /// Its outcome has come; the batch that brought it still adds blows.
    pub done: bool,
    /// Takes no more events: its outcome's batch is over.
    pub closed: bool,
    /// Seconds since it ended, counted by `watch_ui.rs` to let it go.
    pub since_done: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowKind {
    Battle,
    Guard,
    /// A champion and an undead (§20.4).
    Mob,
    Trial,
}

pub struct ThrowView {
    /// 0 for the attacker's tray, 1 for the defender's.
    pub side: usize,
    pub seed: u64,
    pub count: u8,
    pub faces: Vec<necromy_rules::Face>,
}

/// Where the table is: in this process, or on a server.
enum Link {
    Local(Box<Table>),
    Remote(Box<Remote>),
}

/// A server connection that finds its way back after a break (§17.1).
struct Remote {
    conn: ClientConn,
    ticket: crate::lobby::Ticket,
    /// The connection broke; `retry` counts down to the next attempt.
    lost: bool,
    retry: f32,
    /// The server refused the ticket: the seat stays with a bot.
    gone: bool,
    notices: Vec<Incoming>,
}

/// Seconds between attempts to sit back down.
const RETRY_SECS: f32 = 3.0;

/// What reaches the client: the table's messages, or a word about the link.
enum Incoming {
    Table(FromTable),
    Notice(String),
    /// The connection broke: whatever was sent and not answered is lost.
    Broke,
}

impl Link {
    fn submit(&mut self, seat: PlayerId, message: ToTable) {
        match self {
            Link::Local(table) => table.submit(seat, message),
            Link::Remote(r) => r.conn.send(ClientMsg::Table(message)),
        }
    }

    fn tick(&mut self, dt: f32) {
        match self {
            Link::Local(table) => table.tick(dt),
            Link::Remote(r) if r.lost && !r.gone => {
                r.retry -= dt;
                if r.retry > 0.0 {
                    return;
                }
                r.retry = RETRY_SECS;
                if let Ok(conn) = necromy_net::connect(&r.ticket.server, &r.ticket.name) {
                    conn.send(ClientMsg::Rejoin {
                        code: r.ticket.code.clone(),
                        ticket: r.ticket.ticket,
                    });
                    r.conn = conn;
                    r.lost = false;
                }
            }
            Link::Remote(_) => {}
        }
    }

    fn drain(&mut self, seat: PlayerId) -> Vec<Incoming> {
        match self {
            Link::Local(table) => table.drain(seat).into_iter().map(Incoming::Table).collect(),
            Link::Remote(r) => {
                let mut out = std::mem::take(&mut r.notices);
                while let Some(message) = r.conn.poll() {
                    match message {
                        ServerMsg::Table(m) => out.push(Incoming::Table(m)),
                        ServerMsg::Started { .. } => {
                            out.push(Incoming::Notice("Ты снова за столом.".into()));
                        }
                        ServerMsg::Error(e) => {
                            // Only a refused ticket comes here mid-match.
                            r.gone = true;
                            crate::lobby::Ticket::forget();
                            out.push(Incoming::Notice(format!(
                                "Сервер: {e}. Твоё место остаётся за ботом."
                            )));
                        }
                        ServerMsg::Lobby(_) => {}
                    }
                }
                if !r.conn.is_open() && !r.lost && !r.gone {
                    r.lost = true;
                    r.retry = RETRY_SECS;
                    out.push(Incoming::Broke);
                    out.push(Incoming::Notice(
                        "Связь с сервером потеряна: пока за тебя играет бот. Пробую вернуться…"
                            .into(),
                    ));
                }
                out
            }
        }
    }
}
/// The card the human picked and is now aiming.
#[derive(Resource, Default)]
pub struct Selection {
    pub card: Option<CardId>,
    /// Cards marked to burn in the open Battle window.
    pub burn: Vec<CardId>,
    /// Going through the hand (a click on the deck): the cards marked to
    /// let go (§21.2).
    pub sift: Option<Vec<CardId>>,
}

impl Match {
    /// A single player match on a table in this process (§17.3), seeded by
    /// `NECROMY_SEED` or the clock.
    pub fn local(god: God, mode: necromy_rules::Mode) -> Self {
        let seed = std::env::var("NECROMY_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64)
            });
        info!("match seed {seed} (set NECROMY_SEED to replay it)");
        let champions = God::ALL.to_vec();
        let human = PlayerId(
            champions
                .iter()
                .position(|&g| g == god)
                .expect("human god is seated") as u8,
        );
        let autoplay = std::env::var_os("NECROMY_AUTOPLAY").is_some();
        let seats = (0..champions.len() as u8)
            .map(|i| match PlayerId(i) {
                p if p != human => Seat::Bot,
                _ if autoplay => Seat::Autoplay {
                    wish_by_hand: wish_by_hand(),
                },
                _ => Seat::Human,
            })
            .collect();
        // Clients never learn it; here the client is its own server.
        let salt = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
            ^ seed.rotate_left(17);
        let oracle = necromy_oracle::addr_from_env();
        info!("gods' voice: llama-server at {oracle} (NECROMY_ORACLE to change)");
        let mut table = Table::new(Config {
            seed,
            champions,
            seats,
            salt,
            oracle: Some(oracle),
            // Alone, nobody waits on the human.
            timers: None,
            mode,
        });
        let saver = crate::saves::start(&mut table);
        let first = table.drain(human);
        let mut m = Match::begin(Link::Local(Box::new(table)), human, autoplay, first);
        m.saver = saver;
        m
    }

    /// A single player match saved in `dir`, taken up where it stopped.
    pub fn resume(dir: std::path::PathBuf) -> Result<Self, String> {
        let saved = necromy_host::save::load(&dir).map_err(|err| err.to_string())?;
        let human = crate::saves::human_of(&saved.snapshot.seats)
            .ok_or_else(|| "в сохранении нет места игрока".to_string())?;
        let mut table = Table::restore(saved, Some(necromy_oracle::addr_from_env()));
        if let Some(n) = table.replay_stopped() {
            warn!("{n} journal entries did not replay: the match goes on from before them");
        }
        if table.game().winner().is_some() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err("эта партия уже окончена".into());
        }
        let saver = crate::saves::keep(dir, &mut table);
        let first = table.drain(human);
        let mut m = Match::begin(Link::Local(Box::new(table)), human, false, first);
        m.saver = saver;
        Ok(m)
    }

    /// A tutorial chapter: `scene` on a table in this process, the human in
    /// the first seat, dummies in the rest (`Seat::Dummy`).
    pub fn tutorial(scene: &necromy_rules::Scenario) -> Self {
        let (game, events) = Game::scenario(scene);
        let seats = (0..scene.seats.len())
            .map(|i| if i == 0 { Seat::Human } else { Seat::Dummy })
            .collect();
        let mut table = Table::from_game(game, events, seats, 0, None, None);
        let human = PlayerId(0);
        let first = table.drain(human);
        let mut m = Match::begin(Link::Local(Box::new(table)), human, false, first);
        m.lesson_events = Some(Vec::new());
        m
    }

    /// A tutorial's rival plays `card` (by name) at `target`, as scripted.
    /// Only on a table in this process; false if it could not.
    pub fn script_play(&mut self, player: PlayerId, card: &str, target: Target) -> bool {
        let Link::Local(table) = &mut self.link else {
            return false;
        };
        let game = table.game();
        let Some(&id) = game
            .hand(player)
            .iter()
            .find(|&&c| game.def(c).name == card)
        else {
            warn!("script: {card} is not in the hand of seat {}", player.0);
            return false;
        };
        let done = table.act_as(player, Intent::Play { card: id, target });
        if let Err(err) = &done {
            warn!("script: {card} refused: {err}");
        }
        self.pump();
        done.is_ok()
    }

    /// Dev aid (`NECROMY_AUTOPLAY` at a server): the bot plays the human's
    /// seat from its own view, one intent at a time. A local table seats
    /// the bot itself (`Seat::Autoplay`), so this is for servers only.
    pub fn play_for_human(&mut self) {
        let human = self.human;
        if !self.autoplay
            || !matches!(self.link, Link::Remote(_))
            || self.paused_for_wish_panel()
            || self.answer_due
            || !self.walk.is_empty()
            || self.game.winner().is_some()
            || !self.game.awaiting().contains(&human)
        {
            return;
        }
        // With `NECROMY_WISH` the wish is written by hand (`wish_ui::dev_wish`).
        let intent = if wish_by_hand() {
            necromy_rules::bot::choose_turn(&self.game, human)
        } else {
            necromy_rules::bot::choose(&self.game, human)
        };
        if self.act(human, intent).is_err() {
            let fallback = if self.game.wishing().contains(&human) {
                Intent::RefuseWish
            } else if self.game.battle_dice(human).is_some() {
                Intent::Burn { cards: Vec::new() }
            } else if self.game.to_answer(human).is_some() {
                Intent::Pass
            } else {
                Intent::EndTurn
            };
            let _ = self.act(human, fallback);
        }
    }

    /// A match on a server: `first` holds the table's first update for `seat`.
    pub fn remote(
        conn: ClientConn,
        seat: PlayerId,
        first: Vec<FromTable>,
        ticket: crate::lobby::Ticket,
    ) -> Self {
        let remote = Remote {
            conn,
            ticket,
            lost: false,
            retry: 0.0,
            gone: false,
            notices: Vec::new(),
        };
        // Dev aid: at a server the seat stays a person's; `NECROMY_AUTOPLAY`
        // plays it from here instead (`remote_autoplay`).
        let autoplay = std::env::var_os("NECROMY_AUTOPLAY").is_some();
        Match::begin(Link::Remote(Box::new(remote)), seat, autoplay, first)
    }

    fn begin(link: Link, human: PlayerId, autoplay: bool, first: Vec<FromTable>) -> Self {
        let Some(FromTable::Update { view, .. }) =
            first.iter().find(|m| matches!(m, FromTable::Update { .. }))
        else {
            unreachable!("a table greets every watched seat with its view");
        };
        let mut m = Match {
            game: (**view).clone(),
            link,
            saver: None,
            serial: 0,
            shown: 0,
            walk: Vec::new(),
            answer_due: false,
            queued: VecDeque::new(),
            oracle: OracleState::default(),
            clock: None,
            clock_since: 0.0,
            human,
            steps: Vec::new(),
            feed: Vec::new(),
            autoplay,
            throws: Vec::new(),
            battle: None,
            trial: None,
            shows: Vec::new(),
            next_show: 0,
            on_screen: None,
            watching: false,
            show_serial: 0,
            held_falls: Vec::new(),
            held_guard: None,
            held_mobs: Vec::new(),
            seen_mobs: Default::default(),
            held_militia: None,
            incoming_result: None,
            incoming_serial: 0,
            offerings: Vec::new(),
            dusk_news: Vec::new(),
            world_news: Vec::new(),
            heard: Vec::new(),
            effects: Vec::new(),
            clashes: Vec::new(),
            last_clash: None,
            drafting: Default::default(),
            stages_seen: God::ALL.map(|g| view.stage(g)),
            stage_shifts: 0,
            wish_reply: None,
            wish_queue: VecDeque::new(),
            wish_serial: 0,
            told: None,
            told_serial: 0,
            held: Vec::new(),
            holding: false,
            lesson_events: None,
            gate: None,
            refused: 0,
        };
        m.receive(first.into_iter().map(Incoming::Table).collect());
        m
    }

    /// The human may walk or end the turn: acting, not held, no window of
    /// theirs open (§11.2).
    pub fn is_human_turn(&self) -> bool {
        self.game.free_to_act(self.human)
    }

    /// The window the human still has to answer, if any.
    pub fn human_window(&self) -> Option<necromy_rules::WindowKind> {
        self.game.to_answer(self.human).map(|w| w.kind)
    }

    /// Dev aid: with autoplay, `NECROMY_SCREENSHOT_WHEN=wishpanel` stops at the
    /// human's wish so its panel can be captured, and `NECROMY_WISH` stops
    /// there so the wish can be written (`wish_ui::dev_wish`).
    pub fn paused_for_wish_panel(&self) -> bool {
        self.game.wishing().contains(&self.human) && wish_by_hand()
    }

    /// The next god's answer from the queue goes on screen, if any.
    pub fn next_wish_reply(&mut self) {
        self.wish_reply = self.wish_queue.pop_front();
        if self.wish_reply.is_some() {
            self.wish_serial += 1;
        }
    }

    /// The game is waiting on the human, on their turn or in a window.
    pub fn human_awaited(&self) -> bool {
        self.game.awaiting().contains(&self.human)
    }

    /// Send the human's intent to the table. It is checked against the view
    /// first, so a refusal comes at once and in words; the table still
    /// decides. While the last intent waits for its answer, the next one
    /// waits too and is checked again when its turn comes: a server answers
    /// a moment later, and the view is stale until then.
    pub fn act(&mut self, player: PlayerId, intent: Intent) -> Result<(), RuleError> {
        debug_assert_eq!(player, self.human, "the client acts only for its seat");
        if let Some(gate) = self.gate.clone()
            && !gate(&self.game, player, &intent)
        {
            self.refused += 1;
            self.walk.clear();
            return Ok(());
        }
        self.game.clone().apply(player, intent.clone())?;
        if self.answer_due {
            self.queued.push_back(intent);
            return Ok(());
        }
        self.answer_due = true;
        self.link.submit(player, ToTable::Act(intent));
        self.pump();
        Ok(())
    }

    /// The last intent was answered: send the next one still valid.
    fn answered(&mut self) {
        self.answer_due = false;
        let human = self.human;
        while let Some(intent) = self.queued.pop_front() {
            if self.game.clone().apply(human, intent.clone()).is_ok() {
                self.answer_due = true;
                self.link.submit(human, ToTable::Act(intent));
                return;
            }
        }
        if !self.walk.is_empty() {
            self.next_step();
        }
    }

    /// Walk a path one step per intent, as the table wants them: each step
    /// goes once the last one came back, and a window stops the walk.
    pub fn walk(&mut self, path: Vec<Hex>) {
        self.walk = path;
        self.walk.reverse();
        self.next_step();
    }

    fn next_step(&mut self) {
        if !self.is_human_turn() {
            self.walk.clear();
            return;
        }
        let Some(to) = self.walk.pop() else {
            return;
        };
        let human = self.human;
        if let Err(err) = self.act(human, Intent::Move { to }) {
            warn!("move rejected: {err}");
            self.walk.clear();
        }
    }

    /// Hand the human's wish in free words to `god`; the table asks the model.
    pub fn wish_in_words(&mut self, god: God, text: &str) {
        self.oracle.failed = None;
        let human = self.human;
        self.link.submit(
            human,
            ToTable::Wish {
                god,
                text: text.to_string(),
            },
        );
        self.pump();
    }

    /// The human's wish as they write it, for the others to watch.
    pub fn draft_wish(&mut self, god: Option<God>, text: &str) {
        let human = self.human;
        let text = text.to_string();
        self.link.submit(human, ToTable::Draft { god, text });
    }

    fn pump(&mut self) {
        let messages = self.link.drain(self.human);
        self.receive(messages);
    }

    fn receive(&mut self, messages: Vec<Incoming>) {
        for message in messages {
            let message = match message {
                Incoming::Table(m) => m,
                Incoming::Broke => {
                    self.answer_due = false;
                    self.queued.clear();
                    self.walk.clear();
                    self.clock = None;
                    continue;
                }
                Incoming::Notice(text) => {
                    self.feed.push(text);
                    continue;
                }
            };
            match message {
                FromTable::Update {
                    serial,
                    events,
                    view,
                } => {
                    self.game = *view;
                    self.serial = serial;
                    for m in self.game.mobs() {
                        self.seen_mobs.insert(m.id, *m);
                    }
                    self.record(&events);
                    // A wish sealed is no longer being written.
                    let game = &self.game;
                    self.drafting.retain(|&p, _| game.may_wish(p));
                    // The match is over: there is no seat to come back to.
                    if self.game.winner().is_some() && matches!(self.link, Link::Remote(_)) {
                        crate::lobby::Ticket::forget();
                    }
                }
                FromTable::Accepted => self.answered(),
                FromTable::Rejected(err) => {
                    self.walk.clear();
                    self.queued.clear();
                    self.answer_due = false;
                    self.feed.push(format!("Нельзя: {}.", reason(err)));
                }
                FromTable::Clock(clock) => {
                    self.clock = clock;
                    self.clock_since = 0.0;
                }
                FromTable::Drafting { player, god, text } => {
                    if self.game.may_wish(player) {
                        self.drafting.insert(player, (god, text));
                    }
                }
                FromTable::TimedOut(what) => self.feed.push(
                    match what {
                        Decision::Turn => "Время вышло: ход закончен.",
                        Decision::Window => "Время вышло: пас.",
                        Decision::Wish => "Время вышло: желание не загадано.",
                    }
                    .into(),
                ),
                FromTable::Oracle(news) => match news {
                    OracleNews::Online(online) => self.oracle.online = online,
                    // Told in the feed, so whoever runs the server notices.
                    OracleNews::Spare(spare) => {
                        if spare != self.oracle.spare {
                            self.feed.push(
                                if spare {
                                    "Голос богов слабеет: говорит запасная модель."
                                } else {
                                    "Голос богов вернулся в полную силу."
                                }
                                .into(),
                            );
                        }
                        self.oracle.spare = spare;
                    }
                    OracleNews::Listening(god) => self.oracle.listening = god,
                    OracleNews::NotHeard(why) => self.oracle.failed = Some(why),
                    OracleNews::WishVoice {
                        serial,
                        player,
                        text,
                    } => {
                        self.oracle.wish_voices.insert((serial, player), text);
                    }
                    OracleNews::LineVoice { line, text } => {
                        self.oracle.line_voices.insert(line, text);
                    }
                },
            }
        }
    }

    fn record(&mut self, events: &[Event]) {
        self.heard.extend_from_slice(events);
        self.effects.extend_from_slice(events);
        for event in events {
            match *event {
                // Where a mob that leaves the board in this batch last stood.
                Event::MobMoved { id, to, .. } => {
                    if let Some(m) = self.seen_mobs.get_mut(&id) {
                        m.hex = to;
                    }
                }
                Event::MilitiaStruck { .. }
                | Event::MilitiaHit { .. }
                | Event::UndeadHitMilitia { .. }
                | Event::BeastMauled { .. }
                | Event::GuardHewed { .. } => {
                    self.clashes.push(event.clone());
                    // The first one: it plays at once. `NECROMY_CLASH=kill` waits for
                    // one that fells a mob.
                    let fatal = matches!(
                        event,
                        Event::MilitiaStruck { fell: true, .. }
                            | Event::BeastMauled { .. }
                            | Event::GuardHewed { .. }
                    );
                    let kill = std::env::var("NECROMY_CLASH").as_deref() == Ok("kill");
                    if kill && !fatal {
                        continue;
                    }
                    self.last_clash = self.last_clash.or(match *event {
                        Event::MilitiaStruck { hex, .. } | Event::GuardHewed { hex, .. } => {
                            Some(hex)
                        }
                        Event::MilitiaHit { home, .. } | Event::UndeadHitMilitia { home, .. } => {
                            Some(home)
                        }
                        Event::BeastMauled { beast, .. } => self.seen_mob(beast).map(|m| m.hex),
                        _ => None,
                    });
                }
                _ => {}
            }
        }
        if let Some(seen) = self.lesson_events.as_mut() {
            seen.extend_from_slice(events);
        }
        // Wishes in this batch (dusk answers them all at once, §21.4), and
        // the lines of what each did.
        let mut wished: Vec<WishReply> = Vec::new();
        // Outcome of a card aimed at the human, gathered from this batch.
        let mut hit: Option<IncomingResult> = None;
        // What a wish did follows it, up to the first event of something else
        // (the next wish, the gods shifting, the night).
        let mut collecting = false;
        for event in events {
            if matches!(
                event,
                Event::WishGranted { .. }
                    | Event::WishRefused { .. }
                    | Event::WishLost { .. }
                    | Event::StageChanged { .. }
                    | Event::RoundStarted { .. }
                    | Event::LineTold { .. }
                    | Event::LineDone { .. }
                    | Event::LineFailed { .. }
                    | Event::WorldStirred { .. }
                    | Event::TrialSet { .. }
                    | Event::TrialFaded { .. }
            ) {
                collecting = false;
            }
            if collecting
                && let Some(w) = wished.last_mut()
                && let Some(line) = self.describe(event)
            {
                w.lines.push(line);
            }
            if matches!(event, Event::WishGranted { .. }) {
                collecting = true;
            }
            match event {
                Event::WishGranted {
                    player,
                    god,
                    wish,
                    dropped,
                    grade,
                    said,
                } => {
                    wished.push(WishReply {
                        player: *player,
                        serial: self.serial,
                        wish: Some((*god, wish.clone(), *grade, *dropped)),
                        said: said.clone(),
                        lines: Vec::new(),
                    });
                }
                Event::WishRefused { player } => {
                    wished.push(WishReply {
                        player: *player,
                        serial: self.serial,
                        wish: None,
                        said: None,
                        lines: Vec::new(),
                    });
                }
                Event::LineTold { line } if line.owner == self.human => {
                    self.told = Some(*line);
                    self.told_serial += 1;
                }
                _ => {}
            }
            if let Some(line) = self.outcome_line(event, &hit)
                && let Some(h) = hit.as_mut()
            {
                h.lines.push(line);
            }
            if let Event::WindowClosed {
                kind:
                    WindowKind::Target {
                        caster,
                        target,
                        card,
                    },
                ..
            } = event
                && *target == self.human
            {
                hit = Some(IncomingResult {
                    caster: *caster,
                    card: *card,
                    lines: Vec::new(),
                });
            }
            self.route_show(event);
            match event {
                Event::Moved { player, to, .. } => self.steps.push((*player, *to)),
                Event::Blinked { player, to, .. } => self.steps.push((*player, *to)),
                Event::Swapped {
                    player,
                    to,
                    other,
                    other_to,
                } => {
                    self.steps.push((*player, *to));
                    self.steps.push((*other, *other_to));
                }
                // Seen again: the token goes where they really are.
                Event::Revealed { player, hex, .. } => self.steps.push((*player, *hex)),
                // The militia in a fight the human is in keep their men on
                // the board until the dice are down.
                Event::MilitiaAttacked { attacker, home } if *attacker == self.human => {
                    // The attack comes before its dice: the view has them whole.
                    self.held_militia = self.game.militia_unit(*home).map(|m| (*home, m));
                }
                // So does an undead laid to rest on screen.
                Event::MobFell { id, hex, by } => {
                    if self.in_show_on_screen(*by) {
                        self.held_mobs.push(necromy_rules::Mob {
                            id: *id,
                            kind: self.mob_kind(*id),
                            hex: *hex,
                            hp: 0,
                        });
                    }
                }
                // A guard felled on screen stands until its dice are down.
                Event::GuardFell { hex, by } => {
                    if self.in_show_on_screen(*by) {
                        self.held_guard = Some(necromy_rules::Guard {
                            hex: *hex,
                            target: *by,
                            hp: 0,
                        });
                    }
                }
                // A fall in the fight on screen waits for its dice.
                Event::ChampionFell {
                    player,
                    at,
                    respawn,
                } => {
                    if self.in_show_on_screen(*player) {
                        self.held_falls.push((*player, *at, *respawn));
                    } else {
                        self.steps.push((*player, *respawn));
                    }
                }
                Event::Offered {
                    player,
                    god,
                    amount,
                } => self.offerings.push((*player, *god, *amount)),
                Event::StageChanged { god, stage } => {
                    let seen = &mut self.stages_seen[god.index()];
                    let from = std::mem::replace(seen, *stage);
                    self.dusk_news.push((*god, from, *stage));
                    self.stage_shifts += 1;
                }
                Event::WorldGrew {
                    feature,
                    god,
                    player,
                } => {
                    let who = player.map(|p| self.name_genitive(p));
                    self.world_news.push((*feature, *god, who));
                }
                _ => {}
            }
            if let Some(line) = self.describe(event) {
                if self.holding {
                    self.held.push(line);
                } else {
                    self.feed.push(line);
                }
            }
        }
        // A show whose outcome came in this batch has had its blows too.
        for show in self.shows.iter_mut().filter(|s| s.done) {
            show.closed = true;
        }
        self.wish_queue.extend(wished);
        if self.wish_reply.is_none() {
            self.next_wish_reply();
        }
        if let Some(mut h) = hit {
            if h.lines.is_empty() {
                h.lines.push("без последствий".into());
            }
            self.incoming_result = Some(h);
            self.incoming_serial += 1;
        }
        let excess = self.feed.len().saturating_sub(FEED_LINES);
        self.feed.drain(..excess);
    }

    /// Sends a battle's or a trial's event to its show, and on to the
    /// panels when that show is on screen. The human's own shows go on
    /// screen as they begin (all of them with `NECROMY_WATCH=all`).
    fn route_show(&mut self, event: &Event) {
        let begins = match event {
            Event::BattleStarted { attacker, defender } => {
                Some((ShowKind::Battle, vec![*attacker, *defender]))
            }
            Event::GuardStruck { target } => Some((ShowKind::Guard, vec![*target])),
            Event::GuardAttacked { attacker } => Some((ShowKind::Guard, vec![*attacker])),
            Event::MobStruck { target, .. } => Some((ShowKind::Mob, vec![*target])),
            Event::MobAttacked { attacker, .. } => Some((ShowKind::Mob, vec![*attacker])),
            Event::MilitiaAttacked { attacker, .. } => Some((ShowKind::Mob, vec![*attacker])),
            Event::TrialBegun { player, .. } => Some((ShowKind::Trial, vec![*player])),
            _ => None,
        };
        if let Some((kind, who)) = begins {
            let hex = match event {
                Event::TrialBegun { hex, .. } => *hex,
                // Where the one attacked stands.
                _ => {
                    let at = *who.last().expect("a show has someone in it");
                    self.game.champion(at).map_or(Hex::ZERO, |c| c.hex)
                }
            };
            self.next_show += 1;
            let id = self.next_show;
            let mine = who.contains(&self.human) || watch_all();
            self.shows.push(Show {
                id,
                hex,
                kind,
                who,
                events: vec![event.clone()],
                done: false,
                closed: false,
                since_done: 0.0,
            });
            if mine {
                self.put_on_screen(id, false);
            }
            return;
        }
        // Blows come after the outcome, in its batch; the rest before it.
        let (who, foe, ends, after): (Option<PlayerId>, Option<ShowKind>, bool, bool) = match event
        {
            Event::Burned { player, .. } => (Some(*player), None, false, false),
            Event::DiceThrown {
                fighter: Fighter::Champion(p),
                ..
            } => (Some(*p), None, false, false),
            Event::DiceThrown {
                fighter: Fighter::Guard,
                ..
            } => (None, Some(ShowKind::Guard), false, false),
            Event::DiceThrown {
                fighter: Fighter::Mob(_) | Fighter::Militia(_),
                ..
            } => (None, Some(ShowKind::Mob), false, false),
            Event::BattleResolved { defender, .. } => (Some(*defender), None, true, false),
            Event::GuardResolved { target, .. } => {
                (Some(*target), Some(ShowKind::Guard), true, false)
            }
            Event::MobResolved { champion, .. } | Event::MilitiaResolved { champion, .. } => {
                (Some(*champion), Some(ShowKind::Mob), true, false)
            }
            Event::TrialPassed { player, .. } | Event::TrialFailed { player, .. } => {
                (Some(*player), None, true, false)
            }
            Event::Damaged { player, .. } | Event::ChampionFell { player, .. } => {
                (Some(*player), None, false, true)
            }
            Event::GuardHurt { .. } | Event::GuardFell { .. } => {
                (None, Some(ShowKind::Guard), false, true)
            }
            Event::MobHurt { .. }
            | Event::MobFell { .. }
            | Event::MilitiaHurt { .. }
            | Event::MilitiaFell { .. } => (None, Some(ShowKind::Mob), false, true),
            _ => return,
        };
        let Some(show) = self.shows.iter_mut().rev().find(|s| {
            !s.closed
                && s.done == after
                && foe.is_none_or(|k| s.kind == k)
                && who.is_none_or(|p| s.who.contains(&p))
        }) else {
            return;
        };
        show.events.push(event.clone());
        if ends {
            show.done = true;
        }
        if self.on_screen == Some(show.id) {
            self.screen_event(event);
        }
    }

    /// Puts show `id` on the panels, from its first event: the human's own
    /// (`watching` false), or one they chose to look at.
    pub fn put_on_screen(&mut self, id: u32, watching: bool) {
        self.release_feed();
        self.battle = None;
        self.trial = None;
        self.throws.clear();
        self.on_screen = Some(id);
        self.watching = watching;
        self.show_serial += 1;
        let events = self
            .shows
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.events.clone())
            .unwrap_or_default();
        for event in &events {
            self.screen_event(event);
        }
    }

    /// The human gives the item in `slot` to the god of the temple they
    /// stand on (§20.3).
    pub fn sacrifice(&mut self, slot: necromy_rules::Slot) {
        let human = self.human;
        if let Err(err) = self.act(human, Intent::Sacrifice { slot }) {
            self.feed.push(format!("Нельзя: {}.", reason(err)));
        }
    }

    /// The guard as the board shows it: a felled one stands until the
    /// dice of its fall are down.
    pub fn shown_guard(&self) -> Option<necromy_rules::Guard> {
        self.game.guard().or(self.held_guard)
    }

    /// What mob `id` is (or was); the undead if never seen.
    pub fn mob_kind(&self, id: u32) -> necromy_rules::MobKind {
        self.seen_mobs
            .get(&id)
            .map_or(necromy_rules::MobKind::Undead, |m| m.kind)
    }

    /// Mob `id` as last seen.
    pub fn seen_mob(&self, id: u32) -> Option<necromy_rules::Mob> {
        self.seen_mobs.get(&id).copied()
    }

    /// Mob `id`'s name, nominative and accusative.
    pub fn mob_name(&self, id: u32) -> (&'static str, &'static str) {
        names::mob(self.mob_kind(id), id)
    }

    /// The mobs as the board shows them: with those whose fall the dice
    /// on screen have not told yet.
    pub fn shown_mobs(&self) -> Vec<necromy_rules::Mob> {
        let mut all = self.game.mobs().to_vec();
        all.extend(self.held_mobs.iter().copied());
        all
    }

    /// A settlement's militia as the board shows them: as they stood before
    /// the fight on screen, until its dice are down.
    pub fn shown_militia(&self, home: Hex) -> Option<necromy_rules::Militia> {
        match self.held_militia {
            Some((h, m)) if h == home => Some(m),
            _ => self.game.militia_unit(home),
        }
    }

    /// `player` fights in the show on the panels now.
    fn in_show_on_screen(&self, player: PlayerId) -> bool {
        self.on_screen
            .and_then(|id| self.shows.iter().find(|s| s.id == id))
            .is_some_and(|s| s.who.contains(&player))
    }

    /// The human looks away from a show that is not theirs.
    pub fn close_show(&mut self) {
        self.end_battle_view();
        self.show_serial += 1;
    }

    /// What the battle and trial panels learn from an event of the show on
    /// screen.
    fn screen_event(&mut self, event: &Event) {
        match event {
            Event::TrialBegun { player, hex } => {
                self.trial = Some(TrialInfo {
                    player: *player,
                    hex: *hex,
                    trial: self.game.trial_at(*hex).cloned(),
                    burned: Vec::new(),
                    result: None,
                });
            }
            Event::TrialPassed {
                player,
                trial,
                got,
                need,
            }
            | Event::TrialFailed {
                player,
                trial,
                got,
                need,
            } => {
                let passed = matches!(event, Event::TrialPassed { .. });
                if let Some(t) = self.trial.as_mut().filter(|t| t.player == *player) {
                    t.trial = Some(trial.clone());
                    t.result = Some((*got, *need, passed));
                }
            }
            Event::BattleStarted { attacker, defender } => {
                self.battle = Some(BattleInfo::new([
                    Fighter::Champion(*attacker),
                    Fighter::Champion(*defender),
                ]));
            }
            Event::GuardStruck { target } => {
                self.battle = Some(BattleInfo::new([
                    Fighter::Guard,
                    Fighter::Champion(*target),
                ]));
            }
            Event::GuardAttacked { attacker } => {
                self.battle = Some(BattleInfo::new([
                    Fighter::Champion(*attacker),
                    Fighter::Guard,
                ]));
            }
            Event::MobStruck { id, target } => {
                self.battle = Some(BattleInfo::new([
                    Fighter::Mob(*id),
                    Fighter::Champion(*target),
                ]));
            }
            Event::MobAttacked { attacker, id } => {
                self.battle = Some(BattleInfo::new([
                    Fighter::Champion(*attacker),
                    Fighter::Mob(*id),
                ]));
            }
            Event::MilitiaAttacked { attacker, home } => {
                self.battle = Some(BattleInfo::new([
                    Fighter::Champion(*attacker),
                    Fighter::Militia(*home),
                ]));
            }
            // The champion always strikes first against the militia.
            Event::MilitiaResolved {
                militia_score,
                champion_score,
                ..
            } => {
                if let Some(b) = self.battle.as_mut() {
                    b.scores = Some((*champion_score, *militia_score));
                }
            }
            // Men lost, as a side's health on the panel.
            Event::MilitiaHurt { men, .. } => {
                if let Some(b) = self.battle.as_mut()
                    && let Some(side) = b.foe_side()
                    && matches!(b.sides[side], Fighter::Militia(_))
                {
                    let before = b.hp[side].map_or(necromy_rules::MILITIA, |(before, _)| before);
                    b.hp[side] = Some((before, *men));
                }
            }
            Event::MilitiaFell { .. } => {
                if let Some(b) = self.battle.as_mut()
                    && let Some(side) = b.foe_side()
                    && matches!(b.sides[side], Fighter::Militia(_))
                {
                    b.fell[side] = true;
                }
            }
            Event::MobResolved {
                champion_attacked,
                mob_score,
                champion_score,
                ..
            } => {
                if let Some(b) = self.battle.as_mut() {
                    b.scores = Some(if *champion_attacked {
                        (*champion_score, *mob_score)
                    } else {
                        (*mob_score, *champion_score)
                    });
                }
            }
            Event::MobHurt { amount, hp, .. } => {
                if let Some(b) = self.battle.as_mut()
                    && let Some(side) = b.foe_side()
                {
                    let before = b.hp[side].map_or(hp + amount, |(before, _)| before);
                    b.hp[side] = Some((before, *hp));
                }
            }
            Event::MobFell { .. } => {
                if let Some(b) = self.battle.as_mut()
                    && let Some(side) = b.foe_side()
                {
                    b.fell[side] = true;
                }
            }
            // The guard's wounds, as a side's health on the panel.
            Event::GuardHurt { amount, hp } => {
                if let Some(b) = self.battle.as_mut()
                    && let Some(side) = b.foe_side()
                {
                    let before = b.hp[side].map_or(hp + amount, |(before, _)| before);
                    b.hp[side] = Some((before, *hp));
                }
            }
            Event::GuardFell { .. } => {
                if let Some(b) = self.battle.as_mut()
                    && let Some(side) = b.foe_side()
                {
                    b.fell[side] = true;
                }
            }
            Event::Burned { player, faces, .. } => {
                if let Some(t) = self.trial.as_mut().filter(|t| t.player == *player) {
                    t.burned = faces.clone();
                }
                if let Some(b) = self.battle.as_mut() {
                    let side = b.side(*player);
                    b.burned[side] = faces.clone();
                }
            }
            Event::BattleResolved {
                attacker_score,
                defender_score,
                ..
            } => {
                if let Some(b) = self.battle.as_mut() {
                    b.scores = Some((*attacker_score, *defender_score));
                }
            }
            Event::GuardResolved {
                guard_score,
                target_score,
                ..
            } => {
                // In the order of the panel's sides: attacker first.
                if let Some(b) = self.battle.as_mut() {
                    b.scores = Some(if b.foe_side() == Some(0) {
                        (*guard_score, *target_score)
                    } else {
                        (*target_score, *guard_score)
                    });
                }
            }
            // Blows of the battle on screen: remember health before and after.
            Event::Damaged { player, amount, hp } => {
                if let Some(b) = self.battle.as_mut().filter(|b| b.scores.is_some())
                    && let Some(side) = b.side_of(*player)
                {
                    let before = b.hp[side].map_or(hp + amount, |(before, _)| before);
                    b.hp[side] = Some((before, *hp));
                }
            }
            Event::DiceThrown {
                fighter,
                seed,
                count,
                faces,
            } => {
                let side = self.battle.as_ref().map_or(0, |b| match fighter {
                    Fighter::Champion(p) => b.side(*p),
                    Fighter::Guard | Fighter::Mob(_) | Fighter::Militia(_) => {
                        b.foe_side().unwrap_or(0)
                    }
                });
                self.throws.push(ThrowView {
                    side,
                    seed: *seed,
                    count: *count,
                    faces: faces.clone(),
                });
                self.holding = true;
            }
            Event::ChampionFell { player, .. } => {
                if let Some(b) = self.battle.as_mut().filter(|b| b.scores.is_some())
                    && let Some(side) = b.side_of(*player)
                {
                    b.fell[side] = true;
                }
            }
            _ => {}
        }
    }

    /// The battle panel closes: the fight is told in the feed.
    pub fn end_battle_view(&mut self) {
        // The fallen go home now that the dice have told it.
        for (player, _, respawn) in std::mem::take(&mut self.held_falls) {
            self.steps.push((player, respawn));
        }
        self.held_guard = None;
        self.held_mobs.clear();
        self.held_militia = None;
        self.battle = None;
        self.trial = None;
        self.on_screen = None;
        self.watching = false;
        self.release_feed();
    }

    /// The dice show is over: what happened can be told now.
    pub fn release_feed(&mut self) {
        self.holding = false;
        let held = std::mem::take(&mut self.held);
        self.feed.extend(held);
        let excess = self.feed.len().saturating_sub(FEED_LINES);
        self.feed.drain(..excess);
    }

    /// "даёт Тришне": the seat's name in the dative.
    pub fn name_dative(&self, player: PlayerId) -> String {
        let god = self
            .game
            .champion(player)
            .map_or("?", |c| names::god_dative(c.god));
        if player == self.human {
            format!("{god} (тебе)")
        } else {
            god.to_string()
        }
    }

    /// "по желанию Тришны": the seat's name in the genitive.
    pub fn name_genitive(&self, player: PlayerId) -> String {
        let god = self
            .game
            .champion(player)
            .map_or("?", |c| names::god_genitive(c.god));
        if player == self.human {
            format!("{god} (твоему)")
        } else {
            god.to_string()
        }
    }

    /// "натыкается на Тришну": the seat's name in the accusative.
    pub fn name_accusative(&self, player: PlayerId) -> String {
        let god = self
            .game
            .champion(player)
            .map_or("?", |c| names::god_accusative(c.god));
        if player == self.human {
            format!("{god} (тебя)")
        } else {
            god.to_string()
        }
    }

    pub fn name(&self, player: PlayerId) -> String {
        let god = self
            .game
            .champion(player)
            .map_or("?", |c| names::god(c.god));
        if player == self.human {
            format!("{god} (ты)")
        } else {
            god.to_string()
        }
    }

    fn card_name(&self, card: CardId) -> &'static str {
        self.game.def(card).name
    }

    fn describe(&self, event: &Event) -> Option<String> {
        let me = self.human;
        Some(match event {
            Event::RoundStarted { round, time, .. } => {
                format!("— раунд {round}: {}, все ходят разом —", time_name(*time))
            }
            Event::Dawn { .. } => "Рассвет.".into(),
            Event::Dusk { .. } => "Закат. (Здесь проснутся боги.)".into(),
            Event::CorpseAppeared { .. } => "На доске появилось тело.".into(),
            Event::CorpseDecayed { .. } => "Тело истлело.".into(),
            Event::GroveGrew { .. } => "Выросла роща.".into(),
            Event::CardDrawn { player, card, .. } if *player == me => {
                // A card a wish changed in the deck shows itself when drawn.
                match self.game.card_mod(*card) {
                    Some(m) => format!(
                        "Ты берёшь «{}» — {}.",
                        self.card_name(*card),
                        names::card_mod(m).0.to_lowercase()
                    ),
                    None => format!("Ты берёшь «{}».", self.card_name(*card)),
                }
            }
            Event::CardPlayed { player, card, .. } => {
                format!("{} играет «{}».", self.name(*player), self.card_name(*card))
            }
            Event::Chain {
                from,
                to,
                rhythm_broken,
                ..
            } => {
                let tail = if *rhythm_broken {
                    " Ритм сломан: всплеск ци."
                } else {
                    ""
                };
                format!(
                    "Цепочка: {} питает {}, +1.{tail}",
                    names::element(*from),
                    names::element(*to)
                )
            }
            // Simultaneous turns (§11.2): an action that came too close waits.
            Event::Held { player, on } if *player == me => match on {
                Some(on) => format!(
                    "{} ещё ходит: твоё действие сыграется, когда {} закончит.",
                    self.name(*on),
                    self.name(*on)
                ),
                None => "Рядом кто-то занят: твоё действие подождёт.".into(),
            },
            Event::Held {
                player,
                on: Some(on),
            } if *on == me => format!("{} ждёт, пока ты закончишь ход.", self.name(*player)),
            Event::Resumed { player } if *player == me => {
                "Твоё отложенное действие играется.".into()
            }
            Event::HoldDropped { player, why } if *player == me => {
                format!("Отложенное действие отменено: {}.", reason(*why))
            }
            Event::WindowOpened { kind, eligible } if eligible.contains(&me) => {
                format!("Окно реакции: {}.", window_name(self, *kind))
            }
            Event::Canceled { card, .. } => format!("«{}» гаснет.", self.card_name(*card)),
            Event::CancelFailed { card, .. } => {
                format!(
                    "«{}» не погасить: её стихия сильнее.",
                    self.card_name(*card)
                )
            }
            Event::Fizzled { card } => format!("«{}» уходит впустую.", self.card_name(*card)),
            Event::Damaged { player, amount, hp } => {
                format!("{}: −{amount} здоровья ({hp}).", self.name(*player))
            }
            Event::Healed { player, amount, hp } => {
                format!("{}: +{amount} здоровья ({hp}).", self.name(*player))
            }
            Event::WardRaised { player, element } => {
                format!(
                    "{}: оберег ({}).",
                    self.name(*player),
                    names::element(*element)
                )
            }
            Event::WardBroken { player, ward, by } => format!(
                "{}: {} ломает оберег ({}).",
                self.name(*player),
                names::element(*by),
                names::element(*ward)
            ),
            Event::Blocked { player, ward } => format!(
                "{}: оберег ({}) держит удар.",
                self.name(*player),
                names::element(*ward)
            ),
            Event::Rooted { player } => format!("{} скован.", self.name(*player)),
            Event::Poisoned {
                player,
                element,
                stacks,
            } => format!(
                "{}: яд {} ({stacks}).",
                self.name(*player),
                names::element_genitive(*element)
            ),
            Event::PoisonFed { player, stacks } => {
                format!("{}: лечение кормит яд ({stacks}).", self.name(*player))
            }
            Event::PoisonBit {
                player, amount, hp, ..
            } if *amount > 0 => format!("{}: яд, −{amount} здоровья ({hp}).", self.name(*player)),
            Event::PoisonBit { player, .. } => {
                format!("{}: яд не берёт последнее здоровье.", self.name(*player))
            }
            Event::PoisonCured { player, by } => match by {
                Cure::Heal(e) => {
                    format!("{}: {} снимает яд.", self.name(*player), names::element(*e))
                }
                Cure::Temple => format!("{}: храм очищает от яда.", self.name(*player)),
                Cure::Trial => format!("{}: испытание снимает яд.", self.name(*player)),
            },
            Event::Hid { player, .. } => format!("{} уходит в тень.", self.name(*player)),
            Event::Revealed { player, why, .. } => format!(
                "{} выходит из тени: {}.",
                self.name(*player),
                names::reveal(*why)
            ),
            Event::Stumbled { mover, hidden, .. } => format!(
                "{} натыкается на {} — засада!",
                self.name(*mover),
                self.name_accusative(*hidden)
            ),
            Event::TrapSet { player, .. } if *player == me => "Ловушка поставлена.".into(),
            Event::TrapSprung {
                owner, victim, def, ..
            } => format!(
                "{} попадает в ловушку «{}» ({}).",
                self.name(*victim),
                def.def().name,
                self.name(*owner)
            ),
            Event::ChampionFell { player, .. } => {
                format!("{} падает и просыпается дома.", self.name(*player))
            }
            Event::TrialSet { trial } => format!(
                "{} ставит {}: {}.",
                names::god(trial.god),
                names::trial_name(trial.god),
                names::trial_ask(&self.game, trial)
            ),
            Event::TrialBegun { player, hex } => {
                let what = self
                    .game
                    .trial_at(*hex)
                    .map_or("испытание", |t| names::trial_name(t.god));
                format!("{} выходит на {what}.", self.name(*player))
            }
            Event::TrialPassed {
                player,
                trial,
                got,
                need,
            } => format!(
                "{} проходит {} ({got} из {need}): {}.",
                self.name(*player),
                names::trial_name(trial.god),
                names::boon(&self.game, trial)
            ),
            Event::TrialFailed {
                player,
                trial,
                got,
                need,
            } => format!(
                "{} не выдерживает {} ({got} из {need}): {}.",
                self.name(*player),
                names::trial_name(trial.god),
                names::trial_price(&self.game, trial.god)
            ),
            Event::TrialFaded { .. } => "Испытание угасло: его никто не прошёл.".into(),
            Event::ItemGained { player, item, from } => match from {
                necromy_rules::Gain::Loot => format!(
                    "{} получает из добычи: {} ({}).",
                    self.name(*player),
                    item.def().name,
                    names::slot(item.def().slot)
                ),
                necromy_rules::Gain::Ground => {
                    format!("{} подбирает: {}.", self.name(*player), item.def().name)
                }
                necromy_rules::Gain::Gift(god) => format!(
                    "{} дарит {}: {} ({}).",
                    names::god(*god),
                    self.name_dative(*player),
                    item.def().name,
                    names::slot(item.def().slot)
                ),
            },
            Event::ItemDropped { player, item, .. } => {
                format!(
                    "{} оставляет на земле: {}.",
                    self.name(*player),
                    item.def().name
                )
            }
            Event::ItemBroken { player, item, by } => format!(
                "{}: {} ломает «{}».",
                self.name(*player),
                names::element(*by),
                item.def().name
            ),
            Event::ItemSacrificed { player, item, god } => format!(
                "{} отдаёт {}: {}.",
                self.name(*player),
                names::god_dative(*god),
                item.def().name
            ),
            Event::ItemWorked { player, item } => format!(
                "{}: {} — {}.",
                self.name(*player),
                item.def().name,
                names::item_does(&self.game, *item)
            ),
            Event::ItemToll { player, item } => {
                let god = God::from_index(item.def().element.index());
                format!(
                    "{}: тёмный {} берёт плату за «{}» — {}.",
                    self.name(*player),
                    names::god(god),
                    item.def().name,
                    names::item_toll(god)
                )
            }
            Event::DeckReshuffled => "Колода перемешана.".into(),
            Event::LineTold { line } => format!(
                "{} даёт {} линию «{}» до раунда {}.",
                names::god(line.god),
                self.name_dative(line.owner),
                names::line_title(line.kind),
                line.deadline
            ),
            Event::LineDone { line } => format!(
                "{}: линия «{}» исполнена, +{} Стиля.",
                self.name(line.owner),
                names::line_title(line.kind),
                line.style
            ),
            Event::LineFailed { line } => format!(
                "{}: линия «{}» провалена{}.",
                self.name(line.owner),
                names::line_title(line.kind),
                if line.stake > 0 {
                    format!(", −{} Стиля", line.stake)
                } else {
                    String::new()
                }
            ),
            Event::WorldStirred { stir } => names::world_stir(*stir).into(),
            Event::WishDue { player } => format!("Закат ждёт желания: {}…", self.name(*player)),
            Event::WishSealed { player } => format!("{} запечатывает желание.", self.name(*player)),
            Event::WishLost { player, god } => format!(
                "{} не находит, что дать {}: запечатанного больше нет.",
                names::god(*god),
                self.name_dative(*player)
            ),
            Event::WishRefused { player } => format!("{} ничего не просит.", self.name(*player)),
            Event::LandRaised { terrain, .. } => format!(
                "Из мглы поднимается земля: {}.",
                names::terrain(*terrain).0.to_lowercase()
            ),
            Event::WorldGrew {
                feature,
                god,
                player,
            } => {
                let (name, what) = names::feature(*feature);
                match player {
                    Some(p) => format!(
                        "В мир приходит новое — {name}: {what}. Принёс {} по желанию {}.",
                        names::god(*god),
                        self.name_genitive(*p)
                    ),
                    None => format!("В мир приходит новое — {name}: {what}."),
                }
            }
            Event::CargoTaken { player, cargo, .. } => {
                format!(
                    "{} берёт на плечи: {}.",
                    self.name(*player),
                    names::cargo(*cargo)
                )
            }
            Event::CargoLaid { player, cargo, .. } => {
                format!(
                    "{} опускает ношу: {}.",
                    self.name(*player),
                    names::cargo(*cargo)
                )
            }
            Event::CargoSeized {
                player,
                from,
                cargo,
            } => format!(
                "{} забирает у {} ношу: {}.",
                self.name(*player),
                self.name_genitive(*from),
                names::cargo(*cargo)
            ),
            Event::Built {
                player, building, ..
            } => format!(
                "{} строит на своём поселении: {}.",
                self.name(*player),
                names::building(*building)
            ),
            Event::Feasted { player, guests, .. } => {
                format!("{} устраивает пир: гостей {guests}.", self.name(*player))
            }
            Event::FoodStored { player, food, .. } => {
                format!("{} несёт еду в запасы: теперь {food}.", self.name(*player))
            }
            Event::Scorched { player, amount, .. } if *amount > 0 => format!(
                "Огонь обжигает {}: −{amount}.",
                self.name_accusative(*player)
            ),
            Event::PiranhasBit { player, amount, .. } if *amount > 0 => format!(
                "Пираньи кусают {}: −{amount}.",
                self.name_accusative(*player)
            ),
            Event::CompanionJoined { player, companion } => format!(
                "{} ведёт нового спутника: {}.",
                self.name(*player),
                names::companion(*companion)
            ),
            Event::CompanionSeized {
                player,
                from,
                companion,
            } => format!(
                "{} уводит у {} спутника: {}.",
                self.name(*player),
                self.name_genitive(*from),
                names::companion(*companion)
            ),
            Event::CompanionsScattered { player, count } => format!(
                "У {} разбегаются спутники: {count}.",
                self.name_genitive(*player)
            ),
            Event::QuarterRaised { player, .. } => format!(
                "{} поднимает новый квартал своего города.",
                self.name(*player)
            ),
            Event::First { player, novelty } => format!(
                "{} первым за столом {}: +{} Стиля.",
                self.name(*player),
                names::novelty(*novelty),
                necromy_rules::FIRST_STYLE
            ),
            Event::Cycled {
                player,
                let_go,
                drawn,
            } => format!(
                "{} перебирает руку: сбрасывает {let_go}, берёт {drawn}.",
                self.name(*player)
            ),
            Event::DeckGrew { cards, .. } => {
                format!("В колоду замешаны новые карты: {cards}.")
            }
            Event::AwakeningDeferred { god, feature, .. } => format!(
                "{} откладывает до следующего заката: {}. Нового — не больше одного за закат.",
                names::god(*god),
                names::feature(*feature).0.to_lowercase()
            ),
            Event::WishGranted {
                player,
                god,
                wish,
                dropped,
                grade,
                ..
            } => format!(
                "{} просит {}: «{}». Оценка {grade}/3.{}",
                self.name(*player),
                names::god_accusative(*god),
                names::wish_phrase(wish),
                if *dropped > 0 {
                    " Всего бог не дал: оценка не покрыла."
                } else {
                    ""
                }
            ),
            // What a wish told one player stays in their feed (§7.3).
            Event::SecretLearned { player, about } if *player == me => format!(
                "Бог шепчет тебе, что {} загадает на закате: ты увидишь это, как только желание запечатают.",
                self.name(*about)
            ),
            Event::SecretLearned { player, about } => format!(
                "{} узнаёт тайное условие {}.",
                self.name(*player),
                self.name(*about)
            ),
            Event::HandSeen {
                player,
                about,
                cards,
            } if *player == me => {
                let names: Vec<String> = cards
                    .iter()
                    .map(|d| format!("«{}»", d.def().name))
                    .collect();
                if names.is_empty() {
                    format!("Рука {} пуста.", self.name(*about))
                } else {
                    format!("В руке {}: {}.", self.name(*about), names.join(", "))
                }
            }
            Event::HandSeen { player, about, .. } => format!(
                "{} заглядывает в руку {}.",
                self.name(*player),
                self.name(*about)
            ),
            Event::CardChanged {
                owner,
                card,
                god,
                blessed,
            } => {
                let what = if *owner == me {
                    format!("твою «{}»", self.game.card_name(*card))
                } else {
                    format!("карту {}", self.name(*owner))
                };
                if *blessed {
                    format!("{} благословляет {what}.", names::god(*god))
                } else {
                    format!("{} наводит порчу на {what}.", names::god(*god))
                }
            }
            Event::CardForged { player, card, god } => {
                if *player == me {
                    format!(
                        "{} куёт тебе новую карту: «{}».",
                        names::god(*god),
                        self.game.card_name(*card)
                    )
                } else {
                    format!(
                        "{} куёт новую карту для {}.",
                        names::god(*god),
                        self.name(*player)
                    )
                }
            }
            Event::TruceMade { player, other, god } => format!(
                "{} скрепляет мир: {} и {} не сражаются до заката.",
                names::god(*god),
                self.name(*player),
                self.name(*other)
            ),
            Event::TruceBroken { player, other, god } => format!(
                "{} нарушает мир с {}: проклятие {} и −2 Стиля.",
                self.name(*player),
                self.name(*other),
                names::god_genitive(*god)
            ),
            Event::DeckChanged {
                player,
                god,
                element,
                count,
                blessed,
            } => format!(
                "{} {} в колоде карты стихии «{}» ({} шт.) по просьбе {}.",
                names::god(*god),
                if *blessed {
                    "освящает"
                } else {
                    "отравляет"
                },
                names::element(*element),
                count,
                self.name(*player)
            ),
            Event::CursePlanted { player, god } => format!(
                "{} прячет в колоде проклятие {}.",
                self.name(*player),
                names::god_genitive(*god)
            ),
            Event::Foreseen { player, cards } if *player == me => {
                let names: Vec<String> = cards
                    .iter()
                    .map(|d| format!("«{}»", d.def().name))
                    .collect();
                format!("Сверху колоды: {}.", names.join(", "))
            }
            Event::Foreseen { player, .. } => {
                format!("{} заглядывает в колоду.", self.name(*player))
            }
            Event::CurseDrawn {
                player, god, bit, ..
            } => {
                if *bit {
                    format!(
                        "{} вытягивает проклятие {}: −1 здоровья.",
                        self.name(*player),
                        names::god_genitive(*god)
                    )
                } else {
                    format!(
                        "{} вытягивает своё же проклятие: оно рассеивается.",
                        self.name(*player)
                    )
                }
            }
            Event::TributeGiven { player, to, card } => {
                if *to == me {
                    format!(
                        "{} платит тебе дань: «{}».",
                        self.name(*player),
                        self.game.card_name(*card)
                    )
                } else if *player == me {
                    format!(
                        "Ты отдаёшь {} дань: «{}».",
                        self.name(*to),
                        self.game.card_name(*card)
                    )
                } else {
                    format!("{} платит дань {}.", self.name(*player), self.name(*to))
                }
            }
            Event::TributeRefused { player, to } => format!(
                "{} отказывает {} в дани: +2 Угрозы.",
                self.name(*player),
                self.name(*to)
            ),
            Event::WagerMade {
                player,
                target,
                bet,
                god,
            } => format!(
                "{} ставит у {}: {} до заката {}.",
                self.name(*player),
                names::god_genitive(*god),
                self.name(*target),
                names::bet(*bet)
            ),
            Event::WagerWon { player, .. } => {
                format!("{} выигрывает пари: +2 Стиля.", self.name(*player))
            }
            Event::WagerLost { player, god, .. } => format!(
                "{} проигрывает пари: +2 Угрозы и долг {} (проклятие).",
                self.name(*player),
                names::god_genitive(*god)
            ),
            Event::Swapped { player, other, .. } => format!(
                "{} и {} меняются местами.",
                self.name(*player),
                self.name(*other)
            ),
            Event::PricePaid { player, price } => format!(
                "{} отдаёт богу {}.",
                self.name(*player),
                names::price(*price, |c| self.card_name(c).to_string())
            ),
            Event::TerrainChanged { terrain, .. } => {
                format!(
                    "Земля становится: {}.",
                    names::terrain(*terrain).0.to_lowercase()
                )
            }
            Event::CardDissolved { player, card } => {
                format!(
                    "Майя растворяет «{}» у {}.",
                    self.card_name(*card),
                    self.name(*player)
                )
            }
            Event::CurseLaid { player, god } => format!(
                "Проклятие {} на {}: −1 здоровья каждый ход.",
                names::god_genitive(*god),
                self.name(*player)
            ),
            Event::CurseBit { player, god } => format!(
                "Проклятие {} жжёт {}.",
                names::god_genitive(*god),
                self.name(*player)
            ),
            Event::CurseLifted { player, god } => format!(
                "Проклятие {} снято с {}.",
                names::god_genitive(*god),
                self.name(*player)
            ),
            Event::Victory { player, deed } => format!(
                "Победа: {} — {}.",
                self.name(*player),
                names::great_deed(*deed).0
            ),
            Event::DeedChosen { player, deed } => format!(
                "{} берётся за Великое деяние: «{}».",
                self.name(*player),
                names::great_deed(*deed).0
            ),
            Event::DeedEve { player, deed } => format!(
                "Канун: деяние «{}» у {} готово — на закате свершится, если его не сорвут.",
                names::great_deed(*deed).0,
                self.name_genitive(*player)
            ),
            Event::EveBroken { player, deed } => format!(
                "Канун сорван: деяние «{}» у {} уже не держится.",
                names::great_deed(*deed).0,
                self.name_genitive(*player)
            ),
            Event::Claimed { player, hex, .. } => {
                let what = match self.game.board().tile(*hex).map(|t| t.terrain) {
                    Some(Terrain::Temple) => "храм",
                    Some(Terrain::Table) => "Стол Ахамара",
                    _ => "поселение",
                };
                format!("{} занимает {what}.", self.name(*player))
            }
            Event::StyleChanged {
                player,
                delta,
                total,
                reason,
            } => format!(
                "{}: {:+} Стиля за {} ({total}).",
                self.name(*player),
                delta,
                names::style_reason(*reason)
            ),
            Event::ThreatChanged {
                player,
                delta,
                total,
            } => {
                format!("{}: Угроза {:+} ({total}).", self.name(*player), delta)
            }
            Event::Crowned { player: Some(p) } => {
                format!("Венец у {}: желание за ним.", self.name(*p))
            }
            Event::Crowned { player: None } => "Венец ни у кого: стол спорный.".into(),
            Event::GuardSpawned { target, .. } => {
                format!("Королевская гвардия выходит за {}!", self.name(*target))
            }
            Event::GuardLeft { .. } => "Гвардия уходит: на столе тихо.".into(),
            Event::GuardStruck { target } => format!("Гвардия бьёт {}!", self.name(*target)),
            Event::GuardAttacked { attacker } => {
                format!("{} нападает на королевскую гвардию!", self.name(*attacker))
            }
            Event::GuardHurt { amount, hp } => {
                format!("Гвардия: −{amount} ({hp}/{}).", necromy_rules::GUARD_HEALTH)
            }
            Event::MobAppeared { mob } if mob.is_beast() => format!(
                "Из леса Бхавы выходит зверь: {}.",
                names::mob(mob.kind, mob.id).1
            ),
            Event::MobAppeared { .. } => "Нетронутое тело поднимается: неупокоенный.".into(),
            Event::MobLeft { .. } => "Бхава светел: звери уходят в чащу.".into(),
            Event::BeastMauled { beast, .. } => format!(
                "{} рвёт неупокоенного, забредшего в его лес.",
                self.mob_name(*beast).0
            ),
            Event::MobStruck { id, target } => {
                format!(
                    "{} бьёт {}!",
                    self.mob_name(*id).0,
                    self.name_accusative(*target)
                )
            }
            Event::MobAttacked { attacker, id } => {
                format!(
                    "{} нападает на {}.",
                    self.name(*attacker),
                    self.mob_name(*id).1
                )
            }
            Event::MobHurt { id, amount, hp } => format!(
                "{}: −{amount} ({hp}/{}).",
                self.mob_name(*id).0,
                self.mob_kind(*id).health()
            ),
            Event::MobFell { id, by, .. } if self.mob_kind(*id).is_beast() => {
                format!("{} убивает {}.", self.name(*by), self.mob_name(*id).1)
            }
            Event::MobFell { by, .. } => {
                format!("{} упокаивает неупокоенного.", self.name(*by))
            }
            Event::MilitiaStruck {
                mob, fell: true, ..
            } => {
                format!("Ополчение поселения рубит {}.", self.mob_name(*mob).1)
            }
            Event::MilitiaStruck { mob, .. } => {
                format!("Ополчение поселения ранит {}.", self.mob_name(*mob).1)
            }
            Event::MilitiaHit {
                player,
                why: necromy_rules::MilitiaWhy::Loud,
                ..
            } => format!(
                "Ополчение бьёт {}: от такого шума жди беды (Угроза).",
                self.name_accusative(*player)
            ),
            Event::MilitiaHit {
                player,
                why: necromy_rules::MilitiaWhy::Pursuer { friend },
                ..
            } => format!(
                "Ополчение заступается за {} и бьёт {}.",
                self.name_accusative(*friend),
                self.name_accusative(*player)
            ),
            Event::GuardHewed { .. } => "Гвардия по пути рубит неупокоенного.".into(),
            Event::SettlementRuined { .. } => {
                "Неупокоенные разорили поселение без ополчения: теперь там руины.".into()
            }
            Event::StandingChanged { player, standing } if *player == me => {
                format!(
                    "Ополчение к тебе: {standing:+} ({}).",
                    names::standing(*standing)
                )
            }
            Event::MilitiaHelped { player, .. } => {
                format!("{}: ополчение привечает, +1 здоровья.", self.name(*player))
            }
            Event::MilitiaBeat { player, .. } => {
                format!("{}: ополчение гонит из поселения.", self.name(*player))
            }
            Event::MilitiaSwapped { player, .. } if *player == me => {
                "Ополчение пропускает тебя и отходит в сторону.".into()
            }
            Event::MilitiaAttacked { attacker, .. } => {
                format!("{} нападает на ополчение поселения!", self.name(*attacker))
            }
            Event::MilitiaHurt { men, .. } => {
                format!("Ополчение: {men}/{} человек.", necromy_rules::MILITIA)
            }
            Event::MilitiaFell { .. } => "Ополчение поселения пало: его некому держать.".into(),
            Event::UndeadHitMilitia { .. } => "Неупокоенный бьёт ополчение у ворот.".into(),
            Event::SettlementRebuilt { player, .. } => format!(
                "{} восстанавливает поселение: оно снова живо.",
                self.name(*player)
            ),
            Event::MilitiaBarred { player, .. } => {
                format!(
                    "{}: ополчение не даёт занять поселение.",
                    self.name(*player)
                )
            }
            Event::GuardFell { by, .. } => format!(
                "{} повергает королевскую гвардию: она уходит со стола.",
                self.name(*by)
            ),
            Event::GuardResolved {
                target,
                guard_score: gs,
                target_score: ts,
            } => format!(
                "Итог удара гвардии: ударов {}, щитов {}; {} — ударов {}, щитов {}.",
                gs.hits,
                gs.shields,
                self.name(*target),
                ts.hits,
                ts.shields
            ),
            Event::StageChanged { god, stage } => format!(
                "{} переходит в стадию «{}»: {}.",
                names::god(*god),
                names::stage(*god, *stage),
                names::law_text(necromy_rules::Law::of(*god, *stage))
            ),
            // A law of the world acted (§5.3).
            Event::Law { law, player, .. } => {
                let whom = player.map_or(String::new(), |p| format!(" ({})", self.name(p)));
                format!(
                    "Закон «{}»{whom}: {}.",
                    names::law_name(*law),
                    names::law_text(*law)
                )
            }
            Event::BattleStarted { attacker, defender } => {
                format!(
                    "{} нападает на {}!",
                    self.name(*attacker),
                    self.name(*defender)
                )
            }
            Event::Burned { player, faces, .. } => format!(
                "{} сжигает карты: {}.",
                self.name(*player),
                faces
                    .iter()
                    .map(|f| names::face(*f))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Event::BattleResolved {
                attacker,
                defender,
                attacker_score: a,
                defender_score: d,
            } => format!(
                "Итог боя: {} — ударов {}, щитов {}; {} — ударов {}, щитов {}.",
                self.name(*attacker),
                a.hits,
                a.shields,
                self.name(*defender),
                d.hits,
                d.shields
            ),
            _ => return None,
        })
    }
}

pub fn time_name(time: TimeOfDay) -> &'static str {
    match time {
        TimeOfDay::Day => "день",
        TimeOfDay::Night => "ночь",
    }
}

pub fn window_name(m: &Match, kind: WindowKind) -> String {
    match kind {
        WindowKind::Enter { mover, .. } => format!("{} входит рядом", m.name(mover)),
        WindowKind::Target {
            caster,
            target,
            card,
        } => format!(
            "{} целит «{}» в {}",
            m.name(caster),
            m.game.def(card).name,
            m.name(target)
        ),
        WindowKind::Battle { attacker, defender } => {
            format!("бой: {} против {}", m.name(attacker), m.name(defender))
        }
        WindowKind::Tribute { asker } => format!("дань для {}", m.name(asker)),
        WindowKind::Trial { player, .. } => format!("{} на испытании", m.name(player)),
        WindowKind::MilitiaBattle { attacker, .. } => {
            format!("бой: {} против ополчения", m.name(attacker))
        }
        WindowKind::MobBattle { attacker, id } => {
            format!("бой: {} против {}", m.name(attacker), m.mob_name(id).1)
        }
        WindowKind::GuardBattle { attacker } => {
            format!("бой: {} против гвардии", m.name(attacker))
        }
    }
}

/// Plays `card` for the human with the only sensible target, or starts
/// aiming it. Called by the hand UI.
pub fn pick_card(m: &mut Match, selection: &mut Selection, card: CardId) {
    // Going through the hand: a click marks the card, or unmarks it.
    if let Some(marked) = selection.sift.as_mut() {
        match marked.iter().position(|&c| c == card) {
            Some(i) => {
                marked.remove(i);
            }
            None => marked.push(card),
        }
        return;
    }
    // Tribute: the card clicked is the card given, no aiming.
    if matches!(
        m.game.to_answer(m.human).map(|w| w.kind),
        Some(WindowKind::Tribute { .. })
    ) {
        let human = m.human;
        let _ = m.act(
            human,
            Intent::Play {
                card,
                target: Target::None,
            },
        );
        selection.card = None;
        return;
    }
    if let Some(max) = m.game.battle_dice(m.human)
        && m.human_awaited()
    {
        if let Some(i) = selection.burn.iter().position(|&c| c == card) {
            selection.burn.remove(i);
        } else if selection.burn.len() < max as usize {
            selection.burn.push(card);
        } else {
            m.feed.push(format!("Сжечь можно не больше {max} карт."));
        }
        return;
    }
    if selection.card == Some(card) {
        selection.card = None;
        return;
    }
    if let Err(err) = m.game.can_play_now(m.human, card) {
        m.feed.push(format!("Нельзя: {}.", reason(err)));
        return;
    }
    let targets = m.game.targets(m.human, card);
    match targets.as_slice() {
        [] => m.feed.push("Нельзя: нет цели.".into()),
        [only @ (Target::None | Target::Hex(_))] => play(m, selection, card, *only),
        [Target::Champion(p)] if *p == m.human => play(m, selection, card, targets[0]),
        _ => selection.card = Some(card),
    }
}

fn play(m: &mut Match, selection: &mut Selection, card: CardId, target: Target) {
    selection.card = None;
    let human = m.human;
    if let Err(err) = m.act(human, Intent::Play { card, target }) {
        m.feed.push(format!("Нельзя: {}.", reason(err)));
    }
}

fn reason(err: RuleError) -> String {
    match err {
        RuleError::NotYourTurn => "не твой ход".into(),
        RuleError::WindowOpen => "идёт окно реакции".into(),
        RuleError::WrongTiming => "сейчас не время для этой карты".into(),
        RuleError::NotEnoughSpirit { need, have } => format!("нужно {need} Духа, есть {have}"),
        RuleError::InvalidTarget => "не та цель".into(),
        RuleError::NotAtTemple => "отдать богу можно только в его храме".into(),
        RuleError::NothingWorn => "там ничего нет".into(),
        RuleError::NotRuins => "восстановить можно только руины поселения".into(),
        RuleError::AlreadyChose => "ты уже выбрал".into(),
        other => other.to_string(),
    }
}

fn click_board(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::TableCamera>>,
    buttons: Query<&Interaction, With<Button>>,
    board: Res<Board>,
    mut game: ResMut<Match>,
    mut selection: ResMut<Selection>,
) {
    if mouse.just_pressed(MouseButton::Right) && selection.card.is_some() {
        selection.card = None;
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    // A click on a card belongs to the hand, not to the board under it.
    if buttons.iter().any(|i| *i != Interaction::None) {
        return;
    }
    let (camera, cam_transform) = *camera;
    let Some(hex) = board::cursor_hex(&window, camera, cam_transform, &board, game.game.board())
    else {
        return;
    };

    if let Some(card) = selection.card {
        let targets = game.game.targets(game.human, card);
        let on_champion = game
            .game
            .occupant(hex)
            .map(Target::Champion)
            .filter(|t| targets.contains(t));
        let target = on_champion.or(Some(Target::Hex(hex)).filter(|t| targets.contains(t)));
        match target {
            Some(target) => play(&mut game, &mut selection, card, target),
            None => game.feed.push("Не та цель. Правый клик — отменить.".into()),
        }
        return;
    }

    // A click on one's own hex on ruins builds the settlement again (§20.4).
    if game.is_human_turn()
        && game.game.champion(game.human).is_some_and(|c| c.hex == hex)
        && game.game.can_rebuild(game.human)
    {
        let human = game.human;
        if let Err(err) = game.act(human, Intent::Rebuild) {
            game.feed.push(format!("Нельзя: {}.", reason(err)));
        }
        return;
    }
    if game.is_human_turn() && game.game.attackable(game.human).contains(&hex) {
        let human = game.human;
        if let Err(err) = game.act(human, Intent::Move { to: hex }) {
            warn!("attack rejected: {err}");
        }
        return;
    }
    if !game.is_human_turn() {
        return;
    }
    let Some(path) = game.game.path_to(game.human, hex) else {
        return;
    };
    // One intent per step, as the table receives them. A step may open a
    // window; the rest of the path waits until the human clicks again.
    game.walk(path);
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    typing: Res<crate::wish_ui::Typing>,
    mut game: ResMut<Match>,
    mut selection: ResMut<Selection>,
) {
    // While the human writes a wish, the keyboard is the wish's.
    if typing.0 {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        selection.card = None;
        selection.sift = None;
    }
    let human = game.human;
    // Space in any window waiting on you: go on without answering (a pass),
    // as P does; on your own turn it ends the turn.
    if keys.just_pressed(KeyCode::Space) && game.human_window().is_some() {
        selection.card = None;
        selection.burn.clear();
        let _ = game.act(human, Intent::Pass);
        return;
    }
    if keys.any_just_pressed([KeyCode::Space, KeyCode::Enter])
        && game.is_human_turn()
        && let Err(err) = game.act(human, Intent::EndTurn)
    {
        warn!("end turn rejected: {err}");
    }
    let in_battle = game.game.battle_dice(human).is_some() && game.human_awaited();
    if in_battle && keys.just_pressed(KeyCode::Enter) {
        let cards = std::mem::take(&mut selection.burn);
        if let Err(err) = game.act(human, Intent::Burn { cards }) {
            game.feed.push(format!("Нельзя: {}.", reason(err)));
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyP) && game.human_window().is_some() {
        selection.card = None;
        selection.burn.clear();
        if let Err(err) = game.act(human, Intent::Pass) {
            warn!("pass rejected: {err}");
        }
    }
}

/// With nothing to answer, the human passes at once. The rules still see an
/// ordinary pass; on a server the window timer hides who had cards (§11.3).
/// A card aimed at the human stays on screen this long even when they have no
/// answer, so they can read what hits them.
const READ_INCOMING_SECS: f32 = 3.0;

/// Seconds left before an unanswerable card aimed at the human resolves by
/// itself; `None` when no such countdown runs.
#[derive(Resource, Default, PartialEq)]
pub struct IncomingCountdown(pub Option<f32>);

fn auto_pass(
    time: Res<Time>,
    mut seen: Local<Option<(WindowKind, f32)>>,
    mut countdown: ResMut<IncomingCountdown>,
    mut game: ResMut<Match>,
) {
    let Some(kind) = game.human_window() else {
        *seen = None;
        countdown.set_if_neq(IncomingCountdown(None));
        return;
    };
    let now = time.elapsed_secs();
    let opened = match *seen {
        Some((k, at)) if k == kind => at,
        _ => {
            *seen = Some((kind, now));
            now
        }
    };
    let aimed_at_me = matches!(kind, WindowKind::Target { target, .. } if target == game.human);
    let nothing_to_answer = !worth_answering(&game.game, game.human, kind);
    if aimed_at_me && now - opened < READ_INCOMING_SECS {
        let left = nothing_to_answer.then(|| (READ_INCOMING_SECS - (now - opened)).ceil());
        countdown.set_if_neq(IncomingCountdown(left));
        return;
    }
    countdown.set_if_neq(IncomingCountdown(None));
    // A battle waits for the human: burning cards is a choice even with no
    // card playable. Only an empty hand has nothing to decide.
    if game.game.battle_dice(game.human).is_some() && !game.game.hand(game.human).is_empty() {
        return;
    }
    // Under autoplay the table's bot answers for the seat.
    if nothing_to_answer && !game.autoplay {
        let human = game.human;
        let _ = game.act(human, Intent::Pass);
    }
}

/// Whether the human has a card worth stopping the table for in this
/// window. A heal can wait for their own turn, so a hand of heals alone
/// does not hold up a rival's move; with a card aimed at them any answer
/// counts, a heal before the blow included. They may still play a heal in
/// a window that waits on them for something else.
pub fn worth_answering(g: &necromy_rules::Game, human: PlayerId, kind: WindowKind) -> bool {
    // Tribute: giving a card or taking Threat is always a choice.
    if matches!(kind, WindowKind::Tribute { .. }) {
        return !g.hand(human).is_empty();
    }
    let aimed_at_me = matches!(kind, WindowKind::Target { target, .. } if target == human);
    g.playable(human)
        .into_iter()
        .any(|card| aimed_at_me || !matches!(g.def(card).effect, necromy_rules::Effect::Heal(_)))
}

/// Forget an aimed card that can no longer be played.
fn drop_stale_selection(game: Res<Match>, mut selection: ResMut<Selection>) {
    if selection.sift.is_some() && !game.game.may_cycle(game.human) {
        selection.sift = None;
    }
    if !selection.burn.is_empty() && game.game.battle_dice(game.human).is_none() {
        selection.burn.clear();
    }
    if let Some(card) = selection.card
        && game.game.can_play_now(game.human, card).is_err()
    {
        selection.card = None;
    }
}

/// Dev aid: autoplay leaves the human's wish to the wish panel, for
/// `NECROMY_SCREENSHOT_WHEN=wishpanel` and `NECROMY_WISH` (`wish_ui::dev_wish`).
fn wish_by_hand() -> bool {
    std::env::var("NECROMY_SCREENSHOT_WHEN").is_ok_and(|w| w == "wishpanel")
        || std::env::var("NECROMY_WISH").is_ok()
}

/// Time passes at the table and what it sends is shown. The table hears when
/// the screen has caught up (tokens walked, dice rolled), so bots do not run
/// ahead of what the human could see.
fn drive_table(
    time: Res<Time>,
    mut game: ResMut<Match>,
    dice: Res<DiceShow>,
    tokens: Query<&Token>,
) {
    // Only a message from the table is a change worth redrawing for.
    let m = game.bypass_change_detection();
    let human = m.human;
    let idle = !dice.busy() && !tokens.iter().any(Token::is_walking);
    if idle && m.shown < m.serial {
        m.shown = m.serial;
        let serial = m.serial;
        m.link.submit(human, ToTable::Shown(serial));
    }
    m.link.tick(time.delta_secs());
    if let (Link::Local(table), Some(saver)) = (&mut m.link, m.saver.as_mut()) {
        if let Err(err) = saver.persist(table) {
            warn!("the match is no longer saved: {err}");
            m.saver = None;
        } else if table.game().winner().is_some()
            && let Some(saver) = m.saver.take()
        {
            // Over: nothing to come back to.
            let _ = saver.discard();
        }
    }
    m.clock_since += time.delta_secs();
    let messages = m.link.drain(human);
    if !messages.is_empty() {
        game.receive(messages);
    }
}

impl Match {
    /// What an event did to the human, while a card aimed at them resolves.
    fn outcome_line(&self, event: &Event, hit: &Option<IncomingResult>) -> Option<String> {
        let hit = hit.as_ref()?;
        let me = self.human;
        Some(match event {
            Event::Damaged { player, amount, .. } if *player == me => format!("−{amount} здоровья"),
            Event::Healed { player, amount, .. } if *player == me => format!("+{amount} здоровья"),
            Event::Blocked { player, .. } if *player == me => "оберег удержал удар".into(),
            Event::WardBroken { player, .. } if *player == me => "оберег сломан".into(),
            Event::WardRaised { player, .. } if *player == me => "на тебе оберег".into(),
            Event::Rooted { player } if *player == me => "ты скован".into(),
            Event::Poisoned { player, stacks, .. } if *player == me => format!("яд ({stacks})"),
            Event::PoisonFed { player, .. } if *player == me => "лечение кормит яд".into(),
            Event::PoisonCured { player, .. } if *player == me => "яд снят".into(),
            Event::Canceled { card, .. } if *card == hit.card => "карта погашена".into(),
            Event::CancelFailed { card, .. } if *card == hit.card => "погасить не вышло".into(),
            Event::Fizzled { card } if *card == hit.card => "карта ушла впустую".into(),
            Event::ChampionFell { player, .. } if *player == me => {
                "ты пал и просыпаешься дома".into()
            }
            _ => return None,
        })
    }
}

/// Dev aid: the bot plays the human's seat at a server (`NECROMY_AUTOPLAY`),
/// at the pace the table's own bots keep.
fn remote_autoplay(time: Res<Time>, mut next: Local<f32>, mut game: ResMut<Match>) {
    if !game.autoplay || time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 0.35;
    // The answer redraws when it comes (`drive_table`), not the asking.
    game.bypass_change_detection().play_for_human();
}

/// Dev aid: `NECROMY_WATCH=all` puts every battle and trial on screen, as
/// before shows could wait behind their icons (screenshots of rivals' fights).
fn watch_all() -> bool {
    std::env::var("NECROMY_WATCH").is_ok_and(|v| v == "all")
}
