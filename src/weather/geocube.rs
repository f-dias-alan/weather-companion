use crate::weather::models::Coordinates;
use anyhow::{Context, Result};
use gio::prelude::*;

pub async fn get_current_location() -> Option<Coordinates> {
    match try_geoclue().await {
        Ok(c) => { tracing::info!("GeoClue: {:.4},{:.4}", c.latitude, c.longitude); Some(c) }
        Err(e) => { tracing::info!("GeoClue unavailable: {:#}", e); None }
    }
}

async fn try_geoclue() -> Result<Coordinates> {
    let dbus = gio::bus_get_future(gio::BusType::System).await.context("D-Bus connect failed")?;

    let manager = gio::DBusProxy::new_future(&dbus, gio::DBusProxyFlags::NONE, None,
        Some("org.freedesktop.GeoClue2"), "/org/freedesktop/GeoClue2/Manager",
        "org.freedesktop.GeoClue2.Manager").await.context("Manager proxy failed")?;

    let client_path: String = manager.call_future("GetClient", None, gio::DBusCallFlags::NONE, 5000)
        .await.context("GetClient failed")?.child_value(0).get::<String>().context("Bad client path")?;

    let client = gio::DBusProxy::new_future(&dbus, gio::DBusProxyFlags::NONE, None,
        Some("org.freedesktop.GeoClue2"), &client_path,
        "org.freedesktop.GeoClue2.Client").await.context("Client proxy failed")?;

    client.call_future("org.freedesktop.DBus.Properties.Set",
        Some(&glib::Variant::from(("org.freedesktop.GeoClue2.Client", "RequestedAccuracyLevel", glib::Variant::from(4u32)))),
        gio::DBusCallFlags::NONE, 5000).await.context("Set accuracy failed")?;

    client.call_future("Start", None, gio::DBusCallFlags::NONE, 5000).await.context("Start failed")?;

    let loc_path: String = client.call_future("org.freedesktop.DBus.Properties.Get",
        Some(&glib::Variant::from(("org.freedesktop.GeoClue2.Client", "Location"))),
        gio::DBusCallFlags::NONE, 10_000).await.context("Get Location failed")?
        .child_value(0).get::<glib::Variant>().and_then(|v| v.get::<String>()).context("Bad location path")?;

    anyhow::ensure!(loc_path != "/", "No fix yet");

    let loc = gio::DBusProxy::new_future(&dbus, gio::DBusProxyFlags::NONE, None,
        Some("org.freedesktop.GeoClue2"), &loc_path,
        "org.freedesktop.GeoClue2.Location").await.context("Location proxy failed")?;

    let lat: f64 = loc.call_future("org.freedesktop.DBus.Properties.Get",
        Some(&glib::Variant::from(("org.freedesktop.GeoClue2.Location", "Latitude"))),
        gio::DBusCallFlags::NONE, 3000).await.context("Read Latitude failed")?
        .child_value(0).get::<glib::Variant>().and_then(|v| v.get::<f64>()).context("Latitude not f64")?;

    let lon: f64 = loc.call_future("org.freedesktop.DBus.Properties.Get",
        Some(&glib::Variant::from(("org.freedesktop.GeoClue2.Location", "Longitude"))),
        gio::DBusCallFlags::NONE, 3000).await.context("Read Longitude failed")?
        .child_value(0).get::<glib::Variant>().and_then(|v| v.get::<f64>()).context("Longitude not f64")?;

    client.call_future("Stop", None, gio::DBusCallFlags::NONE, 3000).await.ok();

    Ok(Coordinates::new(lat, lon))
}
