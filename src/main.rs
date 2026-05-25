fn main() {
    println!("Hello, world!");
}
// weather-companion/src/main.rs
//
// Entry point. Initialises tracing, loads resources, and hands off to the
// GTK application object defined in app.rs.
//
// GNOME note: We call gtk4::init() implicitly through Application::new(), but
// we must initialise GLib's type system before touching any GObject subclass.
// The #[tokio::main] macro sets up the async runtime; GTK itself is NOT async
// and must run on the main thread. All network calls are dispatched via
// glib::MainContext::spawn_local() so they share the GLib event loop.

mod app;
mod services;
mod storage;
mod ui;
mod utils;
mod weather;

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> Result<()> {
    // Structured logging: WEATHER_COMPANION_LOG=debug cargo run
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_env("WEATHER_COMPANION_LOG"))
        .init();

    tracing::info!(
        "Starting Weather Companion v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Run the GTK application. This blocks until all windows are closed.
    app::run()
}