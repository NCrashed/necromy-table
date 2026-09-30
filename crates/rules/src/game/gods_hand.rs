//! The gods move the world themselves (docs/design.md §5, §21.6).
//!
//! At every dusk the god pressed hardest (the greatest pressure either way)
//! puts its hand on the world, in its domain and after its stage: in its
//! light it gives, near whoever favours it most; at mid it works the land
//! it likes; in its dark it strikes, near whoever favours it least. So the
//! board changes even while the players only walk, and the ring's shifts
//! show on the map.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId};
use crate::board::{Corpse, Terrain};
use crate::features::Feature;
use crate::gods::God;

/// What a god did to the world at dusk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GodAct {
    /// Bhava: a grove, woods spreading, a beast or the woods swallowing.
    Grove,
    Woods,
    Beast,
    /// Trishna: a new settlement, goods everywhere, a fire.
    Settlement,
    Goods,
    Fire,
    /// Zaga: standing stones, mountains risen, a river dammed with the dead.
    Stones,
    Mountains,
    Dam,
    /// Ahamar: the register's road, a trial, the register's judgement.
    Road,
    Trial,
    Judgement,
    /// Maya: the mist lifts, a river runs, the waters rise.
    Unveil,
    River,
    Flood,
    /// The god had nothing of its kind to do: its land grew instead.
    Land,
}

impl Game {
    /// Dusk: the god pressed hardest acts on the world.
    pub(super) fn gods_hand(&mut self, events: &mut Vec<Event>) {
        if self.scripted.is_some() {
            return;
        }
        let pressure = self.pantheon.pressure;
        let god = *God::ALL
            .iter()
            .max_by_key(|g| (pressure[g.index()].unsigned_abs(), self.rng.below(5)))
            .expect("five gods");
        let stage = self.stage(god);
        // Its light gives to the one who favours it most; its dark strikes
        // the one who favours it least.
        let by_favour = |g: &Game, most: bool| {
            g.players().max_by_key(|&p| {
                let f = i32::from(g.favor(p, god));
                (if most { f } else { -f }, p.0)
            })
        };
        let near_player = match stage {
            0 => by_favour(self, true),
            2 => by_favour(self, false),
            _ => by_favour(self, true),
        };
        let Some(who) = near_player else {
            return;
        };
        let near = self.hex_of(who);
        let act = self.god_act(god, stage, who, near, events);
        events.push(Event::GodActed {
            god,
            act,
            near: who,
        });
    }

    fn god_act(
        &mut self,
        god: God,
        stage: u8,
        who: PlayerId,
        near: Hex,
        events: &mut Vec<Event>,
    ) -> GodAct {
        let done = match (god, stage.min(2)) {
            (God::Bhava, 0) if self.has(Feature::Groves) => self
                .free_land_near(near, 1, |_, t| {
                    t.terrain.can_grow_grove() && t.terrain != Terrain::Grove
                })
                .map(|h| {
                    self.grow(h, events);
                    GodAct::Grove
                }),
            (God::Bhava, 1) => {
                let plains: Vec<Hex> = self
                    .board
                    .land()
                    .filter(|(h, t)| {
                        t.terrain == Terrain::Plains
                            && h.unsigned_distance_to(near) <= 3
                            && self.champion_at(*h).is_none()
                    })
                    .map(|(h, _)| h)
                    .take(3)
                    .collect();
                (!plains.is_empty()).then(|| {
                    for h in plains {
                        self.set_terrain_by_god(h, Terrain::Forest, events);
                    }
                    GodAct::Woods
                })
            }
            (God::Bhava, _) if self.has(Feature::Beasts) => self
                .free_land_near(near, 2, |_, t| {
                    matches!(
                        t.terrain,
                        Terrain::Forest | Terrain::Grove | Terrain::Plains
                    )
                })
                .map(|h| {
                    self.next_mob += 1;
                    let mob = super::mobs::Mob {
                        id: self.next_mob,
                        kind: super::mobs::MobKind::Beast { lair: h },
                        hex: h,
                        hp: super::BEAST_HEALTH,
                    };
                    self.mobs.push(mob);
                    events.push(Event::MobAppeared { mob });
                    GodAct::Beast
                }),
            (God::Trishna, 0) if self.has(Feature::Settlements) => {
                self.settle_near(near, events).map(|_| GodAct::Settlement)
            }
            (God::Trishna, 1) if self.has(Feature::Goods) => {
                self.make_goods(events);
                Some(GodAct::Goods)
            }
            (God::Trishna, _) if self.has(Feature::Fires) => {
                let fuel = self
                    .board
                    .land()
                    .filter(|(h, t)| {
                        t.terrain.burns() && h.unsigned_distance_to(near) <= 3 && *h != near
                    })
                    .map(|(h, _)| h)
                    .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()));
                fuel.map(|h| {
                    self.set_fire(h, None, events);
                    GodAct::Fire
                })
            }
            // Stones where few stand; where they crowd already, mountains.
            (God::Zaga, 0)
                if self
                    .board
                    .land()
                    .filter(|(h, t)| {
                        t.terrain == Terrain::Stones && h.unsigned_distance_to(near) <= 3
                    })
                    .count()
                    < 3 =>
            {
                self.stones_near(near, events).map(|_| GodAct::Stones)
            }
            (God::Zaga, 0) => {
                let raised = self.rise(God::Zaga, Some(Terrain::Mountain), near, 2, events);
                (!raised.is_empty()).then_some(GodAct::Mountains)
            }
            (God::Zaga, 1) => {
                let raised = self.rise(God::Zaga, Some(Terrain::Mountain), near, 2, events);
                (!raised.is_empty()).then_some(GodAct::Mountains)
            }
            (God::Zaga, _) => {
                // A river dammed into a marsh, and the dead beside the one
                // who forgot her.
                let river = self
                    .board
                    .land()
                    .filter(|(_, t)| t.terrain == Terrain::River)
                    .map(|(h, _)| h)
                    .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()));
                if let Some(h) = river {
                    self.dam(h, events);
                }
                if self.has(Feature::Bodies) {
                    for h in near.ring(1) {
                        if let Some(t) = self.board.tile_mut(h)
                            && t.terrain.is_land()
                            && t.corpse.is_none()
                        {
                            t.corpse = Some(Corpse::fresh());
                            events.push(Event::CorpseAppeared { hex: h });
                            break;
                        }
                    }
                }
                Some(GodAct::Dam)
            }
            (God::Ahamar, 0) if self.has(Feature::Roads) => {
                (self.run_road(near, 3, events) > 0).then_some(GodAct::Road)
            }
            (God::Ahamar, 1) if self.has(Feature::Trials) => self
                .trial_spot(Some((near, 2, 4)))
                .and_then(|h| self.set_trial(h, events))
                .map(|_| GodAct::Trial),
            (God::Ahamar, _) => {
                self.add_threat(who, 2, events);
                Some(GodAct::Judgement)
            }
            (God::Maya, 0) => {
                let mist = self.board.tiles().any(|(_, t)| t.terrain == Terrain::Mist);
                mist.then(|| {
                    self.unveil_near(near, 2, events);
                    GodAct::Unveil
                })
            }
            (God::Maya, 1) if self.has(Feature::Rivers) => {
                (!self.run_river(near, 2, events).is_empty()).then_some(GodAct::River)
            }
            (God::Maya, _) if self.has(Feature::Lakes) => {
                (self.flood(near, 2, events) > 0).then_some(GodAct::Flood)
            }
            _ => None,
        };
        done.unwrap_or_else(|| {
            self.grant_land(god, near, 2, events);
            GodAct::Land
        })
    }

    fn set_terrain_by_god(&mut self, hex: Hex, terrain: Terrain, events: &mut Vec<Event>) {
        if let Some(t) = self.board.tile_mut(hex) {
            t.terrain = terrain;
        }
        events.push(Event::TerrainChanged { hex, terrain });
    }

    /// A river hex silts up into a marsh: the river is cut there.
    pub(super) fn dam(&mut self, hex: Hex, events: &mut Vec<Event>) {
        if self
            .board
            .tile(hex)
            .is_some_and(|t| t.terrain == Terrain::River)
        {
            self.wash_road(hex);
            self.set_terrain_by_god(hex, Terrain::Swamp, events);
        }
    }
}
