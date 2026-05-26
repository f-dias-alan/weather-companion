use anyhow::Result;
use libadwaita as adw;
use adw::prelude::*;
use std::path::PathBuf;
use crate::storage::database::Database;
use crate::ui::window::MainWindow;
use crate::weather::icao::load_airports;

const APP_ID: &str = "org.gnome.WeatherCompanion";

fn airports_path() -> PathBuf {
    // Flatpak: /app/share/weather-companion/airports.csv
    // Native:  ./data/airports.csv
    let flatpak = PathBuf::from("/app/share/weather-companion/airports.csv");
    if flatpak.exists() { return flatpak; }
    PathBuf::from("data/airports.csv")
}

pub fn run() -> Result<()> {
    glib::set_application_name("Weather Companion");
    glib::set_prgname(Some("weather-companion"));

    // Carrega banco de aeroportos antes de qualquer coisa
    let path = airports_path();
    load_airports(&path)?;

    // Runtime tokio em background para o reqwest
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");
    let _guard = rt.enter();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(move |app| {
        let db = Database::open().expect("Failed to open local database");
        let window = MainWindow::new(app, db);
        window.present();
    });

    let exit_code = app.run();
    if exit_code != glib::ExitCode::SUCCESS {
        anyhow::bail!("Application exited with non-zero code");
    }
    Ok(())
}
