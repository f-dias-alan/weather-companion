# Weather Companion

**Add small cities and unsupported locations to GNOME Weather — safely.**

Weather Companion is a GTK4/libadwaita application that lets you search for any
city or town worldwide (including tiny municipalities not in GNOME Weather's
built-in database), finds the nearest ICAO weather station, and registers the
location with GNOME Weather correctly using GSettings.

---

## Why does this exist?

GNOME Weather ships with the libgweather location database which covers major
cities. Smaller cities — like **Pau dos Ferros, RN** (pop. ~40,000), **Francisco
Dantas, RN**, and thousands of similar municipalities worldwide — are absent.
Attempting to add them via raw `gsettings set` commands is fragile and causes
GNOME Weather to crash or show blank data because the GVariant format must
include a valid ICAO weather station code.

Weather Companion automates the whole process:

```
Your query
   ↓
OpenStreetMap / Nominatim  →  Latitude + Longitude
   ↓
OpenFlights airport DB     →  Nearest ICAO station (Haversine)
   ↓
GSettings (org.gnome.Weather.locations)
   ↓
GNOME Weather shows real weather data ✓
```

---

## Features

- **Smart city search** — powered by OpenStreetMap Nominatim, finds any place
  on Earth.
- **Automatic ICAO detection** — uses the Haversine formula over the full
  OpenFlights airport database (~14,000 stations) to find the closest station.
- **Safe GSettings integration** — constructs valid `a(uv)` GVariant values
  that libgweather can deserialise without crashing.
- **Modern UI** — GTK4 + libadwaita, follows the GNOME Human Interface Guidelines.
- **GeoClue support** — "Use my location" button fetches GPS/network coordinates
  automatically (CITY accuracy level — no exact address required).
- **Offline-capable local DB** — all your saved cities are stored in SQLite so
  you can re-sync after a GNOME Weather reset.
- **Flatpak ready** — ships with a complete manifest for Flathub submission.

---

## Screenshots


---

## Building

### Prerequisites

**Native build:**
```bash
# Fedora / RHEL
sudo dnf install rust cargo gtk4-devel libadwaita-devel libgweather4-devel \
                 glib2-devel sqlite-devel

# Debian / Ubuntu
sudo apt install rustup libgtk-4-dev libadwaita-1-dev libgweather-4-dev \
                 libglib2.0-dev libsqlite3-dev
```

**Flatpak build:**
```bash
flatpak install flathub org.gnome.Platform//47 org.gnome.Sdk//47
flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable//24.08
```

### Native build

```bash
git clone https://github.com/example/weather-companion
cd weather-companion

# Download the airport database (one-time)
curl -L "https://raw.githubusercontent.com/jpatokal/openflights/master/data/airports.dat" \
     -o data/airports.csv

cargo build --release
./target/release/weather-companion
```

### Flatpak build

```bash
cd flatpak
flatpak-builder --user --install --force-clean build-dir \
    org.gnome.WeatherCompanion.yml
flatpak run org.gnome.WeatherCompanion
```

### Enable logging

```bash
WEATHER_COMPANION_LOG=debug ./target/release/weather-companion
```

---

## Architecture

```
src/
├── main.rs            Entry point, logging init
├── app.rs             GTK Application object
│
├── weather/
│   ├── models.rs      Core types (Place, AirportStation, SavedLocation)
│   ├── icao.rs        Airport DB loading + Haversine nearest-station
│   ├── gweather.rs    GSettings integration (read/write org.gnome.Weather)
│   └── geoclue.rs     GeoClue2 D-Bus client for automatic location
│
├── services/
│   ├── nominatim.rs   OpenStreetMap geocoding API client
│   └── sync.rs        Orchestrates search → ICAO → DB → GSettings
│
├── storage/
│   └── database.rs    SQLite persistence (rusqlite)
│
├── ui/
│   ├── window.rs      MainWindow + AppState
│   ├── search.rs      Search page with debounced autocomplete
│   └── locations.rs   Saved cities management page
│
└── utils/
    └── geo.rs         Geographic helper functions
```

### Key design decisions

**Why ICAO?**
libgweather identifies weather stations by ICAO code internally. Without a
valid ICAO code, GNOME Weather cannot fetch METAR data and either silently
fails or crashes with a GLib assertion error.

**Why Nominatim?**
OpenStreetMap's Nominatim API is free, covers the entire world including tiny
villages, and returns structured address components. No API key is required.
We respect the 1 req/s rate limit via an async rate limiter.

**Why SQLite?**
We need a local copy of what we've added to GSettings so we can remove entries
cleanly and re-sync after a GNOME Weather reset. A flat JSON file would work
but SQLite handles concurrent access better and is trivially indexed.

**Why not use libgweather Rust bindings directly?**
The `libgweather4` crate is optional (feature flag `gweather`). When available
it provides type-safe access to the C API. When not available (older systems,
CI) we fall back to constructing the GVariant manually — with thorough
validation to prevent injecting bad data.

---

## Roadmap

### Phase 1 — Stable release
- [x] Nominatim search
- [x] ICAO nearest-station
- [x] GSettings sync
- [x] GTK4 UI
- [x] SQLite persistence
- [x] Flatpak manifest

### Phase 2 — Quality of life
- [ ] Allow the user to manually select among the 3 nearest stations
- [ ] Station distance warning dialog (> 150 km)
- [ ] Import locations from a CSV file
- [ ] Export / backup saved cities

### Phase 3 — Advanced
- [ ] Offline METAR preview (show raw weather data from selected station)
- [ ] Background sync daemon (systemd user service)
- [ ] GNOME Shell quick settings tile
- [ ] Multi-provider support (Open-Meteo, OpenWeatherMap)

---

## Contributing

Pull requests are welcome. Please:
1. Run `cargo fmt` and `cargo clippy` before submitting.
2. Add tests for any new geographic or parsing logic.
3. Follow the GNOME Human Interface Guidelines for UI changes.

## License

GPL-3.0-or-later — the same licence as GNOME Weather itself.