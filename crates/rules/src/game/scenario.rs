//! Scripted scenes: a small board set up by hand, for the tutorial's
//! chapters. The match is a normal `Game` (every rule applies), only the
//! world holds still (`scripted`): no bodies appear, no guard marches, no
//! stories, Crown or wishes, and hands are what the scene deals, no draws.

use hexx::Hex;

use super::{Event, Game, PlayerId, Setup, TimeOfDay};
use crate::board::{Board, Corpse, Terrain};
use crate::cards::{CardId, DefId, POOL};
use crate::gods::God;

/// One champion in a scene.
#[derive(Clone, Debug)]
pub struct SceneSeat {
    pub god: God,
    pub hex: Hex,
    /// Cards in hand, by name, in order.
    pub hand: Vec<&'static str>,
    /// Health; `None` keeps the champion's full body.
    pub hp: Option<u8>,
    /// Spirit left to spend; `None` keeps it full.
    pub spirit: Option<u8>,
}

impl SceneSeat {
    pub fn new(god: God, hex: Hex) -> SceneSeat {
        SceneSeat {
            god,
            hex,
            hand: Vec::new(),
            hp: None,
            spirit: None,
        }
    }

    pub fn hand(mut self, cards: &[&'static str]) -> SceneSeat {
        self.hand = cards.to_vec();
        self
    }

    pub fn hp(mut self, hp: u8) -> SceneSeat {
        self.hp = Some(hp);
        self
    }

    pub fn spirit(mut self, spirit: u8) -> SceneSeat {
        self.spirit = Some(spirit);
        self
    }
}

/// A scene: the board and who sits where. The first seat acts first.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub radius: u32,
    pub terrain: Vec<(Hex, Terrain)>,
    pub corpses: Vec<Hex>,
    pub seats: Vec<SceneSeat>,
    /// Each god's stage (0 light .. 2 dark), in ring order.
    pub stages: [u8; 5],
}

impl Scenario {
    pub fn new(radius: u32, seats: Vec<SceneSeat>) -> Scenario {
        Scenario {
            radius,
            terrain: Vec::new(),
            corpses: Vec::new(),
            seats,
            // Light: the gentlest laws, none that hides or harms by itself.
            stages: [0; 5],
        }
    }
}

/// The pool's card of this name. Scenes name their cards; a wrong name
/// is a bug in the scene.
fn def_named(name: &str) -> DefId {
    DefId(
        POOL.iter()
            .position(|d| d.name == name)
            .unwrap_or_else(|| panic!("no card named {name}")) as u16,
    )
}

impl Game {
    /// A match set up as `scene` says, on the first day. Everyone acts.
    pub fn scenario(scene: &Scenario) -> (Game, Vec<Event>) {
        let (mut game, _) = Game::new(Setup {
            seed: 1,
            champions: scene.seats.iter().map(|s| s.god).collect(),
        });
        game.scripted = true;
        game.board = Board::plain(scene.radius);
        for &(hex, terrain) in &scene.terrain {
            if let Some(tile) = game.board.tile_mut(hex) {
                tile.terrain = terrain;
            }
        }
        for &hex in &scene.corpses {
            if let Some(tile) = game.board.tile_mut(hex) {
                tile.corpse = Some(Corpse { age: 0 });
            }
        }
        game.deck.clear();
        game.discard.clear();
        game.traps.clear();
        game.open.clear();
        game.secrets = vec![None; scene.seats.len()];
        game.lines.clear();
        game.guard = None;
        game.dominant = None;
        game.claims.clear();
        game.pantheon.stages = scene.stages;
        game.pantheon.pressure = [0; 5];
        game.order = (0..scene.seats.len() as u8).map(PlayerId).collect();
        for (i, seat) in scene.seats.iter().enumerate() {
            let player = PlayerId(i as u8);
            game.board.set_start(seat.god, seat.hex);
            let champ = game.champ_mut(player);
            champ.hex = seat.hex;
            champ.seen_at = seat.hex;
            if let Some(hp) = seat.hp {
                champ.hp = hp.min(champ.body);
            }
            if let Some(spirit) = seat.spirit {
                champ.spirit_points = spirit.min(champ.spirit);
            }
            game.hands[i].clear();
            for name in &seat.hand {
                let card = CardId(game.defs.len() as u32);
                game.defs.push(def_named(name));
                game.hands[i].push(card);
            }
        }
        debug_assert_eq!(game.time, TimeOfDay::Day);
        let mut events = vec![Event::RoundStarted {
            round: game.round,
            time: game.time,
            order: game.order.clone(),
        }];
        for player in game.order.clone() {
            events.push(Event::TurnStarted {
                player,
                move_points: game.move_points(player),
            });
        }
        game.log = events.clone();
        (game, events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Intent;

    fn scene() -> Scenario {
        let mut s = Scenario::new(
            3,
            vec![
                SceneSeat::new(God::Trishna, Hex::new(0, 3)).hand(&["Искра"]),
                SceneSeat::new(God::Zaga, Hex::new(0, 1)),
            ],
        );
        s.terrain.push((Hex::new(1, 2), Terrain::Forest));
        s
    }

    #[test]
    fn a_scene_is_set_as_written_and_holds_still() {
        let (mut g, _) = Game::scenario(&scene());
        let me = PlayerId(0);
        let foe = PlayerId(1);
        assert_eq!(g.board().radius(), 3);
        assert_eq!(g.champion(me).unwrap().hex, Hex::new(0, 3));
        assert_eq!(g.hand(me).len(), 1);
        assert_eq!(g.def(g.hand(me)[0]).name, "Искра");
        assert!(g.hand(foe).is_empty());
        assert_eq!(
            g.board().tile(Hex::new(1, 2)).unwrap().terrain,
            Terrain::Forest
        );
        assert!(g.free_to_act(me) && g.free_to_act(foe));
        // Several rounds pass: nothing appears, nobody draws, nobody wins.
        for _ in 0..6 {
            g.apply(foe, Intent::EndTurn).unwrap();
            g.apply(me, Intent::EndTurn).unwrap();
        }
        assert_eq!(g.board().corpses().count(), 0);
        assert!(g.hand(foe).is_empty());
        assert_eq!(g.hand(me).len(), 1);
        assert!(g.guard().is_none());
        assert!(g.winner().is_none());
        assert!(g.wish_due().is_none());
    }
}
