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
