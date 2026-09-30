//! Rivers, lakes and what lives in them (docs/design.md §21.8).
//!
//! A river is land one walks, but crossing onto it ends the walk; along it
//! a step costs one. A lake is still water nobody walks: not the mist, it
//! does not lift, and it cuts land off as the mist does. Piranhas in the
//! rivers bite whoever steps in, never taking the last health.
//!
//! A river runs from the mountains (or a swamp) nearest the asker out to
//! the rim of the world, winding: each hex it takes out or aside, never
//! back towards the Table, as the land will have it. At the rim it has its
//! mouth and runs no further; a river runs on from its end short of the
//! rim. A flood spreads the water nearest the asker, drawn
//! towards Ahamar's Table: Maya's water against his fire.

use hexx::Hex;

use super::victory::{JUNGLE, JUNGLE_RIVER};
use super::{Event, Game, PlayerId};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::God;

/// Hexes a river runs for a wish of power 0; each power adds one.
pub const RIVER_RUN: usize = 3;

impl Game {
    /// Move points to step from `from` onto `to`, with `points` left:
    /// crossing onto a river takes them all; a road costs one (§21.8).
    pub(super) fn step_price(&self, player: PlayerId, from: Hex, to: Hex, points: u32) -> u32 {
        let Some(terrain) = self.board.tile(to).map(|t| t.terrain) else {
            return points.max(1);
        };
        if self.on_road(from, to) {
            return 1;
        }
        let along = self
            .board
            .tile(from)
            .is_some_and(|t| t.terrain == Terrain::River);
        match terrain {
            Terrain::River if along => 1,
            // Maya's Manipulation: the hidden ford where they like.
            Terrain::River
                if self.is_hidden(player) && self.law_active(super::Law::Manipulation) =>
            {
                1
            }
            Terrain::River => points.max(1),
            _ => self.terrain_cost(player, terrain),
        }
    }

    /// A step into a river where piranhas live: a bite, never the last
    /// health; Bhava's Chosen they leave alone.
    pub(super) fn piranhas(&mut self, player: PlayerId, at: Hex, events: &mut Vec<Event>) {
        let river = self
            .board
            .tile(at)
            .is_some_and(|t| t.terrain == Terrain::River);
        if !river || !self.has(Feature::Piranhas) || self.chosen(player, God::Bhava) {
            return;
        }
        let champ = self.champ_mut(player);
        let bite = u8::from(champ.hp > 1);
        champ.hp -= bite;
        let hp = champ.hp;
        events.push(Event::PiranhasBit {
            player,
            amount: bite,
            hp,
        });
    }

    /// Land that water may take: open ground nobody stands on, no temple,
    /// settlement, trial or home.
    fn floodable(&self, hex: Hex) -> bool {
        self.board.tile(hex).is_some_and(|t| {
            matches!(
                t.terrain,
                Terrain::Plains
                    | Terrain::Forest
                    | Terrain::Swamp
                    | Terrain::Grove
                    | Terrain::Ruins
                    | Terrain::Stones
                    | Terrain::Mountain
                    | Terrain::River
                    | Terrain::Table
            )
        }) && self.champion_at(hex).is_none()
            && !self.guard_at(hex)
            && self.trial_at(hex).is_none()
            && !God::ALL.iter().any(|&g| self.board.start_of(g) == hex)
    }

    /// `hex` becomes water of `terrain`: whatever lay there goes, the undead
    /// and beasts on it drown.
    pub(super) fn water(&mut self, hex: Hex, terrain: Terrain, events: &mut Vec<Event>) {
        let key = (hex.x(), hex.y());
        let drowned: Vec<u32> = self
            .mobs
            .iter()
            .filter(|m| m.hex == hex)
            .map(|m| m.id)
            .collect();
        for id in drowned {
            self.mobs.retain(|m| m.id != id);
            events.push(Event::MobLeft { id });
        }
        self.put_out(hex, events);
        if terrain == Terrain::Lake {
            self.graves.remove(&key);
            self.pits.remove(&key);
            self.wash_road(hex);
            self.traps.retain(|t| t.hex != hex);
            self.ground.retain(|(h, _)| *h != hex);
            self.loads.retain(|(h, _)| *h != hex);
            self.claims.remove(&key);
            self.ruins.remove(&key);
        }
        let tile = self.board.tile_mut(hex).expect("water on the board");
        tile.terrain = terrain;
        tile.corpse = None;
        events.push(Event::TerrainChanged { hex, terrain });
    }

    /// The end of a river within three hexes of `near` that can still run: a
    /// river hex with at most one river beside it, short of the rim.
    fn river_end(&self, near: Hex) -> Option<Hex> {
        self.board
            .tiles()
            .filter(|(h, t)| {
                t.terrain == Terrain::River
                    && h.unsigned_distance_to(near) <= 3
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
            .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()))
    }

    /// The end of the river nearest `near` that could run on, anywhere.
    pub fn river_head(&self, near: Hex) -> Option<Hex> {
        self.board
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
            .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()))
    }

    /// Where the rim is: a hex with no land beside it on some side.
    pub fn at_rim(&self, hex: Hex) -> bool {
        hex.all_neighbors().iter().any(|&n| {
            self.board
                .tile(n)
                .is_none_or(|t| t.terrain == Terrain::Mist)
        })
    }

    /// A river near `near`, `count` hexes: on from the end of one within
    /// reach, else from the mountains (or a swamp) nearest, out towards the
    /// rim. It stops at the rim.
    pub(super) fn run_river(
        &mut self,
        near: Hex,
        count: usize,
        events: &mut Vec<Event>,
    ) -> Vec<Hex> {
        let mut at = match self.river_end(near) {
            Some(end) => end,
            None => {
                let Some(source) = self
                    .board
                    .land()
                    .filter(|(_, t)| matches!(t.terrain, Terrain::Mountain | Terrain::Swamp))
                    .map(|(h, _)| h)
                    .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()))
                else {
                    return Vec::new();
                };
                source
            }
        };
        let mut run = Vec::new();
        for _ in 0..count {
            // At the rim the river has its mouth: it runs no further.
            let river_here = self
                .board
                .tile(at)
                .is_some_and(|t| t.terrain == Terrain::River);
            if river_here && self.at_rim(at) {
                break;
            }
            // Out or aside, never back towards the Table; which way is the
            // land's whim, so rivers wind.
            let ways: Vec<Hex> = at
                .all_neighbors()
                .into_iter()
                .filter(|&n| {
                    n.ulength() >= at.ulength()
                        && self.floodable(n)
                        && self.board.tile(n).is_some_and(|t| {
                            !matches!(t.terrain, Terrain::River | Terrain::Mountain | Terrain::Table)
                        })
                        // Not back against the river it came from.
                        && !n.all_neighbors().iter().any(|&m| {
                            m != at && self.board.tile(m).is_some_and(|t| t.terrain == Terrain::River)
                        })
                })
                .collect();
            let Some(&next) = self.rng.pick(&ways) else {
                break;
            };
            self.water(next, Terrain::River, events);
            run.push(next);
            at = next;
        }
        run
    }

    /// The water nearest `near` spreads by `count` hexes, drawn towards the
    /// Table; with no water within two hexes a spring opens beside them.
    pub(super) fn flood(&mut self, near: Hex, count: usize, events: &mut Vec<Event>) -> usize {
        let mut done = 0;
        let water_near = self
            .board
            .tiles()
            .any(|(h, t)| t.terrain.is_water() && h.unsigned_distance_to(near) <= 2);
        if !water_near {
            let spring = near
                .all_neighbors()
                .into_iter()
                .filter(|&n| self.floodable(n))
                .min_by_key(|n| (n.ulength(), n.x(), n.y()));
            let Some(spring) = spring else {
                return 0;
            };
            self.water(spring, Terrain::Lake, events);
            done += 1;
        }
        while done < count {
            // The body of water nearest the asker, and the land beside it.
            let mut body: Vec<Hex> = self
                .board
                .tiles()
                .filter(|(_, t)| t.terrain.is_water())
                .map(|(h, _)| h)
                .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()))
                .into_iter()
                .collect();
            let mut i = 0;
            while i < body.len() {
                for n in body[i].all_neighbors() {
                    if self.board.tile(n).is_some_and(|t| t.terrain.is_water())
                        && !body.contains(&n)
                    {
                        body.push(n);
                    }
                }
                i += 1;
            }
            // Rivers of the body widen first where they touch the lake.
            let next = body
                .iter()
                .flat_map(|h| {
                    let mut around: Vec<Hex> = h.all_neighbors().to_vec();
                    if self
                        .board
                        .tile(*h)
                        .is_some_and(|t| t.terrain == Terrain::River)
                    {
                        around.push(*h);
                    }
                    around
                })
                .filter(|&n| {
                    self.floodable(n)
                        && self
                            .board
                            .tile(n)
                            .is_some_and(|t| t.terrain != Terrain::Lake)
                })
                .min_by_key(|n| (n.ulength(), n.unsigned_distance_to(near), n.x(), n.y()));
            let Some(next) = next else {
                break;
            };
            self.water(next, Terrain::Lake, events);
            done += 1;
        }
        done
    }

    /// Of the rivers, the one furthest on: its length, whether it rises by
    /// the mountains and whether it reaches the rim.
    pub(super) fn best_river(&self) -> (usize, bool, bool) {
        let mut seen: Vec<Hex> = Vec::new();
        let mut best = (0, false, false);
        for (hex, t) in self.board.tiles() {
            if t.terrain != Terrain::River || seen.contains(&hex) {
                continue;
            }
            let mut river = vec![hex];
            let mut i = 0;
            while i < river.len() {
                for n in river[i].all_neighbors() {
                    if self
                        .board
                        .tile(n)
                        .is_some_and(|t| t.terrain == Terrain::River)
                        && !river.contains(&n)
                    {
                        river.push(n);
                    }
                }
                i += 1;
            }
            let source = river.iter().any(|h| {
                h.all_neighbors().iter().any(|&n| {
                    self.board
                        .tile(n)
                        .is_some_and(|t| t.terrain == Terrain::Mountain)
                })
            });
            let mouth = river.iter().any(|&h| self.at_rim(h));
            let score = |(len, source, mouth): (usize, bool, bool)| {
                (usize::from(source) + usize::from(mouth), len)
            };
            let found = (river.len(), source, mouth);
            if score(found) > score(best) {
                best = found;
            }
            seen.extend(river);
        }
        best
    }

    /// Of the Table and the six round it, how many are under water.
    pub(super) fn table_flooded(&self) -> usize {
        std::iter::once(Hex::ZERO)
            .chain(Hex::ZERO.all_neighbors())
            .filter(|&h| {
                self.board
                    .tile(h)
                    .is_some_and(|t| t.terrain == Terrain::Lake)
            })
            .count()
    }

    /// Of the woods a river runs through, the greatest: its woods, and
    /// the river hexes in it.
    pub(super) fn best_jungle(&self) -> (usize, usize) {
        let wild = |t: Terrain| matches!(t, Terrain::Forest | Terrain::Grove | Terrain::River);
        let mut seen: Vec<Hex> = Vec::new();
        let mut best = (0, 0);
        for (hex, t) in self.board.tiles() {
            if !matches!(t.terrain, Terrain::Forest | Terrain::Grove) || seen.contains(&hex) {
                continue;
            }
            let mut wood = vec![hex];
            let mut i = 0;
            while i < wood.len() {
                for n in wood[i].all_neighbors() {
                    if self.board.tile(n).is_some_and(|t| wild(t.terrain)) && !wood.contains(&n) {
                        wood.push(n);
                    }
                }
                i += 1;
            }
            let rivers = wood
                .iter()
                .filter(|&&h| {
                    self.board
                        .tile(h)
                        .is_some_and(|t| t.terrain == Terrain::River)
                })
                .count();
            let found = (wood.len() - rivers, rivers);
            let score = |(woods, rivers): (usize, usize)| {
                (woods.min(JUNGLE) + rivers.min(JUNGLE_RIVER), woods)
            };
            if score(found) > score(best) {
                best = found;
            }
            seen.extend(wood);
        }
        best
    }
}
