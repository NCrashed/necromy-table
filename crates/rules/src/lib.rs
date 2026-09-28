//! Rules core of Necromy Table.
//!
//! Pure game logic: no rendering, no I/O, no wall clock. The same seed and the
//! same sequence of intents always produce the same events, so a match can be
//! replayed from its log and later run on an authoritative server
//! (docs/design.md §16–17).
//!
//! Flow: a client sends an [`Intent`] for a player, [`Game::apply`] validates it
//! and returns the [`Event`]s it caused. Nothing else mutates the state.

pub mod board;
pub mod bot;
pub mod game;
pub mod gods;
pub mod rng;

pub use board::{Board, Corpse, Terrain, Tile};
pub mod cards;

pub use cards::{CardDef, CardId, CardKind, DefId, Effect, TargetRule, Timing};
pub use game::{
    BodyVerb, CHOSEN, Champion, Character, Check, CheckKind, Condition, Cure, Deed, Event, Fighter,
    GUARD_DICE, GUARD_RELIEF, GUARD_STEPS, GUARD_THRESHOLD, Game, Goal, Guard, Intent, Law, Line,
    LineKind, Patronage, Phase, PlayerId, Poison, RevealReason, RuleError, SECRET_FROM_ROUND, SIGN,
    STAGE_THRESHOLD, STAGES, Said, Scenario, SceneSeat, Score, Setup, StyleReason, TRISHNA_DRIFT,
    Target, Taste, TasteKind, TimeOfDay, Trap, VOICE, Window, WindowKind, WishKind, WorldStir,
};
pub use gods::{Element, God};
pub use hexx::Hex;
pub use necromy_dice::Face;
