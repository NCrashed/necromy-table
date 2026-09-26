//! Match state, intents and events (docs/design.md §11).
//!
//! A round is a day or a night. Inside it players take turns in initiative
//! order, then the world acts. Dawn opens a day, dusk closes it.
//!
//! Someone else's turn is a chain of reaction windows (§11.3). While a window
//! is open only its eligible players may act, each once: a card or a pass.
//! Choices stay hidden until the last one is in, then resolve together.
//! A response never opens another window.

use std::collections::{BTreeMap, BinaryHeap, HashMap};

use hexx::Hex;
use serde::{Deserialize, Serialize};

use necromy_dice::Face;

use crate::board::{Board, Corpse, GROVE_AGE, Terrain};
use crate::cards::{
    self, CardDef, CardId, CardKind, DefId, Effect, TargetRule, Timing, TrapEffect,
};
use crate::gods::{Element, God};
use crate::rng::Rng;

pub const MOVE_POINTS: u32 = 3;
/// Rivals this close to an event may react to it (§11.3).
pub const REACTION_RANGE: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeOfDay {
    Day,
    Night,
}

#[derive(Clone, Debug)]
pub struct Setup {
    pub seed: u64,
    /// One patron god per seat, 2–5 seats, no god twice.
    pub champions: Vec<God>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Champion {
    pub god: God,
    pub hex: Hex,
    pub might: u8,
    pub body: u8,
    pub wits: u8,
    pub spirit: u8,
    pub hp: u8,
    /// Spirit left to spend; refills by one each turn up to `spirit`.
    pub spirit_points: u8,
    /// Stops harmful cards until the owner's next turn, unless they are of
    /// the one element that quenches it (§4).
    pub ward: Option<Element>,
    /// Loses the movement of its next turn.
    pub rooted: bool,
}

impl Champion {
    fn new(god: God, hex: Hex) -> Self {
        // Placeholder stats: each patron leans one way (§13).
        let [might, body, wits, spirit] = match god {
            God::Bhava => [3, 5, 2, 3],
            God::Trishna => [4, 3, 3, 3],
            God::Zaga => [3, 4, 2, 4],
            God::Ahamar => [4, 4, 3, 2],
            God::Maya => [2, 3, 4, 4],
        };
        Champion {
            god,
            hex,
            might,
            body,
            wits,
            spirit,
            hp: body,
            spirit_points: spirit,
            ward: None,
            rooted: false,
        }
    }

    pub fn hand_limit(&self) -> usize {
        3 + self.wits as usize / 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    /// For cards that aim at the pending card, not at the board.
    None,
    Champion(PlayerId),
    Hex(Hex),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    /// Step onto an adjacent hex. One step per intent: every step may open
    /// an Enter window. Stepping onto a rival attacks them (§12.1).
    Move {
        to: Hex,
    },
    EndTurn,
    /// On your turn: play a card. In a window: your hidden choice.
    Play {
        card: CardId,
        target: Target,
    },
    /// In a Battle window: cards to burn for guaranteed faces. May be empty.
    Burn {
        cards: Vec<CardId>,
    },
    /// In a window: play nothing.
    Pass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowKind {
    /// A champion stepped onto `hex`.
    Enter { mover: PlayerId, hex: Hex },
    /// `caster` aimed `card` at `target`; it waits until the window closes.
    Target {
        caster: PlayerId,
        target: PlayerId,
        card: CardId,
    },
    /// Before the dice: both sides pick cards to burn (§12.1).
    Battle {
        attacker: PlayerId,
        defender: PlayerId,
    },
    /// `player` finished their turn.
    End { player: PlayerId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Choice {
    Pass,
    Play(CardId, Target),
    Burn(Vec<CardId>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub kind: WindowKind,
    /// In resolution order.
    pub eligible: Vec<PlayerId>,
    choices: BTreeMap<PlayerId, Choice>,
}

impl Window {
    pub fn has_chosen(&self, player: PlayerId) -> bool {
        self.choices.contains_key(&player)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pending {
    caster: PlayerId,
    card: CardId,
    target: Target,
    bonus: u8,
    canceled: bool,
}

/// Who throws dice in a battle: a champion or the royal guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fighter {
    Champion(PlayerId),
    Guard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trap {
    pub owner: PlayerId,
    pub hex: Hex,
    pub card: CardId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    RoundStarted {
        round: u32,
        time: TimeOfDay,
        order: Vec<PlayerId>,
    },
    /// Wishes hook in here (§7).
    Dawn {
        round: u32,
    },
    TurnStarted {
        player: PlayerId,
        move_points: u32,
    },
    Moved {
        player: PlayerId,
        from: Hex,
        to: Hex,
        cost: u32,
    },
    TurnEnded {
        player: PlayerId,
    },
    CorpseAppeared {
        hex: Hex,
    },
    CorpseDecayed {
        hex: Hex,
    },
    CorpseTaken {
        hex: Hex,
    },
    GroveGrew {
        hex: Hex,
    },
    /// God stages and the storyteller hook in here (§5, §8).
    Dusk {
        round: u32,
    },
    /// Something given to a god: by a player, or by the world (`None`).
    Offered {
        player: Option<PlayerId>,
        god: God,
        amount: u8,
    },
    StageChanged {
        god: God,
        stage: u8,
    },
    /// A settlement, temple or the Table changed hands.
    Claimed {
        player: PlayerId,
        hex: Hex,
        from: Option<PlayerId>,
    },
    StyleChanged {
        player: PlayerId,
        delta: i16,
        total: u16,
        reason: StyleReason,
    },
    ThreatChanged {
        player: PlayerId,
        delta: i8,
        total: u8,
    },
    /// Who wears the Crown after this dawn; `None` if nobody leads.
    Crowned {
        player: Option<PlayerId>,
    },
    GuardSpawned {
        hex: Hex,
        target: PlayerId,
    },
    GuardMoved {
        from: Hex,
        to: Hex,
    },
    GuardLeft {
        hex: Hex,
    },
    GuardStruck {
        target: PlayerId,
    },
    GuardResolved {
        target: PlayerId,
        guard_score: Score,
        target_score: Score,
    },
    /// The match is over.
    Victory {
        player: PlayerId,
        condition: victory::Condition,
    },

    // Hidden information below (draws, traps, choices) goes to every
    // listener for now; the server will filter it per player (§17.1).
    CardDrawn {
        player: PlayerId,
        card: CardId,
        def: DefId,
    },
    DeckReshuffled,
    CardPlayed {
        player: PlayerId,
        card: CardId,
        def: DefId,
        target: Target,
        response: bool,
    },
    /// A card of the element generated by the previous one this turn.
    Chain {
        player: PlayerId,
        from: Element,
        to: Element,
        rhythm_broken: bool,
    },
    WindowOpened {
        kind: WindowKind,
        eligible: Vec<PlayerId>,
    },
    /// Someone locked in a choice; what it was stays hidden.
    ChoiceMade {
        player: PlayerId,
    },
    WindowClosed {
        kind: WindowKind,
        played: Vec<PlayerId>,
    },
    Canceled {
        card: CardId,
        by: CardId,
    },
    CancelFailed {
        card: CardId,
        by: CardId,
    },
    /// The card had no legal target left when it resolved.
    Fizzled {
        card: CardId,
    },
    Damaged {
        player: PlayerId,
        amount: u8,
        hp: u8,
    },
    Healed {
        player: PlayerId,
        amount: u8,
        hp: u8,
    },
    SpiritChanged {
        player: PlayerId,
        spirit: u8,
    },
    WardRaised {
        player: PlayerId,
        element: Element,
    },
    WardBroken {
        player: PlayerId,
        ward: Element,
        by: Element,
    },
    WardFaded {
        player: PlayerId,
    },
    Blocked {
        player: PlayerId,
        ward: Element,
    },
    Rooted {
        player: PlayerId,
    },
    Hasted {
        player: PlayerId,
        move_points: u32,
    },
    Blinked {
        player: PlayerId,
        from: Hex,
        to: Hex,
    },
    TrapSet {
        player: PlayerId,
        hex: Hex,
    },
    TrapSprung {
        owner: PlayerId,
        victim: PlayerId,
        hex: Hex,
        def: DefId,
    },
    ChampionFell {
        player: PlayerId,
        at: Hex,
        respawn: Hex,
    },
    BattleStarted {
        attacker: PlayerId,
        defender: PlayerId,
    },
    /// Cards burned for guaranteed faces, revealed when the Battle window closes.
    Burned {
        player: PlayerId,
        cards: Vec<CardId>,
        faces: Vec<Face>,
    },
    /// One physical throw. Clients replay it with `necromy_dice::throw(seed, count)`
    /// and must land on `faces` (§12.2).
    DiceThrown {
        fighter: Fighter,
        seed: u64,
        count: u8,
        faces: Vec<Face>,
    },
    BattleResolved {
        attacker: PlayerId,
        defender: PlayerId,
        /// Hits and shields of the attacker, then of the defender.
        attacker_score: Score,
        defender_score: Score,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleError {
    NotYourTurn,
    UnknownPlayer,
    OffBoard,
    NotAdjacent,
    Occupied,
    NotEnoughMovePoints {
        need: u32,
        have: u32,
    },
    /// Only window choices are accepted until it closes.
    WindowOpen,
    NoWindow,
    /// Someone has won; the match takes no more intents.
    GameOver,
    AlreadyChose,
    NotInHand,
    WrongTiming,
    NotEnoughSpirit {
        need: u8,
        have: u8,
    },
    InvalidTarget,
    /// Burned more cards than the side has dice.
    TooManyBurned {
        max: u8,
    },
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::NotYourTurn => write!(f, "not your turn"),
            RuleError::UnknownPlayer => write!(f, "unknown player"),
            RuleError::OffBoard => write!(f, "off the board"),
            RuleError::NotAdjacent => write!(f, "not an adjacent hex"),
            RuleError::Occupied => write!(f, "hex is occupied"),
            RuleError::NotEnoughMovePoints { need, have } => {
                write!(f, "needs {need} move points, {have} left")
            }
            RuleError::WindowOpen => write!(f, "a reaction window is open"),
            RuleError::NoWindow => write!(f, "no reaction window to pass in"),
            RuleError::GameOver => write!(f, "the match is over"),
            RuleError::AlreadyChose => write!(f, "already chose in this window"),
            RuleError::NotInHand => write!(f, "card is not in hand"),
            RuleError::WrongTiming => write!(f, "card cannot be played now"),
            RuleError::NotEnoughSpirit { need, have } => {
                write!(f, "needs {need} spirit, {have} left")
            }
            RuleError::InvalidTarget => write!(f, "invalid target"),
            RuleError::TooManyBurned { max } => write!(f, "can burn at most {max} cards"),
        }
    }
}

impl std::error::Error for RuleError {}

#[derive(Clone, Debug)]
pub struct Game {
    seed: u64,
    rng: Rng,
    board: Board,
    champions: Vec<Champion>,
    round: u32,
    time: TimeOfDay,
    order: Vec<PlayerId>,
    turn: usize,
    move_points: u32,
    /// This match's slice of the pool.
    slice: Vec<DefId>,
    /// Card instance → definition.
    defs: Vec<DefId>,
    deck: Vec<CardId>,
    discard: Vec<CardId>,
    hands: Vec<Vec<CardId>>,
    traps: Vec<Trap>,
    window: Option<Window>,
    pending: Option<Pending>,
    /// Element of the last card the active player played this turn.
    last_element: Option<Element>,
    /// The active player has ended their turn; only the End window is left.
    turn_over: bool,
    /// Battles fought so far; part of every throw's seed.
    battles: u64,
    pantheon: world::Pantheon,
    /// Per player, per god (`God::index`).
    favor: Vec<[u16; 5]>,
    style: Vec<u16>,
    threat: Vec<u8>,
    dominant: Option<PlayerId>,
    /// Settlements, temples and the Table, keyed by axial coordinates.
    claims: BTreeMap<(i32, i32), PlayerId>,
    taste: style::Taste,
    /// Per player, what they did since the last dusk.
    deeds: Vec<Vec<style::Deed>>,
    guard: Option<guard::Guard>,
    /// Victory conditions (§10): open to all, and one secret per player.
    open: Vec<victory::Condition>,
    secrets: Vec<victory::Condition>,
    progress: Vec<victory::Progress>,
    winner: Option<(PlayerId, victory::Condition)>,
    log: Vec<Event>,
}

impl Game {
    pub fn new(setup: Setup) -> (Self, Vec<Event>) {
        assert!(
            (2..=5).contains(&setup.champions.len()),
            "2–5 champions, got {}",
            setup.champions.len()
        );
        let mut rng = Rng::new(setup.seed);
        let board = Board::generate(&mut rng);
        let champions: Vec<Champion> = setup
            .champions
            .iter()
            .map(|&god| Champion::new(god, board.start_of(god)))
            .collect();

        let mut order: Vec<PlayerId> = (0..champions.len() as u8).map(PlayerId).collect();
        rng.shuffle(&mut order);

        // Gods start light or mid, never dark (§14).
        let stages = God::ALL.map(|_| rng.below(2) as u8);
        let taste = style::Taste::draw(&mut rng);
        let (open, secrets) = victory::draw(&mut rng, setup.champions.len());
        let slice = cards::match_slice(&mut rng);
        let defs: Vec<DefId> = slice
            .iter()
            .flat_map(|&d| std::iter::repeat_n(d, cards::COPIES as usize))
            .collect();
        let mut deck: Vec<CardId> = (0..defs.len() as u32).map(CardId).collect();
        rng.shuffle(&mut deck);

        let hands = vec![Vec::new(); champions.len()];
        let champions_len = champions.len();
        let mut game = Game {
            seed: setup.seed,
            rng,
            board,
            champions,
            round: 0,
            time: TimeOfDay::Night,
            order,
            turn: 0,
            move_points: 0,
            slice,
            defs,
            deck,
            discard: Vec::new(),
            hands,
            traps: Vec::new(),
            window: None,
            pending: None,
            last_element: None,
            turn_over: false,
            battles: 0,
            pantheon: world::Pantheon {
                stages,
                pressure: [0; 5],
            },
            favor: vec![[0; 5]; champions_len],
            style: vec![0; champions_len],
            threat: vec![0; champions_len],
            dominant: None,
            claims: BTreeMap::new(),
            taste,
            deeds: vec![Vec::new(); champions_len],
            guard: None,
            open,
            secrets,
            progress: vec![victory::Progress::default(); champions_len],
            winner: None,
            log: Vec::new(),
        };

        let mut events = Vec::new();
        for _ in 0..5 {
            game.spawn_corpse(&mut events);
        }
        for player in game.players().collect::<Vec<_>>() {
            game.refill_hand(player, &mut events);
        }
        game.start_round(&mut events);
        game.log.extend(events.iter().cloned());
        (game, events)
    }

    // ---- Reading the state ----

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn champions(&self) -> &[Champion] {
        &self.champions
    }

    pub fn champion(&self, player: PlayerId) -> Option<&Champion> {
        self.champions.get(player.0 as usize)
    }

    pub fn players(&self) -> impl Iterator<Item = PlayerId> {
        (0..self.champions.len() as u8).map(PlayerId)
    }

    pub fn round(&self) -> u32 {
        self.round
    }

    pub fn time(&self) -> TimeOfDay {
        self.time
    }

    pub fn order(&self) -> &[PlayerId] {
        &self.order
    }

    /// Whose turn it is. During a window others may still have to act first:
    /// see [`Game::awaiting`].
    pub fn current_player(&self) -> PlayerId {
        self.order[self.turn]
    }

    /// `player` is mid-turn and may still move. False between turns, in the End
    /// window and during the world phase.
    fn is_active(&self, player: PlayerId) -> bool {
        !self.turn_over && self.order.get(self.turn) == Some(&player)
    }

    pub fn move_points(&self) -> u32 {
        self.move_points
    }

    pub fn window(&self) -> Option<&Window> {
        self.window.as_ref()
    }

    /// Chain bonus (§4) of the card waiting in an open Target window.
    pub fn pending_bonus(&self) -> Option<u8> {
        self.pending.map(|p| p.bonus)
    }

    /// Players the game is waiting on right now.
    pub fn awaiting(&self) -> Vec<PlayerId> {
        match &self.window {
            Some(w) => w
                .eligible
                .iter()
                .copied()
                .filter(|p| !w.has_chosen(*p))
                .collect(),
            None => vec![self.current_player()],
        }
    }

    pub fn hand(&self, player: PlayerId) -> &[CardId] {
        self.hands
            .get(player.0 as usize)
            .map_or(&[][..], Vec::as_slice)
    }

    pub fn def_id(&self, card: CardId) -> DefId {
        self.defs[card.0 as usize]
    }

    pub fn def(&self, card: CardId) -> &'static CardDef {
        self.def_id(card).def()
    }

    pub fn slice(&self) -> &[DefId] {
        &self.slice
    }

    pub fn traps(&self) -> &[Trap] {
        &self.traps
    }

    pub fn deck_len(&self) -> usize {
        self.deck.len()
    }

    /// Every event since the match began, in order.
    pub fn log(&self) -> &[Event] {
        &self.log
    }

    pub fn occupant(&self, hex: Hex) -> Option<PlayerId> {
        self.champions
            .iter()
            .position(|c| c.hex == hex)
            .map(|i| PlayerId(i as u8))
    }

    fn hex_of(&self, player: PlayerId) -> Hex {
        self.champions[player.0 as usize].hex
    }

    fn champ_mut(&mut self, player: PlayerId) -> &mut Champion {
        &mut self.champions[player.0 as usize]
    }

    /// Players after `first` in initiative order, wrapping around.
    fn initiative_after(&self, first: PlayerId) -> Vec<PlayerId> {
        let at = self.order.iter().position(|&p| p == first).unwrap_or(0);
        (1..self.order.len())
            .map(|i| self.order[(at + i) % self.order.len()])
            .collect()
    }

    // ---- Movement ----

    /// Cost to step from a champion's hex onto `to`, if it is allowed at all.
    pub fn step_cost(&self, player: PlayerId, to: Hex) -> Result<u32, RuleError> {
        let champion = self.champion(player).ok_or(RuleError::UnknownPlayer)?;
        let tile = self.board.tile(to).ok_or(RuleError::OffBoard)?;
        if champion.hex.unsigned_distance_to(to) != 1 {
            return Err(RuleError::NotAdjacent);
        }
        if self.occupant(to).is_some() || self.guard_at(to) {
            return Err(RuleError::Occupied);
        }
        Ok(tile.terrain.move_cost())
    }

    /// Hexes the current player can reach this turn, with the cheapest cost.
    pub fn reachable(&self) -> HashMap<Hex, u32> {
        self.paths().into_iter().map(|(h, (c, _))| (h, c)).collect()
    }

    /// Cheapest path for the current player to `to`, excluding the start hex.
    pub fn path_to(&self, to: Hex) -> Option<Vec<Hex>> {
        let paths = self.paths();
        paths.get(&to)?;
        let start = self.hex_of(self.current_player());
        let mut path = vec![to];
        let mut at = to;
        while let Some(&(_, prev)) = paths.get(&at) {
            if prev == start {
                break;
            }
            path.push(prev);
            at = prev;
        }
        path.reverse();
        Some(path)
    }

    /// Dijkstra over move costs within the remaining move points:
    /// hex → (cost, previous hex). Empty while a window is open.
    fn paths(&self) -> HashMap<Hex, (u32, Hex)> {
        let start = self.hex_of(self.current_player());
        let mut best: HashMap<Hex, (u32, Hex)> = HashMap::new();
        if self.window.is_some() {
            return best;
        }
        let mut queue = BinaryHeap::new();
        queue.push(std::cmp::Reverse((0u32, start.x(), start.y())));
        while let Some(std::cmp::Reverse((cost, x, y))) = queue.pop() {
            let at = Hex::new(x, y);
            if at != start && best.get(&at).is_some_and(|&(c, _)| c < cost) {
                continue;
            }
            for next in at.all_neighbors() {
                let Some(tile) = self.board.tile(next) else {
                    continue;
                };
                if next == start || self.occupant(next).is_some() || self.guard_at(next) {
                    continue;
                }
                let total = cost + tile.terrain.move_cost();
                if total > self.move_points {
                    continue;
                }
                if best.get(&next).is_none_or(|&(c, _)| total < c) {
                    best.insert(next, (total, at));
                    queue.push(std::cmp::Reverse((total, next.x(), next.y())));
                }
            }
        }
        best
    }

    // ---- Cards: what is legal ----

    /// Whether `player` could play `card` right now at all, ignoring targets.
    pub fn can_play_now(&self, player: PlayerId, card: CardId) -> Result<(), RuleError> {
        if !self.hand(player).contains(&card) {
            return Err(RuleError::NotInHand);
        }
        let def = self.def(card);
        match &self.window {
            Some(w) => {
                if !w.eligible.contains(&player) {
                    return Err(RuleError::NotYourTurn);
                }
                if w.has_chosen(player) {
                    return Err(RuleError::AlreadyChose);
                }
                let fits = match w.kind {
                    WindowKind::Target { .. } => def.timing == Timing::Response,
                    // Cards go into a battle only as burned faces.
                    WindowKind::Battle { .. } => false,
                    WindowKind::Enter { .. } | WindowKind::End { .. } => {
                        def.timing == Timing::Instant
                    }
                };
                if !fits {
                    return Err(RuleError::WrongTiming);
                }
            }
            None => {
                if player != self.current_player() {
                    return Err(RuleError::NotYourTurn);
                }
                if def.timing == Timing::Response {
                    return Err(RuleError::WrongTiming);
                }
            }
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if def.cost > have {
            return Err(RuleError::NotEnoughSpirit {
                need: def.cost,
                have,
            });
        }
        Ok(())
    }

    /// Legal targets for `card` held by `player`, whether or not it is
    /// playable this moment.
    pub fn targets(&self, player: PlayerId, card: CardId) -> Vec<Target> {
        let def = self.def(card);
        let me = self.hex_of(player);
        match def.target {
            TargetRule::Caster => vec![Target::Champion(player)],
            TargetRule::Champion { range } => self
                .players()
                .filter(|&p| self.hex_of(p).unsigned_distance_to(me) <= range)
                .map(Target::Champion)
                .collect(),
            TargetRule::Enemy { range } => self
                .players()
                .filter(|&p| p != player && self.hex_of(p).unsigned_distance_to(me) <= range)
                .map(Target::Champion)
                .collect(),
            TargetRule::EmptyHex { range } => self
                .board
                .tiles()
                .filter(|(h, t)| {
                    h.unsigned_distance_to(me) <= range
                        && self.occupant(*h).is_none()
                        && !self.guard_at(*h)
                        && match def.effect {
                            Effect::Grow => {
                                t.terrain.can_grow_grove() && t.terrain != Terrain::Grove
                            }
                            Effect::Trap(_) => !self.traps.iter().any(|tr| tr.hex == *h),
                            _ => true,
                        }
                })
                .map(|(h, _)| Target::Hex(h))
                .collect(),
            TargetRule::Corpse => {
                let has = self.board.tile(me).is_some_and(|t| t.corpse.is_some());
                if has {
                    vec![Target::Hex(me)]
                } else {
                    Vec::new()
                }
            }
            TargetRule::Pending => {
                if self.pending.is_some() {
                    vec![Target::None]
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// Cards `player` can play right now that have at least one target.
    pub fn playable(&self, player: PlayerId) -> Vec<CardId> {
        self.hand(player)
            .iter()
            .copied()
            .filter(|&c| {
                self.can_play_now(player, c).is_ok() && !self.targets(player, c).is_empty()
            })
            .collect()
    }

    fn check_play(&self, player: PlayerId, card: CardId, target: Target) -> Result<(), RuleError> {
        self.can_play_now(player, card)?;
        if !self.targets(player, card).contains(&target) {
            return Err(RuleError::InvalidTarget);
        }
        Ok(())
    }

    // ---- Applying intents ----

    pub fn apply(&mut self, player: PlayerId, intent: Intent) -> Result<Vec<Event>, RuleError> {
        if self.winner.is_some() {
            return Err(RuleError::GameOver);
        }
        if self.champion(player).is_none() {
            return Err(RuleError::UnknownPlayer);
        }
        let mut events = Vec::new();
        if self.window.is_some() {
            self.apply_in_window(player, intent, &mut events)?;
        } else {
            if player != self.current_player() {
                return Err(RuleError::NotYourTurn);
            }
            match intent {
                Intent::Move { to } => self.step(player, to, &mut events)?,
                Intent::EndTurn => self.end_turn(player, &mut events),
                Intent::Play { card, target } => {
                    self.play_own(player, card, target, &mut events)?
                }
                Intent::Pass | Intent::Burn { .. } => return Err(RuleError::NoWindow),
            }
        }
        self.check_victory(&mut events);
        self.log.extend(events.iter().cloned());
        Ok(events)
    }

    fn step(
        &mut self,
        player: PlayerId,
        to: Hex,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        if let Some(defender) = self.occupant(to)
            && defender != player
        {
            let cost = self.attack_cost(player, to)?;
            self.start_battle(player, defender, cost, events);
            return Ok(());
        }
        let cost = self.step_cost(player, to)?;
        if cost > self.move_points {
            return Err(RuleError::NotEnoughMovePoints {
                need: cost,
                have: self.move_points,
            });
        }
        let from = self.hex_of(player);
        self.champ_mut(player).hex = to;
        self.move_points -= cost;
        events.push(Event::Moved {
            player,
            from,
            to,
            cost,
        });
        self.spring_traps(player, to, events);

        // The champion may have fallen to a trap and woken at home.
        let at = self.hex_of(player);
        if at == to {
            self.claim(player, at, events);
        }
        let near: Vec<PlayerId> = self
            .initiative_after(player)
            .into_iter()
            .filter(|&p| self.hex_of(p).unsigned_distance_to(at) <= REACTION_RANGE)
            .collect();
        self.open_window(
            WindowKind::Enter {
                mover: player,
                hex: at,
            },
            near,
            events,
        );
        Ok(())
    }

    fn end_turn(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        events.push(Event::TurnEnded { player });
        self.turn_over = true;
        // Ending the turn on a temple is a prayer to its god.
        let hex = self.hex_of(player);
        if let Some(tile) = self.board.tile(hex)
            && tile.terrain == Terrain::Temple
            && let Some(god) = tile.region
        {
            self.offer(Some(player), god, 1, events);
            self.record_deed(player, style::Deed::Prayed);
        }
        let others = self.initiative_after(player);
        self.open_window(WindowKind::End { player }, others, events);
        if self.window.is_none() {
            self.advance_turn(events);
        }
    }

    fn play_own(
        &mut self,
        player: PlayerId,
        card: CardId,
        target: Target,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_play(player, card, target)?;
        self.take_from_hand(player, card, events);
        events.push(Event::CardPlayed {
            player,
            card,
            def: self.def_id(card),
            target,
            response: false,
        });
        let bonus = self.chain(player, self.def(card).element, events);

        if let Target::Champion(aimed) = target
            && aimed != player
        {
            let mut eligible = vec![aimed];
            let at = self.hex_of(aimed);
            eligible.extend(self.initiative_after(player).into_iter().filter(|&p| {
                p != aimed && self.hex_of(p).unsigned_distance_to(at) <= REACTION_RANGE
            }));
            self.pending = Some(Pending {
                caster: player,
                card,
                target,
                bonus,
                canceled: false,
            });
            self.open_window(
                WindowKind::Target {
                    caster: player,
                    target: aimed,
                    card,
                },
                eligible,
                events,
            );
        } else {
            self.resolve(player, card, target, bonus, events);
        }
        Ok(())
    }

    fn apply_in_window(
        &mut self,
        player: PlayerId,
        intent: Intent,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let window = self.window.as_ref().expect("checked by caller");
        if !window.eligible.contains(&player) {
            return Err(if player == self.current_player() {
                RuleError::WindowOpen
            } else {
                RuleError::NotYourTurn
            });
        }
        if window.has_chosen(player) {
            return Err(RuleError::AlreadyChose);
        }
        let battle = matches!(window.kind, WindowKind::Battle { .. });
        let choice = match intent {
            Intent::Pass => Choice::Pass,
            Intent::Play { card, target } => {
                self.check_play(player, card, target)?;
                self.take_from_hand(player, card, events);
                Choice::Play(card, target)
            }
            Intent::Burn { cards } if battle => {
                self.check_burn(player, &cards)?;
                for &card in &cards {
                    self.hands[player.0 as usize].retain(|&c| c != card);
                }
                Choice::Burn(cards)
            }
            Intent::Burn { .. } => return Err(RuleError::WrongTiming),
            Intent::Move { .. } | Intent::EndTurn => return Err(RuleError::WindowOpen),
        };
        let window = self.window.as_mut().expect("still open");
        window.choices.insert(player, choice);
        events.push(Event::ChoiceMade { player });
        if window.eligible.iter().all(|p| window.has_chosen(*p)) {
            self.close_window(events);
        }
        Ok(())
    }

    fn open_window(&mut self, kind: WindowKind, eligible: Vec<PlayerId>, events: &mut Vec<Event>) {
        if eligible.is_empty() {
            // Nobody may react: a pending card resolves at once.
            if let Some(p) = self.pending.take() {
                self.resolve(p.caster, p.card, p.target, p.bonus, events);
            }
            return;
        }
        events.push(Event::WindowOpened {
            kind,
            eligible: eligible.clone(),
        });
        self.window = Some(Window {
            kind,
            eligible,
            choices: BTreeMap::new(),
        });
    }

    fn close_window(&mut self, events: &mut Vec<Event>) {
        let window = self.window.take().expect("closing an open window");
        let plays: Vec<(PlayerId, CardId, Target)> = window
            .eligible
            .iter()
            .filter_map(|&p| match window.choices.get(&p) {
                Some(&Choice::Play(c, t)) => Some((p, c, t)),
                _ => None,
            })
            .collect();
        events.push(Event::WindowClosed {
            kind: window.kind,
            played: plays.iter().map(|(p, _, _)| *p).collect(),
        });

        for (player, card, target) in plays {
            events.push(Event::CardPlayed {
                player,
                card,
                def: self.def_id(card),
                target,
                response: true,
            });
            self.resolve(player, card, target, 0, events);
        }

        if let Some(p) = self.pending.take() {
            if p.canceled {
                self.discard.push(p.card);
            } else {
                self.resolve(p.caster, p.card, p.target, p.bonus, events);
            }
        }
        match window.kind {
            WindowKind::End { .. } => self.advance_turn(events),
            WindowKind::Battle { attacker, defender } => {
                let burned = |p: PlayerId| match window.choices.get(&p) {
                    Some(Choice::Burn(cards)) => cards.clone(),
                    _ => Vec::new(),
                };
                let (a, d) = (burned(attacker), burned(defender));
                self.resolve_battle(attacker, defender, a, d, events);
            }
            _ => {}
        }
    }

    fn cancel_pending(&mut self, by: CardId, events: &mut Vec<Event>) {
        let by_element = self.def(by).element;
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let pending_element = self.defs[pending.card.0 as usize].def().element;
        // "Growth breaks through the grave": a card of the element that
        // quenches the canceller's element cannot be silenced by it.
        let pierces = matches!((by_element, pending_element),
            (Some(b), Some(p)) if p == b.quenched_by());
        if pierces {
            events.push(Event::CancelFailed {
                card: pending.card,
                by,
            });
        } else {
            pending.canceled = true;
            events.push(Event::Canceled {
                card: pending.card,
                by,
            });
        }
    }

    fn take_from_hand(&mut self, player: PlayerId, card: CardId, events: &mut Vec<Event>) {
        self.hands[player.0 as usize].retain(|&c| c != card);
        let cost = self.def(card).cost;
        if cost > 0 {
            let champ = self.champ_mut(player);
            champ.spirit_points -= cost;
            let spirit = champ.spirit_points;
            events.push(Event::SpiritChanged { player, spirit });
        }
    }

    /// Generation chain on your own turn: +1 to the card's number. A chain
    /// that breaks the yin-yang rhythm costs a surge of qi (§4).
    fn chain(&mut self, player: PlayerId, element: Option<Element>, events: &mut Vec<Event>) -> u8 {
        let prev = self.last_element;
        self.last_element = element;
        let (Some(from), Some(to)) = (prev, element) else {
            return 0;
        };
        if from.generates() != to {
            return 0;
        }
        let rhythm_broken = from.breaks_rhythm_with(to);
        events.push(Event::Chain {
            player,
            from,
            to,
            rhythm_broken,
        });
        if rhythm_broken {
            self.add_threat(player, 1, events);
            let champ = self.champ_mut(player);
            if champ.spirit_points > 0 {
                champ.spirit_points -= 1;
                let spirit = champ.spirit_points;
                events.push(Event::SpiritChanged { player, spirit });
            } else {
                self.damage(player, 1, events);
            }
        }
        1
    }

    // ---- Resolving effects ----

    fn resolve(
        &mut self,
        caster: PlayerId,
        card: CardId,
        target: Target,
        bonus: u8,
        events: &mut Vec<Event>,
    ) {
        let def = self.def(card);
        let still_valid = match target {
            Target::Hex(h) => match def.target {
                TargetRule::EmptyHex { .. } => self.occupant(h).is_none(),
                TargetRule::Corpse => {
                    self.hex_of(caster) == h
                        && self.board.tile(h).is_some_and(|t| t.corpse.is_some())
                }
                _ => true,
            },
            _ => true,
        };
        if !still_valid {
            events.push(Event::Fizzled { card });
            self.discard.push(card);
            return;
        }

        let aimed = match target {
            Target::Champion(p) => Some(p),
            _ => None,
        };
        // Its god takes the card as an offering (§5); bodies weigh double.
        if let Some(element) = def.element {
            let amount = if def.kind == CardKind::Body { 2 } else { 1 };
            self.offer(
                Some(caster),
                God::from_index(element.index()),
                amount,
                events,
            );
            self.record_deed(caster, style::Deed::Played(element));
            // Zaga is the one god whose cards quiet a champion down (§6.5).
            if element == Element::Earth {
                self.add_threat(caster, -1, events);
            }
        }
        if let Some(verb) = style::BodyVerb::of(def.effect) {
            self.record_deed(caster, style::Deed::Body(verb));
            // Burning and conscripting the dead is noticed; releasing is not.
            if matches!(verb, style::BodyVerb::Fuel | style::BodyVerb::Legion) {
                self.add_threat(caster, 1, events);
            }
        }
        let shift = self.stage_shift(def.element, def.effect.is_harmful());
        let n = |base: u8| (i16::from(base + bonus) + i16::from(shift)).max(1) as u8;
        match def.effect {
            Effect::Damage(x) => {
                if let Some(t) = aimed
                    && self.pierce(t, def.element, events)
                {
                    self.damage(t, n(x), events);
                }
            }
            Effect::Drain(x) => {
                if let Some(t) = aimed
                    && self.pierce(t, def.element, events)
                {
                    self.damage(t, n(x), events);
                    self.heal(caster, n(x), events);
                }
            }
            Effect::Finish(x) => {
                if let Some(t) = aimed {
                    let c = &self.champions[t.0 as usize];
                    if c.hp == c.body {
                        events.push(Event::Fizzled { card });
                    } else if self.pierce(t, def.element, events) {
                        self.damage(t, n(x), events);
                    }
                }
            }
            Effect::Root => {
                if let Some(t) = aimed
                    && self.pierce(t, def.element, events)
                {
                    self.root(t, events);
                }
            }
            Effect::Heal(x) => {
                if let Some(t) = aimed {
                    self.heal(t, n(x), events);
                }
            }
            Effect::Ward => {
                if let (Some(t), Some(element)) = (aimed, def.element) {
                    self.raise_ward(t, element, events);
                }
            }
            Effect::Haste(x) => {
                if caster == self.current_player() {
                    self.move_points += u32::from(n(x));
                    events.push(Event::Hasted {
                        player: caster,
                        move_points: self.move_points,
                    });
                }
            }
            Effect::Draw(x) => self.draw(caster, n(x) as usize, events),
            Effect::Trap(_) => {
                if let Target::Hex(hex) = target {
                    self.traps.push(Trap {
                        owner: caster,
                        hex,
                        card,
                    });
                    events.push(Event::TrapSet {
                        player: caster,
                        hex,
                    });
                    // The card stays on the board until it springs.
                    return;
                }
            }
            Effect::Grow => {
                if let Target::Hex(hex) = target {
                    self.grow(hex, events);
                }
            }
            Effect::Blink => {
                if let Target::Hex(to) = target {
                    let from = self.hex_of(caster);
                    self.champ_mut(caster).hex = to;
                    events.push(Event::Blinked {
                        player: caster,
                        from,
                        to,
                    });
                    self.spring_traps(caster, to, events);
                    if self.hex_of(caster) == to {
                        self.claim(caster, to, events);
                    }
                }
            }
            Effect::Cancel => self.cancel_pending(card, events),
            Effect::BodyFuel => {
                self.take_corpse(caster, events);
                self.gain_spirit(caster, n(2), events);
                if caster == self.current_player() {
                    self.move_points += 1;
                    events.push(Event::Hasted {
                        player: caster,
                        move_points: self.move_points,
                    });
                }
            }
            Effect::BodyLegion => {
                self.take_corpse(caster, events);
                self.raise_ward(caster, Element::Metal, events);
            }
            Effect::BodyDissolve => {
                self.take_corpse(caster, events);
                self.gain_spirit(caster, n(1), events);
                self.heal(caster, n(2), events);
            }
            Effect::BodyRest => {
                self.take_corpse(caster, events);
                self.heal(caster, n(1), events);
                self.draw(caster, 1, events);
            }
            Effect::BodySeed => {
                let hex = self.hex_of(caster);
                self.take_corpse(caster, events);
                self.grow(hex, events);
                self.heal(caster, n(2), events);
            }
            Effect::Feast => self.feast(caster, n(1), events),
        }
        self.discard.push(card);
    }

    /// Checks a harmful card against the target's ward. Only the element that
    /// quenches the ward gets through, breaking it.
    fn pierce(
        &mut self,
        target: PlayerId,
        element: Option<Element>,
        events: &mut Vec<Event>,
    ) -> bool {
        let Some(ward) = self.champions[target.0 as usize].ward else {
            return true;
        };
        if element == Some(ward.quenched_by()) {
            self.champ_mut(target).ward = None;
            events.push(Event::WardBroken {
                player: target,
                ward,
                by: ward.quenched_by(),
            });
            true
        } else {
            events.push(Event::Blocked {
                player: target,
                ward,
            });
            false
        }
    }

    fn damage(&mut self, player: PlayerId, amount: u8, events: &mut Vec<Event>) {
        let champ = self.champ_mut(player);
        champ.hp = champ.hp.saturating_sub(amount);
        let hp = champ.hp;
        events.push(Event::Damaged { player, amount, hp });
        if hp == 0 {
            self.fall(player, events);
        }
    }

    fn heal(&mut self, player: PlayerId, amount: u8, events: &mut Vec<Event>) {
        let champ = self.champ_mut(player);
        let before = champ.hp;
        champ.hp = (champ.hp + amount).min(champ.body);
        let (hp, healed) = (champ.hp, champ.hp - before);
        if healed > 0 {
            events.push(Event::Healed {
                player,
                amount: healed,
                hp,
            });
        }
    }

    fn gain_spirit(&mut self, player: PlayerId, amount: u8, events: &mut Vec<Event>) {
        let champ = self.champ_mut(player);
        champ.spirit_points = (champ.spirit_points + amount).min(champ.spirit);
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
    }

    fn raise_ward(&mut self, player: PlayerId, element: Element, events: &mut Vec<Event>) {
        self.champ_mut(player).ward = Some(element);
        events.push(Event::WardRaised { player, element });
    }

    fn root(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        if self.is_active(player) {
            self.move_points = 0;
        } else {
            self.champ_mut(player).rooted = true;
        }
        events.push(Event::Rooted { player });
    }

    fn grow(&mut self, hex: Hex, events: &mut Vec<Event>) {
        if let Some(tile) = self.board.tile_mut(hex)
            && tile.terrain.can_grow_grove()
        {
            tile.terrain = Terrain::Grove;
            events.push(Event::GroveGrew { hex });
        }
    }

    fn take_corpse(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let hex = self.hex_of(player);
        if let Some(tile) = self.board.tile_mut(hex)
            && tile.corpse.take().is_some()
        {
            events.push(Event::CorpseTaken { hex });
        }
    }

    fn spring_traps(&mut self, victim: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        let (sprung, kept): (Vec<Trap>, Vec<Trap>) = self
            .traps
            .iter()
            .partition(|t| t.hex == hex && t.owner != victim);
        self.traps = kept;
        for trap in sprung {
            let def = self.def(trap.card);
            events.push(Event::TrapSprung {
                owner: trap.owner,
                victim,
                hex,
                def: self.def_id(trap.card),
            });
            if self.pierce(victim, def.element, events) {
                match def.effect {
                    Effect::Trap(TrapEffect::Damage(x)) => {
                        let shift = self.stage_shift(def.element, true);
                        let x = (i16::from(x) + i16::from(shift)).max(1) as u8;
                        self.damage(victim, x, events)
                    }
                    Effect::Trap(TrapEffect::Root) => self.root(victim, events),
                    _ => {}
                }
            }
            self.discard.push(trap.card);
        }
    }

    /// Zero health: the champion leaves a body and wakes at home, whole.
    fn fall(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let at = self.hex_of(player);
        if let Some(tile) = self.board.tile_mut(at)
            && tile.corpse.is_none()
        {
            tile.corpse = Some(Corpse { age: 0 });
        }
        let home = self.board.start_of(self.champions[player.0 as usize].god);
        let respawn = (0..=self.board.radius() * 2)
            .flat_map(|r| home.ring(r).collect::<Vec<_>>())
            .find(|&h| {
                self.board.contains(h)
                    && self.occupant(h).is_none_or(|p| p == player)
                    && !self.guard_at(h)
            })
            .unwrap_or(home);
        let champ = self.champ_mut(player);
        champ.hex = respawn;
        champ.hp = champ.body;
        champ.ward = None;
        champ.rooted = false;
        if self.is_active(player) {
            self.move_points = 0;
        }
        events.push(Event::ChampionFell {
            player,
            at,
            respawn,
        });
    }

    fn draw(&mut self, player: PlayerId, count: usize, events: &mut Vec<Event>) {
        for _ in 0..count {
            if self.deck.is_empty() {
                if self.discard.is_empty() {
                    return;
                }
                self.deck = std::mem::take(&mut self.discard);
                self.rng.shuffle(&mut self.deck);
                events.push(Event::DeckReshuffled);
            }
            let card = self.deck.pop().expect("deck refilled");
            self.hands[player.0 as usize].push(card);
            events.push(Event::CardDrawn {
                player,
                card,
                def: self.def_id(card),
            });
        }
    }

    fn refill_hand(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let limit = self.champions[player.0 as usize].hand_limit();
        let have = self.hands[player.0 as usize].len();
        self.draw(player, limit.saturating_sub(have), events);
    }

    // ---- Rounds and turns ----

    fn advance_turn(&mut self, events: &mut Vec<Event>) {
        self.turn += 1;
        if self.turn == self.order.len() {
            self.world_phase(events);
            if self.time == TimeOfDay::Day {
                events.push(Event::Dusk { round: self.round });
                self.dusk(events);
                self.judge_the_day(events);
            }
            self.start_round(events);
        } else {
            self.start_turn(events);
        }
    }

    fn start_round(&mut self, events: &mut Vec<Event>) {
        self.round += 1;
        self.time = match self.time {
            TimeOfDay::Day => TimeOfDay::Night,
            TimeOfDay::Night => TimeOfDay::Day,
        };
        if self.round > 1 {
            // Initiative rotates: the first player goes last.
            self.order.rotate_left(1);
        }
        self.turn = 0;
        events.push(Event::RoundStarted {
            round: self.round,
            time: self.time,
            order: self.order.clone(),
        });
        if self.time == TimeOfDay::Day {
            events.push(Event::Dawn { round: self.round });
            self.dawn(events);
        }
        self.start_turn(events);
    }

    fn start_turn(&mut self, events: &mut Vec<Event>) {
        let player = self.current_player();
        self.move_points = MOVE_POINTS;
        self.last_element = None;
        self.turn_over = false;
        let champ = self.champ_mut(player);
        let faded = champ.ward.take().is_some();
        let rooted = std::mem::take(&mut champ.rooted);
        if faded {
            events.push(Event::WardFaded { player });
        }
        if rooted {
            self.move_points = 0;
        }
        events.push(Event::TurnStarted {
            player,
            move_points: self.move_points,
        });
        let champ = &self.champions[player.0 as usize];
        if champ.spirit_points < champ.spirit {
            self.gain_spirit(player, 1, events);
        }
        self.refill_hand(player, events);
    }

    /// Corpses age and sprout; the night leaves a new one behind.
    fn world_phase(&mut self, events: &mut Vec<Event>) {
        let corpses: Vec<(Hex, Corpse)> = self.board.corpses().collect();
        for (hex, corpse) in corpses {
            let tile = self.board.tile_mut(hex).expect("corpse on the board");
            let age = corpse.age + 1;
            if age < GROVE_AGE {
                tile.corpse = Some(Corpse { age });
            } else if tile.terrain.can_grow_grove() {
                tile.corpse = None;
                tile.terrain = Terrain::Grove;
                events.push(Event::GroveGrew { hex });
                // An untouched body is Bhava's offering (§3).
                self.offer(None, God::Bhava, 1, events);
            } else {
                tile.corpse = None;
                events.push(Event::CorpseDecayed { hex });
            }
        }
        if self.time == TimeOfDay::Night {
            self.spawn_corpse(events);
        }
        self.guard_phase(events);
    }

    fn spawn_corpse(&mut self, events: &mut Vec<Event>) {
        let free: Vec<Hex> = self
            .board
            .tiles()
            .filter(|(h, t)| {
                t.corpse.is_none()
                    && t.terrain.can_grow_grove()
                    && self.occupant(*h).is_none()
                    && !God::ALL.iter().any(|g| self.board.start_of(*g) == *h)
            })
            .map(|(h, _)| h)
            .collect();
        if let Some(&hex) = self.rng.pick(&free) {
            self.board.tile_mut(hex).expect("free hex").corpse = Some(Corpse { age: 0 });
            events.push(Event::CorpseAppeared { hex });
        }
    }
}

mod battle;
mod guard;
mod style;
mod victory;
mod world;
pub use battle::Score;
pub use guard::{GUARD_DICE, GUARD_RELIEF, GUARD_STEPS, Guard};
pub use style::{BodyVerb, Character, Deed, GUARD_THRESHOLD, StyleReason, Taste, TasteKind};
pub use victory::{Check, CheckKind, Condition, OPEN_COUNT};
pub use world::{Pantheon, STAGE_THRESHOLD, STAGES, TRISHNA_DRIFT};

#[cfg(test)]
mod tests;
