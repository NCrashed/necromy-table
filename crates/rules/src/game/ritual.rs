//! Rituals, monsters, dragons and a guest from beyond (docs/design.md
//! §21.8).
//!
//! A champion draws a circle on standing stones; bodies laid in it feed
//! it. A circle fed five bodies opens its gate at dusk and a monster walks
//! out: strong, of the summoner's element, going for the living. Whoever
//! fells it gains Style and loot; if the summoner strikes the last blow,
//! the Summoning is done.
//!
//! A trial passed in the mountains may give an egg. Laid down in a fire
//! that burns in the world phase it warms; three times warmed it hatches,
//! and the dragon follows whoever laid it there last.
//!
//! Beyond the mist a stranger sometimes steps out at dusk. Whoever stands
//! beside them may lead them; led to Ahamar's Table alive they are home.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::mobs::{Mob, MobKind};
use super::{Event, Game, PlayerId, RuleError, StyleReason};
use crate::board::Terrain;
use crate::features::Feature;

pub const MONSTER_HEALTH: u8 = 8;
pub const MONSTER_DICE: u8 = 4;
/// Bodies a circle must be fed before its gate opens.
pub const SUMMON_BODIES: u8 = 5;
/// Spirit to draw a circle.
pub const CIRCLE_SPIRIT: u8 = 1;
/// Fires an egg must lie in before it hatches.
pub const EGG_WARMTH: u8 = 3;
/// Style for felling a monster, and for bringing the guest home.
pub const MONSTER_STYLE: i16 = 2;
pub const GUEST_STYLE: i16 = 3;

/// A circle on the stones and what it has been fed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Circle {
    pub owner: PlayerId,
    pub bodies: u8,
}

impl Game {
    pub fn circle(&self, hex: Hex) -> Option<Circle> {
        self.circles.get(&(hex.x(), hex.y())).copied()
    }

    pub fn circles(&self) -> impl Iterator<Item = (Hex, Circle)> + '_ {
        self.circles.iter().map(|(&(x, y), &c)| (Hex::new(x, y), c))
    }

    /// `player` felled a monster of their own summoning.
    pub fn summoned(&self, player: PlayerId) -> bool {
        self.wonders
            .get(player.0 as usize)
            .is_some_and(|w| w.summoned)
    }

    /// `player` led the guest to the Table.
    pub fn guest_home(&self, player: PlayerId) -> bool {
        self.wonders
            .get(player.0 as usize)
            .is_some_and(|w| w.guest_home)
    }

    pub fn may_draw_circle(&self, player: PlayerId) -> bool {
        let at = self.hex_of(player);
        self.has(Feature::Ritual)
            && self.circle(at).is_none()
            && self
                .board
                .tile(at)
                .is_some_and(|t| t.terrain == Terrain::Stones)
    }

    pub(super) fn check_circle(&self, player: PlayerId) -> Result<(), RuleError> {
        if !self.may_draw_circle(player) {
            return Err(RuleError::CannotBuild);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < CIRCLE_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: CIRCLE_SPIRIT,
                have,
            });
        }
        Ok(())
    }

    /// A circle on the stones underfoot, theirs.
    pub(super) fn draw_circle(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_circle(player)?;
        let champ = self.champ_mut(player);
        champ.spirit_points -= CIRCLE_SPIRIT;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
        let hex = self.hex_of(player);
        self.circles.insert(
            (hex.x(), hex.y()),
            Circle {
                owner: player,
                bodies: 0,
            },
        );
        events.push(Event::CircleDrawn { player, hex });
        Ok(())
    }

    /// A body laid down in a circle feeds it.
    pub(super) fn feed_circle(
        &mut self,
        player: PlayerId,
        hex: Hex,
        events: &mut Vec<Event>,
    ) -> bool {
        let Some(circle) = self.circles.get_mut(&(hex.x(), hex.y())) else {
            return false;
        };
        circle.bodies = circle.bodies.saturating_add(1);
        let bodies = circle.bodies;
        events.push(Event::CircleFed {
            player,
            hex,
            bodies,
        });
        true
    }

    /// Dusk: a circle fed enough opens its gate, and a monster walks out.
    pub(super) fn open_gates(&mut self, events: &mut Vec<Event>) {
        if !self.has(Feature::Monsters) {
            return;
        }
        let full: Vec<(Hex, Circle)> = self
            .circles()
            .filter(|(_, c)| c.bodies >= SUMMON_BODIES)
            .collect();
        for (hex, circle) in full {
            let Some(spot) = std::iter::once(hex).chain(hex.all_neighbors()).find(|&h| {
                self.board.contains(h) && self.champion_at(h).is_none() && !self.mob_at(h)
            }) else {
                continue;
            };
            self.circles.remove(&(hex.x(), hex.y()));
            let element = self.champions[circle.owner.0 as usize].god.element();
            self.next_mob += 1;
            let mob = Mob {
                id: self.next_mob,
                kind: MobKind::Monster {
                    summoner: Some(circle.owner),
                    element,
                },
                hex: spot,
                hp: MONSTER_HEALTH,
            };
            self.mobs.push(mob);
            events.push(Event::GateOpened {
                hex,
                summoner: circle.owner,
            });
            events.push(Event::MobAppeared { mob });
        }
    }

    /// World phase: a monster strikes a champion beside it, else lays waste
    /// the settlement it stands in, else walks to the nearest of the living.
    pub(super) fn monster_phase(&mut self, events: &mut Vec<Event>) {
        let ids: Vec<u32> = self
            .mobs
            .iter()
            .filter(|m| m.is_monster())
            .map(|m| m.id)
            .collect();
        for id in ids {
            let Some(m) = self.mobs.iter().find(|m| m.id == id).copied() else {
                continue;
            };
            let prey = self
                .players()
                .filter(|&p| !self.is_hidden(p) && self.hex_of(p).unsigned_distance_to(m.hex) <= 1)
                .min_by_key(|&p| (self.champions[p.0 as usize].hp, p.0));
            if let Some(p) = prey {
                self.mob_strike(id, p, events);
                continue;
            }
            if self
                .board
                .tile(m.hex)
                .is_some_and(|t| t.terrain == Terrain::Settlement)
            {
                self.ruin(m.hex, events);
                continue;
            }
            // The noise of a fair draws it first (§21.8).
            let fair = self.fair_in_sight(m.hex, 8);
            let goal = fair.or_else(|| {
                self.players()
                    .filter(|&p| !self.is_hidden(p))
                    .map(|p| self.hex_of(p))
                    .chain(
                        self.board
                            .land()
                            .filter(|(_, t)| t.terrain == Terrain::Settlement)
                            .map(|(h, _)| h),
                    )
                    .min_by_key(|h| (h.unsigned_distance_to(m.hex), h.x(), h.y()))
            });
            let Some(goal) = goal else {
                continue;
            };
            let here = m.hex.unsigned_distance_to(goal);
            let next = m
                .hex
                .all_neighbors()
                .into_iter()
                .filter(|&h| {
                    self.board.contains(h) && self.champion_at(h).is_none() && !self.mob_at(h)
                })
                .filter(|&h| h.unsigned_distance_to(goal) < here)
                .min_by_key(|&h| (h.unsigned_distance_to(goal), h.x(), h.y()));
            if let Some(next) = next {
                if let Some(u) = self.mobs.iter_mut().find(|u| u.id == id) {
                    u.hex = next;
                }
                events.push(Event::MobMoved {
                    id,
                    from: m.hex,
                    to: next,
                });
            }
        }
    }

    /// A monster fell to `by`: Style and loot, and the Summoning if it was
    /// theirs.
    pub(super) fn monster_slain(
        &mut self,
        by: PlayerId,
        summoner: Option<PlayerId>,
        events: &mut Vec<Event>,
    ) {
        self.add_style(by, MONSTER_STYLE, StyleReason::Battle, events);
        self.gain_loot(by, events);
        self.first(by, super::Novelty::SlewMonster, events);
        let own = summoner == Some(by);
        if own {
            self.wonders[by.0 as usize].summoned = true;
        }
        events.push(Event::MonsterSlain { by, own });
    }

    /// A trial passed in the mountains: an egg, if the world has dragons
    /// and none is about yet.
    pub(super) fn egg_from_trial(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        let mountain = self
            .board
            .tile(hex)
            .is_some_and(|t| t.terrain == Terrain::Mountain);
        let about = self
            .loads
            .iter()
            .any(|(_, c)| matches!(c, super::Cargo::Egg { .. }))
            || self.champions.iter().any(|c| {
                matches!(c.cargo, Some(super::Cargo::Egg { .. }))
                    || c.companions.contains(&super::Companion::Dragon)
            });
        if !mountain || about || !self.has(Feature::Dragons) || !self.has(Feature::Cargo) {
            return;
        }
        let egg = super::Cargo::Egg {
            warmth: 0,
            by: Some(player),
        };
        if self.cargo(player).is_none() {
            self.champ_mut(player).cargo = Some(egg);
            events.push(Event::CargoTaken {
                player,
                cargo: egg,
                hex,
            });
        } else {
            self.loads.push((hex, egg));
        }
        events.push(Event::EggFound { player, hex });
    }

    /// World phase, before the fires burn out: an egg lying in one warms,
    /// and hatches when warm enough.
    pub(super) fn warm_eggs(&mut self, events: &mut Vec<Event>) {
        let mut hatched: Vec<(usize, Option<PlayerId>)> = Vec::new();
        for (i, (hex, cargo)) in self.loads.iter_mut().enumerate() {
            let super::Cargo::Egg { warmth, by } = cargo else {
                continue;
            };
            if !self.fires.contains_key(&(hex.x(), hex.y())) {
                continue;
            }
            *warmth += 1;
            events.push(Event::EggWarmed {
                hex: *hex,
                warmth: *warmth,
            });
            if *warmth >= EGG_WARMTH {
                hatched.push((i, *by));
            }
        }
        for (i, by) in hatched.into_iter().rev() {
            let (hex, _) = self.loads.remove(i);
            if let Some(p) = by {
                self.champ_mut(p).companions.push(super::Companion::Dragon);
            }
            events.push(Event::DragonHatched { hex, player: by });
        }
    }

    /// Dusk: with nobody from beyond about, one steps out of the mist.
    pub(super) fn guest_at_dusk(&mut self, events: &mut Vec<Event>) {
        if !self.has(Feature::Guests) {
            return;
        }
        let about = self.mobs.iter().any(|m| matches!(m.kind, MobKind::Guest))
            || self
                .champions
                .iter()
                .any(|c| c.companions.contains(&super::Companion::Guest));
        if about || self.wonders.iter().any(|w| w.guest_home) {
            return;
        }
        let spots: Vec<Hex> = self
            .board
            .land()
            .map(|(h, _)| h)
            .filter(|&h| self.at_rim(h) && self.champion_at(h).is_none() && !self.mob_at(h))
            .collect();
        let Some(&hex) = self.rng.pick(&spots) else {
            return;
        };
        self.next_mob += 1;
        let mob = Mob {
            id: self.next_mob,
            kind: MobKind::Guest,
            hex,
            hp: MobKind::Guest.health(),
        };
        self.mobs.push(mob);
        events.push(Event::MobAppeared { mob });
    }

    /// A step onto the Table with the guest: they are home.
    pub(super) fn guest_arrives(&mut self, player: PlayerId, at: Hex, events: &mut Vec<Event>) {
        if at != Hex::ZERO {
            return;
        }
        let champ = self.champ_mut(player);
        let Some(i) = champ
            .companions
            .iter()
            .position(|&c| c == super::Companion::Guest)
        else {
            return;
        };
        champ.companions.remove(i);
        self.wonders[player.0 as usize].guest_home = true;
        self.add_style(player, GUEST_STYLE, StyleReason::Story, events);
        events.push(Event::GuestHome { player });
    }
}

/// Wonders done by a player, for their deeds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wonders {
    pub summoned: bool,
    pub guest_home: bool,
}
