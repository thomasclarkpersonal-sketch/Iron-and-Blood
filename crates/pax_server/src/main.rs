//! `pax_server`: the authoritative game server (D10, D22, D23).
//!
//! A stub until M3-2 (docs/MILESTONE_3.md). It exits with an error, so nothing
//! that launches it (the client, scripts) mistakes it for a working server.

use tracing::{Level, error};
use tracing_subscriber::FmtSubscriber;

mod server;
mod session;

fn main() -> std::process::ExitCode {
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    error!("pax_server is not implemented yet: see docs/MILESTONE_3.md, task M3-2");
    std::process::ExitCode::FAILURE
}
