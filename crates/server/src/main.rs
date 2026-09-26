//! `necromy-server [--listen ADDR] [--no-oracle]`
//!
//! Listens on `0.0.0.0:7878` by default. The gods' voice is the
//! `llama-server` at `NECROMY_ORACLE` (default 127.0.0.1:8080); the host
//! pays for it, clients need no keys (docs/design.md §17.1).

use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use necromy_net::{Conn, DEFAULT_PORT, ServerConn};
use necromy_server::Server;

const STEP: Duration = Duration::from_millis(50);

fn main() -> std::io::Result<()> {
    let mut listen = format!("0.0.0.0:{DEFAULT_PORT}");
    let mut oracle = Some(necromy_oracle::addr_from_env());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => listen = args.next().expect("--listen needs an address"),
            "--no-oracle" => oracle = None,
            other => {
                eprintln!(
                    "unknown argument {other}; usage: necromy-server [--listen ADDR] [--no-oracle]"
                );
                std::process::exit(2);
            }
        }
    }
    let listener = TcpListener::bind(&listen)?;
    match &oracle {
        Some(addr) => eprintln!("necromy-server on {listen}; gods' voice: llama-server at {addr}"),
        None => eprintln!("necromy-server on {listen}; gods' voice off (templates)"),
    }

    let (arrivals, arrived) = channel::<ServerConn>();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            match Conn::new(stream) {
                Ok(conn) => {
                    if arrivals.send(conn).is_err() {
                        return;
                    }
                }
                Err(err) => eprintln!("connection failed: {err}"),
            }
        }
    });

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut server = Server::new(oracle, seed);
    let mut last = Instant::now();
    let mut lobbies = 0;
    loop {
        while let Ok(conn) = arrived.try_recv() {
            if let Some(peer) = conn.peer() {
                eprintln!("connected: {peer}");
            }
            server.accept(conn);
        }
        let now = Instant::now();
        server.step((now - last).as_secs_f32());
        last = now;
        if server.lobby_count() != lobbies {
            lobbies = server.lobby_count();
            eprintln!("tables open: {lobbies}");
        }
        std::thread::sleep(STEP);
    }
}
