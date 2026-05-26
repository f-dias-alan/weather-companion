use anyhow::Result;
use libadwaita as adw;
use adw::prelude::*;
use crate::storage::database::Database;
use crate::ui::window::MainWindow;

const APP_ID: &str = "org.gnome.WeatherCompanion";

pub fn run() -> Result<()> {
    glib::set_application_name("Weather Companion");
    glib::set_prgname(Some("weather-companion"));

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
