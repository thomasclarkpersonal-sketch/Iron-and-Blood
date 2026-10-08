//! `pax_server <scenario-dir> [--bind ADDR] [--port-file PATH] [--exit-when-idle]`
//!
//! The authoritative game server (D10). In single player the client launches it with
//! `--bind 127.0.0.1:0 --port-file <tmp> --exit-when-idle` and reads the port from the
//! file (NETWORK_PROTOCOL §6).

use std::path::PathBuf;
use std::process::ExitCode;

use pax_server::{Config, Server};
use tracing::error;

const USAGE: &str = "usage: pax_server <scenario-dir> [--bind ADDR] [--port-file PATH] [--exit-when-idle]";

fn parse_args() -> Result<(Config, Option<PathBuf>), String> {
    let mut it = std::env::args().skip(1);
    let mut config = Config::local(it.next().ok_or("missing scenario directory")?);
    let mut port_file = None;
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--exit-when-idle" => config.exit_when_idle = true,
            "--bind" => {
                let value = it.next().ok_or("--bind needs a value")?;
                config.bind =
                    value.parse().map_err(|_| format!("--bind: '{value}' is not an address like 127.0.0.1:0"))?;
            }
            "--port-file" => port_file = Some(PathBuf::from(it.next().ok_or("--port-file needs a value")?)),
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
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
    let (config, port_file) = match parse_args() {
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
        server.shutdown();
        return ExitCode::FAILURE;
    }
    server.wait();
    ExitCode::SUCCESS
}
