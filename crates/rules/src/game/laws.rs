//! The laws of the world (docs/design.md §5.3): every stage of every god
//! is one rule for the whole table, so darkening or lightening a god
//! changes how everyone plays. Each law hooks into a system that already
//! exists (stealth, the guard, Threat, bodies, claims, Spirit); the hooks
//! call `law_active` and push `Event::Law` when the law visibly acts.

use serde::{Deserialize, Serialize};

use super::Game;
use crate::gods::God;

/// One god's stage as a rule of the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Law {
    /// Bhava, light: bodies sprout into groves a round sooner.
    Sprout,
    /// Bhava, mid: forest and grove hide by day too.
    Thicket,
    /// Bhava, dark: at dusk a plain in his land runs wild into forest.
    Wildgrowth,
    /// Trishna, light: a settlement feeds whoever starts their turn in it.
    Generosity,
    /// Trishna, mid: whoever deals more in a battle drinks a Spirit.
    Thirst,
    /// Trishna, dark: at dusk bodies in her land burn, and settlements
    /// starve those standing in them.
    Devouring,
    /// Zaga, light: at dusk everyone grows a little quieter.
    Stillness,
    /// Zaga, mid: every card past the second in a turn is loud.
    Burden,
    /// Zaga, dark: the guard marches at a lower Threat and strikes harder,
    /// and poison is catching (`bite_poison`).
    Sentence,
    /// Ahamar, light: holding any land earns Style at dawn.
    Mask,
    /// Ahamar, mid: land pays at dawn only if its owner is near.
    Crack,
    /// Ahamar, dark: nobody hides by day.
    Exposure,
    /// Maya, light: the fallen wake with their Spirit full.
    Rest,
    /// Maya, mid: at night one may hide anywhere, not only under cover.
    Manipulation,
    /// Maya, dark: reactions reach one hex only, and the fallen drop a card.
    Wrath,
}

/// Land further than this from its owner pays nothing under the Crack.
pub const CRACK_REACH: u32 = 3;
/// Cards a turn may play before the Burden makes them loud.
pub const BURDEN_FREE: u8 = 2;
/// Threat at which the guard marches under the Sentence.
pub const SENTENCE_THRESHOLD: u8 = 3;

impl Law {
    pub const ALL: [Law; 15] = [
        Law::Sprout,
        Law::Thicket,
        Law::Wildgrowth,
        Law::Generosity,
        Law::Thirst,
        Law::Devouring,
        Law::Stillness,
        Law::Burden,
        Law::Sentence,
        Law::Mask,
        Law::Crack,
        Law::Exposure,
        Law::Rest,
        Law::Manipulation,
        Law::Wrath,
    ];

    /// The law of `god` at `stage` (0 light .. 2 dark).
    pub const fn of(god: God, stage: u8) -> Law {
        let s = if stage > 2 { 2 } else { stage } as usize;
        Law::ALL[god.index() * 3 + s]
    }

    pub const fn god(self) -> God {
        God::from_index(self as usize / 3)
    }

    pub const fn stage(self) -> u8 {
        (self as usize % 3) as u8
    }
}

impl Game {
    /// The laws in force now, one per god.
    pub fn laws(&self) -> [Law; 5] {
        God::ALL.map(|g| Law::of(g, self.stage(g)))
    }

    pub fn law_active(&self, law: Law) -> bool {
        self.stage(law.god()) == law.stage()
    }

    /// Rivals this close to an event may react to it (§11.3): two hexes,
    /// one under Maya's Wrath.
    pub fn reaction_range(&self) -> u32 {
        if self.law_active(Law::Wrath) {
            1
        } else {
            super::REACTION_RANGE
        }
    }

    /// Rounds an untouched body lies before it grows a grove: one fewer
    /// under Bhava's Sprout.
    pub fn grove_age(&self) -> u8 {
        crate::board::GROVE_AGE - u8::from(self.law_active(Law::Sprout))
    }

    /// Threat at which the royal guard marches (§6.5).
    pub fn guard_threshold(&self) -> u8 {
        if self.law_active(Law::Sentence) {
            SENTENCE_THRESHOLD
        } else {
            super::GUARD_THRESHOLD
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifteen_laws_three_per_god_in_ring_order() {
        for god in God::ALL {
            for stage in 0..3 {
                let law = Law::of(god, stage);
                assert_eq!((law.god(), law.stage()), (god, stage));
            }
        }
    }
}

impl Game {
    /// What the laws do at dusk, once the stages have shifted.
    pub(super) fn dusk_laws(&mut self, events: &mut Vec<super::Event>) {
        use super::Event;
        use crate::board::Terrain;
        // Zaga's Stillness: the land goes quiet.
        if self.law_active(Law::Stillness) {
            events.push(Event::Law {
                law: Law::Stillness,
                player: None,
                hex: None,
            });
            for p in self.players().collect::<Vec<_>>() {
                if self.threat(p) > 0 {
                    self.add_threat(p, -1, events);
                }
            }
        }
        // Trishna's Devouring: her land burns its dead; the settlements
        // starve whoever stands in them.
        if self.law_active(Law::Devouring) {
            let burned: Vec<_> = self
                .board
                .corpses()
                .map(|(h, _)| h)
                .filter(|&h| {
                    self.board
                        .tile(h)
                        .is_some_and(|t| t.region == Some(God::Trishna))
                })
                .collect();
            for hex in burned {
                if let Some(tile) = self.board.tile_mut(hex) {
                    tile.corpse = None;
                }
                events.push(Event::Law {
                    law: Law::Devouring,
                    player: None,
                    hex: Some(hex),
                });
                events.push(Event::CorpseDecayed { hex });
            }
            let hungry: Vec<_> = self
                .players()
                .filter(|&p| {
                    !self.chosen(p, God::Trishna)
                        && self
                            .board
                            .tile(self.hex_of(p))
                            .is_some_and(|t| t.terrain == Terrain::Settlement)
                })
                .collect();
            for p in hungry {
                events.push(Event::Law {
                    law: Law::Devouring,
                    player: Some(p),
                    hex: None,
                });
                self.damage(p, 1, events);
            }
        }
        // Bhava's Wildgrowth: a plain in his land runs wild.
        if self.law_active(Law::Wildgrowth) {
            let plains: Vec<_> = self
                .board
                .tiles()
                .filter(|(h, t)| {
                    t.terrain == Terrain::Plains
                        && t.region == Some(God::Bhava)
                        && self.champion_at(*h).is_none()
                        && !self.mob_at(*h)
                })
                .map(|(h, _)| h)
                .collect();
            if let Some(&hex) = self.rng.pick(&plains) {
                if let Some(tile) = self.board.tile_mut(hex) {
                    tile.terrain = Terrain::Forest;
                }
                events.push(Event::Law {
                    law: Law::Wildgrowth,
                    player: None,
                    hex: Some(hex),
                });
            }
        }
    }
}

/// Favour with a god at which it grants its patronage (§5.4): its Sign
/// (its cards cost a Spirit less), its Voice (wishes to it rise a grade),
/// and its Chosen (its harsh laws spare you).
pub const SIGN: u16 = 3;
pub const VOICE: u16 = 6;
pub const CHOSEN: u16 = 9;

/// How far a god's patronage of a player goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Patronage {
    None,
    Sign,
    Voice,
    Chosen,
}

impl Game {
    pub fn patronage(&self, player: super::PlayerId, god: God) -> Patronage {
        match self.favor(player, god) {
            f if f >= CHOSEN => Patronage::Chosen,
            f if f >= VOICE => Patronage::Voice,
            f if f >= SIGN => Patronage::Sign,
            _ => Patronage::None,
        }
    }

    /// `player` is the Chosen of `god`: its harsh laws spare them.
    pub fn chosen(&self, player: super::PlayerId, god: God) -> bool {
        self.patronage(player, god) >= Patronage::Chosen
    }

    /// Spirit `player` pays for `card`: its god's Sign takes one off.
    pub fn cost_of(&self, player: super::PlayerId, card: crate::cards::CardId) -> u8 {
        let def = self.def(card);
        let sign = def
            .element
            .is_some_and(|e| self.patronage(player, God::from_index(e.index())) >= Patronage::Sign);
        def.cost.saturating_sub(u8::from(sign))
    }
}

impl Game {
    /// Move points a step onto `terrain` costs `player`: Bhava's Chosen
    /// walk the woods as open ground.
    pub fn terrain_cost(&self, player: super::PlayerId, terrain: crate::board::Terrain) -> u32 {
        use crate::board::Terrain;
        if matches!(terrain, Terrain::Forest | Terrain::Grove) && self.chosen(player, God::Bhava) {
            1
        } else {
            terrain.move_cost()
        }
    }
}
