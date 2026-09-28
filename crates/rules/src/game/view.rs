//! What one player may see (docs/design.md §17.1).
//!
//! The server keeps the whole match and sends each client a view: the same
//! `Game`, with what that player cannot know taken out. The client reads it
//! with the same accessors and checks its own intents against it, so there
//! is no second model of the state to keep in step.
//!
//! Hidden: the seed and the generator (with them the rest of the match could
//! be predicted), the deck's order, rivals' hands and face-down traps, rivals'
//! secret conditions, rivals' choices in an open window, where hidden rivals
//! are (§11.6: they stand where last seen), and the log. Hidden
//! cards keep their ids, so hands keep their size, but their definitions are
//! shuffled among themselves with the server's salt: a client learns what is
//! left unseen, not who holds it.

use super::{Choice, Event, Game, Intent, Phase, PlayerId, Target};
use crate::cards::CardId;
use crate::rng::Rng;

impl Game {
    /// This match as `viewer` sees it; `None` for a spectator, who holds no
    /// cards. `salt` must be unknown to clients (the server's own randomness):
    /// it drives the shuffle that hides cards and the view's generator.
    pub fn view_for(&self, viewer: Option<PlayerId>, salt: u64) -> Game {
        let mut v = self.clone();
        let mine = |p: PlayerId| Some(p) == viewer;
        v.seed = 0;
        v.rng = Rng::derived(salt, &[u64::from(self.round), self.log.len() as u64]);
        v.log.clear();

        // Cards this viewer cannot see.
        let mut hidden: Vec<usize> = self.deck.iter().map(|c| c.0 as usize).collect();
        for (i, hand) in self.hands.iter().enumerate() {
            if !mine(PlayerId(i as u8)) {
                hidden.extend(hand.iter().map(|c| c.0 as usize));
            }
        }
        hidden.extend(
            self.traps
                .iter()
                .filter(|t| !mine(t.owner))
                .map(|t| t.card.0 as usize),
        );
        hidden.sort_unstable();
        let mut defs: Vec<_> = hidden.iter().map(|&i| self.defs[i]).collect();
        v.rng.shuffle(&mut defs);
        for (&i, def) in hidden.iter().zip(defs) {
            v.defs[i] = def;
        }
        // A change on a card nobody shows would give it away (§7.3).
        for &i in &hidden {
            v.mods.remove(&CardId(i as u32));
        }
        v.rng.shuffle(&mut v.deck);

        // A hidden rival stands where they were last seen (§11.6).
        for (i, c) in v.champions.iter_mut().enumerate() {
            if c.hidden && !mine(PlayerId(i as u8)) {
                c.hex = c.seen_at;
            }
        }
        // Rivals' secrets stay hidden, unless a wish told this viewer (§7.3).
        for (i, secret) in v.secrets.iter_mut().enumerate() {
            let p = PlayerId(i as u8);
            if !mine(p) && !viewer.is_some_and(|w| self.knows_secret(w, p)) {
                *secret = None;
            }
        }
        v.known.retain(|&(who, _)| mine(who));
        for window in &mut v.windows {
            for (p, choice) in window.choices.iter_mut() {
                if !mine(*p) {
                    *choice = Choice::Pass;
                }
            }
        }
        // A rival's held action would give away a card in hand or a step.
        for (i, turn) in v.turns.iter_mut().enumerate() {
            if !mine(PlayerId(i as u8))
                && let Phase::Held { intent, .. } = &mut turn.phase
            {
                *intent = Intent::EndTurn;
            }
        }
        v
    }

    /// `event` as `viewer` may hear it, given their `view` of the state after
    /// it: a rival's draw names the card as the view has it, and what a
    /// hidden rival does where nobody sees them (steps, blinks, traps, the
    /// hex a card lands on) does not reach them at all (§11.6).
    pub fn event_for(view: &Game, viewer: Option<PlayerId>, event: &Event) -> Option<Event> {
        let unseen = |p: PlayerId| Some(p) != viewer && view.is_hidden(p);
        Some(match event {
            Event::CardDrawn { player, card, .. } if Some(*player) != viewer => Event::CardDrawn {
                player: *player,
                card: *card,
                def: view.def_id(*card),
            },
            // What a wish showed one player of a rival's hand stays theirs.
            Event::HandSeen { player, about, .. } if Some(*player) != viewer => Event::HandSeen {
                player: *player,
                about: *about,
                cards: Vec::new(),
            },
            Event::Moved { player, .. }
            | Event::Blinked { player, .. }
            | Event::TrapSet { player, .. }
                if unseen(*player) =>
            {
                return None;
            }
            Event::CardPlayed {
                player,
                card,
                def,
                target: Target::Hex(_),
                response,
            } if unseen(*player) => Event::CardPlayed {
                player: *player,
                card: *card,
                def: *def,
                target: Target::None,
                response: *response,
            },
            other => other.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{God, Intent, Setup, bot};

    fn game() -> Game {
        Game::new(Setup {
            seed: 5,
            champions: God::ALL.to_vec(),
        })
        .0
    }

    #[test]
    fn a_view_keeps_own_things_and_hides_the_rest() {
        let g = game();
        let me = PlayerId(1);
        let v = g.view_for(Some(me), 42);
        assert_eq!(v.seed(), 0);
        assert!(v.log().is_empty());
        assert_eq!(v.secret(me), g.secret(me));
        assert!(v.secret(PlayerId(0)).is_none());
        let defs =
            |g: &Game, p: PlayerId| g.hand(p).iter().map(|&c| g.def_id(c)).collect::<Vec<_>>();
        assert_eq!(defs(&v, me), defs(&g, me));
        // Rivals' hands keep their size; what is unseen keeps its contents.
        let unseen = |g: &Game| {
            let mut d: Vec<_> = g
                .players()
                .filter(|&p| p != me)
                .flat_map(|p| defs(g, p))
                .chain(g.deck.iter().map(|&c| g.def_id(c)))
                .collect();
            d.sort();
            d
        };
        assert_eq!(unseen(&v), unseen(&g));
        for p in g.players() {
            assert_eq!(v.hand(p).len(), g.hand(p).len());
        }
    }

    #[test]
    fn the_hidden_cards_move_with_the_salt() {
        let g = game();
        let a = g.view_for(Some(PlayerId(1)), 1);
        let b = g.view_for(Some(PlayerId(1)), 2);
        let rivals = |v: &Game| {
            v.players()
                .filter(|&p| p != PlayerId(1))
                .flat_map(|p| v.hand(p).iter().map(|&c| v.def_id(c)).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };
        assert_ne!(rivals(&a), rivals(&b));
    }

    #[test]
    fn a_view_judges_intents_like_the_match() {
        let mut g = game();
        for _ in 0..300 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = bot::choose(&g, p);
            let seen = g.view_for(Some(p), 7).apply(p, intent.clone()).is_ok();
            let real = g.apply(p, intent.clone());
            assert_eq!(seen, real.is_ok(), "{intent:?}");
            // Someone else trying to act is refused in both.
            let other = g.players().find(|q| !g.awaiting().contains(q));
            if let Some(q) = other {
                assert!(g.view_for(Some(q), 7).apply(q, Intent::EndTurn).is_err());
            }
        }
    }

    #[test]
    fn rivals_choices_in_a_window_stay_hidden() {
        let mut g = game();
        for _ in 0..2000 {
            if let Some(w) = g.windows().first()
                && w.choices.values().any(|c| *c != Choice::Pass)
            {
                let chooser = *w
                    .choices
                    .iter()
                    .find(|(_, c)| **c != Choice::Pass)
                    .unwrap()
                    .0;
                let viewer = g.players().find(|&p| p != chooser).unwrap();
                let v = g.view_for(Some(viewer), 3);
                let vw = v.windows().first().unwrap();
                assert!(vw.has_chosen(chooser));
                assert!(
                    vw.choices
                        .iter()
                        .all(|(p, c)| *p == viewer || *c == Choice::Pass)
                );
                return;
            }
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let i = bot::choose(&g, p);
            g.apply(p, i).unwrap();
        }
        panic!("no window with a played card in 2000 steps");
    }
}
