use super::*;

fn table(seats: Vec<Seat>) -> Table {
    Table::new(Config {
        seed: 9,
        champions: God::ALL.to_vec(),
        seats,
        salt: 1234,
        oracle: None,
        timers: None,
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
fn bots_alone_play_a_match_to_the_end() {
    let mut t = table(vec![Seat::Bot; 5]);
    for _ in 0..200_000 {
        if t.game().winner().is_some() {
            return;
        }
        t.tick(BOT_STEP_SECS);
    }
    panic!("no winner after many steps");
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
    assert!(view.secret(PlayerId(1)).is_some());
    assert!(view.secret(PlayerId(0)).is_none());
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
    let someone_else = t
        .game()
        .players()
        .find(|p| *p != human && !t.game().awaiting().contains(p))
        .unwrap();
    // A human seat cannot act for another seat, and out of turn is refused.
    t.submit(someone_else, ToTable::Act(Intent::EndTurn));
    if !t.game().awaiting().contains(&human) {
        t.submit(human, ToTable::Act(Intent::EndTurn));
        assert!(matches!(
            t.drain(human).as_slice(),
            [FromTable::Rejected(_)]
        ));
    }
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
