use super::*;
use crate::cards::POOL;
use necromy_dice::Face;

use crate::game::Condition;

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
        if let Some(d) = self.wish_due {
            self.apply(d, Intent::RefuseWish).unwrap();
        }
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
    // Mid stages: cards work exactly as printed (§5).
    g.pantheon.stages = [1; 5];
    g.pantheon.pressure = [0; 5];
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
        if a.winner().is_some() {
            break;
        }
        let p = a.awaiting()[0];
        let intent = crate::bot::choose(&a, p);
        intents.push((p, intent.clone()));
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
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent.clone())
                .unwrap_or_else(|e| panic!("seed {seed}: bot {p:?} {intent:?}: {e}"));
        }
        assert!(
            g.winner().is_some() || g.round() >= 6,
            "seed {seed}: only reached round {} and nobody won",
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

// ---- Battles (§12) ----

fn start_battle(g: &mut Game, me: PlayerId, foe: PlayerId) -> Vec<Event> {
    let at = g.champion(foe).unwrap().hex;
    g.apply(me, Intent::Move { to: at }).unwrap()
}

#[test]
fn stepping_onto_a_rival_opens_a_battle() {
    let (mut g, me, foe) = duel(1);
    assert_eq!(g.attackable(), vec![Hex::new(1, 0)]);
    let events = start_battle(&mut g, me, foe);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::WindowOpened { kind: WindowKind::Battle { .. }, eligible } if eligible == &vec![me, foe]
    )));
    // Nobody moved: the attacker stays on their own hex.
    assert_eq!(g.champion(me).unwrap().hex, Hex::ZERO);
    let spark = g.give(foe, "Искра");
    assert_eq!(
        g.apply(
            foe,
            Intent::Play {
                card: spark,
                target: Target::Champion(me)
            }
        ),
        Err(RuleError::WrongTiming)
    );
    let third = g.players().find(|&p| p != me && p != foe).unwrap();
    assert_eq!(g.apply(third, Intent::Pass), Err(RuleError::NotYourTurn));
}

#[test]
fn burns_are_checked() {
    let (mut g, me, foe) = duel(1);
    let might = g.champion(me).unwrap().might;
    let cards: Vec<CardId> = (0..=might).map(|_| g.give(me, "Искра")).collect();
    start_battle(&mut g, me, foe);
    assert_eq!(
        g.apply(
            me,
            Intent::Burn {
                cards: cards.clone()
            }
        ),
        Err(RuleError::TooManyBurned { max: might })
    );
    assert_eq!(
        g.apply(
            me,
            Intent::Burn {
                cards: vec![cards[0], cards[0]]
            }
        ),
        Err(RuleError::NotInHand)
    );
    assert_eq!(
        g.apply(
            me,
            Intent::Burn {
                cards: vec![CardId(9999)]
            }
        ),
        Err(RuleError::NotInHand)
    );
    g.apply(
        me,
        Intent::Burn {
            cards: vec![cards[0]],
        },
    )
    .unwrap();
    assert!(!g.hand(me).contains(&cards[0]));
}

#[test]
fn battle_damage_follows_the_faces_both_ways() {
    for seed in 0..40 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        let me = g.current_player();
        let foe = g.order()[1];
        g.place(me, Hex::ZERO);
        g.place(foe, Hex::new(1, 0));
        g.champ_mut(me).body = 20;
        g.champ_mut(me).hp = 20;
        g.champ_mut(foe).body = 20;
        g.champ_mut(foe).hp = 20;
        start_battle(&mut g, me, foe);
        g.apply(me, Intent::Burn { cards: vec![] }).unwrap();
        let events = g.apply(foe, Intent::Burn { cards: vec![] }).unwrap();

        let (a, d) = events
            .iter()
            .find_map(|e| match e {
                Event::BattleResolved {
                    attacker_score,
                    defender_score,
                    ..
                } => Some((*attacker_score, *defender_score)),
                _ => None,
            })
            .expect("battle resolved");
        assert_eq!(
            g.champion(foe).unwrap().hp,
            20 - a.hits.saturating_sub(d.shields)
        );
        assert_eq!(
            g.champion(me).unwrap().hp,
            20 - d.hits.saturating_sub(a.shields)
        );
        assert_eq!(g.move_points(), 0, "a battle ends the movement");

        // Every throw replays to the same faces on a client (§12.2), and each
        // explosion throws exactly as many dice as Element faces came up.
        let throws: Vec<(Fighter, u64, u8, Vec<Face>)> = events
            .iter()
            .filter_map(|e| match e {
                Event::DiceThrown {
                    fighter,
                    seed,
                    count,
                    faces,
                } => Some((*fighter, *seed, *count, faces.clone())),
                _ => None,
            })
            .collect();
        for (fighter, s, count, faces) in &throws {
            assert_eq!(&necromy_dice::throw(*s, *count).faces, faces);
            assert!(*fighter == Fighter::Champion(me) || *fighter == Fighter::Champion(foe));
        }
        for pair in throws.windows(2) {
            let ((p1, _, _, f1), (p2, _, c2, _)) = (&pair[0], &pair[1]);
            if p1 == p2 {
                let elements = f1.iter().filter(|&&f| f == Face::Element).count();
                assert_eq!(*c2 as usize, elements, "seed {seed}");
            }
        }
    }
}

#[test]
fn burned_cards_give_their_faces() {
    let (mut g, me, foe) = duel(1);
    // Strip the dice: every face comes from burned cards.
    g.champ_mut(me).might = 2;
    let tricks = vec![g.give(me, "Искра"), g.give(me, "Бинт")];
    start_battle(&mut g, me, foe);
    g.apply(
        me,
        Intent::Burn {
            cards: tricks.clone(),
        },
    )
    .unwrap();
    let events = g.apply(foe, Intent::Burn { cards: vec![] }).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Burned { player, faces, .. } if *player == me && faces == &vec![Face::Strike, Face::Strike]
    )));
    assert!(
        !events.iter().any(
            |e| matches!(e, Event::DiceThrown { fighter, .. } if *fighter == Fighter::Champion(me))
        ),
        "all of the attacker's dice were replaced"
    );
    let a = events
        .iter()
        .find_map(|e| match e {
            Event::BattleResolved { attacker_score, .. } => Some(*attacker_score),
            _ => None,
        })
        .unwrap();
    assert_eq!(a.hits, 2);
}

#[test]
fn defending_on_a_mountain_adds_a_die() {
    let (mut g, me, foe) = duel(1);
    let might = g.champion(foe).unwrap().might;
    g.board.tile_mut(Hex::new(1, 0)).unwrap().terrain = Terrain::Mountain;
    g.move_points = MOVE_POINTS;
    start_battle(&mut g, me, foe);
    assert_eq!(g.battle_dice(foe), Some(might + 1));
    assert_eq!(g.battle_dice(me), Some(g.champion(me).unwrap().might));
}

#[test]
fn bots_fight() {
    let battles: usize = (0..30)
        .map(|seed| {
            let (mut g, _) = Game::new(Setup {
                seed,
                champions: God::ALL.to_vec(),
            });
            for _ in 0..1500 {
                if g.winner().is_some() {
                    break;
                }
                let p = g.awaiting()[0];
                let intent = crate::bot::choose(&g, p);
                g.apply(p, intent).unwrap();
            }
            g.log()
                .iter()
                .filter(|e| matches!(e, Event::BattleResolved { .. }))
                .count()
        })
        .sum();
    assert!(battles > 0, "bots never fought in 30 matches");
}

// ---- Gods as world state (§5) ----

/// Ends turns until the next dusk has passed.
fn to_next_dusk(g: &mut Game) {
    let dusks = |g: &Game| {
        g.log()
            .iter()
            .filter(|e| matches!(e, Event::Dusk { .. }))
            .count()
    };
    let before = dusks(g);
    while dusks(g) == before {
        g.end_turn_and_settle();
    }
}

#[test]
fn gods_start_light_or_mid() {
    for seed in 0..50 {
        let (g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        assert!(God::ALL.iter().all(|&god| g.stage(god) < 2), "seed {seed}");
    }
}

#[test]
fn offerings_feed_one_god_and_relieve_the_one_it_quenches() {
    let (mut g, me, _) = duel(3);
    let mut events = Vec::new();
    // Water quenches fire: serving Maya cools Trishna.
    g.offer(Some(me), God::Maya, 2, &mut events);
    assert_eq!(g.pressure(God::Maya), 2);
    assert_eq!(g.pressure(God::Trishna), -2);
    assert_eq!(g.favor(me, God::Maya), 2);
    // The world's own offerings build pressure but no one's favour.
    g.offer(None, God::Bhava, 1, &mut events);
    assert!(g.players().all(|p| g.favor(p, God::Bhava) == 0));
}

#[test]
fn left_alone_the_world_slides_towards_trishna() {
    let (mut g, _, _) = duel(3);
    g.pantheon.stages[God::Trishna.index()] = 0;
    for _ in 0..STAGE_THRESHOLD {
        to_next_dusk(&mut g);
    }
    assert_eq!(g.stage(God::Trishna), 1);
    assert_eq!(
        g.pressure(God::Trishna),
        0,
        "pressure starts over after a shift"
    );
}

#[test]
fn stages_move_both_ways_and_stay_in_range() {
    let (mut g, _, _) = duel(3);
    let z = God::Zaga.index();
    g.pantheon.stages[z] = 2;
    g.pantheon.pressure[z] = 9;
    to_next_dusk(&mut g);
    assert_eq!(g.stage(God::Zaga), 2, "already darkest");
    g.pantheon.pressure[z] = -STAGE_THRESHOLD;
    to_next_dusk(&mut g);
    assert_eq!(g.stage(God::Zaga), 1);
}

#[test]
fn a_gods_stage_bends_its_cards() {
    for (stage, expected) in [(0, 1), (1, 2), (2, 3)] {
        let (mut g, me, foe) = duel(2);
        g.champ_mut(foe).body = 9;
        g.champ_mut(foe).hp = 9;
        g.champ_mut(me).spirit_points = 3;
        g.pantheon.stages[God::Trishna.index()] = stage;
        let fire = g.give(me, "Пламя пира");
        g.apply(
            me,
            Intent::Play {
                card: fire,
                target: Target::Champion(foe),
            },
        )
        .unwrap();
        g.pass_all();
        assert_eq!(9 - g.champion(foe).unwrap().hp, expected, "stage {stage}");
    }
}

#[test]
fn played_cards_are_offerings_and_bodies_weigh_double() {
    let (mut g, me, _) = duel(3);
    let here = g.champion(me).unwrap().hex;
    g.board.tile_mut(here).unwrap().corpse = Some(Corpse { age: 0 });
    let legion = g.give(me, "Вписать в легион");
    g.apply(
        me,
        Intent::Play {
            card: legion,
            target: Target::Hex(here),
        },
    )
    .unwrap();
    assert_eq!(g.favor(me, God::Ahamar), 2);
    let short = g.give(me, "Короткий путь");
    g.apply(
        me,
        Intent::Play {
            card: short,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    let total: u16 = God::ALL.iter().map(|&god| g.favor(me, god)).sum();
    assert_eq!(total, 2, "neutral cards feed no god");
}

#[test]
fn ending_a_turn_on_a_temple_is_a_prayer() {
    let (mut g, me, _) = duel(3);
    let temple = g.board().temple_of(God::Maya);
    g.place(me, temple);
    g.apply(me, Intent::EndTurn).unwrap();
    assert_eq!(g.favor(me, God::Maya), 1);
}

#[test]
fn feast_reads_trishnas_stage() {
    // Generosity: everyone near heals.
    let (mut g, me, foe) = duel(1);
    g.pantheon.stages[God::Trishna.index()] = 0;
    g.champ_mut(me).hp = 1;
    g.champ_mut(foe).hp = 1;
    let feast = g.give(me, "Пир урожая");
    g.apply(
        me,
        Intent::Play {
            card: feast,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    assert!(g.champion(me).unwrap().hp > 1);
    assert!(g.champion(foe).unwrap().hp > 1);

    // Thirst: the host eats, the guest pays.
    let (mut g, me, foe) = duel(1);
    g.pantheon.stages[God::Trishna.index()] = 1;
    g.champ_mut(me).hp = 1;
    let foe_hp = g.champion(foe).unwrap().hp;
    let feast = g.give(me, "Пир урожая");
    g.apply(
        me,
        Intent::Play {
            card: feast,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    assert_eq!(g.champion(me).unwrap().hp, 3);
    assert_eq!(g.champion(foe).unwrap().hp, foe_hp - 1);

    // Devouring: bodies near burn into spirit, the host is scorched.
    let (mut g, me, _) = duel(3);
    g.pantheon.stages[God::Trishna.index()] = 2;
    g.champ_mut(me).spirit_points = 0;
    g.champ_mut(me).spirit = 4;
    for hex in [Hex::ZERO, Hex::new(0, 1)] {
        g.board.tile_mut(hex).unwrap().corpse = Some(Corpse { age: 0 });
    }
    let hp = g.champion(me).unwrap().hp;
    let feast = g.give(me, "Пир урожая");
    g.champ_mut(me).spirit_points = 1;
    g.apply(
        me,
        Intent::Play {
            card: feast,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    assert_eq!(g.champion(me).unwrap().spirit_points, 2);
    assert_eq!(g.champion(me).unwrap().hp, hp - 1);
    assert!(
        g.board()
            .corpses()
            .next()
            .is_none_or(|(h, _)| h.unsigned_distance_to(Hex::ZERO) > 1)
    );
}

#[test]
fn favor_vector_points_at_the_patron_served() {
    let (mut g, me, _) = duel(3);
    let mut events = Vec::new();
    assert_eq!(g.favor_vector(me), [0.0, 0.0]);
    g.offer(Some(me), God::Bhava, 4, &mut events);
    let [x, y] = g.favor_vector(me);
    assert!((x - 1.0).abs() < 1e-5 && y.abs() < 1e-5);
    for god in God::ALL {
        g.favor[me.0 as usize][god.index()] = 3;
    }
    let [x, y] = g.favor_vector(me);
    assert!(
        x.abs() < 1e-5 && y.abs() < 1e-5,
        "serving all equally is the centre"
    );
}

#[test]
fn stages_move_in_bot_games() {
    let mut changes = 0;
    let mut trishna_dark = 0;
    for seed in 0..20 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        for _ in 0..1500 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent).unwrap();
        }
        changes += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::StageChanged { .. }))
            .count();
        trishna_dark += usize::from(g.stage(God::Trishna) == 2);
    }
    assert!(changes > 0);
    // The default drift: most untended worlds end in Trishna's dark.
    assert!(
        trishna_dark >= 10,
        "only {trishna_dark} of 20 ended in Devouring"
    );
}

// ---- Style, the Crown and Threat (§6) ----

fn to_next_dawn(g: &mut Game) {
    let dawns = |g: &Game| {
        g.log()
            .iter()
            .filter(|e| matches!(e, Event::Dawn { .. }))
            .count()
    };
    let before = dawns(g);
    while dawns(g) == before {
        g.end_turn_and_settle();
    }
}

#[test]
fn entering_a_settlement_claims_it_and_it_pays_at_dawn() {
    let (mut g, me, foe) = duel(3);
    let spot = Hex::new(1, 0);
    g.board.tile_mut(spot).unwrap().terrain = Terrain::Settlement;
    let events = g.apply(me, Intent::Move { to: spot }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Claimed { player, from: None, .. } if *player == me))
    );
    assert_eq!(g.owner(spot), Some(me));
    // Being placed on the Table claims nothing: only entering does.
    g.pass_all();
    let expected = u16::from(g.taste().settlement);
    to_next_dawn(&mut g);
    assert!(g.style(me) >= expected);
    assert_eq!(g.style(foe), 0);
}

#[test]
fn the_crown_goes_to_the_leader_and_ties_keep_it() {
    let (mut g, me, foe) = duel(3);
    let mut ev = Vec::new();
    g.add_style(me, 3, StyleReason::Territory, &mut ev);
    g.add_style(foe, 1, StyleReason::Territory, &mut ev);
    g.dawn(&mut ev);
    assert_eq!(g.dominant(), Some(me));
    assert_eq!(g.threat(me), 1, "the Crown draws the guard's eye");

    g.add_style(foe, 2, StyleReason::Territory, &mut ev);
    g.dawn(&mut ev);
    assert_eq!(g.dominant(), Some(me), "a tie keeps the Crown where it was");

    let third = g.players().find(|&p| p != me && p != foe).unwrap();
    g.add_style(third, 3, StyleReason::Territory, &mut ev);
    g.add_style(me, -1, StyleReason::Oath, &mut ev);
    g.dawn(&mut ev);
    assert_eq!(
        g.dominant(),
        None,
        "two new leaders, the table is contested"
    );
}

#[test]
fn nobody_is_crowned_with_no_style() {
    let (g, _) = Game::new(five());
    assert_eq!(g.dominant(), None);
    assert!(
        g.log()
            .iter()
            .any(|e| matches!(e, Event::Crowned { player: None }))
    );
}

#[test]
fn battle_winner_takes_style_double_from_the_dominant() {
    for seed in 0..40 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        let me = g.current_player();
        let foe = g.order()[1];
        g.place(me, Hex::ZERO);
        g.place(foe, Hex::new(1, 0));
        let mut ev = Vec::new();
        g.add_style(foe, 5, StyleReason::Territory, &mut ev);
        g.dominant = Some(foe);
        start_battle(&mut g, me, foe);
        g.apply(me, Intent::Burn { cards: vec![] }).unwrap();
        let events = g.apply(foe, Intent::Burn { cards: vec![] }).unwrap();
        let (a, d) = events
            .iter()
            .find_map(|e| match e {
                Event::BattleResolved {
                    attacker_score,
                    defender_score,
                    ..
                } => Some((*attacker_score, *defender_score)),
                _ => None,
            })
            .unwrap();
        let (to_d, to_a) = (
            a.hits.saturating_sub(d.shields),
            d.hits.saturating_sub(a.shields),
        );
        let stake = i16::from(g.taste().battle) * 2;
        match to_d.cmp(&to_a) {
            std::cmp::Ordering::Greater => {
                assert_eq!(g.style(me), stake as u16, "seed {seed}");
                assert_eq!(g.style(foe), 5 - stake as u16, "seed {seed}");
            }
            std::cmp::Ordering::Less => {
                assert_eq!(
                    g.style(foe),
                    5 + i16::from(g.taste().battle) as u16,
                    "seed {seed}"
                );
            }
            std::cmp::Ordering::Equal => assert_eq!(g.style(foe), 5, "seed {seed}"),
        }
        assert_eq!(g.threat(me), 1, "attacking is loud");
    }
}

#[test]
fn manner_pays_and_a_broken_oath_costs_at_dusk() {
    let (mut g, me, _) = duel(3);
    let character = g.character(me).unwrap();
    let mut ev = Vec::new();
    g.record_deed(me, character.manner);
    g.judge_the_day(&mut ev);
    assert_eq!(g.style(me), u16::from(g.taste().roleplay));
    g.record_deed(me, character.oath);
    g.judge_the_day(&mut ev);
    assert_eq!(g.style(me), u16::from(g.taste().roleplay) - 1);
    g.judge_the_day(&mut ev);
    assert_eq!(
        g.style(me),
        u16::from(g.taste().roleplay) - 1,
        "deeds reset each day"
    );
}

#[test]
fn zaga_quiets_and_the_dead_used_are_noticed() {
    let (mut g, me, _) = duel(3);
    let here = g.champion(me).unwrap().hex;
    g.board.tile_mut(here).unwrap().corpse = Some(Corpse { age: 0 });
    let fuel = g.give(me, "Сжечь как топливо");
    g.apply(
        me,
        Intent::Play {
            card: fuel,
            target: Target::Hex(here),
        },
    )
    .unwrap();
    assert_eq!(g.threat(me), 1);
    let hair = g.give(me, "Власяница");
    g.apply(
        me,
        Intent::Play {
            card: hair,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    assert_eq!(g.threat(me), 0);
}

#[test]
fn the_guard_hunts_the_loudest_and_strikes() {
    let (mut g, _, foe) = duel(4);
    let mut ev = Vec::new();
    g.add_threat(foe, GUARD_THRESHOLD as i8 + 1, &mut ev);
    g.champ_mut(foe).body = 30;
    g.champ_mut(foe).hp = 30;
    // Put the foe far from the centre so the guard has to walk.
    g.place(foe, Hex::new(5, -5));
    let mut spawned = false;
    let mut struck = false;
    for _ in 0..12 {
        g.end_turn_and_settle();
        spawned |= g
            .log()
            .iter()
            .any(|e| matches!(e, Event::GuardSpawned { .. }));
        if g.log()
            .iter()
            .any(|e| matches!(e, Event::GuardStruck { target } if *target == foe))
        {
            struck = true;
            break;
        }
    }
    assert!(spawned && struck);
    assert!(
        g.threat(foe) < GUARD_THRESHOLD + 1,
        "the strike quiets them down"
    );
    let guard_throw = g.log().iter().find_map(|e| match e {
        Event::DiceThrown {
            fighter: Fighter::Guard,
            seed,
            count,
            faces,
        } => Some((*seed, *count, faces.clone())),
        _ => None,
    });
    let (seed, count, faces) = guard_throw.expect("the guard threw dice");
    assert_eq!(count, crate::game::GUARD_DICE);
    assert_eq!(necromy_dice::throw(seed, count).faces, faces);
}

#[test]
fn nobody_walks_through_the_guard() {
    let (mut g, me, _) = duel(4);
    g.guard = Some(Guard {
        hex: Hex::new(1, 0),
        target: me,
    });
    assert_eq!(
        g.apply(me, Intent::Move { to: Hex::new(1, 0) }),
        Err(RuleError::Occupied)
    );
    assert!(!g.reachable().contains_key(&Hex::new(1, 0)));
}

#[test]
fn crowns_and_guards_happen_in_bot_games() {
    let (mut crowned, mut guards) = (0, 0);
    for seed in 0..20 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        for _ in 0..1500 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent).unwrap();
        }
        crowned += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::Crowned { player: Some(_) }))
            .count();
        guards += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::GuardStruck { .. }))
            .count();
    }
    assert!(crowned > 0, "nobody ever wore the Crown");
    assert!(guards > 0, "the guard never struck");
}

// ---- Victory (§10) ----

#[test]
fn conditions_are_drawn_open_and_secret() {
    for seed in 0..100 {
        let (g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        let open = g.open_conditions();
        assert_eq!(open.len(), crate::game::OPEN_COUNT);
        for (i, a) in open.iter().enumerate() {
            for b in &open[i + 1..] {
                assert_ne!(
                    std::mem::discriminant(a),
                    std::mem::discriminant(b),
                    "seed {seed}"
                );
            }
        }
        for p in g.players() {
            let s = g.secret(p).unwrap();
            assert!(
                !open
                    .iter()
                    .any(|o| std::mem::discriminant(o) == std::mem::discriminant(&s)),
                "seed {seed}: secret {s:?} repeats an open kind"
            );
        }
    }
}

#[test]
fn meeting_a_condition_wins_and_stops_the_match() {
    let (mut g, me, foe) = duel(3);
    g.open = vec![Condition::Registry { regions: 2 }];
    g.secrets = vec![Some(Condition::Overthrow { wins: 99 }); 5];
    let settlements: Vec<Hex> = [God::Bhava, God::Maya]
        .iter()
        .map(|&god| {
            g.board()
                .tiles()
                .find(|(_, t)| t.region == Some(god) && t.terrain == Terrain::Settlement)
                .unwrap()
                .0
        })
        .collect();
    g.claims
        .insert((settlements[0].x(), settlements[0].y()), me);
    assert!(!g.meets(me, Condition::Registry { regions: 2 }));
    g.claims
        .insert((settlements[1].x(), settlements[1].y()), me);
    let events = g.apply(me, Intent::EndTurn).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Victory { player, .. } if *player == me))
    );
    assert_eq!(g.winner(), Some((me, Condition::Registry { regions: 2 })));
    assert_eq!(g.apply(foe, Intent::Pass), Err(RuleError::GameOver));
}

#[test]
fn beating_the_dominant_counts_towards_overthrow() {
    let (mut g, me, foe) = duel(3);
    g.dominant = Some(foe);
    let mut ev = Vec::new();
    g.battle_style(me, foe, &mut ev);
    g.battle_style(me, foe, &mut ev);
    let third = g.players().find(|&p| p != me && p != foe).unwrap();
    g.battle_style(me, third, &mut ev);
    assert_eq!(
        g.progress[me.0 as usize].overthrows, 2,
        "only wins over the Dominant count"
    );
    assert!(g.meets(me, Condition::Overthrow { wins: 2 }));
}

#[test]
fn the_wager_needs_the_crown_dawn_after_dawn() {
    let (mut g, me, foe) = duel(3);
    let mut ev = Vec::new();
    g.add_style(me, 5, StyleReason::Territory, &mut ev);
    for _ in 0..3 {
        g.dawn(&mut ev);
    }
    assert_eq!(g.progress[me.0 as usize].crown_streak, 3);
    g.add_style(foe, 9, StyleReason::Territory, &mut ev);
    g.dawn(&mut ev);
    assert_eq!(
        g.progress[me.0 as usize].crown_streak, 0,
        "losing the Crown breaks the streak"
    );
    assert_eq!(g.progress[foe.0 as usize].crown_streak, 1);
}

#[test]
fn god_limit_needs_fanaticism_and_a_dark_god() {
    let (mut g, me, _) = duel(3);
    g.favor[me.0 as usize] = [0, 9, 0, 0, 1];
    g.pantheon.stages[God::Trishna.index()] = 1;
    assert!(!g.meets(me, Condition::GodLimit), "Trishna is not dark yet");
    g.pantheon.stages[God::Trishna.index()] = 2;
    assert!(g.meets(me, Condition::GodLimit));
    g.favor[me.0 as usize] = [9, 9, 9, 9, 9];
    assert!(
        !g.meets(me, Condition::GodLimit),
        "serving everyone is not fanaticism"
    );
}

#[test]
fn fusion_needs_an_adjacent_pair_past_their_light() {
    let (mut g, me, _) = duel(3);
    // Wood (0) and fire (1) sit next to each other on the ring.
    g.favor[me.0 as usize] = [8, 9, 0, 0, 2];
    g.pantheon.stages = [1, 1, 1, 1, 1];
    assert!(g.meets(me, Condition::Fusion));
    g.pantheon.stages[God::Bhava.index()] = 0;
    assert!(!g.meets(me, Condition::Fusion), "a light god does not fuse");
    // Wood (0) and earth (2) are not neighbours.
    g.pantheon.stages = [1, 1, 1, 1, 1];
    g.favor[me.0 as usize] = [8, 0, 9, 0, 0];
    assert!(!g.meets(me, Condition::Fusion));
}

#[test]
fn first_at_table_waits_for_its_round() {
    let (mut g, me, _) = duel(3);
    let mut ev = Vec::new();
    g.add_style(me, 3, StyleReason::Territory, &mut ev);
    let c = Condition::FirstAtTable { round: 19 };
    assert!(!g.meets(me, c));
    g.round = 19;
    assert!(g.meets(me, c));
}

#[test]
fn bot_matches_end_in_many_ways() {
    let mut kinds = std::collections::HashSet::new();
    for seed in 0..60 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        for _ in 0..6000 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent).unwrap();
        }
        let (_, c) = g
            .winner()
            .unwrap_or_else(|| panic!("seed {seed}: nobody won"));
        kinds.insert(std::mem::discriminant(&c));
    }
    assert!(
        kinds.len() >= 4,
        "only {} kinds of victory in 60 matches",
        kinds.len()
    );
}

// ---- Wishes (§7) ----

use crate::game::WishKind;

/// Crowns `me` at a dawn and returns the events.
fn crown(g: &mut Game, me: PlayerId) -> Vec<Event> {
    let mut ev = Vec::new();
    g.add_style(me, 5, StyleReason::Territory, &mut ev);
    g.dawn(&mut ev);
    ev
}

#[test]
fn the_dominant_owes_a_wish_before_play_goes_on() {
    let (mut g, me, foe) = duel(3);
    let ev = crown(&mut g, me);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::WishDue { player } if *player == me))
    );
    assert_eq!(g.awaiting(), vec![me]);
    assert_eq!(g.apply(foe, Intent::Pass), Err(RuleError::WishPending));
    assert_eq!(g.apply(me, Intent::EndTurn), Err(RuleError::WishPending));
    g.apply(
        me,
        Intent::Wish {
            god: God::Bhava,
            kind: WishKind::Land,
            target: None,
            said: None,
        },
    )
    .unwrap();
    assert_eq!(g.wish_due(), None);
}

#[test]
fn gods_grade_by_nature_and_novelty() {
    let (mut g, me, foe) = duel(3);
    // Bhava loves land; first time asked: 1 + 1 + 1.
    assert_eq!(g.wish_grade(God::Bhava, WishKind::Land), 3);
    // Bhava dislikes harm: 1 − 1 + 1.
    assert_eq!(g.wish_grade(God::Bhava, WishKind::Weaken), 1);
    assert_eq!(g.wish_grade(God::Trishna, WishKind::Fortune), 0, "crude");
    crown(&mut g, me);
    g.apply(
        me,
        Intent::Wish {
            god: God::Bhava,
            kind: WishKind::Land,
            target: None,
            said: None,
        },
    )
    .unwrap();
    assert_eq!(
        g.wish_grade(God::Bhava, WishKind::Land),
        1,
        "the gods remember"
    );
    let _ = foe;
}

#[test]
fn a_wish_without_style_comes_with_a_curse() {
    let (mut g, me, _) = duel(3);
    crown(&mut g, me);
    let style = g.style(me);
    let events = g
        .apply(
            me,
            Intent::Wish {
                god: God::Trishna,
                kind: WishKind::Fortune,
                target: None,
                said: None,
            },
        )
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::WishGranted { grade: 0, .. }))
    );
    // Two Style of riches, one lost for the lack of style.
    assert_eq!(g.style(me), style + 1);
    assert_eq!(g.curses(me), &[God::Trishna]);
}

#[test]
fn curses_bite_until_the_quenching_element_lifts_them() {
    let (mut g, me, _) = duel(3);
    g.curses[me.0 as usize].push(God::Trishna);
    g.champ_mut(me).body = 9;
    g.champ_mut(me).hp = 9;
    while g.current_player() == me {
        g.end_turn_and_settle();
    }
    while g.current_player() != me {
        g.end_turn_and_settle();
    }
    assert_eq!(
        g.champion(me).unwrap().hp,
        8,
        "one bite at the start of the turn"
    );
    // Water quenches fire: a Maya card lifts Trishna's curse.
    let hand = g.give(me, "Бирюзовый оберег");
    g.champ_mut(me).spirit_points = 1;
    let events = g
        .apply(
            me,
            Intent::Play {
                card: hand,
                target: Target::Champion(me),
            },
        )
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::CurseLifted {
            god: God::Trishna,
            ..
        }
    )));
    assert!(g.curses(me).is_empty());
}

#[test]
fn weaken_needs_a_rival_and_passes_wards() {
    let (mut g, me, foe) = duel(3);
    crown(&mut g, me);
    assert_eq!(
        g.apply(
            me,
            Intent::Wish {
                god: God::Ahamar,
                kind: WishKind::Weaken,
                target: None,
                said: None,
            },
        ),
        Err(RuleError::InvalidWish)
    );
    assert_eq!(
        g.apply(
            me,
            Intent::Wish {
                god: God::Ahamar,
                kind: WishKind::Weaken,
                target: Some(me),
                said: None,
            },
        ),
        Err(RuleError::InvalidWish)
    );
    g.champ_mut(foe).ward = Some(Element::Water);
    let hp = g.champion(foe).unwrap().hp;
    g.apply(
        me,
        Intent::Wish {
            god: God::Ahamar,
            kind: WishKind::Weaken,
            target: Some(foe),
            said: None,
        },
    )
    .unwrap();
    assert!(
        g.champion(foe).unwrap().hp < hp
            || g.champion(foe).unwrap().hp == g.champion(foe).unwrap().body
    );
    assert!(g.champion(foe).unwrap().rooted);
}

#[test]
fn every_god_twists_the_wish() {
    // Ahamar writes a debt: Threat.
    let (mut g, me, _) = duel(3);
    crown(&mut g, me);
    let threat = g.threat(me);
    g.apply(
        me,
        Intent::Wish {
            god: God::Ahamar,
            kind: WishKind::Land,
            target: None,
            said: None,
        },
    )
    .unwrap();
    assert_eq!(g.threat(me), threat + 1);

    // Maya takes a card from the hand.
    let (mut g, me, _) = duel(3);
    g.give(me, "Бинт");
    crown(&mut g, me);
    let events = g
        .apply(
            me,
            Intent::Wish {
                god: God::Maya,
                kind: WishKind::Peace,
                target: None,
                said: None,
            },
        )
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::CardDissolved { .. }))
    );
    assert!(g.hand(me).is_empty());
}

#[test]
fn the_wager_is_won_by_refusing_the_crowned_wish() {
    let (mut g, me, _) = duel(3);
    g.open = vec![];
    g.secrets = vec![Some(Condition::Wager { dawns: 2 }); 5];
    crown(&mut g, me);
    g.apply(me, Intent::RefuseWish).unwrap();
    assert_eq!(g.winner(), None, "one dawn is not the bet");
    let mut ev = Vec::new();
    g.dawn(&mut ev);
    let events = g.apply(me, Intent::RefuseWish).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Victory { player, .. } if *player == me))
    );
}

#[test]
fn bots_wish_in_many_ways() {
    let mut kinds = std::collections::HashSet::new();
    let mut gods = std::collections::HashSet::new();
    for seed in 0..20 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        for _ in 0..3000 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent).unwrap();
        }
        for e in g.log() {
            if let Event::WishGranted { god, kind, .. } = e {
                kinds.insert(*kind);
                gods.insert(*god);
            }
        }
    }
    assert!(kinds.len() >= 4, "bots asked only {kinds:?}");
    assert!(gods.len() >= 4, "bots asked only {gods:?}");
}

// ---- The storyteller (§8) ----

use crate::game::{Goal, LineKind, MAX_OPEN};

fn line(g: &mut Game, owner: PlayerId, kind: LineKind, goal: Goal, stake: u8) -> u32 {
    g.next_line += 1;
    let id = g.next_line;
    g.lines.push(crate::game::Line {
        id,
        owner,
        god: God::Ahamar,
        kind,
        goal,
        deadline: g.round + 4,
        style: 2,
        stake,
    });
    id
}

#[test]
fn dusk_tells_lines_to_those_lagging_within_limits() {
    let (mut g, _) = Game::new(five());
    // Nobody lags at the start; lines come once the table spreads out.
    for _ in 0..600 {
        if g.winner().is_some() {
            break;
        }
        let p = g.awaiting()[0];
        let intent = crate::bot::choose(&g, p);
        g.apply(p, intent).unwrap();
    }
    assert!(g.log().iter().any(|e| matches!(e, Event::LineTold { .. })));
    for p in g.players() {
        assert!(g.lines_of(p).count() <= MAX_OPEN);
    }
}

#[test]
fn a_pilgrimage_ends_at_the_temple() {
    let (mut g, me, _) = duel(3);
    let temple = g.board().temple_of(God::Maya);
    line(&mut g, me, LineKind::Pilgrimage, Goal::ReachHex(temple), 0);
    let style = g.style(me);
    g.place(me, temple);
    let events = g.apply(me, Intent::EndTurn).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::LineDone { .. })));
    assert_eq!(g.style(me), style + 2);
    assert_eq!(g.lines_of(me).count(), 0);
}

#[test]
fn spoils_are_won_in_battle() {
    let (mut g, me, foe) = duel(3);
    line(&mut g, me, LineKind::Spoils, Goal::WinBattle, 0);
    let mut ev = Vec::new();
    g.battle_style(me, foe, &mut ev);
    g.settle_story(&mut ev);
    assert!(ev.iter().any(|e| matches!(e, Event::LineDone { .. })));
}

#[test]
fn a_quiet_crown_breaks_on_a_fight_and_holds_to_its_deadline() {
    let (mut g, me, foe) = duel(1);
    line(&mut g, me, LineKind::QuietCrown, Goal::AvoidBattle, 2);
    let mut ev = Vec::new();
    g.add_style(me, 5, StyleReason::Territory, &mut ev);
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::LineFailed { .. })));
    assert_eq!(g.style(me), 3, "the stake is lost");
    let _ = foe;

    let (mut g, me, _) = duel(4);
    line(&mut g, me, LineKind::QuietCrown, Goal::AvoidBattle, 2);
    g.lines[0].deadline = g.round;
    let mut ev = Vec::new();
    g.storyteller(&mut ev);
    assert!(
        ev.iter().any(|e| matches!(e, Event::LineDone { .. })),
        "kept quiet to the end"
    );
}

#[test]
fn a_quiet_board_stirs() {
    let (mut g, _, _) = duel(4);
    g.round = 10;
    g.last_fight = 0;
    let mut ev = Vec::new();
    g.storyteller(&mut ev);
    assert!(ev.iter().any(|e| matches!(e, Event::WorldStirred { .. })));
    assert_eq!(g.last_fight, 10, "one stir, then the calm counts again");
}

#[test]
fn bots_live_their_lines() {
    let (mut done, mut told) = (0, 0);
    for seed in 0..20 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
        });
        for _ in 0..3000 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent).unwrap();
        }
        told += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::LineTold { .. }))
            .count();
        done += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::LineDone { .. }))
            .count();
    }
    assert!(told > 20, "only {told} lines told");
    assert!(done * 5 >= told, "only {done} of {told} lines done");
}
