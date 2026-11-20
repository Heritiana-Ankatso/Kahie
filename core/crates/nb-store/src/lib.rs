//! Storage v0 (P0-12, data-model §9): one SQLite database (WAL mode) per document.
//!
//! The append-only `updates` table is the source of truth: an edit is durable as soon as
//! its row is committed. `snapshots` are an optimisation for fast loading; [`Store::compact`]
//! writes a snapshot and drops the updates it covers in one transaction.
//!
//! The store is CRDT-agnostic: it keeps opaque byte strings. `nb-model` produces them.

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;

/// Schema version written to `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 1;

/// Migrations, applied in order; entry `i` upgrades from version `i` to `i + 1`.
const MIGRATIONS: &[&str] = &[
    // 0 → 1
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value BLOB NOT NULL);
     CREATE TABLE updates (seq INTEGER PRIMARY KEY AUTOINCREMENT, bytes BLOB NOT NULL);
     CREATE TABLE snapshots (seq INTEGER PRIMARY KEY, bytes BLOB NOT NULL);
     CREATE TABLE assets (hash TEXT PRIMARY KEY, mime TEXT NOT NULL, bytes BLOB NOT NULL,
                          refcount INTEGER NOT NULL DEFAULT 0);",
];

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    /// The database was written by a newer app version.
    TooNew {
        found: i64,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "storage: {e}"),
            StoreError::TooNew { found } => {
                write!(
                    f,
                    "storage: schema version {found} is newer than supported {SCHEMA_VERSION}"
                )
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// What to feed the CRDT on open: the latest snapshot, then every later update in order.
#[derive(Debug, Default, PartialEq)]
pub struct Loaded {
    pub snapshot: Option<Vec<u8>>,
    pub updates: Vec<Vec<u8>>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open or create the database at `path`, applying migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// In-memory database (tests).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        // WAL + synchronous=FULL: a committed transaction survives process kill and power loss.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(StoreError::TooNew { found: version });
        }
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        }
        tx.commit()?;
        Ok(Self { conn })
    }

    /// Append one CRDT update. Returns its sequence number once it is durable.
    pub fn append_update(&self, bytes: &[u8]) -> Result<i64> {
        self.conn
            .execute("INSERT INTO updates (bytes) VALUES (?1)", params![bytes])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Latest snapshot and the updates written after it.
    pub fn load(&self) -> Result<Loaded> {
        let snap: Option<(i64, Vec<u8>)> = self
            .conn
            .query_row("SELECT seq, bytes FROM snapshots ORDER BY seq DESC LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        let after = snap.as_ref().map_or(0, |(s, _)| *s);
        let mut stmt = self
            .conn
            .prepare("SELECT bytes FROM updates WHERE seq > ?1 ORDER BY seq")?;
        let updates = stmt
            .query_map(params![after], |r| r.get::<_, Vec<u8>>(0))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(Loaded {
            snapshot: snap.map(|(_, b)| b),
            updates,
        })
    }

    /// Store `snapshot` as covering every update up to now, and delete those updates.
    /// The caller must pass a snapshot that includes all updates appended so far.
    pub fn compact(&mut self, snapshot: &[u8]) -> Result<()> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let last: i64 = tx.query_row("SELECT COALESCE(MAX(seq), 0) FROM updates", [], |r| r.get(0))?;
        let prev: i64 = tx.query_row("SELECT COALESCE(MAX(seq), 0) FROM snapshots", [], |r| r.get(0))?;
        let seq = last.max(prev);
        tx.execute(
            "INSERT OR REPLACE INTO snapshots (seq, bytes) VALUES (?1, ?2)",
            params![seq, snapshot],
        )?;
        tx.execute("DELETE FROM snapshots WHERE seq < ?1", params![seq])?;
        tx.execute("DELETE FROM updates WHERE seq <= ?1", params![seq])?;
        tx.commit()?;
        Ok(())
    }

    /// Updates with a sequence number greater than `after`, in order (relay catch-up).
    pub fn updates_after(&self, after: i64) -> Result<Vec<(i64, Vec<u8>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT seq, bytes FROM updates WHERE seq > ?1 ORDER BY seq")?;
        let rows = stmt
            .query_map(params![after], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// Number of updates not yet folded into a snapshot (compaction heuristic).
    pub fn pending_updates(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM updates", [], |r| r.get(0))?)
    }

    pub fn set_meta(&self, key: &str, value: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    /// Store an asset under its content hash (caller computes the hash). Idempotent.
    pub fn put_asset(&self, hash: &str, mime: &str, bytes: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO assets (hash, mime, bytes) VALUES (?1, ?2, ?3)",
            params![hash, mime, bytes],
        )?;
        Ok(())
    }

    pub fn asset(&self, hash: &str) -> Result<Option<(String, Vec<u8>)>> {
        Ok(self
            .conn
            .query_row("SELECT mime, bytes FROM assets WHERE hash = ?1", params![hash], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?)
    }

    /// `PRAGMA integrity_check` returns "ok" for a healthy database.
    pub fn integrity_ok(&self) -> Result<bool> {
        let r: String = self.conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        Ok(r == "ok")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_load_compact() {
        let mut s = Store::open_in_memory().unwrap();
        assert_eq!(s.load().unwrap(), Loaded::default());
        s.append_update(b"u1").unwrap();
        s.append_update(b"u2").unwrap();
        assert_eq!(s.load().unwrap().updates, vec![b"u1".to_vec(), b"u2".to_vec()]);
        assert_eq!(s.updates_after(1).unwrap(), vec![(2, b"u2".to_vec())]);
        s.compact(b"snap12").unwrap();
        assert_eq!(s.pending_updates().unwrap(), 0);
        s.append_update(b"u3").unwrap();
        let l = s.load().unwrap();
        assert_eq!(l.snapshot.as_deref(), Some(&b"snap12"[..]));
        assert_eq!(l.updates, vec![b"u3".to_vec()]);
        s.compact(b"snap123").unwrap();
        s.compact(b"snap123-again").unwrap();
        let l = s.load().unwrap();
        assert_eq!(
            (l.snapshot.as_deref(), l.updates.len()),
            (Some(&b"snap123-again"[..]), 0)
        );
        assert!(s.integrity_ok().unwrap());
    }

    #[test]
    fn meta_assets_and_reopen() {
        let dir = std::env::temp_dir().join(format!("nb-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("n.db");
        {
            let s = Store::open(&path).unwrap();
            s.set_meta("title", "Cahier".as_bytes()).unwrap();
            s.put_asset("abc", "image/png", b"png").unwrap();
            s.put_asset("abc", "image/png", b"ignored").unwrap();
            s.append_update(b"u").unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.meta("title").unwrap().as_deref(), Some("Cahier".as_bytes()));
        assert_eq!(s.asset("abc").unwrap(), Some(("image/png".into(), b"png".to_vec())));
        assert_eq!(s.load().unwrap().updates.len(), 1);
        drop(s);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = std::env::temp_dir().join(format!("nb-store-new-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("n.db");
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(matches!(Store::open(&path), Err(StoreError::TooNew { found: 99 })));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
