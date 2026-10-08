//! `pax_server --scenario DIR [--bind ADDR] [--port-file PATH] [--saves DIR] [--exit-when-idle]`
//!
//! The authoritative game server (D10). In single player the client launches it with
//! `--scenario <dir> --bind 127.0.0.1:0 --port-file <tmp> --exit-when-idle` and reads
//! the port from the file (NETWORK_PROTOCOL §6).

use std::path::PathBuf;
use std::process::ExitCode;

use pax_server::{Config, Server};
use tracing::error;

const USAGE: &str =
    "usage: pax_server --scenario DIR [--bind ADDR] [--port-file PATH] [--saves DIR] [--exit-when-idle]";

/// Parses the arguments after the program name.
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<(Config, Option<PathBuf>), String> {
    let mut it = args.into_iter();
    let mut scenario = None;
    let mut config = Config::local(PathBuf::new());
    let mut port_file = None;
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--scenario" => scenario = Some(PathBuf::from(it.next().ok_or("--scenario needs a value")?)),
            "--exit-when-idle" => config.exit_when_idle = true,
            "--bind" => {
                let value = it.next().ok_or("--bind needs a value")?;
                config.bind =
                    value.parse().map_err(|_| format!("--bind: '{value}' is not an address like 127.0.0.1:0"))?;
            }
            "--saves" => config.saves_dir = PathBuf::from(it.next().ok_or("--saves needs a value")?),
            "--port-file" => port_file = Some(PathBuf::from(it.next().ok_or("--port-file needs a value")?)),
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    config.scenario = scenario.ok_or("missing --scenario DIR")?;
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
        let (config, port_file) =
            parse_args(args("--scenario scenarios/two_states --bind 127.0.0.1:0 --port-file /tmp/p --exit-when-idle"))
                .unwrap();
        assert_eq!(config.scenario, PathBuf::from("scenarios/two_states"));
        assert_eq!(config.bind, "127.0.0.1:0".parse().unwrap());
        assert_eq!(port_file, Some(PathBuf::from("/tmp/p")));
        assert!(config.exit_when_idle);
    }

    #[test]
    fn the_scenario_is_required_and_unknown_arguments_are_refused() {
        assert!(parse_args(args("--bind 127.0.0.1:0")).unwrap_err().contains("--scenario"));
        assert!(parse_args(args("scenarios/two_states")).unwrap_err().contains("unknown argument"));
        assert!(parse_args(args("--scenario")).unwrap_err().contains("needs a value"));
    }
}
