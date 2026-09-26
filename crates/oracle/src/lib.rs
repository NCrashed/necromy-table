//! The gods' voice (docs/design.md §7.6, §8.6).
//!
//! A worker thread talks to a local `llama-server` so the game never waits
//! on the model: jobs go in, answers come out, the game polls once a frame.
//! Nothing here knows about Bevy; later the same crate runs on the dedicated
//! server (§17.2), where the key and the model live.
//!
//! The model never changes the game by itself. It reads free text into the
//! rules' closed sets (a wish, a rival, a grade) and writes words; the
//! rules apply everything else.

pub mod client;
pub mod prompt;

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

pub use client::{Message, alive};

/// Where `llama-server` listens unless `NECROMY_ORACLE` says otherwise; the
/// same default as the first-person game, so both can share one server.
pub const DEFAULT_ADDR: &str = "127.0.0.1:8080";

pub fn addr_from_env() -> String {
    std::env::var("NECROMY_ORACLE").unwrap_or_else(|_| DEFAULT_ADDR.to_string())
}

/// One request to the model.
pub struct Job {
    pub id: u64,
    pub messages: Vec<Message>,
    /// JSON schema the answer must follow; `None` for plain words.
    pub schema: Option<serde_json::Value>,
    pub max_tokens: u32,
    pub temperature: f32,
}

pub struct Answer {
    pub id: u64,
    pub result: Result<String, String>,
}

/// A worker thread with a queue in front of it. Jobs run one at a time, in
/// the order sent: a CPU server gains nothing from two at once.
pub struct Oracle {
    jobs: Sender<Job>,
    // Behind a lock only so the oracle can live in a Bevy resource (`Sync`).
    answers: Mutex<Receiver<Answer>>,
}

impl Oracle {
    pub fn spawn(addr: String) -> Oracle {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, answers) = channel();
        std::thread::Builder::new()
            .name("oracle".into())
            .spawn(move || {
                for job in inbox {
                    let result = client::chat(
                        &addr,
                        &job.messages,
                        job.schema.as_ref(),
                        job.max_tokens,
                        job.temperature,
                    );
                    if outbox.send(Answer { id: job.id, result }).is_err() {
                        return;
                    }
                }
            })
            .expect("spawn the oracle thread");
        Oracle {
            jobs,
            answers: Mutex::new(answers),
        }
    }

    pub fn send(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    /// The next finished answer, if any; never blocks.
    pub fn poll(&self) -> Option<Answer> {
        self.answers.lock().ok()?.try_recv().ok()
    }
}
