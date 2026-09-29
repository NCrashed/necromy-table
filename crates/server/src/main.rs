//! `necromy-server [--listen ADDR] [--no-oracle] [--turn S] [--window S] [--wish S] [--no-timers]
//! [--saves DIR | --no-saves] [--keep-days N]`
//!
//! Listens on `0.0.0.0:7878` by default. The gods' voice is the
//! `llama-server` at `NECROMY_ORACLE` (default 127.0.0.1:8080); the host
//! pays for it, clients need no keys (docs/design.md §17.1).
//!
//! Matches are kept in `--saves` (default `$STATE_DIRECTORY`, which systemd
//! sets for a unit with `StateDirectory=`, else `./necromy-tables`), so a
//! restart loses none; one nobody has played for `--keep-days` (30) goes.

use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use necromy_net::{Conn, DEFAULT_PORT, ServerConn};
use necromy_server::Server;

const STEP: Duration = Duration::from_millis(50);
/// How often kept matches are checked for age.
const SWEEP_EVERY: Duration = Duration::from_secs(3600);

fn main() -> std::io::Result<()> {
    let mut listen = format!("0.0.0.0:{DEFAULT_PORT}");
    let mut oracle = Some(necromy_oracle::addr_from_env());
    let mut timers = Some(necromy_host::Timers::default());
    let mut saves = Some(
        std::env::var_os("STATE_DIRECTORY")
            .map_or_else(|| "necromy-tables".into(), std::path::PathBuf::from),
    );
    let mut keep_days: u64 = 30;
    let secs = |v: Option<String>, what: &str| -> f32 {
        v.and_then(|s| s.parse().ok())
            .unwrap_or_else(|| panic!("{what} needs seconds"))
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => listen = args.next().expect("--listen needs an address"),
            "--no-oracle" => oracle = None,
            "--no-timers" => timers = None,
            "--saves" => saves = Some(args.next().expect("--saves needs a directory").into()),
            "--no-saves" => saves = None,
            "--keep-days" => {
                keep_days = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .expect("--keep-days needs a number")
            }
            "--turn" => {
                timers.get_or_insert_with(Default::default).turn = secs(args.next(), "--turn")
            }
            "--window" => {
                timers.get_or_insert_with(Default::default).window = secs(args.next(), "--window")
            }
            "--wish" => {
                timers.get_or_insert_with(Default::default).wish = secs(args.next(), "--wish")
            }
            other => {
                eprintln!(
                    "unknown argument {other}; usage: necromy-server [--listen ADDR] [--no-oracle] [--turn S] [--window S] [--wish S] [--no-timers] [--saves DIR | --no-saves] [--keep-days N]"
                );
                std::process::exit(2);
            }
        }
    }
    let listener = TcpListener::bind(&listen)?;
    match &oracle {
        Some(addr) => {
            eprintln!("necromy-server on {listen}; gods' voice: llama-server at {addr}");
            // Probe from the start, not from the first table: the journal
            // tells the voice's state (and every switch to a spare) at once.
            necromy_oracle::Route::shared(addr);
        }
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
    server.timers = timers;
    server.saves = saves.clone();
    let keep = Duration::from_secs(keep_days * 24 * 3600);
    match &saves {
        Some(dir) => eprintln!("matches kept in {}", dir.display()),
        None => eprintln!("matches not kept"),
    }
    server.sweep(keep);
    let mut swept = Instant::now();
    match timers {
        Some(t) => eprintln!(
            "clocks: turn {}s, window {}s, wish {}s",
            t.turn, t.window, t.wish
        ),
        None => eprintln!("clocks off"),
    }
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
        if swept.elapsed() > SWEEP_EVERY {
            server.sweep(keep);
            swept = Instant::now();
        }
        std::thread::sleep(STEP);
    }
}
