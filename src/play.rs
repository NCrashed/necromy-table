//! Runs a local match: one human seat, bots on the rest (docs/design.md §19).
//!
//! The client only sends intents to the rules core and redraws from its
//! state. Later the same intents go to the dedicated server instead (§17).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use necromy_rules::{
    CardId, Event, Game, God, Intent, PlayerId, RuleError, Setup, Target, TimeOfDay, WindowKind,
    bot,
};

use crate::board::Board;
use crate::names;
use crate::token::Token;

/// Seat the human plays until there is a champion select screen.
const HUMAN_GOD: God = God::Trishna;
const BOT_STEP_SECS: f32 = 0.35;
const FEED_LINES: usize = 12;

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
            .insert_resource(BotClock(Timer::from_seconds(
                BOT_STEP_SECS,
                TimerMode::Repeating,
            )))
            .add_systems(
                Update,
                (click_board, keys, auto_pass, run_bots, drop_stale_selection),
            );
    }
}

#[derive(Resource)]
pub struct Match {
    pub game: Game,
    pub human: PlayerId,
    /// Accepted steps not yet picked up by the token animation.
    pub steps: Vec<(PlayerId, necromy_rules::Hex)>,
    pub feed: Vec<String>,
    /// Dev aid (`NECROMY_AUTOPLAY`): a bot plays the human seat too.
    pub autoplay: bool,
}

/// The card the human picked and is now aiming.
#[derive(Resource, Default)]
pub struct Selection {
    pub card: Option<CardId>,
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
        let (game, events) = Game::new(Setup { seed, champions });
        let mut m = Match {
            game,
            human,
            steps: Vec::new(),
            feed: Vec::new(),
            autoplay: std::env::var_os("NECROMY_AUTOPLAY").is_some(),
        };
        m.record(&events);
        m
    }

    /// The human may walk or end the turn.
    pub fn is_human_turn(&self) -> bool {
        self.game.window().is_none() && self.game.current_player() == self.human
    }

    /// The game is waiting on the human, on their turn or in a window.
    pub fn human_awaited(&self) -> bool {
        self.game.awaiting().contains(&self.human)
    }

    pub fn act(&mut self, player: PlayerId, intent: Intent) -> Result<(), RuleError> {
        let events = self.game.apply(player, intent)?;
        self.record(&events);
        Ok(())
    }

    fn record(&mut self, events: &[Event]) {
        for event in events {
            match event {
                Event::Moved { player, to, .. } => self.steps.push((*player, *to)),
                Event::Blinked { player, to, .. } => self.steps.push((*player, *to)),
                Event::ChampionFell {
                    player, respawn, ..
                } => self.steps.push((*player, *respawn)),
                _ => {}
            }
            if let Some(line) = self.describe(event) {
                self.feed.push(line);
            }
        }
        let excess = self.feed.len().saturating_sub(FEED_LINES);
        self.feed.drain(..excess);
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
            Event::Dawn { .. } => "Рассвет. (Здесь будут желания.)".into(),
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
        WindowKind::End { player } => format!("{} заканчивает ход", m.name(player)),
    }
}

/// Plays `card` for the human with the only sensible target, or starts
/// aiming it. Called by the hand UI.
pub fn pick_card(m: &mut Match, selection: &mut Selection, card: CardId) {
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
    camera: Single<(&Camera, &GlobalTransform)>,
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
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let (camera, cam_transform) = *camera;
    let Ok(ray) = camera.viewport_to_world(cam_transform, cursor) else {
        return;
    };
    let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
        return;
    };
    let hex = board.world_to_hex(ray.get_point(dist));

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
    if keys.just_pressed(KeyCode::Escape) {
        selection.card = None;
    }
    let human = game.human;
    if keys.any_just_pressed([KeyCode::Space, KeyCode::Enter])
        && game.is_human_turn()
        && let Err(err) = game.act(human, Intent::EndTurn)
    {
        warn!("end turn rejected: {err}");
    }
    if keys.just_pressed(KeyCode::KeyP) && game.game.window().is_some() && game.human_awaited() {
        selection.card = None;
        if let Err(err) = game.act(human, Intent::Pass) {
            warn!("pass rejected: {err}");
        }
    }
}

/// With nothing to answer, the human passes at once. The rules still see an
/// ordinary pass; on a server the window timer hides who had cards (§11.3).
fn auto_pass(mut game: ResMut<Match>) {
    if game.game.window().is_none() || !game.human_awaited() {
        return;
    }
    if game.game.playable(game.human).is_empty() {
        let human = game.human;
        let _ = game.act(human, Intent::Pass);
    }
}

/// Forget an aimed card that can no longer be played.
fn drop_stale_selection(game: Res<Match>, mut selection: ResMut<Selection>) {
    if let Some(card) = selection.card
        && game.game.can_play_now(game.human, card).is_err()
    {
        selection.card = None;
    }
}

#[derive(Resource)]
struct BotClock(Timer);

fn run_bots(
    time: Res<Time>,
    mut clock: ResMut<BotClock>,
    mut game: ResMut<Match>,
    tokens: Query<&Token>,
) {
    if tokens.iter().any(Token::is_walking) {
        return;
    }
    let human = game.human;
    let autoplay = game.autoplay;
    let Some(player) = game
        .game
        .awaiting()
        .into_iter()
        .find(|&p| autoplay || p != human)
    else {
        return;
    };
    if !clock.0.tick(time.delta()).just_finished() {
        return;
    }
    let intent = bot::choose(&game.game, player);
    if let Err(err) = game.act(player, intent) {
        // A bot that cannot act must not stall the table.
        warn!("bot {player:?} {intent:?} rejected: {err}");
        let fallback = if game.game.window().is_some() {
            Intent::Pass
        } else {
            Intent::EndTurn
        };
        let _ = game.act(player, fallback);
    }
}
