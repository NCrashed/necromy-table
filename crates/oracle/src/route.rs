//! Where the gods' voice answers from: a list of `llama-server` addresses
//! in order of preference (`NECROMY_ORACLE=main:port,spare:port`). One
//! thread per list and process probes them every few seconds and routes
//! every job to the first that answers, so a dead main voice falls over
//! to a spare and comes back by itself.
//!
//! Every change is one line on stderr (the server's journal), marked so it
//! is easy to find and to alert on: `gods' voice: SPARE ...`, `SILENT ...`,
//! `back on ...`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Seconds between probes.
const PROBE_SECS: u64 = 5;
/// `now` when no address answers.
const NONE: usize = usize::MAX;

/// Which state the voice is in, as the game tells players.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    /// Nothing answers: prepared wishes and template words only.
    Silent,
    /// The first address answers.
    Main,
    /// The first is down; a later one answers.
    Spare,
}

pub struct Route {
    addrs: Vec<String>,
    now: AtomicUsize,
}

impl Route {
    /// The addresses of a comma-separated list, in order; blanks dropped.
    pub fn parse(list: &str) -> Route {
        let addrs: Vec<String> = list
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(String::from)
            .collect();
        Route {
            addrs: if addrs.is_empty() {
                vec![crate::DEFAULT_ADDR.to_string()]
            } else {
                addrs
            },
            now: AtomicUsize::new(NONE),
        }
    }

    /// The route for `list` in this process, probed by its own thread from
    /// the first call on: every table of a server shares it, so a change is
    /// reported once.
    pub fn shared(list: &str) -> Arc<Route> {
        static ROUTES: Mutex<Vec<Arc<Route>>> = Mutex::new(Vec::new());
        let wanted = Route::parse(list);
        let mut routes = ROUTES.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(route) = routes.iter().find(|r| r.addrs == wanted.addrs) {
            return route.clone();
        }
        let route = Arc::new(wanted);
        routes.push(route.clone());
        let probed = route.clone();
        std::thread::Builder::new()
            .name("oracle-probe".into())
            .spawn(move || {
                let mut first = true;
                loop {
                    let (was, is) = probed.probe();
                    if first || was != is {
                        eprintln!("{}", probed.describe(was, is, first));
                    }
                    first = false;
                    std::thread::sleep(Duration::from_secs(PROBE_SECS));
                }
            })
            .expect("spawn the oracle probe");
        route
    }

    pub fn addrs(&self) -> &[String] {
        &self.addrs
    }

    /// The address answering now, if any.
    pub fn now(&self) -> Option<usize> {
        match self.now.load(Ordering::Relaxed) {
            NONE => None,
            i => Some(i),
        }
    }

    pub fn voice(&self) -> Voice {
        match self.now() {
            None => Voice::Silent,
            Some(0) => Voice::Main,
            Some(_) => Voice::Spare,
        }
    }

    /// Where a job goes: the address answering now, else the first.
    pub fn addr(&self) -> &str {
        &self.addrs[self.now().unwrap_or(0)]
    }

    /// Ask each address in order; the first that answers carries the voice.
    /// Returns the index before and after.
    pub fn probe(&self) -> (Option<usize>, Option<usize>) {
        let is = self.addrs.iter().position(|a| crate::alive(a));
        let was = self.now();
        self.set(is);
        (was, is)
    }

    fn set(&self, now: Option<usize>) {
        self.now.store(now.unwrap_or(NONE), Ordering::Relaxed);
    }

    /// The journal line for a change from `was` to `is`.
    fn describe(&self, was: Option<usize>, is: Option<usize>, first: bool) -> String {
        let main = &self.addrs[0];
        match is {
            Some(0) if first || was.is_none() => format!("gods' voice: on {main}"),
            Some(0) => format!("gods' voice: back on {main}"),
            Some(i) => format!(
                "gods' voice: SPARE {} ({main} does not answer)",
                self.addrs[i]
            ),
            None => format!(
                "gods' voice: SILENT, none of {} answers; prepared wishes only",
                self.addrs.join(", ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_parses_in_order_and_blank_means_the_default() {
        let r = Route::parse(" 127.0.0.1:8081 , ,127.0.0.1:8080");
        assert_eq!(r.addrs(), ["127.0.0.1:8081", "127.0.0.1:8080"]);
        assert_eq!(Route::parse("").addrs(), [crate::DEFAULT_ADDR]);
    }

    #[test]
    fn the_voice_follows_the_first_that_answers() {
        let r = Route::parse("a:1,b:2");
        assert_eq!(r.voice(), Voice::Silent);
        assert_eq!(r.addr(), "a:1");
        r.set(Some(1));
        assert_eq!((r.voice(), r.addr()), (Voice::Spare, "b:2"));
        assert!(r.describe(Some(0), Some(1), false).contains("SPARE b:2"));
        r.set(Some(0));
        assert_eq!((r.voice(), r.addr()), (Voice::Main, "a:1"));
        assert!(r.describe(Some(1), Some(0), false).contains("back on a:1"));
        assert!(r.describe(Some(0), None, false).contains("SILENT"));
    }

    #[test]
    fn nothing_listening_is_silent() {
        // Port 9 on loopback: nothing serves it in a test sandbox.
        let r = Route::parse("127.0.0.1:9");
        assert_eq!(r.probe(), (None, None));
        assert_eq!(r.voice(), Voice::Silent);
    }
}
