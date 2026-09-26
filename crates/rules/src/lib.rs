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
    Champion, Event, Game, Intent, PlayerId, RuleError, Setup, Target, TimeOfDay, Trap, Window,
    WindowKind,
};
pub use gods::{Element, God};
pub use hexx::Hex;
