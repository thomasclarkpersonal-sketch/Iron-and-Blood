//! `pax_cli`: headless runner for the simulation.
//!
//! ```text
//! pax_cli run    <scenario-dir> [--days N] [--every K] [--market KEY]
//! pax_cli record <scenario-dir> [--days N]      # write <scenario-dir>/golden.hashes (keeps the existing length)
//! pax_cli verify <scenario-dir> [--threads T]   # replay and compare with golden.hashes
//! pax_cli bench  <scenario-dir> [--days N] [--scale K] [--regions R] [--threads T]
//!     (--scale copies POP rows with identical identities, which month-end
//!     compaction merges back; use --regions for runs past day 29)
//! pax_cli report <scenario-dir> [--days N] [--every K]   # economy health indicators
//! pax_cli replay <save.toml> [--threads T]      # replay a server save, verifying its checkpoints
//! ```
//!
//! `verify` is the determinism gate used by CI: any change to simulation
//! results makes it fail until the hashes are deliberately re-recorded.

// Floats are banned from simulation state, not from presentation: the CLI
// uses them only to print wall-clock timings.
#![allow(clippy::float_arithmetic)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

mod report;

use pax_data::{CommandLog, golden};
use pax_engine::{DayReport, Fixed, World, step};

const USAGE: &str = "usage:
  pax_cli run    <scenario-dir> [--days N] [--every K] [--market KEY]
  pax_cli record <scenario-dir> [--days N]
  pax_cli verify <scenario-dir> [--threads T]
  pax_cli bench  <scenario-dir> [--days N] [--scale K] [--regions R] [--threads T]
  pax_cli report <scenario-dir> [--days N] [--every K]
  pax_cli replay <save.toml> [--threads T]";

/// The subcommands, parsed once: every match over them is exhaustive, so a new one
/// can't be missing a flag list or a dispatch arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cmd {
    Run,
    Record,
    Verify,
    Bench,
    Report,
    Replay,
}

impl Cmd {
    fn parse(name: &str) -> Result<Cmd, String> {
        Ok(match name {
            "run" => Cmd::Run,
            "record" => Cmd::Record,
            "verify" => Cmd::Verify,
            "bench" => Cmd::Bench,
            "report" => Cmd::Report,
            "replay" => Cmd::Replay,
            other => return Err(format!("unknown command '{other}'")),
        })
    }

    /// The flags the command takes, as in `USAGE`.
    fn flags(self) -> &'static [Flag] {
        match self {
            Cmd::Run => &[Flag::Days, Flag::Every, Flag::Market],
            Cmd::Record => &[Flag::Days],
            Cmd::Verify | Cmd::Replay => &[Flag::Threads],
            Cmd::Bench => &[Flag::Days, Flag::Scale, Flag::Regions, Flag::Threads],
            Cmd::Report => &[Flag::Days, Flag::Every],
        }
    }
}

/// The flags, parsed once: `Cmd::flags` and `parse_args` both match over this type,
/// so a flag can't be listed for a command without a parser, or the reverse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flag {
    Days,
    Every,
    Scale,
    Regions,
    Market,
    Threads,
}

impl Flag {
    fn parse(name: &str) -> Option<Flag> {
        Some(match name {
            "--days" => Flag::Days,
            "--every" => Flag::Every,
            "--scale" => Flag::Scale,
            "--regions" => Flag::Regions,
            "--market" => Flag::Market,
            "--threads" => Flag::Threads,
            _ => return None,
        })
    }
}

struct Args {
    command: Cmd,
    /// The command's operand: the scenario directory, or for `replay` the save file.
    target: PathBuf,
    days: Option<u64>,
    every: u64,
    scale: u32,
    regions: u32,
    market: Option<String>,
    threads: Option<usize>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let name = it.next().ok_or("missing command")?;
    let command = Cmd::parse(&name)?;
    let target = PathBuf::from(it.next().ok_or("missing scenario directory (or save file, for replay)")?);
    let mut args = Args { command, target, days: None, every: 30, scale: 1, regions: 1, market: None, threads: None };
    while let Some(text) = it.next() {
        // A flag a command doesn't use is an error, never silently ignored.
        let flag = Flag::parse(&text)
            .filter(|f| command.flags().contains(f))
            .ok_or_else(|| format!("unknown flag {text} for {name}"))?;
        let value = it.next().ok_or(format!("{text} needs a value"))?;
        let num = |v: &str| v.parse::<u64>().map_err(|_| format!("{text}: '{v}' is not a number"));
        match flag {
            Flag::Days => args.days = Some(num(&value)?),
            Flag::Every => args.every = num(&value)?.max(1),
            Flag::Scale => args.scale = num(&value)?.max(1) as u32,
            Flag::Regions => args.regions = num(&value)?.max(1) as u32,
            Flag::Market => args.market = Some(value),
            Flag::Threads => args.threads = Some(num(&value)?.max(1) as usize),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut pool = rayon::ThreadPoolBuilder::new();
    if let Some(t) = args.threads {
        pool = pool.num_threads(t);
    }
    let pool = pool.build().expect("failed to build thread pool");
    pool.install(|| match dispatch(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    })
}

fn dispatch(args: &Args) -> Result<ExitCode, String> {
    if args.command == Cmd::Replay {
        return replay(&args.target);
    }
    let scenario = pax_data::load_scenario(&args.target).map_err(|e| e.to_string())?;
    match args.command {
        Cmd::Replay => unreachable!("handled above: it loads a save, not a scenario"),
        Cmd::Run => {
            run(scenario.world, &scenario.commands, args.days.unwrap_or(365), args.every, args.market.as_deref())
        }
        Cmd::Record => {
            // D11: keep the existing file's length unless --days says otherwise, and
            // never record less than the scenario's minimum (past its last command).
            let path = golden_path(&args.target);
            let existing = golden::existing_len(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let min = golden::min_days(&scenario.commands);
            let days = args.days.or(existing).unwrap_or(min);
            if days < min {
                return Err(format!(
                    "--days {days} is shorter than this scenario's minimum of {min} (D11: past its last command)"
                ));
            }
            let mut world = scenario.world;
            let hashes = pax_data::run_logged(&mut world, &scenario.commands, days)?;
            golden::write(&path, &scenario.name, &hashes).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("recorded {days} day hashes to {}", path.display());
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Verify => verify(scenario.world, &scenario.commands, &golden_path(&args.target)),
        Cmd::Bench => bench(scenario.world, args.days.unwrap_or(30), args.scale, args.regions),
        Cmd::Report => report::run(scenario.world, &scenario.commands, args.days.unwrap_or(365), args.every),
    }
}

/// Replays a server save from its scenario (D23, M3-9): every logged command on its
/// day, every checkpoint verified, and the end checked against the save's snapshot
/// if it has one. Prints the final day and `state_hash`, which must match what the
/// server sent in that day's `DayUpdate` (D10).
fn replay(save: &Path) -> Result<ExitCode, String> {
    let started = Instant::now();
    let loaded = pax_data::save::load_by_replay(save).map_err(|e| e.to_string())?;
    let world = &loaded.scenario.world;
    println!(
        "ok: replayed {} to day {}: state_hash {:#018x}, {} checkpoints and {} commands ({:.1} s)",
        save.display(),
        world.day,
        world.state_hash(),
        loaded.save.checkpoints.len(),
        loaded.save.commands.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(ExitCode::SUCCESS)
}

fn golden_path(scenario: &Path) -> PathBuf {
    scenario.join("golden.hashes")
}

fn run(mut world: World, log: &CommandLog, days: u64, every: u64, market: Option<&str>) -> Result<ExitCode, String> {
    let m =
        match market {
            None => 0,
            Some(key) => world.geography.market_keys.iter().position(|k| k == key).ok_or_else(|| {
                format!("unknown market '{key}' (markets: {})", world.geography.market_keys.join(", "))
            })?,
        };
    println!("market: {}", world.geography.market_keys[m]);
    let goods: Vec<String> = world.defs.goods.iter().map(|g| g.key.clone()).collect();
    print!("{:>5} {:>10} {:>14}", "day", "population", "money");
    for g in &goods {
        print!(" {:>12} {:>10}", format!("{g} p"), "traded");
    }
    println!();
    for _ in 0..days {
        let report = tick(&mut world, log);
        if !is_report_day(report.day, every, days) {
            continue;
        }
        print!("{:>5} {:>10} {:>14}", report.day + 1, world.population(), short(report.total_money, 2));
        for r in &report.goods[m * goods.len()..(m + 1) * goods.len()] {
            print!(" {:>12} {:>10}", short(r.price, 4), short(r.traded, 1));
        }
        println!();
    }
    Ok(ExitCode::SUCCESS)
}

fn verify(mut world: World, log: &CommandLog, path: &Path) -> Result<ExitCode, String> {
    let expected = golden::read(path).map_err(|e| format!("{}: {e} (run `pax_cli record` first)", path.display()))?;
    let min = golden::min_days(log);
    if (expected.len() as u64) < min {
        return Err(format!(
            "{}: pins {} days, but this scenario needs at least {min} to cover its command log (D11)",
            path.display(),
            expected.len()
        ));
    }
    for (day, &want) in expected.iter().enumerate() {
        tick(&mut world, log);
        let got = world.state_hash();
        if got != want {
            eprintln!("DESYNC on day {day}: expected {want:016x}, got {got:016x}");
            eprintln!("If this change to simulation results is intended, re-record with `pax_cli record`.");
            return Ok(ExitCode::FAILURE);
        }
    }
    println!("ok: {} days match {}", expected.len(), path.display());
    Ok(ExitCode::SUCCESS)
}

fn bench(world: World, days: u64, scale: u32, regions: u32) -> Result<ExitCode, String> {
    let mut world = pax_data::bench::replicate(&world, scale, regions);
    let start = Instant::now();
    for _ in 0..days {
        step(&mut world);
    }
    let per_day = start.elapsed().as_secs_f64() * 1e3 / days.max(1) as f64;
    println!(
        "{} POP rows, {} producers, {} markets, {} threads: {per_day:.3} ms/day over {days} days",
        world.pops.len(),
        world.producers.len(),
        world.geography.market_count(),
        rayon::current_num_threads()
    );
    Ok(ExitCode::SUCCESS)
}

/// Advances one day through the shared replay step (`pax_data::step_logged`).
/// A rejected logged command is reported on stderr; the day has still run.
pub(crate) fn tick(world: &mut World, log: &CommandLog) -> DayReport {
    let day = world.day;
    let (report, rejected) = pax_data::step_logged(world, log);
    for (command, error) in rejected {
        eprintln!("warning: command {command:?} on day {day} rejected: {error}");
    }
    report
}

/// True if 0-based `day` ends a reporting period of `every` days, or is the last
/// of `days`. Shared by `run` and `report` so their rows line up.
pub(crate) fn is_report_day(day: u64, every: u64, days: u64) -> bool {
    (day + 1).is_multiple_of(every) || day + 1 == days
}

/// Formats a Fixed with `decimals` places (display only).
fn short(v: Fixed, decimals: usize) -> String {
    let s = v.to_string();
    match s.split_once('.') {
        Some((i, _)) if decimals == 0 => i.to_string(),
        Some((i, f)) => format!("{i}.{}", &f[..decimals.min(f.len())]),
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_states() -> pax_data::Scenario {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        pax_data::load_scenario(&dir).expect("two_states loads")
    }

    #[test]
    fn run_shows_a_chosen_market() {
        let s = two_states();
        assert_eq!(run(s.world, &s.commands, 5, 5, Some("highland")).unwrap(), ExitCode::SUCCESS);
    }

    #[test]
    fn run_rejects_an_unknown_market_by_name() {
        let s = two_states();
        let err = run(s.world, &s.commands, 5, 5, Some("atlantis")).unwrap_err();
        assert!(err.contains("unknown market 'atlantis'") && err.contains("lowland"), "{err}");
    }
}
