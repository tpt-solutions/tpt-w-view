// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Filesystem watcher for live directory updates (Phase 3).
//!
//! Watches one directory (non-recursively) and reports supported image files as they are added,
//! modified or removed. Events are coalesced per path within a single poll so that a file being
//! written in several steps produces one notification.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Duration;

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::error::{Error, Result};
use crate::scan::is_supported;

/// A change to an image file in the watched directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirEvent {
    /// A new supported file appeared (or was renamed into the directory).
    Added(PathBuf),
    /// An existing supported file's contents or metadata changed.
    Modified(PathBuf),
    /// A supported file was removed or renamed away.
    Removed(PathBuf),
}

impl DirEvent {
    pub fn path(&self) -> &Path {
        match self {
            DirEvent::Added(p) | DirEvent::Modified(p) | DirEvent::Removed(p) => p,
        }
    }
}

/// Keeps the OS watch alive for as long as it is held.
pub struct DirWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
    dir: PathBuf,
}

impl DirWatcher {
    /// Starts watching `dir`. The directory must exist.
    pub fn start(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        if !dir.is_dir() {
            return Err(Error::Invalid(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        let (tx, rx): (Sender<_>, Receiver<_>) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            // The receiver going away just means the watcher is being dropped.
            let _ = tx.send(res);
        })
        .map_err(to_io)?;
        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(to_io)?;
        Ok(Self {
            _watcher: watcher,
            rx,
            dir,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Returns every pending change without blocking, coalesced to the latest state per path.
    pub fn poll(&self) -> Vec<DirEvent> {
        let mut raw = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(Ok(event)) => raw.push(event),
                Ok(Err(_)) => {} // Backend errors are transient; the next event resyncs state.
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        coalesce(raw.iter().flat_map(classify))
    }

    /// Waits up to `timeout` for at least one change, then returns all pending changes.
    pub fn wait(&self, timeout: Duration) -> Vec<DirEvent> {
        match self.rx.recv_timeout(timeout) {
            Ok(Ok(first)) => {
                let mut events: Vec<DirEvent> = classify(&first).collect();
                events.extend(self.poll());
                coalesce(events)
            }
            _ => Vec::new(),
        }
    }
}

fn to_io(err: notify::Error) -> Error {
    Error::Io(std::io::Error::other(err))
}

/// Maps one raw notify event to zero or more image-file changes.
fn classify(event: &Event) -> impl Iterator<Item = DirEvent> + '_ {
    let kind = event.kind;
    event
        .paths
        .iter()
        .filter(|p| is_supported(p))
        .filter_map(move |path| {
            let exists = path.is_file();
            let mapped = match kind {
                EventKind::Create(_) if exists => DirEvent::Added(path.clone()),
                EventKind::Modify(ModifyKind::Name(_)) => {
                    if exists {
                        DirEvent::Added(path.clone())
                    } else {
                        DirEvent::Removed(path.clone())
                    }
                }
                EventKind::Modify(_) if exists => DirEvent::Modified(path.clone()),
                EventKind::Remove(_) => DirEvent::Removed(path.clone()),
                EventKind::Modify(_) => DirEvent::Removed(path.clone()),
                _ => return None,
            };
            Some(mapped)
        })
}

/// Keeps one event per path, preferring the most recent state. Order follows first appearance.
fn coalesce(events: impl IntoIterator<Item = DirEvent>) -> Vec<DirEvent> {
    let mut order: Vec<PathBuf> = Vec::new();
    let mut latest: HashMap<PathBuf, DirEvent> = HashMap::new();
    for ev in events {
        let key = ev.path().to_path_buf();
        if !latest.contains_key(&key) {
            order.push(key.clone());
        }
        // A file added then modified in the same batch stays "Added" to consumers.
        let merged = match (latest.get(&key), &ev) {
            (Some(DirEvent::Added(_)), DirEvent::Modified(_)) => DirEvent::Added(key.clone()),
            _ => ev,
        };
        latest.insert(key, merged);
    }
    order
        .into_iter()
        .filter_map(|p| latest.remove(&p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesce_keeps_latest_state_per_path() {
        let p = PathBuf::from("a.png");
        let q = PathBuf::from("b.png");
        let out = coalesce(vec![
            DirEvent::Added(p.clone()),
            DirEvent::Modified(p.clone()),
            DirEvent::Removed(q.clone()),
            DirEvent::Added(q.clone()),
        ]);
        assert_eq!(out, vec![DirEvent::Added(p), DirEvent::Added(q)]);
    }

    #[test]
    fn coalesce_reports_removal_after_modification() {
        let p = PathBuf::from("c.jpg");
        let out = coalesce(vec![
            DirEvent::Modified(p.clone()),
            DirEvent::Removed(p.clone()),
        ]);
        assert_eq!(out, vec![DirEvent::Removed(p)]);
    }

    #[test]
    fn start_rejects_missing_directory() {
        assert!(DirWatcher::start(std::env::temp_dir().join("tpt-no-such-dir-xyz")).is_err());
    }

    #[test]
    fn reports_new_and_removed_images() {
        let dir = std::env::temp_dir().join(format!("tpt-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let watcher = DirWatcher::start(&dir).unwrap();

        let file = dir.join("new.png");
        std::fs::write(&file, b"placeholder bytes").unwrap();
        std::fs::write(dir.join("ignored.txt"), b"not an image").unwrap();

        let mut seen_added = false;
        for _ in 0..40 {
            for ev in watcher.wait(Duration::from_millis(100)) {
                assert!(ev.path().extension().is_some_and(|e| e == "png"), "{ev:?}");
                if matches!(ev, DirEvent::Added(_)) {
                    seen_added = true;
                }
            }
            if seen_added {
                break;
            }
        }
        assert!(seen_added, "expected an Added event for new.png");

        std::fs::remove_file(&file).unwrap();
        let mut seen_removed = false;
        for _ in 0..40 {
            if watcher
                .wait(Duration::from_millis(100))
                .iter()
                .any(|e| matches!(e, DirEvent::Removed(p) if p.ends_with("new.png")))
            {
                seen_removed = true;
                break;
            }
        }
        assert!(seen_removed, "expected a Removed event for new.png");
    }
}
