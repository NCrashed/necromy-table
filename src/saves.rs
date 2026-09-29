//! Single player matches kept on disk (docs/design.md §17.5): each in its
//! own directory under `state_dir()/saves`, written as it plays by a
//! `necromy_host::save::Saver`, and offered as «Продолжить» in the menu.
//! A finished match is deleted; only the newest few are kept.
//!
//! Dev runs (`NECROMY_SEED`, `NECROMY_AUTOPLAY`, `NECROMY_SCREENSHOT`) and
//! `NECROMY_SAVE=off` keep nothing.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use necromy_host::save::{self, Saver};
use necromy_host::{Seat, Table};
use necromy_rules::PlayerId;

/// Saved matches kept; older ones go when a new one starts.
const KEEP: usize = 5;

fn root() -> Option<PathBuf> {
    Some(crate::state_dir()?.join("saves"))
}

fn enabled() -> bool {
    let dev = ["NECROMY_SEED", "NECROMY_AUTOPLAY", "NECROMY_SCREENSHOT"]
        .iter()
        .any(|v| std::env::var_os(v).is_some());
    !dev && std::env::var("NECROMY_SAVE").as_deref() != Ok("off")
}

/// Keep a new match: a directory named by the time, so names sort by age.
pub fn start(table: &mut Table) -> Option<Saver> {
    if !enabled() {
        return None;
    }
    let root = root()?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    prune(&root, KEEP - 1);
    keep(root.join(format!("{secs:012}")), table)
}

/// Go on keeping a match in `dir` (a resumed one).
pub fn keep(dir: PathBuf, table: &mut Table) -> Option<Saver> {
    match Saver::start(&dir, table) {
        Ok(saver) => Some(saver),
        Err(err) => {
            warn!("cannot save the match in {}: {err}", dir.display());
            None
        }
    }
}

/// Saved matches, newest first.
fn all(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| save::exists(p))
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs.reverse();
    dirs
}

fn prune(root: &Path, keep: usize) {
    for old in all(root).into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(old);
    }
}

/// The newest saved match and a line about it for the menu.
pub fn latest() -> Option<(PathBuf, String)> {
    if std::env::var("NECROMY_SAVE").as_deref() == Ok("off") {
        return None;
    }
    all(&root()?).into_iter().find_map(|dir| {
        let snapshot = save::peek(&dir).ok()?;
        let human = human_of(&snapshot.seats)?;
        let god = snapshot.game.champion(human)?.god;
        let line = format!(
            "{}, день {}",
            crate::names::god(god),
            snapshot.game.round().div_ceil(2)
        );
        Some((dir, line))
    })
}

/// The seat a person plays in a saved match.
pub fn human_of(seats: &[Seat]) -> Option<PlayerId> {
    seats
        .iter()
        .position(|s| matches!(s, Seat::Human | Seat::Autoplay { .. }))
        .map(|i| PlayerId(i as u8))
}
