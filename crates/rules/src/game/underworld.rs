//! The underworld (docs/design.md §21.8), for now without a second board.
//!
//! Under ruins a champion opens a way down. From its mouth they dig
//! tunnels, a turn each, until a hall is reached, and make a treasury in it.
//! Companions left there guard it. A rival at the mouth may raid it: with
//! more might than it has guards (and one more), they take it, with loot
//! and Style; else they are hurt. A treasury nobody has taken for three
//! dusks is the Treasury deed.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError, StyleReason};
use crate::board::Terrain;
use crate::features::Feature;

/// Tunnels from the mouth to the hall.
pub const HALL_DEPTH: u8 = 3;
/// Spirit to open a way down or dig a tunnel; a treasury costs twice that.
pub const DELVE_SPIRIT: u8 = 1;
/// Dusks a treasury must hold for the deed.
pub const TREASURY_DUSKS: u8 = 3;

/// A way down under ruins, and what lies below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delve {
    pub owner: PlayerId,
    pub tunnels: u8,
    pub treasury: bool,
    pub guards: u8,
    /// Dusks the treasury has held.
    pub held: u8,
}

/// What a champion may do at a way down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelveWork {
    /// Open a way down under the ruins underfoot.
    Open,
    /// Dig a tunnel towards the hall.
    Dig,
    /// Make a treasury in the hall.
    Treasury,
    /// Leave a companion below to guard it.
    Guard,
    /// Raid a rival's treasury.
    Raid,
}

impl Game {
    pub fn delve(&self, hex: Hex) -> Option<Delve> {
        self.delves.get(&(hex.x(), hex.y())).copied()
    }

    pub fn delves(&self) -> impl Iterator<Item = (Hex, Delve)> + '_ {
        self.delves.iter().map(|(&(x, y), &d)| (Hex::new(x, y), d))
    }

    /// The most dusks a treasury of `player`'s has held.
    pub fn treasury_held(&self, player: PlayerId) -> usize {
        self.delves()
            .filter(|(_, d)| d.owner == player && d.treasury)
            .map(|(_, d)| usize::from(d.held))
            .max()
            .unwrap_or(0)
    }

    /// What `player` could do where they stand.
    pub fn delve_work(&self, player: PlayerId) -> Vec<DelveWork> {
        if !self.has(Feature::Underworld) {
            return Vec::new();
        }
        let at = self.hex_of(player);
        let spirit = self.champions[player.0 as usize].spirit_points;
        let Some(d) = self.delve(at) else {
            let ruins = self
                .board
                .tile(at)
                .is_some_and(|t| t.terrain == Terrain::Ruins);
            return if ruins && spirit >= DELVE_SPIRIT {
                vec![DelveWork::Open]
            } else {
                Vec::new()
            };
        };
        let mut out = Vec::new();
        if d.owner == player {
            if d.tunnels < HALL_DEPTH && spirit >= DELVE_SPIRIT {
                out.push(DelveWork::Dig);
            }
            if d.tunnels >= HALL_DEPTH && !d.treasury && spirit >= 2 * DELVE_SPIRIT {
                out.push(DelveWork::Treasury);
            }
            if d.treasury && !self.companions(player).is_empty() {
                out.push(DelveWork::Guard);
            }
        } else if d.treasury {
            out.push(DelveWork::Raid);
        }
        out
    }

    pub(super) fn check_delve(&self, player: PlayerId, work: DelveWork) -> Result<(), RuleError> {
        if self.delve_work(player).contains(&work) {
            Ok(())
        } else {
            Err(RuleError::CannotBuild)
        }
    }

    fn pay_delve(&mut self, player: PlayerId, amount: u8, events: &mut Vec<Event>) {
        let champ = self.champ_mut(player);
        champ.spirit_points -= amount;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
    }

    pub(super) fn work_delve(
        &mut self,
        player: PlayerId,
        work: DelveWork,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_delve(player, work)?;
        let at = self.hex_of(player);
        let key = (at.x(), at.y());
        match work {
            DelveWork::Open => {
                self.pay_delve(player, DELVE_SPIRIT, events);
                self.delves.insert(
                    key,
                    Delve {
                        owner: player,
                        tunnels: 0,
                        treasury: false,
                        guards: 0,
                        held: 0,
                    },
                );
            }
            DelveWork::Dig => {
                self.pay_delve(player, DELVE_SPIRIT, events);
                self.delves.get_mut(&key).expect("checked").tunnels += 1;
                self.turns[player.0 as usize].move_points = 0;
            }
            DelveWork::Treasury => {
                self.pay_delve(player, 2 * DELVE_SPIRIT, events);
                let d = self.delves.get_mut(&key).expect("checked");
                d.treasury = true;
                d.held = 0;
            }
            DelveWork::Guard => {
                self.champ_mut(player).companions.pop();
                self.delves.get_mut(&key).expect("checked").guards += 1;
            }
            DelveWork::Raid => {
                let d = self.delve(at).expect("checked");
                let might = self.champions[player.0 as usize].might + self.companion_dice(player);
                let won = might > d.guards + 1;
                if won {
                    let d = self.delves.get_mut(&key).expect("checked");
                    d.treasury = false;
                    d.guards = 0;
                    d.held = 0;
                    self.add_style(player, 2, StyleReason::Battle, events);
                    self.gain_loot(player, events);
                } else {
                    self.damage(player, 1, events);
                }
                events.push(Event::Raided {
                    player,
                    hex: at,
                    owner: d.owner,
                    won,
                });
                self.turns[player.0 as usize].move_points = 0;
                return Ok(());
            }
        }
        let d = self.delve(at).expect("just worked");
        events.push(Event::Delved {
            player,
            hex: at,
            work,
            tunnels: d.tunnels,
            guards: d.guards,
        });
        Ok(())
    }

    /// Dusk: each treasury has held another day.
    pub(super) fn treasuries_at_dusk(&mut self) {
        for d in self.delves.values_mut() {
            if d.treasury {
                d.held = d.held.saturating_add(1);
            }
        }
    }
}
