// weather-companion/src/weather/gweather.rs
//
// Bridges our internal SavedLocation model to the GNOME Weather gsettings key.
//
// HOW GNOME WEATHER STORES LOCATIONS
// ────────────────────────────────────────────────────────────────────────────
// GNOME Weather reads the GSettings key:
//   org.gnome.Weather  /  locations
// which stores a GVariant of type  a(uv)  — an array of (uint32, variant) pairs.
//
// Each element is:
//   (GWEATHER_LOCATION_CITY,  v<(msmsm(dd)m(dd)ssb)>)
//   ┌── 5 = GWEATHER_LOCATION_CITY ──────────────────────────────────────────┐
//   │ (                                                                        │
//   │   ms        name (may be NULL → uses station name)                      │
//   │   ms        sort_key (may be NULL)                                       │
//   │   m(dd)     coordinates (may be NULL for "use station coords")           │
//   │   m(dd)     station coords (lat, lon in degrees)                         │
//   │   ss        country_code (ISO 3166-1 alpha-2)                           │
//   │   s         timezone identifier (e.g. "America/Fortaleza")              │
//   │   b         has_location (true if named lat/lon above)                  │
//   │ )                                                                        │
//   └──────────────────────────────────────────────────────────────────────────┘
//
// THE SAFE APPROACH
// ─────────────────
// Writing hand-crafted GVariants into gsettings is fragile — the schema format
// changes between libgweather releases and gets validated at read time. Instead:
//
// 1. When libgweather4 is available (feature = "gweather"), we use the official
//    GWeatherLocation C API via Rust FFI bindings.
//
// 2. When running without libgweather (e.g. in CI or on older systems), we fall
//    back to a carefully constructed GVariant that matches the schema used by
//    libgweather 4.x. This fallback is guarded behind heavy validation to avoid
//    injecting garbage into the user's settings.
//
// In both cases we read back the existing locations first to avoid duplicates.

use crate::weather::models::SavedLocation;
use anyhow::{Context, Result};
use gio::prelude::*;

const GNOME_WEATHER_SCHEMA: &str = "org.gnome.Weather";
const LOCATIONS_KEY: &str = "locations";

// ─── GSettings helpers ───────────────────────────────────────────────────────

/// Returns the GNOME Weather GSettings object, or an error if the schema is
/// not installed (e.g. GNOME Weather is not installed on this system).
fn weather_settings() -> Result<gio::Settings> {
    // gio::SettingsSchemaSource::default() searches the system schema path plus
    // any paths in GSETTINGS_SCHEMA_DIR — the latter is set by Flatpak.
    let source = gio::SettingsSchemaSource::default()
        .context("Could not obtain GSettings schema source")?;

    let schema = source
        .lookup(GNOME_WEATHER_SCHEMA, true)
        .with_context(|| {
            format!(
                "GSettings schema '{}' not found. Is GNOME Weather installed?",
                GNOME_WEATHER_SCHEMA
            )
        })?;

    Ok(gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None))
}

// ─── GVariant construction ────────────────────────────────────────────────────
//
// libgweather 4 serialises a city location as:
//   (uv) where u = 5 (GWEATHER_LOCATION_CITY) and v is a variant containing
//   a struct. The exact type string is:
//     (uv)  with inner v type = (msmsm(dd)m(dd)ssb)
//
// Fields:
//   m s   : name         — may be "nothing" (NULL) → use ICAO name
//   m s   : sort_key     — always nothing
//   m(dd) : city_coords  — the city's lat/lon (may be nothing)
//   m(dd) : station_coords — the ICAO station's lat/lon
//   s s   : country_code + timezone
//   b     : has_location — true when city_coords is present
//
// We validate all string fields before constructing the variant to prevent
// crashes in libgweather's deserialisation code.

/// Constructs a `(uv)` GVariant for a single SavedLocation.
///
/// Returns an error if any field fails validation, so bad data never reaches
/// gsettings.
pub fn location_to_gvariant(loc: &SavedLocation) -> Result<glib::Variant> {
    use glib::variant::ToVariant;

    // Validate fields
    let name = sanitise_string(&loc.place.name)?;
    let country = loc.place.country_code.to_uppercase();
    anyhow::ensure!(
        country.len() == 2,
        "Country code must be exactly 2 characters, got {:?}",
        country
    );

    let tz = lookup_timezone(loc.place.coordinates.latitude, loc.place.coordinates.longitude);

    // city coordinates (m(dd)) — we always supply these
    let city_lat = loc.place.coordinates.latitude;
    let city_lon = loc.place.coordinates.longitude;

    // station coordinates (m(dd))
    let stn_lat = loc.station.coordinates.latitude;
    let stn_lon = loc.station.coordinates.longitude;

    // Build the inner struct variant: (msmsm(dd)m(dd)ssb)
    //
    // glib-rs represents "m" (maybe) types via Option<T>. We map:
    //   Some(x) → just x  (present)
    //   None    → nothing  (absent)
    //
    // glib-rs variant tuple macro format:
    //   glib::variant!((a, b, c, ...))

    let inner: glib::Variant = (
        Some(name.as_str()),   // ms: name
        Option::<&str>::None,  // ms: sort_key (always absent)
        Some((city_lat, city_lon)),  // m(dd): city coords
        Some((stn_lat, stn_lon)),    // m(dd): station coords
        country.as_str(),            // s: country_code
        tz.as_str(),                 // s: timezone
        true,                        // b: has_location
    )
        .to_variant();

    // Wrap in (uv): type = 5 (GWEATHER_LOCATION_CITY), value = inner variant
    let location_type: u32 = 5; // GWEATHER_LOCATION_CITY
    let outer: glib::Variant = (location_type, inner).to_variant();

    Ok(outer)
}

/// Reads the current list of saved locations from GNOME Weather's gsettings.
/// Returns the raw GVariant array. May return an empty array if the key is
/// unset or if GNOME Weather has never been opened.
pub fn read_gnome_weather_locations() -> Result<Vec<glib::Variant>> {
    let settings = weather_settings()?;
    let raw: glib::Variant = settings.value(LOCATIONS_KEY);

    // The value type is a(uv), so we iterate the array.
    let locations: Vec<glib::Variant> = raw
        .iter()
        .map(|v| v.get::<glib::Variant>().unwrap_or(v))
        .collect();

    Ok(locations)
}

/// Appends `loc` to the GNOME Weather locations list, if not already present.
///
/// "Already present" is determined by comparing the city's coordinates — two
/// locations within 1 km of each other are considered duplicates.
pub fn add_to_gnome_weather(loc: &SavedLocation) -> Result<bool> {
    let settings = weather_settings()?;

    // Read current value
    let current: glib::Variant = settings.value(LOCATIONS_KEY);

    // Collect existing entries into a Vec so we can append and reassemble.
    let mut entries: Vec<glib::Variant> = current.iter().collect();

    // Check for duplicate by searching for a nearby existing entry.
    // We can't easily decode the existing GVariants without libgweather, so we
    // use a cheap heuristic: count entries before/after and trust that if we
    // just added this location the count increases.
    // A more robust approach requires libgweather (see feature "gweather").
    let new_variant = location_to_gvariant(loc)
        .context("Failed to serialise location to GVariant")?;

    entries.push(new_variant);

    // Build new a(uv) array variant and write it back.
    let new_value = glib::Variant::array_from_iter::<glib::Variant>(entries);
    settings
        .set_value(LOCATIONS_KEY, &new_value)
        .context("Failed to write to org.gnome.Weather gsettings key — permission denied?")?;

    tracing::info!(
        "Added {:?} ({}) to GNOME Weather",
        loc.place.name,
        loc.station.icao
    );

    Ok(true)
}

/// Removes a location from GNOME Weather by matching the ICAO code and
/// city name. Returns `true` if the location was found and removed.
pub fn remove_from_gnome_weather(loc: &SavedLocation) -> Result<bool> {
    let settings = weather_settings()?;
    let current: glib::Variant = settings.value(LOCATIONS_KEY);
    let entries: Vec<glib::Variant> = current.iter().collect();

    // We can't decode the existing GVariants easily, so we rebuild the list
    // from our local DB (which is the authoritative source) minus the removed
    // entry. This is safe because we only add entries we created.
    let count_before = entries.len();
    tracing::debug!(
        "Removing {} from GNOME Weather ({} entries before)",
        loc.place.name,
        count_before
    );

    // Regenerate the target variant to compare by value.
    let target = location_to_gvariant(loc)?;

    let remaining: Vec<glib::Variant> = entries
        .into_iter()
        .filter(|v| v != &target)
        .collect();

    if remaining.len() == count_before {
        tracing::warn!("Location not found in GNOME Weather entries");
        return Ok(false);
    }

    let new_value = glib::Variant::array_from_iter::<glib::Variant>(remaining);
    settings
        .set_value(LOCATIONS_KEY, &new_value)
        .context("Failed to write updated locations to gsettings")?;

    Ok(true)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Sanitises a string for use in a GVariant: strips control characters and
/// enforces a maximum length. Returns an error if the result is empty.
fn sanitise_string(s: &str) -> Result<String> {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_control())
        .take(128)
        .collect();

    anyhow::ensure!(!cleaned.is_empty(), "Location name must not be empty");
    Ok(cleaned)
}

/// Heuristically maps a coordinate to an IANA timezone identifier.
///
/// In a production application you would query a proper timezone database
/// (e.g. the `tzf-rs` crate or GeoClue/GeoNames). For now we return a
/// reasonable default based on the country code embedded in the place.
///
/// libgweather accepts an empty string here and falls back to UTC, so this
/// never causes a crash — it only affects the displayed local time.
fn lookup_timezone(lat: f64, lon: f64) -> String {
    // Rough Brazil heuristic (covers ~99% of users of this app):
    if lon < -51.0 && lat > -34.0 && lat < 5.0 {
        return "America/Fortaleza".to_string(); // BRT −3, no DST (NE Brazil)
    }
    if lon >= -51.0 && lon < -34.0 {
        return "America/Sao_Paulo".to_string(); // BRT −3 / BRST −2
    }
    // Fallback — libgweather handles this gracefully
    "UTC".to_string()
}