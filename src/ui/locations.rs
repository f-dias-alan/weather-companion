// weather-companion/src/ui/locations.rs
//
// The "Manage Cities" page: shows a scrollable list of saved locations with
// remove buttons, a sync-all button, and station distance information.
//
// DESIGN PATTERN
// ───────────────
// We use adw::PreferencesGroup → adw::ActionRow as the list container because
// it automatically handles GNOME HIG row styling, keyboard navigation, and
// dark mode. Each row displays:
//   - City name (title)
//   - Station ICAO + distance (subtitle)
//   - Remove button (suffix widget)
//
// The list is rebuilt from scratch whenever the user navigates to this page.
// This is simple and fast enough for the expected number of cities (< 100).

use crate::services::sync;
use crate::ui::window::AppState;
use glib::{clone, MainContext};
use gtk4::{self as gtk, prelude::*};
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;

pub struct LocationsPage;

impl LocationsPage {
    pub fn build(state: Rc<RefCell<AppState>>, nav: adw::NavigationView) -> adw::NavigationPage {
        // ── Header bar ──────────────────────────────────────────────────────
        let header = adw::HeaderBar::new();

        // Re-sync button
        let sync_btn = gtk::Button::builder()
            .icon_name("emblem-synchronizing-symbolic")
            .tooltip_text("Re-sync all locations to GNOME Weather")
            .build();

        header.pack_end(&sync_btn);

        // ── Scrollable content ───────────────────────────────────────────────
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();

        let clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&content_box)
            .build();

        scroll.set_child(Some(&clamp));

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&scroll));

        let page = adw::NavigationPage::builder()
            .title("Saved Cities")
            .tag("locations")
            .child(&toolbar_view)
            .build();

        // Rebuild the list every time this page is shown.
        let content_box2 = content_box.clone();
        let state2 = Rc::clone(&state);
        let nav2 = nav.clone();

        page.connect_shown(move |_| {
            rebuild_list(&content_box2, Rc::clone(&state2), nav2.clone());
        });

        // ── Re-sync handler ──────────────────────────────────────────────────
        sync_btn.connect_clicked(clone!(@strong state => move |btn| {
            btn.set_sensitive(false);
            let btn2 = btn.clone();
            let st = Rc::clone(&state);

            MainContext::default().spawn_local(async move {
                let count = sync::resync_all(&st.borrow().db).await.unwrap_or(0);
                tracing::info!("Re-synced {} locations", count);
                btn2.set_sensitive(true);
            });
        }));

        page
    }
}

/// Clears and rebuilds the locations list.
fn rebuild_list(
    content: &gtk::Box,
    state: Rc<RefCell<AppState>>,
    nav: adw::NavigationView,
) {
    // Remove all children
    while let Some(child) = content.first_child() {
        content.remove(&child);
    }

    let locations = state.borrow().saved_locations.clone();

    if locations.is_empty() {
        let empty = adw::StatusPage::builder()
            .title("No Saved Cities")
            .description("Search for a city on the previous page to add it to GNOME Weather.")
            .icon_name("weather-few-clouds-symbolic")
            .build();
        content.append(&empty);
        return;
    }

    let group = adw::PreferencesGroup::builder()
        .title("Your Cities")
        .description(format!(
            "{} location{} synced to GNOME Weather",
            locations.len(),
            if locations.len() == 1 { "" } else { "s" }
        ))
        .build();

    for loc in locations {
        let row = build_location_row(&loc, Rc::clone(&state), nav.clone(), content);
        group.add(&row);
    }

    content.append(&group);
}

fn build_location_row(
    loc: &crate::weather::models::SavedLocation,
    state: Rc<RefCell<AppState>>,
    _nav: adw::NavigationView,
    content: &gtk::Box,
) -> adw::ActionRow {
    let subtitle = format!(
        "ICAO: {} · {:.0} km away",
        loc.station.icao, loc.station_distance_km
    );

    let row = adw::ActionRow::builder()
        .title(loc.place.name.as_str())
        .subtitle(&subtitle)
        .build();

    // Station name as a secondary label
    let station_label = gtk::Label::builder()
        .label(&loc.station.city)
        .css_classes(["dim-label", "caption"])
        .valign(gtk::Align::Center)
        .build();
    row.add_suffix(&station_label);

    // Remove button
    let remove_btn = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Remove this city")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .build();

    let loc_clone = loc.clone();
    let content_clone = content.clone();

    remove_btn.connect_clicked(clone!(@strong state => move |_| {
        let loc2 = loc_clone.clone();
        let st = Rc::clone(&state);
        let ct = content_clone.clone();

        MainContext::default().spawn_local(async move {
            match sync::remove_location(&loc2, &st.borrow().db).await {
                Ok(_) => {
                    // Remove from in-memory list
                    st.borrow_mut()
                        .saved_locations
                        .retain(|l| l.id != loc2.id);

                    // Rebuild the list widget
                    rebuild_list(&ct, Rc::clone(&st), adw::NavigationView::new());
                }
                Err(e) => tracing::error!("Failed to remove location: {:#}", e),
            }
        });
    }));

    row.add_suffix(&remove_btn);
    row
}