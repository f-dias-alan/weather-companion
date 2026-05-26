use crate::weather::models::{Coordinates, Place};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const NOMINATIM_BASE: &str = "https://nominatim.openstreetmap.org";
const USER_AGENT: &str = concat!("WeatherCompanion/", env!("CARGO_PKG_VERSION"));
const MIN_INTERVAL: Duration = Duration::from_millis(1100);

static LAST_REQUEST: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));

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

pub async fn search(query: &str, limit: u8) -> Result<Vec<Place>> {
    let client = build_client()?;
    rate_limit().await;
    let encoded = query.split_whitespace().collect::<Vec<_>>().join("+");
    let url = format!("{}/search?q={}&format=jsonv2&addressdetails=1&limit={}", NOMINATIM_BASE, encoded, limit);
    let response = client.get(&url).send().await.context("Nominatim request failed")?;
    anyhow::ensure!(response.status().is_success(), "Nominatim HTTP {}", response.status());
    let results: Vec<NominatimResult> = response.json().await.context("Deserialise failed")?;
    Ok(results.into_iter().filter_map(try_parse_result).collect())
}

pub async fn reverse(coords: Coordinates) -> Result<Option<Place>> {
    let client = build_client()?;
    rate_limit().await;
    let url = format!("{}/reverse?lat={}&lon={}&format=jsonv2&addressdetails=1", NOMINATIM_BASE, coords.latitude, coords.longitude);
    let response = client.get(&url).send().await.context("Reverse failed")?;
    if response.status() == 404 { return Ok(None); }
    let result: NominatimResult = response.json().await.context("Deserialise reverse failed")?;
    Ok(try_parse_result(result))
}

fn build_client() -> Result<Client> {
    Client::builder().user_agent(USER_AGENT).timeout(Duration::from_secs(10)).build().context("HTTP client failed")
}

async fn rate_limit() {
    let mut last = LAST_REQUEST.lock().await;
    if let Some(t) = *last {
        let elapsed = t.elapsed();
        if elapsed < MIN_INTERVAL { tokio::time::sleep(MIN_INTERVAL - elapsed).await; }
    }
    *last = Some(Instant::now());
}

fn try_parse_result(r: NominatimResult) -> Option<Place> {
    let lat: f64 = r.lat.parse().ok()?;
    let lon: f64 = r.lon.parse().ok()?;
    let address = r.address.as_ref();
    let name = address.and_then(|a| a.city.as_deref().or(a.town.as_deref()).or(a.village.as_deref()).or(a.municipality.as_deref()))
        .map(str::to_owned)
        .unwrap_or_else(|| r.display_name.split(',').next().unwrap_or("Unknown").trim().to_owned());
    let country_code = address.and_then(|a| a.country_code.as_deref()).unwrap_or("--").to_uppercase();
    Some(Place { display_name: r.display_name, name, state: address.and_then(|a| a.state.clone()), country_code, coordinates: Coordinates::new(lat, lon), osm_id: r.place_id })
}
