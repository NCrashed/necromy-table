//! Fields, food and the feast (docs/design.md §21.8).
//!
//! A champion sows the plains they stand on beside a settlement of theirs:
//! it becomes a field. At dusk every field bears a food, which lies on it
//! until someone takes it on their back. Food laid down on a settlement of
//! one's own goes into its stores. Whoever has `FEAST_FOOD` in a settlement
//! may hold a feast there: the food is eaten, and every champion within
//! two hexes is a guest, healed and given Style. A feast with two guests
//! or more is the Feast for the Whole Land's last step.

use hexx::Hex;

use super::{Event, Game, PlayerId, RuleError, StyleReason};
use crate::board::Terrain;
use crate::features::Feature;

/// Spirit to sow a field.
pub const SOW_SPIRIT: u8 = 1;
/// Food a feast eats.
pub const FEAST_FOOD: u8 = 5;
/// Guests a feast needs to count for the Feast deed.
pub const FEAST_GUESTS: usize = 2;
/// Fields beside one's settlements for the Feast deed.
pub const FEAST_FIELDS: usize = 3;
/// How near a guest must stand.
pub const GUEST_RANGE: u32 = 2;

impl Game {
    /// Food in the stores of the settlement on `hex`.
    pub fn food_at(&self, hex: Hex) -> u8 {
        self.stores.get(&(hex.x(), hex.y())).copied().unwrap_or(0)
    }

    /// Whether `player` could sow where they stand: plains beside a
    /// settlement of theirs.
    pub fn may_sow(&self, player: PlayerId) -> bool {
        let at = self.hex_of(player);
        self.has(Feature::Fields)
            && self
                .board
                .tile(at)
                .is_some_and(|t| t.terrain == Terrain::Plains)
            && at
                .all_neighbors()
                .iter()
                .any(|&n| self.own_settlement(player, n))
    }

    pub(super) fn own_settlement(&self, player: PlayerId, hex: Hex) -> bool {
        self.owner(hex) == Some(player)
            && self
                .board
                .tile(hex)
                .is_some_and(|t| t.terrain == Terrain::Settlement)
    }

    pub(super) fn check_sow(&self, player: PlayerId) -> Result<(), RuleError> {
        if !self.may_sow(player) {
            return Err(RuleError::CannotBuild);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < SOW_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: SOW_SPIRIT,
                have,
            });
        }
        Ok(())
    }

    /// The plains underfoot become a field.
    pub(super) fn sow(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_sow(player)?;
        let champ = self.champ_mut(player);
        champ.spirit_points -= SOW_SPIRIT;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
        let hex = self.hex_of(player);
        self.make_field(hex, events);
        self.first(player, super::Novelty::Sowed, events);
        Ok(())
    }

    pub(super) fn make_field(&mut self, hex: Hex, events: &mut Vec<Event>) {
        if let Some(tile) = self.board.tile_mut(hex) {
            tile.terrain = Terrain::Fields;
            events.push(Event::TerrainChanged {
                hex,
                terrain: Terrain::Fields,
            });
        }
    }

    /// Fields beside settlements of `player`'s.
    pub fn fields_of(&self, player: PlayerId) -> usize {
        self.board
            .land()
            .filter(|(h, t)| {
                t.terrain == Terrain::Fields
                    && h.all_neighbors()
                        .iter()
                        .any(|&n| self.own_settlement(player, n))
            })
            .count()
    }

    /// Dusk: each field without food on it bears one.
    pub(super) fn harvest(&mut self, events: &mut Vec<Event>) {
        if !self.has(Feature::Fields) {
            return;
        }
        let fields: Vec<Hex> = self
            .board
            .land()
            .filter(|(_, t)| t.terrain == Terrain::Fields)
            .map(|(h, _)| h)
            .collect();
        for hex in fields {
            if !self.loads.iter().any(|(h, _)| *h == hex) {
                self.loads.push((hex, super::Cargo::Food));
                events.push(Event::FoodGrew { hex });
            }
        }
    }

    /// Food laid down on a settlement of one's own goes into its stores.
    pub(super) fn store_food(
        &mut self,
        player: PlayerId,
        hex: Hex,
        events: &mut Vec<Event>,
    ) -> bool {
        if !self.own_settlement(player, hex) {
            return false;
        }
        let food = self.stores.entry((hex.x(), hex.y())).or_insert(0);
        *food = food.saturating_add(1);
        let food = *food;
        events.push(Event::FoodStored { player, hex, food });
        true
    }

    /// Who would come to a feast of `player`'s where they stand.
    pub fn guests(&self, player: PlayerId) -> Vec<PlayerId> {
        let at = self.hex_of(player);
        self.players()
            .filter(|&p| p != player && self.hex_of(p).unsigned_distance_to(at) <= GUEST_RANGE)
            .collect()
    }

    pub fn may_feast(&self, player: PlayerId) -> bool {
        let at = self.hex_of(player);
        self.own_settlement(player, at) && self.food_at(at) >= FEAST_FOOD
    }

    pub(super) fn check_feast(&self, player: PlayerId) -> Result<(), RuleError> {
        if self.may_feast(player) {
            Ok(())
        } else {
            Err(RuleError::CannotBuild)
        }
    }

    /// The stores are eaten; the guests heal and gain Style.
    pub(super) fn hold_feast(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_feast(player)?;
        let at = self.hex_of(player);
        if let Some(food) = self.stores.get_mut(&(at.x(), at.y())) {
            *food -= FEAST_FOOD;
        }
        let guests = self.guests(player);
        for &g in &guests {
            self.heal(g, 1, events);
            self.add_style(g, 1, StyleReason::Feast, events);
        }
        events.push(Event::Feasted {
            player,
            hex: at,
            guests: guests.len() as u8,
        });
        if guests.len() >= FEAST_GUESTS {
            self.feasted[player.0 as usize] = true;
        }
        // At night, with the dead near and the militia at the gate: a ball of
        // the dead, if nobody fights before dawn (§21.7).
        let dead_near = self
            .mobs
            .iter()
            .any(|m| m.is_undead() && m.hex.unsigned_distance_to(at) <= GUEST_RANGE);
        if self.time == super::TimeOfDay::Night
            && guests.len() >= FEAST_GUESTS
            && dead_near
            && self.militia_at(at).is_some()
        {
            self.ball = Some((player, self.brawls));
            events.push(Event::BallBegun { host: player });
        }
        self.first(player, super::Novelty::Feasted, events);
        self.turns[player.0 as usize].move_points = 0;
        Ok(())
    }

    /// Dawn: a ball of the dead nobody fought through is kept.
    pub(super) fn ball_at_dawn(&mut self, events: &mut Vec<Event>) {
        let Some((host, brawls)) = self.ball.take() else {
            return;
        };
        let kept = self.brawls == brawls;
        if kept {
            self.balls[host.0 as usize] = true;
        }
        events.push(Event::BallEnded { host, kept });
    }

    /// Whether `player` kept a ball of the dead.
    pub fn kept_ball(&self, player: PlayerId) -> bool {
        self.balls.get(player.0 as usize).copied().unwrap_or(false)
    }

    /// A settlement of `player`'s with food enough for a feast, if any.
    pub fn feast_hall(&self, player: PlayerId) -> Option<Hex> {
        self.stores
            .iter()
            .map(|(&(x, y), &f)| (Hex::new(x, y), f))
            .filter(|&(h, f)| f >= FEAST_FOOD && self.own_settlement(player, h))
            .map(|(h, _)| h)
            .next()
    }
}
