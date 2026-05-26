use crate::storage::database::Database;
use crate::weather::{gweather, icao::nearest_station, models::SavedLocation};
use crate::weather::models::Place;
use anyhow::{Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn add_location(place: Place, db: &Database) -> Result<SavedLocation> {
    let (station, dist_km) = nearest_station(place.coordinates).context("Airport database is empty")?;
    tracing::info!("Nearest station for {:?}: {} at {:.1} km", place.name, station.icao, dist_km);
    if dist_km > 150.0 {
        tracing::warn!("Station {} is {:.0} km away", station.icao, dist_km);
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let mut loc = SavedLocation { id: None, place, station, station_distance_km: dist_km, created_at: now };
    let id = db.insert_location(&loc).context("Failed to save location")?;
    loc.id = Some(id);
    match gweather::add_to_gnome_weather(&loc) {
        Ok(_) => tracing::info!("Synced {} to GNOME Weather", loc.place.name),
        Err(e) => tracing::warn!("Could not sync to GNOME Weather: {:#}", e),
    }
    Ok(loc)
}

pub async fn remove_location(loc: &SavedLocation, db: &Database) -> Result<()> {
    match gweather::remove_from_gnome_weather(loc) {
        Ok(removed) => { if !removed { tracing::warn!("{} not found in GNOME Weather", loc.place.name); } }
        Err(e) => tracing::warn!("Could not remove from GNOME Weather: {:#}", e),
    }
    if let Some(id) = loc.id {
        db.delete_location(id).context("Failed to delete from database")?;
    }
    Ok(())
}

pub async fn resync_all(db: &Database) -> Result<usize> {
    let locations = db.list_locations()?;
    let mut count = 0usize;
    for loc in &locations {
        match gweather::add_to_gnome_weather(loc) {
            Ok(_) => count += 1,
            Err(e) => tracing::warn!("Failed to sync {}: {:#}", loc.place.name, e),
        }
    }
    tracing::info!("Re-synced {}/{} locations", count, locations.len());
    Ok(count)
}
