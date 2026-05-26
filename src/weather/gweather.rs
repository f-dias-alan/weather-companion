use crate::weather::models::SavedLocation;
use anyhow::{Context, Result};
use std::process::Command;

fn read_current() -> String {
    Command::new("gsettings")
        .args(["get", "org.gnome.Weather", "locations"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "@av []".to_string())
}

pub fn add_to_gnome_weather(loc: &SavedLocation) -> Result<bool> {
    let lat = loc.place.coordinates.latitude;
    let lon = loc.place.coordinates.longitude;
    let slat = loc.station.coordinates.latitude;
    let slon = loc.station.coordinates.longitude;
    let name = loc.place.name.replace('"', "");
    let country = loc.place.country_code.to_uppercase();
    let tz = timezone_for(lon);

    let current = read_current();
    let inner = current
        .trim_start_matches("@av ")
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();

    // Tipo explicito: m(dd) = maybe tuple of two doubles
    // Sem type hints o gsettings nao consegue inferir o tipo das coordenadas
    let new_entry = format!(
    "<(uint32 5, <(just \"{name}\", nothing, just ({lat}, {lon}), nothing, \"{country}\", \"{tz}\", true)>)>"
);

    let new_value = if inner.is_empty() {
        format!("[{}]", new_entry)
    } else {
        format!("[{}, {}]", inner, new_entry)
    };

    tracing::debug!("Writing to gsettings: {}", new_value);

    let status = Command::new("gsettings")
        .args(["set", "org.gnome.Weather", "locations", &new_value])
        .status()
        .context("Failed to run gsettings")?;

    anyhow::ensure!(status.success(), "gsettings set failed");
    tracing::info!("Added {} to GNOME Weather", loc.place.name);
    Ok(true)
}

pub fn remove_from_gnome_weather(loc: &SavedLocation) -> Result<bool> {
    tracing::warn!(
        "To remove manually: gsettings reset org.gnome.Weather locations"
    );
    let status = Command::new("gsettings")
        .args(["reset", "org.gnome.Weather", "locations"])
        .status()
        .context("Failed to run gsettings reset")?;
    Ok(status.success())
}

pub fn read_gnome_weather_locations() -> Result<Vec<glib::Variant>> {
    Ok(vec![])
}

fn timezone_for(lon: f64) -> String {
    if lon < -51.0 {
        "America/Fortaleza".to_string()
    } else {
        "America/Sao_Paulo".to_string()
    }
}
