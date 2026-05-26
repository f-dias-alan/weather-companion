mod app;
mod services;
mod storage;
mod ui;
mod utils;
mod weather;

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_env("WEATHER_COMPANION_LOG"))
        .init();
    tracing::info!("Starting Weather Companion v{}", env!("CARGO_PKG_VERSION"));
    app::run()
}
