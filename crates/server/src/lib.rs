//! The dedicated server (docs/design.md §17.1): lobbies by invite code and
//! the tables that run in them. No graphics, no rules of its own: a table
//! from `necromy-host` decides everything; the server only carries messages
//! between it and the people at it.
//!
//! One loop owns all state. Connections arrive through [`Server::accept`],
//! and [`Server::step`] reads what came in, lets time pass at every table
//! and sends out what the tables said.

use std::collections::BTreeMap;

use necromy_host::{Config, Seat, Table, Timers};
use necromy_net::{
    CODE_LEN, CODE_LETTERS, ClientMsg, LobbyInfo, PROTOCOL, Person, ServerConn, ServerMsg,
    normalize_code,
};
use necromy_rules::{God, PlayerId};

/// Seats at a table; free ones go to bots.
pub const SEATS: usize = 5;
/// A match nobody sits at waits this long for someone to come back.
pub const ABANDON_SECS: f32 = 600.0;
const MAX_NAME: usize = 24;

pub struct Server {
    clients: BTreeMap<u64, Client>,
    lobbies: BTreeMap<String, Lobby>,
    next_client: u64,
    rng: u64,
    /// `llama-server` address for the tables' gods' voice.
    oracle: Option<String>,
    /// Clocks on people's decisions at every table.
    pub timers: Option<Timers>,
}

struct Client {
    conn: ServerConn,
    /// Set by the hello; nothing else is accepted before it.
    name: Option<String>,
    lobby: Option<String>,
}

struct Lobby {
    owner: u64,
    /// In joining order.
    members: Vec<u64>,
    picks: BTreeMap<u64, God>,
    running: Option<Running>,
}

struct Running {
    table: Table,
    /// Connected clients and their seats.
    seats: BTreeMap<u64, PlayerId>,
    /// Every person's ticket back to their seat.
    tickets: BTreeMap<u64, PlayerId>,
    /// Seconds since the last person left, while nobody sits.
    empty_for: f32,
}

impl Server {
    pub fn new(oracle: Option<String>, seed: u64) -> Server {
        Server {
            clients: BTreeMap::new(),
            lobbies: BTreeMap::new(),
            next_client: 0,
            rng: seed,
            oracle,
            timers: Some(Timers::default()),
        }
    }

    pub fn accept(&mut self, conn: ServerConn) {
        self.next_client += 1;
        self.clients.insert(
            self.next_client,
            Client {
                conn,
                name: None,
                lobby: None,
            },
        );
    }

    /// Lobbies open or playing, for the log.
    pub fn lobby_count(&self) -> usize {
        self.lobbies.len()
    }

    pub fn step(&mut self, dt: f32) {
        let ids: Vec<u64> = self.clients.keys().copied().collect();
        for id in ids {
            while let Some(message) = self.clients.get(&id).and_then(|c| c.conn.poll()) {
                self.handle(id, message);
            }
            if self.clients.get(&id).is_some_and(|c| !c.conn.is_open()) {
                self.drop_client(id);
            }
        }
        let mut abandoned = Vec::new();
        for (code, lobby) in self.lobbies.iter_mut() {
            let Some(running) = lobby.running.as_mut() else {
                continue;
            };
            // Nobody sits: the match waits for someone to come back, a while.
            if running.seats.is_empty() {
                running.empty_for += dt;
                if running.empty_for >= ABANDON_SECS {
                    abandoned.push(code.clone());
                }
                continue;
            }
            running.empty_for = 0.0;
            running.table.tick(dt);
            for (&client, &seat) in &running.seats {
                let messages = running.table.drain(seat);
                if let Some(c) = self.clients.get(&client) {
                    for m in messages {
                        c.conn.send(ServerMsg::Table(m));
                    }
                }
            }
        }
        for code in abandoned {
            self.lobbies.remove(&code);
        }
    }

    fn next_random(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn send(&self, id: u64, message: ServerMsg) {
        if let Some(c) = self.clients.get(&id) {
            c.conn.send(message);
        }
    }

    fn refuse(&self, id: u64, why: &str) {
        self.send(id, ServerMsg::Error(why.to_string()));
    }

    fn handle(&mut self, id: u64, message: ClientMsg) {
        let named = self.clients.get(&id).is_some_and(|c| c.name.is_some());
        match message {
            ClientMsg::Hello { protocol, name } => {
                if protocol != PROTOCOL {
                    self.refuse(
                        id,
                        &format!("другая версия игры (сервер {PROTOCOL}, клиент {protocol})"),
                    );
                    return;
                }
                let name: String = name.trim().chars().take(MAX_NAME).collect();
                let name = if name.is_empty() {
                    "Игрок".into()
                } else {
                    name
                };
                if let Some(c) = self.clients.get_mut(&id) {
                    c.name = Some(name);
                }
            }
            _ if !named => self.refuse(id, "сначала представься"),
            ClientMsg::Create => {
                self.leave(id);
                let code = self.fresh_code();
                self.lobbies.insert(
                    code.clone(),
                    Lobby {
                        owner: id,
                        members: vec![id],
                        picks: BTreeMap::new(),
                        running: None,
                    },
                );
                if let Some(c) = self.clients.get_mut(&id) {
                    c.lobby = Some(code.clone());
                }
                self.tell_lobby(&code);
            }
            ClientMsg::Join { code } => {
                let code = normalize_code(&code);
                let refusal = match self.lobbies.get(&code) {
                    None => Some("нет стола с таким кодом"),
                    Some(l) if l.running.is_some() => Some("за этим столом уже играют"),
                    Some(l) if l.members.len() >= SEATS => Some("стол полон"),
                    Some(l) if l.members.contains(&id) => None,
                    Some(_) => None,
                };
                if let Some(why) = refusal {
                    self.refuse(id, why);
                    return;
                }
                if self.lobby_of(id) != Some(code.clone()) {
                    self.leave(id);
                    if let Some(l) = self.lobbies.get_mut(&code) {
                        l.members.push(id);
                    }
                    if let Some(c) = self.clients.get_mut(&id) {
                        c.lobby = Some(code.clone());
                    }
                }
                self.tell_lobby(&code);
            }
            ClientMsg::Pick(god) => {
                let Some(code) = self.lobby_of(id) else {
                    self.refuse(id, "ты не за столом");
                    return;
                };
                let Some(lobby) = self.lobbies.get_mut(&code) else {
                    return;
                };
                if lobby.running.is_some() {
                    return;
                }
                match god {
                    Some(g) if lobby.picks.iter().any(|(&who, &p)| p == g && who != id) => {
                        self.refuse(id, "этого бога уже выбрали");
                        return;
                    }
                    Some(g) => {
                        lobby.picks.insert(id, g);
                    }
                    None => {
                        lobby.picks.remove(&id);
                    }
                }
                self.tell_lobby(&code);
            }
            ClientMsg::Rejoin { code, ticket } => {
                self.leave(id);
                self.rejoin(id, &code, ticket);
            }
            ClientMsg::Start => {
                let Some(code) = self.lobby_of(id) else {
                    self.refuse(id, "ты не за столом");
                    return;
                };
                if self.lobbies.get(&code).is_some_and(|l| l.owner != id) {
                    self.refuse(id, "начинает тот, кто открыл стол");
                    return;
                }
                self.start(&code);
            }
            ClientMsg::Table(message) => {
                let Some(code) = self.lobby_of(id) else {
                    return;
                };
                if let Some(running) = self.lobbies.get_mut(&code).and_then(|l| l.running.as_mut())
                    && let Some(&seat) = running.seats.get(&id)
                {
                    running.table.submit(seat, message);
                }
            }
        }
    }

    fn lobby_of(&self, id: u64) -> Option<String> {
        self.clients.get(&id).and_then(|c| c.lobby.clone())
    }

    fn fresh_code(&mut self) -> String {
        loop {
            let code: String = (0..CODE_LEN)
                .map(|_| {
                    let i = (self.next_random() % CODE_LETTERS.len() as u64) as usize;
                    CODE_LETTERS[i] as char
                })
                .collect();
            if !self.lobbies.contains_key(&code) {
                return code;
            }
        }
    }

    fn tell_lobby(&self, code: &str) {
        let Some(lobby) = self.lobbies.get(code) else {
            return;
        };
        let people: Vec<Person> = lobby
            .members
            .iter()
            .map(|id| Person {
                name: self
                    .clients
                    .get(id)
                    .and_then(|c| c.name.clone())
                    .unwrap_or_default(),
                god: lobby.picks.get(id).copied(),
            })
            .collect();
        let owner = lobby
            .members
            .iter()
            .position(|&m| m == lobby.owner)
            .unwrap_or(0);
        for (you, &member) in lobby.members.iter().enumerate() {
            self.send(
                member,
                ServerMsg::Lobby(LobbyInfo {
                    code: code.to_string(),
                    you,
                    owner,
                    people: people.clone(),
                    oracle: self.oracle.as_ref().map(|_| "локальная модель".to_string()),
                }),
            );
        }
    }

    fn start(&mut self, code: &str) {
        let seed = self.next_random();
        let salt = self.next_random();
        let oracle = self.oracle.clone();
        let timers = self.timers;
        let tickets: Vec<u64> = (0..SEATS).map(|_| self.next_random()).collect();
        let Some(lobby) = self.lobbies.get_mut(code) else {
            return;
        };
        if lobby.running.is_some() {
            return;
        }
        // Picked gods first, then the rest take free gods in joining order.
        let mut taken: Vec<God> = lobby.picks.values().copied().collect();
        let mut seats_of = BTreeMap::new();
        for &member in &lobby.members {
            let god = match lobby.picks.get(&member) {
                Some(&g) => g,
                None => {
                    let g = *God::ALL
                        .iter()
                        .find(|g| !taken.contains(g))
                        .expect("five gods for five seats");
                    taken.push(g);
                    g
                }
            };
            seats_of.insert(member, PlayerId(god.index() as u8));
        }
        let mut seats = vec![Seat::Bot; SEATS];
        for seat in seats_of.values() {
            seats[seat.0 as usize] = Seat::Human;
        }
        let table = Table::new(Config {
            seed,
            champions: God::ALL.to_vec(),
            seats,
            salt,
            oracle,
            timers,
        });
        let handed: Vec<(u64, PlayerId, u64)> = seats_of
            .iter()
            .zip(tickets)
            .map(|((&member, &seat), ticket)| (member, seat, ticket))
            .collect();
        lobby.running = Some(Running {
            table,
            seats: seats_of,
            tickets: handed.iter().map(|&(_, seat, t)| (t, seat)).collect(),
            empty_for: 0.0,
        });
        for (member, seat, ticket) in handed {
            self.send(member, ServerMsg::Started { seat, ticket });
        }
    }

    /// Someone who lost the connection sits back down by ticket.
    fn rejoin(&mut self, id: u64, code: &str, ticket: u64) {
        let code = normalize_code(code);
        let Some(running) = self.lobbies.get_mut(&code).and_then(|l| l.running.as_mut()) else {
            self.refuse(id, "этой партии больше нет");
            return;
        };
        let Some(&seat) = running.tickets.get(&ticket) else {
            self.refuse(id, "это место не твоё");
            return;
        };
        // A stale connection on the same seat gives way to the new one.
        running.seats.retain(|_, s| *s != seat);
        running.seats.insert(id, seat);
        running.table.set_seat(seat, Seat::Human);
        if let Some(c) = self.clients.get_mut(&id) {
            c.lobby = Some(code);
        }
        self.send(id, ServerMsg::Started { seat, ticket });
    }

    /// Leave the lobby this client is in, if any.
    fn leave(&mut self, id: u64) {
        let Some(code) = self.lobby_of(id) else {
            return;
        };
        if let Some(c) = self.clients.get_mut(&id) {
            c.lobby = None;
        }
        let Some(lobby) = self.lobbies.get_mut(&code) else {
            return;
        };
        lobby.members.retain(|&m| m != id);
        lobby.picks.remove(&id);
        if let Some(running) = lobby.running.as_mut()
            && let Some(seat) = running.seats.remove(&id)
        {
            // Someone left mid-match: a bot takes the seat (§17.1).
            running.table.set_seat(seat, Seat::Bot);
        }
        // A match waits for its people to come back (`step` gives up after
        // `ABANDON_SECS`); a lobby nobody sits in goes at once.
        if lobby.running.is_none() && lobby.members.is_empty() {
            self.lobbies.remove(&code);
            return;
        }
        if lobby.owner == id
            && let Some(&first) = lobby.members.first()
        {
            lobby.owner = first;
        }
        if lobby.running.is_none() {
            self.tell_lobby(&code);
        }
    }

    fn drop_client(&mut self, id: u64) {
        self.leave(id);
        self.clients.remove(&id);
    }
}

#[cfg(test)]
mod tests;
