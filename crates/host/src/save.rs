//! Keeping a match on disk and taking it up again (docs/design.md §17.5).
//!
//! A match lives in a directory of its own: a `snapshot` of the whole
//! table, written at every dusk, and a `journal` of what happened since,
//! appended after every accepted intent. Loading replays the journal on
//! the snapshot; the rules are deterministic (§16), so the match comes back
//! exactly as it was, and a wish's `Said` rides in its intent, so the model
//! is never asked twice.
//!
//! The table itself does no I/O: it keeps its journal entries in memory
//! (`Table::keep_journal`) and a [`Saver`] writes them out.
//!
//! Crash safety: the snapshot is written to a temporary file and renamed
//! over the old one. The journal names the snapshot it follows
//! (`generation`), so a journal left from before a newer snapshot is
//! ignored, and an entry cut off halfway is dropped.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use necromy_rules::{Game, God, Intent, PlayerId};
use serde::{Deserialize, Serialize};

use crate::{Seat, Timers};

/// Bump whenever `Game` or anything else saved changes shape (together with
/// `necromy_net::PROTOCOL`): postcard is not self-describing, so an old save
/// would read as garbage.
pub const SAVE_VERSION: u32 = 27;

const MAGIC: &[u8; 8] = b"NECROSAV";
const SNAPSHOT: &str = "snapshot";
const JOURNAL: &str = "journal";

/// The whole table at one moment.
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    /// Counts snapshots of this match; the journal names the one it follows.
    pub generation: u64,
    pub game: Game,
    pub seats: Vec<Seat>,
    pub salt: u64,
    pub serial: u32,
    pub timers: Option<Timers>,
    /// Wishes a god was reading when the snapshot was taken.
    pub asked: Vec<(PlayerId, God, String)>,
}

/// What changed the table since the snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Entry {
    /// An intent the rules accepted, and the update serial it made.
    Act {
        seat: PlayerId,
        intent: Intent,
        serial: u32,
    },
    Seat {
        seat: PlayerId,
        kind: Seat,
    },
    /// A wish in words went to the model.
    Asked {
        seat: PlayerId,
        god: God,
        text: String,
    },
    /// The model's answer for that seat's wish came back (or failed).
    Heard {
        seat: PlayerId,
    },
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    /// Not a save, or one this build cannot read.
    Version(u32),
    Corrupt,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(err) => write!(f, "{err}"),
            LoadError::Version(v) => write!(
                f,
                "сохранение другой версии игры ({v}, эта читает {SAVE_VERSION})"
            ),
            LoadError::Corrupt => write!(f, "сохранение повреждено"),
        }
    }
}

impl From<io::Error> for LoadError {
    fn from(err: io::Error) -> LoadError {
        LoadError::Io(err)
    }
}

/// A saved match as read from disk: the snapshot and the journal after it.
pub struct Saved {
    pub snapshot: Snapshot,
    pub journal: Vec<Entry>,
}

/// Whether `dir` holds a save (it may still fail to load).
pub fn exists(dir: &Path) -> bool {
    dir.join(SNAPSHOT).is_file()
}

pub fn load(dir: &Path) -> Result<Saved, LoadError> {
    let bytes = fs::read(dir.join(SNAPSHOT))?;
    let body = header(&bytes)?;
    let snapshot: Snapshot = postcard::from_bytes(body).map_err(|_| LoadError::Corrupt)?;
    let journal = match fs::read(dir.join(JOURNAL)) {
        Ok(bytes) => read_journal(&bytes, snapshot.generation),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(err) => return Err(err.into()),
    };
    Ok(Saved { snapshot, journal })
}

/// Only the snapshot: enough to list a save (who, which day) without
/// replaying it.
pub fn peek(dir: &Path) -> Result<Snapshot, LoadError> {
    let bytes = fs::read(dir.join(SNAPSHOT))?;
    postcard::from_bytes(header(&bytes)?).map_err(|_| LoadError::Corrupt)
}

fn header(bytes: &[u8]) -> Result<&[u8], LoadError> {
    if bytes.len() < 12 || &bytes[..8] != MAGIC {
        return Err(LoadError::Corrupt);
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if version != SAVE_VERSION {
        return Err(LoadError::Version(version));
    }
    Ok(&bytes[12..])
}

/// The entries of a journal that follows snapshot `generation`; nothing if
/// it follows another. A last entry cut short (a crash mid-write) is left
/// out, and so is everything after an entry that does not read.
fn read_journal(bytes: &[u8], generation: u64) -> Vec<Entry> {
    let Ok(body) = header(bytes) else {
        return Vec::new();
    };
    if body.len() < 8 || u64::from_le_bytes(body[..8].try_into().unwrap()) != generation {
        return Vec::new();
    }
    let mut rest = &body[8..];
    let mut entries = Vec::new();
    while rest.len() >= 4 {
        let len = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
        let Some(frame) = rest.get(4..4 + len) else {
            break;
        };
        match postcard::from_bytes(frame) {
            Ok(entry) => entries.push(entry),
            Err(_) => break,
        }
        rest = &rest[4 + len..];
    }
    entries
}

/// Writes one match's table to its directory as it plays.
pub struct Saver {
    dir: PathBuf,
    generation: u64,
    journal: File,
}

impl Saver {
    /// Start keeping `table` in `dir`: a fresh snapshot at once, then the
    /// journal. For a table restored from `dir` this also compacts the save.
    pub fn start(dir: impl Into<PathBuf>, table: &mut crate::Table) -> io::Result<Saver> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        let generation = table.generation() + 1;
        table.keep_journal();
        let journal = write_snapshot(&dir, table, generation)?;
        Ok(Saver {
            dir,
            generation,
            journal,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Write out what happened at the table since the last call; at dusk,
    /// a new snapshot.
    pub fn persist(&mut self, table: &mut crate::Table) -> io::Result<()> {
        if table.take_snapshot_due() {
            self.snapshot(table)
        } else {
            self.append(&table.take_journal())
        }
    }

    /// Everything as it is now in one snapshot, the journal emptied.
    pub fn snapshot(&mut self, table: &mut crate::Table) -> io::Result<()> {
        table.take_journal();
        self.generation += 1;
        self.journal = write_snapshot(&self.dir, table, self.generation)?;
        Ok(())
    }

    fn append(&mut self, entries: &[Entry]) -> io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut buf = Vec::new();
        for entry in entries {
            let frame = postcard::to_stdvec(entry).map_err(io::Error::other)?;
            buf.extend_from_slice(&(frame.len() as u32).to_le_bytes());
            buf.extend_from_slice(&frame);
        }
        self.journal.write_all(&buf)?;
        self.journal.flush()
    }

    /// The match is over: nothing to come back to.
    pub fn discard(self) -> io::Result<()> {
        fs::remove_dir_all(&self.dir)
    }
}

/// Snapshot `table` as `generation` and open a journal that follows it.
fn write_snapshot(dir: &Path, table: &crate::Table, generation: u64) -> io::Result<File> {
    let snapshot = table.snapshot(generation);
    let mut bytes = Vec::from(&MAGIC[..]);
    bytes.extend_from_slice(&SAVE_VERSION.to_le_bytes());
    bytes.extend(postcard::to_stdvec(&snapshot).map_err(io::Error::other)?);
    replace(dir, SNAPSHOT, &bytes)?;

    let mut head = Vec::from(&MAGIC[..]);
    head.extend_from_slice(&SAVE_VERSION.to_le_bytes());
    head.extend_from_slice(&generation.to_le_bytes());
    replace(dir, JOURNAL, &head)?;
    OpenOptions::new().append(true).open(dir.join(JOURNAL))
}

/// Write `name` whole or not at all.
fn replace(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let tmp = dir.join(format!("{name}.tmp"));
    let mut file = File::create(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(tmp, dir.join(name))
}

#[cfg(test)]
mod tests {
    use necromy_rules::Event;

    use super::*;
    use crate::{BOT_STEP_SECS, Config, Table};

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("necromy-save-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn bots(seed: u64, seats: Vec<Seat>) -> Table {
        Table::new(Config {
            seed,
            champions: God::ALL.to_vec(),
            seats,
            salt: 1,
            oracle: None,
            timers: None,
            mode: Default::default(),
        })
    }

    fn bytes(table: &Table) -> Vec<u8> {
        postcard::to_stdvec(table.game()).unwrap()
    }

    /// Bots play `steps` steps, the saver keeping up.
    fn play(table: &mut Table, saver: &mut Saver, steps: usize) {
        for _ in 0..steps {
            if table.game().winner().is_some() {
                return;
            }
            table.tick(BOT_STEP_SECS);
            saver.persist(table).unwrap();
        }
    }

    #[test]
    fn a_saved_match_comes_back_exactly_and_plays_on_alike() {
        for seed in [3, 9, 21] {
            let dir = dir(&format!("exact-{seed}"));
            let mut table = bots(seed, vec![Seat::Bot; 5]);
            let mut saver = Saver::start(&dir, &mut table).unwrap();
            for _ in 0..3 {
                play(&mut table, &mut saver, 250);
                let back = Table::restore(load(&dir).unwrap(), None);
                assert_eq!(back.replay_stopped(), None, "seed {seed}");
                assert_eq!(bytes(&back), bytes(&table), "seed {seed}");
                assert_eq!(back.seats(), table.seats());
            }
            assert!(peek(&dir).unwrap().generation > 1, "no dusk snapshot");

            // The restored table and the one that never stopped go on the same.
            let mut back = Table::restore(load(&dir).unwrap(), None);
            let mut other = Saver::start(dir.join("again"), &mut back).unwrap();
            play(&mut table, &mut saver, 300);
            play(&mut back, &mut other, 300);
            assert_eq!(bytes(&back), bytes(&table), "seed {seed}");
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_torn_last_entry_is_dropped() {
        let dir = dir("torn");
        let mut table = bots(5, vec![Seat::Bot; 5]);
        let mut saver = Saver::start(&dir, &mut table).unwrap();
        // Early on, and not just after a dusk (a snapshot empties the journal).
        for _ in 0..30 {
            play(&mut table, &mut saver, 1);
            if table.game().round() >= 2 && !load(&dir).unwrap().journal.is_empty() {
                break;
            }
        }
        let whole = load(&dir).unwrap().journal.len();
        assert!(whole > 0);
        let mut journal = OpenOptions::new()
            .append(true)
            .open(dir.join(JOURNAL))
            .unwrap();
        journal.write_all(&[200, 0, 0, 0, 1, 2]).unwrap();
        let saved = load(&dir).unwrap();
        assert_eq!(saved.journal.len(), whole);
        assert_eq!(bytes(&Table::restore(saved, None)), bytes(&table));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_journal_older_than_the_snapshot_is_ignored() {
        let dir = dir("stale");
        let mut table = bots(5, vec![Seat::Bot; 5]);
        let mut saver = Saver::start(&dir, &mut table).unwrap();
        play(&mut table, &mut saver, 40);
        let old = fs::read(dir.join(JOURNAL)).unwrap();
        saver.snapshot(&mut table).unwrap();
        // A crash between the new snapshot and the new journal.
        fs::write(dir.join(JOURNAL), old).unwrap();
        let saved = load(&dir).unwrap();
        assert!(saved.journal.is_empty());
        assert_eq!(bytes(&Table::restore(saved, None)), bytes(&table));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_of_another_version_is_refused() {
        let dir = dir("version");
        let mut table = bots(5, vec![Seat::Bot; 5]);
        Saver::start(&dir, &mut table).unwrap();
        let mut snapshot = fs::read(dir.join(SNAPSHOT)).unwrap();
        snapshot[8..12].copy_from_slice(&(SAVE_VERSION + 1).to_le_bytes());
        fs::write(dir.join(SNAPSHOT), snapshot).unwrap();
        assert!(matches!(load(&dir), Err(LoadError::Version(v)) if v == SAVE_VERSION + 1));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn someone_away_makes_no_wish() {
        let mut table = bots(9, vec![Seat::Away; 5]);
        for _ in 0..3000 {
            if table.game().winner().is_some() || table.game().round() > 8 {
                break;
            }
            table.tick(BOT_STEP_SECS);
        }
        let log = table.game().log();
        assert!(!log.iter().any(|e| matches!(e, Event::WishGranted { .. })));
        assert!(log.iter().any(|e| matches!(e, Event::WishRefused { .. })));
    }
}
