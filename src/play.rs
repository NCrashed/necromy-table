//! Runs a local match: one human seat, bots on the rest (docs/design.md §19).
//!
//! The match lives on a table (`necromy-host`) in this same process, as it
//! will on the dedicated server (§17.3): the client sends it intents and
//! redraws from the view it sends back, which holds only what the human may
//! know. Intents are checked against that view first, so a refusal comes at
//! once and in words.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use necromy_host::{Config, FromTable, OracleNews, Seat, Table, ToTable};
use necromy_rules::{
    CardId, Event, Fighter, Game, God, Intent, PlayerId, RuleError, Score, Target, Terrain,
    TimeOfDay, WindowKind,
};

use crate::board::{self, Board};
use crate::dice::DiceShow;
use crate::names;
use crate::token::Token;

/// Seat the human plays until there is a champion select screen.
const HUMAN_GOD: God = God::Trishna;
const FEED_LINES: usize = 9;

pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        let seed = std::env::var("NECROMY_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64)
            });
        info!("match seed {seed} (set NECROMY_SEED to replay it)");

        app.insert_resource(Match::new(seed))
            .init_resource::<Selection>()
            .init_resource::<IncomingCountdown>()
            .add_systems(
                Update,
                (
                    drive_table,
                    click_board,
                    keys,
                    auto_pass,
                    drop_stale_selection,
                ),
            );
    }
}

#[derive(Resource)]
pub struct Match {
    /// The match as the human sees it (`Game::view_for`), after the last update.
    pub game: Game,
    table: Table,
    /// The table's last update, and the last one the screen has shown.
    serial: u32,
    shown: u32,
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
    /// The last card that hit the human and what it did, shown for a moment
    /// after its Target window closed.
    pub incoming_result: Option<IncomingResult>,
    /// Bumped for every new `incoming_result`.
    pub incoming_serial: u32,
    /// The last wish and the god's answer, shown for a moment (§7).
    pub wish_reply: Option<WishReply>,
    /// Bumped for every new `wish_reply`.
    pub wish_serial: u32,
    /// The last story line told to the human, for its voice popup (§8).
    pub told: Option<necromy_rules::Line>,
    pub told_serial: u32,
    /// Feed lines that would spoil dice still rolling on screen.
    held: Vec<String>,
    holding: bool,
}

/// What the table told about the gods' voice.
#[derive(Default)]
pub struct OracleState {
    /// The model answers: wishes can be written in free words.
    pub online: bool,
    /// The god thinking about the human's wish.
    pub listening: Option<God>,
    /// Why the last free-words wish was not heard.
    pub failed: Option<String>,
    /// The model's words for wishes, by the table's update serial.
    pub wish_voices: HashMap<u32, String>,
    /// The model's words for story lines, by line id.
    pub line_voices: HashMap<u32, String>,
}

/// A wish and what came of it.
pub struct WishReply {
    pub player: PlayerId,
    /// The table's update it came in; its god's words are keyed by it.
    pub serial: u32,
    /// `None` when the Dominant refused to wish.
    pub wish: Option<(God, necromy_rules::WishKind, u8)>,
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

/// What the battle panel shows about the current fight.
pub struct BattleInfo {
    /// `None` for the royal guard.
    pub attacker: Option<PlayerId>,
    pub defender: PlayerId,
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
    fn new(attacker: Option<PlayerId>, defender: PlayerId) -> Self {
        BattleInfo {
            attacker,
            defender,
            burned: [Vec::new(), Vec::new()],
            scores: None,
            hp: [None, None],
            fell: [false, false],
        }
    }

    /// 0 for the attacker, 1 for the defender, `None` for bystanders.
    fn side_of(&self, player: PlayerId) -> Option<usize> {
        if player == self.defender {
            Some(1)
        } else if Some(player) == self.attacker {
            Some(0)
        } else {
            None
        }
    }

    fn side(&self, player: PlayerId) -> usize {
        usize::from(player == self.defender)
    }
}

pub struct ThrowView {
    /// 0 for the attacker's tray, 1 for the defender's.
    pub side: usize,
    pub seed: u64,
    pub count: u8,
    pub faces: Vec<necromy_rules::Face>,
}

/// The card the human picked and is now aiming.
#[derive(Resource, Default)]
pub struct Selection {
    pub card: Option<CardId>,
    /// Cards marked to burn in the open Battle window.
    pub burn: Vec<CardId>,
}

impl Match {
    fn new(seed: u64) -> Self {
        let champions = God::ALL.to_vec();
        let human = PlayerId(
            champions
                .iter()
                .position(|&g| g == HUMAN_GOD)
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
        });
        let first = table.drain(human);
        let Some(FromTable::Update { view, .. }) = first.first() else {
            unreachable!("a table greets every watched seat with its view");
        };
        let mut m = Match {
            game: (**view).clone(),
            table,
            serial: 0,
            shown: 0,
            oracle: OracleState::default(),
            human,
            steps: Vec::new(),
            feed: Vec::new(),
            autoplay,
            throws: Vec::new(),
            battle: None,
            incoming_result: None,
            incoming_serial: 0,
            wish_reply: None,
            wish_serial: 0,
            told: None,
            told_serial: 0,
            held: Vec::new(),
            holding: false,
        };
        m.receive(first);
        m
    }

    /// The human may walk or end the turn.
    pub fn is_human_turn(&self) -> bool {
        self.game.window().is_none() && self.game.current_player() == self.human
    }

    /// Dev aid: with autoplay, `NECROMY_SCREENSHOT_WHEN=wishpanel` stops at the
    /// human's wish so its panel can be captured, and `NECROMY_WISH` stops
    /// there so the wish can be written (`wish_ui::dev_wish`).
    pub fn paused_for_wish_panel(&self) -> bool {
        self.game.wish_due() == Some(self.human) && wish_by_hand()
    }

    /// The game is waiting on the human, on their turn or in a window.
    pub fn human_awaited(&self) -> bool {
        self.game.awaiting().contains(&self.human)
    }

    /// Send the human's intent to the table. It is checked against the view
    /// first; the table's answer arrives before this returns (the table is
    /// local), so the view is already up to date.
    pub fn act(&mut self, player: PlayerId, intent: Intent) -> Result<(), RuleError> {
        debug_assert_eq!(player, self.human, "the client acts only for its seat");
        self.game.clone().apply(player, intent.clone())?;
        self.table.submit(player, ToTable::Act(intent));
        self.pump();
        Ok(())
    }

    /// Hand the human's wish in free words to `god`; the table asks the model.
    pub fn wish_in_words(&mut self, god: God, text: &str) {
        self.oracle.failed = None;
        let human = self.human;
        self.table.submit(
            human,
            ToTable::Wish {
                god,
                text: text.to_string(),
            },
        );
        self.pump();
    }

    fn pump(&mut self) {
        let messages = self.table.drain(self.human);
        self.receive(messages);
    }

    fn receive(&mut self, messages: Vec<FromTable>) {
        for message in messages {
            match message {
                FromTable::Update {
                    serial,
                    events,
                    view,
                } => {
                    self.game = *view;
                    self.serial = serial;
                    self.record(&events);
                }
                FromTable::Rejected(err) => self.feed.push(format!("Нельзя: {}.", reason(err))),
                FromTable::Oracle(news) => match news {
                    OracleNews::Online(online) => self.oracle.online = online,
                    OracleNews::Listening(god) => self.oracle.listening = god,
                    OracleNews::NotHeard(why) => self.oracle.failed = Some(why),
                    OracleNews::WishVoice { serial, text } => {
                        self.oracle.wish_voices.insert(serial, text);
                    }
                    OracleNews::LineVoice { line, text } => {
                        self.oracle.line_voices.insert(line, text);
                    }
                },
            }
        }
    }

    fn record(&mut self, events: &[Event]) {
        // A wish in this batch, and the lines of what it did.
        let mut wished: Option<WishReply> = None;
        // Outcome of a card aimed at the human, gathered from this batch.
        let mut hit: Option<IncomingResult> = None;
        for event in events {
            if let Some(w) = wished.as_mut()
                && let Some(line) = self.describe(event)
            {
                w.lines.push(line);
            }
            match event {
                Event::WishGranted {
                    player,
                    god,
                    kind,
                    grade,
                    said,
                    ..
                } => {
                    wished = Some(WishReply {
                        player: *player,
                        serial: self.serial,
                        wish: Some((*god, *kind, *grade)),
                        said: said.clone(),
                        lines: Vec::new(),
                    });
                }
                Event::WishRefused { player } => {
                    wished = Some(WishReply {
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
            match event {
                Event::Moved { player, to, .. } => self.steps.push((*player, *to)),
                Event::Blinked { player, to, .. } => self.steps.push((*player, *to)),
                Event::ChampionFell {
                    player, respawn, ..
                } => {
                    self.steps.push((*player, *respawn));
                    if let Some(b) = self.battle.as_mut().filter(|b| b.scores.is_some())
                        && let Some(side) = b.side_of(*player)
                    {
                        b.fell[side] = true;
                    }
                }
                Event::BattleStarted { attacker, defender } => {
                    self.battle = Some(BattleInfo::new(Some(*attacker), *defender));
                }
                Event::GuardStruck { target } => {
                    self.battle = Some(BattleInfo::new(None, *target));
                }
                Event::Burned { player, faces, .. } => {
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
                    if let Some(b) = self.battle.as_mut() {
                        b.scores = Some((*guard_score, *target_score));
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
                    let defender = self.battle.as_ref().map(|b| b.defender);
                    let side = usize::from(
                        matches!(fighter, Fighter::Champion(p) if Some(*p) == defender),
                    );
                    self.throws.push(ThrowView {
                        side,
                        seed: *seed,
                        count: *count,
                        faces: faces.clone(),
                    });
                    self.holding = true;
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
        if let Some(w) = wished {
            self.wish_reply = Some(w);
            self.wish_serial += 1;
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

    /// The battle panel closes: the fight is told in the feed.
    pub fn end_battle_view(&mut self) {
        self.battle = None;
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
                format!("— раунд {round}: {} —", time_name(*time))
            }
            Event::Dawn { .. } => "Рассвет.".into(),
            Event::Dusk { .. } => "Закат. (Здесь проснутся боги.)".into(),
            Event::TurnStarted { player, .. } => format!("Ход: {}", self.name(*player)),
            Event::CorpseAppeared { .. } => "На доске появилось тело.".into(),
            Event::CorpseDecayed { .. } => "Тело истлело.".into(),
            Event::GroveGrew { .. } => "Выросла роща.".into(),
            Event::CardDrawn { player, card, .. } if *player == me => {
                format!("Ты берёшь «{}».", self.card_name(*card))
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
            // End windows open every turn; the status line covers them.
            Event::WindowOpened { kind, eligible }
                if eligible.contains(&me) && !matches!(kind, WindowKind::End { .. }) =>
            {
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
            Event::WishDue { player } => format!("{} загадывает желание…", self.name(*player)),
            Event::WishRefused { player } => {
                format!("{} отказывается от желания.", self.name(*player))
            }
            Event::WishGranted {
                player,
                god,
                kind,
                grade,
                ..
            } => format!(
                "{} просит {}: «{}». Оценка {grade}/3.",
                self.name(*player),
                names::god_accusative(*god),
                names::wish(*kind)
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
            Event::Victory { player, condition } => {
                format!(
                    "Победа: {} — {}.",
                    self.name(*player),
                    names::condition(*condition).0
                )
            }
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
                "{} переходит в стадию «{}».",
                names::god(*god),
                names::stage(*god, *stage)
            ),
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
        WindowKind::End { player } => format!("{} заканчивает ход", m.name(player)),
    }
}

/// Plays `card` for the human with the only sensible target, or starts
/// aiming it. Called by the hand UI.
pub fn pick_card(m: &mut Match, selection: &mut Selection, card: CardId) {
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
    let radius = game.game.board().radius();
    let Some(hex) = board::cursor_hex(&window, camera, cam_transform, &board, radius) else {
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

    if game.is_human_turn() && game.game.attackable().contains(&hex) {
        let human = game.human;
        if let Err(err) = game.act(human, Intent::Move { to: hex }) {
            warn!("attack rejected: {err}");
        }
        return;
    }
    if !game.is_human_turn() {
        return;
    }
    let Some(path) = game.game.path_to(hex) else {
        return;
    };
    // One intent per step, as a server would receive them. A step may open
    // a window; the rest of the path waits until the human clicks again.
    let human = game.human;
    for step in path {
        if let Err(err) = game.act(human, Intent::Move { to: step }) {
            warn!("move rejected: {err}");
            break;
        }
        if game.game.window().is_some() {
            break;
        }
    }
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut game: ResMut<Match>,
    mut selection: ResMut<Selection>,
) {
    // While the human writes a wish, the keyboard is the wish's.
    if game.game.wish_due() == Some(game.human) {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        selection.card = None;
    }
    let human = game.human;
    // Space with a card aimed at you: go on without waiting (a pass).
    let aimed_at_me = matches!(
        game.game.window().map(|w| w.kind),
        Some(WindowKind::Target { target, .. }) if target == human
    );
    if keys.just_pressed(KeyCode::Space) && aimed_at_me && game.human_awaited() {
        selection.card = None;
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
    if keys.just_pressed(KeyCode::KeyP) && game.game.window().is_some() && game.human_awaited() {
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
    let Some(kind) = game.game.window().map(|w| w.kind) else {
        *seen = None;
        countdown.set_if_neq(IncomingCountdown(None));
        return;
    };
    if !game.human_awaited() {
        countdown.set_if_neq(IncomingCountdown(None));
        return;
    }
    let now = time.elapsed_secs();
    let opened = match *seen {
        Some((k, at)) if k == kind => at,
        _ => {
            *seen = Some((kind, now));
            now
        }
    };
    let aimed_at_me = matches!(kind, WindowKind::Target { target, .. } if target == game.human);
    let nothing_to_answer = game.game.playable(game.human).is_empty();
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
    if game.game.playable(game.human).is_empty() && !game.autoplay {
        let human = game.human;
        let _ = game.act(human, Intent::Pass);
    }
}

/// Forget an aimed card that can no longer be played.
fn drop_stale_selection(game: Res<Match>, mut selection: ResMut<Selection>) {
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
        m.table.submit(human, ToTable::Shown(serial));
    }
    m.table.tick(time.delta_secs());
    let messages = m.table.drain(human);
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
