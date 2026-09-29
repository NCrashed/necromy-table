//! Timers, so one player cannot hold the table (docs/design.md §17.1).
//!
//! Only people's seats have clocks. A turn has its own clock, which stands
//! still while the seat waits (on a window, or held on a rival, §11.2); each window a seat may answer has one; the
//! wish has one while dusk waits for it (§21.4), which stands still while
//! a god thinks about words already sent. When a clock runs out the table does the plainest
//! thing for the seat: pass (in a battle, throw with nothing burned), end
//! the turn, or refuse the wish. It never plays for them.

use necromy_rules::{Event, Game, Intent, PlayerId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timers {
    pub turn: f32,
    pub window: f32,
    pub wish: f32,
}

impl Default for Timers {
    fn default() -> Timers {
        Timers {
            turn: 90.0,
            window: 20.0,
            wish: 60.0,
        }
    }
}

/// What a seat's running clock is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Turn,
    Window,
    Wish,
}

/// The clock a seat should see: what it is for and seconds left.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    pub what: Decision,
    pub left: f32,
}

#[derive(Clone, Default)]
pub(crate) struct SeatClock {
    turn: Option<f32>,
    window: Option<f32>,
    wish: Option<f32>,
}

impl SeatClock {
    /// After a change: start, restart or stop this seat's clocks.
    pub(crate) fn follow(&mut self, t: &Timers, game: &Game, seat: PlayerId, events: &[Event]) {
        let window = game.to_answer(seat).is_some();
        let free = game.free_to_act(seat);
        let wish = game.wishing().contains(&seat);

        if events
            .iter()
            .any(|e| matches!(e, Event::TurnStarted { player, .. } if *player == seat))
        {
            self.turn = Some(t.turn);
        } else if events
            .iter()
            .any(|e| matches!(e, Event::TurnEnded { player } if *player == seat))
        {
            self.turn = None;
        } else if free && self.turn.is_none() {
            // A seat taken back mid-turn gets a fresh clock.
            self.turn = Some(t.turn);
        }

        let opened = events
            .iter()
            .any(|e| matches!(e, Event::WindowOpened { .. }));
        self.window = match (window && !wish, self.window) {
            (false, _) => None,
            (true, Some(left)) if !opened => Some(left),
            (true, _) => Some(t.window),
        };

        self.wish = match (wish, self.wish) {
            (false, _) => None,
            (true, Some(left)) => Some(left),
            (true, None) => Some(t.wish),
        };
    }

    pub(crate) fn clear(&mut self) {
        *self = SeatClock::default();
    }

    /// The clock the seat should look at, if any runs for it now.
    pub(crate) fn shown(&self, game: &Game, seat: PlayerId) -> Option<Clock> {
        if let Some(left) = self.wish {
            return Some(Clock {
                what: Decision::Wish,
                left,
            });
        }
        if let Some(left) = self.window {
            return Some(Clock {
                what: Decision::Window,
                left,
            });
        }
        match self.turn {
            Some(left) if game.free_to_act(seat) => Some(Clock {
                what: Decision::Turn,
                left,
            }),
            _ => None,
        }
    }

    /// Time passes; the intent to make for the seat if a clock ran out.
    /// `thinking`: a god is judging this seat's wish right now.
    pub(crate) fn run(
        &mut self,
        dt: f32,
        game: &Game,
        seat: PlayerId,
        thinking: bool,
    ) -> Option<Intent> {
        if let Some(left) = self.wish.as_mut() {
            if !thinking {
                *left -= dt;
            }
            return (*left <= 0.0).then_some(Intent::RefuseWish);
        }
        if let Some(left) = self.window.as_mut() {
            *left -= dt;
            if *left > 0.0 {
                return None;
            }
            return Some(if game.battle_dice(seat).is_some() {
                Intent::Burn { cards: Vec::new() }
            } else {
                Intent::Pass
            });
        }
        match self.turn.as_mut() {
            Some(left) if game.free_to_act(seat) => {
                *left -= dt;
                (*left <= 0.0).then_some(Intent::EndTurn)
            }
            _ => None,
        }
    }
}
