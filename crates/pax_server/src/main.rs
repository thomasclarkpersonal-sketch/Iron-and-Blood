//! `pax_server --scenario DIR [--bind ADDR] [--port-file PATH] [--saves DIR] [--players N] [--sandbox] [--admin NAME --admin-password-file PATH] [--password-file PATH] [--commands-per-second N] [--pause-after S] [--drop-after S] [--updates-per-second N] [--map-every N] [--exit-when-idle]`
//!
//! The authoritative game server (D10). In single player the client launches it with
//! `--scenario <dir> --bind 127.0.0.1:0 --port-file <tmp> --sandbox --exit-when-idle`
//! and reads the port from the file (NETWORK_PROTOCOL §6). `--players N` lets up to N
//! clients play at once (M4-1; default 1). `--sandbox` accepts sessions without a
//! nation. `--admin NAME --admin-password-file PATH` makes the client of that name
//! the host when it gives the admin password (D24, M4-3, M4-6): a name alone proves
//! nothing. `--password-file PATH` makes every player give the server password.
//!
//! D24 requires TLS whenever a server is not bound to localhost, and TLS arrives with
//! M4-6b. Until then, `--players` above 1 is refused on any non-loopback address.
//!
//! With several players, a silent client pauses the game after `--pause-after`
//! seconds and is dropped after `--drop-after` (D24: 5 and 30 by default). A remote
//! client gets at most `--updates-per-second` updates a second, and the map with
//! every `--map-every`th (D24, M4-7: 4 and 5 by default).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use pax_server::{Config, Secret, Server};
use tracing::error;

const USAGE: &str = "usage: pax_server --scenario DIR [--bind ADDR] [--port-file PATH] [--saves DIR] [--players N] [--sandbox] [--admin NAME --admin-password-file PATH] [--password-file PATH] [--commands-per-second N] [--pause-after S] [--drop-after S] [--updates-per-second N] [--map-every N] [--exit-when-idle]";

/// The most players `--players` allows. Player ids are `u16` on the wire; the cap is
/// far below that, a sanity limit for a server whose every player gets every update.
const MAX_PLAYERS: u16 = 64;

/// A password from the file at `path` (D24, M4-6): read from a file, so it never
/// shows in the process list, with a trailing line break removed.
fn secret(flag: &str, path: Option<String>) -> Result<Secret, String> {
    let path = path.ok_or(format!("{flag} needs a value"))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{flag}: cannot read {path}: {e}"))?;
    let password = text.trim_end_matches(['\n', '\r']).to_owned();
    if password.is_empty() {
        return Err(format!("{flag}: {path} is empty"));
    }
    Ok(Secret::new(password))
}

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
    let count = |flag: &str, value: Option<String>| -> Result<u32, String> {
        let value = value.ok_or(format!("{flag} needs a value"))?;
        value.parse::<u32>().ok().filter(|&n| n > 0).ok_or(format!("{flag}: '{value}' is not a whole number above 0"))
    };
    let (mut admin_name, mut admin_password) = (None, None);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--scenario" => scenario = Some(PathBuf::from(it.next().ok_or("--scenario needs a value")?)),
            "--exit-when-idle" => config.exit_when_idle = true,
            "--sandbox" => config.sandbox = true,
            "--pause-after" => pause_after = Some(seconds("--pause-after", it.next())?),
            "--drop-after" => drop_after = Some(seconds("--drop-after", it.next())?),
            "--updates-per-second" => {
                config.bandwidth.updates_per_second = count("--updates-per-second", it.next())?;
            }
            "--map-every" => config.bandwidth.map_every = count("--map-every", it.next())?,
            "--admin" => admin_name = Some(it.next().ok_or("--admin needs a value")?),
            "--password-file" => config.password = Some(secret("--password-file", it.next())?),
            "--admin-password-file" => admin_password = Some(secret("--admin-password-file", it.next())?),
            "--commands-per-second" => {
                let value = it.next().ok_or("--commands-per-second needs a value")?;
                config.commands_per_second =
                    value.parse().map_err(|_| format!("--commands-per-second: '{value}' is not a number"))?;
            }
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
    config.admin = match (admin_name, admin_password) {
        (Some(name), Some(password)) => Some(pax_server::Admin { name, password }),
        (None, None) => None,
        _ => {
            return Err("--admin and --admin-password-file go together: a name alone proves nothing (D24)".to_owned());
        }
    };
    // D24's lag rules replace D22's 10 s timeout in multiplayer only.
    if config.max_players > 1 {
        config.use_multiplayer_lag();
        if let Some(drop) = drop_after {
            config.idle_timeout = drop;
        }
        if let Some(pause) = pause_after {
            config.pause_after = Some(pause);
        }
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

    /// D24's bandwidth defaults are settings (M4-7).
    #[test]
    fn bandwidth_settings_have_d24s_defaults() {
        let (config, _) = parse_args(args("--scenario s --players 2")).unwrap();
        assert_eq!(config.bandwidth, pax_server::Bandwidth { updates_per_second: 4, map_every: 5 });
        let (config, _) = parse_args(args("--scenario s --updates-per-second 10 --map-every 1")).unwrap();
        assert_eq!(config.bandwidth, pax_server::Bandwidth { updates_per_second: 10, map_every: 1 });
        assert!(parse_args(args("--scenario s --updates-per-second 0")).is_err());
        assert!(parse_args(args("--scenario s --map-every x")).is_err());
    }

    #[test]
    fn sandbox_and_admin_are_off_unless_asked_for() {
        let (config, _) = parse_args(args("--scenario s")).unwrap();
        assert!(!config.sandbox, "D24: sandbox only with --sandbox");
        assert_eq!(config.admin, None);
        assert!(parse_args(args("--scenario s --admin ada")).unwrap_err().contains("a name alone proves nothing"));
        let file = std::env::temp_dir().join(format!("pax-admin-pw-{}", std::process::id()));
        std::fs::write(&file, "s3cret\n").unwrap();
        let line = format!("--scenario s --admin ada --admin-password-file {}", file.display());
        let (config, _) = parse_args(args(&line)).unwrap();
        let admin = config.admin.expect("an admin");
        assert_eq!((admin.name.as_str(), &admin.password), ("ada", &Secret::new("s3cret")));
        assert!(!format!("{admin:?}").contains("s3cret"), "Debug never prints the password");
        assert!(parse_args(args(&format!("--scenario s --admin-password-file {}", file.display()))).is_err());
        let line = format!("--scenario s --password-file {}", file.display());
        assert_eq!(parse_args(args(&line)).unwrap().0.password, Some(Secret::new("s3cret")), "line break trimmed");
        std::fs::write(&file, "\n").unwrap();
        assert!(parse_args(args(&line)).unwrap_err().contains("is empty"));
        std::fs::remove_file(&file).unwrap();
        assert!(parse_args(args("--scenario s --commands-per-second 0")).is_err());
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
