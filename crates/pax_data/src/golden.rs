//! Determinism golden files.
//!
//! `golden.hashes` format: comment lines start with `#`; every other
//! line is `<day> <16 hex digit state hash>` for days `0..n` in order.

use std::fmt::Write as _;
use std::path::Path;

/// Default replay length: one in-game year.
pub const DEFAULT_DAYS: u64 = 365;

pub fn write(path: &Path, scenario: &str, hashes: &[u64]) -> std::io::Result<()> {
    let mut out = String::new();
    let _ = writeln!(out, "# Determinism golden file for scenario '{scenario}'.");
    let _ = writeln!(out, "# Regenerate only for intended behaviour changes: cargo run -p pax_cli -- record <dir>");
    for (day, h) in hashes.iter().enumerate() {
        let _ = writeln!(out, "{day} {h:016x}");
    }
    std::fs::write(path, out)
}

pub fn read(path: &Path) -> Result<Vec<u64>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut hashes = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (day, hash) = line.split_once(' ').ok_or(format!("line {}: expected '<day> <hash>'", n + 1))?;
        let day: usize = day.parse().map_err(|_| format!("line {}: bad day", n + 1))?;
        if day != hashes.len() {
            return Err(format!("line {}: expected day {}, found {day}", n + 1, hashes.len()));
        }
        hashes.push(u64::from_str_radix(hash, 16).map_err(|_| format!("line {}: bad hash", n + 1))?);
    }
    Ok(hashes)
}

/// The minimum length of a golden file for a scenario (D11): at least
/// [`DEFAULT_DAYS`], and past the last logged command, so every command is
/// pinned. `pax_cli record` refuses to write less; `pax_cli verify` rejects less.
pub fn min_days(log: &crate::CommandLog) -> u64 {
    log.last_day().map_or(DEFAULT_DAYS, |last| DEFAULT_DAYS.max(last + 1))
}

/// Length of an existing golden file: `Ok(None)` if there is none yet; any
/// other read or parse problem is an error, never a silent default.
pub fn existing_len(path: &Path) -> Result<Option<u64>, String> {
    if !path.exists() {
        return Ok(None);
    }
    read(path).map(|h| Some(h.len() as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_days_covers_the_command_log() {
        assert_eq!(min_days(&crate::CommandLog::default()), DEFAULT_DAYS);
        let world =
            crate::load_scenario(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states"))
                .expect("two_states loads");
        assert_eq!(min_days(&world.commands), 721, "two_states' last command is on day 720");
    }
}
