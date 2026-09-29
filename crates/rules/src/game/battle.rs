//! Battles and dice (docs/design.md §12).
//!
//! Stepping onto a rival starts a battle. Both sides secretly burn cards for
//! guaranteed faces, then throw physical dice for the rest. Every throw is a
//! `necromy_dice` simulation seeded from the match seed, so the server's
//! result and every client's animation agree.

use necromy_dice::Face;
use serde::{Deserialize, Serialize};

use super::{Event, Fighter, Game, PlayerId, RuleError, TimeOfDay, WindowKind};
use crate::board::Terrain;
use crate::cards::CardId;
use crate::gods::{Element, God};
use crate::rng::Rng;
use hexx::Hex;

/// What one side's faces add up to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub hits: u8,
    pub shields: u8,
}

/// An Element face adds a die at most this many times in a row.
const MAX_EXPLOSIONS: u64 = 2;
/// Separates battle throws from any other stream derived from the seed.
const BATTLE_STREAM: u64 = 0x00ba_771e;

impl Game {
    /// Move points to attack the rival on `to`, if allowed.
    pub fn attack_cost(&self, player: PlayerId, to: Hex) -> Result<u32, RuleError> {
        let me = self.champion(player).ok_or(RuleError::UnknownPlayer)?;
        let tile = self.board.tile(to).ok_or(RuleError::OffBoard)?;
        if me.hex.unsigned_distance_to(to) != 1 {
            return Err(RuleError::NotAdjacent);
        }
        if !self.mob_at(to) && self.occupant(to).is_none_or(|p| p == player) {
            return Err(RuleError::InvalidTarget);
        }
        let cost = self.terrain_cost(player, tile.terrain);
        let have = self.move_points(player);
        if cost > have {
            return Err(RuleError::NotEnoughMovePoints { need: cost, have });
        }
        Ok(cost)
    }

    /// Hexes of rivals `player` can attack right now.
    pub fn attackable(&self, player: PlayerId) -> Vec<Hex> {
        if !self.free_to_act(player) {
            return Vec::new();
        }
        self.hex_of(player)
            .all_neighbors()
            .into_iter()
            .filter(|&h| self.attack_cost(player, h).is_ok())
            // Militia who let one through are no fight.
            .filter(|&h| !self.lets_pass(player, h))
            .collect()
    }

    /// Dice a champion throws in a battle; defending on a mountain adds one.
    pub fn dice_for(&self, player: PlayerId, defending: bool) -> u8 {
        let c = &self.champions[player.0 as usize];
        let high_ground = defending
            && self
                .board
                .tile(c.hex)
                .is_some_and(|t| t.terrain == Terrain::Mountain);
        // Striking from the shadow in an open battle (§11.6).
        let ambush = !defending
            && self.windows.iter().any(|w| {
                w.ambush == Some(player)
                    && matches!(w.kind, WindowKind::Battle { attacker, .. } if attacker == player)
            });
        c.might + u8::from(high_ground) + u8::from(ambush) + self.item_dice(player, defending)
    }

    /// Dice `player` throws in an open Battle or Trial window, if they are
    /// in one: the most cards they may burn.
    pub fn battle_dice(&self, player: PlayerId) -> Option<u8> {
        self.windows.iter().find_map(|w| match w.kind {
            WindowKind::GuardBattle { attacker }
            | WindowKind::MobBattle { attacker, .. }
            | WindowKind::MilitiaBattle { attacker, .. }
                if attacker == player =>
            {
                Some(self.dice_for(player, false))
            }
            WindowKind::Trial { player: p, .. } if p == player => {
                Some(self.champions[player.0 as usize].might + self.item_trial_dice(player))
            }
            WindowKind::Battle { attacker, .. } if attacker == player => {
                Some(self.dice_for(player, false))
            }
            WindowKind::Battle { defender, .. } if defender == player => {
                Some(self.dice_for(player, true))
            }
            _ => None,
        })
    }

    /// `actor`'s step brought on a battle: they pay `cost` and wait for it.
    pub(super) fn start_battle(
        &mut self,
        actor: PlayerId,
        attacker: PlayerId,
        defender: PlayerId,
        cost: u32,
        ambush: Option<PlayerId>,
        events: &mut Vec<Event>,
    ) {
        self.turns[actor.0 as usize].move_points -= cost;
        events.push(Event::BattleStarted { attacker, defender });
        // A fight inside a truce breaks it (§7.3).
        self.break_truce(attacker, defender, events);
        self.note_bet(attacker, super::wish::Bet::Fight);
        self.note_bet(defender, super::wish::Bet::Fight);
        self.last_fight = self.round;
        // Attacking is loud (§6.5).
        self.add_threat(attacker, 1, events);
        self.record_deed(attacker, super::style::Deed::Attacked);
        self.record_deed(attacker, super::style::Deed::Fought);
        self.record_deed(defender, super::style::Deed::Fought);
        self.open_window(
            actor,
            WindowKind::Battle { attacker, defender },
            vec![attacker, defender],
            None,
            ambush,
            events,
        );
    }

    pub(super) fn check_burn(&self, player: PlayerId, cards: &[CardId]) -> Result<(), RuleError> {
        let hand = self.hand(player);
        for (i, card) in cards.iter().enumerate() {
            if !hand.contains(card) || cards[..i].contains(card) {
                return Err(RuleError::NotInHand);
            }
        }
        let max = self.battle_dice(player).unwrap_or(0);
        if cards.len() > max as usize {
            return Err(RuleError::TooManyBurned { max });
        }
        Ok(())
    }

    pub(super) fn resolve_battle(
        &mut self,
        attacker: PlayerId,
        defender: PlayerId,
        attacker_burn: Vec<CardId>,
        defender_burn: Vec<CardId>,
        events: &mut Vec<Event>,
    ) {
        self.battles += 1;
        // Every fight feeds the hunger (§5).
        self.offer(None, God::Trishna, 1, events);
        let a_faces = self.throw_side(attacker, false, attacker_burn, events);
        let d_faces = self.throw_side(defender, true, defender_burn, events);
        let (a_el, d_el) = (self.element_of(attacker), self.element_of(defender));
        self.element_breaks_ward(a_el, defender, &a_faces, events);
        self.element_breaks_ward(d_el, attacker, &d_faces, events);

        let mut attacker_score = self.score(&a_faces);
        let mut defender_score = self.score(&d_faces);
        // Armour adds its shields to whatever the dice gave (§20.3).
        attacker_score.shields += self.item_shields(attacker);
        defender_score.shields += self.item_shields(defender);
        events.push(Event::BattleResolved {
            attacker,
            defender,
            attacker_score,
            defender_score,
        });
        // Both sides take what got past the other's shields (§12.1).
        let to_defender = attacker_score.hits.saturating_sub(defender_score.shields);
        let to_attacker = defender_score.hits.saturating_sub(attacker_score.shields);
        if to_defender > 0 {
            self.damage(defender, to_defender, events);
        }
        if to_attacker > 0 {
            self.damage(attacker, to_attacker, events);
        }
        // Whoever dealt more takes Style from the other (§6.3).
        if to_defender > to_attacker {
            self.battle_style(attacker, defender, events);
        } else if to_attacker > to_defender {
            self.battle_style(defender, attacker, events);
        }
        // Trishna's Thirst: whoever drew more blood drinks a Spirit (§5.3).
        if self.law_active(super::Law::Thirst) && to_defender != to_attacker {
            let winner = if to_defender > to_attacker {
                attacker
            } else {
                defender
            };
            events.push(Event::Law {
                law: super::Law::Thirst,
                player: Some(winner),
                hex: None,
            });
            self.gain_spirit(winner, 1, events);
        }
    }

    /// Burned faces first, then physical throws for the remaining dice. Each
    /// Element face adds one more die, up to `MAX_EXPLOSIONS` times.
    pub(super) fn throw_side(
        &mut self,
        player: PlayerId,
        defending: bool,
        burned: Vec<CardId>,
        events: &mut Vec<Event>,
    ) -> Vec<Face> {
        let faces: Vec<Face> = burned.iter().map(|&c| self.def(c).burn_face()).collect();
        if !burned.is_empty() {
            events.push(Event::Burned {
                player,
                cards: burned.clone(),
                faces: faces.clone(),
            });
        }
        let count = self
            .dice_for(player, defending)
            .saturating_sub(burned.len() as u8);
        // A burned card is given to its god.
        for &card in &burned {
            if let Some(e) = self.def(card).element {
                self.offer(Some(player), God::from_index(e.index()), 1, events);
            }
        }
        self.discard.extend(burned);
        self.roll(Fighter::Champion(player), defending, count, faces, events)
    }

    /// Physical throws of `count` dice added to `faces`. Each Element face adds
    /// one more die, up to `MAX_EXPLOSIONS` times.
    pub(super) fn roll(
        &mut self,
        fighter: Fighter,
        defending: bool,
        count: u8,
        faces: Vec<Face>,
        events: &mut Vec<Event>,
    ) -> Vec<Face> {
        let label = [BATTLE_STREAM, self.battles, u64::from(defending)];
        self.roll_with(fighter, &label, count, faces, events)
    }

    /// Throws seeded from the match seed under `label`, one more seed per
    /// explosion depth.
    pub(super) fn roll_with(
        &mut self,
        fighter: Fighter,
        label: &[u64],
        mut count: u8,
        mut faces: Vec<Face>,
        events: &mut Vec<Event>,
    ) -> Vec<Face> {
        for depth in 0..=MAX_EXPLOSIONS {
            if count == 0 {
                break;
            }
            let mut key = label.to_vec();
            key.push(depth);
            let seed = Rng::derived(self.seed, &key).next_u64();
            let throw = necromy_dice::throw(seed, count);
            events.push(Event::DiceThrown {
                fighter,
                seed,
                count,
                faces: throw.faces.clone(),
            });
            count = throw.faces.iter().filter(|&&f| f == Face::Element).count() as u8;
            faces.extend(throw.faces);
        }
        faces
    }

    pub(super) fn score(&self, faces: &[Face]) -> Score {
        let day = self.time == TimeOfDay::Day;
        faces.iter().fold(Score::default(), |mut s, face| {
            match face {
                Face::Strike | Face::Element => s.hits += 1,
                Face::Sun if day => s.hits += 1,
                Face::Moon if !day => s.hits += 1,
                Face::Shield => s.shields += 1,
                Face::Sun | Face::Moon | Face::Blank => {}
            }
            s
        })
    }

    /// An Element face in the patron's element breaks a ward it quenches.
    fn element_of(&self, player: PlayerId) -> Element {
        self.champions[player.0 as usize].god.element()
    }

    pub(super) fn element_breaks_ward(
        &mut self,
        element: Element,
        target: PlayerId,
        faces: &[Face],
        events: &mut Vec<Event>,
    ) {
        if !faces.contains(&Face::Element) {
            return;
        }
        // The same blow breaks a worn item it quenches (§20.3).
        self.crack_item(target, element, events);
        let Some(ward) = self.champions[target.0 as usize].ward else {
            return;
        };
        if element == ward.quenched_by() {
            self.champ_mut(target).ward = None;
            events.push(Event::WardBroken {
                player: target,
                ward,
                by: element,
            });
        }
    }
}
