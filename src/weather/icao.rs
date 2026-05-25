// weather-companion/src/weather/icao.rs
//
// Loads the OpenFlights airport database and finds the nearest ICAO weather
// station to a given coordinate using the Haversine great-circle formula.
//
// WHY ICAO?
// libgweather's internal location database identifies weather stations by their
// ICAO code. When you ask GNOME Weather to show a city it doesn't know about,
// it still needs a valid ICAO code to fetch METAR/weather data. Providing an
// invalid or missing ICAO causes libgweather to silently fail or crash.
//
// STRATEGY:
// We bundle airports.csv (from openflights.org) in the Flatpak and parse it at
// startup into an in-memory Vec. Lookups are O(n) with n ≈ 14,000 which is
// fast enough (< 1 ms) for interactive use without needing a spatial index.
// A future optimisation could use a KD-tree (e.g. the `kiddo` crate).

use crate::weather::models::{AirportStation, Coordinates};
use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use std::path::Path;

/// Global airport database, loaded once at startup.
static AIRPORT_DB: OnceCell<Vec<AirportStation>> = OnceCell::new();

/// Returns a reference to the global airport database.
/// Panics if [`load_airports`] has not been called first.
pub fn airport_db() -> &'static Vec<AirportStation> {
    AIRPORT_DB.get().expect("Airport database not loaded")
}

/// Loads and parses the OpenFlights airports.csv file.
///
/// # File format
/// The OpenFlights CSV has these columns (no header row):
/// 0: Airport ID (integer)
/// 1: Name
/// 2: City
/// 3: Country
/// 4: IATA (3-letter, or "\\N")
/// 5: ICAO (4-letter, or "\\N")
/// 6: Latitude
/// 7: Longitude
/// 8: Altitude (feet)
/// 9: Timezone offset
/// 10: DST
/// 11: Tz database timezone
/// 12: Type ("airport", "station", "port", "unknown")
/// 13: Source
pub fn load_airports(csv_path: &Path) -> Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(csv_path)
        .with_context(|| format!("Failed to open airports CSV at {:?}", csv_path))?;

    let mut stations: Vec<AirportStation> = Vec::with_capacity(15_000);

    for result in reader.records() {
        let record = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Skipping malformed CSV row: {}", e);
                continue;
            }
        };

        // We need at least 9 columns to extract lat/lon/elevation.
        if record.len() < 9 {
            continue;
        }

        // Parse ICAO code — skip rows without a valid 4-letter code.
        let icao = record.get(5).unwrap_or("\\N").trim().to_uppercase();
        if icao == "\\N" || icao.len() != 4 {
            continue;
        }

        let iata_raw = record.get(4).unwrap_or("\\N").trim();
        let iata = if iata_raw == "\\N" || iata_raw.is_empty() {
            None
        } else {
            Some(iata_raw.to_string())
        };

        let lat: f64 = match record.get(6).and_then(|s| s.trim().parse().ok()) {
            Some(v) => v,
            None => continue,
        };
        let lon: f64 = match record.get(7).and_then(|s| s.trim().parse().ok()) {
            Some(v) => v,
            None => continue,
        };

        // Elevation is in feet in OpenFlights; convert to metres.
        let elevation_m: Option<f32> = record
            .get(8)
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|ft| ft * 0.3048);

        stations.push(AirportStation {
            icao,
            iata,
            name: record.get(1).unwrap_or("").trim().to_string(),
            city: record.get(2).unwrap_or("").trim().to_string(),
            country: record.get(3).unwrap_or("").trim().to_string(),
            coordinates: Coordinates::new(lat, lon),
            elevation_m,
        });
    }

    tracing::info!("Loaded {} ICAO stations", stations.len());

    AIRPORT_DB
        .set(stations)
        .map_err(|_| anyhow::anyhow!("Airport database already loaded"))?;

    Ok(())
}

// ─── Haversine distance ────────────────────────────────────────────────────────

/// Earth's mean radius in kilometres.
const EARTH_RADIUS_KM: f64 = 6371.0;

/// Computes the great-circle distance between two coordinate pairs using the
/// Haversine formula.
///
/// d = 2r · arcsin(√(sin²(Δφ/2) + cos(φ₁)·cos(φ₂)·sin²(Δλ/2)))
///
/// Returns kilometres.
pub fn haversine_km(a: Coordinates, b: Coordinates) -> f64 {
    let d_lat = (b.latitude - a.latitude).to_radians();
    let d_lon = (b.longitude - a.longitude).to_radians();

    let lat1 = a.latitude.to_radians();
    let lat2 = b.latitude.to_radians();

    let sin_dlat = (d_lat / 2.0).sin();
    let sin_dlon = (d_lon / 2.0).sin();

    let h = sin_dlat * sin_dlat + lat1.cos() * lat2.cos() * sin_dlon * sin_dlon;

    2.0 * EARTH_RADIUS_KM * h.sqrt().asin()
}

// ─── Nearest station lookup ───────────────────────────────────────────────────

/// Finds the closest ICAO weather station to `coords`.
///
/// Returns `None` only if the airport database is empty (should not happen in
/// normal operation).
pub fn nearest_station(coords: Coordinates) -> Option<(AirportStation, f64)> {
    let db = airport_db();

    db.iter()
        .map(|station| {
            let dist = haversine_km(coords, station.coordinates);
            (station, dist)
        })
        .min_by(|(_, d1), (_, d2)| d1.partial_cmp(d2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(station, dist)| (station.clone(), dist))
}

/// Like [`nearest_station`] but returns the `k` closest stations, sorted by
/// ascending distance. Useful for letting the user choose among candidates.
pub fn k_nearest_stations(coords: Coordinates, k: usize) -> Vec<(AirportStation, f64)> {
    let db = airport_db();

    let mut ranked: Vec<(&AirportStation, f64)> = db
        .iter()
        .map(|s| (s, haversine_km(coords, s.coordinates)))
        .collect();

    // Partial sort would be more efficient, but n ≈ 14,000 is small enough that
    // a full sort is imperceptibly fast in practice.
    ranked.sort_by(|(_, d1), (_, d2)| d1.partial_cmp(d2).unwrap_or(std::cmp::Ordering::Equal));
    ranked
        .into_iter()
        .take(k)
        .map(|(s, d)| (s.clone(), d))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_known_distance() {
        // Distance between Natal (RN) and Fortaleza (CE) is approximately 537 km.
        let natal = Coordinates::new(-5.7793, -35.2009);
        let fortaleza = Coordinates::new(-3.7172, -38.5433);
        let dist = haversine_km(natal, fortaleza);
        assert!((dist - 537.0).abs() < 10.0, "Got {} km", dist);
    }

    #[test]
    fn haversine_same_point() {
        let p = Coordinates::new(-6.0, -38.0);
        assert!(haversine_km(p, p) < 1e-9);
    }
}