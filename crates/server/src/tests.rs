use std::net::TcpListener;
use std::time::Duration;

use necromy_host::{FromTable, ToTable};
use necromy_net::{ClientConn, Conn, LobbyInfo, connect};
use necromy_rules::Intent;

use super::*;

/// A server on a loopback port and a way to bring clients in.
struct Rig {
    server: Server,
    listener: TcpListener,
    addr: String,
}

impl Rig {
    fn new() -> Rig {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        Rig {
            server: Server::new(None, 77),
            listener,
            addr,
        }
    }

    fn client(&mut self, name: &str) -> ClientConn {
        let c = connect(&self.addr, name).unwrap();
        let (stream, _) = self.listener.accept().unwrap();
        self.server.accept(Conn::new(stream).unwrap());
        c
    }

    /// Run the server until `pred` holds for a message `c` receives.
    fn until<T>(&mut self, c: &ClientConn, mut pred: impl FnMut(ServerMsg) -> Option<T>) -> T {
        for _ in 0..400 {
            self.server.step(0.05);
            while let Some(m) = c.wait(Duration::from_millis(5)) {
                if let Some(t) = pred(m) {
                    return t;
                }
            }
        }
        panic!("the awaited message never came");
    }
}

fn lobby(m: ServerMsg) -> Option<LobbyInfo> {
    match m {
        ServerMsg::Lobby(info) => Some(info),
        _ => None,
    }
}

#[test]
fn friends_meet_by_code_and_play_against_bots() {
    let mut rig = Rig::new();
    let anna = rig.client("Аня");
    let boris = rig.client("Борис");

    anna.send(ClientMsg::Create);
    let code = rig.until(&anna, lobby).code;
    assert_eq!(code.len(), CODE_LEN);

    boris.send(ClientMsg::Join {
        code: code.to_lowercase(),
    });
    let seen = rig.until(&boris, lobby);
    assert_eq!(seen.people.len(), 2);
    assert_eq!(seen.you, 1);
    assert_eq!(seen.owner, 0);

    boris.send(ClientMsg::Pick(Some(God::Maya)));
    rig.until(&boris, |m| {
        lobby(m).filter(|l| l.people[1].god == Some(God::Maya))
    });
    anna.send(ClientMsg::Pick(Some(God::Maya)));
    let refused = rig.until(&anna, |m| match m {
        ServerMsg::Error(e) => Some(e),
        _ => None,
    });
    assert!(refused.contains("выбрали"));

    // Only the owner starts.
    boris.send(ClientMsg::Start);
    rig.until(&boris, |m| matches!(m, ServerMsg::Error(_)).then_some(()));
    anna.send(ClientMsg::Start);
    let anna_seat = rig.until(&anna, |m| match m {
        ServerMsg::Started { seat, .. } => Some(seat),
        _ => None,
    });
    let boris_seat = rig.until(&boris, |m| match m {
        ServerMsg::Started { seat, .. } => Some(seat),
        _ => None,
    });
    assert_eq!(boris_seat, PlayerId(God::Maya.index() as u8));
    assert_ne!(anna_seat, boris_seat);

    // Each sees their own seat; both play by passing and ending turns
    // until the table moves past the first round.
    let mut round = 0;
    for _ in 0..3000 {
        rig.server.step(0.4);
        for (c, seat) in [(&anna, anna_seat), (&boris, boris_seat)] {
            while let Some(m) = c.poll() {
                if let ServerMsg::Table(FromTable::Update { serial, view, .. }) = m {
                    assert!(view.log().is_empty(), "a view carries no log");
                    round = view.round();
                    c.send(ClientMsg::Table(ToTable::Shown(serial)));
                    if view.awaiting().contains(&seat) {
                        let intent = if view.to_answer(seat).is_some() {
                            Intent::Pass
                        } else if view.wishing().contains(&seat) {
                            Intent::RefuseWish
                        } else {
                            Intent::EndTurn
                        };
                        c.send(ClientMsg::Table(ToTable::Act(intent)));
                    }
                }
            }
        }
        if round >= 3 {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the match did not move on: round {round}");
}

#[test]
fn a_seat_left_empty_goes_to_a_bot() {
    let mut rig = Rig::new();
    let anna = rig.client("Аня");
    let boris = rig.client("Борис");
    anna.send(ClientMsg::Create);
    let code = rig.until(&anna, lobby).code;
    boris.send(ClientMsg::Join { code });
    rig.until(&boris, lobby);
    anna.send(ClientMsg::Start);
    let seat = rig.until(&anna, |m| match m {
        ServerMsg::Started { seat, .. } => Some(seat),
        _ => None,
    });
    drop(boris);
    // Anna alone keeps playing; the table runs on.
    let mut round = 0;
    for _ in 0..3000 {
        rig.server.step(0.4);
        while let Some(m) = anna.poll() {
            if let ServerMsg::Table(FromTable::Update { serial, view, .. }) = m {
                round = view.round();
                anna.send(ClientMsg::Table(ToTable::Shown(serial)));
                if view.awaiting().contains(&seat) {
                    let intent = if view.to_answer(seat).is_some() {
                        Intent::Pass
                    } else if view.wishing().contains(&seat) {
                        Intent::RefuseWish
                    } else {
                        Intent::EndTurn
                    };
                    anna.send(ClientMsg::Table(ToTable::Act(intent)));
                }
            }
        }
        if round >= 3 {
            assert_eq!(rig.server.lobby_count(), 1);
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("stuck at round {round}");
}

#[test]
fn wrong_codes_and_strangers_are_turned_away() {
    let mut rig = Rig::new();
    let anna = rig.client("Аня");
    anna.send(ClientMsg::Join {
        code: "ZZZZZ".into(),
    });
    let e = rig.until(&anna, |m| match m {
        ServerMsg::Error(e) => Some(e),
        _ => None,
    });
    assert!(e.contains("код"));
}

/// Open a table for `names`, start it, and return the clients, their seats
/// and tickets, and the code.
fn seated(rig: &mut Rig, names: &[&str]) -> (Vec<ClientConn>, Vec<(PlayerId, u64)>, String) {
    let clients: Vec<ClientConn> = names.iter().map(|n| rig.client(n)).collect();
    clients[0].send(ClientMsg::Create);
    let code = rig.until(&clients[0], lobby).code;
    for c in &clients[1..] {
        c.send(ClientMsg::Join { code: code.clone() });
        rig.until(c, lobby);
    }
    clients[0].send(ClientMsg::Start);
    let seats = clients
        .iter()
        .map(|c| {
            rig.until(c, |m| match m {
                ServerMsg::Started { seat, ticket } => Some((seat, ticket)),
                _ => None,
            })
        })
        .collect();
    (clients, seats, code)
}

#[test]
fn a_lost_player_sits_back_down_by_ticket() {
    let mut rig = Rig::new();
    let (mut clients, seats, code) = seated(&mut rig, &["Аня", "Борис"]);
    let (seat, ticket) = seats[1];
    drop(clients.pop());
    for _ in 0..5 {
        rig.server.step(0.05);
    }
    let boris = rig.client("Борис");
    // Someone else's ticket does not work.
    boris.send(ClientMsg::Rejoin {
        code: code.clone(),
        ticket: ticket ^ 1,
    });
    rig.until(&boris, |m| matches!(m, ServerMsg::Error(_)).then_some(()));
    boris.send(ClientMsg::Rejoin { code, ticket });
    let back = rig.until(&boris, |m| match m {
        ServerMsg::Started { seat, .. } => Some(seat),
        _ => None,
    });
    assert_eq!(back, seat);
    // A fresh view of the running match follows.
    let view = rig.until(&boris, |m| match m {
        ServerMsg::Table(FromTable::Update { view, .. }) => Some(view),
        _ => None,
    });
    assert!(!view.offers(seat).is_empty());
}

#[test]
fn a_player_who_sits_idle_is_timed_out() {
    let mut rig = Rig::new();
    rig.server.timers = Some(necromy_host::Timers {
        turn: 2.0,
        window: 1.0,
        wish: 1.0,
    });
    let (clients, seats, _) = seated(&mut rig, &["Аня"]);
    let (seat, _) = seats[0];
    // Anna only watches: she never acts, yet the rounds go on.
    let mut round = 0;
    let mut timed_out = 0;
    for _ in 0..4000 {
        rig.server.step(0.5);
        while let Some(m) = clients[0].poll() {
            match m {
                ServerMsg::Table(FromTable::Update { serial, view, .. }) => {
                    round = view.round();
                    clients[0].send(ClientMsg::Table(ToTable::Shown(serial)));
                }
                ServerMsg::Table(FromTable::TimedOut(_)) => timed_out += 1,
                _ => {}
            }
        }
        if round >= 3 {
            assert!(timed_out > 0, "{seat:?} was never timed out");
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("an idle player held the table: round {round}");
}

#[test]
fn an_empty_match_waits_and_then_closes() {
    let mut rig = Rig::new();
    let (clients, _, _) = seated(&mut rig, &["Аня"]);
    drop(clients);
    for _ in 0..20 {
        rig.server.step(0.05);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(rig.server.lobby_count(), 1, "kept for a return");
    rig.server.step(ABANDON_SECS + 1.0);
    assert_eq!(rig.server.lobby_count(), 0);
}

/// A fresh directory for a test's saves.
fn saves(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("necromy-server-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Play until the table has moved on; the round `c` last saw.
fn play_a_while(rig: &mut Rig, c: &ClientConn, seat: PlayerId) -> u32 {
    let mut round = 0;
    for _ in 0..400 {
        rig.server.step(0.4);
        while let Some(m) = c.poll() {
            if let ServerMsg::Table(FromTable::Update { serial, view, .. }) = m {
                round = view.round();
                c.send(ClientMsg::Table(ToTable::Shown(serial)));
                if view.awaiting().contains(&seat) {
                    let intent = if view.to_answer(seat).is_some() {
                        Intent::Pass
                    } else if view.wishing().contains(&seat) {
                        Intent::RefuseWish
                    } else {
                        Intent::EndTurn
                    };
                    c.send(ClientMsg::Table(ToTable::Act(intent)));
                }
            }
        }
        if round >= 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(round >= 2, "the match did not move: round {round}");
    round
}

/// Sit back down by ticket; the round of the first view.
fn back_by_ticket(rig: &mut Rig, code: &str, (seat, ticket): (PlayerId, u64)) -> u32 {
    let anna = rig.client("Аня");
    anna.send(ClientMsg::Rejoin {
        code: code.to_string(),
        ticket,
    });
    let back = rig.until(&anna, |m| match m {
        ServerMsg::Started { seat, .. } => Some(seat),
        ServerMsg::Error(e) => panic!("refused: {e}"),
        _ => None,
    });
    assert_eq!(back, seat);
    rig.until(&anna, |m| match m {
        ServerMsg::Table(FromTable::Update { view, .. }) => Some(view.round()),
        _ => None,
    })
}

#[test]
fn a_match_outlives_a_server_restart() {
    let dir = saves("restart");
    let mut rig = Rig::new();
    rig.server.saves = Some(dir.clone());
    let (clients, seats, code) = seated(&mut rig, &["Аня"]);
    let round = play_a_while(&mut rig, &clients[0], seats[0].0);
    // The server goes down with everyone on it, and comes up again.
    rig.server = Server::new(None, 78);
    rig.server.saves = Some(dir.clone());
    drop(clients);
    assert_eq!(rig.server.lobby_count(), 0);
    assert_eq!(back_by_ticket(&mut rig, &code, seats[0]), round);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_match_is_put_away_and_taken_up_by_ticket() {
    let dir = saves("away");
    let mut rig = Rig::new();
    rig.server.saves = Some(dir.clone());
    let (clients, seats, code) = seated(&mut rig, &["Аня"]);
    let round = play_a_while(&mut rig, &clients[0], seats[0].0);
    drop(clients);
    for _ in 0..20 {
        rig.server.step(0.05);
        std::thread::sleep(Duration::from_millis(5));
    }
    rig.server.step(ABANDON_SECS + 1.0);
    assert_eq!(rig.server.lobby_count(), 0, "put away");
    // A code taken by a match on disk is not handed out again.
    assert!(dir.join(&code).is_dir());
    let back = back_by_ticket(&mut rig, &code, seats[0]);
    assert!(back >= round);
    assert_eq!(rig.server.lobby_count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn old_kept_matches_are_swept() {
    let dir = saves("sweep");
    let mut rig = Rig::new();
    rig.server.saves = Some(dir.clone());
    let (clients, _, code) = seated(&mut rig, &["Аня"]);
    rig.server.sweep(Duration::ZERO);
    assert!(dir.join(&code).is_dir(), "a running match stays");
    drop(clients);
    for _ in 0..20 {
        rig.server.step(0.05);
        std::thread::sleep(Duration::from_millis(5));
    }
    rig.server.step(ABANDON_SECS + 1.0);
    std::thread::sleep(Duration::from_millis(20));
    rig.server.sweep(Duration::from_millis(1));
    assert!(!dir.join(&code).exists());
    let _ = std::fs::remove_dir_all(&dir);
}
