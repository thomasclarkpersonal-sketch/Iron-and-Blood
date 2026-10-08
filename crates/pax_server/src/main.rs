//! `pax_server --scenario DIR [--bind ADDR] [--port-file PATH] [--saves DIR] [--players N] [--sandbox] [--admin NAME] [--pause-after S] [--drop-after S] [--exit-when-idle]`
//!
//! The authoritative game server (D10). In single player the client launches it with
//! `--scenario <dir> --bind 127.0.0.1:0 --port-file <tmp> --sandbox --exit-when-idle`
//! and reads the port from the file (NETWORK_PROTOCOL §6). `--players N` lets up to N
//! clients play at once (M4-1; default 1). `--sandbox` accepts sessions without a
//! nation, and `--admin NAME` makes the client of that name the host (D24, M4-3).
//!
//! D24 requires TLS whenever a server is not bound to localhost, and TLS arrives with
//! M4-6. Until then, `--players` above 1 is refused on any non-loopback address.
//!
//! With several players, a silent client pauses the game after `--pause-after`
//! seconds and is dropped after `--drop-after` (D24: 5 and 30 by default).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use pax_server::{Config, Server};
use tracing::error;

const USAGE: &str = "usage: pax_server --scenario DIR [--bind ADDR] [--port-file PATH] [--saves DIR] [--players N] [--sandbox] [--admin NAME] [--pause-after S] [--drop-after S] [--exit-when-idle]";

/// The most players `--players` allows. Player ids are `u16` on the wire; the cap is
/// far below that, a sanity limit for a server whose every player gets every update.
const MAX_PLAYERS: u16 = 64;

/// Parses the arguments after the program name.
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<(Config, Option<PathBuf>), String> {
    let mut it = args.into_iter();
    let mut scenario = None;
    let mut config = Config::local(PathBuf::new());
    // Sandbox seats exist only when asked for (D24).
    config.sandbox = false;
    let mut port_file = None;
    let (mut pause_after, mut drop_after) = (None, None);
    let seconds = |flag: &str, value: Option<String>| -> Result<Duration, String> {
        let value = value.ok_or(format!("{flag} needs a value"))?;
        value
            .parse::<u64>()
            .ok()
            .filter(|&s| s > 0)
            .map(Duration::from_secs)
            .ok_or(format!("{flag}: '{value}' is not a whole number of seconds"))
    };
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--scenario" => scenario = Some(PathBuf::from(it.next().ok_or("--scenario needs a value")?)),
            "--exit-when-idle" => config.exit_when_idle = true,
            "--sandbox" => config.sandbox = true,
            "--pause-after" => pause_after = Some(seconds("--pause-after", it.next())?),
            "--drop-after" => drop_after = Some(seconds("--drop-after", it.next())?),
            "--admin" => config.admin = Some(it.next().ok_or("--admin needs a value")?),
            "--bind" => {
                let value = it.next().ok_or("--bind needs a value")?;
                config.bind =
                    value.parse().map_err(|_| format!("--bind: '{value}' is not an address like 127.0.0.1:0"))?;
            }
            "--saves" => config.saves_dir = PathBuf::from(it.next().ok_or("--saves needs a value")?),
            "--players" => {
                let value = it.next().ok_or("--players needs a value")?;
                config.max_players = value
                    .parse()
                    .ok()
                    .filter(|&n| (1..=MAX_PLAYERS).contains(&n))
                    .ok_or(format!("--players: '{value}' is not a number from 1 to {MAX_PLAYERS}"))?;
            }
            "--port-file" => port_file = Some(PathBuf::from(it.next().ok_or("--port-file needs a value")?)),
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    config.scenario = scenario.ok_or("missing --scenario DIR")?;
    // D24's lag rules replace D22's 10 s timeout in multiplayer only.
    if config.max_players > 1 {
        config.idle_timeout = drop_after.unwrap_or(pax_server::DROP_AFTER);
        config.pause_after = Some(pause_after.unwrap_or(pax_server::PAUSE_AFTER));
    } else if pause_after.is_some() || drop_after.is_some() {
        return Err("--pause-after and --drop-after are multiplayer settings (--players above 1)".to_owned());
    }
    // The rules every way of building a server shares (`Config::validate`).
    config.validate().map_err(|e| e.to_string())?;
    Ok((config, port_file))
}

/// Writes the port atomically, so a client polling the file never reads half of it.
fn write_port_file(path: &PathBuf, port: u16) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, format!("{port}\n"))?;
    std::fs::rename(&tmp, path)
}

fn main() -> ExitCode {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let (config, port_file) = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let server = match Server::start(config) {
        Ok(server) => server,
        Err(e) => {
            error!("could not start: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(path) = port_file
        && let Err(e) = write_port_file(&path, server.local_addr().port())
    {
        error!("could not write the port file {}: {e}", path.display());
        let _ = server.shutdown();
        return ExitCode::FAILURE;
    }
    match server.wait() {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            error!("{failure}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    /// The exact command line NETWORK_PROTOCOL §6 tells the client to run.
    #[test]
    fn the_documented_launch_command_parses() {
        let (config, port_file) = parse_args(args(
            "--scenario scenarios/two_states --bind 127.0.0.1:0 --port-file /tmp/p --sandbox --exit-when-idle",
        ))
        .unwrap();
        assert_eq!(config.scenario, PathBuf::from("scenarios/two_states"));
        assert_eq!(config.bind, "127.0.0.1:0".parse().unwrap());
        assert_eq!(port_file, Some(PathBuf::from("/tmp/p")));
        assert!(config.exit_when_idle);
        assert_eq!(config.max_players, 1, "single player by default");
        assert!(config.sandbox);
    }

    /// D24: no plaintext multiplayer off localhost. M4-6's TLS lifts this.
    #[test]
    fn multiplayer_binds_loopback_only_until_tls() {
        let e = parse_args(args("--scenario s --players 2 --bind 0.0.0.0:7777")).unwrap_err();
        assert!(e.contains("needs TLS"), "{e}");
        assert!(parse_args(args("--scenario s --players 2 --bind 0.0.0.0:7777 --insecure-no-tls")).is_err());
        assert!(parse_args(args("--scenario s --players 2 --bind 127.0.0.1:7777")).is_ok());
        assert!(parse_args(args("--scenario s --bind 0.0.0.0:7777")).is_ok(), "one player: unchanged from M3");
    }

    /// D24's lag rules: multiplayer defaults, settings, and single player unchanged.
    #[test]
    fn lag_settings_apply_to_multiplayer_only() {
        let (one, _) = parse_args(args("--scenario s")).unwrap();
        assert_eq!((one.idle_timeout, one.pause_after), (pax_protocol::IDLE_TIMEOUT, None));
        let (many, _) = parse_args(args("--scenario s --players 2")).unwrap();
        assert_eq!((many.idle_timeout, many.pause_after), (pax_server::DROP_AFTER, Some(pax_server::PAUSE_AFTER)));
        let (set, _) = parse_args(args("--scenario s --players 2 --pause-after 2 --drop-after 9")).unwrap();
        assert_eq!((set.idle_timeout, set.pause_after), (Duration::from_secs(9), Some(Duration::from_secs(2))));
        assert!(parse_args(args("--scenario s --pause-after 2")).unwrap_err().contains("multiplayer"));
        assert!(parse_args(args("--scenario s --players 2 --pause-after 9 --drop-after 9")).is_err());
        assert!(parse_args(args("--scenario s --players 2 --drop-after 0")).is_err());
    }

    #[test]
    fn sandbox_and_admin_are_off_unless_asked_for() {
        let (config, _) = parse_args(args("--scenario s")).unwrap();
        assert!(!config.sandbox, "D24: sandbox only with --sandbox");
        assert_eq!(config.admin, None);
        let (config, _) = parse_args(args("--scenario s --admin ada")).unwrap();
        assert_eq!(config.admin.as_deref(), Some("ada"));
    }

    #[test]
    fn players_must_be_from_1_to_the_cap() {
        let (config, _) = parse_args(args("--scenario s --players 4")).unwrap();
        assert_eq!(config.max_players, 4);
        for bad in ["0", "65", "-1", "four"] {
            let e = parse_args(args(&format!("--scenario s --players {bad}"))).unwrap_err();
            assert!(e.contains("--players"), "{bad}: {e}");
        }
    }

    #[test]
    fn the_scenario_is_required_and_unknown_arguments_are_refused() {
        assert!(parse_args(args("--bind 127.0.0.1:0")).unwrap_err().contains("--scenario"));
        assert!(parse_args(args("scenarios/two_states")).unwrap_err().contains("unknown argument"));
        assert!(parse_args(args("--scenario")).unwrap_err().contains("needs a value"));
    }
}
