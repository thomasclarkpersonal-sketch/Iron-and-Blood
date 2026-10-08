//! Save files (D23): a game is its scenario plus the log of every command that
//! applied, so a save is that log, plus checkpoints that prove a replay reached the
//! same state.
//!
//! ```toml
//! format = 1
//! scenario = "scenarios/two_states"   # as the server was started
//! content_hash = "0x9a1c…"            # Scenario::content_hash, hex (TOML ints are i64)
//! day = 360                           # the day the game was saved on
//!
//! [[checkpoint]]                      # World::state_hash once world.day reached `day`
//! day = 30
//! state_hash = "0x51f0…"
//!
//! [[command]]                         # as in commands.toml (DATA_FORMAT), plus `player`
//! day = 12
//! player = 0                          # absent: the scenario's own scripted command
//! type = "set_income_tax"
//! nation = "lowland_kingdom"
//! rate = 0.150000
//! ```
//!
//! **Snapshot** (M3-6b, D10): next to `<name>.toml`, the save writes `<name>.world`, a
//! binary snapshot of the whole world on the saved day (`crate::snapshot`). The TOML
//! records it:
//!
//! ```toml
//! snapshot = "first_war.world"
//! snapshot_hash = "0x2c4e…"           # World::state_hash of the snapshot
//! ```
//!
//! **Loading** ([`load`]) reloads the scenario, whose content hash must match, then
//! reads the snapshot. It never replays, so it is fast at any game length. The
//! snapshot must be the one the save names: its day and state hash must match. A
//! save that names a missing or damaged snapshot is an error, never a silent
//! fallback.
//!
//! **Replaying** ([`load_by_replay`]) re-applies every logged command on its day,
//! verifies every checkpoint, and, if the save has a snapshot, checks that the
//! replay ends exactly at it. It is the determinism check (M3-9), and it also loads
//! saves written without a snapshot.
//!
//! The log includes the scenario's scripted commands for the days played. Scripted
//! commands for later days still come from the scenario, so nothing applies twice.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use pax_engine::{Command, DayReport, World};
use serde::Deserialize;

use crate::schema::Dec;
use crate::{CommandLog, LoadError, Scenario, command_of, describe_command, load_scenario, parse, read};

/// Version of the save format; a different version is refused.
pub const SAVE_FORMAT: u32 = 1;

/// Days between state-hash checkpoints (D23). The server records one whenever
/// `world.day` reaches a multiple of this, and sends it in that day's `DayUpdate` (D22).
pub const CHECKPOINT_DAYS: u64 = 30;

/// A command that applied, as the game's history records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedCommand {
    /// The day at whose start it applied.
    pub day: u64,
    /// `None` for the scenario's own scripted commands (`commands.toml`).
    pub player: Option<u16>,
    pub command: Command,
}

/// Everything a save file holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveData {
    /// The scenario directory, as the server was started with it.
    pub scenario: PathBuf,
    pub content_hash: u64,
    /// The day the game was saved on: the save replays days `0..day`.
    pub day: u64,
    /// `(day, World::state_hash)` pairs, in day order.
    pub checkpoints: Vec<(u64, u64)>,
    /// Every applied command, in application order.
    pub commands: Vec<SavedCommand>,
}

/// TOML basic string for `s` (quoted and escaped).
fn string(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

impl SaveData {
    /// The save as TOML text. `world` supplies the nation keys commands are written
    /// with; `snapshot` names the snapshot file and its state hash, if one was written.
    pub fn to_toml(&self, world: &World, snapshot: Option<(&str, u64)>) -> String {
        let mut out = String::from("# Iron and Blood save game (D23). Load it with the server's LoadGame.\n");
        let _ = writeln!(out, "format = {SAVE_FORMAT}");
        let _ = writeln!(out, "scenario = {}", string(&self.scenario.to_string_lossy()));
        let _ = writeln!(out, "content_hash = \"{:#018x}\"", self.content_hash);
        let _ = writeln!(out, "day = {}", self.day);
        if let Some((file, hash)) = snapshot {
            let _ = writeln!(out, "snapshot = {}\nsnapshot_hash = \"{hash:#018x}\"", string(file));
        }
        for &(day, hash) in &self.checkpoints {
            let _ = write!(out, "\n[[checkpoint]]\nday = {day}\nstate_hash = \"{hash:#018x}\"\n");
        }
        for c in &self.commands {
            let (kind, nation, rate) = describe_command(&c.command);
            let _ = write!(out, "\n[[command]]\nday = {}\n", c.day);
            if let Some(player) = c.player {
                let _ = writeln!(out, "player = {player}");
            }
            let _ = writeln!(out, "type = \"{kind}\"\nnation = {}\nrate = {rate}", string(&world.nations.key[nation]));
        }
        out
    }

    /// Writes `<name>.world` (the snapshot of `world`, which must be on `self.day`),
    /// then `<name>.toml` naming it. Both writes are atomic, and the TOML is written
    /// last, so a reader never sees a save whose snapshot isn't complete.
    pub fn write(&self, path: &Path, world: &World) -> std::io::Result<()> {
        assert_eq!(world.day, self.day, "a save's snapshot is of the saved day");
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let snapshot_path = path.with_extension("world");
        let hash = crate::snapshot::write(&snapshot_path, world, self.content_hash)?;
        let file = snapshot_path.file_name().expect("a file path").to_string_lossy().into_owned();
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, self.to_toml(world, Some((&file, hash))))?;
        std::fs::rename(&tmp, path)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveFile {
    format: u32,
    scenario: String,
    content_hash: String,
    day: u64,
    snapshot: Option<String>,
    snapshot_hash: Option<String>,
    #[serde(default)]
    checkpoint: Vec<CheckpointEntry>,
    #[serde(default)]
    command: Vec<SaveCommandEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointEntry {
    day: u64,
    state_hash: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveCommandEntry {
    day: u64,
    player: Option<u16>,
    #[serde(rename = "type")]
    kind: String,
    nation: String,
    rate: Dec,
}

fn hex(field: &str, text: &str) -> Result<u64, String> {
    text.strip_prefix("0x")
        .and_then(|h| u64::from_str_radix(h, 16).ok())
        .ok_or_else(|| format!("{field} '{text}' is not a hex number like \"0x00ff\""))
}

/// A loaded save: the world at the saved day, and the history that reached it.
pub struct LoadedSave {
    /// The scenario with its world advanced to `save.day`. Its own command log still
    /// holds the scripted commands for later days.
    pub scenario: Scenario,
    pub save: SaveData,
    /// The report of the last replayed day: `None` when loaded from a snapshot (a
    /// report isn't state) or for a day-0 save.
    pub last_report: Option<DayReport>,
}

/// Loads a save from its snapshot, without replaying (see the module docs).
pub fn load(path: &Path) -> Result<LoadedSave, LoadError> {
    load_with(path, false)
}

/// Loads a save by replaying its whole history, verifying every checkpoint, and
/// checking the end state against the snapshot if there is one (the determinism check).
pub fn load_by_replay(path: &Path) -> Result<LoadedSave, LoadError> {
    load_with(path, true)
}

fn load_with(path: &Path, replay: bool) -> Result<LoadedSave, LoadError> {
    let file: SaveFile = parse(&path.display().to_string(), &read(path)?)?;
    if file.format != SAVE_FORMAT {
        return Err(LoadError::single(format!(
            "save format {} is not supported (expected {SAVE_FORMAT})",
            file.format
        )));
    }
    let content_hash = hex("content_hash", &file.content_hash).map_err(LoadError::single)?;
    let scenario_dir = PathBuf::from(&file.scenario);
    let mut scenario = load_scenario(&scenario_dir)?;
    if scenario.content_hash != content_hash {
        return Err(LoadError::single(format!(
            "the scenario's files changed since this game was saved (content hash {:#018x}, the save expects {content_hash:#018x})",
            scenario.content_hash
        )));
    }

    let mut errors = Vec::new();
    let mut commands = Vec::with_capacity(file.command.len());
    for (i, c) in file.command.iter().enumerate() {
        let ctx = format!("command #{} (day {})", i + 1, c.day);
        if c.day >= file.day {
            errors.push(format!("{ctx}: after the saved day {}", file.day));
        } else if commands.last().is_some_and(|prev: &SavedCommand| prev.day > c.day) {
            errors.push(format!("{ctx}: out of day order"));
        }
        let world = &scenario.world;
        match world.nations.key.iter().position(|k| *k == c.nation) {
            None => errors.push(format!("{ctx}: unknown nation '{}'", c.nation)),
            Some(nation) => match command_of(world, &c.kind, nation, c.rate.0) {
                Ok(command) => commands.push(SavedCommand { day: c.day, player: c.player, command }),
                Err(e) => errors.push(format!("{ctx}: {e}")),
            },
        }
    }
    let mut checkpoints = Vec::with_capacity(file.checkpoint.len());
    for c in &file.checkpoint {
        match hex("state_hash", &c.state_hash) {
            Ok(hash) if c.day <= file.day && c.day.is_multiple_of(CHECKPOINT_DAYS) => checkpoints.push((c.day, hash)),
            Ok(_) => errors.push(format!("checkpoint for day {}: not a checkpoint day up to {}", c.day, file.day)),
            Err(e) => errors.push(format!("checkpoint for day {}: {e}", c.day)),
        }
    }
    let snapshot = match (&file.snapshot, &file.snapshot_hash) {
        (Some(name), Some(hash)) => match hex("snapshot_hash", hash) {
            Ok(hash) => Some((path.with_file_name(name), hash)),
            Err(e) => {
                errors.push(e);
                None
            }
        },
        (None, None) => None,
        _ => {
            errors.push("a save names `snapshot` and `snapshot_hash` together, or neither".to_owned());
            None
        }
    };
    if !errors.is_empty() {
        return Err(LoadError { messages: errors });
    }

    if !replay && let Some((snapshot_path, hash)) = &snapshot {
        let world = crate::snapshot::read(snapshot_path, scenario.world.defs.clone(), content_hash)?;
        if world.day != file.day || world.state_hash() != *hash {
            return Err(LoadError::single(format!(
                "{} is not this save's snapshot (day {}, hash {:#018x}; the save expects day {} and {hash:#018x})",
                snapshot_path.display(),
                world.day,
                world.state_hash(),
                file.day
            )));
        }
        scenario.world = world;
        let save = SaveData { scenario: scenario_dir, content_hash, day: file.day, checkpoints, commands };
        return Ok(LoadedSave { scenario, save, last_report: None });
    }

    // Replay. Each day applies exactly what the server applied on it, in the same
    // order, so the save's log alone reproduces the game.
    let mut last_report = None;
    let mut next_command = 0;
    let mut next_checkpoint = 0;
    while scenario.world.day < file.day {
        let day = scenario.world.day;
        let start = next_command;
        while next_command < commands.len() && commands[next_command].day == day {
            next_command += 1;
        }
        let todays: Vec<Command> = commands[start..next_command].iter().map(|c| c.command).collect();
        // The shared day step, with the saved log as the day's only commands: the
        // scenario's scripted ones for this day are already in it (D23).
        let step = crate::step_day(&mut scenario.world, &CommandLog::default(), &todays);
        if let Some((command, e)) = todays.iter().zip(step.outcomes.players).find_map(|(c, r)| r.err().map(|e| (c, e)))
        {
            return Err(LoadError::single(format!("replaying day {day}: {command:?} was rejected: {e}")));
        }
        last_report = Some(step.report);
        if let Some(&(cp_day, hash)) = checkpoints.get(next_checkpoint)
            && cp_day == scenario.world.day
        {
            let actual = scenario.world.state_hash();
            if actual != hash {
                return Err(LoadError::single(format!(
                    "replaying the save reached a different state on day {cp_day} (hash {actual:#018x}, the save recorded {hash:#018x})"
                )));
            }
            next_checkpoint += 1;
        }
    }
    if let Some((_, hash)) = snapshot
        && scenario.world.state_hash() != hash
    {
        return Err(LoadError::single(format!(
            "replaying the save ended in a different state than its snapshot (hash {:#018x}, the snapshot is {hash:#018x})",
            scenario.world.state_hash()
        )));
    }
    let save = SaveData { scenario: scenario_dir, content_hash, day: file.day, checkpoints, commands };
    Ok(LoadedSave { scenario, save, last_report })
}

/// The rate written for a command: `Fixed`'s exact six-decimal form.
#[cfg(test)]
fn rate_text(rate: pax_engine::Fixed) -> String {
    rate.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_engine::Fixed;

    #[test]
    fn rates_are_written_exactly() {
        assert_eq!(rate_text(Fixed::from_raw(150_000)), "0.150000");
        assert_eq!(rate_text(Fixed::from_raw(1)), "0.000001");
    }

    #[test]
    fn hashes_round_trip_as_hex() {
        assert_eq!(hex("h", &format!("{:#018x}", u64::MAX)), Ok(u64::MAX));
        assert!(hex("h", "12").is_err());
    }
}
