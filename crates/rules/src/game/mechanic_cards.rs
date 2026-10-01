//! Cards of the world's mechanics (docs/design.md §21.8): a card family
//! for each, in its god's element, so a mechanic is played as a card too:
//! as an answer, in the chain of elements, burned in a battle, given by a
//! god. What they do goes through the mechanic's own rules.

use hexx::Hex;

use super::{Event, Game, PlayerId, Target};
use crate::board::Terrain;
use crate::cards::Effect;

/// Rounds a duel called by the gauntlet gives.
const GAUNTLET_ROUNDS: u32 = super::DUEL_ROUNDS;

impl Game {
    /// Whether a card on oneself would do anything where `player` stands.
    pub(super) fn caster_card_useful(&self, player: PlayerId, effect: Effect) -> bool {
        let me = self.hex_of(player);
        match effect {
            Effect::Channel(_) => self.board.land().any(|(_, t)| {
                matches!(
                    t.terrain,
                    Terrain::Mountain | Terrain::Swamp | Terrain::River
                )
            }),
            Effect::Deluge(_) => true,
            Effect::Harvest => self.fields_near(me).next().is_some(),
            Effect::Spade => self.may_consecrate(player),
            Effect::Offering => self.circles().any(|(h, _)| h.unsigned_distance_to(me) <= 2),
            Effect::Levy => {
                self.own_settlement(player, me)
                    && !self.loads.iter().any(|(h, _)| *h == me)
                    && self.board.tile(me).is_some_and(|t| t.region.is_some())
            }
            Effect::Tunnel => match self.delve(me) {
                Some(d) => d.owner == player && d.tunnels < super::HALL_DEPTH,
                None => self
                    .board
                    .tile(me)
                    .is_some_and(|t| t.terrain == Terrain::Ruins),
            },
            _ => true,
        }
    }

    fn fields_near(&self, at: Hex) -> impl Iterator<Item = Hex> + '_ {
        self.board
            .land()
            .filter(move |(h, t)| {
                t.terrain == Terrain::Fields
                    && h.unsigned_distance_to(at) <= 2
                    && !self.loads.iter().any(|(l, _)| l == h)
            })
            .map(|(h, _)| h)
    }

    /// What a card of the world's mechanics does.
    pub(super) fn mechanic_card(
        &mut self,
        caster: PlayerId,
        effect: Effect,
        target: Target,
        events: &mut Vec<Event>,
    ) {
        let me = self.hex_of(caster);
        let hex = match target {
            Target::Hex(h) => Some(h),
            _ => None,
        };
        let aimed = match target {
            Target::Champion(p) => Some(p),
            _ => None,
        };
        match effect {
            Effect::Kindle => {
                if let Some(h) = hex {
                    self.set_fire(h, Some(caster), events);
                }
            }
            Effect::Rain => {
                if let Some(h) = hex {
                    for n in h.range(1) {
                        self.put_out(n, Some(caster), events);
                    }
                }
            }
            Effect::Channel(n) => {
                self.run_river(me, usize::from(n), events);
            }
            Effect::Deluge(n) => {
                self.flood(me, usize::from(n), events);
            }
            Effect::Causeway => {
                if let Some(h) = hex {
                    self.lay_road(h, events);
                }
            }
            Effect::Dam => {
                if let Some(h) = hex {
                    self.dam(h, events);
                }
            }
            Effect::Rob => {
                if let Some(t) = aimed {
                    self.seize_cargo(caster, t, events);
                }
            }
            Effect::Court => {
                if let Some(h) = hex
                    && let Some(r) = self.rulers.get_mut(&(h.x(), h.y()))
                {
                    if let Some(x) = r.regard.get_mut(caster.0 as usize) {
                        *x = x.saturating_add(2);
                    }
                    events.push(Event::Gifted {
                        player: caster,
                        hex: h,
                        worth: 2,
                    });
                    self.settle_oath(h, events);
                }
            }
            Effect::Lure => {
                let beast = hex.and_then(|h| {
                    self.mobs
                        .iter()
                        .find(|m| m.hex == h && m.is_beast())
                        .copied()
                });
                if let Some(m) = beast
                    && self.companions(caster).len() < super::RETINUE
                {
                    let companion = super::Companion::Beast(self.beast_element(&m));
                    self.mobs.retain(|x| x.id != m.id);
                    events.push(Event::MobLeft { id: m.id });
                    self.champ_mut(caster).companions.push(companion);
                    events.push(Event::CompanionJoined {
                        player: caster,
                        companion,
                    });
                }
            }
            Effect::Harvest => {
                let fields: Vec<Hex> = self.fields_near(me).collect();
                for h in fields {
                    self.loads.push((h, super::Cargo::Food));
                    events.push(Event::FoodGrew { hex: h });
                }
            }
            Effect::Spade => {
                if self.may_consecrate(caster) {
                    self.set_ground(me, Terrain::Graveyard, events);
                }
            }
            Effect::Offering => {
                let circle = self
                    .circles()
                    .map(|(h, _)| h)
                    .filter(|h| h.unsigned_distance_to(me) <= 2)
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()));
                if let Some(h) = circle {
                    self.feed_circle(caster, h, events);
                }
            }
            Effect::Gauntlet => {
                if let Some(t) = aimed
                    && !self.duels.iter().any(|d| d.host == caster || d.rival == t)
                {
                    self.duels.push(super::Duel {
                        host: caster,
                        rival: t,
                        until: self.round + GAUNTLET_ROUNDS,
                    });
                    events.push(Event::Challenged {
                        host: caster,
                        rival: t,
                        hex: me,
                    });
                }
            }
            Effect::Writ => {
                if let Some(t) = aimed {
                    self.owe(t, caster, 1);
                    events.push(Event::BetSettled {
                        by: caster,
                        on: t,
                        bet: super::wish::Bet::Fight,
                        won: true,
                    });
                }
            }
            Effect::Piranha(n) => {
                if let Some(t) = aimed {
                    let river = self
                        .board
                        .tile(self.hex_of(t))
                        .is_some_and(|x| x.terrain == Terrain::River);
                    self.damage(t, n + if river { 3 } else { 0 }, events);
                }
            }
            Effect::Levy => {
                if let Some(god) = self.board.tile(me).and_then(|t| t.region) {
                    self.loads.push((me, super::Cargo::Goods(god)));
                    events.push(Event::GoodsMade { hex: me, god });
                }
            }
            Effect::Tunnel => {
                let key = (me.x(), me.y());
                match self.delves.get_mut(&key) {
                    Some(d) if d.owner == caster && d.tunnels < super::HALL_DEPTH => {
                        d.tunnels += 1;
                    }
                    Some(_) => {}
                    None => {
                        self.delves.insert(
                            key,
                            super::Delve {
                                owner: caster,
                                tunnels: 0,
                                treasury: false,
                                guards: 0,
                                held: 0,
                            },
                        );
                    }
                }
                if let Some(d) = self.delve(me) {
                    events.push(Event::Delved {
                        player: caster,
                        hex: me,
                        work: super::DelveWork::Dig,
                        tunnels: d.tunnels,
                        guards: d.guards,
                    });
                }
            }
            _ => {}
        }
    }
}
