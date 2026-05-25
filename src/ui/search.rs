// weather-companion/src/ui/search.rs
//
// The main search page: a search entry with a live dropdown of Nominatim
// results, a spinner for loading state, and a list of recently added cities.
//
// SEARCH DEBOUNCE
// ───────────────
// We debounce the search entry's "changed" signal by 400 ms using
// glib::timeout_add_local(). Each keystroke cancels any pending timer and
// starts a new one. When the timer fires, we issue the Nominatim request.
// This avoids hammering the API on every character.
//
// DROPDOWN IMPLEMENTATION
// ────────────────────────
// We use a gtk::Popover attached to the search entry (not gtk::EntryCompletion
// which is deprecated in GTK4). The popover contains a gtk::ListBox populated
// with gtk::ListBoxRow items, one per result.

use crate::services::nominatim;
use crate::services::sync;
use crate::ui::window::AppState;
use crate::weather::{geoclue, icao::nearest_station, models::Place};
use glib::{clone, MainContext};
use gtk4::{self as gtk, prelude::*};
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;

pub struct SearchPage;

impl SearchPage {
    pub fn build(state: Rc<RefCell<AppState>>, nav: adw::NavigationView) -> adw::NavigationPage {
        // ── Header bar ──────────────────────────────────────────────────────
        let header = adw::HeaderBar::new();

        // "Manage cities" button → push the locations page
        let manage_btn = gtk::Button::builder()
            .icon_name("view-list-symbolic")
            .tooltip_text("Manage saved cities")
            .build();

        let nav_for_manage = nav.clone();
        manage_btn.connect_clicked(move |_| {
            nav_for_manage.push_by_tag("locations");
        });
        header.pack_end(&manage_btn);

        // ── Search entry ─────────────────────────────────────────────────────
        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Search for a city…")
            .hexpand(true)
            .build();

        // ── Spinner ──────────────────────────────────────────────────────────
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);

        // ── Results list (inside a Popover) ───────────────────────────────────
        let results_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();

        let popover = gtk::Popover::builder()
            .child(&results_list)
            .autohide(true)
            .has_arrow(false)
            .build();
        popover.set_parent(&search_entry);

        // ── Status / error label ──────────────────────────────────────────────
        let status_label = gtk::Label::builder()
            .halign(gtk::Align::Center)
            .visible(false)
            .css_classes(["dim-label"])
            .build();

        // ── Locate me button (GeoClue) ────────────────────────────────────────
        let locate_btn = gtk::Button::builder()
            .label("Use my location")
            .icon_name("find-location-symbolic")
            .css_classes(["suggested-action"])
            .margin_top(12)
            .build();

        // ── Main content layout ───────────────────────────────────────────────
        let vbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();

        let search_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        search_row.append(&search_entry);
        search_row.append(&spinner);

        vbox.append(&search_row);
        vbox.append(&status_label);
        vbox.append(&locate_btn);

        let clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&vbox)
            .build();

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&clamp));

        // ── Debounced search ──────────────────────────────────────────────────
        // We store the pending timeout ID so we can cancel it on each new keystroke.
        let pending_timeout: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        search_entry.connect_changed(clone!(
            @strong state,
            @strong spinner,
            @strong results_list,
            @strong popover,
            @strong status_label,
            @strong pending_timeout
            => move |entry| {
                let query = entry.text().to_string();

                // Cancel any existing timer
                if let Some(id) = pending_timeout.borrow_mut().take() {
                    id.remove();
                }

                if query.len() < 3 {
                    popover.popdown();
                    return;
                }

                // Schedule a new request after 400 ms
                let spinner2 = spinner.clone();
                let results_list2 = results_list.clone();
                let popover2 = popover.clone();
                let status_label2 = status_label.clone();
                let state2 = Rc::clone(&state);

                let id = glib::timeout_add_local(
                    std::time::Duration::from_millis(400),
                    clone!(@strong spinner2, @strong results_list2, @strong popover2, @strong status_label2
                        => move || {
                        spinner2.set_visible(true);
                        spinner2.start();

                        let q = query.clone();
                        let sp = spinner2.clone();
                        let rl = results_list2.clone();
                        let pop = popover2.clone();
                        let sl = status_label2.clone();
                        let st = Rc::clone(&state2);

                        MainContext::default().spawn_local(async move {
                            match nominatim::search(&q, 5).await {
                                Ok(places) => {
                                    populate_results(&rl, &pop, places, Rc::clone(&st));
                                    sl.set_visible(false);
                                }
                                Err(e) => {
                                    sl.set_text(&format!("Search failed: {}", e));
                                    sl.set_visible(true);
                                    pop.popdown();
                                }
                            }
                            sp.stop();
                            sp.set_visible(false);
                        });

                        glib::ControlFlow::Break
                    }),
                );

                *pending_timeout.borrow_mut() = Some(id);
            }
        ));

        // ── GeoClue locate button ─────────────────────────────────────────────
        locate_btn.connect_clicked(clone!(
            @strong state,
            @strong spinner,
            @strong status_label
            => move |btn| {
                btn.set_sensitive(false);
                spinner.set_visible(true);
                spinner.start();

                let btn2 = btn.clone();
                let sp = spinner.clone();
                let sl = status_label.clone();
                let st = Rc::clone(&state);

                MainContext::default().spawn_local(async move {
                    match geoclue::get_current_location().await {
                        Some(coords) => {
                            match nominatim::reverse(coords).await {
                                Ok(Some(place)) => {
                                    let result = sync::add_location(place, &st.borrow().db).await;
                                    match result {
                                        Ok(_) => {
                                            sl.set_text("Location added!");
                                            sl.set_visible(true);
                                        }
                                        Err(e) => {
                                            sl.set_text(&format!("Error: {}", e));
                                            sl.set_visible(true);
                                        }
                                    }
                                }
                                Ok(None) => {
                                    sl.set_text("Could not identify your location");
                                    sl.set_visible(true);
                                }
                                Err(e) => {
                                    sl.set_text(&format!("Geocoding failed: {}", e));
                                    sl.set_visible(true);
                                }
                            }
                        }
                        None => {
                            sl.set_text("Location services unavailable");
                            sl.set_visible(true);
                        }
                    }

                    sp.stop();
                    sp.set_visible(false);
                    btn2.set_sensitive(true);
                });
            }
        ));

        adw::NavigationPage::builder()
            .title("Weather Companion")
            .tag("search")
            .child(&toolbar_view)
            .build()
    }
}

/// Clears and repopulates the results list box.
fn populate_results(
    list: &gtk::ListBox,
    popover: &gtk::Popover,
    places: Vec<Place>,
    state: Rc<RefCell<AppState>>,
) {
    // Remove all existing rows
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if places.is_empty() {
        popover.popdown();
        return;
    }

    for place in places {
        let row = build_result_row(&place, Rc::clone(&state), popover.clone());
        list.append(&row);
    }

    // Position and show the popover below the search entry
    popover.popup();
}

/// Builds a single result row for the dropdown.
fn build_result_row(
    place: &Place,
    state: Rc<RefCell<AppState>>,
    popover: gtk::Popover,
) -> gtk::ListBoxRow {
    let hbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(8)
        .margin_end(8)
        .margin_top(6)
        .margin_bottom(6)
        .build();

    let name_label = gtk::Label::builder()
        .label(&place.name)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();

    let sub = place
        .state
        .as_deref()
        .map(|s| format!("{}, {}", s, place.country_code))
        .unwrap_or_else(|| place.country_code.clone());

    let sub_label = gtk::Label::builder()
        .label(&sub)
        .halign(gtk::Align::End)
        .css_classes(["dim-label"])
        .build();

    hbox.append(&name_label);
    hbox.append(&sub_label);

    let row = gtk::ListBoxRow::builder().child(&hbox).build();

    let place_clone = place.clone();
    row.connect_activate(move |_| {
        let place2 = place_clone.clone();
        let state2 = Rc::clone(&state);
        let pop = popover.clone();

        MainContext::default().spawn_local(async move {
            match sync::add_location(place2, &state2.borrow().db).await {
                Ok(loc) => {
                    state2.borrow_mut().saved_locations.push(loc);
                    tracing::info!("Location added successfully");
                }
                Err(e) => tracing::error!("Failed to add location: {:#}", e),
            }
            pop.popdown();
        });
    });

    row
}