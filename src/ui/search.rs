use crate::services::{nominatim, sync};
use crate::ui::window::AppState;
use crate::weather::geocube;
use crate::weather::models::Place;
use glib::clone;
use glib::MainContext;
use gtk4::{self as gtk, prelude::*};
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;

pub struct SearchPage;

impl SearchPage {
    pub fn build(state: Rc<RefCell<AppState>>, nav: adw::NavigationView) -> adw::NavigationPage {
        let header = adw::HeaderBar::new();
        let manage_btn = gtk::Button::builder()
            .icon_name("view-list-symbolic")
            .tooltip_text("Manage saved cities")
            .build();
        let nav_for_manage = nav.clone();
        manage_btn.connect_clicked(move |_| { nav_for_manage.push_by_tag("locations"); });
        header.pack_end(&manage_btn);

        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Search for a city...")
            .hexpand(true)
            .build();
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
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
        let status_label = gtk::Label::builder()
            .halign(gtk::Align::Center)
            .visible(false)
            .css_classes(["dim-label"])
            .build();
        let locate_btn = gtk::Button::builder()
            .label("Use my location")
            .css_classes(["suggested-action"])
            .margin_top(12)
            .build();

        let vbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical).spacing(8)
            .margin_start(12).margin_end(12).margin_top(12).margin_bottom(12)
            .build();
        let search_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal).spacing(8).build();
        search_row.append(&search_entry);
        search_row.append(&spinner);
        vbox.append(&search_row);
        vbox.append(&status_label);
        vbox.append(&locate_btn);

        let clamp = adw::Clamp::builder().maximum_size(600).child(&vbox).build();
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&clamp));

        let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        search_entry.connect_changed(clone!(
            #[strong] state, #[strong] spinner, #[strong] results_list,
            #[strong] popover, #[strong] status_label, #[strong] pending,
            move |entry| {
                let query = entry.text().to_string();
                let _ = pending.borrow_mut().take();
                if query.len() < 3 { popover.popdown(); return; }
                let spinner2 = spinner.clone();
                let rl = results_list.clone();
                let pop = popover.clone();
                let sl = status_label.clone();
                let st = Rc::clone(&state);
                let id = glib::timeout_add_local(
                    std::time::Duration::from_millis(400),
                    clone!(#[strong] spinner2, #[strong] rl, #[strong] pop, #[strong] sl, move || {
                        spinner2.set_visible(true);
                        spinner2.start();
                        let q = query.clone();
                        let sp = spinner2.clone();
                        let rl2 = rl.clone();
                        let pop2 = pop.clone();
                        let sl2 = sl.clone();
                        let st2 = Rc::clone(&st);
                        MainContext::default().spawn_local(async move {
                            match nominatim::search(&q, 5).await {
                                Ok(places) => { populate_results(&rl2, &pop2, places, st2); sl2.set_visible(false); }
                                Err(e) => { sl2.set_text(&format!("Search failed: {}", e)); sl2.set_visible(true); pop2.popdown(); }
                            }
                            sp.stop();
                            sp.set_visible(false);
                        });
                        glib::ControlFlow::Break
                    }),
                );
                *pending.borrow_mut() = Some(id);
            }
        ));

        locate_btn.connect_clicked(clone!(
            #[strong] state, #[strong] spinner, #[strong] status_label,
            move |btn| {
                btn.set_sensitive(false);
                spinner.set_visible(true);
                spinner.start();
                let btn2 = btn.clone();
                let sp = spinner.clone();
                let sl = status_label.clone();
                let st = Rc::clone(&state);
                MainContext::default().spawn_local(async move {
                    match geocube::get_current_location().await {
                        Some(coords) => match nominatim::reverse(coords).await {
                            Ok(Some(place)) => {
                                // Extrai db sem manter o borrow vivo durante o await
                                let result = {
                                    sync::add_location(place, &st.borrow().db).await
                                };
                                match result {
                                    Ok(loc) => {
                                        st.borrow_mut().saved_locations.push(loc);
                                        sl.set_text("Location added!");
                                        sl.set_visible(true);
                                    }
                                    Err(e) => { sl.set_text(&format!("Error: {}", e)); sl.set_visible(true); }
                                }
                            }
                            Ok(None) => { sl.set_text("Could not identify location"); sl.set_visible(true); }
                            Err(e) => { sl.set_text(&format!("Geocoding failed: {}", e)); sl.set_visible(true); }
                        },
                        None => { sl.set_text("Location services unavailable"); sl.set_visible(true); }
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

fn populate_results(list: &gtk::ListBox, popover: &gtk::Popover, places: Vec<Place>, state: Rc<RefCell<AppState>>) {
    while let Some(child) = list.first_child() { list.remove(&child); }
    if places.is_empty() { popover.popdown(); return; }
    for place in places {
        let row = build_result_row(&place, Rc::clone(&state), popover.clone());
        list.append(&row);
    }
    popover.popup();
}

fn build_result_row(place: &Place, state: Rc<RefCell<AppState>>, popover: gtk::Popover) -> gtk::ListBoxRow {
    let hbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal).spacing(8)
        .margin_start(8).margin_end(8).margin_top(6).margin_bottom(6)
        .build();
    let name_label = gtk::Label::builder()
        .label(&place.name).halign(gtk::Align::Start).hexpand(true).build();
    let sub = place.state.as_deref()
        .map(|s| format!("{}, {}", s, place.country_code))
        .unwrap_or_else(|| place.country_code.clone());
    let sub_label = gtk::Label::builder()
        .label(&sub).halign(gtk::Align::End).css_classes(["dim-label"]).build();
    hbox.append(&name_label);
    hbox.append(&sub_label);

    let row = gtk::ListBoxRow::builder()
        .child(&hbox)
        .activatable(true)
        .build();

    let gesture = gtk::GestureClick::new();
    let place_click = place.clone();
    let state_click = Rc::clone(&state);
    let pop_click = popover.clone();
    gesture.connect_released(move |_, _, _, _| {
        let p = place_click.clone();
        let st = Rc::clone(&state_click);
        let pop = pop_click.clone();
        MainContext::default().spawn_local(async move {
            // Borrow apenas para obter o que precisamos, depois soltar
            let result = sync::add_location(p, &st.borrow().db).await;
            match result {
                Ok(loc) => { st.borrow_mut().saved_locations.push(loc); }
                Err(e) => tracing::error!("Failed to add: {:#}", e),
            }
            pop.popdown();
        });
    });
    row.add_controller(gesture);

    let place_key = place.clone();
    let state_key = Rc::clone(&state);
    row.connect_activate(move |_| {
        let p = place_key.clone();
        let st = Rc::clone(&state_key);
        let pop = popover.clone();
        MainContext::default().spawn_local(async move {
            let result = sync::add_location(p, &st.borrow().db).await;
            match result {
                Ok(loc) => { st.borrow_mut().saved_locations.push(loc); }
                Err(e) => tracing::error!("Failed to add: {:#}", e),
            }
            pop.popdown();
        });
    });

    row
}
