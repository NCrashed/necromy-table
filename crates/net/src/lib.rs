//! The wire between clients and the dedicated server (docs/design.md §17).
//!
//! TCP, one frame per message: a little-endian `u32` length, then the
//! message in postcard. A connection is two threads (a reader and a writer)
//! with queues in front, so neither the game nor the server loop ever
//! blocks on a socket.
//!
//! A client says hello, then creates a lobby or joins one by its invite
//! code, picks a god, and the owner starts the match. From then on the
//! messages are the table's own (`ToTable`, `FromTable`), wrapped.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use necromy_host::{FromTable, ToTable};
use necromy_rules::{God, PlayerId};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Bumped whenever a message changes shape; mismatched sides refuse.
pub const PROTOCOL: u32 = 55;
pub const DEFAULT_PORT: u16 = 7878;
/// Our playtest server (aerospace, service/necromy-table.nix): where the
/// menu points unless `NECROMY_SERVER` or the field says otherwise.
pub const PUBLIC_SERVER: &str = "81.88.219.217";
/// A view of the match is a few kilobytes; anything near this is garbage.
const MAX_FRAME: usize = 4 << 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello {
        protocol: u32,
        name: String,
    },
    /// Open a new lobby; the server answers with its code.
    Create,
    Join {
        code: String,
    },
    /// Take a god's seat, or give it back.
    Pick(Option<God>),
    /// The owner picks the world the match begins with (§21).
    Mode(necromy_rules::Mode),
    /// The owner picks the clocks on people's decisions.
    Pace(Pace),
    /// The owner starts the match; free seats go to bots.
    Start,
    /// Sit back down at a running match after losing the connection.
    Rejoin {
        code: String,
        ticket: u64,
    },
    Table(ToTable),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMsg {
    Lobby(LobbyInfo),
    /// The match began, or the client sat back down: it plays `seat`, and
    /// `ticket` brings it back to this seat if the connection breaks.
    Started {
        seat: PlayerId,
        ticket: u64,
    },
    Table(FromTable),
    /// Something was refused; in words for the player.
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyInfo {
    pub code: String,
    /// Index of the receiving client in `people`.
    pub you: usize,
    /// Index of the owner in `people`.
    pub owner: usize,
    pub people: Vec<Person>,
    /// Which model speaks for the gods on this server, if any.
    pub oracle: Option<String>,
    /// The world the match will begin with.
    pub mode: necromy_rules::Mode,
    /// The clocks the match will run with.
    pub pace: Pace,
}

/// How long people may think over a turn, a window and a wish.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pace {
    /// The server's clocks.
    #[default]
    Timed,
    /// Three times as long.
    Slow,
    /// No clocks: for a thoughtful test.
    Untimed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub god: Option<God>,
}

/// One end of a connection: send `Out`, receive `In`.
pub struct Conn<In, Out> {
    out: Sender<Out>,
    // Behind a lock only so a connection can live in a Bevy resource.
    inbox: Mutex<Receiver<In>>,
    open: Arc<AtomicBool>,
    stream: TcpStream,
}

/// The client's end.
pub type ClientConn = Conn<ServerMsg, ClientMsg>;
/// The server's end of one client.
pub type ServerConn = Conn<ClientMsg, ServerMsg>;

impl<In, Out> Conn<In, Out>
where
    In: DeserializeOwned + Send + 'static,
    Out: Serialize + Send + 'static,
{
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nodelay(true)?;
        let open = Arc::new(AtomicBool::new(true));
        let (out, outbox) = channel::<Out>();
        let (inbox_tx, inbox) = channel::<In>();
        {
            let mut stream = stream.try_clone()?;
            let open = open.clone();
            std::thread::Builder::new()
                .name("net-read".into())
                .spawn(move || {
                    while let Ok(message) = read_frame(&mut stream) {
                        if inbox_tx.send(message).is_err() {
                            break;
                        }
                    }
                    open.store(false, Ordering::Relaxed);
                })?;
        }
        {
            let mut stream = stream.try_clone()?;
            let open = open.clone();
            std::thread::Builder::new()
                .name("net-write".into())
                .spawn(move || {
                    for message in outbox {
                        if write_frame(&mut stream, &message).is_err() {
                            break;
                        }
                    }
                    open.store(false, Ordering::Relaxed);
                    let _ = stream.shutdown(Shutdown::Both);
                })?;
        }
        Ok(Conn {
            out,
            inbox: Mutex::new(inbox),
            open,
            stream,
        })
    }

    pub fn send(&self, message: Out) {
        let _ = self.out.send(message);
    }

    /// The next message that arrived, if any; never blocks.
    pub fn poll(&self) -> Option<In> {
        self.inbox.lock().ok()?.try_recv().ok()
    }

    /// Wait up to `timeout` for the next message.
    pub fn wait(&self, timeout: Duration) -> Option<In> {
        self.inbox.lock().ok()?.recv_timeout(timeout).ok()
    }

    /// False once either direction broke; queued messages can still be polled.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }

    pub fn peer(&self) -> Option<std::net::SocketAddr> {
        self.stream.peer_addr().ok()
    }
}

impl<In, Out> Drop for Conn<In, Out> {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// Dial the server and say hello.
pub fn connect(addr: &str, name: &str) -> io::Result<ClientConn> {
    let addr = with_port(addr);
    let target = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no address"))?;
    let stream = TcpStream::connect_timeout(&target, Duration::from_secs(5))?;
    let conn = Conn::new(stream)?;
    conn.send(ClientMsg::Hello {
        protocol: PROTOCOL,
        name: name.to_string(),
    });
    Ok(conn)
}

/// `host` alone means the default port.
pub fn with_port(addr: &str) -> String {
    if addr
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
    {
        addr.to_string()
    } else {
        format!("{addr}:{DEFAULT_PORT}")
    }
}

/// Invite codes are short, upper case and free of look-alikes (0/O, 1/I).
/// A table code as typed or said ("482 915", "482-915") down to its digits.
pub fn normalize_code(code: &str) -> String {
    code.chars().filter(char::is_ascii_digit).collect()
}

/// A code to say aloud: two groups of three.
pub fn spoken_code(code: &str) -> String {
    if code.len() == CODE_LEN {
        format!("{} {}", &code[..3], &code[3..])
    } else {
        code.to_string()
    }
}

/// Table codes are digits: easy to dictate over voice chat.
pub const CODE_LETTERS: &[u8] = b"0123456789";
pub const CODE_LEN: usize = 6;

fn read_frame<T: DeserializeOwned>(stream: &mut TcpStream) -> io::Result<T> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too big"));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    postcard::from_bytes(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn write_frame<T: Serialize>(stream: &mut TcpStream, message: &T) -> io::Result<()> {
    let buf =
        postcard::to_stdvec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    stream.write_all(&(buf.len() as u32).to_le_bytes())?;
    stream.write_all(&buf)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use necromy_host::{Config, Seat, Table};
    use std::net::TcpListener;

    #[test]
    fn a_view_of_the_match_crosses_the_wire_intact() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let client = connect(&addr, "Аня").unwrap();
        let (stream, _) = listener.accept().unwrap();
        let server: ServerConn = Conn::new(stream).unwrap();

        let hello = server.wait(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(hello, ClientMsg::Hello { protocol: PROTOCOL, ref name } if name == "Аня")
        );

        let mut seats = vec![Seat::Bot; 5];
        seats[2] = Seat::Human;
        let mut table = Table::new(Config {
            seed: 3,
            champions: God::ALL.to_vec(),
            seats,
            salt: 9,
            oracle: None,
            timers: None,
            mode: Default::default(),
        });
        let sent = table.drain(PlayerId(2));
        for m in sent.clone() {
            server.send(ServerMsg::Table(m));
        }
        let got = client.wait(Duration::from_secs(5)).unwrap();
        let (
            ServerMsg::Table(FromTable::Update { view, events, .. }),
            FromTable::Update {
                view: sent_view,
                events: sent_events,
                ..
            },
        ) = (got, &sent[0])
        else {
            panic!("expected an update");
        };
        assert_eq!(&events, sent_events);
        assert_eq!(view.board().extent(), sent_view.board().extent());
        assert_eq!(view.hand(PlayerId(2)), sent_view.hand(PlayerId(2)));
        assert_eq!(view.round(), sent_view.round());
    }

    #[test]
    fn ports_default() {
        assert_eq!(with_port("example.org"), "example.org:7878");
        assert_eq!(with_port("10.0.0.2:9000"), "10.0.0.2:9000");
    }
}
