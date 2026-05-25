// weather-companion/src/services/sync.rs
//
// High-level service that orchestrates the full "add location" workflow:
//   1. Geocode user query → Place
//   2. Find nearest ICAO station → AirportStation
//   3. Persist to local SQLite database
//   4. Write to GNOME Weather gsettings

use crate::storage::database::Database;
use crate::weather::{
    gweather,
    icao::nearest_station,
    models::{Coordinates, SavedLocation},
};
use anyhow::{Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

/// Adds a location (by coordinates + name, already geocoded) to both the local
/// database and GNOME Weather.
///
/// Returns the persisted SavedLocation including its database ID.
pub async fn add_location(
    place: crate::weather::models::Place,
    db: &Database,
) -> Result<SavedLocation> {
    // Find nearest ICAO station.
    let (station, dist_km) =
        nearest_station(place.coordinates).context("Airport database is empty")?;

    tracing::info!(
        "Nearest station for {:?}: {} at {:.1} km",
        place.name,
        station.icao,
        dist_km
    );

    // Warn if the station is very far — the weather data may be irrelevant.
    if dist_km > 150.0 {
        tracing::warn!(
            "Station {} is {:.0} km away — weather data may differ significantly",
            station.icao,
            dist_km
        );
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut loc = SavedLocation {
        id: None,
        place,
        station,
        station_distance_km: dist_km,
        created_at: now,
    };

    // Persist to SQLite.
    let id = db.insert_location(&loc).context("Failed to save location")?;
    loc.id = Some(id);

    // Write to GNOME Weather.
    match gweather::add_to_gnome_weather(&loc) {
        Ok(_) => tracing::info!("Synced {} to GNOME Weather", loc.place.name),
        Err(e) => {
            // Non-fatal: the location is still saved locally.
            tracing::warn!("Could not sync to GNOME Weather: {:#}", e);
        }
    }

    Ok(loc)
}

/// Removes a location from both the local database and GNOME Weather.
pub async fn remove_location(loc: &SavedLocation, db: &Database) -> Result<()> {
    // Remove from GNOME Weather first (so any error doesn't leave an orphaned
    // local record).
    match gweather::remove_from_gnome_weather(loc) {
        Ok(removed) => {
            if removed {
                tracing::info!("Removed {} from GNOME Weather", loc.place.name);
            } else {
                tracing::warn!(
                    "{} was not found in GNOME Weather (may have been removed manually)",
                    loc.place.name
                );
            }
        }
        Err(e) => tracing::warn!("Could not remove from GNOME Weather: {:#}", e),
    }

    // Remove from local DB.
    if let Some(id) = loc.id {
        db.delete_location(id)
            .context("Failed to delete location from local database")?;
    }

    Ok(())
}

/// Re-syncs all locally saved locations to GNOME Weather.
///
/// Useful after a GNOME Weather reinstall or reset.
pub async fn resync_all(db: &Database) -> Result<usize> {
    let locations = db.list_locations()?;
    let mut count = 0usize;

    for loc in &locations {
        match gweather::add_to_gnome_weather(loc) {
            Ok(_) => count += 1,
            Err(e) => tracing::warn!("Failed to sync {}: {:#}", loc.place.name, e),
        }
    }

    tracing::info!("Re-synced {}/{} locations to GNOME Weather", count, locations.len());
    Ok(count)
}