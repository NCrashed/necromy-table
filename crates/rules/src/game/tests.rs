use super::*;
use crate::board::GROVE_AGE;
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
        loop {
            let Some(p) = self.players().find(|&p| self.to_answer(p).is_some()) else {
                break;
            };
            self.apply(p, Intent::Pass).unwrap();
        }
    }

    fn end_turn_and_settle(&mut self) {
        self.apply(self.current_player(), Intent::EndTurn).unwrap();
        self.pass_all();
    }

    /// The first player still acting, in initiative order: the one a test
    /// drives. Everyone acts at once now (§11.2).
    fn current_player(&self) -> PlayerId {
        self.acting().first().copied().unwrap_or(self.order[0])
    }

    /// The oldest open window.
    fn window(&self) -> Option<&Window> {
        self.windows.first()
    }

    fn mp(&self) -> u32 {
        self.move_points(self.current_player())
    }

    fn reach(&self) -> HashMap<Hex, u32> {
        self.reachable(self.current_player())
    }

    fn attackable_now(&self) -> Vec<Hex> {
        self.attackable(self.current_player())
    }

    /// `player` has taken their turn already this round.
    fn finish(&mut self, player: PlayerId) {
        self.turns[player.0 as usize].phase = Phase::Done;
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
    // Only `me` is still taking their turn: the others went already, so the
    // duel plays out in order as it would between neighbours (§11.2).
    for p in g.players().collect::<Vec<_>>() {
        if p != me {
            g.finish(p);
        }
    }
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
fn everyone_takes_their_turn_at_once() {
    let (mut game, _) = Game::new(five());
    assert_eq!(game.acting().len(), 5);
    assert_eq!(game.awaiting().len(), 5);
    let p = game.order()[2];
    game.apply(p, Intent::EndTurn).unwrap();
    assert_eq!(game.phase(p), &Phase::Done);
    // Done for the round: nothing more until the next one.
    assert_eq!(game.apply(p, Intent::EndTurn), Err(RuleError::NotYourTurn));
    assert_eq!(game.round(), 1);
}

/// `duel`, but the foe has not taken their turn yet either.
fn duel_both_acting(distance: i32) -> (Game, PlayerId, PlayerId) {
    let (mut g, me, foe) = duel(distance);
    g.turns[foe.0 as usize].phase = Phase::Acting;
    (g, me, foe)
}

#[test]
fn a_step_near_an_acting_rival_waits_for_their_turn_to_end() {
    let (mut g, me, foe) = duel_both_acting(3);
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(events.contains(&Event::Held {
        player: me,
        on: Some(foe)
    }));
    assert_eq!(g.champion(me).unwrap().hex, Hex::ZERO, "the step waits");
    assert!(!g.awaiting().contains(&me));
    // Someone far away is not in the way: the foe goes on as they like.
    let events = g.apply(foe, Intent::EndTurn).unwrap();
    assert!(events.contains(&Event::Resumed { player: me }));
    assert_eq!(g.champion(me).unwrap().hex, Hex::new(1, 0));
    // Played now, against a rival who is done: they may react.
    assert!(g.to_answer(foe).is_some());
}

#[test]
fn a_held_card_is_dropped_when_its_target_walked_away() {
    let (mut g, me, foe) = duel_both_acting(2);
    let spark = g.give(me, "Искра");
    g.apply(
        me,
        Intent::Play {
            card: spark,
            target: Target::Champion(foe),
        },
    )
    .unwrap();
    assert!(matches!(g.phase(me), Phase::Held { .. }));
    assert!(g.hand(me).contains(&spark), "nothing is spent while held");
    g.apply(foe, Intent::Move { to: Hex::new(3, 0) }).unwrap();
    g.pass_all();
    g.apply(foe, Intent::Move { to: Hex::new(4, 0) }).unwrap();
    g.pass_all();
    let events = g.apply(foe, Intent::EndTurn).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::HoldDropped {
            player,
            why: RuleError::InvalidTarget
        } if *player == me
    )));
    assert_eq!(g.phase(me), &Phase::Acting, "the turn goes on");
    assert!(g.hand(me).contains(&spark));
}

#[test]
fn a_rival_who_is_done_is_no_obstacle() {
    let (mut g, me, foe) = duel(3);
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Held { .. })));
    assert!(g.to_answer(foe).is_some(), "an Enter window for the foe");
}

#[test]
fn the_round_ends_when_everyone_is_done() {
    let (mut game, _) = Game::new(five());
    for p in game.order().to_vec() {
        assert_eq!(game.round(), 1);
        game.apply(p, Intent::EndTurn).unwrap();
        game.pass_all();
    }
    assert_eq!(game.round(), 2);
    assert_eq!(game.acting().len(), 5);
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
        .reach()
        .iter()
        .find(|(h, _)| h.unsigned_distance_to(at) == 1)
        .unwrap();
    game.apply(p, Intent::Move { to }).unwrap();
    game.pass_all();
    assert_eq!(game.mp(), MOVE_POINTS - cost);
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
    // Out of the way: a rival still acting nearby would hold the foe's steps.
    if g.phase(me) == &Phase::Acting {
        g.apply(me, Intent::EndTurn).unwrap();
        g.pass_all();
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
fn root_on_a_champion_who_is_done_costs_the_next_turn() {
    let (mut g, me, foe) = duel_both_acting(2);
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
    g.apply(foe, Intent::EndTurn).unwrap();
    g.pass_all();
    assert_eq!(g.round(), 2, "everyone else was done already");
    assert_eq!(g.move_points(me), 0);
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
    assert_eq!(g.mp(), MOVE_POINTS + 1);
}

#[test]
fn bots_never_stall_or_break_rules() {
    let mut poisoned = 0;
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
        poisoned += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::Poisoned { .. }))
            .count();
    }
    assert!(poisoned > 0, "no bot ever poisoned anyone");
}

// ---- Battles (§12) ----

fn start_battle(g: &mut Game, me: PlayerId, foe: PlayerId) -> Vec<Event> {
    let at = g.champion(foe).unwrap().hex;
    g.apply(me, Intent::Move { to: at }).unwrap()
}

#[test]
fn stepping_onto_a_rival_opens_a_battle() {
    let (mut g, me, foe) = duel(1);
    assert_eq!(g.attackable_now(), vec![Hex::new(1, 0)]);
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
    assert_eq!(g.apply(third, Intent::Pass), Err(RuleError::NoWindow));
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
        // The foe has taken their turn: the attack goes ahead at once.
        g.finish(foe);
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
        assert_eq!(g.mp(), 0, "a battle ends the movement");

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
    g.turns[me.0 as usize].move_points = MOVE_POINTS;
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
        // The foe has taken their turn: the attack goes ahead at once.
        g.finish(foe);
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
    assert!(!g.reach().contains_key(&Hex::new(1, 0)));
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

use crate::game::{Act, Price, Wish, WishKind};

/// A prepared wish: one act of `kind`, no price. With no rival for a kind
/// that needs one, no act at all (the rules turn that away).
fn wish_of(kind: WishKind, target: Option<PlayerId>) -> Wish {
    Wish {
        acts: Act::of(kind, target).into_iter().collect(),
        price: None,
    }
}

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
            wish: wish_of(WishKind::Land, None),
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
            wish: wish_of(WishKind::Land, None),
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
                wish: wish_of(WishKind::Fortune, None),
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
                wish: wish_of(WishKind::Weaken, None),
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
                wish: wish_of(WishKind::Weaken, Some(me)),
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
            wish: wish_of(WishKind::Weaken, Some(foe)),
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
            wish: wish_of(WishKind::Land, None),
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
                wish: wish_of(WishKind::Peace, None),
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
    g.secrets = vec![Some(Condition::Wager { refusals: 2 }); 5];
    g.round = crate::game::SECRET_FROM_ROUND;
    crown(&mut g, me);
    g.apply(me, Intent::RefuseWish).unwrap();
    assert_eq!(g.winner(), None, "one refusal is not the bet");
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
fn a_wish_made_starts_the_wager_over() {
    let (mut g, me, _) = duel(3);
    g.open = vec![];
    g.secrets = vec![Some(Condition::Wager { refusals: 2 }); 5];
    g.round = crate::game::SECRET_FROM_ROUND;
    crown(&mut g, me);
    g.apply(me, Intent::RefuseWish).unwrap();
    let mut ev = Vec::new();
    g.dawn(&mut ev);
    let intent = crate::bot::choose(&g, me);
    assert_eq!(
        intent,
        Intent::RefuseWish,
        "a bot holding the Wager refuses"
    );
    g.secrets = vec![None; 5];
    let intent = crate::bot::choose(&g, me);
    g.apply(me, intent).unwrap();
    assert_eq!(g.progress[me.0 as usize].refusals, 0);
    g.secrets = vec![Some(Condition::Wager { refusals: 2 }); 5];
    g.dawn(&mut ev);
    g.apply(me, Intent::RefuseWish).unwrap();
    assert_eq!(g.winner(), None, "the wish in between broke the streak");
}

#[test]
fn a_refusal_is_loud_and_a_fall_breaks_the_wager() {
    let (mut g, me, _) = duel(3);
    crown(&mut g, me);
    let before = g.threat(me);
    g.apply(me, Intent::RefuseWish).unwrap();
    assert_eq!(g.threat(me), before + crate::game::REFUSAL_THREAT as u8);
    assert_eq!(g.progress[me.0 as usize].refusals, 1);
    let mut ev = Vec::new();
    g.fall(me, &mut ev);
    assert_eq!(g.progress[me.0 as usize].refusals, 0);
}

#[test]
fn a_secret_is_a_fallback_that_waits_for_its_round() {
    let (mut g, me, foe) = duel(3);
    g.open = vec![];
    g.secrets = vec![Some(Condition::Overthrow { wins: 1 }); 5];
    g.dominant = Some(foe);
    let mut ev = Vec::new();
    g.battle_style(me, foe, &mut ev);
    g.round = crate::game::SECRET_FROM_ROUND - 1;
    g.check_victory(&mut ev);
    assert_eq!(g.winner(), None, "too early for a secret");
    g.round = crate::game::SECRET_FROM_ROUND;
    g.check_victory(&mut ev);
    assert_eq!(g.winner(), Some((me, Condition::Overthrow { wins: 1 })));
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
            if let Event::WishGranted { god, wish, .. } = e {
                kinds.extend(wish.acts.iter().map(|a| a.kind()));
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

// ---- Stealth (§11.6) ----

/// `duel`, with `me` on a forest hex at night, well away from `foe`.
fn in_the_woods() -> (Game, PlayerId, PlayerId) {
    let (mut g, me, foe) = duel(3);
    let woods = Hex::new(0, 2);
    g.board.tile_mut(woods).unwrap().terrain = Terrain::Forest;
    g.place(me, woods);
    g.time = TimeOfDay::Night;
    (g, me, foe)
}

#[test]
fn night_in_the_woods_hides_and_dawn_in_the_open_reveals() {
    let (mut g, me, _) = in_the_woods();
    let events = g.apply(me, Intent::EndTurn).unwrap();
    assert!(g.is_hidden(me));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Hid { player, .. } if *player == me))
    );

    // Dawn finds them out of cover.
    g.board.tile_mut(g.hex_of(me)).unwrap().terrain = Terrain::Plains;
    let mut events = Vec::new();
    g.stealth_at_dawn(&mut events);
    assert!(!g.is_hidden(me));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Revealed {
            why: RevealReason::Dawn,
            ..
        }
    )));
}

#[test]
fn no_cover_by_day_or_next_to_a_rival() {
    let (mut g, me, _) = in_the_woods();
    g.time = TimeOfDay::Day;
    // Not under Bhava's Thicket, which keeps the woods dark by day.
    g.pantheon.stages[God::Bhava.index()] = 0;
    g.apply(me, Intent::EndTurn).unwrap();
    assert!(!g.is_hidden(me));

    let (mut g, me, foe) = in_the_woods();
    g.place(foe, Hex::new(1, 2));
    g.apply(me, Intent::EndTurn).unwrap();
    assert!(!g.is_hidden(me), "a rival next door sees them");
}

#[test]
fn the_hidden_cannot_be_aimed_at_or_attacked() {
    let (mut g, me, foe) = duel(1);
    let mut events = Vec::new();
    g.hide(foe, &mut events);
    assert!(g.attackable_now().is_empty());
    let spark = g.give(me, "Искра");
    assert!(g.targets(me, spark).is_empty());
    assert!(g.occupant(g.hex_of(foe)).is_none());
}

#[test]
fn walking_into_the_hidden_is_an_ambush() {
    let (mut g, me, foe) = duel(1);
    let mut events = Vec::new();
    g.hide(foe, &mut events);
    let might = g.champion(foe).unwrap().might;
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Stumbled { mover, hidden, .. }
        if *mover == me && *hidden == foe))
    );
    assert!(!g.is_hidden(foe));
    assert_eq!(g.hex_of(me), Hex::ZERO, "the step does not happen");
    assert_eq!(g.mp(), 0, "the walk is over");
    assert!(matches!(
        g.window().map(|w| w.kind),
        Some(WindowKind::Battle { attacker, defender }) if attacker == foe && defender == me
    ));
    assert_eq!(
        g.battle_dice(foe),
        Some(might + 1),
        "one more die from the shadow"
    );
    g.pass_all();
    assert_eq!(
        g.dice_for(foe, false),
        might,
        "the ambush die is for that one battle"
    );
}

#[test]
fn striking_from_the_shadow_reveals_with_an_extra_die() {
    let (mut g, me, _) = duel(1);
    let mut events = Vec::new();
    g.hide(me, &mut events);
    let might = g.champion(me).unwrap().might;
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(events.iter().any(
        |e| matches!(e, Event::Revealed { player, why: RevealReason::Attacked, .. } if *player == me)
    ));
    assert_eq!(g.battle_dice(me), Some(might + 1));
}

#[test]
fn aiming_at_a_rival_reveals() {
    let (mut g, me, foe) = duel(2);
    let mut events = Vec::new();
    g.hide(me, &mut events);
    let flame = g.give(me, "Пламя пира");
    g.champ_mut(me).spirit_points = 3;
    let events = g
        .apply(
            me,
            Intent::Play {
                card: flame,
                target: Target::Champion(foe),
            },
        )
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Revealed {
            why: RevealReason::Aimed,
            ..
        }
    )));
    assert!(!g.is_hidden(me));
}

#[test]
fn walking_into_a_crowd_reveals() {
    let (mut g, me, _) = duel(3);
    let mut events = Vec::new();
    g.hide(me, &mut events);
    let town = Hex::new(0, 1);
    g.board.tile_mut(town).unwrap().terrain = Terrain::Settlement;
    let events = g.apply(me, Intent::Move { to: town }).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Revealed {
            why: RevealReason::Crowd,
            ..
        }
    )));
    assert!(!g.is_hidden(me));
}

#[test]
fn a_hidden_walk_opens_no_window_and_reaches_no_rival() {
    let (mut g, me, foe) = duel(3);
    let mut events = Vec::new();
    g.hide(me, &mut events);
    let seen_at = g.hex_of(me);
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(g.window().is_none(), "nobody sees the step, nobody reacts");
    let view = g.view_for(Some(foe), 5);
    assert_eq!(
        view.champion(me).unwrap().hex,
        seen_at,
        "the rival sees the old spot"
    );
    assert!(
        events
            .iter()
            .filter_map(|e| Game::event_for(&view, Some(foe), e))
            .all(|e| !matches!(e, Event::Moved { .. }))
    );
    // The hidden one sees themselves where they are.
    let own = g.view_for(Some(me), 5);
    assert_eq!(own.champion(me).unwrap().hex, Hex::new(1, 0));
}

#[test]
fn the_veil_hides_anywhere() {
    let (mut g, me, _) = duel(3);
    let veil = g.give(me, "Пелена");
    g.champ_mut(me).spirit_points = 3;
    g.apply(
        me,
        Intent::Play {
            card: veil,
            target: Target::Champion(me),
        },
    )
    .unwrap();
    g.pass_all();
    assert!(g.is_hidden(me));
}

#[test]
fn a_hidden_rival_holds_nobody_up() {
    let (mut g, me, foe) = duel_both_acting(3);
    g.champ_mut(foe).hidden = true;
    // Waiting here would tell `me` someone lies close by (§11.6).
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Held { .. })));
    assert_eq!(g.champion(me).unwrap().hex, Hex::new(1, 0));
}

// ---- Laws of the world and patronage (§5.3, §5.4) ----

#[test]
fn thicket_hides_in_the_woods_by_day() {
    let (mut g, me, _) = in_the_woods();
    g.time = TimeOfDay::Day;
    g.pantheon.stages[God::Bhava.index()] = 1;
    g.apply(me, Intent::EndTurn).unwrap();
    assert!(g.is_hidden(me));
}

#[test]
fn exposure_forbids_hiding_by_day_except_to_ahamars_chosen() {
    let (mut g, me, _) = in_the_woods();
    g.time = TimeOfDay::Day;
    g.pantheon.stages[God::Bhava.index()] = 1;
    g.pantheon.stages[God::Ahamar.index()] = 2;
    g.apply(me, Intent::EndTurn).unwrap();
    assert!(!g.is_hidden(me));

    let (mut g, me, _) = in_the_woods();
    g.time = TimeOfDay::Day;
    g.pantheon.stages[God::Bhava.index()] = 1;
    g.pantheon.stages[God::Ahamar.index()] = 2;
    g.favor[me.0 as usize][God::Ahamar.index()] = CHOSEN;
    g.apply(me, Intent::EndTurn).unwrap();
    assert!(g.is_hidden(me));
}

#[test]
fn stillness_quiets_everyone_at_dusk() {
    let (mut g, me, foe) = duel(3);
    g.pantheon.stages[God::Zaga.index()] = 0;
    let mut ev = Vec::new();
    g.add_threat(me, 2, &mut ev);
    g.add_threat(foe, 1, &mut ev);
    g.dusk(&mut ev);
    assert_eq!((g.threat(me), g.threat(foe)), (1, 0));
}

#[test]
fn burden_makes_the_third_card_loud() {
    let (mut g, me, _) = duel(3);
    g.pantheon.stages[God::Zaga.index()] = 1;
    let before = g.threat(me);
    for _ in 0..3 {
        let bandage = g.give(me, "Бинт");
        g.apply(
            me,
            Intent::Play {
                card: bandage,
                target: Target::Champion(me),
            },
        )
        .unwrap();
    }
    assert_eq!(g.threat(me), before + 1);
}

#[test]
fn a_gods_sign_takes_a_spirit_off_its_cards() {
    let (mut g, me, _) = duel(3);
    let resin = g.give(me, "Живица");
    let full = g.def(resin).cost;
    assert!(full > 0);
    g.favor[me.0 as usize][God::Bhava.index()] = SIGN;
    assert_eq!(g.cost_of(me, resin), full - 1);
}

#[test]
fn devouring_starves_the_settled_but_not_trishnas_chosen() {
    let (mut g, me, foe) = duel(3);
    g.pantheon.stages[God::Trishna.index()] = 2;
    g.board.tile_mut(Hex::new(0, 0)).unwrap().terrain = Terrain::Settlement;
    g.board.tile_mut(Hex::new(3, 0)).unwrap().terrain = Terrain::Settlement;
    g.favor[foe.0 as usize][God::Trishna.index()] = CHOSEN;
    let (hp_me, hp_foe) = (g.champion(me).unwrap().hp, g.champion(foe).unwrap().hp);
    let mut ev = Vec::new();
    g.dusk_laws(&mut ev);
    assert_eq!(g.champion(me).unwrap().hp, hp_me - 1);
    assert_eq!(g.champion(foe).unwrap().hp, hp_foe);
}

#[test]
fn wrath_shortens_the_reach_of_reactions() {
    let (mut g, me, foe) = duel(4);
    g.pantheon.stages[God::Maya.index()] = 2;
    // Two hexes away is no longer close enough to react.
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    g.apply(me, Intent::Move { to: Hex::new(2, 0) }).unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::WindowOpened { .. }))
    );
    assert!(g.to_answer(foe).is_none());
}

// ---- Poison (§20.1) ----

fn play_at(g: &mut Game, me: PlayerId, name: &str, target: PlayerId) {
    let card = g.give(me, name);
    g.apply(
        me,
        Intent::Play {
            card,
            target: Target::Champion(target),
        },
    )
    .unwrap();
    g.pass_all();
}

#[test]
fn poison_bites_down_to_one_and_wears_off() {
    let (mut g, me, foe) = duel(2);
    play_at(&mut g, me, "Болиголов", foe);
    let poison = g.champion(foe).unwrap().poison;
    assert_eq!(
        poison,
        Some(Poison {
            element: Element::Wood,
            stacks: 2
        })
    );
    g.champ_mut(foe).hp = 2;
    let mut events = Vec::new();
    g.start_turn(foe, &mut events);
    assert_eq!(g.champion(foe).unwrap().hp, 1);
    // The last health is never poison's to take.
    g.start_turn(foe, &mut events);
    assert_eq!(g.champion(foe).unwrap().hp, 1);
    assert_eq!(g.champion(foe).unwrap().poison, None);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::PoisonBit {
            amount: 0,
            hp: 1,
            stacks: 0,
            ..
        }
    )));
}

#[test]
fn new_poison_adds_up_and_takes_its_element() {
    let (mut g, me, foe) = duel(2);
    play_at(&mut g, me, "Болиголов", foe);
    play_at(&mut g, me, "Чумной вздох", foe);
    let poison = g.champion(foe).unwrap().poison.unwrap();
    assert_eq!(poison.element, Element::Earth);
    assert_eq!(poison.stacks, 2 + 3);
}

#[test]
fn quenching_heal_cures_generating_heal_feeds() {
    let (mut g, me, foe) = duel(1);
    let wood = Some(Poison {
        element: Element::Wood,
        stacks: 2,
    });
    // Metal quenches wood: the heal takes the poison off.
    g.champ_mut(foe).poison = wood;
    play_at(&mut g, me, "Калёное железо", foe);
    assert_eq!(g.champion(foe).unwrap().poison, None);

    // Water generates wood: healing with it feeds the poison.
    g.champ_mut(me).poison = wood;
    g.champ_mut(me).hp = 1;
    play_at(&mut g, me, "Дымная ладонь", foe);
    assert_eq!(g.champion(me).unwrap().poison.unwrap().stacks, 3);
    // Metal then water is a generation chain: the drain heals 2.
    assert_eq!(g.champion(me).unwrap().hp, 3);
}

#[test]
fn a_ward_stops_poison_unless_it_is_quenched() {
    let (mut g, me, foe) = duel(2);
    g.champ_mut(foe).ward = Some(Element::Water);
    play_at(&mut g, me, "Болиголов", foe);
    assert_eq!(g.champion(foe).unwrap().poison, None);
    // Wood quenches earth: the ward breaks and the poison gets in.
    g.champ_mut(foe).ward = Some(Element::Earth);
    play_at(&mut g, me, "Болиголов", foe);
    assert_eq!(g.champion(foe).unwrap().ward, None);
    assert!(g.champion(foe).unwrap().poison.is_some());
}

#[test]
fn a_temple_and_death_cleanse() {
    let (mut g, me, foe) = duel(2);
    let poison = Some(Poison {
        element: Element::Earth,
        stacks: 3,
    });
    g.champ_mut(me).poison = poison;
    let tile = g.board.tile_mut(Hex::new(0, 0)).unwrap();
    tile.terrain = Terrain::Temple;
    tile.region = Some(God::Zaga);
    g.apply(me, Intent::EndTurn).unwrap();
    assert_eq!(g.champion(me).unwrap().poison, None);

    g.champ_mut(foe).poison = poison;
    let mut events = Vec::new();
    g.fall(foe, &mut events);
    assert_eq!(g.champion(foe).unwrap().poison, None);
}

#[test]
fn poison_traps_poison() {
    let (mut g, me, foe) = duel(2);
    let card = g.give(foe, "Мёртвая вода");
    g.traps.push(Trap {
        owner: foe,
        hex: Hex::new(1, 0),
        card,
    });
    g.hands[foe.0 as usize].clear();
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    g.pass_all();
    let poison = g.champion(me).unwrap().poison.unwrap();
    assert_eq!((poison.element, poison.stacks), (Element::Water, 2));
}

#[test]
fn a_price_is_paid_first_and_buys_budget_and_strength() {
    let (mut g, me, foe) = duel(3);
    let card = g.give(me, "Бинт");
    crown(&mut g, me);
    // Zaga (earth) likes a stake: its grade rises with a price.
    let plain = Wish::one(Act::Peace);
    let priced = Wish {
        acts: vec![Act::Peace, Act::Weaken { target: foe }],
        price: Some(Price::Card(card)),
    };
    assert_eq!(
        g.wish_grade_of(God::Zaga, &priced),
        (g.wish_grade_of(God::Zaga, &plain) + 1).min(3)
    );
    let events = g
        .apply(
            me,
            Intent::Wish {
                god: God::Zaga,
                wish: priced,
                said: None,
            },
        )
        .unwrap();
    let paid = events
        .iter()
        .position(|e| matches!(e, Event::PricePaid { .. }))
        .expect("the price is paid");
    let granted = events
        .iter()
        .position(|e| matches!(e, Event::WishGranted { .. }))
        .unwrap();
    assert!(paid < granted, "the price goes before the answer");
    assert!(!g.hand(me).contains(&card));
    // Budget covered both acts: the rival was hit and held.
    assert!(events.iter().any(|e| matches!(
        e,
        Event::WishGranted { wish, dropped: 0, .. } if wish.acts.len() == 2
    )));
    assert!(g.champion(foe).unwrap().rooted);
}

#[test]
fn a_wish_beyond_its_budget_is_cut_and_a_price_must_be_payable() {
    let (mut g, me, foe) = duel(3);
    crown(&mut g, me);
    // Health that would kill, Style one has not, a card not in hand.
    let hp = g.champion(me).unwrap().hp;
    for price in [
        Price::Health(hp.min(2)),
        Price::Style(2),
        Price::Card(crate::cards::CardId(9999)),
    ] {
        g.style[me.0 as usize] = 0;
        g.champ_mut(me).hp = 2;
        let wish = Wish {
            acts: vec![Act::Peace],
            price: Some(price),
        };
        assert_eq!(
            g.apply(
                me,
                Intent::Wish {
                    god: God::Zaga,
                    wish,
                    said: None,
                },
            ),
            Err(RuleError::InvalidWish),
            "{price:?}"
        );
    }
    g.champ_mut(me).hp = hp;
    // Three acts are too many; two at grade 0 are cut to one.
    let too_many = Wish {
        acts: vec![Act::Peace, Act::Land, Act::Dead],
        price: None,
    };
    assert_eq!(
        g.apply(
            me,
            Intent::Wish {
                god: God::Zaga,
                wish: too_many,
                said: None,
            },
        ),
        Err(RuleError::InvalidWish)
    );
    let crude_pair = Wish {
        acts: vec![Act::Fortune, Act::Weaken { target: foe }],
        price: None,
    };
    let events = g
        .apply(
            me,
            Intent::Wish {
                god: God::Zaga,
                wish: crude_pair,
                said: None,
            },
        )
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::WishGranted { grade: 0, dropped: 1, wish, .. } if wish.acts == [Act::Fortune]
    )));
}

/// How far a granted wish bends a card: 1, or 2 at full strength (grade 3).
fn boost_of(events: &[Event]) -> u8 {
    let grade = events
        .iter()
        .find_map(|e| match e {
            Event::WishGranted { grade, .. } => Some(*grade),
            _ => None,
        })
        .expect("a wish was granted");
    1 + u8::from(grade + 1 >= 4)
}

/// Crowns `me` and grants `act` from `god` at once, with no price.
fn wish_now(g: &mut Game, me: PlayerId, god: God, act: Act) -> Vec<Event> {
    crown(g, me);
    g.apply(
        me,
        Intent::Wish {
            god,
            wish: Wish::one(act),
            said: None,
        },
    )
    .unwrap()
}

#[test]
fn a_learned_secret_is_seen_by_its_learner_only() {
    let (mut g, me, foe) = duel(3);
    let third = g.players().find(|&p| p != me && p != foe).unwrap();
    assert!(g.view_for(Some(me), 1).secret(foe).is_none());
    wish_now(&mut g, me, God::Ahamar, Act::Secret { target: foe });
    assert_eq!(g.view_for(Some(me), 1).secret(foe), g.secret(foe));
    assert!(g.view_for(Some(third), 1).secret(foe).is_none());
    assert!(!g.view_for(Some(foe), 1).knows_secret(me, foe), "not told");
}

#[test]
fn a_seen_hand_is_told_to_the_seer_only() {
    let (mut g, me, foe) = duel(3);
    g.give(foe, "Искра");
    let events = wish_now(&mut g, me, God::Maya, Act::Hand { target: foe });
    let seen = events
        .iter()
        .find(|e| matches!(e, Event::HandSeen { .. }))
        .unwrap();
    let Event::HandSeen { cards, .. } = seen else {
        unreachable!()
    };
    assert_eq!(cards.len(), g.hand(foe).len());
    let for_foe = Game::event_for(&g.view_for(Some(foe), 1), Some(foe), seen).unwrap();
    assert!(matches!(for_foe, Event::HandSeen { cards, .. } if cards.is_empty()));
}

#[test]
fn a_blessing_bends_one_copy_and_a_rival_cannot_see_it() {
    let (mut g, me, foe) = duel(3);
    let flame = g.give(me, "Пламя пира");
    let other = g.give(me, "Пламя пира");
    let events = wish_now(&mut g, me, God::Bhava, Act::Bless { card: None });
    assert_eq!(g.def(flame).cost, 1, "cheaper by one");
    assert_eq!(
        g.def(flame).effect,
        Effect::Damage(2 + boost_of(&events)),
        "stronger by the wish's boost"
    );
    assert_eq!(g.def(other).cost, 2, "the other copy as printed");
    assert!(g.view_for(Some(me), 1).card_mod(flame).is_some());
    assert!(g.view_for(Some(foe), 1).card_mod(flame).is_none());
}

#[test]
fn a_blight_weighs_on_the_rivals_dearest_card() {
    let (mut g, me, foe) = duel(3);
    let feast = g.give(foe, "Пламя пира");
    let spark = g.give(foe, "Искра");
    let events = wish_now(&mut g, me, God::Zaga, Act::Blight { target: foe });
    assert!(events.iter().any(|e| matches!(
        e,
        Event::CardChanged { owner, blessed: false, .. } if *owner == foe
    )));
    assert_eq!(g.def(feast).cost, 3);
    assert_eq!(g.def(feast).effect, Effect::Damage(1));
    assert_eq!(g.def(spark).cost, 0, "the cheaper card is spared");
    assert!(
        g.view_for(Some(foe), 1).card_mod(feast).is_some(),
        "the owner sees it"
    );
}

#[test]
fn a_forged_card_is_the_gods_own_and_better() {
    let (mut g, me, _) = duel(3);
    let before = g.hand(me).len();
    let events = wish_now(&mut g, me, God::Trishna, Act::Forge);
    let boost = boost_of(&events);
    assert_eq!(g.hand(me).len(), before + 1);
    let card = *g.hand(me).last().unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::CardForged { card: c, .. } if *c == card))
    );
    assert_eq!(g.def(card).name, forge_template(God::Trishna));
    assert_eq!(g.def(card).cost, 1);
    assert_eq!(g.def(card).effect, Effect::Damage(2 + boost));
}

#[test]
fn a_truce_breaks_on_a_blow_and_ends_at_dusk() {
    let (mut g, me, foe) = duel(1);
    wish_now(&mut g, me, God::Ahamar, Act::Truce { target: foe });
    assert_eq!(g.truces().len(), 1);
    let style = g.style(me);
    let foe_hex = g.champion(foe).unwrap().hex;
    let events = g.apply(me, Intent::Move { to: foe_hex }).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::TruceBroken { player, god: God::Ahamar, .. } if *player == me
    )));
    assert!(g.curses(me).contains(&God::Ahamar));
    assert_eq!(g.style(me), style.saturating_sub(2));
    assert!(g.truces().is_empty());

    let (mut g, me, foe) = duel(3);
    wish_now(&mut g, me, God::Ahamar, Act::Truce { target: foe });
    to_next_dusk(&mut g);
    assert!(g.truces().is_empty(), "gone at dusk");
}

#[test]
fn a_swap_trades_places() {
    let (mut g, me, foe) = duel(3);
    let (a, b) = (g.champion(me).unwrap().hex, g.champion(foe).unwrap().hex);
    wish_now(&mut g, me, God::Maya, Act::Swap { target: foe });
    assert_eq!(g.champion(me).unwrap().hex, b);
    assert_eq!(g.champion(foe).unwrap().hex, a);
}

#[test]
fn a_blessing_takes_the_card_named_or_none_not_in_hand() {
    let (mut g, me, foe) = duel(3);
    let spark = g.give(me, "Искра");
    let feast = g.give(me, "Пламя пира");
    wish_now(&mut g, me, God::Bhava, Act::Bless { card: Some(spark) });
    assert!(g.card_mod(spark).is_some(), "the one named");
    assert!(g.card_mod(feast).is_none(), "not the dearest");

    let (mut g, me, foe2) = duel(3);
    let theirs = g.give(foe2, "Искра");
    crown(&mut g, me);
    assert_eq!(
        g.apply(
            me,
            Intent::Wish {
                god: God::Bhava,
                wish: Wish::one(Act::Bless { card: Some(theirs) }),
                said: None,
            },
        ),
        Err(RuleError::InvalidWish),
        "not a card of one's own"
    );
    let _ = foe;
}
