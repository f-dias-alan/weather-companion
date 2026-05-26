// weather-companion/src/storage/database.rs
//
// SQLite-backed local storage for saved locations.
//
// We store each SavedLocation as a row containing:
//   - Primary key (integer)
//   - JSON blob of the full SavedLocation struct
//   - A few indexed columns for quick queries (name, icao, created_at)
//
// The JSON blob approach means schema migrations are trivial: add a field to
// the struct, re-serialise. Old rows without the field deserialise via serde's
// #[serde(default)] mechanism.

use crate::weather::models::SavedLocation;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::rc::Rc;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens (or creates) the SQLite database in the user's data directory.
    ///
    /// Under Flatpak, the data directory is:
    ///   ~/.var/app/org.gnome.WeatherCompanion/data/weather-companion/
    /// On a native install it falls back to:
    ///   ~/.local/share/weather-companion/
    pub fn open() -> Result<Self> {
        let path = db_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create data directory")?;
        }

        tracing::debug!("Opening database at {:?}", path);

        let conn = Connection::open(&path)
            .with_context(|| format!("Failed to open SQLite at {:?}", path))?;

        // Enable WAL mode for better concurrent read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let db = Self { conn };
        db.migrate()?;

        Ok(db)
    }

    /// Applies all pending schema migrations in order.
    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS locations (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT    NOT NULL,
                icao        TEXT    NOT NULL,
                country     TEXT    NOT NULL,
                created_at  INTEGER NOT NULL,
                data        TEXT    NOT NULL   -- full JSON blob
            );
            CREATE INDEX IF NOT EXISTS idx_locations_name ON locations (name);
            CREATE INDEX IF NOT EXISTS idx_locations_icao ON locations (icao);
            "#,
        )
        .context("Failed to run database migrations")
    }

    /// Inserts a new location and returns its assigned row ID.
    pub fn insert_location(&self, loc: &SavedLocation) -> Result<i64> {
        let json = serde_json::to_string(loc)
            .context("Failed to serialise SavedLocation")?;

        self.conn.execute(
            "INSERT INTO locations (name, icao, country, created_at, data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                loc.place.name,
                loc.station.icao,
                loc.place.country_code,
                loc.created_at,
                json,
            ],
        )
        .context("Failed to insert location")?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Returns all saved locations ordered by creation time (oldest first).
    pub fn list_locations(&self) -> Result<Vec<SavedLocation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, data FROM locations ORDER BY created_at ASC")
            .context("Failed to prepare list query")?;

        let rows = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let json: String = row.get(1)?;
                Ok((id, json))
            })
            .context("Failed to execute list query")?;

        let mut locations = Vec::new();
        for row in rows {
            let (id, json) = row.context("Failed to read row")?;
            let mut loc: SavedLocation = serde_json::from_str(&json)
                .with_context(|| format!("Failed to deserialise location (id={})", id))?;
            loc.id = Some(id);
            locations.push(loc);
        }

        Ok(locations)
    }

    /// Deletes a location by its database ID.
    pub fn delete_location(&self, id: i64) -> Result<()> {
        let rows_changed = self
            .conn
            .execute("DELETE FROM locations WHERE id = ?1", params![id])
            .context("Failed to delete location")?;

        if rows_changed == 0 {
            anyhow::bail!("No location found with id {}", id);
        }

        Ok(())
    }

    /// Returns the number of saved locations.
    pub fn count(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM locations", [], |r| r.get(0))
            .context("Failed to count locations")?;
        Ok(n as usize)
    }
}

fn db_path() -> Result<PathBuf> {
    // Prefer XDG_DATA_HOME (respects Flatpak sandbox automatically).
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            home.join(".local").join("share")
        });

    Ok(base.join("weather-companion").join("locations.db"))
}