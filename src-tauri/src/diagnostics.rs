// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Opt-in local crash logging (Phase 13).
//!
//! Nothing is recorded unless the user enables it. Enabling creates a marker file in the app log
//! directory; while the marker exists, a panic appends its message and a backtrace to
//! `crash.log` in that directory. Reports never leave the machine.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MARKER: &str = "crash-logging.enabled";
const LOG_FILE: &str = "crash.log";

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

fn marker(dir: &Path) -> PathBuf {
    dir.join(MARKER)
}

/// Whether crash logging is switched on in `dir`.
pub fn is_enabled(dir: &Path) -> bool {
    marker(dir).is_file()
}

/// Turns crash logging on or off. Creates `dir` when enabling.
pub fn set_enabled(dir: &Path, enabled: bool) -> std::io::Result<()> {
    if enabled {
        std::fs::create_dir_all(dir)?;
        std::fs::write(marker(dir), b"opt-in: local crash reports only\n")
    } else {
        match std::fs::remove_file(marker(dir)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// Appends one report to `crash.log` if logging is enabled. Returns whether a report was written.
pub fn write_report(dir: &Path, message: &str, backtrace: &str) -> std::io::Result<bool> {
    if !is_enabled(dir) {
        return Ok(false);
    }
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(LOG_FILE))?;
    writeln!(file, "--- panic ---\n{message}\n{backtrace}\n")?;
    Ok(true)
}

/// Installs the panic hook. The hook only writes when the user has opted in, and always chains
/// to the previous hook so normal panic output still appears.
pub fn install(dir: PathBuf) {
    let _ = LOG_DIR.set(dir);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = LOG_DIR.get() {
            let message = info.to_string();
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            // Failing to write a crash report must never cause a second panic.
            let _ = write_report(dir, &message, &backtrace);
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tpt-diag-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn logging_is_off_by_default_and_writes_nothing() {
        let dir = temp("off");
        assert!(!is_enabled(&dir));
        assert!(!write_report(&dir, "boom", "bt").unwrap());
        assert!(!dir.join(LOG_FILE).exists());
    }

    #[test]
    fn enabling_writes_reports_and_disabling_stops_them() {
        let dir = temp("on");
        set_enabled(&dir, true).unwrap();
        assert!(is_enabled(&dir));
        assert!(write_report(&dir, "first", "trace").unwrap());

        set_enabled(&dir, false).unwrap();
        assert!(!write_report(&dir, "second", "trace").unwrap());

        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("first"));
        assert!(!log.contains("second"));
    }

    #[test]
    fn disabling_when_never_enabled_is_not_an_error() {
        let dir = temp("never");
        assert!(set_enabled(&dir, false).is_ok());
    }
}
