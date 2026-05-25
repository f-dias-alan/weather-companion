// weather-companion/src/ui/window.rs
//
// The main application window.
//
// ARCHITECTURE
// ─────────────
// We use GTK4 + libadwaita with the recommended pattern:
//   - adw::ApplicationWindow as the top-level container
//   - adw::NavigationView for stack-based navigation (main ↔ manage)
//   - adw::ToolbarView inside each page to hold the HeaderBar
//   - adw::Clamp to constrain the content width on wide screens
//
// GTK STATE MANAGEMENT
// ─────────────────────
// All mutable application state is kept in a Rc<RefCell<AppState>> shared
// between widgets via closure captures. This is the standard GTK4/Rust pattern
// because GTK widgets are not Send, so we stay on the main thread and use
// RefCell for interior mutability.
//
// ASYNC CALLS
// ───────────
// Network requests (Nominatim) are dispatched via glib::MainContext::spawn_local()
// which runs the future on the GLib main loop without blocking the UI. The
// callback updates widget state after the await.

use crate::services::{nominatim, sync};
use crate::storage::database::Database;
use crate::ui::{locations::LocationsPage, search::SearchPage};
use crate::weather::models::SavedLocation;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;

/// Shared application state passed to all UI components.
pub struct AppState {
    pub db: Database,
    pub saved_locations: Vec<SavedLocation>,
}

pub struct MainWindow {
    window: adw::ApplicationWindow,
}

impl MainWindow {
    pub fn new(app: &adw::Application, db: Database) -> Self {
        let initial_locations = db.list_locations().unwrap_or_default();
        let state = Rc::new(RefCell::new(AppState {
            db,
            saved_locations: initial_locations,
        }));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Weather Companion")
            .default_width(480)
            .default_height(640)
            .build();

        // AdwNavigationView manages the page stack with push/pop animations.
        let nav_view = adw::NavigationView::new();

        // Build the two pages.
        let search_page = SearchPage::build(Rc::clone(&state), nav_view.clone());
        let locations_page = LocationsPage::build(Rc::clone(&state), nav_view.clone());

        nav_view.add(&search_page);
        nav_view.add(&locations_page);

        window.set_content(Some(&nav_view));

        Self { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}