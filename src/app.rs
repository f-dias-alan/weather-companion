// weather-companion/src/app.rs
//
// Defines the top-level GTK Application object and wires everything together.
//
// GNOME note: We use adw::Application (a thin wrapper around gtk4::Application)
// so we get Adwaita's colour scheme and system font integration for free.
// The application ID follows reverse-DNS notation as required by Flatpak/DBus.

use adw::prelude::*;
use anyhow::Result;
use libadwaita as adw;

use crate::storage::database::Database;
use crate::ui::window::MainWindow;

const APP_ID: &str = "org.gnome.WeatherCompanion";

pub fn run() -> Result<()> {
    // Initialise GLib's type system. Required before registering any GObject
    // subclasses (our custom widgets inherit from adw::ApplicationWindow).
    glib::set_application_name("Weather Companion");
    glib::set_prgname(Some("weather-companion"));

    let app = adw::Application::builder()
        .application_id(APP_ID)
        // Tells GNOME Shell this is a proper desktop app (used for taskbar
        // grouping, .desktop file matching, etc.)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();

    app.connect_activate(move |app| {
        // Initialise local SQLite database on first launch.
        // We use a blocking call here because activate() is synchronous;
        // the database open is fast (< 5 ms on any modern drive).
        let db = Database::open().expect("Failed to open local database");

        let window = MainWindow::new(app, db);
        window.present();
    });

    // gtk::Application::run() starts the GLib main loop and returns only when
    // all application windows have been destroyed.
    let exit_code = app.run();

    if exit_code != glib::ExitCode::SUCCESS {
        anyhow::bail!("Application exited with non-zero code: {:?}", exit_code);
    }

    Ok(())
}