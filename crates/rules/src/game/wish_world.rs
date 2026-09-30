//! Wishes in the words of the world's mechanics (docs/design.md §7.3,
//! §21.8): poison, a beast, the dead, the guard, a debt, a building, a
//! ruler's favour, a harvest, a fair, a walking wood. Each goes through the
//! rules of its mechanic; a mechanic the world lacks comes in instead, at
//! an awakening's price (`act_cost`).

use hexx::Hex;

use super::buildings::Building;
use super::mobs::{Mob, MobKind};
use super::wish::Act;
use super::{Event, Game, PlayerId};
use crate::board::Terrain;
use crate::gods::God;

impl Game {
    pub(super) fn grant_world_act(
        &mut self,
        player: PlayerId,
        god: God,
        act: Act,
        power: u8,
        events: &mut Vec<Event>,
    ) {
        let me = self.hex_of(player);
        if let Some(missing) = self.lacks_for(act) {
            if self.can_awaken(missing) {
                self.awaken(Some(player), god, missing, me, events);
            }
            return;
        }
        match act {
            Act::Poison { target } => {
                self.poison(target, god.element(), power.max(1), events);
            }
            Act::Beast { target: None } if self.has(crate::features::Feature::Companions) => {
                if self.companions(player).len() < super::RETINUE {
                    let companion = super::Companion::Beast(god.element());
                    self.champ_mut(player).companions.push(companion);
                    events.push(Event::CompanionJoined { player, companion });
                }
            }
            Act::Beast { target } => {
                let at = target.map_or(me, |t| self.hex_of(t));
                self.mob_beside(at, MobKind::Beast { lair: at }, events);
            }
            Act::Undead { target } => {
                let at = self.hex_of(target);
                self.mob_beside(at, MobKind::Undead, events);
            }
            Act::Guard { target } => {
                // The register marks them: the guard marches on the loudest.
                self.add_threat(target, (power + 1) as i8, events);
            }
            Act::Debt { target } => {
                self.owe(target, player, power.max(1));
                events.push(Event::BetSettled {
                    by: player,
                    on: target,
                    bet: super::wish::Bet::Fight,
                    won: true,
                });
            }
            Act::Build { building } => {
                let Some(town) = self.own_town_near(player, me, |g, h| g.building(h).is_none())
                else {
                    return;
                };
                let building = building.unwrap_or(match god {
                    God::Bhava => Building::Pen,
                    God::Trishna => Building::Tavern,
                    God::Zaga => Building::Shrine([God::Zaga, God::Zaga]),
                    God::Ahamar => Building::Forge,
                    God::Maya => Building::Shrine([God::Maya, God::Maya]),
                });
                self.buildings.insert((town.x(), town.y()), building);
                events.push(Event::Built {
                    player,
                    hex: town,
                    building,
                });
            }
            Act::Sway => {
                let ruler = self
                    .rulers()
                    .map(|(h, _)| h)
                    .filter(|h| h.unsigned_distance_to(me) <= 4)
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()));
                if let Some(h) = ruler
                    && let Some(r) = self.rulers.get_mut(&(h.x(), h.y()))
                {
                    let worth = (power + 1) as i8;
                    if let Some(x) = r.regard.get_mut(player.0 as usize) {
                        *x = x.saturating_add(worth);
                    }
                    events.push(Event::Gifted {
                        player,
                        hex: h,
                        worth,
                    });
                    self.settle_oath(h, events);
                }
            }
            Act::Harvest => {
                let fields: Vec<Hex> = self
                    .board
                    .land()
                    .filter(|(h, t)| {
                        t.terrain == Terrain::Fields
                            && h.unsigned_distance_to(me) <= 3
                            && !self.loads.iter().any(|(l, _)| l == h)
                    })
                    .map(|(h, _)| h)
                    .collect();
                for h in fields {
                    self.loads.push((h, super::Cargo::Food));
                    events.push(Event::FoodGrew { hex: h });
                }
                // And the stores of their settlement fill.
                if let Some(town) = self.own_town_near(player, me, |_, _| true) {
                    for _ in 0..power {
                        self.store_food(player, town, events);
                    }
                }
            }
            Act::Fair => {
                let Some(town) = self.own_town_near(player, me, |g, h| g.fair(h).is_none()) else {
                    return;
                };
                self.fairs.insert(
                    (town.x(), town.y()),
                    super::Fair {
                        host: player,
                        goods: 0,
                        until: self.dusks + super::FAIR_DUSKS,
                    },
                );
                events.push(Event::FairOpened { player, hex: town });
            }
            Act::WakeGrove => {
                let grove = self
                    .board
                    .land()
                    .filter(|(h, t)| t.terrain == Terrain::Grove && h.unsigned_distance_to(me) <= 4)
                    .map(|(h, _)| h)
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()));
                if let Some(h) = grove {
                    self.wake_grove(player, h, events);
                }
            }
            _ => {}
        }
    }

    /// A settlement of `player`'s nearest `near` that `fits`.
    fn own_town_near(
        &self,
        player: PlayerId,
        near: Hex,
        fits: impl Fn(&Game, Hex) -> bool,
    ) -> Option<Hex> {
        self.claims()
            .filter(|&(h, p)| p == player && self.own_settlement(player, h) && fits(self, h))
            .map(|(h, _)| h)
            .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()))
    }

    /// A mob of `kind` out on free land beside `at`.
    fn mob_beside(&mut self, at: Hex, kind: MobKind, events: &mut Vec<Event>) {
        let Some(spot) = at
            .all_neighbors()
            .into_iter()
            .find(|&h| self.board.contains(h) && self.champion_at(h).is_none() && !self.mob_at(h))
        else {
            return;
        };
        let kind = match kind {
            MobKind::Beast { .. } => MobKind::Beast { lair: spot },
            k => k,
        };
        self.next_mob += 1;
        let mob = Mob {
            id: self.next_mob,
            kind,
            hex: spot,
            hp: kind.health(),
        };
        self.mobs.push(mob);
        events.push(Event::MobAppeared { mob });
    }
}
