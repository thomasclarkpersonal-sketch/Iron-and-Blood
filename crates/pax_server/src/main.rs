use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

mod server;
mod session;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    info!("Starting Iron and Blood M3 pax_server...");

    // TODO: Load scenario definitions using pax_data
    // TODO: Initialize server loop and bind TCP listener

    Ok(())
}
