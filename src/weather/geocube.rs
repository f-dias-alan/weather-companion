// weather-companion/src/weather/geoclue.rs
//
// Optional automatic geolocation via GeoClue2 D-Bus service.
//
// GeoClue2 is the standard Linux location service used by GNOME Maps, GNOME
// Weather, and other location-aware applications. It is available out-of-the-box
// on all modern GNOME desktops and inside Flatpak sandboxes (with the correct
// portal permissions).
//
// FLATPAK NOTE:
// The manifest must include:
//   --talk-name=org.freedesktop.GeoClue2
// or use the xdg-desktop-portal location API instead. We use GeoClue directly
// here because it gives us a one-shot async call without the portal round-trip.
//
// PRIVACY NOTE:
// We request accuracy level 4 (CITY) — coarse enough not to reveal the exact
// street address, fine enough to pick the right city. We release the client
// immediately after the first fix to avoid background tracking.

use crate::weather::models::Coordinates;
use anyhow::{Context, Result};

// GeoClue accuracy levels (matches GClueAccuracyLevel enum in geoclue-2.0)
const ACCURACY_CITY: u32 = 4;

/// Attempts a single geolocation fix using GeoClue2.
///
/// This is an async function because D-Bus calls are non-blocking.
/// Returns `None` if GeoClue is unavailable (not installed, permission denied,
/// or location services are disabled) without treating it as a hard error.
pub async fn get_current_location() -> Option<Coordinates> {
    match try_geoclue().await {
        Ok(coords) => {
            tracing::info!(
                "GeoClue fix: lat={:.4} lon={:.4}",
                coords.latitude,
                coords.longitude
            );
            Some(coords)
        }
        Err(e) => {
            // GeoClue being unavailable is expected in many environments
            // (e.g. CI, Wayland compositors without the portal, older systems).
            tracing::info!("GeoClue unavailable: {:#}", e);
            None
        }
    }
}

async fn try_geoclue() -> Result<Coordinates> {
    // We use the zbus crate for D-Bus communication. Since zbus is not in our
    // Cargo.toml (to keep dependencies lean), this implementation uses raw
    // gio::DBusConnection via the GLib bindings.
    //
    // Step 1: Connect to the system bus
    let dbus = gio::bus_get_future(gio::BusType::System)
        .await
        .context("Failed to connect to system D-Bus")?;

    // Step 2: Create a GeoClue2 Manager client
    //   Interface: org.freedesktop.GeoClue2.Manager
    //   Object:    /org/freedesktop/GeoClue2/Manager
    let manager_proxy = gio::DBusProxy::new_future(
        &dbus,
        gio::DBusProxyFlags::NONE,
        None,
        Some("org.freedesktop.GeoClue2"),
        "/org/freedesktop/GeoClue2/Manager",
        "org.freedesktop.GeoClue2.Manager",
    )
    .await
    .context("Failed to create GeoClue2 Manager proxy")?;

    // Step 3: Call GetClient() to obtain a client object path
    let client_path_variant = manager_proxy
        .call_future("GetClient", None, gio::DBusCallFlags::NONE, 5000)
        .await
        .context("GeoClue2 Manager.GetClient() failed")?;

    let client_path: String = client_path_variant
        .child_value(0)
        .get::<String>()
        .context("Expected string client path from GetClient()")?;

    // Step 4: Configure and start the client
    let client_proxy = gio::DBusProxy::new_future(
        &dbus,
        gio::DBusProxyFlags::NONE,
        None,
        Some("org.freedesktop.GeoClue2"),
        &client_path,
        "org.freedesktop.GeoClue2.Client",
    )
    .await
    .context("Failed to create GeoClue2 Client proxy")?;

    // Set desktop ID (required by GeoClue2 >= 2.4.14)
    client_proxy
        .call_future(
            "org.freedesktop.DBus.Properties.Set",
            Some(&glib::Variant::from((
                "org.freedesktop.GeoClue2.Client",
                "DesktopId",
                glib::Variant::from("org.gnome.WeatherCompanion"),
            ))),
            gio::DBusCallFlags::NONE,
            5000,
        )
        .await
        .ok(); // Non-fatal if this fails on older geoclue

    // Set accuracy level to CITY
    client_proxy
        .call_future(
            "org.freedesktop.DBus.Properties.Set",
            Some(&glib::Variant::from((
                "org.freedesktop.GeoClue2.Client",
                "RequestedAccuracyLevel",
                glib::Variant::from(ACCURACY_CITY),
            ))),
            gio::DBusCallFlags::NONE,
            5000,
        )
        .await
        .context("Failed to set accuracy level")?;

    // Step 5: Start the client — GeoClue will fire a LocationUpdated signal
    client_proxy
        .call_future("Start", None, gio::DBusCallFlags::NONE, 5000)
        .await
        .context("GeoClue2 Client.Start() failed")?;

    // Step 6: Read the Location property
    let location_path_variant = client_proxy
        .call_future(
            "org.freedesktop.DBus.Properties.Get",
            Some(&glib::Variant::from((
                "org.freedesktop.GeoClue2.Client",
                "Location",
            ))),
            gio::DBusCallFlags::NONE,
            10_000, // 10 s timeout for GPS cold start
        )
        .await
        .context("Failed to get Location property")?;

    let location_path: String = location_path_variant
        .child_value(0)
        .get::<glib::Variant>()
        .and_then(|v| v.get::<String>())
        .context("Expected object path for Location")?;

    anyhow::ensure!(
        location_path != "/",
        "GeoClue returned no fix yet — location services may be disabled"
    );

    // Step 7: Read lat/lon from the Location object
    let loc_proxy = gio::DBusProxy::new_future(
        &dbus,
        gio::DBusProxyFlags::NONE,
        None,
        Some("org.freedesktop.GeoClue2"),
        &location_path,
        "org.freedesktop.GeoClue2.Location",
    )
    .await
    .context("Failed to create Location proxy")?;

    let lat_variant = loc_proxy
        .call_future(
            "org.freedesktop.DBus.Properties.Get",
            Some(&glib::Variant::from((
                "org.freedesktop.GeoClue2.Location",
                "Latitude",
            ))),
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await
        .context("Failed to read Latitude")?;

    let lon_variant = loc_proxy
        .call_future(
            "org.freedesktop.DBus.Properties.Get",
            Some(&glib::Variant::from((
                "org.freedesktop.GeoClue2.Location",
                "Longitude",
            ))),
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await
        .context("Failed to read Longitude")?;

    let lat: f64 = lat_variant
        .child_value(0)
        .get::<glib::Variant>()
        .and_then(|v| v.get::<f64>())
        .context("Latitude is not a double")?;

    let lon: f64 = lon_variant
        .child_value(0)
        .get::<glib::Variant>()
        .and_then(|v| v.get::<f64>())
        .context("Longitude is not a double")?;

    // Step 8: Stop the client immediately to release GPS/network resources
    client_proxy
        .call_future("Stop", None, gio::DBusCallFlags::NONE, 3000)
        .await
        .ok();

    Ok(Coordinates::new(lat, lon))
}