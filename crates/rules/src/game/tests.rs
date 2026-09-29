use super::*;
use crate::board::GROVE_AGE;
use crate::cards::POOL;
use necromy_dice::Face;

/// A match with no deeds to pick first: most tests are about the moves.
fn started(setup: Setup) -> (Game, Vec<Event>) {
    let (mut g, events) = Game::new(setup);
    g.offers = vec![Vec::new(); g.champions.len()];
    (g, events)
}

fn five() -> Setup {
    Setup {
        seed: 7,
        champions: God::ALL.to_vec(),
        mode: Default::default(),
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

    /// A wish sealed and answered at once, as if dusk had come for it.
    fn wishes(&mut self, player: PlayerId, intent: Intent) -> Result<Vec<Event>, RuleError> {
        let mut events = self.apply(player, intent)?;
        let from = events.len();
        self.answer_wish(player, &mut events);
        self.settle_story(&mut events);
        self.check_victory(&mut events);
        self.log.extend(events[from..].iter().cloned());
        Ok(events)
    }

    /// Everyone still owed a choice passes; dusk gets refusals.
    fn pass_all(&mut self) {
        loop {
            if let Some(&p) = self.wishing().first() {
                self.apply(p, Intent::RefuseWish).unwrap();
                continue;
            }
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
    let (mut g, _) = started(five());
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
    let (a, ea) = started(five());
    let (b, eb) = started(five());
    assert_eq!(ea, eb);
    assert_eq!(a.order(), b.order());
    assert_eq!(a.slice(), b.slice());
}

#[test]
fn replay_from_intents_matches() {
    let (mut a, _) = started(five());
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
    let (mut b, _) = started(five());
    for (p, intent) in intents {
        b.apply(p, intent).unwrap();
    }
    assert_eq!(a.log(), b.log());
    assert!(a.round() > 2, "bots should keep the table moving");
}

#[test]
fn everyone_starts_with_a_hand() {
    let (g, _) = started(five());
    for p in g.players() {
        assert_eq!(g.hand(p).len(), g.champion(p).unwrap().hand_limit());
    }
}

#[test]
fn everyone_takes_their_turn_at_once() {
    let (mut game, _) = started(five());
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
    let (mut game, _) = started(five());
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
    let (mut game, _) = started(five());
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
    let (mut game, _) = started(five());
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
    let (mut game, _) = started(five());
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
    g.board.tile_mut(here).unwrap().corpse = Some(Corpse::fresh());
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
            mode: Default::default(),
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
        // A match won early (seed 5: round 7) has had little time for cards.
        let early = g.winner().is_some() && g.round() < 8;
        assert!(
            played > 10 || early,
            "seed {seed}: bots played only {played} cards"
        );
        poisoned += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::Poisoned { .. }))
            .count();
    }
    assert!(poisoned > 0, "no bot ever poisoned anyone");
}

/// A world with only some mechanics (§21.2) plays on just the same: bots
/// never stall, never break a rule, and what the world lacks never happens.
#[test]
fn bots_play_in_worlds_lacking_mechanics() {
    for seed in 0..40u64 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
        });
        let mut pick = crate::rng::Rng::new(seed ^ 0xFEA7);
        let features: Vec<Feature> = Feature::ALL
            .into_iter()
            .filter(|_| pick.below(2) == 0)
            .collect();
        g.world = World::of(features.iter().copied());
        if !g.has(Feature::Militia) {
            g.militia.clear();
        }
        let since = g.log().len();
        for _ in 0..1500 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent.clone())
                .unwrap_or_else(|e| panic!("seed {seed} {features:?}: bot {p:?} {intent:?}: {e}"));
        }
        assert!(
            g.winner().is_some() || g.round() >= 6,
            "seed {seed} {features:?}: only reached round {}",
            g.round()
        );
        for e in &g.log()[since..] {
            let lacking = match e {
                Event::GroveGrew { .. } => !g.has(Feature::Groves),
                Event::MobAppeared { mob } => match mob.kind {
                    MobKind::Undead => !g.has(Feature::Undead),
                    MobKind::Beast { .. } => !g.has(Feature::Beasts),
                    MobKind::Monster { .. } => !g.has(Feature::Monsters),
                    MobKind::Guest => !g.has(Feature::Guests),
                },
                Event::Poisoned { .. } => !g.has(Feature::Poison),
                Event::TrialSet { .. } => !g.has(Feature::Trials),
                Event::ItemGained { .. } => !g.has(Feature::Loot),
                Event::GuardSpawned { .. } => !g.has(Feature::Guard),
                Event::Hid { .. } => !g.has(Feature::Stealth),
                Event::SettlementRuined { .. } => !g.has(Feature::Ruins),
                Event::CorpseAppeared { .. } => !g.has(Feature::Bodies),
                _ => false,
            };
            assert!(!lacking, "seed {seed} {features:?}: {e:?}");
        }
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
        let (mut g, _) = started(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
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
                mode: Default::default(),
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
            mode: Default::default(),
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
    g.board.tile_mut(here).unwrap().corpse = Some(Corpse::fresh());
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
    let lying: Vec<Hex> = g.board().corpses().map(|(h, _)| h).collect();
    for hex in lying {
        g.board.tile_mut(hex).unwrap().corpse = None;
    }
    for hex in [Hex::ZERO, Hex::new(0, 1)] {
        g.board.tile_mut(hex).unwrap().corpse = Some(Corpse::fresh());
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
            mode: Default::default(),
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
    // The default drift: a world nobody cools ends in Trishna's dark. Bots
    // now wish Maya's mist for their deeds, and water cools fire: fewer do.
    assert!(
        trishna_dark >= 4,
        "only {trishna_dark} of 20 ended in Devouring"
    );
}

// ---- Style, the Crown and Threat (§6) ----

#[test]
fn entering_a_settlement_claims_it_but_land_is_no_style() {
    let (mut g, me, _) = duel(3);
    let spot = Hex::new(1, 0);
    g.board.tile_mut(spot).unwrap().terrain = Terrain::Settlement;
    let events = g.apply(me, Intent::Move { to: spot }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Claimed { player, from: None, .. } if *player == me))
    );
    assert_eq!(g.owner(spot), Some(me));
    // The first to take a settlement earns that, once (§21.5).
    assert!(events.iter().any(|e| matches!(
        e,
        Event::First {
            novelty: Novelty::TookSettlement,
            ..
        }
    )));
    // But it pays nothing at dawn: only the Table does.
    let style = g.style(me);
    let mut ev = Vec::new();
    g.dawn(&mut ev);
    assert_eq!(g.style(me), style);
    g.claims.insert((0, 0), me);
    g.dawn(&mut ev);
    assert_eq!(g.style(me), style + u16::from(g.taste().table));
}

/// Everything has been done first already: no first earns Style here.
fn nothing_new(g: &mut Game) {
    let all = [
        Novelty::WonBattle,
        Novelty::FelledGuard,
        Novelty::LaidToRest,
        Novelty::SlewBeast,
        Novelty::PassedTrial,
        Novelty::TookSettlement,
        Novelty::TookTable,
        Novelty::Rebuilt,
        Novelty::Sacrificed,
        Novelty::Hid,
        Novelty::FinishedLine,
    ]
    .into_iter()
    .chain(WishKind::ALL.map(Novelty::Wished))
    .chain(Feature::ALL.map(Novelty::Brought))
    .chain(Feature::ALL.map(Novelty::PlayedFor));
    for n in all {
        g.firsts.insert(n, PlayerId(0));
    }
}

#[test]
fn the_crown_goes_to_the_leader_and_ties_keep_it() {
    let (mut g, me, foe) = duel(3);
    let mut ev = Vec::new();
    g.add_style(me, 3, StyleReason::Territory, &mut ev);
    g.add_style(foe, 1, StyleReason::Territory, &mut ev);
    g.crown(&mut ev);
    assert_eq!(g.dominant(), Some(me));
    assert_eq!(g.threat(me), 1, "the Crown draws the guard's eye");

    g.add_style(foe, 2, StyleReason::Territory, &mut ev);
    g.crown(&mut ev);
    assert_eq!(g.dominant(), Some(me), "a tie keeps the Crown where it was");

    let third = g.players().find(|&p| p != me && p != foe).unwrap();
    g.add_style(third, 3, StyleReason::Territory, &mut ev);
    g.add_style(me, -1, StyleReason::Oath, &mut ev);
    g.crown(&mut ev);
    assert_eq!(
        g.dominant(),
        None,
        "two new leaders, the table is contested"
    );
}

#[test]
fn nobody_is_crowned_with_no_style() {
    let (mut g, _) = started(five());
    to_next_dusk(&mut g);
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
        let (mut g, _) = started(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
        });
        let me = g.current_player();
        let foe = g.order()[1];
        g.place(me, Hex::ZERO);
        g.place(foe, Hex::new(1, 0));
        // The foe has taken their turn: the attack goes ahead at once.
        g.finish(foe);
        nothing_new(&mut g);
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
    g.board.tile_mut(here).unwrap().corpse = Some(Corpse::fresh());
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
        hp: GUARD_HEALTH,
    });
    // Its hex is no path: a step onto it is an attack (§20.4).
    assert!(!g.reach().contains_key(&Hex::new(1, 0)));
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::GuardAttacked { .. }))
    );
    assert_eq!(g.champion(me).unwrap().hex, Hex::new(0, 0));
}

#[test]
fn crowns_and_guards_happen_in_bot_games() {
    let (mut crowned, mut guards) = (0, 0);
    for seed in 0..20 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
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

/// Crowns `me` at a dusk and returns the events.
fn crown(g: &mut Game, me: PlayerId) -> Vec<Event> {
    let mut ev = Vec::new();
    g.add_style(me, 5, StyleReason::Territory, &mut ev);
    g.crown(&mut ev);
    ev
}

#[test]
fn dusk_waits_for_every_wish_and_answers_least_style_first() {
    let (mut g, me, foe) = duel(3);
    let mut ev = Vec::new();
    g.add_style(me, 5, StyleReason::Territory, &mut ev);
    g.add_style(foe, 2, StyleReason::Territory, &mut ev);
    let land = Intent::Wish {
        god: God::Bhava,
        wish: wish_of(WishKind::Land, None),
        said: None,
    };
    // Sealed mid-turn: the turn goes on, and it is sealed once only.
    let sealed = g.apply(me, land.clone()).unwrap();
    assert!(matches!(sealed.as_slice(), [Event::WishSealed { player }] if *player == me));
    assert!(g.free_to_act(me) && !g.may_wish(me));
    assert_eq!(g.apply(me, land), Err(RuleError::InvalidWish));
    // Rivals see that a wish was sealed, not what it was.
    assert_eq!(g.view_for(Some(foe), 1).seal(me), &Seal::Wish(None));
    assert!(matches!(
        g.view_for(Some(me), 1).seal(me),
        Seal::Wish(Some(_))
    ));

    // The day ends: the Crown goes to the leader, and dusk waits for the rest.
    g.apply(me, Intent::EndTurn).unwrap();
    assert_eq!(g.at_dusk(), Some(DuskStep::Sealing));
    assert_eq!(g.dominant(), Some(me));
    assert!(g.awaiting().contains(&foe) && !g.awaiting().contains(&me));
    assert_eq!(g.apply(foe, Intent::EndTurn), Err(RuleError::NotYourTurn));
    let before = g.log().len();
    for p in g.wishing() {
        let intent = if p == foe {
            Intent::Wish {
                god: God::Maya,
                wish: wish_of(WishKind::Peace, None),
                said: None,
            }
        } else {
            Intent::RefuseWish
        };
        g.apply(p, intent).unwrap();
    }
    // Least Style first, the Crown last; then the night.
    let granted: Vec<PlayerId> = g.log()[before..]
        .iter()
        .filter_map(|e| match e {
            Event::WishGranted { player, .. } => Some(*player),
            _ => None,
        })
        .collect();
    assert_eq!(granted, vec![foe, me]);
    assert_eq!(g.at_dusk(), None);
    assert_eq!(g.time(), TimeOfDay::Night);
    assert!(!g.may_wish(me), "no wishes by night");
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
    g.wishes(
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
    nothing_new(&mut g);
    crown(&mut g, me);
    let style = g.style(me);
    let events = g
        .wishes(
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
    // Two Style of riches, two lost: the Crown pays double for a wish
    // without style (§21.4).
    assert_eq!(g.style(me), style);
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
        g.wishes(
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
        g.wishes(
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
    g.wishes(
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
    let threat = g.threat(me);
    g.wishes(
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
    let events = g
        .wishes(
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
fn bots_wish_in_many_ways() {
    let mut kinds = std::collections::HashSet::new();
    let mut gods = std::collections::HashSet::new();
    for seed in 0..20 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
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
    let (mut g, _) = started(five());
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
    nothing_new(&mut g);
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
    let mut kinds: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
    for seed in 0..20 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
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
            match e {
                Event::LineTold { line } => {
                    kinds.entry(format!("{:?}", line.kind)).or_default().0 += 1
                }
                Event::LineDone { line } => {
                    kinds.entry(format!("{:?}", line.kind)).or_default().1 += 1
                }
                _ => {}
            }
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
    assert!(
        done * 5 >= told,
        "only {done} of {told} lines done: {kinds:?}"
    );
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
    g.shift_stages(&mut ev);
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
        .wishes(
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
            g.wishes(
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
        g.wishes(
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
        .wishes(
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
    g.wishes(
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
        g.wishes(
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

#[test]
fn tribute_is_a_card_given_or_threat_taken() {
    let (mut g, me, foe) = duel(3);
    let gift = g.give(foe, "Искра");
    let events = wish_now(&mut g, me, God::Trishna, Act::Tribute);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::WindowOpened { kind: WindowKind::Tribute { asker }, .. } if *asker == me
    )));
    let owed = g.awaiting();
    assert!(!owed.contains(&me) && owed.contains(&foe));
    let threats: Vec<u8> = g.players().map(|p| g.threat(p)).collect();
    let mine = g.hand(me).len();
    // The foe gives a card; everyone else refuses.
    for p in owed {
        let intent = if p == foe {
            Intent::Play {
                card: gift,
                target: Target::None,
            }
        } else {
            Intent::Pass
        };
        g.apply(p, intent).unwrap();
    }
    assert!(g.windows().is_empty());
    assert_eq!(g.hand(me).len(), mine + 1);
    assert!(g.hand(me).contains(&gift));
    assert!(!g.hand(foe).contains(&gift));
    assert_eq!(g.threat(foe), threats[foe.0 as usize], "gave, no Threat");
    let refuser = g.players().find(|&p| p != me && p != foe).unwrap();
    assert_eq!(
        g.threat(refuser),
        threats[refuser.0 as usize] + crate::game::TRIBUTE_THREAT as u8
    );
}

#[test]
fn a_wager_pays_if_it_happens_and_is_a_debt_if_not() {
    // Won: the foe fights today.
    let (mut g, me, foe) = duel(1);
    wish_now(
        &mut g,
        me,
        God::Ahamar,
        Act::Wager {
            target: foe,
            bet: crate::game::Bet::Fight,
        },
    );
    let foe_hex = g.champion(foe).unwrap().hex;
    g.apply(me, Intent::Move { to: foe_hex }).unwrap();
    g.pass_all();
    to_next_dusk(&mut g);
    assert!(g.log().iter().any(|e| matches!(e, Event::WagerWon { .. })));
    assert!(g.wagers().is_empty());

    // Lost: the foe takes no land today; the debt is the god's curse.
    let (mut g, me, foe) = duel(3);
    wish_now(
        &mut g,
        me,
        God::Ahamar,
        Act::Wager {
            target: foe,
            bet: crate::game::Bet::Claim,
        },
    );
    to_next_dusk(&mut g);
    assert!(g.log().iter().any(|e| matches!(e, Event::WagerLost { .. })));
    assert!(g.curses(me).contains(&God::Ahamar));
}

#[test]
fn a_hallowed_deck_hides_its_gifts_until_drawn() {
    let (mut g, me, foe) = duel(3);
    let wood = |g: &Game| {
        g.deck
            .iter()
            .filter(|&&c| g.def(c).element == Some(Element::Wood))
            .count()
    };
    assert!(wood(&g) > 0, "the deck holds wood");
    let events = wish_now(&mut g, me, God::Bhava, Act::Hallow { element: None });
    let count = events
        .iter()
        .find_map(|e| match e {
            Event::DeckChanged {
                element: Element::Wood,
                count,
                blessed: true,
                ..
            } => Some(*count as usize),
            _ => None,
        })
        .expect("the deck changed");
    assert!(count >= 1 && count <= wood(&g));
    let changed: Vec<CardId> = g
        .deck
        .iter()
        .copied()
        .filter(|&c| g.card_mod(c).is_some())
        .collect();
    assert_eq!(changed.len(), count);
    // Nobody sees which, the asker neither.
    for p in [me, foe] {
        let v = g.view_for(Some(p), 1);
        assert!(changed.iter().all(|&c| v.card_mod(c).is_none()));
    }
    // Drawn, the gift shows in its holder's hand.
    let card = changed[0];
    g.deck.retain(|&c| c != card);
    g.deck.push(card);
    let mut ev = Vec::new();
    g.draw(foe, 1, &mut ev);
    assert!(g.view_for(Some(foe), 1).card_mod(card).is_some());
}

#[test]
fn a_rotted_deck_takes_the_element_the_god_quenches() {
    let (mut g, me, _) = duel(3);
    let events = wish_now(&mut g, me, God::Bhava, Act::Rot { element: None });
    // Wood quenches earth.
    assert!(events.iter().any(|e| matches!(
        e,
        Event::DeckChanged {
            element: Element::Earth,
            blessed: false,
            ..
        }
    )));
}

#[test]
fn a_planted_curse_bites_whoever_else_draws_it_and_is_gone() {
    let (mut g, me, foe) = duel(3);
    let deck = g.deck.len();
    wish_now(&mut g, me, God::Trishna, Act::Plant);
    assert_eq!(g.deck.len(), deck + 1);
    let (&curse, _) = g.planted.iter().next().expect("planted");
    let depth = g.deck.len() - 1 - g.deck.iter().position(|&c| c == curse).unwrap();
    assert!(depth <= 3, "near the top");
    // Up to the top, then the foe draws it.
    g.deck.retain(|&c| c != curse);
    g.deck.push(curse);
    let hp = g.champion(foe).unwrap().hp;
    let mut ev = Vec::new();
    g.draw(foe, 1, &mut ev);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::CurseDrawn { bit: true, .. }))
    );
    assert_eq!(g.champion(foe).unwrap().hp, hp - 1);
    assert!(!g.hand(foe).contains(&curse), "gone from the game");
    assert!(!g.discard.contains(&curse));

    // The planter draws their own: no bite.
    let (mut g, me, _) = duel(3);
    wish_now(&mut g, me, God::Trishna, Act::Plant);
    let (&curse, _) = g.planted.iter().next().unwrap();
    g.deck.retain(|&c| c != curse);
    g.deck.push(curse);
    let hp = g.champion(me).unwrap().hp;
    let mut ev = Vec::new();
    g.draw(me, 1, &mut ev);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::CurseDrawn { bit: false, .. }))
    );
    assert_eq!(g.champion(me).unwrap().hp, hp);
}

#[test]
fn foreseeing_shows_the_top_to_the_seer_only() {
    let (mut g, me, foe) = duel(3);
    let top: Vec<DefId> = g.deck.iter().rev().take(3).map(|&c| g.def_id(c)).collect();
    let events = wish_now(&mut g, me, God::Ahamar, Act::Foresee);
    let seen = events
        .iter()
        .find(|e| matches!(e, Event::Foreseen { .. }))
        .unwrap();
    assert!(matches!(seen, Event::Foreseen { cards, .. } if *cards == top));
    let for_foe = Game::event_for(&g.view_for(Some(foe), 1), Some(foe), seen).unwrap();
    assert!(matches!(for_foe, Event::Foreseen { cards, .. } if cards.is_empty()));
}

// ---- Trials (§20.2) ----

/// A trial of `god` on `hex`, the hex made plain land of that god.
fn trial_on(g: &mut Game, hex: Hex, god: God, boon: Boon) {
    let tile = g.board.tile_mut(hex).unwrap();
    tile.terrain = Terrain::Plains;
    tile.region = Some(god);
    g.trials.push(Trial {
        id: 99,
        hex,
        god,
        boon,
        deadline: g.round() + TRIAL_ROUNDS,
        tried: Vec::new(),
    });
}

/// Enough copies of a card to burn one for every die `player` throws.
fn burn_all(g: &mut Game, player: PlayerId, name: &str) -> Vec<CardId> {
    let might = g.champion(player).unwrap().might;
    (0..might).map(|_| g.give(player, name)).collect()
}

#[test]
fn a_trial_stops_the_walker_and_asks_for_a_burn() {
    let (mut g, me, _) = duel(3);
    trial_on(&mut g, Hex::new(1, 0), God::Zaga, Boon::Style);
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert_eq!(g.move_points(me), 0);
    assert!(matches!(
        g.to_answer(me).map(|w| w.kind),
        Some(WindowKind::Trial { player, .. }) if player == me
    ));
    assert_eq!(g.battle_dice(me), Some(g.champion(me).unwrap().might));
    assert_eq!(g.trial_at(Hex::new(1, 0)).unwrap().tried, vec![me]);
}

#[test]
fn passing_a_trial_takes_it_and_gives_the_boon() {
    let (mut g, me, _) = duel(3);
    nothing_new(&mut g);
    trial_on(&mut g, Hex::new(1, 0), God::Zaga, Boon::Style);
    // Bodies burn for shields: Zaga's face.
    let cards = burn_all(&mut g, me, "Упокоить");
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    let before = g.style(me);
    let events = g.apply(me, Intent::Burn { cards }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TrialPassed { got, need: 2, .. } if *got >= 2))
    );
    // No die left to throw.
    assert!(!events.iter().any(|e| matches!(e, Event::DiceThrown { .. })));
    assert!(g.trials().is_empty());
    // Mid stage: 1 + 1.
    assert_eq!(g.style(me), before + 2);
}

#[test]
fn failing_a_trial_costs_its_price_and_leaves_it_for_others() {
    let (mut g, me, _) = duel(3);
    trial_on(&mut g, Hex::new(1, 0), God::Zaga, Boon::Style);
    // Tricks burn for strikes: nothing Zaga counts.
    let cards = burn_all(&mut g, me, "Искра");
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    let events = g.apply(me, Intent::Burn { cards }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TrialFailed { got: 0, .. }))
    );
    // Zaga's price: the earth holds on.
    assert!(g.champion(me).unwrap().rooted);
    assert_eq!(g.trials().len(), 1);
    // Once is all: the same champion walks over it freely now.
    assert!(g.trial_for(me, Hex::new(1, 0)).is_none());
}

#[test]
fn sun_counts_by_day_only_and_the_element_always() {
    let (mut g, _, _) = duel(3);
    trial_on(&mut g, Hex::new(1, 0), God::Trishna, Boon::Cards);
    let trial = g.trial_at(Hex::new(1, 0)).unwrap().clone();
    g.time = TimeOfDay::Day;
    assert!(g.trial_counts(&trial, Face::Sun));
    assert!(!g.trial_counts(&trial, Face::Strike));
    g.time = TimeOfDay::Night;
    assert!(!g.trial_counts(&trial, Face::Sun));
    assert!(g.trial_counts(&trial, Face::Element));
}

#[test]
fn a_trial_stage_sets_how_many_faces_it_asks() {
    let (mut g, _, _) = duel(3);
    trial_on(&mut g, Hex::new(1, 0), God::Maya, Boon::Cards);
    trial_on(&mut g, Hex::new(2, 0), God::Ahamar, Boon::Cards);
    let maya = g.trial_at(Hex::new(1, 0)).unwrap().clone();
    let ahamar = g.trial_at(Hex::new(2, 0)).unwrap().clone();
    g.pantheon.stages = [0; 5];
    assert_eq!((g.trial_need(&maya), g.trial_need(&ahamar)), (1, 1));
    g.pantheon.stages = [2; 5];
    assert_eq!((g.trial_need(&maya), g.trial_need(&ahamar)), (3, 2));
}

#[test]
fn no_path_runs_through_a_trial() {
    let (mut g, me, _) = duel(4);
    let trial = Hex::new(1, 0);
    trial_on(&mut g, trial, God::Bhava, Boon::Cards);
    assert!(g.reachable(me).contains_key(&trial));
    for hex in g.reachable(me).into_keys() {
        let path = g.path_to(me, hex).unwrap();
        assert!(
            !path[..path.len() - 1].contains(&trial),
            "{hex:?} via the trial"
        );
    }
}

#[test]
fn the_gods_keep_trials_on_the_board_and_let_old_ones_fade() {
    let (mut g, _) = started(five());
    let mut events = Vec::new();
    g.trials_at_dusk(&mut events);
    assert_eq!(g.trials().len(), TRIALS_ON_BOARD);
    for t in g.trials() {
        let tile = g.board.tile(t.hex).unwrap();
        assert_eq!(tile.region, Some(t.god));
        assert!(
            g.players()
                .all(|p| g.champion(p).unwrap().hex.unsigned_distance_to(t.hex) >= 2)
        );
    }
    let old: Vec<Hex> = g.trials().iter().map(|t| t.hex).collect();
    g.round += TRIAL_ROUNDS;
    let mut events = Vec::new();
    g.trials_at_dusk(&mut events);
    let faded = events
        .iter()
        .filter(|e| matches!(e, Event::TrialFaded { .. }))
        .count();
    assert_eq!(faded, old.len());
    assert_eq!(g.trials().len(), TRIALS_ON_BOARD);
}

#[test]
fn passing_the_trial_of_a_line_closes_it() {
    let (mut g, me, _) = duel(3);
    trial_on(&mut g, Hex::new(1, 0), God::Zaga, Boon::Favour);
    g.lines.push(Line {
        id: 7,
        owner: me,
        god: God::Zaga,
        kind: LineKind::Ordeal,
        goal: Goal::PassTrial(Hex::new(1, 0)),
        deadline: g.round() + 4,
        style: 3,
        stake: 0,
    });
    let cards = burn_all(&mut g, me, "Упокоить");
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    let events = g.apply(me, Intent::Burn { cards }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::LineDone { line } if line.id == 7))
    );
}

#[test]
fn bots_try_trials() {
    let mut tried = 0;
    let mut gained = 0;
    let mut fought_guard = 0;
    let mut risen = 0;
    for seed in 0..30 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
        });
        for _ in 0..1500 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            g.apply(p, intent).unwrap();
        }
        tried += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::TrialPassed { .. } | Event::TrialFailed { .. }))
            .count();
        gained += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::ItemGained { .. }))
            .count();
        risen += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::MobAppeared { .. }))
            .count();
        fought_guard += g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::GuardAttacked { .. }))
            .count();
    }
    assert!(tried > 0, "no bot ever tried a trial");
    assert!(gained > 0, "no bot ever gained an item");
    assert!(fought_guard > 0, "no bot ever fought the guard");
    assert!(risen > 0, "no dead ever rose");
}

#[test]
fn a_forged_card_takes_the_name_the_god_gave_it() {
    let (mut g, me, _) = duel(3);
    crown(&mut g, me);
    let said = crate::game::Said {
        text: "выкуй мне клинок".into(),
        grade: 2,
        speech: "Держи.".into(),
        reason: "в духе".into(),
        forged: Some((
            "«Клинок голодного пира»".into(),
            "Режет, пока не насытится — а он не насытится никогда, и это очень длинная строка"
                .into(),
        )),
    };
    g.wishes(
        me,
        Intent::Wish {
            god: God::Trishna,
            wish: Wish::one(Act::Forge),
            said: Some(said),
        },
    )
    .unwrap();
    let card = *g.hand(me).last().unwrap();
    assert_eq!(g.card_name(card), "Клинок голодного пира", "quotes trimmed");
    let flavor = g.card_mod(card).unwrap().flavor.clone().unwrap();
    assert!(flavor.chars().count() <= crate::game::FORGED_LINE);
    // The rules still read it as its template.
    assert_eq!(g.def(card).name, forge_template(God::Trishna));
}

#[test]
fn a_trial_in_the_open_brings_the_hidden_out_but_cover_keeps_them() {
    let (mut g, me, _) = duel(3);
    trial_on(&mut g, Hex::new(1, 0), God::Zaga, Boon::Style);
    g.champ_mut(me).hidden = true;
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(!g.is_hidden(me), "plains: the gods watch in the open");

    let (mut g, me, foe) = duel(3);
    trial_on(&mut g, Hex::new(1, 0), God::Bhava, Boon::Style);
    g.board.tile_mut(Hex::new(1, 0)).unwrap().terrain = Terrain::Forest;
    g.champ_mut(me).hidden = true;
    let events = g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert!(g.is_hidden(me), "the woods keep them");
    assert!(g.trial_of(me).is_some());
    // The foe learns nothing of it: no window, no name on the trial, no event.
    let view = g.view_for(Some(foe), 3);
    assert!(view.windows().is_empty());
    assert!(view.trial_at(Hex::new(1, 0)).unwrap().tried.is_empty());
    for e in &events {
        if matches!(e, Event::TrialBegun { .. } | Event::WindowOpened { .. }) {
            assert!(Game::event_for(&view, Some(foe), e).is_none(), "{e:?}");
        }
    }
    // The one in hiding sees their own trial.
    let mine = g.view_for(Some(me), 3);
    assert!(mine.trial_of(me).is_some());
    let cards = burn_all(&mut g, me, "Искра");
    let events = g.apply(me, Intent::Burn { cards }).unwrap();
    let view = g.view_for(Some(foe), 3);
    assert!(events.iter().all(|e| {
        !matches!(
            e,
            Event::TrialPassed { .. } | Event::TrialFailed { .. } | Event::Burned { .. }
        ) || Game::event_for(&view, Some(foe), e).is_none()
    }));
}

// ---- Items (§20.3) ----

fn item_named(name: &str) -> crate::items::ItemId {
    crate::items::ItemId(
        crate::items::ITEMS
            .iter()
            .position(|d| d.name == name)
            .unwrap_or_else(|| panic!("no item {name}")) as u16,
    )
}

fn wear(g: &mut Game, player: PlayerId, name: &str) -> crate::items::ItemId {
    let item = item_named(name);
    g.loot.retain(|&i| i != item);
    g.equip(player, item, Gain::Loot, &mut Vec::new());
    item
}

#[test]
fn a_new_item_pushes_the_old_one_to_the_ground() {
    let (mut g, me, _) = duel(3);
    let bow = wear(&mut g, me, "Тисовый лук");
    let sword = wear(&mut g, me, "Меч присяги");
    assert_eq!(g.gear(me)[crate::items::Slot::Weapon.index()], Some(sword));
    assert_eq!(g.ground_items(), &[(Hex::new(0, 0), bow)]);
}

#[test]
fn weapons_add_dice_and_the_light_adds_more() {
    let (mut g, me, _) = duel(3);
    let might = g.champion(me).unwrap().might;
    wear(&mut g, me, "Меч присяги");
    assert_eq!(g.dice_for(me, false), might + 1);
    // Ahamar in his light stage: the sword gives two.
    g.pantheon.stages[God::Ahamar.index()] = 0;
    assert_eq!(g.dice_for(me, false), might + 2);
    // A bow counts only in attack.
    let (mut g, me, _) = duel(3);
    wear(&mut g, me, "Тисовый лук");
    assert_eq!(g.dice_for(me, true), might);
    assert_eq!(g.dice_for(me, false), might + 1);
}

#[test]
fn the_quenching_element_breaks_an_item() {
    // A metal card that gets through breaks a wood item.
    let (mut g, me, foe) = duel(2);
    let bow = wear(&mut g, foe, "Тисовый лук");
    g.champ_mut(me).spirit_points = 3;
    play_at(&mut g, me, "Приговор порядка", foe);
    assert_eq!(g.gear(foe), [None; 3]);
    assert_eq!(g.loot[0], bow, "under the loot deck");
    // An Element face of a metal patron does the same in battle.
    let (mut g, _, foe) = duel(2);
    wear(&mut g, foe, "Посох-корень");
    let mut events = Vec::new();
    g.element_breaks_ward(Element::Metal, foe, &[Face::Element], &mut events);
    assert!(events.iter().any(|e| matches!(e, Event::ItemBroken { .. })));
    // Other elements leave it be.
    let (mut g, me, foe) = duel(2);
    wear(&mut g, foe, "Тисовый лук");
    play_at(&mut g, me, "Искра", foe);
    assert!(g.gear(foe)[0].is_some());
}

#[test]
fn the_fallen_drop_an_item_and_a_step_picks_it_up() {
    let (mut g, me, foe) = duel(2);
    let seal = wear(&mut g, foe, "Печать реестра");
    let at = g.champion(foe).unwrap().hex;
    g.fall(foe, &mut Vec::new());
    assert_eq!(g.ground_items(), &[(at, seal)]);
    // `me` walks over and puts it on.
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    g.pass_all();
    g.apply(me, Intent::Move { to: at }).unwrap();
    assert_eq!(g.gear(me)[crate::items::Slot::Relic.index()], Some(seal));
    assert!(g.ground_items().is_empty());
    assert_eq!(g.hand_limit(me), g.champion(me).unwrap().hand_limit() + 1);
}

#[test]
fn an_item_given_at_a_temple_is_a_great_offering() {
    let (mut g, me, _) = duel(3);
    wear(&mut g, me, "Кубок пира");
    let slot = crate::items::Slot::Relic;
    assert_eq!(
        g.apply(me, Intent::Sacrifice { slot }),
        Err(RuleError::NotAtTemple)
    );
    let tile = g.board.tile_mut(Hex::new(0, 0)).unwrap();
    tile.terrain = Terrain::Temple;
    tile.region = Some(God::Zaga);
    let before = g.favor(me, God::Zaga);
    g.apply(me, Intent::Sacrifice { slot }).unwrap();
    assert_eq!(g.gear(me), [None; 3]);
    assert_eq!(g.favor(me, God::Zaga), before + u16::from(SACRIFICE));
    assert_eq!(
        g.apply(me, Intent::Sacrifice { slot }),
        Err(RuleError::NothingWorn)
    );
}

#[test]
fn items_work_as_the_turn_starts_and_dark_gods_take_a_toll() {
    let (mut g, me, _) = duel(3);
    wear(&mut g, me, "Посох-корень");
    wear(&mut g, me, "Сандалии пути");
    g.champ_mut(me).hp = 1;
    let mut events = Vec::new();
    g.start_turn(me, &mut events);
    assert_eq!(g.champion(me).unwrap().hp, 2);
    assert_eq!(g.move_points(me), MOVE_POINTS + 1);

    // Trishna dark: her cup burns whoever holds it, never to death.
    let (mut g, me, _) = duel(3);
    wear(&mut g, me, "Кубок пира");
    g.pantheon.stages[God::Trishna.index()] = 2;
    let hp = g.champion(me).unwrap().hp;
    let mut events = Vec::new();
    g.start_turn(me, &mut events);
    assert!(events.iter().any(|e| matches!(e, Event::ItemToll { .. })));
    assert_eq!(g.champion(me).unwrap().hp, hp - 1);
    g.champ_mut(me).hp = 1;
    g.start_turn(me, &mut Vec::new());
    assert_eq!(g.champion(me).unwrap().hp, 1);
}

#[test]
fn trials_and_story_lines_give_loot() {
    let (mut g, me, _) = duel(3);
    trial_on(&mut g, Hex::new(1, 0), God::Zaga, Boon::Loot);
    let top = *g.loot.last().unwrap();
    let cards = burn_all(&mut g, me, "Упокоить");
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    g.apply(me, Intent::Burn { cards }).unwrap();
    assert!(g.gear(me).contains(&Some(top)));

    let (mut g, me, _) = duel(3);
    g.lines.push(Line {
        id: 7,
        owner: me,
        god: God::Maya,
        kind: LineKind::Pilgrimage,
        goal: Goal::ReachHex(Hex::new(1, 0)),
        deadline: g.round() + 4,
        style: 2,
        stake: 0,
    });
    let loot = g.loot_len();
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    g.pass_all();
    assert_eq!(g.loot_len(), loot - 1);
    assert!(g.gear(me).iter().any(Option::is_some));
}

#[test]
fn a_view_hides_the_loot_order_not_what_is_worn() {
    let (mut g, me, foe) = duel(3);
    let bow = wear(&mut g, foe, "Тисовый лук");
    let v = g.view_for(Some(me), 11);
    assert_eq!(v.gear(foe)[0], Some(bow));
    let mut a = g.loot.clone();
    let mut b = v.loot.clone();
    a.sort();
    b.sort();
    assert_eq!(a, b);
}

// ---- Fighting the guard (§20.4) ----

/// The guard next to `me`, hunting them, with `hp` left.
fn guard_by(g: &mut Game, me: PlayerId, hp: u8) -> Hex {
    let hex = Hex::new(-1, 0);
    g.board.tile_mut(hex).unwrap().terrain = Terrain::Plains;
    g.guard = Some(Guard {
        hex,
        target: me,
        hp,
    });
    hex
}

#[test]
fn stepping_onto_the_guard_attacks_it() {
    let (mut g, me, _) = duel(3);
    let hex = guard_by(&mut g, me, GUARD_HEALTH);
    assert!(g.attackable(me).contains(&hex));
    let events = g.apply(me, Intent::Move { to: hex }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::GuardAttacked { attacker } if *attacker == me))
    );
    assert!(matches!(
        g.to_answer(me).map(|w| w.kind),
        Some(WindowKind::GuardBattle { attacker }) if attacker == me
    ));
    assert_eq!(g.battle_dice(me), Some(g.dice_for(me, false)));
    // The champion stays where they were; the fight ends the walk.
    g.apply(me, Intent::Pass).unwrap();
    assert_eq!(g.champion(me).unwrap().hex, Hex::new(0, 0));
    assert_eq!(g.move_points(me), 0);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ThreatChanged { player, .. } if *player == me))
    );
}

#[test]
fn a_felled_guard_leaves_and_pays_its_feller() {
    let (mut g, me, _) = duel(3);
    // More strikes than the guard has dice to shield.
    g.champ_mut(me).might = 5;
    let hex = guard_by(&mut g, me, 1);
    let cards = burn_all(&mut g, me, "Искра");
    let (style, loot) = (g.style(me), g.loot_len());
    g.apply(me, Intent::Move { to: hex }).unwrap();
    let events = g.apply(me, Intent::Burn { cards }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::GuardFell { by, .. } if *by == me))
    );
    assert!(g.guard().is_none());
    assert!(g.style(me) > style);
    assert_eq!(g.loot_len(), loot - 1);
    assert!(g.gear(me).iter().any(Option::is_some));
}

#[test]
fn the_guard_keeps_its_wounds() {
    let (mut g, me, _) = duel(3);
    guard_by(&mut g, me, GUARD_HEALTH);
    let mut events = Vec::new();
    g.hurt_guard(me, 1, &mut events);
    assert_eq!(g.guard().unwrap().hp, GUARD_HEALTH - 1);
    // It stands down and comes out whole next time.
    g.guard = None;
    g.threat[me.0 as usize] = 9;
    let mut events = Vec::new();
    g.guard_phase(&mut events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::GuardSpawned { .. }))
    );
    // It may strike at once and take a blow back; it began whole.
    let taken: u8 = events
        .iter()
        .map(|e| match e {
            Event::GuardHurt { amount, .. } => *amount,
            _ => 0,
        })
        .sum();
    let left = g.guard().map_or(0, |g| g.hp);
    assert_eq!(left + taken, GUARD_HEALTH);
}

// ---- Mobs and factions (§20.4) ----

fn undead_on(g: &mut Game, hex: Hex, hp: u8) -> u32 {
    g.next_mob += 1;
    let id = g.next_mob;
    g.mobs.push(Mob {
        id,
        kind: MobKind::Undead,
        hex,
        hp,
    });
    id
}

#[test]
fn untended_bodies_rise_where_no_grove_grows() {
    let (mut g, _, _) = duel(3);
    let rock = Hex::new(0, 3);
    let tile = g.board.tile_mut(rock).unwrap();
    tile.terrain = Terrain::Mountain;
    tile.region = Some(God::Ahamar);
    tile.corpse = Some(Corpse {
        age: UNDEAD_AGE,
        hero: false,
    });
    let meadow = Hex::new(3, -3);
    let tile = g.board.tile_mut(meadow).unwrap();
    tile.terrain = Terrain::Plains;
    tile.region = Some(God::Maya);
    tile.corpse = Some(Corpse {
        age: UNDEAD_AGE_DARK,
        hero: false,
    });
    // Whatever the board drew there, no militia stand on the two.
    g.militia
        .retain(|_, m| m.at != Some(rock) && m.at != Some(meadow));
    let mut events = Vec::new();
    g.raise_dead(&mut events);
    assert_eq!(g.mobs().len(), 1, "a meadow body waits for its grove");
    assert_eq!(g.mobs()[0].hex, rock);
    // In the land of a dark Maya the dead rise sooner, anywhere.
    g.pantheon.stages[God::Maya.index()] = 2;
    g.raise_dead(&mut events);
    assert!(g.mob_on(meadow).is_some());
}

#[test]
fn a_lone_undead_wears_the_militia_down_and_dawn_brings_a_man_back() {
    let (mut g, me, _) = duel(3);
    let town = town_by(&mut g, me);
    // Nobody near but the dead.
    g.place(me, Hex::new(-4, 4));
    undead_on(&mut g, Hex::new(1, 1), UNDEAD_HEALTH);
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    // Blows traded: it knocks a man down, the militia wound it.
    assert_eq!(g.mobs().len(), 1);
    assert_eq!(g.mobs()[0].hp, UNDEAD_HEALTH - 1);
    assert_eq!(g.militia(town), Some(MILITIA - 1));
    // Left alone it knocks the last man down; nobody strikes back.
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    assert_eq!(g.militia(town), Some(0));
    assert_eq!(g.mobs().len(), 1);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MilitiaFell { .. }))
    );
    // A man a dawn comes back.
    g.militia_at_dawn();
    assert_eq!(g.militia(town), Some(1));
}

#[test]
fn the_undead_lay_an_undefended_settlement_waste() {
    let (mut g, me, _) = duel(3);
    // A settlement with no champion next to it: nobody to strike instead.
    let town = g
        .militia
        .keys()
        .map(|&(x, y)| Hex::new(x, y))
        .find(|&t| {
            g.players()
                .all(|p| g.champion(p).unwrap().hex.unsigned_distance_to(t) > 1)
        })
        .unwrap();
    g.militia
        .insert((town.x(), town.y()), Militia { men: 0, at: None });
    g.claims.insert((town.x(), town.y()), me);
    undead_on(&mut g, town, UNDEAD_HEALTH);
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    assert_eq!(g.board.tile(town).unwrap().terrain, Terrain::Ruins);
    assert_eq!(g.militia(town), None);
    assert_eq!(g.owner(town), None);
    assert!(g.is_ruined_settlement(town));
}

#[test]
fn the_undead_walk_to_the_living_and_strike_them() {
    let (mut g, me, _) = duel(3);
    // Nobody's settlements near: only the champion draws them.
    g.militia.clear();
    let id = undead_on(&mut g, Hex::new(0, 3), UNDEAD_HEALTH);
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    let at = g.mobs().iter().find(|u| u.id == id).unwrap().hex;
    assert_eq!(at.unsigned_distance_to(Hex::new(0, 0)), 2);
    g.mobs.iter_mut().for_each(|u| u.hex = Hex::new(0, 1));
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MobStruck { target, .. } if *target == me))
    );
}

#[test]
fn a_champion_lays_the_undead_to_rest() {
    let (mut g, me, _) = duel(3);
    g.champ_mut(me).might = 5;
    let hex = Hex::new(0, 1);
    g.board.tile_mut(hex).unwrap().terrain = Terrain::Plains;
    let id = undead_on(&mut g, hex, UNDEAD_HEALTH);
    assert!(g.attackable(me).contains(&hex));
    g.apply(me, Intent::Move { to: hex }).unwrap();
    assert!(matches!(
        g.to_answer(me).map(|w| w.kind),
        Some(WindowKind::MobBattle { attacker, id: i }) if attacker == me && i == id
    ));
    let cards = burn_all(&mut g, me, "Искра");
    let events = g.apply(me, Intent::Burn { cards }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MobFell { by, .. } if *by == me))
    );
    assert!(g.mobs().is_empty());
    assert_eq!(g.standing(me), 1);
}

#[test]
fn the_militia_remember_what_is_done_near_them() {
    let (mut g, me, _) = duel(3);
    let town = g
        .militia
        .keys()
        .next()
        .map(|&(x, y)| Hex::new(x, y))
        .unwrap();
    // Laying the dead to rest by their gate wins them over. (The militia
    // stand aside: a champion and they never share a hex.)
    g.militia.get_mut(&(town.x(), town.y())).unwrap().at = None;
    g.place(me, town);
    g.board.tile_mut(town).unwrap().corpse = Some(Corpse::fresh());
    let rest = g.give(me, "Упокоить");
    g.apply(
        me,
        Intent::Play {
            card: rest,
            target: Target::Hex(town),
        },
    )
    .unwrap();
    assert_eq!(g.standing(me), 1);
    // Friends are healed in the settlement.
    g.standing[me.0 as usize] = FRIENDLY;
    g.champ_mut(me).hp = 1;
    let mut events = Vec::new();
    g.start_turn(me, &mut events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MilitiaHelped { .. }))
    );
    // The unwelcome cannot take it.
    g.standing[me.0 as usize] = HOSTILE;
    g.claims.clear();
    let mut events = Vec::new();
    g.claim(me, town, &mut events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MilitiaBarred { .. }))
    );
    assert_eq!(g.owner(town), None);
}

/// Tuning aid for §20.4, not a check: how soon and how many rise in bot
/// matches. `cargo test -p necromy-rules when_the_dead -- --ignored --nocapture`
#[test]
#[ignore]
fn when_the_dead_rise() {
    for seed in 1u64..=12 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
        });
        let mut first = None;
        let mut first_beast = None;
        let mut most = 0;
        for _ in 0..3000 {
            if g.winner().is_some() {
                break;
            }
            let p = g.awaiting()[0];
            let intent = crate::bot::choose(&g, p);
            let events = g.apply(p, intent).unwrap();
            if first.is_none()
                && events
                    .iter()
                    .any(|e| matches!(e, Event::MobAppeared { mob } if mob.is_undead()))
            {
                first = Some(g.round());
            }
            if first_beast.is_none()
                && events
                    .iter()
                    .any(|e| matches!(e, Event::MobAppeared { mob } if mob.is_beast()))
            {
                first_beast = Some(g.round());
            }
            most = most.max(g.mobs().len());
        }
        let corpses = g
            .log()
            .iter()
            .filter(|e| matches!(e, Event::CorpseAppeared { .. } | Event::ChampionFell { .. }))
            .count();
        let count = |f: fn(&Event) -> bool| g.log().iter().filter(|e| f(e)).count();
        eprintln!(
            "seed {seed}: first rise {first:?}, rounds {}, bodies {corpses}, most at once {most}, risen {}, felled by champions {}, by militia {}, ruined {}, struck champions {}",
            g.round(),
            count(|e| matches!(e, Event::MobAppeared { mob } if mob.is_undead())),
            count(|e| matches!(e, Event::MobFell { .. })),
            count(|e| matches!(e, Event::MilitiaStruck { .. })),
            count(|e| matches!(e, Event::SettlementRuined { .. })),
            count(|e| matches!(e, Event::MobStruck { .. })),
        );
        eprintln!(
            "    militia hit by undead {}, rebuilt {}, swaps {}, militia fought {}",
            count(|e| matches!(e, Event::UndeadHitMilitia { .. })),
            count(|e| matches!(e, Event::SettlementRebuilt { .. })),
            count(|e| matches!(e, Event::MilitiaSwapped { .. })),
            count(|e| matches!(e, Event::MilitiaAttacked { .. })),
        );
        eprintln!(
            "    first beast {first_beast:?}, beasts {}, mauled {}, left {}, militia hit {} (pursuers {}), guard hewed {}",
            count(|e| matches!(e, Event::MobAppeared { mob } if mob.is_beast())),
            count(|e| matches!(e, Event::BeastMauled { .. })),
            count(|e| matches!(e, Event::MobLeft { .. })),
            count(|e| matches!(e, Event::MilitiaHit { .. })),
            count(|e| matches!(
                e,
                Event::MilitiaHit {
                    why: MilitiaWhy::Pursuer { .. },
                    ..
                }
            )),
            count(|e| matches!(e, Event::GuardHewed { .. })),
        );
    }
}

/// A settlement next to `me` at (0,0), its militia at home, the ground plain.
fn town_by(g: &mut Game, me: PlayerId) -> Hex {
    let town = Hex::new(0, 1);
    // The only settlement that matters here: no neighbour's militia in reach.
    g.militia.clear();
    let tile = g.board.tile_mut(town).unwrap();
    tile.terrain = Terrain::Settlement;
    tile.region = Some(God::Ahamar);
    g.militia.insert(
        (town.x(), town.y()),
        Militia {
            men: MILITIA,
            at: Some(town),
        },
    );
    assert_eq!(g.champion(me).unwrap().hex, Hex::new(0, 0));
    town
}

#[test]
fn militia_let_the_unhated_through_by_trading_places() {
    let (mut g, me, _) = duel(3);
    let town = town_by(&mut g, me);
    assert!(g.lets_pass(me, town));
    // No fight on offer; a path ends there.
    assert!(!g.attackable(me).contains(&town));
    assert!(g.reachable(me).contains_key(&town));
    let events = g.apply(me, Intent::Move { to: town }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MilitiaSwapped { .. }))
    );
    assert_eq!(g.champion(me).unwrap().hex, town);
    assert_eq!(g.militia_unit(town).unwrap().at, Some(Hex::new(0, 0)));
    assert_eq!(g.owner(town), Some(me));
    // Once the champion walks on, the militia go home.
    g.place(me, Hex::new(1, 1));
    g.militia_go_home(&mut Vec::new());
    assert_eq!(g.militia_unit(town).unwrap().at, Some(town));
}

#[test]
fn militia_fight_those_they_hold_something_against() {
    let (mut g, me, _) = duel(3);
    let town = town_by(&mut g, me);
    g.standing[me.0 as usize] = -1;
    assert!(g.attackable(me).contains(&town));
    g.champ_mut(me).might = 6;
    let cards = burn_all(&mut g, me, "Искра");
    g.apply(me, Intent::Move { to: town }).unwrap();
    assert!(matches!(
        g.to_answer(me).map(|w| w.kind),
        Some(WindowKind::MilitiaBattle { attacker, home }) if attacker == me && home == town
    ));
    let events = g.apply(me, Intent::Burn { cards }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MilitiaFell { .. }))
    );
    assert_eq!(g.militia(town), Some(0));
    assert!(g.militia_at(town).is_none());
    // Attacking at their gate and cutting them down: far out of favour.
    assert!(g.standing(me) <= -3);
    // The champion stays where they stood.
    assert_eq!(g.champion(me).unwrap().hex, Hex::new(0, 0));
}

#[test]
fn undead_at_the_gate_wear_the_militia_down() {
    let (mut g, me, _) = duel(3);
    let town = town_by(&mut g, me);
    g.place(me, Hex::new(-4, 4));
    // Two at the gate: the militia cut one down (a man lost), the other
    // knocks a man down.
    undead_on(&mut g, Hex::new(1, 1), UNDEAD_HEALTH);
    undead_on(&mut g, Hex::new(-1, 2), UNDEAD_HEALTH);
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::UndeadHitMilitia { .. }))
    );
    assert_eq!(g.militia(town), Some(0));
    // A man comes back at dawn, not all of them.
    g.militia_at_dawn();
    assert_eq!(g.militia(town), Some(1));
}

#[test]
fn ruins_can_be_built_again() {
    let (mut g, me, _) = duel(3);
    let town = town_by(&mut g, me);
    g.ruin(town, &mut Vec::new());
    assert_eq!(
        g.apply(me, Intent::Rebuild),
        Err(RuleError::NotRuins),
        "only standing on them"
    );
    g.place(me, town);
    g.champ_mut(me).spirit_points = REBUILD_SPIRIT;
    let events = g.apply(me, Intent::Rebuild).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::SettlementRebuilt { .. }))
    );
    assert_eq!(g.board.tile(town).unwrap().terrain, Terrain::Settlement);
    assert!(!g.is_ruined_settlement(town));
    assert_eq!(g.militia(town), Some(1));
    assert_eq!(g.owner(town), Some(me));
    assert_eq!(g.standing(me), REBUILD_STANDING);
    assert_eq!(g.move_points(me), 0);
    assert_eq!(g.champion(me).unwrap().spirit_points, 0);
}

// ---- Beasts of Bhava's forests (§20.4) ----

fn beast_on(g: &mut Game, lair: Hex, hex: Hex) -> u32 {
    g.next_mob += 1;
    let id = g.next_mob;
    g.mobs.push(Mob {
        id,
        kind: MobKind::Beast { lair },
        hex,
        hp: BEAST_HEALTH,
    });
    id
}

#[test]
fn beasts_come_out_of_bhavas_woods_unless_he_is_light() {
    let (mut g, _, _) = duel(3);
    without(&mut g, &[Feature::Wilds]);
    g.time = TimeOfDay::Night;
    g.pantheon.stages[God::Bhava.index()] = 0;
    let mut events = Vec::new();
    g.beasts_at_night(&mut events);
    assert!(g.mobs().is_empty(), "none in the light");
    g.pantheon.stages[God::Bhava.index()] = 1;
    g.beasts_at_night(&mut events);
    let beast = g.mobs()[0];
    let MobKind::Beast { lair } = beast.kind else {
        panic!("a beast");
    };
    let tile = g.board.tile(lair).unwrap();
    assert_eq!(tile.region, Some(God::Bhava));
    assert!(matches!(tile.terrain, Terrain::Forest | Terrain::Grove));
    // No more than his stage allows.
    for _ in 0..10 {
        g.beasts_at_night(&mut events);
    }
    assert!(g.mobs().len() <= g.beasts_allowed());
}

#[test]
fn a_beast_keeps_to_its_land() {
    let (mut g, me, _) = duel(3);
    g.militia.clear();
    g.pantheon.stages[God::Bhava.index()] = 1;
    // Its lair two hexes off: the champion stands on its land.
    let id = beast_on(&mut g, Hex::new(0, 2), Hex::new(0, 2));
    let mut events = Vec::new();
    g.beast_phase(&mut events);
    assert_eq!(g.mobs()[0].hex.unsigned_distance_to(Hex::new(0, 0)), 1);
    let mut events = Vec::new();
    g.beast_phase(&mut events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MobStruck { id: i, target } if *i == id && *target == me))
    );
    // Off its land nobody is hunted: it goes home.
    g.place(me, Hex::new(-3, 0));
    let mut events = Vec::new();
    g.beast_phase(&mut events);
    assert_eq!(g.mobs()[0].hex, Hex::new(0, 2));
}

#[test]
fn beasts_spare_bhavas_chosen_and_tear_the_dead_apart() {
    let (mut g, me, _) = duel(3);
    g.militia.clear();
    g.pantheon.stages[God::Bhava.index()] = 1;
    beast_on(&mut g, Hex::new(0, 1), Hex::new(0, 1));
    g.favor[me.0 as usize][God::Bhava.index()] = CHOSEN;
    let mut events = Vec::new();
    g.beast_phase(&mut events);
    assert!(!events.iter().any(|e| matches!(e, Event::MobStruck { .. })));
    // An undead that wanders onto its land is torn apart.
    undead_on(&mut g, Hex::new(1, 1), UNDEAD_HEALTH);
    let mut events = Vec::new();
    g.beast_phase(&mut events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::BeastMauled { .. }))
    );
    assert_eq!(g.mobs().iter().filter(|m| m.is_undead()).count(), 0);
}

#[test]
fn beasts_go_back_into_the_woods_when_bhava_turns_light() {
    let (mut g, _, _) = duel(3);
    beast_on(&mut g, Hex::new(0, 3), Hex::new(0, 3));
    g.pantheon.stages[God::Bhava.index()] = 0;
    let mut events = Vec::new();
    g.beast_phase(&mut events);
    assert!(g.mobs().is_empty());
    assert!(events.iter().any(|e| matches!(e, Event::MobLeft { .. })));
}

#[test]
fn a_champion_hunts_a_beast_down() {
    let (mut g, me, _) = duel(3);
    g.champ_mut(me).might = 7;
    let hex = Hex::new(0, 1);
    g.board.tile_mut(hex).unwrap().terrain = Terrain::Plains;
    let id = beast_on(&mut g, hex, hex);
    g.apply(me, Intent::Move { to: hex }).unwrap();
    assert!(matches!(
        g.to_answer(me).map(|w| w.kind),
        Some(WindowKind::MobBattle { attacker, id: i }) if attacker == me && i == id
    ));
    // It throws its own three dice.
    assert_eq!(g.mob_dice(id), BEAST_DICE);
    let cards = burn_all(&mut g, me, "Искра");
    let events = g.apply(me, Intent::Burn { cards }).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MobFell { by, .. } if *by == me))
    );
    // Bhava's beasts leave the militia's view of the hunter alone.
    assert_eq!(g.standing(me), 0);
}

#[test]
fn the_guard_hews_down_the_undead_on_its_way() {
    let (mut g, _, foe) = duel(3);
    let mut ev = Vec::new();
    g.add_threat(foe, GUARD_THRESHOLD as i8 + 1, &mut ev);
    g.place(foe, Hex::new(5, -5));
    let start = Hex::new(-3, 0);
    g.board.tile_mut(start).unwrap().terrain = Terrain::Plains;
    g.guard = Some(Guard {
        hex: start,
        target: foe,
        hp: GUARD_HEALTH,
    });
    // Where it gets to, and a hex beside it off its path.
    let mut dry = g.clone();
    dry.guard_phase(&mut ev);
    let there = dry.guard().unwrap().hex;
    let goal = Hex::new(5, -5);
    let beside = there
        .all_neighbors()
        .into_iter()
        .find(|&h| {
            h.unsigned_distance_to(goal) == there.unsigned_distance_to(goal)
                && g.champion_at(h).is_none()
        })
        .unwrap();
    let undead = undead_on(&mut g, beside, UNDEAD_HEALTH);
    let mut events = Vec::new();
    g.guard_phase(&mut events);
    assert_eq!(g.guard().unwrap().hex, there);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::GuardHewed { undead: u, .. } if *u == undead))
    );
    assert!(g.mobs().is_empty());
}

#[test]
fn the_militia_wound_a_beast_at_their_gate() {
    let (mut g, me, _) = duel(3);
    let town = town_by(&mut g, me);
    let (_, _, foe) = (0, 0, g.order()[1]);
    g.place(me, Hex::new(0, -4));
    g.place(foe, Hex::new(-4, 4));
    let at = Hex::new(1, 1);
    assert_eq!(at.unsigned_distance_to(town), 1);
    let beast = beast_on(&mut g, at, at);
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    assert!(events.iter().any(
        |e| matches!(e, Event::MobHurt { id, hp, .. } if *id == beast && *hp == BEAST_HEALTH - 1)
    ));
}

#[test]
fn the_militia_strike_the_loud_but_not_their_friends() {
    let (mut g, me, _) = duel(3);
    let town = town_by(&mut g, me);
    g.champ_mut(me).hp = 3;
    let loud = g.guard_threshold();
    g.threat[me.0 as usize] = loud;
    assert_eq!(g.militia_target(town), Some((me, MilitiaWhy::Loud)));
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::MilitiaHit { player, why: MilitiaWhy::Loud, .. } if *player == me
    )));
    assert_eq!(g.champion(me).unwrap().hp, 2);
    // Never the last health.
    g.champ_mut(me).hp = 1;
    g.mob_phase(&mut Vec::new());
    assert_eq!(g.champion(me).unwrap().hp, 1);
    // Their friends may be as loud as they like; so may the quiet.
    g.standing[me.0 as usize] = FRIENDLY;
    assert_eq!(g.militia_target(town), None);
    g.standing[me.0 as usize] = 0;
    g.threat[me.0 as usize] = loud - 1;
    assert_eq!(g.militia_target(town), None);
}

#[test]
fn the_militia_strike_whoever_goes_after_their_friend() {
    let (mut g, me, foe) = duel(3);
    let town = town_by(&mut g, me);
    g.place(foe, Hex::new(1, 0));
    assert_eq!(Hex::new(1, 0).unsigned_distance_to(town), 1);
    g.standing[me.0 as usize] = FRIENDLY;
    g.note_pursuit(foe, me);
    assert_eq!(g.pursuer(me), Some(foe));
    let mut events = Vec::new();
    g.mob_phase(&mut events);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::MilitiaHit { player, why: MilitiaWhy::Pursuer { friend }, .. }
            if *player == foe && *friend == me
    )));
    // They hold it against them for a round after.
    g.round += PURSUIT_ROUNDS + 1;
    assert_eq!(g.pursuer(me), None);
    assert_eq!(g.militia_target(town), None);
    // A battle marks its attacker as the defender's pursuer.
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert_eq!(g.pursuer(foe), Some(me));
}

// ---- The world as players make it (§21.1) ----

#[test]
fn nobody_walks_into_the_mist() {
    let (mut g, me, _) = duel(4);
    let ahead = Hex::new(1, 0);
    let mut events = Vec::new();
    assert!(g.veil(ahead, &mut events));
    assert!(matches!(
        events.as_slice(),
        [Event::TerrainChanged {
            terrain: Terrain::Mist,
            ..
        }]
    ));
    assert_eq!(
        g.apply(me, Intent::Move { to: ahead }),
        Err(RuleError::OffBoard)
    );
    assert!(!g.reachable(me).contains_key(&ahead));
    // Nothing is laid there either.
    for _ in 0..50 {
        g.spawn_corpse(&mut events);
    }
    assert!(g.board().tile(ahead).unwrap().corpse.is_none());
    // Back out of the mist, the way is open again.
    assert!(g.unveil(ahead, &mut events));
    g.apply(me, Intent::Move { to: ahead }).unwrap();
}

#[test]
fn the_mist_never_takes_a_champion_or_a_temple() {
    let (mut g, me, _) = duel(2);
    let mut events = Vec::new();
    let at = g.champion(me).unwrap().hex;
    assert!(!g.veil(at, &mut events));
    assert!(!g.veil(g.board().temple_of(God::Zaga), &mut events));
    assert!(!g.veil(Hex::ZERO, &mut events));
    assert!(events.is_empty());
}

#[test]
fn raised_land_can_be_walked_at_once() {
    let (mut g, me, _) = duel(2);
    // Put `me` on the rim of the board, facing the void.
    let rim = g
        .board()
        .land()
        .map(|(h, _)| h)
        .find(|&h| {
            h.ulength() == g.board().extent()
                && g.champion_at(h).is_none()
                && !g.mob_at(h)
                && h.all_neighbors()
                    .iter()
                    .any(|&n| g.board().tile(n).is_none())
        })
        .unwrap();
    g.place(me, rim);
    let beyond = rim
        .all_neighbors()
        .into_iter()
        .find(|&n| g.board().tile(n).is_none())
        .unwrap();
    let mut events = Vec::new();
    assert!(g.raise_land(beyond, Terrain::Plains, &mut events));
    assert!(matches!(events.as_slice(), [Event::LandRaised { .. }]));
    assert_eq!(g.board().extent(), rim.ulength().max(beyond.ulength()));
    g.apply(me, Intent::Move { to: beyond }).unwrap();
    assert_eq!(g.champion(me).unwrap().hex, beyond);
}

/// `g`'s world without `lacking`.
fn without(g: &mut Game, lacking: &[Feature]) {
    g.world = World::of(Feature::ALL.into_iter().filter(|f| !lacking.contains(f)));
}

fn grew(events: &[Event]) -> Vec<Feature> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::WorldGrew { feature, .. } => Some(*feature),
            _ => None,
        })
        .collect()
}

#[test]
fn a_wish_raises_land_at_the_rim_of_the_gods_region() {
    let (mut g, me, _) = duel(3);
    let extent = g.board().extent();
    let events = g
        .wishes(
            me,
            Intent::Wish {
                god: God::Bhava,
                wish: Wish::one(Act::Rise { terrain: None }),
                said: None,
            },
        )
        .unwrap();
    let raised: Vec<Hex> = events
        .iter()
        .filter_map(|e| match e {
            Event::LandRaised { hex, .. } => Some(*hex),
            _ => None,
        })
        .collect();
    // Bhava loves growth: grade 3, power 4, five new hexes.
    assert_eq!(raised.len(), 5);
    for hex in raised {
        let tile = g.board().tile(hex).unwrap();
        assert_eq!(tile.region, Some(God::Bhava));
        assert!(tile.terrain.is_land());
    }
    assert!(g.board().extent() > extent);
}

#[test]
fn asking_for_the_dead_where_there_are_none_brings_bodies_in() {
    let (mut g, me, _) = duel(3);
    without(&mut g, &[Feature::Bodies, Feature::Groves, Feature::Undead]);
    for (h, _) in g.board().corpses().collect::<Vec<_>>() {
        g.board.tile_mut(h).unwrap().corpse = None;
    }
    // Trishna loves a feast of the dead: 3, enough for a new rule.
    assert_eq!(g.act_cost(Act::Dead), crate::game::wish::AWAKEN_COST);
    let events = g
        .wishes(
            me,
            Intent::Wish {
                god: God::Trishna,
                wish: Wish::one(Act::Dead),
                said: None,
            },
        )
        .unwrap();
    assert_eq!(grew(&events), vec![Feature::Bodies]);
    assert!(g.has(Feature::Bodies));
    assert!(g.board().corpses().count() > 0);
    // Now bodies are a thing of this world: the next asking is cheap.
    assert_eq!(g.act_cost(Act::Dead), 1);
}

#[test]
fn a_mechanic_comes_in_only_on_what_it_stands_on() {
    let (mut g, _, _) = duel(3);
    without(
        &mut g,
        &[Feature::Settlements, Feature::Militia, Feature::Ruins],
    );
    assert!(!g.can_awaken(Feature::Militia), "no settlements yet");
    assert!(!g.can_awaken(Feature::Ruins));
    assert!(g.can_awaken(Feature::Settlements));
    g.world.add(Feature::Settlements);
    assert!(g.can_awaken(Feature::Militia));
    assert!(!g.can_awaken(Feature::Settlements), "already there");
}

#[test]
fn one_mechanic_a_dusk_and_the_next_waits_for_the_next() {
    let (mut g, me, foe) = duel(3);
    without(
        &mut g,
        &[Feature::Settlements, Feature::Militia, Feature::Loot],
    );
    let mut ev = Vec::new();
    let at = g.champion(me).unwrap().hex;
    assert!(g.awaken(Some(me), God::Trishna, Feature::Settlements, at, &mut ev));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::TerrainChanged {
            terrain: Terrain::Settlement,
            ..
        }
    )));
    // Its first thing appears near whoever brought it.
    let settled = g
        .board()
        .land()
        .filter(|(_, t)| t.terrain == Terrain::Settlement)
        .map(|(h, _)| h)
        .min_by_key(|h| h.unsigned_distance_to(at))
        .unwrap();
    assert!(settled.unsigned_distance_to(at) <= 3);
    // A second the same dusk is set aside.
    let mut ev = Vec::new();
    assert!(!g.awaken(Some(foe), God::Ahamar, Feature::Militia, at, &mut ev));
    assert!(matches!(
        ev.as_slice(),
        [Event::AwakeningDeferred {
            feature: Feature::Militia,
            ..
        }]
    ));
    assert!(!g.has(Feature::Militia));
    // The next dusk it comes in, before any wish.
    g.round += 2;
    let mut ev = Vec::new();
    g.begin_dusk(&mut ev);
    assert_eq!(grew(&ev), vec![Feature::Militia]);
    assert!(g.militia(settled).is_some(), "the new settlement's militia");
}

#[test]
fn mayas_new_land_hides_in_her_fog_until_dusk() {
    let (mut g, me, foe) = duel(3);
    g.pantheon.stages[God::Maya.index()] = 1;
    let events = g
        .wishes(
            me,
            Intent::Wish {
                god: God::Maya,
                wish: Wish::one(Act::Rise { terrain: None }),
                said: None,
            },
        )
        .unwrap();
    let hex = events
        .iter()
        .find_map(|e| match e {
            Event::LandRaised { hex, .. } => Some(*hex),
            _ => None,
        })
        .unwrap();
    let mine = g.view_for(Some(me), 1);
    let theirs = g.view_for(Some(foe), 1);
    assert!(mine.board().tile(hex).unwrap().terrain.is_land());
    assert_eq!(theirs.board().tile(hex).unwrap().terrain, Terrain::Mist);
    let raised = events
        .iter()
        .find(|e| matches!(e, Event::LandRaised { .. }))
        .unwrap();
    assert!(Game::event_for(&theirs, Some(foe), raised).is_none());
    // Dusk lifts the fog.
    let mut ev = Vec::new();
    g.begin_dusk(&mut ev);
    let theirs = g.view_for(Some(foe), 1);
    assert!(theirs.board().tile(hex).unwrap().terrain.is_land());
}

#[test]
fn a_wish_without_style_lets_the_god_make_what_it_likes() {
    let (mut g, me, _) = duel(3);
    // A swamp near, so poison can come in.
    let at = g.champion(me).unwrap().hex;
    g.board.tile_mut(at + Hex::new(1, 0)).unwrap().terrain = Terrain::Swamp;
    without(&mut g, &[Feature::Poison]);
    let events = g
        .wishes(
            me,
            Intent::Wish {
                god: God::Zaga,
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
    assert_eq!(grew(&events), vec![Feature::Poison]);
}

#[test]
fn awakening_another_gods_domain_is_graded_lower() {
    let (mut g, me, _) = duel(3);
    without(
        &mut g,
        &[Feature::Settlements, Feature::Militia, Feature::Ruins],
    );
    let grade = |events: &[Event]| {
        events.iter().find_map(|e| match e {
            Event::WishGranted { grade, .. } => Some(*grade),
            _ => None,
        })
    };
    // Settlements are Trishna's; asked of Zaga, one less.
    let mut zaga = g.clone();
    let foreign = zaga
        .wishes(
            me,
            Intent::Wish {
                god: God::Zaga,
                wish: Wish::one(Act::Awaken {
                    feature: Some(Feature::Settlements),
                }),
                said: None,
            },
        )
        .unwrap();
    let own = g
        .wishes(
            me,
            Intent::Wish {
                god: God::Trishna,
                wish: Wish::one(Act::Awaken {
                    feature: Some(Feature::Settlements),
                }),
                said: None,
            },
        )
        .unwrap();
    assert_eq!(grade(&foreign).unwrap() + 1, grade(&own).unwrap());
}

/// Tuning aid, not a check: what the gods make of the world in bot games.
/// `cargo test -p necromy-rules creation_in_bot_games -- --ignored --nocapture`
#[test]
#[ignore]
fn creation_in_bot_games() {
    let (mut raised, mut veiled, mut grown, mut deferred) = (0, 0, 0, 0);
    for seed in 0..30 {
        let (mut g, _) = Game::new(Setup {
            seed,
            champions: God::ALL.to_vec(),
            mode: Default::default(),
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
            match e {
                Event::LandRaised { .. } => raised += 1,
                Event::TerrainChanged {
                    terrain: Terrain::Mist,
                    ..
                } => veiled += 1,
                Event::WorldGrew { .. } => grown += 1,
                Event::AwakeningDeferred { .. } => deferred += 1,
                _ => {}
            }
        }
        println!(
            "seed {seed}: round {}, extent {}, land {}",
            g.round(),
            g.board().extent(),
            g.board().land().count()
        );
    }
    println!("raised {raised}, veiled {veiled}, grown {grown}, deferred {deferred}");
}

fn creation(seed: u64) -> Game {
    Game::new(Setup {
        seed,
        champions: God::ALL.to_vec(),
        mode: Mode::Creation,
    })
    .0
}

#[test]
fn a_world_to_create_starts_small_with_one_mechanic() {
    for seed in 0..40 {
        let g = creation(seed);
        assert_eq!(g.board().extent(), crate::game::creation::CREATION_RADIUS);
        let features: Vec<Feature> = g.world().features().collect();
        assert_eq!(features.len(), 1, "seed {seed}: {features:?}");
        // The deck holds no card of a mechanic the world lacks.
        for id in g.slice() {
            assert!(
                id.def().needs().is_none_or(|f| g.has(f)),
                "seed {seed}: {} in a world without {:?}",
                id.def().name,
                id.def().needs()
            );
        }
        // Settlements only where the world has them.
        let settled = g
            .board()
            .land()
            .any(|(_, t)| t.terrain == Terrain::Settlement);
        assert_eq!(settled, g.has(Feature::Settlements), "seed {seed}");
        for god in God::ALL {
            assert_eq!(
                g.board().tile(g.board().temple_of(god)).unwrap().terrain,
                Terrain::Temple
            );
        }
    }
}

#[test]
fn a_mechanic_that_comes_in_brings_its_cards() {
    let mut g = creation(3);
    without(&mut g, &Feature::ALL);
    g.world.add(Feature::Bodies);
    let deck = g.deck_len();
    let mut ev = Vec::new();
    let at = g.board().start_of(God::Maya);
    // Poison needs a swamp.
    g.board.tile_mut(at).unwrap().terrain = Terrain::Swamp;
    assert!(g.awaken(None, God::Zaga, Feature::Poison, at, &mut ev));
    let grown = ev.iter().find_map(|e| match e {
        Event::DeckGrew { cards, .. } => Some(*cards),
        _ => None,
    });
    assert!(grown.is_some_and(|n| n >= 2));
    assert_eq!(g.deck_len(), deck + grown.unwrap() as usize);
    // Every poison dealt in comes with its cure.
    for id in g.slice() {
        if let Some(e) = crate::cards::poison_element(id.def()) {
            assert!(
                g.slice().iter().any(|a| crate::cards::cures(a.def(), e)),
                "{} without a cure",
                id.def().name
            );
        }
    }
}

#[test]
fn bots_play_a_world_being_created() {
    for seed in 0..30 {
        let mut g = creation(seed);
        for _ in 0..3000 {
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
            "seed {seed}: only reached round {}",
            g.round()
        );
    }
}

#[test]
fn the_hand_goes_through_once_a_turn_one_fewer_but_at_a_temple() {
    let (mut g, me, _) = duel(3);
    let a = g.give(me, "Бинт");
    let b = g.give(me, "Искра");
    let events = g.apply(me, Intent::Cycle { cards: vec![a, b] }).unwrap();
    assert!(matches!(
        events.iter().find(|e| matches!(e, Event::Cycled { .. })),
        Some(Event::Cycled {
            let_go: 2,
            drawn: 1,
            ..
        })
    ));
    assert_eq!(g.hand(me).len(), 1);
    assert!(!g.hand(me).contains(&a) && !g.hand(me).contains(&b));
    let c = g.hand(me)[0];
    assert_eq!(
        g.apply(me, Intent::Cycle { cards: vec![c] }),
        Err(RuleError::NoCycle),
        "once a turn"
    );
    // At a temple the gods give back as many as went.
    let (mut g, me, _) = duel(3);
    let temple = g.board().temple_of(God::Zaga);
    g.place(me, temple);
    let a = g.give(me, "Бинт");
    g.apply(me, Intent::Cycle { cards: vec![a] }).unwrap();
    assert_eq!(g.hand(me).len(), 1);
    assert_ne!(g.hand(me)[0], a);
}

#[test]
fn the_first_at_the_table_earns_style_once() {
    let (mut g, me, foe) = duel(3);
    let mut ev = Vec::new();
    g.first(me, Novelty::WonBattle, &mut ev);
    assert_eq!(g.style(me), FIRST_STYLE as u16);
    g.first(foe, Novelty::WonBattle, &mut ev);
    g.first(me, Novelty::WonBattle, &mut ev);
    assert_eq!(g.style(foe), 0, "only the first");
    assert_eq!(g.style(me), FIRST_STYLE as u16, "and only once");
    assert_eq!(g.firsts().count(), 1);
}

#[test]
fn battles_won_again_are_worth_less() {
    let (mut g, me, _) = duel(3);
    let worth: Vec<i16> = (0..6).map(|_| g.repeated(me, 2)).collect();
    assert_eq!(worth, vec![2, 2, 1, 1, 0, 0]);
}

#[test]
fn a_day_of_many_deeds_is_worth_more() {
    let (mut g, me, foe) = duel(3);
    nothing_new(&mut g);
    g.record_deed(me, Deed::Prayed);
    g.record_deed(me, Deed::Played(Element::Fire));
    g.record_deed(me, Deed::Claimed);
    // The same deed thrice is still one.
    for _ in 0..3 {
        g.record_deed(foe, Deed::Prayed);
    }
    let mut ev = Vec::new();
    g.judge_variety(&mut ev);
    assert_eq!(g.style(me), 1);
    assert_eq!(g.style(foe), 0);
}

/// Tuning aid: how bot matches end now that deeds win them.
/// `cargo test -p necromy-rules deeds_in_bot_games -- --ignored --nocapture`
#[test]
#[ignore]
fn deeds_in_bot_games() {
    for mode in [Mode::Full, Mode::Creation] {
        let mut wins: Vec<(GreatDeed, u32)> = Vec::new();
        let mut eves = 0;
        for seed in 0..40 {
            let (mut g, _) = Game::new(Setup {
                seed,
                champions: God::ALL.to_vec(),
                mode,
            });
            for _ in 0..6000 {
                if g.winner().is_some() {
                    break;
                }
                let p = g.awaiting()[0];
                let intent = crate::bot::choose(&g, p);
                g.apply(p, intent).unwrap();
            }
            eves += g
                .log()
                .iter()
                .filter(|e| matches!(e, Event::DeedEve { .. }))
                .count();
            if let Some((_, deed)) = g.winner() {
                wins.push((deed, g.round()));
            }
        }
        println!("{mode:?}: {} wins of 40, eves {eves}: {wins:?}", wins.len());
    }
}

// ---- Great Deeds (§21.7) ----

#[test]
fn everyone_is_offered_three_deeds_and_picks_one() {
    let (mut g, _) = Game::new(five());
    for p in g.players() {
        let offers = g.offers(p);
        assert_eq!(offers.len(), OFFERED.min(GreatDeed::ALL.len()));
        let mut unique = offers.to_vec();
        unique.dedup();
        assert_eq!(unique.len(), offers.len(), "no deed twice in a hand");
    }
    let me = g.order()[0];
    // Nothing waits on it: a turn may go before, but no deed, no win.
    assert!(g.free_to_act(me));
    assert!(g.choosing().contains(&me));
    let not_mine = GreatDeed::ALL
        .into_iter()
        .find(|d| !g.offers(me).contains(d));
    if let Some(d) = not_mine {
        assert_eq!(
            g.apply(me, Intent::ChooseDeed { deed: d }),
            Err(RuleError::InvalidDeed)
        );
    }
    let deed = g.offers(me)[0];
    let events = g.apply(me, Intent::ChooseDeed { deed }).unwrap();
    assert!(matches!(events.as_slice(), [Event::DeedChosen { .. }, ..]));
    assert_eq!(g.deed(me), Some(deed));
    assert!(!g.choosing().contains(&me));
    assert_eq!(
        g.apply(me, Intent::ChooseDeed { deed }),
        Err(RuleError::InvalidDeed),
        "once"
    );
}

/// `me` on an island of `size` land hexes around `centre`, the rest of the
/// board in the mist.
fn island(g: &mut Game, me: PlayerId, centre: Hex) {
    let keep: Vec<Hex> = centre.range(1).collect();
    let all: Vec<Hex> = g.board().tiles().map(|(h, _)| h).collect();
    for h in all {
        if !keep.contains(&h)
            && let Some(tile) = g.board.tile_mut(h)
        {
            tile.terrain = Terrain::Mist;
        }
    }
    for p in g.players().collect::<Vec<_>>() {
        if p != me {
            g.champ_mut(p).hex = Hex::new(100, 100);
        }
    }
    g.place(me, centre);
}

#[test]
fn an_island_waits_on_its_eve_and_is_done_at_dusk() {
    let (mut g, me, _) = duel(3);
    g.dusks = EARLIEST_EVE;
    g.chosen[me.0 as usize] = Some(GreatDeed::Island);
    let centre = Hex::new(0, 4);
    island(&mut g, me, centre);
    let town = centre + Hex::new(1, 0);
    g.board.tile_mut(town).unwrap().terrain = Terrain::Settlement;
    g.claims.insert((town.x(), town.y()), me);
    let checks = g.checks(me, GreatDeed::Island);
    assert!(checks.iter().all(Check::met), "{checks:?}");
    let mut ev = Vec::new();
    g.check_victory(&mut ev);
    assert!(matches!(ev.as_slice(), [Event::DeedEve { .. }]));
    assert!(g.on_eve(me));
    assert_eq!(g.winner(), None, "not before dusk");
    let mut ev = Vec::new();
    g.dusk_of_deeds(&mut ev);
    assert_eq!(g.winner(), Some((me, GreatDeed::Island)));
    assert!(ev.iter().any(|e| matches!(e, Event::Victory { .. })));
}

#[test]
fn an_eve_is_broken_when_a_step_fails() {
    let (mut g, me, _) = duel(3);
    g.dusks = EARLIEST_EVE;
    g.chosen[me.0 as usize] = Some(GreatDeed::Island);
    let centre = Hex::new(0, 4);
    island(&mut g, me, centre);
    let town = centre + Hex::new(1, 0);
    g.board.tile_mut(town).unwrap().terrain = Terrain::Settlement;
    g.claims.insert((town.x(), town.y()), me);
    let mut ev = Vec::new();
    g.check_victory(&mut ev);
    assert!(g.on_eve(me));
    // A rival takes the settlement before dusk.
    g.claims.remove(&(town.x(), town.y()));
    let mut ev = Vec::new();
    g.check_victory(&mut ev);
    assert!(matches!(ev.as_slice(), [Event::EveBroken { .. }]));
    g.dusk_of_deeds(&mut ev);
    assert_eq!(g.winner(), None);
}

#[test]
fn a_region_dissolves_but_its_temple_and_the_homes() {
    let (mut g, me, _) = duel(3);
    let own = g.champion(me).unwrap().god;
    let other = God::ALL.into_iter().find(|&g2| g2 != own).unwrap();
    // All of another god's land into the mist, but the temple and homes.
    let spots: Vec<Hex> = g
        .board()
        .tiles()
        .filter(|(_, t)| t.region == Some(other))
        .map(|(h, _)| h)
        .collect();
    for h in spots {
        let home = God::ALL.iter().any(|&o| g.board().start_of(o) == h);
        if h != g.board().temple_of(other) && !home {
            g.board.tile_mut(h).unwrap().terrain = Terrain::Mist;
        }
    }
    let [check] = g.checks(me, GreatDeed::DissolvedLand)[..] else {
        panic!("one check");
    };
    assert!(check.met(), "{check:?}");
    assert!(check.need >= crate::game::DISSOLVED as u16);
}

#[test]
fn a_champions_body_grows_into_the_grove_of_a_world_tree() {
    let (mut g, me, foe) = duel(2);
    let at = g.champion(foe).unwrap().hex;
    g.board.tile_mut(at).unwrap().terrain = Terrain::Plains;
    let mut ev = Vec::new();
    g.fall(foe, &mut ev);
    assert!(g.board().tile(at).unwrap().corpse.unwrap().hero);
    for _ in 0..GROVE_AGE {
        g.world_phase(&mut ev);
    }
    assert_eq!(g.board().tile(at).unwrap().terrain, Terrain::Grove);
    assert_eq!(g.hero_grove(), Some(at));
    let [grove, ..] = g.checks(me, GreatDeed::WorldTree)[..] else {
        panic!("checks");
    };
    assert!(grove.met());
}

// ---- The storyteller in a world being made (§21.6) ----

#[test]
fn one_lagging_is_told_to_bring_in_what_their_deed_needs() {
    let (mut g, me, _) = duel(3);
    without(
        &mut g,
        &[Feature::Settlements, Feature::Militia, Feature::Ruins],
    );
    g.chosen[me.0 as usize] = Some(GreatDeed::Island);
    let mut ev = Vec::new();
    g.opportunity(me, &mut ev);
    let line = *g.lines_of(me).next().expect("a line");
    assert_eq!(line.goal, Goal::Bring(Feature::Settlements));
    assert_eq!(line.god, God::Trishna);
    // Whoever brings it in, the line is done.
    let at = g.champion(me).unwrap().hex;
    g.awaken(None, God::Trishna, Feature::Settlements, at, &mut ev);
    g.check_lines(&mut ev);
    assert!(ev.iter().any(|e| matches!(e, Event::LineDone { .. })));
}

#[test]
fn a_deed_on_its_eve_sets_everyone_else_to_break_it() {
    let (mut g, me, foe) = duel(3);
    g.chosen[me.0 as usize] = Some(GreatDeed::Island);
    g.eves[me.0 as usize] = Some(g.dusks);
    let mut ev = Vec::new();
    g.storyteller(&mut ev);
    let line = g
        .lines_of(foe)
        .find(|l| l.goal == Goal::Thwart(me))
        .copied()
        .expect("told to break it");
    // By the god whose element quenches the patron's: Maya's water, Zaga's earth.
    assert_eq!(line.god, God::Zaga);
    // The eve broken, the line is done.
    g.eves[me.0 as usize] = None;
    let mut ev = Vec::new();
    g.check_lines(&mut ev);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::LineDone { line } if line.owner == foe))
    );
}

#[test]
fn a_still_board_lets_the_darkest_god_bring_something_in() {
    let (mut g, _, _) = duel(3);
    without(&mut g, &[Feature::Undead, Feature::Ruins, Feature::Stealth]);
    g.pantheon.stages = [0, 0, 0, 0, 2];
    g.last_fight = 0;
    g.round = 10;
    let mut ev = Vec::new();
    g.storyteller(&mut ev);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::WorldStirred {
            stir: WorldStir::Awakening
        }
    )));
    let grown = grew(&ev);
    assert_eq!(grown.len(), 1);
    assert_eq!(grown[0].domain(), God::Maya);
}

// ---- Burdens (§21.8) ----

#[test]
fn a_body_is_carried_a_step_shorter_and_laid_down_again() {
    let (mut g, me, _) = duel(3);
    let here = g.champion(me).unwrap().hex;
    g.board.tile_mut(here).unwrap().corpse = Some(Corpse::fresh());
    g.apply(me, Intent::Take).unwrap();
    assert_eq!(g.cargo(me), Some(Cargo::Body { hero: false }));
    assert!(g.board().tile(here).unwrap().corpse.is_none());
    assert_eq!(
        g.apply(me, Intent::Take),
        Err(RuleError::NoCargo),
        "one at a time"
    );
    // A step shorter next turn.
    g.end_turn_and_settle();
    while g.phase(me) != &Phase::Acting {
        g.end_turn_and_settle();
    }
    assert_eq!(g.move_points(me), MOVE_POINTS - 1);
    let there = g.champion(me).unwrap().hex;
    g.apply(me, Intent::Lay).unwrap();
    assert!(g.cargo(me).is_none());
    assert!(g.board().tile(there).unwrap().corpse.is_some());
}

#[test]
fn the_winner_takes_the_losers_burden() {
    let (mut g, me, foe) = duel(3);
    g.champ_mut(foe).cargo = Some(Cargo::Body { hero: true });
    let mut ev = Vec::new();
    g.seize_cargo(me, foe, &mut ev);
    assert_eq!(g.cargo(me), Some(Cargo::Body { hero: true }));
    assert!(g.cargo(foe).is_none());
    // The fallen drop theirs where they fall.
    g.champ_mut(foe).cargo = Some(Cargo::Body { hero: false });
    let at = g.champion(foe).unwrap().hex;
    g.fall(foe, &mut ev);
    assert!(g.cargo(foe).is_none());
    assert!(g.board().tile(at).unwrap().corpse.is_some() || !g.loads().is_empty());
}

// ---- Buildings and cities (§21.8) ----

/// `me` holds a settlement on `hex` and stands on it with Spirit to spend.
fn settled(g: &mut Game, me: PlayerId, hex: Hex) {
    g.board.tile_mut(hex).unwrap().terrain = Terrain::Settlement;
    g.claims.insert((hex.x(), hex.y()), me);
    g.militia.remove(&(hex.x(), hex.y()));
    g.place(me, hex);
    g.champ_mut(me).spirit_points = 9;
    g.champ_mut(me).spirit = 9;
}

#[test]
fn a_building_rises_on_a_settlement_of_ones_own() {
    let (mut g, me, _) = duel(3);
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    g.apply(
        me,
        Intent::Build {
            building: Building::Tavern,
        },
    )
    .unwrap();
    assert_eq!(g.building(town), Some(Building::Tavern));
    assert_eq!(g.champion(me).unwrap().spirit_points, 9 - BUILD_SPIRIT);
    assert_eq!(g.move_points(me), 0, "the walk ends");
    assert_eq!(
        g.apply(
            me,
            Intent::Build {
                building: Building::Forge
            }
        ),
        Err(RuleError::CannotBuild),
        "one a settlement"
    );
    // At the tavern the hand goes through at no loss.
    let a = g.give(me, "Бинт");
    g.apply(me, Intent::Cycle { cards: vec![a] }).unwrap();
    assert_eq!(g.hand(me).len(), 1);
}

#[test]
fn a_shrine_of_two_takes_prayers_for_both() {
    let (mut g, me, _) = duel(3);
    // A settlement where two lands meet.
    let (town, gods) = g
        .board()
        .land()
        .filter(|(h, t)| t.region.is_some() && h.ulength() >= 2 && g.champion_at(*h).is_none())
        .find_map(|(h, t)| {
            let own = t.region?;
            let other = h
                .all_neighbors()
                .iter()
                .filter_map(|&n| g.board().tile(n).and_then(|t| t.region))
                .find(|&o| o != own)?;
            Some((h, [own, other]))
        })
        .unwrap();
    settled(&mut g, me, town);
    let shrine = Building::Shrine(gods);
    assert!(g.may_build(me).contains(&shrine));
    g.apply(me, Intent::Build { building: shrine }).unwrap();
    let before = gods.map(|god| g.favor(me, god));
    g.apply(me, Intent::EndTurn).unwrap();
    for (i, god) in gods.into_iter().enumerate() {
        assert_eq!(g.favor(me, god), before[i] + 1, "{god:?}");
    }
}

#[test]
fn a_wall_keeps_three_men() {
    let (mut g, me, _) = duel(3);
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    g.militia.insert(
        (town.x(), town.y()),
        Militia {
            men: 2,
            at: Some(town),
        },
    );
    g.apply(
        me,
        Intent::Build {
            building: Building::Wall,
        },
    )
    .unwrap();
    g.militia_at_dawn();
    assert_eq!(g.militia(town), Some(WALLED_MILITIA));
}

#[test]
fn quarters_grow_a_city_and_the_city_is_a_deed() {
    let (mut g, me, _) = duel(3);
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    let quarters = g.quarters(me);
    assert!(!quarters.is_empty());
    g.apply(me, Intent::Quarter { hex: quarters[0] }).unwrap();
    assert!(g.city_of(town).len() >= 2);
    assert_eq!(g.owner(quarters[0]), Some(me));
    // A city of seven, all ours, with a tavern, a forge and a shrine.
    for h in town.all_neighbors() {
        if let Some(tile) = g.board.tile_mut(h) {
            tile.terrain = Terrain::Settlement;
        }
        g.claims.insert((h.x(), h.y()), me);
    }
    let [a, b, c, ..] = town.all_neighbors();
    g.buildings.insert((a.x(), a.y()), Building::Tavern);
    g.buildings.insert((b.x(), b.y()), Building::Forge);
    g.buildings
        .insert((c.x(), c.y()), Building::Shrine([God::Zaga, God::Zaga]));
    g.chosen[me.0 as usize] = Some(GreatDeed::City);
    let checks = g.checks(me, GreatDeed::City);
    assert!(checks.iter().all(Check::met), "{checks:?}");
    // One quarter of another's breaks it.
    g.claims.insert((a.x(), a.y()), PlayerId(4));
    assert!(!g.checks(me, GreatDeed::City).iter().all(Check::met));
}

#[test]
fn a_reconciled_pair_keeps_the_peace_two_dusks() {
    let (mut g, me, _) = duel(3);
    g.dusks = EARLIEST_EVE;
    g.chosen[me.0 as usize] = Some(GreatDeed::Reconciliation);
    // Zaga and Maya: earth quenches water.
    g.pantheon.stages[God::Zaga.index()] = 0;
    g.pantheon.stages[God::Maya.index()] = 0;
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    g.buildings.insert(
        (town.x(), town.y()),
        Building::Shrine([God::Zaga, God::Maya]),
    );
    let checks = g.checks(me, GreatDeed::Reconciliation);
    assert!(
        checks
            .iter()
            .filter(|c| c.kind != CheckKind::Dusks)
            .all(Check::met)
    );
    let mut ev = Vec::new();
    g.dusk_of_deeds(&mut ev);
    g.dusk_of_deeds(&mut ev);
    assert!(g.on_eve(me));
    g.dusk_of_deeds(&mut ev);
    assert_eq!(g.winner(), Some((me, GreatDeed::Reconciliation)));
}

// ---- Companions (§21.8) ----

/// An undead next to `me`, and Spirit to spare.
fn undead_beside(g: &mut Game, me: PlayerId) -> u32 {
    let here = g.champion(me).unwrap().hex;
    let spot = here
        .all_neighbors()
        .into_iter()
        .find(|&h| g.board.contains(h) && g.champion_at(h).is_none() && !g.mob_at(h))
        .unwrap();
    g.champ_mut(me).spirit_points = 5;
    undead_on(g, spot, 2)
}

#[test]
fn the_undead_join_a_legion_only_where_the_world_has_one() {
    let (mut g, me, _) = duel(3);
    without(&mut g, &[Feature::Legion]);
    let id = undead_beside(&mut g, me);
    assert_eq!(
        g.apply(me, Intent::Recruit { mob: id }),
        Err(RuleError::CannotRecruit)
    );
    g.world.add(Feature::Legion);
    let before = g.dice_for(me, false);
    g.apply(me, Intent::Recruit { mob: id }).unwrap();
    assert_eq!(g.companions(me), &[Companion::Undead]);
    assert!(g.mobs().iter().all(|m| m.id != id));
    assert_eq!(g.champion(me).unwrap().spirit_points, 5 - ENLIST_SPIRIT);
    assert_eq!(g.dice_for(me, false), before + 1);
}

#[test]
fn a_beast_is_tamed_for_spirit() {
    let (mut g, me, _) = duel(3);
    let id = undead_beside(&mut g, me);
    let lair = g.mobs().iter().find(|m| m.id == id).unwrap().hex;
    g.mobs.iter_mut().find(|m| m.id == id).unwrap().kind = MobKind::Beast { lair };
    g.apply(me, Intent::Recruit { mob: id }).unwrap();
    assert!(matches!(g.companions(me), [Companion::Beast(_)]));
    assert_eq!(g.champion(me).unwrap().spirit_points, 5 - TAME_SPIRIT);
}

#[test]
fn companions_go_to_the_winner_and_scatter_on_a_fall() {
    let (mut g, me, foe) = duel(3);
    g.champ_mut(foe).companions = vec![Companion::Beast(Element::Wood), Companion::Undead];
    let mut ev = Vec::new();
    g.seize_companion(me, foe, &mut ev);
    assert_eq!(g.companions(me), &[Companion::Undead]);
    assert_eq!(g.companions(foe), &[Companion::Beast(Element::Wood)]);
    // The fallen lose the rest; an undead rises where they fell.
    let at = g.champion(me).unwrap().hex;
    let undead = g.mobs().iter().filter(|m| m.is_undead()).count();
    g.fall(me, &mut ev);
    assert!(g.companions(me).is_empty());
    assert_eq!(
        g.mobs().iter().filter(|m| m.is_undead()).count(),
        undead + 1
    );
    assert!(g.mobs().iter().any(|m| m.hex == at));
}

#[test]
fn a_legion_of_five_is_a_deed_and_a_god_can_thin_it() {
    let (mut g, me, foe) = duel(3);
    g.dusks = EARLIEST_EVE;
    g.chosen[me.0 as usize] = Some(GreatDeed::Legion);
    g.champ_mut(me).companions = vec![Companion::Undead; LEGION];
    let mut ev = Vec::new();
    g.dusk_of_deeds(&mut ev);
    g.dusk_of_deeds(&mut ev);
    assert!(g.on_eve(me));
    // A god's hand takes one away before the next dusk.
    g.grant_act(
        foe,
        God::Ahamar,
        Act::Weaken { target: me },
        2,
        None,
        &mut ev,
    );
    assert_eq!(g.companions(me).len(), LEGION - 1);
    g.dusk_of_deeds(&mut ev);
    assert_eq!(g.winner(), None);
}

// ---- Rivers and lakes (§21.8) ----

/// Every check of `deed` met but the dusks it must hold.
fn but_dusks(g: &Game, p: PlayerId, deed: GreatDeed) -> bool {
    g.checks(p, deed)
        .iter()
        .filter(|c| c.kind != CheckKind::Dusks)
        .all(Check::met)
}

fn set_terrain(g: &mut Game, hex: Hex, terrain: Terrain) {
    g.board.tile_mut(hex).unwrap().terrain = terrain;
}

#[test]
fn crossing_a_river_ends_the_walk_and_along_it_is_easy() {
    let (mut g, me, _) = duel(4);
    set_terrain(&mut g, Hex::new(1, 0), Terrain::River);
    set_terrain(&mut g, Hex::new(2, 0), Terrain::River);
    assert_eq!(g.step_cost(me, Hex::new(1, 0)), Ok(MOVE_POINTS));
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert_eq!(g.move_points(me), 0);
    g.turns[me.0 as usize].move_points = 2;
    assert_eq!(g.step_cost(me, Hex::new(2, 0)), Ok(1));
    // The walk plans the same way: crossing takes what is left.
    g.place(me, Hex::new(0, 0));
    g.turns[me.0 as usize].move_points = MOVE_POINTS;
    let reach = g.reachable(me);
    assert_eq!(reach.get(&Hex::new(1, 0)), Some(&MOVE_POINTS));
}

#[test]
fn piranhas_bite_but_never_the_last_health() {
    let (mut g, me, _) = duel(4);
    set_terrain(&mut g, Hex::new(1, 0), Terrain::River);
    g.champ_mut(me).hp = 2;
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    assert_eq!(g.champion(me).unwrap().hp, 1);
    let mut ev = Vec::new();
    g.piranhas(me, Hex::new(1, 0), &mut ev);
    assert_eq!(g.champion(me).unwrap().hp, 1);
    without(&mut g, &[Feature::Piranhas]);
    g.champ_mut(me).hp = 3;
    g.piranhas(me, Hex::new(1, 0), &mut ev);
    assert_eq!(g.champion(me).unwrap().hp, 3);
}

#[test]
fn a_lake_is_not_walked_and_does_not_lift() {
    let (mut g, me, _) = duel(4);
    set_terrain(&mut g, Hex::new(1, 0), Terrain::Lake);
    assert_eq!(g.step_cost(me, Hex::new(1, 0)), Err(RuleError::OffBoard));
    let mut ev = Vec::new();
    g.unveil_near(Hex::new(1, 0), 3, &mut ev);
    assert_eq!(
        g.board().tile(Hex::new(1, 0)).unwrap().terrain,
        Terrain::Lake
    );
}

#[test]
fn a_river_runs_from_the_mountains_to_the_rim_and_beyond() {
    let (mut g, me, _) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::River);
    let mut ev = Vec::new();
    let near = g.champion(me).unwrap().hex;
    let run = g.run_river(near, 20, &mut ev);
    assert!(run.len() >= RIVER, "ran {}", run.len());
    assert!(
        but_dusks(&g, me, GreatDeed::River),
        "{:?}",
        g.checks(me, GreatDeed::River)
    );
    // Past the old rim: the world grew with it.
    assert!(run.iter().any(|h| h.ulength() > crate::board::BOARD_RADIUS));
}

#[test]
fn the_waters_rise_towards_the_table() {
    let (mut g, me, foe) = duel(4);
    g.chosen[foe.0 as usize] = Some(GreatDeed::FloodedTable);
    g.place(me, Hex::new(0, -5));
    g.place(foe, Hex::new(2, 0));
    let mut ev = Vec::new();
    let flooded = g.flood(Hex::new(2, 0), 8, &mut ev);
    assert_eq!(flooded, 8);
    assert_eq!(g.table_flooded(), 7);
    assert!(but_dusks(&g, foe, GreatDeed::FloodedTable));
    // Zaga's mountains take some of it back.
    g.place(foe, Hex::new(1, -1));
    g.grant_land(God::Zaga, Hex::new(1, -1), 3, &mut ev);
    assert!(g.table_flooded() < 7);
}

#[test]
fn a_river_through_the_woods_is_the_amazon() {
    let (mut g, me, _) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::Amazon);
    for hex in Hex::new(0, -4).range(2) {
        set_terrain(&mut g, hex, Terrain::Forest);
    }
    for q in [-5, -4, -3] {
        set_terrain(&mut g, Hex::new(0, q), Terrain::River);
    }
    assert!(but_dusks(&g, me, GreatDeed::Amazon));
    without(&mut g, &[Feature::Piranhas]);
    assert!(!but_dusks(&g, me, GreatDeed::Amazon));
}

// ---- Roads (§21.8) ----

#[test]
fn a_road_makes_any_step_one_and_bridges_a_river() {
    let (mut g, me, _) = duel(4);
    set_terrain(&mut g, Hex::new(1, 0), Terrain::Mountain);
    set_terrain(&mut g, Hex::new(2, 0), Terrain::River);
    assert_eq!(g.step_cost(me, Hex::new(1, 0)), Ok(2));
    g.champ_mut(me).spirit_points = 3;
    g.apply(me, Intent::Pave).unwrap();
    assert!(g.road(Hex::new(0, 0)));
    let mut ev = Vec::new();
    g.lay_road(Hex::new(1, 0), &mut ev);
    g.lay_road(Hex::new(2, 0), &mut ev);
    assert_eq!(g.step_cost(me, Hex::new(1, 0)), Ok(1));
    g.apply(me, Intent::Move { to: Hex::new(1, 0) }).unwrap();
    // Over the bridge: no crossing to make.
    assert_eq!(g.step_cost(me, Hex::new(2, 0)), Ok(1));
}

#[test]
fn nobody_hides_on_a_road_and_the_mist_takes_it() {
    let (mut g, me, _) = duel(4);
    let mut ev = Vec::new();
    let here = g.champion(me).unwrap().hex;
    g.lay_road(here, &mut ev);
    g.hide(me, &mut ev);
    assert!(!g.is_hidden(me));
    g.lay_road(Hex::new(1, 1), &mut ev);
    g.veil(Hex::new(1, 1), &mut ev);
    assert!(!g.road(Hex::new(1, 1)));
}

#[test]
fn the_registers_road_runs_from_the_table_to_the_temples() {
    // Not a duel: it paints the line to Bhava's temple as plains.
    let (mut g, _) = started(five());
    let me = g.order()[0];
    g.chosen[me.0 as usize] = Some(GreatDeed::Roads);
    assert_eq!(g.temples_linked(), 0);
    let mut ev = Vec::new();
    for god in God::ALL {
        let temple = g.board().temple_of(god);
        g.run_road(temple, 20, &mut ev);
    }
    assert_eq!(g.temples_linked(), 5);
    assert!(g.checks(me, GreatDeed::Roads).iter().all(Check::met));
}

// ---- Fires (§21.8) ----

#[test]
fn a_fire_burns_to_ash_and_catches_on_the_woods() {
    let (mut g, me, _) = duel(4);
    for hex in Hex::new(0, -3).range(1) {
        set_terrain(&mut g, hex, Terrain::Forest);
    }
    let mut ev = Vec::new();
    g.set_fire(Hex::new(0, -3), Some(me), &mut ev);
    for _ in 0..4 {
        g.fire_phase(&mut ev);
    }
    assert_eq!(
        g.board().tile(Hex::new(0, -3)).unwrap().terrain,
        Terrain::Ash
    );
    let ash = Hex::new(0, -3)
        .all_neighbors()
        .iter()
        .filter(|&&h| g.board().tile(h).unwrap().terrain == Terrain::Ash)
        .count();
    assert!(ash > 0, "the fire caught on nothing");
    let region = g.board().tile(Hex::new(0, -3)).unwrap().region.unwrap();
    assert!(g.burnt_by(me, region));
    // Plains do not burn: the fire stops at them.
    assert!(
        g.fires()
            .all(|(h, _)| g.board().tile(h).unwrap().terrain.burns())
    );
}

#[test]
fn a_champion_kindles_and_douses_and_walking_in_burns() {
    let (mut g, me, _) = duel(4);
    set_terrain(&mut g, Hex::new(1, 0), Terrain::Forest);
    g.champ_mut(me).spirit_points = 3;
    g.apply(
        me,
        Intent::Kindle {
            hex: Hex::new(1, 0),
        },
    )
    .unwrap();
    assert!(g.fire(Hex::new(1, 0)).is_some());
    g.champ_mut(me).hp = 3;
    let mut ev = Vec::new();
    g.scorch(me, Hex::new(1, 0), &mut ev);
    assert_eq!(g.champion(me).unwrap().hp, 2);
    g.apply(
        me,
        Intent::Douse {
            hex: Hex::new(1, 0),
        },
    )
    .unwrap();
    assert!(g.fire(Hex::new(1, 0)).is_none());
    assert_eq!(g.champion(me).unwrap().spirit_points, 1);
}

#[test]
fn a_great_fire_is_four_regions_and_a_fire_still_burning() {
    let (mut g, me, _) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::GreatFire);
    g.burnt[me.0 as usize] = 0b01111;
    assert!(!g.checks(me, GreatDeed::GreatFire).iter().all(Check::met));
    set_terrain(&mut g, Hex::new(1, 1), Terrain::Forest);
    let mut ev = Vec::new();
    g.set_fire(Hex::new(1, 1), Some(me), &mut ev);
    assert!(g.checks(me, GreatDeed::GreatFire).iter().all(Check::met));
    // Water puts it out.
    g.water(Hex::new(1, 1), Terrain::Lake, &mut ev);
    assert!(g.fire(Hex::new(1, 1)).is_none());
    assert!(!g.checks(me, GreatDeed::GreatFire).iter().all(Check::met));
}

// ---- Fields and the feast (§21.8) ----

#[test]
fn a_field_is_sown_bears_food_and_the_food_is_stored() {
    let (mut g, me, _) = duel(4);
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    let field = Hex::new(0, 1);
    set_terrain(&mut g, field, Terrain::Plains);
    g.place(me, field);
    g.apply(me, Intent::Sow).unwrap();
    assert_eq!(g.board().tile(field).unwrap().terrain, Terrain::Fields);
    assert_eq!(g.fields_of(me), 1);
    let mut ev = Vec::new();
    g.harvest(&mut ev);
    g.harvest(&mut ev);
    assert_eq!(g.loads().iter().filter(|(h, _)| *h == field).count(), 1);
    g.apply(me, Intent::Take).unwrap();
    assert_eq!(g.cargo(me), Some(Cargo::Food));
    g.place(me, town);
    g.drop_cargo(me, town, &mut ev);
    assert_eq!(g.food_at(town), 1);
}

#[test]
fn a_feast_with_guests_is_the_last_step_of_the_deed() {
    let (mut g, me, foe) = duel(2);
    g.dusks = EARLIEST_EVE;
    g.chosen[me.0 as usize] = Some(GreatDeed::Feast);
    let town = Hex::new(0, 0);
    set_terrain(&mut g, town, Terrain::Settlement);
    g.claims.insert((0, 0), me);
    for h in [Hex::new(-1, 0), Hex::new(-1, 1), Hex::new(0, -1)] {
        set_terrain(&mut g, h, Terrain::Fields);
    }
    g.stores.insert((0, 0), FEAST_FOOD);
    // One guest is not a feast for the deed.
    g.apply(me, Intent::Feast).unwrap();
    assert_eq!(g.food_at(town), 0);
    assert!(!g.feasted[me.0 as usize]);
    // With two it is.
    let third = g.players().find(|&p| p != me && p != foe).unwrap();
    g.place(third, Hex::new(0, 2));
    g.stores.insert((0, 0), FEAST_FOOD);
    g.turns[me.0 as usize].move_points = MOVE_POINTS;
    g.apply(me, Intent::Feast).unwrap();
    assert!(g.checks(me, GreatDeed::Feast).iter().all(Check::met));
}

// ---- Goods and fairs (§21.8) ----

#[test]
fn goods_are_sold_at_a_fair_a_kind_once() {
    let (mut g, me, _) = duel(4);
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    let region = g.board().tile(town).unwrap().region.unwrap();
    let mut ev = Vec::new();
    g.make_goods(&mut ev);
    assert!(g.loads().contains(&(town, Cargo::Goods(region))));
    g.champ_mut(me).spirit_points = 3;
    g.apply(me, Intent::Fair).unwrap();
    g.apply(me, Intent::Take).unwrap();
    let style = g.style(me);
    let hand = g.hand(me).len();
    g.apply(me, Intent::Lay).unwrap();
    assert_eq!(g.fair(town).unwrap().kinds(), 1);
    assert_eq!(g.style(me), style + 1);
    assert_eq!(g.hand(me).len(), hand + 1);
    // The same kind again is not bought: it lies there.
    g.champ_mut(me).cargo = Some(Cargo::Goods(region));
    g.drop_cargo(me, town, &mut ev);
    assert_eq!(g.fair(town).unwrap().kinds(), 1);
    // Its days out, the fair closes.
    g.dusks += FAIR_DUSKS;
    g.close_fairs(&mut ev);
    assert!(g.fair(town).is_none());
}

#[test]
fn the_dead_walk_to_a_fair_and_eat_it() {
    let (mut g, me, _) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::DeadFeast);
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    g.place(me, Hex::new(-2, 4));
    g.fairs.insert(
        (town.x(), town.y()),
        Fair {
            host: me,
            goods: 0,
            until: 99,
        },
    );
    g.militia.remove(&(town.x(), town.y()));
    let id = undead_on(&mut g, Hex::new(0, 5), 2);
    let mut ev = Vec::new();
    g.undead_walk(id, &mut ev);
    let at = g.mobs().iter().find(|m| m.id == id).unwrap().hex;
    assert!(at.unsigned_distance_to(town) < 3, "it walked to {at:?}");
    g.mobs.iter_mut().find(|m| m.id == id).unwrap().hex = Hex::new(1, 2);
    g.devour_fairs(&mut ev);
    assert!(g.fair(town).is_none());
    assert_eq!(g.dead_feasts(me), 1);
}

// ---- Rulers (§21.8) ----

/// Settlements of three different lands round the Table, with rulers.
fn three_courts(g: &mut Game) -> Vec<Hex> {
    let mut courts = Vec::new();
    for god in [God::Bhava, God::Trishna, God::Zaga] {
        let hex = g
            .board()
            .land()
            .filter(|(h, t)| t.region == Some(god) && h.ulength() == 2)
            .map(|(h, _)| h)
            .next()
            .unwrap();
        g.board.tile_mut(hex).unwrap().terrain = Terrain::Settlement;
        courts.push(hex);
    }
    g.seat_rulers();
    courts
}

#[test]
fn gifts_win_an_oath_and_a_rival_can_outbid() {
    let (mut g, me, foe) = duel(4);
    let courts = three_courts(&mut g);
    let court = courts[0];
    g.place(me, court);
    g.champ_mut(me).spirit_points = 9;
    let mut ev = Vec::new();
    for _ in 0..OATH_REGARD {
        g.gift(me, court, &mut ev).unwrap();
    }
    assert_eq!(g.ruler(court).unwrap().sworn, Some(me));
    g.place(foe, court.all_neighbors()[0]);
    g.champ_mut(foe).spirit_points = 9;
    for _ in 0..=OATH_REGARD {
        g.gift(foe, court, &mut ev).unwrap();
    }
    assert_eq!(g.ruler(court).unwrap().sworn, Some(foe));
}

#[test]
fn two_weddings_bind_three_lands() {
    let (mut g, me, _) = duel(4);
    g.dusks = EARLIEST_EVE;
    let courts = three_courts(&mut g);
    // A second court in the middle land, so it may wed twice.
    let second = g
        .board()
        .land()
        .filter(|(h, t)| t.region == Some(God::Trishna) && h.ulength() == 3)
        .map(|(h, _)| h)
        .next()
        .unwrap();
    g.board.tile_mut(second).unwrap().terrain = Terrain::Settlement;
    g.seat_rulers();
    for &c in courts.iter().chain([&second]) {
        g.rulers.get_mut(&(c.x(), c.y())).unwrap().regard[me.0 as usize] = MATCH_REGARD;
    }
    let mut ev = Vec::new();
    g.place(me, courts[0]);
    g.betroth(me, courts[0], courts[1], &mut ev).unwrap();
    g.place(me, second);
    g.betroth(me, second, courts[2], &mut ev).unwrap();
    g.weddings(&mut ev);
    assert_eq!(g.union_lands(me), 3);
    g.chosen[me.0 as usize] = Some(GreatDeed::TripleUnion);
    assert!(g.checks(me, GreatDeed::TripleUnion).iter().all(Check::met));
}

#[test]
fn an_emperor_breaks_the_empire_and_a_gift_ends_a_feud() {
    let (mut g, me, foe) = duel(4);
    let courts = three_courts(&mut g);
    for &c in &courts {
        let r = g.rulers.get_mut(&(c.x(), c.y())).unwrap();
        r.regard[me.0 as usize] = OATH_REGARD;
        r.sworn = Some(me);
    }
    g.place(me, Hex::ZERO);
    let mut ev = Vec::new();
    g.coronation(me, &mut ev).unwrap();
    assert_eq!(g.emperor(), Some(me));
    g.place(me, courts[0]);
    g.sow_discord(me, &mut ev).unwrap();
    assert_eq!(g.feuding(me), 3);
    assert!(g.vassals(me).is_empty());
    g.chosen[me.0 as usize] = Some(GreatDeed::FallenEmpire);
    assert!(g.checks(me, GreatDeed::FallenEmpire).iter().all(Check::met));
    g.place(foe, courts[1]);
    g.champ_mut(foe).spirit_points = 2;
    g.gift(foe, courts[1], &mut ev).unwrap();
    assert_eq!(g.feuding(me), 2);
}

// ---- Burial (§21.8) ----

#[test]
fn the_buried_never_rise_and_a_necropolis_needs_zagas_land_quiet() {
    let (mut g, me, _) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::Necropolis);
    let yard = [Hex::new(0, 2), Hex::new(1, 2), Hex::new(-1, 2)];
    for h in yard {
        set_terrain(&mut g, h, Terrain::Plains);
    }
    g.place(me, yard[0]);
    g.champ_mut(me).spirit_points = 3;
    g.apply(me, Intent::Consecrate).unwrap();
    assert_eq!(g.board().tile(yard[0]).unwrap().terrain, Terrain::Graveyard);
    // A body left lying there does not rise.
    g.board.tile_mut(yard[0]).unwrap().corpse = Some(Corpse {
        age: 9,
        hero: false,
    });
    let mut ev = Vec::new();
    g.raise_dead(&mut ev);
    assert!(g.board().tile(yard[0]).unwrap().corpse.is_some());
    g.board.tile_mut(yard[0]).unwrap().corpse = None;
    for h in &yard[1..] {
        set_terrain(&mut g, *h, Terrain::Graveyard);
    }
    for _ in 0..NECROPOLIS_BODIES {
        g.champ_mut(me).cargo = Some(Cargo::Body { hero: false });
        g.drop_cargo(me, yard[0], &mut ev);
    }
    assert_eq!(g.best_necropolis(), (NECROPOLIS, NECROPOLIS_BODIES));
    g.mobs.clear();
    assert!(g.checks(me, GreatDeed::Necropolis).iter().all(Check::met));
    let zaga = g
        .board()
        .land()
        .find(|(_, t)| t.region == Some(God::Zaga))
        .map(|(h, _)| h)
        .unwrap();
    undead_on(&mut g, zaga, 2);
    assert!(!g.checks(me, GreatDeed::Necropolis).iter().all(Check::met));
}

#[test]
fn a_full_pit_is_laid_to_rest_or_raised() {
    let (mut g, me, foe) = duel(2);
    let pit = Hex::new(0, 0);
    set_terrain(&mut g, pit, Terrain::Plains);
    g.champ_mut(me).spirit_points = 3;
    g.apply(me, Intent::DigPit).unwrap();
    let mut ev = Vec::new();
    for _ in 0..PIT_BODIES {
        g.champ_mut(me).cargo = Some(Cargo::Body { hero: false });
        g.drop_cargo(me, pit, &mut ev);
    }
    // Its fumes poison the neighbours.
    g.place(foe, Hex::new(1, 0));
    g.pit_fumes(foe, &mut ev);
    assert!(g.champion(foe).unwrap().poison.is_some());
    g.chosen[me.0 as usize] = Some(GreatDeed::PlaguePit);
    g.apply(me, Intent::SettlePit { raise: true }).unwrap();
    assert!(g.settled_pit(me));
    assert!(g.checks(me, GreatDeed::PlaguePit).iter().all(Check::met));
    assert!(g.mobs().iter().filter(|m| m.is_undead()).count() >= 3);
}

// ---- Rituals, monsters, dragons, the guest (§21.8) ----

#[test]
fn a_fed_circle_opens_its_gate_and_its_monster_is_the_summoning() {
    let (mut g, me, _) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::Summoning);
    let stones = Hex::new(0, 2);
    set_terrain(&mut g, stones, Terrain::Stones);
    g.place(me, stones);
    g.champ_mut(me).spirit_points = 3;
    g.apply(me, Intent::DrawCircle).unwrap();
    let mut ev = Vec::new();
    for _ in 0..SUMMON_BODIES {
        g.champ_mut(me).cargo = Some(Cargo::Body { hero: false });
        g.drop_cargo(me, stones, &mut ev);
    }
    g.open_gates(&mut ev);
    let monster = g.mobs().iter().find(|m| m.is_monster()).unwrap().id;
    assert!(g.circle(stones).is_none());
    g.hurt_mob(monster, me, MONSTER_HEALTH, &mut ev);
    assert!(g.summoned(me));
    assert!(g.checks(me, GreatDeed::Summoning).iter().all(Check::met));
}

#[test]
fn an_egg_warms_in_three_fires_and_hatches_a_dragon() {
    let (mut g, me, _) = duel(4);
    let woods = Hex::new(0, 3);
    let mut ev = Vec::new();
    for _ in 0..EGG_WARMTH {
        set_terrain(&mut g, woods, Terrain::Forest);
        g.loads.retain(|(h, _)| *h != woods);
        let warmth = g
            .champion(me)
            .and_then(|c| match c.cargo {
                Some(Cargo::Egg { warmth, .. }) => Some(warmth),
                _ => None,
            })
            .unwrap_or(0);
        g.champ_mut(me).cargo = Some(Cargo::Egg { warmth, by: None });
        g.drop_cargo(me, woods, &mut ev);
        g.set_fire(woods, Some(me), &mut ev);
        g.fire_phase(&mut ev);
        if let Some(&(_, egg)) = g.loads().iter().find(|(h, _)| *h == woods) {
            g.champ_mut(me).cargo = Some(egg);
        }
    }
    assert!(g.companions(me).contains(&Companion::Dragon));
    g.chosen[me.0 as usize] = Some(GreatDeed::Dragon);
    assert!(g.checks(me, GreatDeed::Dragon).iter().all(Check::met));
}

#[test]
fn the_guest_is_led_to_the_table() {
    let (mut g, me, _) = duel(4);
    g.mobs.clear();
    let mut ev = Vec::new();
    g.guest_at_dusk(&mut ev);
    let guest = g
        .mobs()
        .iter()
        .find(|m| matches!(m.kind, MobKind::Guest))
        .copied()
        .unwrap();
    let beside = guest
        .hex
        .all_neighbors()
        .into_iter()
        .find(|&h| g.board().contains(h) && g.champion_at(h).is_none())
        .unwrap();
    g.place(me, beside);
    g.apply(me, Intent::Recruit { mob: guest.id }).unwrap();
    assert!(g.companions(me).contains(&Companion::Guest));
    g.place(me, Hex::new(1, 0));
    g.turns[me.0 as usize].move_points = MOVE_POINTS;
    g.apply(me, Intent::Move { to: Hex::ZERO }).unwrap();
    assert!(g.guest_home(me));
    g.chosen[me.0 as usize] = Some(GreatDeed::Guest);
    assert!(g.checks(me, GreatDeed::Guest).iter().all(Check::met));
}

// ---- Pens and the Ark (§21.8) ----

#[test]
fn beasts_of_every_element_tethered_by_a_shrine_are_the_ark() {
    let (mut g, me, foe) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::Ark);
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    g.apply(
        me,
        Intent::Build {
            building: Building::Pen,
        },
    )
    .unwrap();
    g.champ_mut(me).companions = crate::gods::Element::ALL.map(Companion::Beast).to_vec();
    let mut ev = Vec::new();
    for e in crate::gods::Element::ALL {
        g.tether(me, e, &mut ev).unwrap();
    }
    assert!(g.companions(me).is_empty());
    assert_eq!(g.best_ark(me), (ARK, false));
    let shrine = Hex::new(1, 2);
    g.board.tile_mut(shrine).unwrap().terrain = Terrain::Settlement;
    g.claims.insert((shrine.x(), shrine.y()), me);
    g.buildings.insert(
        (shrine.x(), shrine.y()),
        Building::Shrine([God::Bhava, God::Bhava]),
    );
    assert!(g.checks(me, GreatDeed::Ark).iter().all(Check::met));
    // A rival in the pen leads one away.
    g.place(me, Hex::new(-1, 2));
    g.place(foe, town);
    g.untether(foe, crate::gods::Element::Fire, &mut ev)
        .unwrap();
    assert!(
        g.companions(foe)
            .contains(&Companion::Beast(crate::gods::Element::Fire))
    );
    assert!(!g.checks(me, GreatDeed::Ark).iter().all(Check::met));
}

// ---- Walking groves (§21.8) ----

#[test]
fn a_woken_grove_walks_to_the_table_and_roots() {
    let (mut g, me, foe) = duel(4);
    g.place(foe, Hex::new(-5, 1));
    g.chosen[me.0 as usize] = Some(GreatDeed::WalkingForest);
    for q in 1..=4 {
        set_terrain(&mut g, Hex::new(q, 0), Terrain::Plains);
    }
    let grove = Hex::new(4, 0);
    set_terrain(&mut g, grove, Terrain::Grove);
    let mut ev = Vec::new();
    g.wake_grove(me, grove, &mut ev);
    for _ in 0..3 {
        g.walk_groves(&mut ev);
    }
    assert_eq!(
        g.board().tile(Hex::new(1, 0)).unwrap().terrain,
        Terrain::Grove
    );
    assert_eq!(
        g.board().tile(Hex::new(3, 0)).unwrap().terrain,
        Terrain::Forest
    );
    assert!(g.rooted(me));
    assert!(
        g.checks(me, GreatDeed::WalkingForest)
            .iter()
            .all(Check::met)
    );
}

#[test]
fn choosing_the_walking_forest_brings_the_ent_into_the_deck() {
    let (mut g, me, _) = duel(4);
    assert!(
        !g.slice
            .iter()
            .any(|d| d.def().effect == crate::cards::Effect::Ent)
    );
    g.offers[me.0 as usize] = vec![GreatDeed::WalkingForest];
    g.chosen[me.0 as usize] = None;
    g.apply(
        me,
        Intent::ChooseDeed {
            deed: GreatDeed::WalkingForest,
        },
    )
    .unwrap();
    assert!(
        g.slice
            .iter()
            .any(|d| d.def().effect == crate::cards::Effect::Ent)
    );
}

// ---- Arenas, bets and debts (§21.8) ----

#[test]
fn a_duel_never_fought_is_the_hosts() {
    let (mut g, me, foe) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::Arena);
    let town = Hex::new(0, 2);
    settled(&mut g, me, town);
    g.apply(
        me,
        Intent::Build {
            building: Building::Arena,
        },
    )
    .unwrap();
    let mut ev = Vec::new();
    g.challenge(me, foe, &mut ev).unwrap();
    assert!(g.challengeable(me).is_empty(), "one duel at a time");
    g.round += DUEL_ROUNDS;
    g.duels_at_dusk(&mut ev);
    assert_eq!(g.arena_wins(me), 1);
    // A duel fought: the winner's, counted for the host if the host won.
    let third = g.players().find(|&p| p != me && p != foe).unwrap();
    g.challenge(me, third, &mut ev).unwrap();
    g.settle_by_battle(me, third, &mut ev);
    assert_eq!(g.arena_wins(me), 2);
    g.arena_wins[me.0 as usize] = ARENA_WINS as u8;
    assert!(g.checks(me, GreatDeed::Arena).iter().all(Check::met));
}

#[test]
fn bets_between_players_make_debts_paid_in_style_or_blood() {
    let (mut g, me, foe) = duel(4);
    let mut ev = Vec::new();
    g.place_bet(me, foe, Bet::Fight, &mut ev).unwrap();
    g.note_bet(foe, Bet::Fight);
    g.round += 2;
    g.settle_player_bets(&mut ev);
    assert_eq!(g.debtors(me), 1);
    // Beaten by the debtor, the creditor loses the debt.
    g.settle_by_battle(foe, me, &mut ev);
    assert_eq!(g.debtors(me), 0);
    // A bet lost: the bettor owes, and pays in Style.
    g.place_bet(me, foe, Bet::Hide, &mut ev).unwrap();
    g.round += 2;
    g.settle_player_bets(&mut ev);
    assert_eq!(g.debtors(foe), 1);
    g.add_style(me, 5, StyleReason::Battle, &mut ev);
    let before = g.style(foe);
    g.pay_debt(me, foe, &mut ev).unwrap();
    assert_eq!(g.style(foe), before + u16::from(BET_STAKE));
}

// ---- The ball of the dead (§21.7) ----

#[test]
fn a_night_feast_with_the_dead_near_kept_till_dawn_is_a_ball() {
    let (mut g, me, foe) = duel(2);
    g.time = TimeOfDay::Night;
    let hall = Hex::new(0, 0);
    set_terrain(&mut g, hall, Terrain::Settlement);
    g.claims.insert((0, 0), me);
    g.militia.insert(
        (0, 0),
        Militia {
            men: 2,
            at: Some(hall),
        },
    );
    let third = g.players().find(|&p| p != me && p != foe).unwrap();
    g.place(third, Hex::new(0, 2));
    undead_on(&mut g, Hex::new(-2, 0), 2);
    g.stores.insert((0, 0), FEAST_FOOD);
    let mut ev = Vec::new();
    g.hold_feast(me, &mut ev).unwrap();
    assert!(ev.iter().any(|e| matches!(e, Event::BallBegun { .. })));
    g.ball_at_dawn(&mut ev);
    assert!(g.kept_ball(me));
    g.chosen[me.0 as usize] = Some(GreatDeed::DeadBall);
    assert!(g.checks(me, GreatDeed::DeadBall).iter().all(Check::met));
    // A fight before dawn breaks it.
    g.balls[me.0 as usize] = false;
    g.stores.insert((0, 0), FEAST_FOOD);
    g.hold_feast(me, &mut ev).unwrap();
    g.brawls += 1;
    g.ball_at_dawn(&mut ev);
    assert!(!g.kept_ball(me));
}

// ---- The underworld (§21.8) ----

#[test]
fn a_treasury_under_ruins_is_dug_guarded_and_raided() {
    let (mut g, me, foe) = duel(4);
    g.chosen[me.0 as usize] = Some(GreatDeed::Treasury);
    let ruins = Hex::new(0, 2);
    set_terrain(&mut g, ruins, Terrain::Ruins);
    g.place(me, ruins);
    g.champ_mut(me).spirit_points = 9;
    let mut ev = Vec::new();
    g.work_delve(me, DelveWork::Open, &mut ev).unwrap();
    for _ in 0..HALL_DEPTH {
        g.work_delve(me, DelveWork::Dig, &mut ev).unwrap();
    }
    assert_eq!(
        g.work_delve(me, DelveWork::Dig, &mut ev),
        Err(RuleError::CannotBuild)
    );
    g.work_delve(me, DelveWork::Treasury, &mut ev).unwrap();
    g.champ_mut(me).companions = vec![Companion::Undead; 3];
    for _ in 0..3 {
        g.work_delve(me, DelveWork::Guard, &mut ev).unwrap();
    }
    for _ in 0..TREASURY_DUSKS {
        g.treasuries_at_dusk();
    }
    assert!(g.checks(me, GreatDeed::Treasury).iter().all(Check::met));
    // Three guards and one more: a champion of ordinary might is beaten back.
    g.place(me, Hex::new(-1, 3));
    g.place(foe, ruins);
    let hp = g.champion(foe).unwrap().hp;
    g.work_delve(foe, DelveWork::Raid, &mut ev).unwrap();
    assert_eq!(g.champion(foe).unwrap().hp, hp - 1);
    g.delves.get_mut(&(ruins.x(), ruins.y())).unwrap().guards = 0;
    g.work_delve(foe, DelveWork::Raid, &mut ev).unwrap();
    assert_eq!(g.treasury_held(me), 0);
}
