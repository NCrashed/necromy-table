//! Mechanics as parts of the rules a world may or may not have yet
//! (docs/design.md §21.2). A full world has them all from the first turn;
//! a world being created starts with few, and wishes, twists and stories
//! bring the rest in. What the world has never goes away again: all the
//! settlements may burn, but the rules keep their militia.
//!
//! Each system asks `Game::has` at its entry points (a body left on the
//! ground, the undead rising, a trial set), so a mechanic the world lacks
//! simply never happens.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::board::Terrain;
use crate::gods::God;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Feature {
    /// Bodies on the ground: left by the fallen and the night (§3).
    Bodies,
    /// An untouched body grows into a grove.
    Groves,
    /// Settlements to hold (§6.7).
    Settlements,
    /// Each settlement's militia (§20.4).
    Militia,
    /// Untended bodies rise (§20.4).
    Undead,
    /// The undead lay a settlement nobody guards waste; ruins are built again.
    Ruins,
    /// Poison of an element (§20.1).
    Poison,
    /// Trials on hexes (§20.2).
    Trials,
    /// The loot deck and items (§20.3).
    Loot,
    /// The royal guard marches on the loudest (§6.7).
    Guard,
    /// Slipping out of sight (§11.6).
    Stealth,
    /// Bhava's beasts out of his woods at night.
    Beasts,
}

impl Feature {
    pub const ALL: [Feature; 12] = [
        Feature::Bodies,
        Feature::Groves,
        Feature::Settlements,
        Feature::Militia,
        Feature::Undead,
        Feature::Ruins,
        Feature::Poison,
        Feature::Trials,
        Feature::Loot,
        Feature::Guard,
        Feature::Stealth,
        Feature::Beasts,
    ];

    /// What the world must have before this can come in (§21.2): all of
    /// them.
    pub const fn requires(self) -> &'static [Need] {
        use Need::*;
        match self {
            Feature::Bodies | Feature::Settlements | Feature::Guard => &[],
            Feature::Groves | Feature::Undead => &[Has(Feature::Bodies)],
            Feature::Militia => &[Has(Feature::Settlements)],
            Feature::Ruins => &[Has(Feature::Settlements), Has(Feature::Undead)],
            Feature::Poison => &[Land(&[Terrain::Swamp])],
            Feature::Trials => &[Land(&[Terrain::Stones])],
            Feature::Loot => &[AnyOf(&[Feature::Trials, Feature::Guard])],
            Feature::Stealth => &[Land(&[Terrain::Forest, Terrain::Swamp, Terrain::Grove])],
            Feature::Beasts => &[Land(&[Terrain::Forest])],
        }
    }

    /// The god whose domain it is: the one who brings it in most readily
    /// (§21.2).
    pub const fn domain(self) -> God {
        match self {
            Feature::Groves | Feature::Beasts => God::Bhava,
            Feature::Settlements => God::Trishna,
            Feature::Bodies | Feature::Poison | Feature::Trials => God::Zaga,
            Feature::Militia | Feature::Loot | Feature::Guard => God::Ahamar,
            Feature::Undead | Feature::Ruins | Feature::Stealth => God::Maya,
        }
    }
}

/// One thing a mechanic needs before it can come into the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Has(Feature),
    /// At least one of these.
    AnyOf(&'static [Feature]),
    /// Land of one of these kinds somewhere on the board.
    Land(&'static [Terrain]),
}

/// How a match's world begins (§21).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Every mechanic from the first turn.
    #[default]
    Full,
    /// A small world with little in it, which the players grow.
    Creation,
}

/// The mechanics a world has.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    features: BTreeSet<Feature>,
}

impl World {
    pub fn full() -> World {
        World {
            features: Feature::ALL.into_iter().collect(),
        }
    }

    pub fn of(features: impl IntoIterator<Item = Feature>) -> World {
        World {
            features: features.into_iter().collect(),
        }
    }

    pub fn has(&self, feature: Feature) -> bool {
        self.features.contains(&feature)
    }

    pub fn features(&self) -> impl Iterator<Item = Feature> + '_ {
        self.features.iter().copied()
    }

    /// Bring a mechanic in; false if the world had it already.
    pub fn add(&mut self, feature: Feature) -> bool {
        self.features.insert(feature)
    }
}
