// weather-companion/src/services/nominatim.rs
//
// Wraps the Nominatim geocoding API (https://nominatim.openstreetmap.org).
//
// USAGE POLICY
// ─────────────
// Nominatim's usage policy requires:
//   1. A valid User-Agent header identifying our application.
//   2. No more than 1 request per second.
//   3. No bulk geocoding (we query only in response to user input).
//
// We enforce the 1 req/s limit with a simple in-memory rate limiter.
// For production scale consider self-hosting or using a commercial provider.

use crate::weather::models::{Coordinates, Place};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const NOMINATIM_BASE: &str = "https://nominatim.openstreetmap.org";
const USER_AGENT: &str = concat!(
    "WeatherCompanion/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/example/weather-companion)"
);
const MIN_INTERVAL: Duration = Duration::from_millis(1100); // ≥ 1 req/s

/// Global rate-limiter state: timestamp of last request.
static LAST_REQUEST: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));

// ─── Nominatim JSON response types ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct NominatimResult {
    place_id: Option<u64>,
    display_name: String,
    lat: String,
    lon: String,
    address: Option<NominatimAddress>,
}

#[derive(Debug, Deserialize)]
struct NominatimAddress {
    city: Option<String>,
    town: Option<String>,
    village: Option<String>,
    municipality: Option<String>,
    state: Option<String>,
    country_code: Option<String>,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Searches for places matching `query` using Nominatim's /search endpoint.
///
/// Returns up to `limit` results ordered by Nominatim's relevance score.
/// The query can be anything the user types, e.g. "Pau dos Ferros RN".
pub async fn search(query: &str, limit: u8) -> Result<Vec<Place>> {
    let client = build_client()?;
    rate_limit().await;

    let url = format!(
        "{}/search?q={}&format=jsonv2&addressdetails=1&limit={}",
        NOMINATIM_BASE,
        urlencoding::encode(query),
        limit
    );

    tracing::debug!("Nominatim search: {}", url);

    let response = client
        .get(&url)
        .send()
        .await
        .context("Nominatim request failed")?;

    let status = response.status();
    anyhow::ensure!(
        status.is_success(),
        "Nominatim returned HTTP {}",
        status
    );

    let results: Vec<NominatimResult> = response
        .json()
        .await
        .context("Failed to deserialise Nominatim response")?;

    let places = results
        .into_iter()
        .filter_map(|r| try_parse_result(r))
        .collect();

    Ok(places)
}

/// Reverse-geocodes a coordinate to a place name.
/// Used when GeoClue provides an automatic location.
pub async fn reverse(coords: Coordinates) -> Result<Option<Place>> {
    let client = build_client()?;
    rate_limit().await;

    let url = format!(
        "{}/reverse?lat={}&lon={}&format=jsonv2&addressdetails=1",
        NOMINATIM_BASE, coords.latitude, coords.longitude
    );

    tracing::debug!("Nominatim reverse: {}", url);

    let response = client
        .get(&url)
        .send()
        .await
        .context("Nominatim reverse request failed")?;

    if response.status() == 404 {
        return Ok(None);
    }

    let result: NominatimResult = response
        .json()
        .await
        .context("Failed to deserialise Nominatim reverse response")?;

    Ok(try_parse_result(result))
}

// ─── Internals ────────────────────────────────────────────────────────────────

fn build_client() -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")
}

/// Enforces the Nominatim 1 req/s rate limit by sleeping if necessary.
async fn rate_limit() {
    let mut last = LAST_REQUEST.lock().await;
    if let Some(t) = *last {
        let elapsed = t.elapsed();
        if elapsed < MIN_INTERVAL {
            tokio::time::sleep(MIN_INTERVAL - elapsed).await;
        }
    }
    *last = Some(Instant::now());
}

fn try_parse_result(r: NominatimResult) -> Option<Place> {
    let lat: f64 = r.lat.parse().ok()?;
    let lon: f64 = r.lon.parse().ok()?;

    let address = r.address.as_ref();

    // Prefer city > town > village > municipality for the short name.
    let name = address
        .and_then(|a| {
            a.city
                .as_deref()
                .or(a.town.as_deref())
                .or(a.village.as_deref())
                .or(a.municipality.as_deref())
        })
        .map(str::to_owned)
        .unwrap_or_else(|| {
            // Fall back to the first segment of the display name.
            r.display_name
                .split(',')
                .next()
                .unwrap_or("Unknown")
                .trim()
                .to_owned()
        });

    let country_code = address
        .and_then(|a| a.country_code.as_deref())
        .unwrap_or("--")
        .to_uppercase();

    Some(Place {
        display_name: r.display_name,
        name,
        state: address.and_then(|a| a.state.clone()),
        country_code,
        coordinates: Coordinates::new(lat, lon),
        osm_id: r.place_id,
    })
}