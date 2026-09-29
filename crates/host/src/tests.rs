use super::*;

fn table(seats: Vec<Seat>) -> Table {
    Table::new(Config {
        seed: 9,
        champions: God::ALL.to_vec(),
        seats,
        salt: 1234,
        oracle: None,
        timers: None,
        mode: Default::default(),
    })
}

fn updates(messages: &[FromTable]) -> Vec<(u32, &Game)> {
    messages
        .iter()
        .filter_map(|m| match m {
            FromTable::Update { serial, view, .. } => Some((*serial, view.as_ref())),
            _ => None,
        })
        .collect()
}

#[test]
fn bots_alone_play_a_match_far() {
    let mut t = table(vec![Seat::Bot; 5]);
    for _ in 0..200_000 {
        if t.game().winner().is_some() || t.game().round() >= 20 {
            return;
        }
        t.tick(BOT_STEP_SECS);
    }
    panic!("stuck at round {}", t.game().round());
}

#[test]
fn a_person_sees_their_seat_only() {
    let mut seats = vec![Seat::Bot; 5];
    seats[1] = Seat::Human;
    let mut t = table(seats);
    let first = t.drain(PlayerId(1));
    let (serial, view) = updates(&first)[0];
    assert_eq!(serial, 1);
    assert_eq!(view.seed(), 0);
    assert!(view.log().is_empty(), "a view carries no log");
    assert!(
        !view.offers(PlayerId(1)).is_empty(),
        "its deeds to pick from"
    );
    assert!(t.drain(PlayerId(0)).is_empty(), "bots get no mail");
}

#[test]
fn bots_wait_until_people_have_seen_the_change() {
    let mut seats = vec![Seat::Bot; 5];
    seats[1] = Seat::Human;
    let mut t = table(seats);
    let human = PlayerId(1);
    // Play until a bot is to act.
    let mut guard = 0;
    while t.game().awaiting().contains(&human) {
        let intent = bot::choose(t.game(), human);
        t.submit(human, ToTable::Act(intent));
        guard += 1;
        assert!(guard < 100);
    }
    let serial = updates(&t.drain(human)).last().unwrap().0;
    let before = t.game().log().len();
    t.tick(BOT_STEP_SECS);
    assert_eq!(t.game().log().len(), before, "not shown yet: bots wait");
    t.submit(human, ToTable::Shown(serial));
    t.tick(BOT_STEP_SECS);
    assert!(t.game().log().len() > before, "shown: a bot acts");
}

#[test]
fn a_slow_screen_holds_the_table_only_so_long() {
    let mut seats = vec![Seat::Bot; 5];
    seats[1] = Seat::Human;
    let mut t = table(seats);
    let human = PlayerId(1);
    while t.game().awaiting().contains(&human) {
        let intent = bot::choose(t.game(), human);
        t.submit(human, ToTable::Act(intent));
    }
    let before = t.game().log().len();
    t.tick(SHOW_TIMEOUT_SECS + 0.1);
    assert!(t.game().log().len() > before);
}

#[test]
fn a_broken_intent_is_refused_to_its_sender() {
    let mut seats = vec![Seat::Bot; 5];
    seats[1] = Seat::Human;
    let mut t = table(seats);
    let human = PlayerId(1);
    t.drain(human);
    // Everyone acts at once (§11.2): the human ends their turn, then tries
    // again out of turn, which is refused to them alone.
    t.submit(human, ToTable::Act(Intent::EndTurn));
    t.drain(human);
    t.submit(human, ToTable::Act(Intent::EndTurn));
    assert!(
        t.drain(human)
            .iter()
            .any(|m| matches!(m, FromTable::Rejected(_)))
    );
}

#[test]
fn a_wish_in_words_without_a_model_is_not_heard() {
    let mut seats = vec![Seat::Bot; 5];
    seats[1] = Seat::Human;
    let mut t = table(seats);
    let human = PlayerId(1);
    t.drain(human);
    t.submit(
        human,
        ToTable::Wish {
            god: God::Maya,
            text: "дай мне сил".into(),
        },
    );
    assert!(matches!(
        t.drain(human).as_slice(),
        [FromTable::Oracle(OracleNews::NotHeard(_))]
    ));
}

#[test]
fn a_wish_being_written_is_seen_by_the_others() {
    let seats = vec![Seat::Autoplay { wish_by_hand: true }; 5];
    let mut t = table(seats);
    let everyone: Vec<PlayerId> = (0..5).map(PlayerId).collect();
    for &p in &everyone {
        t.drain(p);
    }
    // By day everyone may write a wish (§21.4); once sealed, no more.
    let (writer, other) = (PlayerId(0), PlayerId(1));
    t.submit(other, ToTable::Act(Intent::RefuseWish));
    for &p in &everyone {
        t.drain(p);
    }
    t.submit(
        other,
        ToTable::Draft {
            god: None,
            text: "не моё".into(),
        },
    );
    for &p in &everyone {
        assert!(
            !t.drain(p)
                .iter()
                .any(|m| matches!(m, FromTable::Drafting { .. }))
        );
    }
    t.submit(
        writer,
        ToTable::Draft {
            god: Some(God::Maya),
            text: "пусть реки".into(),
        },
    );
    for &p in &everyone {
        let seen = t.drain(p).into_iter().any(|m| {
            matches!(m, FromTable::Drafting { player, god: Some(God::Maya), ref text }
                if player == writer && text == "пусть реки")
        });
        assert_eq!(seen, p != writer, "seat {p:?}");
    }
}
