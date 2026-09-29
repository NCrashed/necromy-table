//! The militia of each settlement (docs/design.md §20.4).
//!
//! A settlement's militia stand on its hex, as many men as it still has.
//! They hold it against the undead: each world phase they cut down one
//! undead next to them and lose a man doing it, and an undead next to them
//! that is still standing knocks one down. Once no man is left the next
//! undead to walk in lays the settlement waste. A man comes back each dawn.
//!
//! Champions meet them on the settlement's hex. Whoever the militia do not
//! hold against (`standing` at least `MILITIA_PASS`) trades places with
//! them: the champion steps in, the militia step out to where the champion
//! stood, and go home in the next world phase. The others have to fight
//! their way in, and the militia remember it.
//!
//! With no mob next to them, the militia strike a champion next to them
//! who is not their friend: whoever went after one of their friends in
//! their sight (`pursuer`), else whoever is loud enough for the guard. A
//! blow of theirs never takes the last health.
//!
//! The ruins of a settlement can be built again: standing on them, on
//! one's own turn, for `REBUILD_SPIRIT` and the rest of the turn's walk. The
//! builder holds it, one man comes back, and the militia think better of them.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::style::Deed;
use super::{Event, Fighter, Game, PlayerId, RevealReason, RuleError, WindowKind};
use crate::board::Terrain;
use crate::cards::CardId;
use crate::gods::Element;

/// A settlement's militia at full strength.
pub const MILITIA: u8 = 2;
/// Standing from which the militia let a champion through: trade places
/// instead of fighting. Neutral is enough; only those they hold something
/// against have to fight.
pub const MILITIA_PASS: i8 = 0;
/// Standing at which the militia are friends, and against.
pub const FRIENDLY: i8 = 2;
pub const HOSTILE: i8 = -2;
const STANDING_LIMIT: i8 = 3;
/// What building a settlement again costs, and what it wins with its people.
pub const REBUILD_SPIRIT: u8 = 2;
pub const REBUILD_STANDING: i8 = 2;
/// What cutting the militia down costs with the rest of them.
const KILLED_MILITIA: i8 = -2;
/// Rounds the militia hold it against whoever went after a friend of theirs:
/// the one it happened in and the next.
pub const PURSUIT_ROUNDS: u32 = 1;
/// Separates militia throws from the others.
const MILITIA_STREAM: u64 = 0x0000_f011;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Militia {
    pub men: u8,
    /// Where they stand: home, or where a champion they let in stood.
    /// `None` while nobody stands for them (none left, or on their way).
    pub at: Option<Hex>,
}

/// Why the militia struck a champion in the world phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MilitiaWhy {
    /// They went after `friend`, a friend of the militia, nearby.
    Pursuer { friend: PlayerId },
    /// Their Threat is up to the guard's threshold.
    Loud,
}

impl Game {
    /// The men a settlement's militia still has, if the settlement stands.
    pub fn militia(&self, home: Hex) -> Option<u8> {
        self.militia.get(&(home.x(), home.y())).map(|m| m.men)
    }

    pub fn militia_unit(&self, home: Hex) -> Option<Militia> {
        self.militia.get(&(home.x(), home.y())).copied()
    }

    /// Every settlement's militia: its home and itself.
    pub fn militias(&self) -> impl Iterator<Item = (Hex, Militia)> + '_ {
        self.militia.iter().map(|(&(x, y), &m)| (Hex::new(x, y), m))
    }

    /// The home of the militia standing on `hex`, if any stand there.
    pub fn militia_at(&self, hex: Hex) -> Option<Hex> {
        self.militias()
            .find(|(_, m)| m.men > 0 && m.at == Some(hex))
            .map(|(home, _)| home)
    }

    /// The militia on `hex` would let `player` through: a step onto them
    /// trades places instead of attacking.
    pub fn lets_pass(&self, player: PlayerId, hex: Hex) -> bool {
        self.militia_at(hex).is_some() && self.standing(player) >= MILITIA_PASS
    }

    /// What the militia think of `player`, −3..=3.
    pub fn standing(&self, player: PlayerId) -> i8 {
        self.standing.get(player.0 as usize).copied().unwrap_or(0)
    }

    /// `hex` was a settlement and lies in ruins.
    pub fn is_ruined_settlement(&self, hex: Hex) -> bool {
        self.ruins.contains(&(hex.x(), hex.y()))
    }

    pub(super) fn militia_of(
        board: &crate::board::Board,
    ) -> std::collections::BTreeMap<(i32, i32), Militia> {
        board
            .tiles()
            .filter(|(_, t)| t.terrain == Terrain::Settlement)
            .map(|(h, _)| {
                let m = Militia {
                    men: MILITIA,
                    at: Some(h),
                };
                ((h.x(), h.y()), m)
            })
            .collect()
    }

    pub(super) fn shift_standing(&mut self, player: PlayerId, delta: i8, events: &mut Vec<Event>) {
        let s = &mut self.standing[player.0 as usize];
        let before = *s;
        *s = (*s + delta).clamp(-STANDING_LIMIT, STANDING_LIMIT);
        if *s != before {
            let standing = *s;
            events.push(Event::StandingChanged { player, standing });
        }
    }

    /// `attacker` went after `victim`: a battle or a harmful card.
    pub(super) fn note_pursuit(&mut self, attacker: PlayerId, victim: PlayerId) {
        if attacker != victim {
            self.pursuers[victim.0 as usize] = Some((attacker, self.round));
        }
    }

    /// The rival who went after `victim` lately, this round or the one
    /// before (`PURSUIT_ROUNDS`).
    pub fn pursuer(&self, victim: PlayerId) -> Option<PlayerId> {
        self.pursuers
            .get(victim.0 as usize)
            .copied()
            .flatten()
            .filter(|&(_, round)| self.round - round <= PURSUIT_ROUNDS)
            .map(|(p, _)| p)
    }

    /// Whom the militia standing on `at` would strike, and why: a champion
    /// next to them in sight and not their friend, who went after a friend
    /// of theirs within two hexes, else who is loud.
    pub fn militia_target(&self, at: Hex) -> Option<(PlayerId, MilitiaWhy)> {
        let near: Vec<PlayerId> = self
            .players()
            .filter(|&p| !self.is_hidden(p) && self.standing(p) < FRIENDLY)
            .filter(|&p| self.hex_of(p).unsigned_distance_to(at) <= 1)
            .collect();
        let pursued = near.iter().find_map(|&p| {
            self.players()
                .filter(|&f| f != p && self.standing(f) >= FRIENDLY)
                .filter(|&f| self.hex_of(f).unsigned_distance_to(at) <= 2)
                .find(|&f| self.pursuer(f) == Some(p))
                .map(|friend| (p, MilitiaWhy::Pursuer { friend }))
        });
        pursued.or_else(|| {
            near.iter()
                .find(|&&p| self.threat(p) >= self.guard_threshold())
                .map(|&p| (p, MilitiaWhy::Loud))
        })
    }

    /// World phase: the militia of `home`, standing on `at`, strike whom
    /// `militia_target` names. Returns whether they struck.
    pub(super) fn militia_hit(&mut self, home: Hex, at: Hex, events: &mut Vec<Event>) -> bool {
        let Some((player, why)) = self.militia_target(at) else {
            return false;
        };
        events.push(Event::MilitiaHit { home, player, why });
        if self.champions[player.0 as usize].hp > 1 {
            self.damage(player, 1, events);
        }
        true
    }

    /// A settlement within two hexes of `hex`: its people see what happens.
    fn near_settlement(&self, hex: Hex) -> bool {
        self.militia
            .keys()
            .any(|&(x, y)| Hex::new(x, y).unsigned_distance_to(hex) <= 2)
    }

    /// What a deed near a settlement does to the militia's view of its doer.
    pub(super) fn standing_deed(&mut self, player: PlayerId, deed: Deed, events: &mut Vec<Event>) {
        if !self.near_settlement(self.hex_of(player)) {
            return;
        }
        use super::style::BodyVerb;
        let delta = match deed {
            Deed::Body(BodyVerb::Rest | BodyVerb::Seed) => 1,
            Deed::Body(BodyVerb::Fuel | BodyVerb::Legion) => -1,
            Deed::Attacked => -1,
            _ => 0,
        };
        if delta != 0 {
            self.shift_standing(player, delta, events);
        }
    }

    /// A step onto militia who let `player` through: they trade places.
    pub(super) fn trade_places(&mut self, player: PlayerId, to: Hex, events: &mut Vec<Event>) {
        let Some(home) = self.militia_at(to) else {
            return;
        };
        let from = self.hex_of(player);
        if let Some(m) = self.militia.get_mut(&(home.x(), home.y())) {
            m.at = Some(from);
        }
        events.push(Event::MilitiaSwapped {
            player,
            home,
            to: from,
        });
    }

    /// Militia take a blow: a man fewer; with none left nobody stands.
    pub(super) fn hurt_militia(&mut self, home: Hex, amount: u8, events: &mut Vec<Event>) {
        let Some(m) = self.militia.get_mut(&(home.x(), home.y())) else {
            return;
        };
        m.men = m.men.saturating_sub(amount);
        let men = m.men;
        if men == 0 {
            m.at = None;
        }
        events.push(Event::MilitiaHurt { home, men });
        if men == 0 {
            events.push(Event::MilitiaFell { home });
        }
    }

    /// World phase: militia away from home go back once it is free.
    pub(super) fn militia_go_home(&mut self, events: &mut Vec<Event>) {
        let homes: Vec<Hex> = self
            .militias()
            .filter(|(home, m)| m.men > 0 && m.at != Some(*home))
            .map(|(home, _)| home)
            .collect();
        for home in homes {
            if self.champion_at(home).is_some() || self.mob_at(home) {
                continue;
            }
            if let Some(m) = self.militia.get_mut(&(home.x(), home.y())) {
                m.at = Some(home);
            }
            events.push(Event::MilitiaMoved { home, to: home });
        }
    }

    /// An undead with no militia to stop it: the settlement becomes ruins.
    pub(super) fn ruin(&mut self, hex: Hex, events: &mut Vec<Event>) {
        if !self.has(super::Feature::Ruins) {
            return;
        }
        self.buildings.remove(&(hex.x(), hex.y()));
        self.militia.remove(&(hex.x(), hex.y()));
        self.claims.remove(&(hex.x(), hex.y()));
        self.ruins.insert((hex.x(), hex.y()));
        if let Some(tile) = self.board.tile_mut(hex) {
            tile.terrain = Terrain::Ruins;
        }
        events.push(Event::SettlementRuined { hex });
        events.push(Event::TerrainChanged {
            hex,
            terrain: Terrain::Ruins,
        });
    }

    /// Whether `player` could build the ruins they stand on again now.
    pub fn can_rebuild(&self, player: PlayerId) -> bool {
        self.free_to_act(player) && self.check_rebuild(player).is_ok()
    }

    pub(super) fn check_rebuild(&self, player: PlayerId) -> Result<Hex, RuleError> {
        let at = self.hex_of(player);
        if !self.is_ruined_settlement(at) {
            return Err(RuleError::NotRuins);
        }
        let have = self.champions[player.0 as usize].spirit_points;
        if have < REBUILD_SPIRIT {
            return Err(RuleError::NotEnoughSpirit {
                need: REBUILD_SPIRIT,
                have,
            });
        }
        Ok(at)
    }

    /// The settlement stands again: its builder holds it, one man comes
    /// back to it, and its people think better of the builder.
    pub(super) fn rebuild(
        &mut self,
        player: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let hex = self.check_rebuild(player)?;
        let champ = self.champ_mut(player);
        champ.spirit_points -= REBUILD_SPIRIT;
        let spirit = champ.spirit_points;
        events.push(Event::SpiritChanged { player, spirit });
        self.turns[player.0 as usize].move_points = 0;
        self.ruins.remove(&(hex.x(), hex.y()));
        if let Some(tile) = self.board.tile_mut(hex) {
            tile.terrain = Terrain::Settlement;
        }
        // The one man comes home once the builder steps off.
        self.militia
            .insert((hex.x(), hex.y()), Militia { men: 1, at: None });
        events.push(Event::SettlementRebuilt { player, hex });
        self.first(player, super::Novelty::Rebuilt, events);
        events.push(Event::TerrainChanged {
            hex,
            terrain: Terrain::Settlement,
        });
        self.shift_standing(player, REBUILD_STANDING, events);
        self.claim(player, hex, events);
        Ok(())
    }

    /// At dawn a man comes back to every militia.
    pub(super) fn militia_at_dawn(&mut self) {
        let homes: Vec<(i32, i32)> = self.militia.keys().copied().collect();
        for home in homes {
            let cap = self.militia_cap(home);
            if let Some(m) = self.militia.get_mut(&home) {
                m.men = (m.men + 1).min(cap);
            }
        }
    }

    /// Friends of the militia rest in their settlements.
    pub(super) fn militia_at_turn_start(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let at = self.hex_of(player);
        if self.militia(at).is_none() || self.standing(player) < FRIENDLY {
            return;
        }
        let c = &self.champions[player.0 as usize];
        if c.hp < c.body {
            events.push(Event::MilitiaHelped { player, hex: at });
            self.heal(player, 1, events);
        }
    }

    /// The unwelcome who end their turn in a settlement are beaten out.
    pub(super) fn militia_at_turn_end(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let at = self.hex_of(player);
        if self.militia(at).is_none_or(|m| m == 0) || self.standing(player) > HOSTILE {
            return;
        }
        events.push(Event::MilitiaBeat { player, hex: at });
        if self.champions[player.0 as usize].hp > 1 {
            self.damage(player, 1, events);
        }
    }

    /// `attacker` stepped onto militia who hold something against them:
    /// they pay `cost`, then pick cards to burn; the militia burn none.
    pub(super) fn start_militia_battle(
        &mut self,
        attacker: PlayerId,
        home: Hex,
        cost: u32,
        events: &mut Vec<Event>,
    ) {
        self.turns[attacker.0 as usize].move_points -= cost;
        if self.is_hidden(attacker) {
            self.reveal(attacker, RevealReason::Attacked, events);
        }
        events.push(Event::MilitiaAttacked { attacker, home });
        self.last_fight = self.round;
        self.add_threat(attacker, 1, events);
        self.record_deed(attacker, Deed::Attacked);
        self.record_deed(attacker, Deed::Fought);
        self.open_window(
            attacker,
            WindowKind::MilitiaBattle { attacker, home },
            vec![attacker],
            None,
            None,
            events,
        );
    }

    /// The attacker's burned faces and throw against a die for each man.
    pub(super) fn resolve_militia_battle(
        &mut self,
        attacker: PlayerId,
        home: Hex,
        burned: Vec<CardId>,
        events: &mut Vec<Event>,
    ) {
        let men = self.militia(home).unwrap_or(0);
        if men == 0 {
            self.discard.extend(burned);
            return;
        }
        self.battles += 1;
        let a_faces = self.throw_side(attacker, false, burned, events);
        self.mob_throws += 1;
        let label = [MILITIA_STREAM, self.mob_throws];
        let m_faces = self.roll_with(Fighter::Militia(home), &label, men, Vec::new(), events);
        // Village iron: its Element face is metal's.
        self.element_breaks_ward(Element::Metal, attacker, &m_faces, events);
        let militia_score = self.score(&m_faces);
        let mut champion_score = self.score(&a_faces);
        champion_score.shields += self.item_shields(attacker);
        events.push(Event::MilitiaResolved {
            home,
            champion: attacker,
            militia_score,
            champion_score,
        });
        let hurt = militia_score.hits.saturating_sub(champion_score.shields);
        if hurt > 0 {
            self.damage(attacker, hurt, events);
        }
        let dealt = champion_score.hits.saturating_sub(militia_score.shields);
        if dealt > 0 {
            self.hurt_militia(home, dealt, events);
            if self.militia(home) == Some(0) {
                self.shift_standing(attacker, KILLED_MILITIA, events);
            }
        }
    }
}
