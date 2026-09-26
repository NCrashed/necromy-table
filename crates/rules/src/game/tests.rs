use super::*;
use crate::cards::POOL;

fn five() -> Setup {
    Setup {
        seed: 7,
        champions: God::ALL.to_vec(),
    }
}

fn def_named(name: &str) -> DefId {
    DefId(
        POOL.iter()
            .position(|d| d.name == name)
            .unwrap_or_else(|| panic!("no card {name}")) as u16,
    )
}

impl Game {
    /// Puts a fresh copy of a card into a hand.
    fn give(&mut self, player: PlayerId, name: &str) -> CardId {
        let card = CardId(self.defs.len() as u32);
        self.defs.push(def_named(name));
        self.hands[player.0 as usize].push(card);
        card
    }

    fn place(&mut self, player: PlayerId, hex: Hex) {
        assert!(self.occupant(hex).is_none_or(|p| p == player));
        self.champ_mut(player).hex = hex;
    }

    /// Everyone still owed a choice passes.
    fn pass_all(&mut self) {
        while self.window.is_some() {
            let p = self.awaiting()[0];
            self.apply(p, Intent::Pass).unwrap();
        }
    }

    fn end_turn_and_settle(&mut self) {
        self.apply(self.current_player(), Intent::EndTurn).unwrap();
        self.pass_all();
    }
}

/// The current player and the next one, standing `distance` apart near the
/// centre with empty hands.
fn duel(distance: i32) -> (Game, PlayerId, PlayerId) {
    let (mut g, _) = Game::new(five());
    let me = g.current_player();
    let foe = g.order()[1];
    for p in g.players().collect::<Vec<_>>() {
        g.hands[p.0 as usize].clear();
    }
    // Park everyone else on the far edge, out of reaction range.
    let parking = [Hex::new(-5, 0), Hex::new(-5, 2), Hex::new(-5, 4)];
    let others: Vec<PlayerId> = g.players().filter(|&p| p != me && p != foe).collect();
    for (p, hex) in others.into_iter().zip(parking) {
        g.champ_mut(p).hex = hex;
    }
    g.place(me, Hex::new(0, 0));
    g.place(foe, Hex::new(distance, 0));
    // Plain ground on the line between them, so steps cost 1.
    for hex in [1, 2, 3, 4]
        .map(|q| Hex::new(q, 0))
        .into_iter()
        .chain([Hex::new(0, 1)])
    {
        g.board.tile_mut(hex).unwrap().terrain = Terrain::Plains;
    }
    (g, me, foe)
}

#[test]
fn same_seed_same_match() {
    let (a, ea) = Game::new(five());
    let (b, eb) = Game::new(five());
    assert_eq!(ea, eb);
    assert_eq!(a.order(), b.order());
    assert_eq!(a.slice(), b.slice());
}

#[test]
fn replay_from_intents_matches() {
    let (mut a, _) = Game::new(five());
    let mut intents = Vec::new();
    for _ in 0..400 {
        let p = a.awaiting()[0];
        let intent = crate::bot::choose(&a, p);
        intents.push((p, intent));
        a.apply(p, intent).unwrap();
    }
    let (mut b, _) = Game::new(five());
    for (p, intent) in intents {
        b.apply(p, intent).unwrap();
    }
    assert_eq!(a.log(), b.log());
    assert!(a.round() > 2, "bots should keep the table moving");
}

#[test]
fn everyone_starts_with_a_hand() {
    let (g, _) = Game::new(five());
    for p in g.players() {
        assert_eq!(g.hand(p).len(), g.champion(p).unwrap().hand_limit());
    }
}

#[test]
fn only_the_current_player_acts() {
    let (mut game, _) = Game::new(five());
    let other = game
        .players()
        .find(|&p| p != game.current_player())
        .unwrap();
    assert_eq!(
        game.apply(other, Intent::EndTurn),
        Err(RuleError::NotYourTurn)
    );
}

#[test]
fn moves_cost_points_and_must_be_adjacent() {
    let (mut game, _) = Game::new(five());
    let p = game.current_player();
    let at = game.champion(p).unwrap().hex;
    assert_eq!(
        game.apply(p, Intent::Move { to: Hex::ZERO }),
        Err(RuleError::NotAdjacent)
    );
    let (&to, &cost) = game
        .reachable()
        .iter()
        .find(|(h, _)| h.unsigned_distance_to(at) == 1)
        .unwrap();
    game.apply(p, Intent::Move { to }).unwrap();
    game.pass_all();
    assert_eq!(game.move_points(), MOVE_POINTS - cost);
}

#[test]
fn rounds_alternate_and_initiative_rotates() {
    let (mut game, _) = Game::new(five());
    assert_eq!((game.round(), game.time()), (1, TimeOfDay::Day));
    let first = game.order()[0];
    for _ in 0..5 {
        game.end_turn_and_settle();
    }
    assert_eq!((game.round(), game.time()), (2, TimeOfDay::Night));
    assert_eq!(*game.order().last().unwrap(), first);
}

#[test]
fn untouched_corpses_grow_groves() {
    let (mut game, _) = Game::new(five());
    let mut grew = false;
    for _ in 0..(5 * GROVE_AGE as usize) {
        game.end_turn_and_settle();
        grew |= game
            .log()
            .iter()
            .any(|e| matches!(e, Event::GroveGrew { .. }));
    }
    assert!(grew);
}

#[test]
fn end_window_waits_for_every_rival_and_hides_choices() {
    let (mut g, me, _) = duel(1);
    let events = g.apply(me, Intent::EndTurn).unwrap();
    assert!(matches!(
        events.last(),
        Some(Event::WindowOpened {
            kind: WindowKind::End { .. },
            ..
        })
    ));
    assert_eq!(
        g.current_player(),
        me,
        "turn passes only when the window closes"
    );
    let waiting = g.awaiting();
    assert_eq!(waiting.len(), 4);
    for (i, p) in waiting.iter().enumerate() {
        let events = g.apply(*p, Intent::Pass).unwrap();
        let closed = events
            .iter()
            .any(|e| matches!(e, Event::WindowClosed { .. }));
        assert_eq!(closed, i == 3);
    }
    assert_ne!(g.current_player(), me);
}

#[test]
fn only_nearby_rivals_get_an_enter_window() {
    let (mut g, me, foe) = duel(4);
    let step = Hex::new(1, 0);
    let events = g.apply(me, Intent::Move { to: step }).unwrap();
    // The foe is now 3 away: out of reaction range.
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::WindowOpened { .. }))
    );
    let step2 = Hex::new(2, 0);
    let events = g.apply(me, Intent::Move { to: step2 }).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::WindowOpened { kind: WindowKind::Enter { .. }, eligible } if eligible == &vec![foe]
    )));
    assert_eq!(g.apply(me, Intent::EndTurn), Err(RuleError::WindowOpen));
}

#[test]
fn instant_in_enter_window_hits_the_mover() {
    let (mut g, me, foe) = duel(3);
    let spark = g.give(foe, "Искра");
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    let hp = g.champion(me).unwrap().hp;
    g.apply(
        foe,
        Intent::Play {
            card: spark,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    assert_eq!(g.champion(me).unwrap().hp, hp - 1);
    assert!(g.window().is_none());
}

#[test]
fn own_turn_cards_are_not_instants() {
    let (mut g, me, foe) = duel(2);
    let fire = g.give(foe, "Пламя пира");
    g.apply(me, Intent::Move { to: Hex::new(0, 1) }).unwrap();
    assert_eq!(
        g.apply(
            foe,
            Intent::Play {
                card: fire,
                target: Target::Champion(me)
            }
        ),
        Err(RuleError::WrongTiming)
    );
    let answer = g.give(me, "Тишь");
    g.pass_all();
    assert_eq!(
        g.apply(
            me,
            Intent::Play {
                card: answer,
                target: Target::None
            }
        ),
        Err(RuleError::WrongTiming)
    );
}

#[test]
fn ward_stops_all_but_its_quencher() {
    let (mut g, me, foe) = duel(2);
    g.champ_mut(foe).ward = Some(Element::Metal);
    // Metal is quenched by fire only. Wood thorns bounce off.
    let thorns = g.give(me, "Шипы чащи");
    g.apply(
        me,
        Intent::Play {
            card: thorns,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    g.pass_all();
    let body = g.champion(foe).unwrap().body;
    assert_eq!(g.champion(foe).unwrap().hp, body);
    assert_eq!(g.champion(foe).unwrap().ward, Some(Element::Metal));

    let spark = g.give(me, "Искра");
    g.apply(
        me,
        Intent::Play {
            card: spark,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    g.pass_all();
    assert_eq!(g.champion(foe).unwrap().ward, None);
    assert!(g.champion(foe).unwrap().hp < body);
}

#[test]
fn response_cancels_the_pending_card() {
    let (mut g, me, foe) = duel(2);
    g.champ_mut(me).spirit_points = 3;
    let fire = g.give(me, "Пламя пира");
    let mirage = g.give(foe, "Морок");
    g.champ_mut(foe).spirit_points = 1;
    g.apply(
        me,
        Intent::Play {
            card: fire,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    assert!(matches!(
        g.window().unwrap().kind,
        WindowKind::Target { .. }
    ));
    let events = g
        .apply(
            foe,
            Intent::Play {
                card: mirage,
                target: Target::None,
            },
        )
        .unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Canceled { .. })));
    let body = g.champion(foe).unwrap().body;
    assert_eq!(g.champion(foe).unwrap().hp, body);
}

#[test]
fn growth_breaks_through_the_grave() {
    // Earth's cancel cannot silence wood: wood quenches earth.
    let (mut g, me, foe) = duel(2);
    let thorns = g.give(me, "Шипы чащи");
    let hush = g.give(foe, "Тишь");
    g.champ_mut(foe).spirit_points = 1;
    g.apply(
        me,
        Intent::Play {
            card: thorns,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    let events = g
        .apply(
            foe,
            Intent::Play {
                card: hush,
                target: Target::None,
            },
        )
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::CancelFailed { .. }))
    );
    let body = g.champion(foe).unwrap().body;
    assert_eq!(g.champion(foe).unwrap().hp, body - 1);
}

#[test]
fn response_ward_lands_before_the_blow() {
    let (mut g, me, foe) = duel(2);
    let thorns = g.give(me, "Шипы чащи");
    let oath = g.give(foe, "Присяга");
    g.apply(
        me,
        Intent::Play {
            card: thorns,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    g.apply(
        foe,
        Intent::Play {
            card: oath,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    let body = g.champion(foe).unwrap().body;
    assert_eq!(g.champion(foe).unwrap().hp, body, "metal ward stops wood");
}

#[test]
fn generation_chain_adds_one_and_may_break_rhythm() {
    let (mut g, me, foe) = duel(2);
    // Enough health to take both hits without falling.
    g.champ_mut(foe).body = 9;
    g.champ_mut(foe).hp = 9;
    g.champ_mut(me).spirit_points = 3;
    let seed = g.give(me, "Бинт"); // neutral: no chain
    let thorns = g.give(me, "Шипы чащи"); // wood
    let spark = g.give(me, "Искра"); // fire: wood → fire, yang-yang
    g.apply(
        me,
        Intent::Play {
            card: seed,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    g.apply(
        me,
        Intent::Play {
            card: thorns,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    g.pass_all();
    let hp = g.champion(foe).unwrap().hp;
    let events = g
        .apply(
            me,
            Intent::Play {
                card: spark,
                target: Target::Champion(foe),
            },
        )
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Chain {
            from: Element::Wood,
            to: Element::Fire,
            rhythm_broken: true,
            ..
        }
    )));
    assert_eq!(g.champion(me).unwrap().spirit_points, 2, "qi surge");
    g.pass_all();
    assert_eq!(g.champion(foe).unwrap().hp, hp.saturating_sub(2));
}

#[test]
fn traps_spring_on_rivals_only() {
    let (mut g, me, foe) = duel(3);
    let registry = g.give(me, "Реестр");
    let trap_hex = Hex::new(1, 0);
    g.apply(
        me,
        Intent::Play {
            card: registry,
            target: Target::Hex(trap_hex),
        },
    )
    .unwrap();
    assert_eq!(g.traps().len(), 1);
    g.end_turn_and_settle();
    while g.current_player() != foe {
        g.end_turn_and_settle();
    }
    let hp = g.champion(foe).unwrap().hp;
    g.apply(foe, Intent::Move { to: Hex::new(2, 0) }).unwrap();
    g.pass_all();
    g.apply(foe, Intent::Move { to: trap_hex }).unwrap();
    assert_eq!(g.champion(foe).unwrap().hp, hp.saturating_sub(2));
    assert!(g.traps().is_empty());
}

#[test]
fn falling_leaves_a_body_and_wakes_at_home() {
    let (mut g, me, foe) = duel(2);
    g.champ_mut(foe).hp = 1;
    let spark = g.give(me, "Искра");
    g.apply(
        me,
        Intent::Play {
            card: spark,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    g.pass_all();
    let fallen_at = Hex::new(2, 0);
    let foe_c = g.champion(foe).unwrap();
    assert_eq!(foe_c.hp, foe_c.body);
    assert_ne!(foe_c.hex, fallen_at);
    assert!(g.board().tile(fallen_at).unwrap().corpse.is_some());
}

#[test]
fn root_in_the_end_window_costs_the_next_turn() {
    let (mut g, me, foe) = duel(2);
    let burden = g.give(foe, "Бремя");
    g.apply(me, Intent::EndTurn).unwrap();
    g.apply(
        foe,
        Intent::Play {
            card: burden,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    g.pass_all();
    assert!(g.champion(me).unwrap().rooted);
    while g.current_player() != me {
        g.end_turn_and_settle();
    }
    assert_eq!(g.move_points(), 0);
}

#[test]
fn body_cards_consume_the_corpse_underfoot() {
    let (mut g, me, _) = duel(3);
    let here = g.champion(me).unwrap().hex;
    g.board.tile_mut(here).unwrap().corpse = Some(Corpse { age: 0 });
    let fuel = g.give(me, "Сжечь как топливо");
    g.champ_mut(me).spirit_points = 0;
    g.apply(
        me,
        Intent::Play {
            card: fuel,
            target: Target::Hex(here),
        },
    )
    .unwrap();
    assert!(g.board().tile(here).unwrap().corpse.is_none());
    assert_eq!(g.champion(me).unwrap().spirit_points, 2);
    assert_eq!(g.move_points(), MOVE_POINTS + 1);
}

#[test]
fn bots_never_stall_or_break_rules() {
    for seed in 0..30 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        for _ in 0..1500 {
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent)
                .unwrap_or_else(|e| panic!("seed {seed}: bot {p:?} {intent:?}: {e}"));
        }
        assert!(
            g.round() >= 6,
            "seed {seed}: only reached round {}",
            g.round()
        );
        let played = g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::CardPlayed { .. }))
            .count();
        assert!(played > 10, "seed {seed}: bots played only {played} cards");
    }
}
