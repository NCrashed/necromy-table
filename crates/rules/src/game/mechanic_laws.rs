//! The laws of the gods over the world's newer mechanics (docs/design.md
//! §5.3, §21.8): each stage of a god bends what lies in its domain, so a
//! shift of the ring changes the whole sandbox, not only the cards.
//!
//! The laws keep their names and their first rule (`laws.rs`); this is
//! their second, and it acts only where the world has the mechanic.
//!
//! - Bhava: Sprout, walking groves take two steps a night; Thicket, beasts
//!   trust whoever tames them (a Spirit less); Wildgrowth, a field of his
//!   land runs wild into forest at dusk.
//! - Trishna: Generosity, a feast wants three food; Thirst, goods sold pay
//!   a Spirit too; Devouring, fire catches on every neighbour and a fire
//!   starts in her land at dusk.
//! - Zaga: Stillness, a burial is an offering to her and quiets the one
//!   who buries; Burden, a burden costs two steps; Sentence, plague pits
//!   poison twice as deep.
//! - Ahamar: Mask, a lord with sworn rulers gains a Style at dawn; Crack,
//!   unpaid debts grow at dusk; Exposure, a rival who never came to a duel
//!   is named in the register (Threat).
//! - Maya: Rest, a river short of the rim runs on a hex at dusk;
//!   Manipulation, the hidden
//!   ford rivers at a step's cost; Wrath, the smallest lake spreads
//!   at dusk, up to seven hexes.

use hexx::Hex;

use super::{Event, Game, Law};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::God;

/// Hexes a lake grows to under Maya's Wrath, and no further.
pub const WRATH_LAKE: usize = 7;

impl Game {
    /// Steps a walking grove takes a night.
    pub(super) fn grove_strides(&self) -> usize {
        1 + usize::from(self.law_active(Law::Sprout))
    }

    /// Spirit to tame a beast.
    pub fn tame_cost(&self) -> u8 {
        super::TAME_SPIRIT - u8::from(self.law_active(Law::Thicket))
    }

    /// Food a feast eats.
    pub fn feast_food(&self) -> u8 {
        if self.law_active(Law::Generosity) {
            3
        } else {
            super::FEAST_FOOD
        }
    }

    /// Move points a burden takes off a turn.
    pub(super) fn burden_weight(&self) -> u32 {
        1 + u32::from(self.law_active(Law::Burden))
    }

    /// Stacks of poison a plague pit gives.
    pub(super) fn pit_stacks(&self) -> u8 {
        1 + u8::from(self.law_active(Law::Sentence))
    }

    /// Whether a fire catches on a neighbour: half the time, always under
    /// Trishna's Devouring.
    pub(super) fn fire_catches(&mut self) -> bool {
        self.law_active(Law::Devouring) || self.rng.below(2) == 0
    }

    /// Dusk, with the other laws: what each god's law does to the world's
    /// mechanics.
    pub(super) fn mechanic_laws_at_dusk(&mut self, events: &mut Vec<Event>) {
        // Bhava's Wildgrowth: a field of his land goes back to the woods.
        if self.law_active(Law::Wildgrowth) && self.has(Feature::Fields) {
            let fields = self.land_of(God::Bhava, |t| t == Terrain::Fields);
            if let Some(&hex) = self.rng.pick(&fields) {
                self.law_terrain(Law::Wildgrowth, hex, Terrain::Forest, events);
            }
        }
        // Trishna's Devouring: her land catches fire.
        if self.law_active(Law::Devouring) && self.has(Feature::Fires) {
            let fuel = self.land_of(God::Trishna, |t| {
                matches!(t, Terrain::Forest | Terrain::Grove | Terrain::Fields)
            });
            if let Some(&hex) = self.rng.pick(&fuel) {
                events.push(Event::Law {
                    law: Law::Devouring,
                    player: None,
                    hex: Some(hex),
                });
                self.set_fire(hex, None, events);
            }
        }
        // Ahamar's Crack: what is owed grows.
        if self.law_active(Law::Crack) && !self.debts.is_empty() {
            for d in &mut self.debts {
                d.amount = d.amount.saturating_add(1);
            }
            events.push(Event::Law {
                law: Law::Crack,
                player: None,
                hex: None,
            });
        }
        // Maya's Rest: a river that has not reached the rim runs on a hex.
        if self.law_active(Law::Rest) && self.has(Feature::Rivers) {
            let head = self
                .board
                .tiles()
                .filter(|(h, t)| {
                    t.terrain == Terrain::River
                        && !self.at_rim(*h)
                        && h.all_neighbors()
                            .iter()
                            .filter(|&&n| {
                                self.board
                                    .tile(n)
                                    .is_some_and(|t| t.terrain == Terrain::River)
                            })
                            .count()
                            <= 1
                })
                .map(|(h, _)| h)
                .min_by_key(|h| (std::cmp::Reverse(h.ulength()), h.x(), h.y()));
            if let Some(head) = head
                && !self.run_river(head, 1, events).is_empty()
            {
                events.push(Event::Law {
                    law: Law::Rest,
                    player: None,
                    hex: Some(head),
                });
            }
        }
        // Maya's Wrath: the smallest lake spreads, up to `WRATH_LAKE`.
        if self.law_active(Law::Wrath) && self.has(Feature::Lakes) {
            let lake = self
                .lakes()
                .into_iter()
                .filter(|l| l.len() < WRATH_LAKE)
                .min_by_key(|l| (l.len(), l[0].x(), l[0].y()));
            if let Some(lake) = lake
                && self.flood(lake[0], 1, events) > 0
            {
                events.push(Event::Law {
                    law: Law::Wrath,
                    player: None,
                    hex: Some(lake[0]),
                });
            }
        }
    }

    /// Every lake, as its hexes.
    fn lakes(&self) -> Vec<Vec<Hex>> {
        let mut seen: Vec<Hex> = Vec::new();
        let mut lakes = Vec::new();
        for (h, t) in self.board.tiles() {
            if t.terrain != Terrain::Lake || seen.contains(&h) {
                continue;
            }
            let mut lake = vec![h];
            let mut i = 0;
            while i < lake.len() {
                for n in lake[i].all_neighbors() {
                    if self
                        .board
                        .tile(n)
                        .is_some_and(|t| t.terrain == Terrain::Lake)
                        && !lake.contains(&n)
                    {
                        lake.push(n);
                    }
                }
                i += 1;
            }
            seen.extend(lake.iter().copied());
            lakes.push(lake);
        }
        lakes
    }

    /// Dawn: Ahamar's Mask makes the sworn pay their lord.
    pub(super) fn mechanic_laws_at_dawn(&mut self, events: &mut Vec<Event>) {
        if !self.law_active(Law::Mask) {
            return;
        }
        let mut lords: Vec<super::PlayerId> = self.rulers().filter_map(|(_, r)| r.sworn).collect();
        lords.sort_by_key(|p| p.0);
        lords.dedup();
        for lord in lords {
            events.push(Event::Law {
                law: Law::Mask,
                player: Some(lord),
                hex: None,
            });
            self.add_style(lord, 1, super::StyleReason::Territory, events);
        }
    }

    /// Hexes of `god`'s land of a kind, free of anyone and anything.
    fn land_of(&self, god: God, kind: impl Fn(Terrain) -> bool) -> Vec<Hex> {
        self.board
            .tiles()
            .filter(|(h, t)| {
                t.region == Some(god)
                    && kind(t.terrain)
                    && self.champion_at(*h).is_none()
                    && !self.mob_at(*h)
            })
            .map(|(h, _)| h)
            .collect()
    }

    fn law_terrain(&mut self, law: Law, hex: Hex, terrain: Terrain, events: &mut Vec<Event>) {
        if let Some(tile) = self.board.tile_mut(hex) {
            tile.terrain = terrain;
        }
        self.loads.retain(|(h, _)| *h != hex);
        events.push(Event::TerrainChanged { hex, terrain });
        events.push(Event::Law {
            law,
            player: None,
            hex: Some(hex),
        });
    }
}
