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
pub mod features;
pub mod items;

pub use cards::{CardDef, CardId, CardKind, CardMod, DefId, Effect, TargetRule, Timing};
pub use features::{Feature, Mode, World};
pub use game::Cargo;
pub use game::EARLIEST_EVE;
pub use game::LEGION;
pub use game::RIVER_RUN;
pub use game::{
    Act, BEAST_DICE, BEAST_HEALTH, BEAST_RANGE, Bet, BodyVerb, Boon, CHOSEN, Champion, Character,
    Check, CheckKind, Cure, DISSOLVED, Deed, DuskStep, Event, FIRST_STYLE, FORESEE, FRIENDLY,
    Fighter, GUARD_DICE, GUARD_HEALTH, GUARD_RELIEF, GUARD_STEPS, GUARD_THRESHOLD, Gain, Game,
    Goal, GreatDeed, Guard, HOSTILE, ISLAND, Intent, Law, Line, LineKind, MAX_ACTS, MAX_UNDEAD,
    MILITIA, MILITIA_PASS, Militia, MilitiaWhy, Mob, MobKind, Novelty, OFFERED, PURSUIT_ROUNDS,
    Patronage, Phase, PlayerId, Poison, Price, REBUILD_SPIRIT, RevealReason, RuleError, SACRIFICE,
    SIGN, STAGE_THRESHOLD, STAGES, Said, Scenario, SceneSeat, Score, Seal, SealedWish, Setup,
    StyleReason, TRIAL_ROUNDS, TRIALS_ON_BOARD, TRIBUTE_THREAT, TRISHNA_DRIFT, Target, Taste,
    TasteKind, TimeOfDay, Trap, Trial, Truce, UNDEAD_DICE, UNDEAD_HEALTH, VARIETY, VOICE,
    WAGER_STAKE, Wager, Window, WindowKind, Wish, WishKind, WorldStir, trial_face,
};
pub use game::{BUILD_SPIRIT, Building, CITY, QUARTER_SPIRIT, WALLED_MILITIA};
pub use game::{COMPANION_DICE, Companion, ENLIST_SPIRIT, RETINUE, TAME_SPIRIT};
pub use game::{DEAD_FEASTS, FAIR_DUSKS, FAIR_SPIRIT, Fair};
pub use game::{DOUSE_SPIRIT, Fire, GREAT_FIRE, KINDLE_SPIRIT};
pub use game::{FEAST_FIELDS, FEAST_FOOD, FEAST_GUESTS, GUEST_RANGE, SOW_SPIRIT};
pub use game::{JUNGLE, JUNGLE_RIVER, RIVER};
pub use game::{PAVE_SPIRIT, ROAD_RUN};
pub use gods::{Element, God};
pub use hexx::Hex;
pub use items::{ITEMS, ItemDef, ItemEffect, ItemId, Slot, When};
pub use necromy_dice::Face;
