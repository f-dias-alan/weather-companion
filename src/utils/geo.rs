// weather-companion/src/utils/geo.rs
//
// Utility functions for geographic calculations beyond basic Haversine.

use crate::weather::models::Coordinates;

/// Returns a bounding box (south, west, north, east) that encloses all points
/// within `radius_km` of `centre`. Useful for pre-filtering station candidates
/// before the full Haversine search.
pub fn bounding_box(centre: Coordinates, radius_km: f64) -> (f64, f64, f64, f64) {
    // 1 degree of latitude ≈ 111.32 km
    let lat_delta = radius_km / 111.32;
    // 1 degree of longitude varies with latitude
    let lon_delta = radius_km / (111.32 * centre.latitude.to_radians().cos());

    (
        centre.latitude - lat_delta,   // south
        centre.longitude - lon_delta,  // west
        centre.latitude + lat_delta,   // north
        centre.longitude + lon_delta,  // east
    )
}

/// Formats a coordinate pair for display, e.g. "5°47′S, 38°21′W".
pub fn format_dms(coords: Coordinates) -> String {
    fn to_dms(deg: f64, pos: char, neg: char) -> String {
        let d = deg.abs().floor() as u32;
        let m = ((deg.abs() - d as f64) * 60.0).floor() as u32;
        let hemi = if deg >= 0.0 { pos } else { neg };
        format!("{}°{}′{}", d, m, hemi)
    }

    format!(
        "{}, {}",
        to_dms(coords.latitude, 'N', 'S'),
        to_dms(coords.longitude, 'E', 'W')
    )
}