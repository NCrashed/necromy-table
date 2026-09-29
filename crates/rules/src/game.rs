//! Match state, intents and events (docs/design.md §11).
//!
//! A round is a day or a night. Inside it players take their turns, then the
//! world acts. Dawn opens a day, dusk closes it.
//!
//! Everyone takes their turn at once (§11.2): each player is acting, held or
//! done. An action that would touch a rival who is still acting (a battle,
//! an Enter window for them, a card at them or next to them) is held until
//! that rival ends their turn; the one who came close waits. Held actions
//! are checked again after every intent and played, or dropped when no
//! longer legal. The round ends when everyone is done.
//!
//! Reaction windows (§11.3) belong to the action that opened them; several
//! may be open at once, but a player answers in one at most. While a window
//! is open its eligible players act in it, each once: a card or a pass.
//! Choices stay hidden until the last one is in, then resolve together.
//! A response never opens another window.

use std::collections::{BTreeMap, BinaryHeap, HashMap};

use hexx::Hex;
use serde::{Deserialize, Serialize};

use necromy_dice::Face;

use crate::board::{Board, Corpse, Terrain};
use crate::cards::{
    self, CardDef, CardId, CardKind, CardMod, DefId, Effect, TargetRule, Timing, TrapEffect,
};
use crate::features::{Feature, Mode, World};
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
    /// A full world, or a small one the players grow (§21).
    pub mode: Mode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Out of rivals' sight (§11.6).
    pub hidden: bool,
    /// Where rivals saw them last; views show a hidden rival here.
    pub seen_at: Hex,
    /// Loses a health each turn, down to one (§20.1).
    pub poison: Option<Poison>,
    /// Worn items by `Slot::index` (§20.3).
    pub gear: [Option<crate::items::ItemId>; 3],
}

impl Champion {
    /// Might, body, wits and spirit a champion of `god` starts with.
    /// Placeholder stats: each patron leans one way (§13).
    pub const fn stats_of(god: God) -> [u8; 4] {
        match god {
            God::Bhava => [3, 5, 2, 3],
            God::Trishna => [4, 3, 3, 3],
            God::Zaga => [3, 4, 2, 4],
            God::Ahamar => [4, 4, 3, 2],
            God::Maya => [2, 3, 4, 4],
        }
    }

    fn new(god: God, hex: Hex) -> Self {
        let [might, body, wits, spirit] = Champion::stats_of(god);
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
            hidden: false,
            seen_at: hex,
            poison: None,
            gear: [None; 3],
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
    /// Any time of the day: seal a wish for dusk, asking `god` for `wish`,
    /// one or two acts and perhaps a price (§7.3, §21.4).
    Wish {
        god: God,
        wish: wish::Wish,
        /// A model's reading of a free-text wish; `None` for a prepared one.
        said: Option<wish::Said>,
    },
    /// Any time of the day: want nothing tonight (the Wager wants this
    /// from the Crown, §10).
    RefuseWish,
    /// In a window: play nothing.
    Pass,
    /// On your turn at a temple: give the item in `slot` to its god (§20.3).
    Sacrifice {
        slot: crate::items::Slot,
    },
    /// On your turn on the ruins of a settlement: build it again (§20.4).
    Rebuild,
    /// At the start: pick the Great Deed of the match from those offered
    /// (§21.7).
    ChooseDeed {
        deed: victory::GreatDeed,
    },
    /// Once a turn: let these cards go and draw one fewer, as many at a
    /// temple (§21.2).
    Cycle {
        cards: Vec<CardId>,
    },
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
    /// A wish demands tribute for `asker` (§7.3): each rival gives a card
    /// (a play of it) or takes Threat (a pass).
    Tribute { asker: PlayerId },
    /// A champion attacks the royal guard: they pick cards to burn, the
    /// guard burns none (§20.4).
    GuardBattle { attacker: PlayerId },
    /// A champion attacks undead `id`; only they burn (§20.4).
    MobBattle { attacker: PlayerId, id: u32 },
    /// A champion attacks the militia of `home`; only they burn (§20.4).
    MilitiaBattle { attacker: PlayerId, home: Hex },
    /// Before a trial's throw: its challenger picks cards to burn (§20.2).
    Trial { player: PlayerId, hex: Hex },
}

/// Where a player stands in the round (§11.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Taking their turn.
    Acting,
    /// Their `intent` touched a rival who is still acting (`on`; `None`
    /// when naming them would give a hidden one away) or busy in another
    /// window: it waits.
    Held {
        on: Option<PlayerId>,
        intent: Intent,
    },
    /// Ended their turn this round.
    Done,
}

/// A player's own turn: its phase and what is left of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Turn {
    phase: Phase,
    move_points: u32,
    /// Element of the last card played this turn (§4 chains).
    last_element: Option<Element>,
    /// Cards played on their own turn so far (Zaga's Burden, §5.3).
    cards: u8,
    /// The hand went through once this turn (`Intent::Cycle`).
    cycled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Choice {
    Pass,
    Play(CardId, Target),
    Burn(Vec<CardId>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub kind: WindowKind,
    /// Whose action opened it; they wait until it closes.
    pub actor: PlayerId,
    /// In resolution order.
    pub eligible: Vec<PlayerId>,
    choices: BTreeMap<PlayerId, Choice>,
    /// The card of a Target window, resolved when the window closes.
    pending: Option<Pending>,
    /// A Battle window struck from the shadow: one more die (§11.6).
    ambush: Option<PlayerId>,
}

impl Window {
    pub fn has_chosen(&self, player: PlayerId) -> bool {
        self.choices.contains_key(&player)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// An undead, by id (§20.4).
    Mob(u32),
    /// A settlement's militia, by its home.
    Militia(Hex),
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
    /// `player`'s action waits for `on` to end their turn (§11.2); `None`
    /// when the one in the way is hidden or busy elsewhere.
    Held {
        player: PlayerId,
        on: Option<PlayerId>,
    },
    /// The held action goes ahead now.
    Resumed {
        player: PlayerId,
    },
    /// The held action is no longer legal and is dropped; the turn goes on.
    HoldDropped {
        player: PlayerId,
        why: RuleError,
    },
    /// A law of the world acted (§5.3): on `player`, at `hex`, or on the
    /// whole table.
    Law {
        law: laws::Law,
        player: Option<PlayerId>,
        hex: Option<Hex>,
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
    /// On its way the guard hewed down undead `undead` next to it.
    GuardHewed {
        hex: Hex,
        undead: u32,
    },
    GuardStruck {
        target: PlayerId,
    },
    GuardResolved {
        target: PlayerId,
        guard_score: Score,
        target_score: Score,
    },
    /// An untended body rose as one of the undead (§20.4).
    MobAppeared {
        mob: mobs::Mob,
    },
    /// A beast went back into Bhava's woods: his light calmed them.
    MobLeft {
        id: u32,
    },
    /// A beast tore apart an undead that wandered onto its land.
    BeastMauled {
        beast: u32,
        undead: u32,
    },
    MobMoved {
        id: u32,
        from: Hex,
        to: Hex,
    },
    /// World phase: an undead strikes `target`; its dice follow.
    MobStruck {
        id: u32,
        target: PlayerId,
    },
    /// `attacker` stepped onto undead `id`; their burn choice follows.
    MobAttacked {
        attacker: PlayerId,
        id: u32,
    },
    MobResolved {
        id: u32,
        champion: PlayerId,
        /// The champion struck first, else the undead did.
        champion_attacked: bool,
        mob_score: Score,
        champion_score: Score,
    },
    MobHurt {
        id: u32,
        amount: u8,
        hp: u8,
    },
    /// Laid to rest by `by`.
    MobFell {
        id: u32,
        hex: Hex,
        by: PlayerId,
    },
    /// A settlement's militia standing on `hex` cut down mob `mob` next
    /// to them.
    MilitiaStruck {
        hex: Hex,
        mob: u32,
    },
    /// The militia of `home` struck `player` next to them in the world
    /// phase, for `why`; a hit that never takes the last health follows.
    MilitiaHit {
        home: Hex,
        player: PlayerId,
        why: militia::MilitiaWhy,
    },
    /// No militia held it: the undead laid it waste.
    SettlementRuined {
        hex: Hex,
    },
    /// What the militia think of `player` now, −3..=3.
    StandingChanged {
        player: PlayerId,
        standing: i8,
    },
    /// A friend of the militia rested in their settlement.
    MilitiaHelped {
        player: PlayerId,
        hex: Hex,
    },
    /// The unwelcome lingered in a settlement and were beaten.
    MilitiaBeat {
        player: PlayerId,
        hex: Hex,
    },
    /// The militia would not let `player` take their settlement.
    MilitiaBarred {
        player: PlayerId,
        hex: Hex,
    },
    /// The militia of `home` let `player` through and stepped out to `to`.
    MilitiaSwapped {
        player: PlayerId,
        home: Hex,
        to: Hex,
    },
    /// The militia of `home` went back to `to` (their home).
    MilitiaMoved {
        home: Hex,
        to: Hex,
    },
    /// The militia of `home` lost men; `men` left.
    MilitiaHurt {
        home: Hex,
        men: u8,
    },
    /// No man of `home`'s militia is left standing.
    MilitiaFell {
        home: Hex,
    },
    /// `attacker` stepped onto militia who hold something against them.
    MilitiaAttacked {
        attacker: PlayerId,
        home: Hex,
    },
    MilitiaResolved {
        home: Hex,
        champion: PlayerId,
        militia_score: Score,
        champion_score: Score,
    },
    /// World phase: undead `id` knocked down a man of `home`'s militia.
    UndeadHitMilitia {
        id: u32,
        home: Hex,
    },
    /// `player` built the ruins on `hex` into a settlement again.
    SettlementRebuilt {
        player: PlayerId,
        hex: Hex,
    },
    /// `attacker` stepped onto the royal guard; the Battle choice follows.
    GuardAttacked {
        attacker: PlayerId,
    },
    /// The guard took `amount`; `hp` left.
    GuardHurt {
        amount: u8,
        hp: u8,
    },
    /// The guard fell to `by` and leaves the board.
    GuardFell {
        hex: Hex,
        by: PlayerId,
    },
    /// Dusk waits for `player`'s wish (§21.4).
    WishDue {
        player: PlayerId,
    },
    /// `player` sealed their wish for tonight (or a refusal); what they
    /// asked stays hidden until the god answers.
    WishSealed {
        player: PlayerId,
    },
    /// Nothing of the sealed wish could still be given at dusk.
    WishLost {
        player: PlayerId,
        god: God,
    },
    WishRefused {
        player: PlayerId,
    },
    /// The god heard; its effects follow as ordinary events.
    /// What the god granted: the acts the budget covered (`dropped` more
    /// were asked) and the price given.
    WishGranted {
        player: PlayerId,
        god: God,
        wish: wish::Wish,
        dropped: u8,
        grade: u8,
        said: Option<wish::Said>,
    },
    /// The Dominant gave this up for their wish, before the god answered.
    PricePaid {
        player: PlayerId,
        price: wish::Price,
    },
    /// A wish told `player` what `about` sealed for tonight (§7.3).
    SecretLearned {
        player: PlayerId,
        about: PlayerId,
    },
    /// A wish showed `player` the hand of `about`; others see it empty.
    HandSeen {
        player: PlayerId,
        about: PlayerId,
        cards: Vec<DefId>,
    },
    /// A god blessed (or blighted) one copy of a card in `owner`'s hand.
    CardChanged {
        owner: PlayerId,
        card: CardId,
        god: God,
        blessed: bool,
    },
    /// A god forged a new card into `player`'s hand.
    CardForged {
        player: PlayerId,
        card: CardId,
        god: God,
    },
    /// No fighting between the two until dusk, kept by `god`.
    TruceMade {
        player: PlayerId,
        other: PlayerId,
        god: God,
    },
    /// `player` fought or harmed `other` in a truce.
    TruceBroken {
        player: PlayerId,
        other: PlayerId,
        god: God,
    },
    /// Cards of `element` in the deck were hallowed (or rotted): `count` of
    /// them, which ones nobody knows until drawn.
    DeckChanged {
        player: PlayerId,
        god: God,
        element: Element,
        count: u8,
        blessed: bool,
    },
    /// A curse of `god` hides in the deck.
    CursePlanted {
        player: PlayerId,
        god: God,
    },
    /// `player` sees the top of the deck, top first; others see it empty.
    Foreseen {
        player: PlayerId,
        cards: Vec<DefId>,
    },
    /// `player` drew a planted curse; it `bit` unless they planted it.
    CurseDrawn {
        player: PlayerId,
        planter: PlayerId,
        god: God,
        bit: bool,
    },
    /// `player` gave `card` to `to` as tribute.
    TributeGiven {
        player: PlayerId,
        to: PlayerId,
        card: CardId,
    },
    /// `player` refused tribute to `to`, and takes Threat for it.
    TributeRefused {
        player: PlayerId,
        to: PlayerId,
    },
    /// `player` bets `target` will do `bet` before dusk; `god` holds it.
    WagerMade {
        player: PlayerId,
        target: PlayerId,
        bet: wish::Bet,
        god: God,
    },
    WagerWon {
        player: PlayerId,
        target: PlayerId,
        bet: wish::Bet,
        god: God,
    },
    WagerLost {
        player: PlayerId,
        target: PlayerId,
        bet: wish::Bet,
        god: God,
    },
    /// The two changed places: `player` now stands at `to`, `other` at
    /// `other_to`.
    Swapped {
        player: PlayerId,
        to: Hex,
        other: PlayerId,
        other_to: Hex,
    },
    TerrainChanged {
        hex: Hex,
        terrain: Terrain,
    },
    /// New land rose at the rim of the world (§21.1).
    LandRaised {
        hex: Hex,
        terrain: Terrain,
    },
    /// A new mechanic came into the world, brought by `god` for `player`
    /// (`None`: the world itself, §21.3).
    WorldGrew {
        feature: Feature,
        god: God,
        player: Option<PlayerId>,
    },
    /// `player` did something first at this table (§21.5); Style follows.
    First {
        player: PlayerId,
        novelty: novelty::Novelty,
    },
    /// `player` let `let_go` cards go and drew `drawn` (`Intent::Cycle`).
    Cycled {
        player: PlayerId,
        let_go: u8,
        drawn: u8,
    },
    /// Cards of a mechanic just come in were shuffled into the deck.
    DeckGrew {
        feature: Feature,
        cards: u8,
    },
    /// One mechanic a dusk: `god` sets `player`'s aside for the next.
    AwakeningDeferred {
        player: PlayerId,
        god: God,
        feature: Feature,
    },
    /// Maya took a card from the hand.
    CardDissolved {
        player: PlayerId,
        card: CardId,
    },
    CurseLaid {
        player: PlayerId,
        god: God,
    },
    CurseBit {
        player: PlayerId,
        god: God,
    },
    CurseLifted {
        player: PlayerId,
        god: God,
    },
    /// A god tells a story line (§8).
    LineTold {
        line: story::Line,
    },
    LineDone {
        line: story::Line,
    },
    LineFailed {
        line: story::Line,
    },
    /// A quiet board moved by itself.
    WorldStirred {
        stir: story::WorldStir,
    },
    /// An item worn: drawn from the loot deck or picked up (§20.3).
    ItemGained {
        player: PlayerId,
        item: crate::items::ItemId,
        from: gear::Gain,
    },
    /// An item left on the ground: pushed off by another, or dropped by the fallen.
    ItemDropped {
        player: PlayerId,
        item: crate::items::ItemId,
        hex: Hex,
    },
    /// The element that quenches it broke the item.
    ItemBroken {
        player: PlayerId,
        item: crate::items::ItemId,
        by: Element,
    },
    /// Given to the temple's god; the offering follows.
    ItemSacrificed {
        player: PlayerId,
        item: crate::items::ItemId,
        god: God,
    },
    /// An item did its turn-start work; its effect follows.
    ItemWorked {
        player: PlayerId,
        item: crate::items::ItemId,
    },
    /// The item's god is dark and takes its toll; the cost follows.
    ItemToll {
        player: PlayerId,
        item: crate::items::ItemId,
    },
    /// A god set a trial on a hex of its land (§20.2).
    TrialSet {
        trial: trial::Trial,
    },
    /// `player` stepped onto the trial on `hex`; the throw follows.
    TrialBegun {
        player: PlayerId,
        hex: Hex,
    },
    /// `got` counting faces of the `need`: passed, the boon follows.
    TrialPassed {
        player: PlayerId,
        trial: trial::Trial,
        got: u8,
        need: u8,
    },
    /// Short of `need`: the god's price follows; the trial stays.
    TrialFailed {
        player: PlayerId,
        trial: trial::Trial,
        got: u8,
        need: u8,
    },
    /// Nobody passed it in time.
    TrialFaded {
        hex: Hex,
    },
    /// The match is over.
    Victory {
        player: PlayerId,
        deed: victory::GreatDeed,
    },
    /// `player` picked the deed of their match (§21.7).
    DeedChosen {
        player: PlayerId,
        deed: victory::GreatDeed,
    },
    /// Every step of `player`'s deed holds: done at the next dusk, if it
    /// still does.
    DeedEve {
        player: PlayerId,
        deed: victory::GreatDeed,
    },
    /// The eve is broken: a step no longer holds.
    EveBroken {
        player: PlayerId,
        deed: victory::GreatDeed,
    },

    // Hidden information below (draws, traps, choices): a table sends each
    // seat its own version through `Game::event_for` (§17.1).
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
    /// Poison laid or added to; `stacks` is the total now (§20.1).
    Poisoned {
        player: PlayerId,
        element: Element,
        stacks: u8,
    },
    /// A heal of the generating element fed the poison a stack.
    PoisonFed {
        player: PlayerId,
        stacks: u8,
    },
    /// The turn started poisoned: `amount` (0 at one health) and a stack gone.
    PoisonBit {
        player: PlayerId,
        amount: u8,
        hp: u8,
        stacks: u8,
    },
    PoisonCured {
        player: PlayerId,
        by: Cure,
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
    /// A champion slipped out of sight at `hex` (§11.6).
    Hid {
        player: PlayerId,
        hex: Hex,
    },
    /// A hidden champion is seen again, at `hex`.
    Revealed {
        player: PlayerId,
        hex: Hex,
        why: stealth::RevealReason,
    },
    /// `mover` walked into `hidden` on `hex`.
    Stumbled {
        mover: PlayerId,
        hidden: PlayerId,
        hex: Hex,
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
    /// Not a wish this player can make now (not theirs, or a bad target).
    InvalidWish,
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
    /// Items are given to a god only at its temple.
    NotAtTemple,
    /// Nothing is worn in that slot.
    NothingWorn,
    /// Only the ruins of a settlement can be built again.
    NotRuins,
    /// The hand goes through once a turn, and only cards in it.
    NoCycle,
    /// Not a deed this player was offered, or one is chosen already.
    InvalidDeed,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::NotYourTurn => write!(f, "not your turn"),
            RuleError::NotAtTemple => write!(f, "not at a temple"),
            RuleError::NotRuins => write!(f, "not the ruins of a settlement"),
            RuleError::InvalidDeed => write!(f, "not a deed to choose"),
            RuleError::NoCycle => write!(f, "the hand went through already or holds no such card"),
            RuleError::NothingWorn => write!(f, "nothing worn there"),
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
            RuleError::InvalidWish => write!(f, "invalid wish"),
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    seed: u64,
    rng: Rng,
    board: Board,
    champions: Vec<Champion>,
    round: u32,
    time: TimeOfDay,
    order: Vec<PlayerId>,
    /// Per player: where they are in the round.
    turns: Vec<Turn>,
    /// This match's slice of the pool.
    slice: Vec<DefId>,
    /// Card instance → definition.
    defs: Vec<DefId>,
    deck: Vec<CardId>,
    discard: Vec<CardId>,
    hands: Vec<Vec<CardId>>,
    traps: Vec<Trap>,
    /// Open reaction windows, oldest first; a player is eligible in one at most.
    windows: Vec<Window>,
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
    /// The undead on the board, the last id and throws so far; each
    /// settlement's militia; what the militia think of each player (§20.4).
    mobs: Vec<mobs::Mob>,
    next_mob: u32,
    mob_throws: u64,
    militia: BTreeMap<(i32, i32), militia::Militia>,
    /// Settlements the undead laid waste, to be built again (§20.4).
    ruins: std::collections::BTreeSet<(i32, i32)>,
    standing: Vec<i8>,
    /// Per player, the rival who last went after them and the round.
    pursuers: Vec<Option<(PlayerId, u32)>>,
    taste: style::Taste,
    /// Per player, what they did since the last dusk.
    deeds: Vec<Vec<style::Deed>>,
    guard: Option<guard::Guard>,
    /// Great Deeds (§21.7): those offered to each player, the one chosen,
    /// the dusk count an eve began at, dusks so far.
    offers: Vec<Vec<victory::GreatDeed>>,
    chosen: Vec<Option<victory::GreatDeed>>,
    eves: Vec<Option<u32>>,
    dusks: u32,
    /// Groves grown from champions' bodies (a World Tree).
    hero_groves: std::collections::BTreeSet<(i32, i32)>,
    progress: Vec<victory::Progress>,
    winner: Option<(PlayerId, victory::GreatDeed)>,
    /// Per player, tonight's wish (§21.4).
    seals: Vec<dusk::Seal>,
    /// Dusk under way, waiting on wishes or tributes.
    dusk: Option<dusk::DuskStep>,
    /// Every wish granted so far: the gods remember (§7.5).
    asked: Vec<(God, wish::WishKind)>,
    /// Per player, the gods whose curse they carry.
    curses: Vec<Vec<God>>,
    /// Open story lines (§8).
    lines: Vec<story::Line>,
    next_line: u32,
    /// Trials on the board (§20.2), the last id and throws so far.
    trials: Vec<trial::Trial>,
    /// The loot deck, top last, and items lying on the board (§20.3).
    loot: Vec<crate::items::ItemId>,
    ground: Vec<(Hex, crate::items::ItemId)>,
    next_trial: u32,
    trial_throws: u64,
    /// Round of the last battle or guard strike.
    last_fight: u32,
    /// Deeds since the last story check.
    pending_story: Vec<(PlayerId, style::Deed)>,
    /// Changes on single copies of cards (§7.3): blessed, blighted, forged.
    mods: BTreeMap<CardId, CardMod>,
    /// Sealed wishes learned through a wish, until dusk: (who knows, whose).
    known: Vec<(PlayerId, PlayerId)>,
    /// Truces a wish made, until the next dusk.
    truces: Vec<wish::Truce>,
    /// Wagers a wish made, settled at the next dusk.
    wagers: Vec<wish::Wager>,
    /// Curses a wish hid in the deck: who planted each, for which god.
    planted: BTreeMap<CardId, (PlayerId, God)>,
    /// A scripted scene (a tutorial chapter, `scenario.rs`): the world holds
    /// still. No new bodies, stories or draws; dawn and the guard only if
    /// the scene lets them.
    scripted: Option<scenario::SceneWorld>,
    /// The mechanics this world has (§21.2).
    world: World,
    /// The round a mechanic last came in: one a dusk (§21.4).
    awakened: Option<u32>,
    /// Mechanics a god set aside for the next dusk: who asked, the god.
    deferred: Vec<(PlayerId, God, Feature)>,
    /// Land in Maya's fog until dusk: who may see it (§21.9).
    fog: BTreeMap<(i32, i32), PlayerId>,
    /// Land the wish being granted raised, for its god's twist.
    raised: Vec<Hex>,
    /// Who did what first at the table (§21.5).
    firsts: BTreeMap<novelty::Novelty, PlayerId>,
    /// Per player, battles won so far: each is worth less.
    won: Vec<u8>,
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
        // A full world, or a small one to create (§21.1).
        let (board, world) = match setup.mode {
            Mode::Full => (Board::generate(&mut rng), World::full()),
            Mode::Creation => creation::seed_world(&mut rng),
        };
        let militia = if world.has(Feature::Militia) {
            Self::militia_of(&board)
        } else {
            BTreeMap::new()
        };
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
        let offers = victory::deal(&mut rng, setup.champions.len());
        // Only cards of mechanics the world has (§21.2); a new world, fewer.
        let slice = match setup.mode {
            Mode::Full => cards::match_slice(&mut rng),
            Mode::Creation => cards::slice_of(&mut rng, creation::CREATION_SLICE, |d| {
                d.needs().is_none_or(|f| world.has(f))
            }),
        };
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
            turns: vec![
                Turn {
                    phase: Phase::Done,
                    move_points: 0,
                    last_element: None,
                    cards: 0,
                    cycled: false,
                };
                champions_len
            ],
            slice,
            defs,
            deck,
            discard: Vec::new(),
            hands,
            traps: Vec::new(),
            windows: Vec::new(),
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
            mobs: Vec::new(),
            next_mob: 0,
            mob_throws: 0,
            militia,
            ruins: Default::default(),
            standing: vec![0; setup.champions.len()],
            pursuers: vec![None; setup.champions.len()],
            taste,
            deeds: vec![Vec::new(); champions_len],
            guard: None,
            offers,
            chosen: vec![None; champions_len],
            eves: vec![None; champions_len],
            dusks: 0,
            hero_groves: Default::default(),
            progress: vec![victory::Progress::default(); champions_len],
            winner: None,
            seals: vec![dusk::Seal::Open; champions_len],
            dusk: None,
            asked: Vec::new(),
            curses: vec![Vec::new(); champions_len],
            lines: Vec::new(),
            next_line: 0,
            trials: Vec::new(),
            loot: Self::loot_deck(setup.seed),
            ground: Vec::new(),
            next_trial: 0,
            trial_throws: 0,
            last_fight: 0,
            pending_story: Vec::new(),
            mods: BTreeMap::new(),
            known: Vec::new(),
            truces: Vec::new(),
            wagers: Vec::new(),
            planted: BTreeMap::new(),
            scripted: None,
            world,
            awakened: None,
            deferred: Vec::new(),
            fog: BTreeMap::new(),
            raised: Vec::new(),
            firsts: BTreeMap::new(),
            won: vec![0; champions_len],
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

    /// The mechanics this world has (§21.2).
    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn has(&self, feature: Feature) -> bool {
        self.world.has(feature)
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

    /// Where `player` is in the round: acting, held or done (§11.2).
    pub fn phase(&self, player: PlayerId) -> &Phase {
        &self.turns[player.0 as usize].phase
    }

    /// Players still taking their turn (not held, not done), in initiative
    /// order.
    pub fn acting(&self) -> Vec<PlayerId> {
        self.order
            .iter()
            .copied()
            .filter(|&p| self.phase(p) == &Phase::Acting)
            .collect()
    }

    /// `player` is mid-turn: acting or held. False once they ended it and
    /// during the world phase.
    fn is_active(&self, player: PlayerId) -> bool {
        !matches!(self.phase(player), Phase::Done)
    }

    /// `player` may move and play on their own turn right now: acting, not
    /// waiting on a window of their own, not answering in one.
    pub fn free_to_act(&self, player: PlayerId) -> bool {
        self.phase(player) == &Phase::Acting
            && self
                .windows
                .iter()
                .all(|w| w.actor != player && !w.eligible.contains(&player))
    }

    pub fn move_points(&self, player: PlayerId) -> u32 {
        self.turns[player.0 as usize].move_points
    }

    /// Every open window, oldest first.
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// The window `player` answers in, or else the one their own action
    /// opened and they wait on.
    pub fn window_for(&self, player: PlayerId) -> Option<&Window> {
        self.windows
            .iter()
            .find(|w| w.eligible.contains(&player))
            .or_else(|| self.windows.iter().find(|w| w.actor == player))
    }

    /// The window `player` still has to answer in, if any.
    pub fn to_answer(&self, player: PlayerId) -> Option<&Window> {
        self.answering(player).map(|i| &self.windows[i])
    }

    /// Index of the window `player` still has to answer in.
    fn answering(&self, player: PlayerId) -> Option<usize> {
        self.windows
            .iter()
            .position(|w| w.eligible.contains(&player) && !w.has_chosen(player))
    }

    /// The element of the last card `player` played this turn: a card of
    /// the element it generates chains (§4).
    pub fn last_element(&self, player: PlayerId) -> Option<Element> {
        self.turns[player.0 as usize].last_element
    }

    /// Chain bonus (§4) of the card waiting in the Target window that
    /// concerns `player`.
    pub fn pending_bonus(&self, player: PlayerId) -> Option<u8> {
        self.window_for(player)?.pending.map(|p| p.bonus)
    }

    /// Players the game is waiting on right now: those with a window to
    /// answer, and those free to take their turn.
    pub fn awaiting(&self) -> Vec<PlayerId> {
        let wishing = self.wishing();
        let choosing = self.choosing();
        self.order
            .iter()
            .copied()
            .filter(|&p| {
                self.answering(p).is_some()
                    || self.free_to_act(p)
                    || wishing.contains(&p)
                    || choosing.contains(&p)
            })
            .collect()
    }

    pub fn hand(&self, player: PlayerId) -> &[CardId] {
        self.hands
            .get(player.0 as usize)
            .map_or(&[][..], Vec::as_slice)
    }

    pub fn def_id(&self, card: CardId) -> DefId {
        self.defs[card.0 as usize]
    }

    /// The card as it plays: its definition with any change on this copy.
    pub fn def(&self, card: CardId) -> CardDef {
        let def = *self.def_id(card).def();
        match self.mods.get(&card) {
            Some(m) => def.with(m),
            None => def,
        }
    }

    /// The change on this copy of a card, if a wish made one.
    pub fn card_mod(&self, card: CardId) -> Option<&CardMod> {
        self.mods.get(&card)
    }

    /// The card's name: a forged card's own, else its definition's.
    pub fn card_name(&self, card: CardId) -> &str {
        self.mods
            .get(&card)
            .and_then(|m| m.name.as_deref())
            .unwrap_or(self.def_id(card).def().name)
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

    /// Cards spent and waiting to be shuffled back when the deck runs out.
    pub fn discard_len(&self) -> usize {
        self.discard.len()
    }

    /// Every event since the match began, in order.
    pub fn log(&self) -> &[Event] {
        &self.log
    }

    /// The champion in sight on `hex`. A hidden one is not there as far as
    /// anyone can tell (§11.6): walking in stumbles into them instead.
    pub fn occupant(&self, hex: Hex) -> Option<PlayerId> {
        self.champion_at(hex).filter(|&p| !self.is_hidden(p))
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
        let tile = self
            .board
            .tile(to)
            .filter(|t| t.terrain.is_land())
            .ok_or(RuleError::OffBoard)?;
        if champion.hex.unsigned_distance_to(to) != 1 {
            return Err(RuleError::NotAdjacent);
        }
        if self.occupant(to).is_some() || (self.mob_at(to) && !self.lets_pass(player, to)) {
            return Err(RuleError::Occupied);
        }
        Ok(self.terrain_cost(player, tile.terrain))
    }

    /// Hexes `player` can reach this turn, with the cheapest cost.
    pub fn reachable(&self, player: PlayerId) -> HashMap<Hex, u32> {
        self.paths(player)
            .into_iter()
            .map(|(h, (c, _))| (h, c))
            .collect()
    }

    /// Cheapest path for `player` to `to`, excluding the start hex.
    pub fn path_to(&self, player: PlayerId, to: Hex) -> Option<Vec<Hex>> {
        let paths = self.paths(player);
        paths.get(&to)?;
        let start = self.hex_of(player);
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
    /// hex → (cost, previous hex). Empty unless `player` is free to act.
    fn paths(&self, player: PlayerId) -> HashMap<Hex, (u32, Hex)> {
        let start = self.hex_of(player);
        let mut best: HashMap<Hex, (u32, Hex)> = HashMap::new();
        if !self.free_to_act(player) {
            return best;
        }
        let points = self.move_points(player);
        let mut queue = BinaryHeap::new();
        queue.push(std::cmp::Reverse((0u32, start.x(), start.y())));
        while let Some(std::cmp::Reverse((cost, x, y))) = queue.pop() {
            let at = Hex::new(x, y);
            if at != start && best.get(&at).is_some_and(|&(c, _)| c < cost) {
                continue;
            }
            for next in at.all_neighbors() {
                let Some(tile) = self.board.tile(next).filter(|t| t.terrain.is_land()) else {
                    continue;
                };
                // Militia who let one through are a hex to end on, not to pass.
                let passing = self.lets_pass(player, next);
                if next == start || self.occupant(next).is_some() || (self.mob_at(next) && !passing)
                {
                    continue;
                }
                let total = cost + self.terrain_cost(player, tile.terrain);
                if total > points {
                    continue;
                }
                if best.get(&next).is_none_or(|&(c, _)| total < c) {
                    best.insert(next, (total, at));
                    // A trial stops the walk: no path goes on through it.
                    if passing || self.trial_for(player, next).is_some() {
                        continue;
                    }
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
        match self.answering(player) {
            Some(i) => {
                let fits = match self.windows[i].kind {
                    WindowKind::Target { .. } => def.timing == Timing::Response,
                    // Cards go into a battle only as burned faces.
                    WindowKind::Battle { .. }
                    | WindowKind::GuardBattle { .. }
                    | WindowKind::MobBattle { .. }
                    | WindowKind::MilitiaBattle { .. }
                    | WindowKind::Trial { .. } => false,
                    WindowKind::Enter { .. } => def.timing == Timing::Instant,
                    // Tribute: any card of the hand may be given, free.
                    WindowKind::Tribute { .. } => return Ok(()),
                };
                if !fits {
                    return Err(RuleError::WrongTiming);
                }
            }
            None => {
                if self.windows.iter().any(|w| w.eligible.contains(&player)) {
                    return Err(RuleError::AlreadyChose);
                }
                if !self.free_to_act(player) {
                    return Err(if self.windows.iter().any(|w| w.actor == player) {
                        RuleError::WindowOpen
                    } else {
                        RuleError::NotYourTurn
                    });
                }
                if def.timing == Timing::Response {
                    return Err(RuleError::WrongTiming);
                }
            }
        }
        let have = self.champions[player.0 as usize].spirit_points;
        let need = self.cost_of(player, card);
        if need > have {
            return Err(RuleError::NotEnoughSpirit { need, have });
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
                // A hidden rival is not there to aim at (§11.6).
                .filter(|&p| p == player || !self.is_hidden(p))
                .filter(|&p| self.hex_of(p).unsigned_distance_to(me) <= range)
                .map(Target::Champion)
                .collect(),
            TargetRule::Enemy { range } => self
                .players()
                .filter(|&p| p != player && !self.is_hidden(p))
                .filter(|&p| self.hex_of(p).unsigned_distance_to(me) <= range)
                .map(Target::Champion)
                .collect(),
            TargetRule::EmptyHex { range } => self
                .board
                .tiles()
                .filter(|(h, t)| {
                    h.unsigned_distance_to(me) <= range
                        && t.terrain.is_land()
                        && self.occupant(*h).is_none()
                        && !self.mob_at(*h)
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
                let window = self.answering(player).map(|i| &self.windows[i]);
                if window.is_some_and(|w| w.pending.is_some()) {
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
        // A wish is sealed any time of the day, turn or no turn (§21.4).
        if let Intent::Wish { god, wish, said } = intent {
            let sealed = dusk::SealedWish { god, wish, said };
            self.seal_wish(player, dusk::Seal::Wish(Some(sealed)), &mut events)?;
        } else if intent == Intent::RefuseWish {
            self.seal_wish(player, dusk::Seal::Refused, &mut events)?;
        } else if let Intent::ChooseDeed { deed } = intent {
            self.choose_deed(player, deed, &mut events)?;
        } else if let Some(i) = self.answering(player) {
            self.apply_in_window(i, player, intent, &mut events)?;
        } else {
            match intent {
                Intent::Pass | Intent::Burn { .. } => {
                    return Err(
                        if self.windows.iter().any(|w| w.eligible.contains(&player)) {
                            RuleError::AlreadyChose
                        } else {
                            RuleError::NoWindow
                        },
                    );
                }
                _ => {}
            }
            if !self.free_to_act(player) {
                return Err(if self.windows.iter().any(|w| w.actor == player) {
                    RuleError::WindowOpen
                } else if self.windows.iter().any(|w| w.eligible.contains(&player)) {
                    RuleError::AlreadyChose
                } else {
                    RuleError::NotYourTurn
                });
            }
            self.act_own(player, intent, &mut events)?;
        }
        self.release_held(&mut events);
        self.end_round_when_done(&mut events);
        self.note_life(&events);
        self.settle_story(&mut events);
        self.check_victory(&mut events);
        self.log.extend(events.iter().cloned());
        Ok(events)
    }

    /// An intent on `player`'s own turn: checked, then held if it touches a
    /// rival who is still acting (§11.2), else played.
    fn act_own(
        &mut self,
        player: PlayerId,
        intent: Intent,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        if intent == Intent::EndTurn {
            self.end_turn(player, events);
            return Ok(());
        }
        self.check_own(player, &intent)?;
        if let Some(on) = self.blocker(player, &intent) {
            events.push(Event::Held { player, on });
            self.turns[player.0 as usize].phase = Phase::Held { on, intent };
            return Ok(());
        }
        self.play_out(player, intent, events)
    }

    fn play_out(
        &mut self,
        player: PlayerId,
        intent: Intent,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        match intent {
            Intent::Move { to } => self.step(player, to, events),
            Intent::Play { card, target } => self.play_own(player, card, target, events),
            Intent::Sacrifice { slot } => self.sacrifice(player, slot, events),
            Intent::Rebuild => self.rebuild(player, events),
            Intent::Cycle { cards } => {
                self.cycle(player, &cards, events);
                Ok(())
            }
            _ => Err(RuleError::WrongTiming),
        }
    }

    /// Whether a step or a card of `player` is legal now, without doing it.
    fn check_own(&self, player: PlayerId, intent: &Intent) -> Result<(), RuleError> {
        match *intent {
            Intent::Move { to } => {
                if self.occupant(to).is_some_and(|d| d != player) || self.mob_at(to) {
                    self.attack_cost(player, to).map(|_| ())
                } else {
                    let cost = self.step_cost(player, to)?;
                    let have = self.move_points(player);
                    if cost > have {
                        return Err(RuleError::NotEnoughMovePoints { need: cost, have });
                    }
                    Ok(())
                }
            }
            Intent::Play { card, target } => self.check_play(player, card, target),
            Intent::Sacrifice { slot } => self.check_sacrifice(player, slot).map(|_| ()),
            Intent::Rebuild => self.check_rebuild(player).map(|_| ()),
            Intent::Cycle { ref cards } => self.check_cycle(player, cards),
            _ => Err(RuleError::WrongTiming),
        }
    }

    /// Who an own-turn intent of `player` would draw in: the defender of a
    /// battle, the rivals an Enter window opens for, the target of a card
    /// and those near it. The flag marks a hidden one stumbled upon, who
    /// fights at once even mid-turn (§11.6).
    fn touched(&self, player: PlayerId, intent: &Intent) -> Vec<(PlayerId, bool)> {
        match *intent {
            Intent::Move { to } => {
                if self.mob_at(to) {
                    // The guard is nobody's turn: it answers at once.
                    Vec::new()
                } else if let Some(d) = self.occupant(to).filter(|&d| d != player) {
                    vec![(d, false)]
                } else if let Some(h) = self.hidden_at(to).filter(|&h| h != player) {
                    vec![(h, true)]
                } else if self.is_hidden(player) {
                    Vec::new()
                } else {
                    self.watchers(player, to)
                        .into_iter()
                        .map(|p| (p, false))
                        .collect()
                }
            }
            Intent::Play {
                target: Target::Champion(aimed),
                ..
            } if aimed != player => std::iter::once(aimed)
                .chain(
                    self.watchers(player, self.hex_of(aimed))
                        .into_iter()
                        .filter(|&p| p != aimed),
                )
                .map(|p| (p, false))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Why an intent of `player` must wait, if it must: `Some(on)` with the
    /// rival still acting, or busy in another window (`None` where naming
    /// them would give a hidden one away).
    fn blocker(&self, player: PlayerId, intent: &Intent) -> Option<Option<PlayerId>> {
        for (p, hidden) in self.touched(player, intent) {
            let busy = self
                .windows
                .iter()
                .any(|w| w.actor == p || w.eligible.contains(&p));
            if busy {
                return Some((!hidden).then_some(p));
            }
            if !hidden && self.phase(p) == &Phase::Acting {
                return Some(Some(p));
            }
        }
        None
    }

    /// Held actions whose way is clear go ahead, in initiative order, or are
    /// dropped when no longer legal.
    fn release_held(&mut self, events: &mut Vec<Event>) {
        loop {
            let mut moved = false;
            for p in self.order.clone() {
                if self.winner.is_some() {
                    return;
                }
                let Phase::Held { intent, .. } = self.phase(p).clone() else {
                    continue;
                };
                let busy = self
                    .windows
                    .iter()
                    .any(|w| w.actor == p || w.eligible.contains(&p));
                if busy || self.blocker(p, &intent).is_some() {
                    continue;
                }
                self.turns[p.0 as usize].phase = Phase::Acting;
                events.push(Event::Resumed { player: p });
                let result = self
                    .check_own(p, &intent)
                    .and_then(|()| self.play_out(p, intent, events));
                if let Err(why) = result {
                    events.push(Event::HoldDropped { player: p, why });
                }
                moved = true;
            }
            if !moved {
                return;
            }
        }
    }

    /// With everyone done and no window open, the world acts and the next
    /// round begins. After a day, dusk comes first (§21.4): the day's
    /// accounts, the Crown, everyone's wish, the gods' answers and the
    /// tributes they ask, then the gods shift. Dusk waits on people twice
    /// (`DuskStep`); each intent moves it on as far as it can go.
    fn end_round_when_done(&mut self, events: &mut Vec<Event>) {
        if self.winner.is_some() || !self.windows.is_empty() {
            return;
        }
        match self.dusk {
            None => {
                if !self.turns.iter().all(|t| t.phase == Phase::Done) {
                    return;
                }
                self.world_phase(events);
                if self.time != TimeOfDay::Day {
                    self.start_round(events);
                    return;
                }
                events.push(Event::Dusk { round: self.round });
                // A deed on its eve is done now, before anything else (§21.7).
                self.dusk_of_deeds(events);
                if self.winner.is_some() {
                    return;
                }
                self.settle_the_day(events);
                self.judge_the_day(events);
                self.begin_dusk(events);
                if self.dusk.is_none() {
                    self.end_dusk(events);
                } else {
                    self.end_round_when_done(events);
                }
            }
            Some(dusk::DuskStep::Sealing) => {
                if !self.wishing().is_empty() {
                    return;
                }
                self.answer_wishes(events);
                self.dusk = Some(dusk::DuskStep::Answering);
                self.end_round_when_done(events);
            }
            Some(dusk::DuskStep::Answering) => {
                self.dusk = None;
                self.end_dusk(events);
            }
        }
    }

    /// The gods shift, trials and stories come, and the night begins.
    fn end_dusk(&mut self, events: &mut Vec<Event>) {
        self.shift_stages(events);
        if self.scripted.is_none() {
            if self.has(Feature::Trials) {
                self.trials_at_dusk(events);
            }
            self.storyteller(events);
        }
        self.start_round(events);
    }

    fn step(
        &mut self,
        player: PlayerId,
        to: Hex,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        if self.guard_at(to) {
            let cost = self.attack_cost(player, to)?;
            self.start_guard_battle(player, cost, events);
            return Ok(());
        }
        if let Some(id) = self.mob_on(to).map(|u| u.id) {
            let cost = self.attack_cost(player, to)?;
            self.start_mob_battle(player, id, cost, events);
            return Ok(());
        }
        // The militia let through those they hold nothing against: they trade
        // places and the step goes on; the others have to fight (§20.4).
        if let Some(home) = self.militia_at(to) {
            if self.lets_pass(player, to) {
                self.trade_places(player, to, events);
            } else {
                let cost = self.attack_cost(player, to)?;
                self.start_militia_battle(player, home, cost, events);
                return Ok(());
            }
        }
        if let Some(defender) = self.occupant(to)
            && defender != player
        {
            let cost = self.attack_cost(player, to)?;
            // Out of the shadow: an ambush, and then everyone sees them.
            let ambush = self.is_hidden(player).then_some(player);
            if ambush.is_some() {
                self.reveal(player, RevealReason::Attacked, events);
            }
            self.start_battle(player, player, defender, cost, ambush, events);
            return Ok(());
        }
        let cost = self.step_cost(player, to)?;
        let have = self.move_points(player);
        if cost > have {
            return Err(RuleError::NotEnoughMovePoints { need: cost, have });
        }
        if let Some(hidden) = self.hidden_at(to)
            && hidden != player
        {
            self.stumble(player, hidden, to, events);
            return Ok(());
        }
        let from = self.hex_of(player);
        self.champ_mut(player).hex = to;
        self.turns[player.0 as usize].move_points -= cost;
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
            // People live there: nobody walks in unseen.
            if self.board.tile(at).is_some_and(|t| t.terrain.crowded()) {
                self.reveal(player, RevealReason::Crowd, events);
            }
            self.claim(player, at, events);
            self.pick_up(player, at, events);
        }
        // A trial stops whoever walks onto it. Out in the open it brings them
        // into view; under cover (woods, groves, swamps) the one in hiding
        // stays there, and the trial is theirs alone to know of (§20.2).
        let trial = at == to && self.trial_for(player, at).is_some();
        let cover = self.board.tile(at).is_some_and(|t| t.terrain.gives_cover());
        if trial && !cover && self.is_hidden(player) {
            self.reveal(player, RevealReason::Trial, events);
        }
        // Nobody sees a hidden champion walk by, so nobody reacts to it.
        if !self.is_hidden(player) {
            let near = self.watchers(player, at);
            self.open_window(
                player,
                WindowKind::Enter {
                    mover: player,
                    hex: at,
                },
                near,
                None,
                None,
                events,
            );
        }
        if trial {
            self.begin_trial(player, at, events);
        }
        Ok(())
    }

    fn end_turn(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        events.push(Event::TurnEnded { player });
        self.turns[player.0 as usize].phase = Phase::Done;
        // Ending the turn on a temple is a prayer to its god.
        let hex = self.hex_of(player);
        if let Some(tile) = self.board.tile(hex)
            && tile.terrain == Terrain::Temple
            && let Some(god) = tile.region
        {
            self.offer(Some(player), god, 1, events);
            self.record_deed(player, style::Deed::Prayed);
            self.cure(player, Cure::Temple, events);
        }
        self.militia_at_turn_end(player, events);
        self.stealth_at_turn_end(player, events);
    }

    /// `player` may go through their hand now (`Intent::Cycle`).
    pub fn may_cycle(&self, player: PlayerId) -> bool {
        self.free_to_act(player)
            && !self.turns[player.0 as usize].cycled
            && !self.hand(player).is_empty()
    }

    /// Whether `player` may let `cards` go now: once a turn, cards of the hand.
    fn check_cycle(&self, player: PlayerId, cards: &[CardId]) -> Result<(), RuleError> {
        let hand = self.hand(player);
        let mut seen = Vec::new();
        let fine = !self.turns[player.0 as usize].cycled
            && !cards.is_empty()
            && cards.iter().all(|c| {
                let fresh = !seen.contains(c);
                seen.push(*c);
                fresh && hand.contains(c)
            });
        if fine {
            Ok(())
        } else {
            Err(RuleError::NoCycle)
        }
    }

    /// The cards go to the discard and fresh ones come: one fewer, as many
    /// at a temple, where the gods listen (§21.2).
    fn cycle(&mut self, player: PlayerId, cards: &[CardId], events: &mut Vec<Event>) {
        self.turns[player.0 as usize].cycled = true;
        self.hands[player.0 as usize].retain(|c| !cards.contains(c));
        self.discard.extend_from_slice(cards);
        let at_temple = self
            .board
            .tile(self.hex_of(player))
            .is_some_and(|t| t.terrain == Terrain::Temple);
        let draw = cards.len() - usize::from(!at_temple);
        events.push(Event::Cycled {
            player,
            let_go: cards.len() as u8,
            drawn: draw as u8,
        });
        self.draw(player, draw, events);
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
        self.hostile_play(player, card, target, events);
        // Zaga's Burden: past the second card a turn, every card is loud.
        let turn = &mut self.turns[player.0 as usize];
        turn.cards = turn.cards.saturating_add(1);
        if turn.cards > BURDEN_FREE
            && self.law_active(Law::Burden)
            && !self.chosen(player, God::Zaga)
        {
            events.push(Event::Law {
                law: Law::Burden,
                player: Some(player),
                hex: None,
            });
            self.add_threat(player, 1, events);
        }
        let bonus = self.chain(player, self.def(card).element, events);

        if let Target::Champion(aimed) = target
            && aimed != player
        {
            // Aiming at a rival gives away where you stand.
            self.reveal(player, RevealReason::Aimed, events);
            let mut eligible = vec![aimed];
            let at = self.hex_of(aimed);
            eligible.extend(
                self.watchers(player, at)
                    .into_iter()
                    .filter(|&p| p != aimed),
            );
            let pending = Pending {
                caster: player,
                card,
                target,
                bonus,
                canceled: false,
            };
            self.open_window(
                player,
                WindowKind::Target {
                    caster: player,
                    target: aimed,
                    card,
                },
                eligible,
                Some(pending),
                None,
                events,
            );
        } else {
            self.resolve(player, card, target, bonus, events);
        }
        Ok(())
    }

    fn apply_in_window(
        &mut self,
        i: usize,
        player: PlayerId,
        intent: Intent,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let tribute = matches!(self.windows[i].kind, WindowKind::Tribute { .. });
        let battle = matches!(
            self.windows[i].kind,
            WindowKind::Battle { .. }
                | WindowKind::GuardBattle { .. }
                | WindowKind::MobBattle { .. }
                | WindowKind::MilitiaBattle { .. }
                | WindowKind::Trial { .. }
        );
        let choice = match intent {
            Intent::Pass => Choice::Pass,
            // Tribute: any card of one's hand, given, not played.
            Intent::Play { card, .. } if tribute => {
                if !self.hand(player).contains(&card) {
                    return Err(RuleError::NotInHand);
                }
                self.hands[player.0 as usize].retain(|&c| c != card);
                Choice::Play(card, Target::None)
            }
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
            Intent::Move { .. }
            | Intent::EndTurn
            | Intent::Sacrifice { .. }
            | Intent::Rebuild
            | Intent::Cycle { .. } => {
                return Err(RuleError::WindowOpen);
            }
            Intent::Wish { .. } | Intent::RefuseWish => return Err(RuleError::InvalidWish),
            Intent::ChooseDeed { .. } => return Err(RuleError::InvalidDeed),
        };
        let window = &mut self.windows[i];
        window.choices.insert(player, choice);
        events.push(Event::ChoiceMade { player });
        if window.eligible.iter().all(|p| window.has_chosen(*p)) {
            self.close_window(i, events);
        }
        Ok(())
    }

    fn open_window(
        &mut self,
        actor: PlayerId,
        kind: WindowKind,
        eligible: Vec<PlayerId>,
        pending: Option<Pending>,
        ambush: Option<PlayerId>,
        events: &mut Vec<Event>,
    ) {
        if eligible.is_empty() {
            // Nobody may react: a pending card resolves at once.
            if let Some(p) = pending {
                self.resolve(p.caster, p.card, p.target, p.bonus, events);
            }
            return;
        }
        events.push(Event::WindowOpened {
            kind,
            eligible: eligible.clone(),
        });
        self.windows.push(Window {
            kind,
            actor,
            eligible,
            choices: BTreeMap::new(),
            pending,
            ambush,
        });
    }

    /// Resolves the `i`th window: answers in initiative order, then the card
    /// it waited on, or the battle. The window stays in place until then, so
    /// a cancel finds its card and a battle its ambush.
    fn close_window(&mut self, i: usize, events: &mut Vec<Event>) {
        let window = self.windows[i].clone();
        if let WindowKind::Tribute { asker } = window.kind {
            events.push(Event::WindowClosed {
                kind: window.kind,
                played: Vec::new(),
            });
            self.settle_tribute(asker, &window, events);
            self.windows.remove(i);
            return;
        }
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
            self.hostile_play(player, card, target, events);
            self.resolve(player, card, target, 0, events);
        }

        if let Some(p) = self.windows[i].pending {
            if p.canceled {
                self.discard.push(p.card);
            } else {
                self.resolve(p.caster, p.card, p.target, p.bonus, events);
            }
        }
        if let WindowKind::Battle { attacker, defender } = window.kind {
            let burned = |p: PlayerId| match window.choices.get(&p) {
                Some(Choice::Burn(cards)) => cards.clone(),
                _ => Vec::new(),
            };
            let (a, d) = (burned(attacker), burned(defender));
            self.resolve_battle(attacker, defender, a, d, events);
            // A battle ends the movement of whoever brought it about.
            if self.is_active(window.actor) {
                self.turns[window.actor.0 as usize].move_points = 0;
            }
        }
        if let WindowKind::MilitiaBattle { attacker, home } = window.kind {
            let burned = match window.choices.get(&attacker) {
                Some(Choice::Burn(cards)) => cards.clone(),
                _ => Vec::new(),
            };
            self.resolve_militia_battle(attacker, home, burned, events);
            if self.is_active(attacker) {
                self.turns[attacker.0 as usize].move_points = 0;
            }
        }
        if let WindowKind::MobBattle { attacker, id } = window.kind {
            let burned = match window.choices.get(&attacker) {
                Some(Choice::Burn(cards)) => cards.clone(),
                _ => Vec::new(),
            };
            self.resolve_mob_battle(attacker, id, burned, events);
            if self.is_active(attacker) {
                self.turns[attacker.0 as usize].move_points = 0;
            }
        }
        if let WindowKind::GuardBattle { attacker } = window.kind {
            let burned = match window.choices.get(&attacker) {
                Some(Choice::Burn(cards)) => cards.clone(),
                _ => Vec::new(),
            };
            self.resolve_guard_battle(attacker, burned, events);
            if self.is_active(attacker) {
                self.turns[attacker.0 as usize].move_points = 0;
            }
        }
        if let WindowKind::Trial { player, hex } = window.kind {
            let burned = match window.choices.get(&player) {
                Some(Choice::Burn(cards)) => cards.clone(),
                _ => Vec::new(),
            };
            self.resolve_trial(player, hex, burned, events);
        }
        self.windows.remove(i);
    }

    fn cancel_pending(&mut self, by: CardId, events: &mut Vec<Event>) {
        let by_element = self.def(by).element;
        // The window the cancelling card was answered in.
        let Some(window) = self.windows.iter_mut().find(|w| {
            w.pending.is_some()
                && w.choices
                    .values()
                    .any(|c| matches!(c, Choice::Play(card, _) if *card == by))
        }) else {
            return;
        };
        let pending = window.pending.as_mut().expect("found by its pending card");
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
        let cost = self.cost_of(player, card);
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
        let turn = &mut self.turns[player.0 as usize];
        let prev = std::mem::replace(&mut turn.last_element, element);
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
        }
        if let Some(feature) = def.needs() {
            self.first(caster, novelty::Novelty::PlayedFor(feature), events);
        }
        if let Some(element) = def.element {
            self.lift_curse(caster, element, events);
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
                    self.mend(caster, n(x), def.element, events);
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
                    self.mend(t, n(x), def.element, events);
                }
            }
            Effect::Ward => {
                if let (Some(t), Some(element)) = (aimed, def.element) {
                    self.raise_ward(t, element, events);
                }
            }
            Effect::Hide => self.hide(caster, events),
            Effect::Haste(x) => {
                if self.is_active(caster) {
                    let turn = &mut self.turns[caster.0 as usize];
                    turn.move_points += u32::from(n(x));
                    events.push(Event::Hasted {
                        player: caster,
                        move_points: turn.move_points,
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
                // Fog drops the caster onto someone hiding there: the step
                // fails and the one in hiding is seen.
                if let Target::Hex(to) = target
                    && let Some(hidden) = self.hidden_at(to)
                {
                    events.push(Event::Stumbled {
                        mover: caster,
                        hidden,
                        hex: to,
                    });
                    self.reveal(hidden, RevealReason::Stumbled, events);
                } else if let Target::Hex(to) = target {
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
                        self.pick_up(caster, to, events);
                    }
                }
            }
            Effect::Cancel => self.cancel_pending(card, events),
            Effect::BodyFuel => {
                self.take_corpse(caster, events);
                self.gain_spirit(caster, n(2), events);
                if self.is_active(caster) {
                    let turn = &mut self.turns[caster.0 as usize];
                    turn.move_points += 1;
                    events.push(Event::Hasted {
                        player: caster,
                        move_points: turn.move_points,
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
                self.mend(caster, n(2), def.element, events);
            }
            Effect::BodyRest => {
                self.take_corpse(caster, events);
                self.mend(caster, n(1), def.element, events);
                self.draw(caster, 1, events);
            }
            Effect::BodySeed => {
                let hex = self.hex_of(caster);
                let hero = self
                    .board
                    .tile(hex)
                    .is_some_and(|t| t.corpse.is_some_and(|c| c.hero));
                self.take_corpse(caster, events);
                self.grow(hex, events);
                if hero
                    && self
                        .board
                        .tile(hex)
                        .is_some_and(|t| t.terrain == Terrain::Grove)
                {
                    self.hero_groves.insert((hex.x(), hex.y()));
                }
                self.mend(caster, n(2), def.element, events);
            }
            Effect::Feast => self.feast(caster, n(1), events),
            Effect::Poison(x) => {
                if let (Some(t), Some(element)) = (aimed, def.element)
                    && self.pierce(t, def.element, events)
                {
                    self.poison(t, element, n(x), events);
                }
            }
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
        let through = match self.champions[target.0 as usize].ward {
            None => true,
            Some(ward) if element == Some(ward.quenched_by()) => {
                self.champ_mut(target).ward = None;
                events.push(Event::WardBroken {
                    player: target,
                    ward,
                    by: ward.quenched_by(),
                });
                true
            }
            Some(ward) => {
                events.push(Event::Blocked {
                    player: target,
                    ward,
                });
                false
            }
        };
        // What gets through breaks a worn item it quenches, as a ward (§20.3).
        if through && let Some(e) = element {
            self.crack_item(target, e, events);
        }
        through
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
            self.turns[player.0 as usize].move_points = 0;
        } else {
            self.champ_mut(player).rooted = true;
        }
        events.push(Event::Rooted { player });
    }

    fn grow(&mut self, hex: Hex, events: &mut Vec<Event>) {
        if !self.has(Feature::Groves) {
            return;
        }
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
                    Effect::Trap(TrapEffect::Poison(x)) => {
                        let shift = self.stage_shift(def.element, true);
                        let x = (i16::from(x) + i16::from(shift)).max(1) as u8;
                        if let Some(element) = def.element {
                            self.poison(victim, element, x, events);
                        }
                    }
                    _ => {}
                }
            }
            self.discard.push(trap.card);
        }
    }

    /// Zero health: the champion leaves a body and wakes at home, whole.
    fn fall(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        self.note_bet(player, wish::Bet::Fall);
        let at = self.hex_of(player);
        let bodies = self.has(Feature::Bodies);
        if bodies
            && let Some(tile) = self.board.tile_mut(at)
            && tile.corpse.is_none()
        {
            // A champion's body: what grows of it may be a World Tree.
            tile.corpse = Some(Corpse { age: 0, hero: true });
        }
        let home = self.board.start_of(self.champions[player.0 as usize].god);
        let respawn = (0..=self.board.extent() * 2)
            .flat_map(|r| home.ring(r).collect::<Vec<_>>())
            .find(|&h| {
                self.board.contains(h)
                    && self.champion_at(h).is_none_or(|p| p == player)
                    && !self.mob_at(h)
            })
            .unwrap_or(home);
        self.drop_on_fall(player, at, events);
        // Whoever falls wakes at home, in plain sight.
        self.reveal(player, stealth::RevealReason::Stumbled, events);
        let champ = self.champ_mut(player);
        champ.hex = respawn;
        champ.seen_at = respawn;
        champ.hp = champ.body;
        champ.ward = None;
        champ.rooted = false;
        champ.poison = None;
        // Maya's Rest: death is a release, the Spirit comes back whole.
        if self.law_active(Law::Rest) {
            let champ = self.champ_mut(player);
            champ.spirit_points = champ.spirit;
            let spirit = champ.spirit_points;
            events.push(Event::Law {
                law: Law::Rest,
                player: Some(player),
                hex: None,
            });
            events.push(Event::SpiritChanged { player, spirit });
        }
        // Maya's Wrath: the fallen leave something behind.
        if self.law_active(Law::Wrath)
            && !self.chosen(player, God::Maya)
            && !self.hands[player.0 as usize].is_empty()
        {
            let hand = self.hands[player.0 as usize].clone();
            if let Some(&card) = self.rng.pick(&hand) {
                self.hands[player.0 as usize].retain(|&c| c != card);
                self.discard.push(card);
                events.push(Event::Law {
                    law: Law::Wrath,
                    player: Some(player),
                    hex: None,
                });
            }
        }
        if self.is_active(player) {
            self.turns[player.0 as usize].move_points = 0;
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
            // A curse a wish hid there (§7.3).
            self.spring_curse(player, card, events);
        }
    }

    fn refill_hand(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        if self.scripted.is_some() {
            return;
        }
        let limit = self.hand_limit(player);
        let have = self.hands[player.0 as usize].len();
        self.draw(player, limit.saturating_sub(have), events);
    }

    // ---- Rounds and turns ----

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
        events.push(Event::RoundStarted {
            round: self.round,
            time: self.time,
            order: self.order.clone(),
        });
        if self.time == TimeOfDay::Day {
            events.push(Event::Dawn { round: self.round });
            if self.scripted.is_none_or(|s| s.dawn) {
                self.dawn(events);
            }
        }
        // Everyone takes their turn at once (§11.2).
        for player in self.order.clone() {
            self.start_turn(player, events);
        }
    }

    fn start_turn(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        self.turns[player.0 as usize] = Turn {
            phase: Phase::Acting,
            move_points: MOVE_POINTS + self.stride(player),
            last_element: None,
            cards: 0,
            cycled: false,
        };
        let champ = self.champ_mut(player);
        let faded = champ.ward.take().is_some();
        let rooted = std::mem::take(&mut champ.rooted);
        if faded {
            events.push(Event::WardFaded { player });
        }
        // Trishna's Generosity: the settlement feeds its guest (§5.3).
        let at = self.hex_of(player);
        if self.law_active(Law::Generosity)
            && self
                .board
                .tile(at)
                .is_some_and(|t| t.terrain == Terrain::Settlement)
        {
            let c = &self.champions[player.0 as usize];
            if c.hp < c.body {
                events.push(Event::Law {
                    law: Law::Generosity,
                    player: Some(player),
                    hex: Some(at),
                });
                self.heal(player, 1, events);
            }
        }
        if rooted {
            self.turns[player.0 as usize].move_points = 0;
        }
        events.push(Event::TurnStarted {
            player,
            move_points: self.move_points(player),
        });
        self.bite_curses(player, events);
        self.bite_poison(player, events);
        self.gear_at_turn_start(player, events);
        self.militia_at_turn_start(player, events);
        let champ = &self.champions[player.0 as usize];
        if champ.spirit_points < champ.spirit {
            self.gain_spirit(player, 1, events);
        }
        self.refill_hand(player, events);
    }

    /// Corpses age and sprout; the night leaves a new one behind.
    fn world_phase(&mut self, events: &mut Vec<Event>) {
        let corpses: Vec<(Hex, Corpse)> = self.board.corpses().collect();
        let ripe = self.grove_age();
        let groves = self.has(Feature::Groves);
        for (hex, corpse) in corpses {
            let tile = self.board.tile_mut(hex).expect("corpse on the board");
            let age = corpse.age + 1;
            if age < ripe {
                tile.corpse = Some(Corpse { age, ..corpse });
            } else if groves && tile.terrain.can_grow_grove() {
                tile.corpse = None;
                tile.terrain = Terrain::Grove;
                events.push(Event::GroveGrew { hex });
                if corpse.hero {
                    self.hero_groves.insert((hex.x(), hex.y()));
                }
                // An untouched body is Bhava's offering (§3).
                self.offer(None, God::Bhava, 1, events);
            } else {
                tile.corpse = None;
                events.push(Event::CorpseDecayed { hex });
            }
        }
        if self.scripted.is_none() && self.time == TimeOfDay::Night {
            self.spawn_corpse(events);
        }
        if self.scripted.is_none() {
            if self.has(Feature::Undead) {
                self.raise_dead(events);
            }
            if self.time == TimeOfDay::Night && self.has(Feature::Beasts) {
                self.beasts_at_night(events);
            }
            self.mob_phase(events);
        }
        if self.scripted.is_none_or(|s| s.guard) && self.has(Feature::Guard) {
            self.guard_phase(events);
        }
    }

    fn spawn_corpse(&mut self, events: &mut Vec<Event>) {
        if !self.has(Feature::Bodies) {
            return;
        }
        let free: Vec<Hex> = self
            .board
            .tiles()
            .filter(|(h, t)| {
                // Any ground but where people are: where no grove can grow,
                // an untended body rises instead (§20.4).
                t.corpse.is_none()
                    && t.terrain.is_land()
                    && !t.terrain.crowded()
                    && self.occupant(*h).is_none()
                    && !self.mob_at(*h)
                    && !God::ALL.iter().any(|g| self.board.start_of(*g) == *h)
            })
            .map(|(h, _)| h)
            .collect();
        if let Some(&hex) = self.rng.pick(&free) {
            self.board.tile_mut(hex).expect("free hex").corpse = Some(Corpse::fresh());
            events.push(Event::CorpseAppeared { hex });
        }
    }
}

mod battle;
mod beasts;
mod creation;
mod dusk;
mod gear;
mod guard;
mod laws;
mod militia;
mod mobs;
mod novelty;
mod poison;
mod scenario;
mod stealth;
mod story;
mod style;
mod trial;
mod victory;
mod view;
mod wish;
mod world;
pub use battle::Score;
pub use beasts::{BEAST_DICE, BEAST_HEALTH, BEAST_RANGE};
pub use dusk::{DuskStep, Seal, SealedWish};
pub use gear::{Gain, SACRIFICE};
pub use guard::{GUARD_DICE, GUARD_HEALTH, GUARD_RELIEF, GUARD_STEPS, Guard};
pub use laws::{BURDEN_FREE, CHOSEN, CRACK_REACH, Law, Patronage, SENTENCE_THRESHOLD, SIGN, VOICE};
pub use militia::{
    FRIENDLY, HOSTILE, MILITIA, MILITIA_PASS, Militia, MilitiaWhy, PURSUIT_ROUNDS, REBUILD_SPIRIT,
    REBUILD_STANDING,
};
pub use mobs::{
    MAX_UNDEAD, Mob, MobKind, UNDEAD_AGE, UNDEAD_AGE_DARK, UNDEAD_DICE, UNDEAD_HEALTH, UNDEAD_SIGHT,
};
pub use novelty::{FIRST_STYLE, Novelty, VARIETY};
pub use poison::{Cure, Poison};
pub use scenario::{Scenario, SceneSeat, SceneWorld};
pub use stealth::RevealReason;
pub use story::{Goal, LINE_ROUNDS, Line, LineKind, MAX_OPEN, WorldStir};
pub use style::{BodyVerb, Character, Deed, GUARD_THRESHOLD, StyleReason, Taste, TasteKind};
pub use trial::{Boon, TRIAL_ROUNDS, TRIALS_ON_BOARD, Trial, trial_face};
pub use victory::{Check, CheckKind, DISSOLVED, GreatDeed, ISLAND, OFFERED, REFUSAL_THREAT};
pub use wish::{
    Act, Bet, FORESEE, FORGED_LINE, FORGED_NAME, MAX_ACTS, Price, Said, TRIBUTE_THREAT, Truce,
    WAGER_STAKE, Wager, Wish, WishKind, forge_template, god_terrain, likes_a_stake, taste_for,
};
pub use world::{Pantheon, STAGE_THRESHOLD, STAGES, TRISHNA_DRIFT};

#[cfg(test)]
mod tests;

impl Game {
    /// Deeds of this intent move story lines; states (a hex reached, favour
    /// gained) are checked after them.
    fn settle_story(&mut self, events: &mut Vec<Event>) {
        for (player, deed) in std::mem::take(&mut self.pending_story) {
            self.story_deed(player, deed, events);
            self.standing_deed(player, deed, events);
        }
        self.check_lines(events);
    }
}
