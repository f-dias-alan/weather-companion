// weather-companion/src/weather/models.rs
//
// Plain data types shared across the application.
// All types derive serde traits so they can be stored in SQLite as JSON blobs
// and serialised for the GNOME Weather gsettings key.

use serde::{Deserialize, Serialize};

/// A geographic coordinate pair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinates {
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self { latitude, longitude }
    }
}

/// A city or place found via Nominatim search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    /// Human-readable display name from Nominatim.
    pub display_name: String,
    /// Short name (city/town/village name only).
    pub name: String,
    /// State / province.
    pub state: Option<String>,
    /// ISO 3166-1 alpha-2 country code.
    pub country_code: String,
    pub coordinates: Coordinates,
    /// OpenStreetMap place_id for deduplication.
    pub osm_id: Option<u64>,
}

/// An airport/weather station entry from the OpenFlights database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirportStation {
    /// ICAO 4-letter identifier (e.g. "SBMS" for Mossoró).
    /// This is the value libgweather expects.
    pub icao: String,
    /// IATA 3-letter code (may be empty for military/private airports).
    pub iata: Option<String>,
    pub name: String,
    pub city: String,
    pub country: String,
    pub coordinates: Coordinates,
    /// Elevation in metres above sea level.
    pub elevation_m: Option<f32>,
}

/// A saved location: a user-chosen place paired with its nearest ICAO station.
/// This is the unit stored in our SQLite database and eventually written to
/// the GNOME Weather gsettings key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedLocation {
    /// Unique ID assigned by our database.
    pub id: Option<i64>,
    pub place: Place,
    /// Nearest ICAO weather station found for this place.
    pub station: AirportStation,
    /// Distance (km) between the place and the selected station.
    pub station_distance_km: f64,
    /// Timestamp of when this location was added (Unix seconds).
    pub created_at: i64,
}

/// Result of writing a location to GNOME Weather.
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub location: SavedLocation,
    /// true if GNOME Weather was updated; false if it was already present.
    pub was_new: bool,
}