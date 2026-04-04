use chrono::{DateTime, Local, TimeZone};
use rusqlite::{params, Connection, Result, Row};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct ClipEntry {
    pub id: i64,
    pub content: String,
    #[serde(skip)]
    pub hash: String,
    pub timestamp: DateTime<Local>,
    pub app_name: Option<String>,
    pub pinned: bool,
}

pub struct Store {
    conn: Connection,
}

fn row_to_entry(row: &Row) -> Result<ClipEntry> {
    Ok(ClipEntry {
        id: row.get(0)?,
        content: row.get(1)?,
        hash: row.get(2)?,
        timestamp: Local.timestamp_opt(row.get::<_, i64>(3)?, 0).unwrap(),
        app_name: row.get(4)?,
        pinned: row.get::<_, i64>(5).unwrap_or(0) != 0,
    })
}

fn query_entries(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> Result<Vec<ClipEntry>> {
    conn.prepare(sql)?
        .query_map(params, row_to_entry)?
        .collect()
}

fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clips (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                content   TEXT NOT NULL,
                hash      TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                app_name  TEXT,
                pinned    INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_clips_hash ON clips(hash);
            CREATE INDEX IF NOT EXISTS idx_clips_ts ON clips(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_content ON clips(content);",
        )?;
        // Migration: add pinned column to existing databases
        let has_pinned: bool = conn
            .prepare("PRAGMA table_info(clips)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .any(|name| name.as_deref() == Ok("pinned"));
        if !has_pinned {
            conn.execute_batch("ALTER TABLE clips ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;")?;
        }
        Ok(Store { conn })
    }

    pub fn insert(&self, content: &str, app_name: Option<&str>) -> Result<i64> {
        let hash = content_hash(content);
        let now = Local::now().timestamp();

        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM clips WHERE hash = ?1 LIMIT 1",
                params![hash],
                |row| row.get(0),
            )
            .ok();

        match existing {
            Some(id) => {
                self.conn.execute(
                    "UPDATE clips SET timestamp = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
                Ok(id)
            }
            None => {
                self.conn.execute(
                    "INSERT INTO clips (content, hash, timestamp, app_name) VALUES (?1, ?2, ?3, ?4)",
                    params![content, hash, now, app_name],
                )?;
                Ok(self.conn.last_insert_rowid())
            }
        }
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<ClipEntry>> {
        query_entries(
            &self.conn,
            "SELECT id, content, hash, timestamp, app_name, pinned FROM clips ORDER BY pinned DESC, timestamp DESC LIMIT ?1",
            &[&(limit as i64)],
        )
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<ClipEntry>> {
        let pattern = format!("%{query}%");
        query_entries(
            &self.conn,
            "SELECT id, content, hash, timestamp, app_name, pinned FROM clips WHERE content LIKE ?1 ORDER BY pinned DESC, timestamp DESC LIMIT ?2",
            &[&pattern as &dyn rusqlite::types::ToSql, &(limit as i64)],
        )
    }

    pub fn get(&self, id: i64) -> Result<Option<ClipEntry>> {
        self.conn
            .query_row(
                "SELECT id, content, hash, timestamp, app_name, pinned FROM clips WHERE id = ?1",
                params![id],
                row_to_entry,
            )
            .optional()
    }

    pub fn toggle_pin(&self, id: i64) -> Result<bool> {
        self.conn.execute(
            "UPDATE clips SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END WHERE id = ?1",
            params![id],
        )?;
        // Return the new pinned state
        self.conn
            .query_row(
                "SELECT pinned FROM clips WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(|v| v != 0)
    }

    pub fn delete(&self, id: i64) -> Result<bool> {
        self.conn
            .execute("DELETE FROM clips WHERE id = ?1", params![id])
            .map(|n| n > 0)
    }

    pub fn clear(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?;
        self.conn.execute("DELETE FROM clips", [])?;
        Ok(count as usize)
    }

    pub fn all(&self) -> Result<Vec<ClipEntry>> {
        self.recent(10000)
    }

    /// Count total entries in the store.
    pub fn count(&self) -> Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get::<_, i64>(0))
            .map(|n| n as usize)
    }

    /// Delete the oldest unpinned entries to keep at most `max_entries` in the store.
    /// Pinned entries are never pruned. Returns the number of entries deleted.
    pub fn prune(&self, max_entries: usize) -> Result<usize> {
        let count = self.count()?;
        if count <= max_entries {
            return Ok(0);
        }
        let excess = count - max_entries;
        let deleted = self.conn.execute(
            "DELETE FROM clips WHERE id IN (SELECT id FROM clips WHERE pinned = 0 ORDER BY timestamp ASC LIMIT ?1)",
            params![excess as i64],
        )?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_store() -> Store {
        Store::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn serialize_clip_entry_excludes_hash() {
        let entry = ClipEntry {
            id: 1,
            content: "hello world".to_string(),
            hash: "abc123".to_string(),
            timestamp: Local.timestamp_opt(1700000000, 0).unwrap(),
            app_name: Some("Terminal".to_string()),
            pinned: false,
        };
        let json: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["content"], "hello world");
        assert_eq!(json["app_name"], "Terminal");
        assert!(
            json.get("hash").is_none(),
            "hash should be excluded from JSON"
        );
        assert!(json["timestamp"].as_str().unwrap().contains("2023-11-14"));
    }

    #[test]
    fn serialize_clip_entry_null_app_name() {
        let entry = ClipEntry {
            id: 2,
            content: "test".to_string(),
            hash: "def456".to_string(),
            timestamp: Local.timestamp_opt(1700000000, 0).unwrap(),
            app_name: None,
            pinned: false,
        };
        let json: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert!(json["app_name"].is_null());
    }

    #[test]
    fn insert_and_retrieve() {
        let store = temp_store();
        let id = store.insert("clipboard content", Some("Safari")).unwrap();
        let entry = store.get(id).unwrap().unwrap();
        assert_eq!(entry.content, "clipboard content");
        assert_eq!(entry.app_name.as_deref(), Some("Safari"));
    }

    #[test]
    fn insert_deduplicates_by_hash() {
        let store = temp_store();
        let id1 = store.insert("same content", None).unwrap();
        let id2 = store.insert("same content", None).unwrap();
        assert_eq!(id1, id2, "duplicate content should return same ID");
        assert_eq!(store.recent(100).unwrap().len(), 1);
    }

    #[test]
    fn search_returns_matching_entries() {
        let store = temp_store();
        store.insert("rust programming", None).unwrap();
        store.insert("go programming", None).unwrap();
        store.insert("grocery list", None).unwrap();

        let results = store.search("programming", 10).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn recent_respects_limit() {
        let store = temp_store();
        for i in 0..10 {
            store.insert(&format!("entry {i}"), None).unwrap();
        }
        assert_eq!(store.recent(3).unwrap().len(), 3);
    }

    #[test]
    fn delete_removes_entry() {
        let store = temp_store();
        let id = store.insert("to delete", None).unwrap();
        assert!(store.delete(id).unwrap());
        assert!(store.get(id).unwrap().is_none());
    }

    #[test]
    fn count_returns_entry_count() {
        let store = temp_store();
        assert_eq!(store.count().unwrap(), 0);
        store.insert("one", None).unwrap();
        store.insert("two", None).unwrap();
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn prune_removes_oldest_entries() {
        let store = temp_store();
        for i in 0..5 {
            store.insert(&format!("entry {i}"), None).unwrap();
            // Force distinct timestamps by bumping the timestamp column directly
            store
                .conn
                .execute(
                    "UPDATE clips SET timestamp = ?1 WHERE content = ?2",
                    params![1000 + i, format!("entry {i}")],
                )
                .unwrap();
        }
        assert_eq!(store.count().unwrap(), 5);

        let pruned = store.prune(3).unwrap();
        assert_eq!(pruned, 2);
        assert_eq!(store.count().unwrap(), 3);

        // Remaining entries should be the newest ones (entries 2, 3, 4)
        let remaining = store.recent(10).unwrap();
        let contents: Vec<&str> = remaining.iter().map(|e| e.content.as_str()).collect();
        assert!(contents.contains(&"entry 2"));
        assert!(contents.contains(&"entry 3"));
        assert!(contents.contains(&"entry 4"));
        assert!(!contents.contains(&"entry 0"));
        assert!(!contents.contains(&"entry 1"));
    }

    #[test]
    fn prune_noop_when_under_limit() {
        let store = temp_store();
        store.insert("one", None).unwrap();
        store.insert("two", None).unwrap();
        let pruned = store.prune(10).unwrap();
        assert_eq!(pruned, 0);
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn prune_exact_limit_is_noop() {
        let store = temp_store();
        store.insert("one", None).unwrap();
        store.insert("two", None).unwrap();
        let pruned = store.prune(2).unwrap();
        assert_eq!(pruned, 0);
    }

    #[test]
    fn clear_removes_all() {
        let store = temp_store();
        store.insert("one", None).unwrap();
        store.insert("two", None).unwrap();
        let count = store.clear().unwrap();
        assert_eq!(count, 2);
        assert!(store.recent(100).unwrap().is_empty());
    }

    #[test]
    fn toggle_pin_pins_and_unpins() {
        let store = temp_store();
        let id = store.insert("pin me", None).unwrap();

        assert!(!store.get(id).unwrap().unwrap().pinned);

        let pinned = store.toggle_pin(id).unwrap();
        assert!(pinned);
        assert!(store.get(id).unwrap().unwrap().pinned);

        let pinned = store.toggle_pin(id).unwrap();
        assert!(!pinned);
        assert!(!store.get(id).unwrap().unwrap().pinned);
    }

    #[test]
    fn pinned_entries_appear_first_in_recent() {
        let store = temp_store();
        store.insert("old", None).unwrap();
        store
            .conn
            .execute(
                "UPDATE clips SET timestamp = 1000 WHERE content = 'old'",
                [],
            )
            .unwrap();

        let new_id = store.insert("new", None).unwrap();
        store
            .conn
            .execute(
                "UPDATE clips SET timestamp = 2000 WHERE content = 'new'",
                [],
            )
            .unwrap();

        let old_id = store.recent(10).unwrap().last().unwrap().id;
        store.toggle_pin(old_id).unwrap();

        let entries = store.recent(10).unwrap();
        assert_eq!(
            entries[0].content, "old",
            "pinned entry should appear first"
        );
        assert!(entries[0].pinned);
        assert_eq!(entries[1].content, "new");
        assert!(!entries[1].pinned);
    }

    #[test]
    fn prune_skips_pinned_entries() {
        let store = temp_store();
        for i in 0..5 {
            store.insert(&format!("entry {i}"), None).unwrap();
            store
                .conn
                .execute(
                    "UPDATE clips SET timestamp = ?1 WHERE content = ?2",
                    params![1000 + i, format!("entry {i}")],
                )
                .unwrap();
        }

        // Pin the oldest entry (entry 0)
        let oldest = store.recent(10).unwrap().last().unwrap().id;
        store.toggle_pin(oldest).unwrap();

        // Prune to 3 — should skip pinned entry 0 and delete entries 1 and 2
        let pruned = store.prune(3).unwrap();
        assert_eq!(pruned, 2);
        assert_eq!(store.count().unwrap(), 3);

        let remaining = store.recent(10).unwrap();
        let contents: Vec<&str> = remaining.iter().map(|e| e.content.as_str()).collect();
        assert!(
            contents.contains(&"entry 0"),
            "pinned entry should survive prune"
        );
        assert!(contents.contains(&"entry 3"));
        assert!(contents.contains(&"entry 4"));
    }

    #[test]
    fn serialize_clip_entry_includes_pinned() {
        let entry = ClipEntry {
            id: 1,
            content: "test".to_string(),
            hash: "abc".to_string(),
            timestamp: Local.timestamp_opt(1700000000, 0).unwrap(),
            app_name: None,
            pinned: true,
        };
        let json: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["pinned"], true);
    }

    #[test]
    fn new_entries_are_not_pinned() {
        let store = temp_store();
        let id = store.insert("fresh", None).unwrap();
        assert!(!store.get(id).unwrap().unwrap().pinned);
    }
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>>;
}

impl<T> OptionalExt<T> for Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
