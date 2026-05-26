use crate::storage::database::Database;
use crate::ui::{locations::LocationsPage, search::SearchPage};
use crate::weather::models::SavedLocation;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;

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
        let state = Rc::new(RefCell::new(AppState { db, saved_locations: initial_locations }));

        let window = adw::ApplicationWindow::builder()
            .application(app).title("Weather Companion")
            .default_width(480).default_height(640)
            .build();

        let nav_view = adw::NavigationView::new();
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
