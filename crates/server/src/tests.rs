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
        ServerMsg::Started { seat } => Some(seat),
        _ => None,
    });
    let boris_seat = rig.until(&boris, |m| match m {
        ServerMsg::Started { seat } => Some(seat),
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
                    assert!(view.secret(seat).is_some());
                    let rival = if seat == anna_seat {
                        boris_seat
                    } else {
                        anna_seat
                    };
                    assert!(view.secret(rival).is_none());
                    round = view.round();
                    c.send(ClientMsg::Table(ToTable::Shown(serial)));
                    if view.awaiting().contains(&seat) {
                        let intent = if view.window().is_some() {
                            Intent::Pass
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
        ServerMsg::Started { seat } => Some(seat),
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
                    let intent = if view.window().is_some() {
                        Intent::Pass
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
