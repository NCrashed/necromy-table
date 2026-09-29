//! Arenas and duels, bets between players and debts (docs/design.md
//! §21.8).
//!
//! A settlement may take an arena. Its owner, standing in it, challenges a
//! rival to a duel: the next battle between the two settles it, and a rival
//! who has not fought by the deadline loses by not coming. A duel the owner
//! wins counts for their arena.
//!
//! A champion may bet against a rival that the rival will do something
//! (fight, take land, fall, hide) before the next dusk. Whoever loses owes
//! the other; a debt is paid in Style, or cancelled by beating the creditor
//! in battle.

use serde::{Deserialize, Serialize};

use super::buildings::Building;
use super::wish::Bet;
use super::{Event, Game, PlayerId, RuleError, StyleReason};
use crate::features::Feature;

/// Rounds a challenged rival has to come.
pub const DUEL_ROUNDS: u32 = 4;
/// Duels won in one's arena for the Arena deed.
pub const ARENA_WINS: usize = 3;
/// Style a bet between players is for.
pub const BET_STAKE: u8 = 2;
/// Rivals in one's debt at once for Debt Bondage.
pub const DEBTORS: usize = 3;

/// A duel called in an arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duel {
    pub host: PlayerId,
    pub rival: PlayerId,
    pub until: u32,
}

/// A bet between two players.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerBet {
    pub by: PlayerId,
    pub on: PlayerId,
    pub bet: Bet,
    pub until: u32,
    pub happened: bool,
}

/// What `debtor` owes `creditor`, in Style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Debt {
    pub debtor: PlayerId,
    pub creditor: PlayerId,
    pub amount: u8,
}

impl Game {
    pub fn duels(&self) -> &[Duel] {
        &self.duels
    }

    pub fn player_bets(&self) -> &[PlayerBet] {
        &self.player_bets
    }

    pub fn debts(&self) -> &[Debt] {
        &self.debts
    }

    /// Duels `player` has won in their arena.
    pub fn arena_wins(&self, player: PlayerId) -> usize {
        self.arena_wins.get(player.0 as usize).copied().unwrap_or(0) as usize
    }

    /// Rivals in `player`'s debt.
    pub fn debtors(&self, player: PlayerId) -> usize {
        let mut who: Vec<PlayerId> = self
            .debts
            .iter()
            .filter(|d| d.creditor == player)
            .map(|d| d.debtor)
            .collect();
        who.sort_by_key(|p| p.0);
        who.dedup();
        who.len()
    }

    /// Rivals `player` could challenge: standing in an arena of theirs with
    /// no duel of theirs open.
    pub fn challengeable(&self, player: PlayerId) -> Vec<PlayerId> {
        let at = self.hex_of(player);
        let arena = self.building(at) == Some(Building::Arena) && self.owner(at) == Some(player);
        if !arena || self.duels.iter().any(|d| d.host == player) {
            return Vec::new();
        }
        self.players()
            .filter(|&p| p != player && !self.duels.iter().any(|d| d.rival == p))
            .collect()
    }

    pub(super) fn check_challenge(
        &self,
        player: PlayerId,
        rival: PlayerId,
    ) -> Result<(), RuleError> {
        if self.challengeable(player).contains(&rival) {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    pub(super) fn challenge(
        &mut self,
        player: PlayerId,
        rival: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_challenge(player, rival)?;
        self.duels.push(Duel {
            host: player,
            rival,
            until: self.round + DUEL_ROUNDS,
        });
        events.push(Event::Challenged {
            host: player,
            rival,
            hex: self.hex_of(player),
        });
        Ok(())
    }

    /// A battle between `winner` and `loser` settles a duel between them,
    /// and cancels what the winner owed the loser.
    pub(super) fn settle_by_battle(
        &mut self,
        winner: PlayerId,
        loser: PlayerId,
        events: &mut Vec<Event>,
    ) {
        if let Some(i) = self.duels.iter().position(|d| {
            (d.host, d.rival) == (winner, loser) || (d.host, d.rival) == (loser, winner)
        }) {
            let duel = self.duels.remove(i);
            if duel.host == winner {
                self.arena_wins[winner.0 as usize] += 1;
            }
            events.push(Event::DuelWon {
                winner,
                loser,
                host: duel.host,
            });
        }
        let before = self.debts.len();
        self.debts
            .retain(|d| !(d.debtor == winner && d.creditor == loser));
        if self.debts.len() < before {
            events.push(Event::DebtVoided {
                debtor: winner,
                creditor: loser,
            });
        }
    }

    /// Dusk: a rival who never came has lost the duel.
    pub(super) fn duels_at_dusk(&mut self, events: &mut Vec<Event>) {
        let round = self.round;
        let due: Vec<Duel> = self
            .duels
            .iter()
            .copied()
            .filter(|d| d.until <= round)
            .collect();
        self.duels.retain(|d| d.until > round);
        for d in due {
            self.arena_wins[d.host.0 as usize] += 1;
            self.add_style(d.rival, -1, StyleReason::Battle, events);
            events.push(Event::DuelForfeit {
                host: d.host,
                rival: d.rival,
            });
        }
    }

    /// Bets `player` could make now: against each rival without one open.
    pub fn bettable(&self, player: PlayerId) -> Vec<PlayerId> {
        if !self.has(Feature::Debts) {
            return Vec::new();
        }
        self.players()
            .filter(|&p| p != player)
            .filter(|&p| !self.player_bets.iter().any(|b| b.by == player && b.on == p))
            .collect()
    }

    pub(super) fn check_bet(&self, player: PlayerId, rival: PlayerId) -> Result<(), RuleError> {
        if self.bettable(player).contains(&rival) {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    pub(super) fn place_bet(
        &mut self,
        player: PlayerId,
        rival: PlayerId,
        bet: Bet,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_bet(player, rival)?;
        self.player_bets.push(PlayerBet {
            by: player,
            on: rival,
            bet,
            until: self.round + 2,
            happened: false,
        });
        events.push(Event::BetPlaced {
            by: player,
            on: rival,
            bet,
        });
        Ok(())
    }

    /// `player` did `bet`: bets on it are won.
    pub(super) fn note_player_bet(&mut self, player: PlayerId, bet: Bet) {
        for b in &mut self.player_bets {
            if b.on == player && b.bet == bet {
                b.happened = true;
            }
        }
    }

    /// Dusk: bets come due; the loser owes the winner.
    pub(super) fn settle_player_bets(&mut self, events: &mut Vec<Event>) {
        let round = self.round;
        let due: Vec<PlayerBet> = self
            .player_bets
            .iter()
            .copied()
            .filter(|b| b.until <= round)
            .collect();
        self.player_bets.retain(|b| b.until > round);
        for b in due {
            let (debtor, creditor) = if b.happened {
                (b.on, b.by)
            } else {
                (b.by, b.on)
            };
            self.owe(debtor, creditor, BET_STAKE);
            events.push(Event::BetSettled {
                by: b.by,
                on: b.on,
                bet: b.bet,
                won: b.happened,
            });
        }
    }

    fn owe(&mut self, debtor: PlayerId, creditor: PlayerId, amount: u8) {
        match self
            .debts
            .iter_mut()
            .find(|d| d.debtor == debtor && d.creditor == creditor)
        {
            Some(d) => d.amount = d.amount.saturating_add(amount),
            None => self.debts.push(Debt {
                debtor,
                creditor,
                amount,
            }),
        }
    }

    pub(super) fn check_pay(&self, player: PlayerId, creditor: PlayerId) -> Result<(), RuleError> {
        if self
            .debts
            .iter()
            .any(|d| d.debtor == player && d.creditor == creditor)
        {
            Ok(())
        } else {
            Err(RuleError::InvalidTarget)
        }
    }

    /// `player` pays what they owe `creditor`, in Style.
    pub(super) fn pay_debt(
        &mut self,
        player: PlayerId,
        creditor: PlayerId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        self.check_pay(player, creditor)?;
        let i = self
            .debts
            .iter()
            .position(|d| d.debtor == player && d.creditor == creditor)
            .expect("checked");
        let debt = self.debts.remove(i);
        let amount = i16::from(debt.amount);
        self.add_style(player, -amount, StyleReason::Battle, events);
        self.add_style(creditor, amount, StyleReason::Battle, events);
        events.push(Event::DebtPaid {
            debtor: player,
            creditor,
            amount: debt.amount,
        });
        Ok(())
    }
}
