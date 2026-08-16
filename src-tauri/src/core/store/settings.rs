use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension};

use crate::core::category::{DEFAULT_DISTRACTION_THRESHOLD_SECONDS, DISTRACTION_THRESHOLD_KEY};
use crate::core::store::resolve;
use crate::core::store::sqlite::SharedConnection;

/// An hour. Beyond this the setting stops being "ignore the flicker" and starts silently
/// swallowing real activity, which is never what someone means by it.
const MAX_DISTRACTION_THRESHOLD_SECONDS: i64 = 3600;

/// Key/value app preferences. Only the distraction threshold lives here so far.
pub struct SettingsStore {
    conn: SharedConnection,
}

impl SettingsStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    /// Seconds below which a *rule-less* activity is filed as a distraction. `0` disables.
    pub fn distraction_threshold_seconds(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![DISTRACTION_THRESHOLD_KEY],
                |row| row.get(0),
            )
            .optional()?;
        // A missing or unparsable row falls back rather than failing: the SQL side of the
        // resolution does the same, and the two must not disagree about what is in effect.
        Ok(raw
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(DEFAULT_DISTRACTION_THRESHOLD_SECONDS))
    }

    /// Stores the threshold and replays it over history, so past activities re-label to
    /// match the new cutoff instead of preserving whatever the old one decided.
    pub fn set_distraction_threshold_seconds(&self, seconds: i64) -> Result<()> {
        if seconds < 0 {
            bail!("threshold must not be negative");
        }
        if seconds > MAX_DISTRACTION_THRESHOLD_SECONDS {
            bail!("threshold must be at most {MAX_DISTRACTION_THRESHOLD_SECONDS} seconds");
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![DISTRACTION_THRESHOLD_KEY, seconds.to_string()],
        )?;
        resolve::resolve_all(&tx)?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::queries::{HistoryReader, SessionFilter};
    use crate::core::store::sqlite::open_connection;
    use crate::core::store::sqlite::SharedConnection;
    use rusqlite::params;

    fn temp_store() -> (SettingsStore, SharedConnection, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "makora-settings-store-test-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                ^ (std::process::id() as u128) << 64
        ));
        let conn = open_connection(&path).expect("opening test database");
        (SettingsStore::new(conn.clone()), conn, path)
    }

    fn insert_session_lasting(conn: &SharedConnection, start: &str, seconds: i64) -> i64 {
        let start_at: chrono::DateTime<chrono::Utc> = start.parse().unwrap();
        let end = (start_at + chrono::Duration::seconds(seconds)).to_rfc3339();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, category_key, category_key_kind, start_time, end_time)
             VALUES ('blip.exe', 'blip.exe', 'title', 'blip.exe', 'exe', ?1, ?2)",
            params![start, end],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        resolve::resolve_session(&conn, id).unwrap();
        id
    }

    #[test]
    fn the_threshold_defaults_to_ten_seconds_and_round_trips() {
        let (settings, _conn, path) = temp_store();
        assert_eq!(settings.distraction_threshold_seconds().unwrap(), 10);

        settings.set_distraction_threshold_seconds(45).unwrap();
        assert_eq!(settings.distraction_threshold_seconds().unwrap(), 45);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_out_of_range_threshold_is_refused_rather_than_silently_coerced() {
        let (settings, _conn, path) = temp_store();
        assert!(settings.set_distraction_threshold_seconds(-1).is_err());
        assert!(settings.set_distraction_threshold_seconds(999_999).is_err());
        assert_eq!(
            settings.distraction_threshold_seconds().unwrap(),
            10,
            "a rejected write must leave the setting where it was"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn moving_the_threshold_relabels_history_in_both_directions() {
        let (settings, conn, path) = temp_store();
        let reader = HistoryReader::new(conn.clone());
        let filter = SessionFilter::default();
        let distraction: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT id FROM categories WHERE system_key = 'distraction'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let thirty_seconds = insert_session_lasting(&conn, "2024-06-15T00:00:00Z", 30);
        let category_of = || {
            reader
                .list_sessions(10, 0, &filter)
                .unwrap()
                .iter()
                .find(|r| r.id == thirty_seconds)
                .unwrap()
                .category_id
        };
        assert_eq!(category_of(), None);

        settings.set_distraction_threshold_seconds(60).unwrap();
        assert_eq!(category_of(), Some(distraction));

        settings.set_distraction_threshold_seconds(10).unwrap();
        assert_eq!(
            category_of(),
            None,
            "raising the bar and lowering it again has to be reversible"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_threshold_of_zero_turns_the_feature_off() {
        let (settings, conn, path) = temp_store();
        let reader = HistoryReader::new(conn.clone());
        let filter = SessionFilter::default();

        settings.set_distraction_threshold_seconds(0).unwrap();
        let instant = insert_session_lasting(&conn, "2024-06-15T00:00:00Z", 0);

        assert_eq!(
            reader
                .list_sessions(10, 0, &filter)
                .unwrap()
                .iter()
                .find(|r| r.id == instant)
                .unwrap()
                .category_id,
            None
        );

        let _ = std::fs::remove_file(&path);
    }
}
