//! SQLite-Datenbank für Zuordnungen, Outbox, Prüfliste, Cache und Sync-Protokoll.
//!
//! rusqlite ist synchron. Die Verbindung gehört einem eigenen Thread, async-Code schickt ihm
//! Aufträge und wartet auf die Antwort. So blockiert nie ein Tokio-Worker.

use std::path::Path;
use std::sync::mpsc;

use musubi_domain::{EntryPatch, Mapping, MappingSource};
use rusqlite::{Connection, OptionalExtension, Row, params};
use rusqlite_migration::{M, Migrations};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::oneshot;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("stored data is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database thread is gone")]
    Closed,
}

const MIGRATIONS: &[M<'_>] = &[M::up(
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
     CREATE TABLE mappings (
        jellyfin_id TEXT NOT NULL,
        season INTEGER NOT NULL,
        ep_start INTEGER NOT NULL,
        ep_end INTEGER,
        media_id INTEGER NOT NULL,
        ep_offset INTEGER NOT NULL,
        source TEXT NOT NULL,
        confidence REAL NOT NULL,
        title TEXT NOT NULL,
        PRIMARY KEY (jellyfin_id, season, ep_start)
     );
     CREATE INDEX mappings_media ON mappings (media_id);
     CREATE TABLE outbox (
        media_id INTEGER PRIMARY KEY,
        op TEXT NOT NULL,
        revision INTEGER NOT NULL,
        attempts INTEGER NOT NULL DEFAULT 0,
        next_attempt_at INTEGER NOT NULL DEFAULT 0,
        last_error TEXT,
        failed INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE review (
        jellyfin_id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        candidates TEXT NOT NULL,
        updated_at INTEGER NOT NULL
     );
     CREATE TABLE cache (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        fetched_at INTEGER NOT NULL
     );
     CREATE TABLE sync_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        ok INTEGER NOT NULL,
        message TEXT NOT NULL
     );",
)];

/// Ein ausstehender Schreibzugriff auf AniList.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OutboxOp {
    Save(EntryPatch),
    Delete { entry_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutboxItem {
    pub media_id: i64,
    pub op: OutboxOp,
    pub revision: i64,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewCandidate {
    pub media_id: i64,
    pub title: String,
    pub format: Option<String>,
    pub year: Option<i32>,
    pub episodes: Option<u32>,
    pub score: f64,
}

/// Serie ohne sichere automatische Zuordnung.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewItem {
    pub jellyfin_id: String,
    pub title: String,
    pub candidates: Vec<ReviewCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncLogEntry {
    pub at: i64,
    pub ok: bool,
    pub message: String,
}

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

#[derive(Clone)]
pub struct Db {
    tx: mpsc::Sender<Job>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        Self::start(conn)
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::start(Connection::open_in_memory()?)
    }

    fn start(mut conn: Connection) -> Result<Self, DbError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Migrations::from_slice(MIGRATIONS).to_latest(&mut conn)?;
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("musubi-db".to_owned())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    job(&mut conn);
                }
            })
            .map_err(|_| DbError::Closed)?;
        Ok(Self { tx })
    }

    async fn call<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, DbError> + Send + 'static,
    {
        let (reply, response) = oneshot::channel();
        self.tx
            .send(Box::new(move |conn| {
                let _ = reply.send(f(conn));
            }))
            .map_err(|_| DbError::Closed)?;
        response.await.map_err(|_| DbError::Closed)?
    }

    // Meta -------------------------------------------------------------------------------

    pub async fn meta_get(&self, key: &str) -> Result<Option<String>, DbError> {
        let key = key.to_owned();
        self.call(move |c| {
            Ok(
                c.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
                    .optional()?,
            )
        })
        .await
    }

    pub async fn meta_set(&self, key: &str, value: &str) -> Result<(), DbError> {
        let (key, value) = (key.to_owned(), value.to_owned());
        self.call(move |c| {
            c.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn meta_delete(&self, key: &str) -> Result<(), DbError> {
        let key = key.to_owned();
        self.call(move |c| {
            c.execute("DELETE FROM meta WHERE key = ?1", [key])?;
            Ok(())
        })
        .await
    }

    // Zuordnungen ------------------------------------------------------------------------

    fn mapping_from_row(row: &Row<'_>) -> rusqlite::Result<Mapping> {
        Ok(Mapping {
            jellyfin_id: row.get(0)?,
            season: row.get(1)?,
            ep_start: row.get(2)?,
            ep_end: row.get(3)?,
            media_id: row.get(4)?,
            offset: row.get(5)?,
            source: MappingSource::parse(&row.get::<_, String>(6)?),
            confidence: row.get(7)?,
            title: row.get(8)?,
        })
    }

    pub async fn all_mappings(&self) -> Result<Vec<Mapping>, DbError> {
        self.call(|c| {
            let mut stmt = c.prepare(
                "SELECT jellyfin_id, season, ep_start, ep_end, media_id, ep_offset, source,
                        confidence, title
                 FROM mappings ORDER BY jellyfin_id, season, ep_start",
            )?;
            let rows = stmt.query_map([], Self::mapping_from_row)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    pub async fn mappings_for(&self, jellyfin_id: &str) -> Result<Vec<Mapping>, DbError> {
        let id = jellyfin_id.to_owned();
        self.call(move |c| {
            let mut stmt = c.prepare(
                "SELECT jellyfin_id, season, ep_start, ep_end, media_id, ep_offset, source,
                        confidence, title
                 FROM mappings WHERE jellyfin_id = ?1 ORDER BY season, ep_start",
            )?;
            let rows = stmt.query_map([id], Self::mapping_from_row)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Ersetzt alle Zuordnungen einer Serie und entfernt sie aus der Prüfliste.
    pub async fn replace_mappings(
        &self,
        jellyfin_id: &str,
        mappings: Vec<Mapping>,
    ) -> Result<(), DbError> {
        let id = jellyfin_id.to_owned();
        self.call(move |c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM mappings WHERE jellyfin_id = ?1", [&id])?;
            tx.execute("DELETE FROM review WHERE jellyfin_id = ?1", [&id])?;
            for m in mappings {
                tx.execute(
                    "INSERT INTO mappings (jellyfin_id, season, ep_start, ep_end, media_id,
                                           ep_offset, source, confidence, title)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        id,
                        m.season,
                        m.ep_start,
                        m.ep_end,
                        m.media_id,
                        m.offset,
                        m.source.as_str(),
                        m.confidence,
                        m.title
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    // Prüfliste --------------------------------------------------------------------------

    pub async fn set_review(&self, item: ReviewItem, now: i64) -> Result<(), DbError> {
        self.call(move |c| {
            let candidates = serde_json::to_string(&item.candidates)?;
            c.execute(
                "INSERT INTO review (jellyfin_id, title, candidates, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (jellyfin_id) DO UPDATE SET
                    title = excluded.title, candidates = excluded.candidates,
                    updated_at = excluded.updated_at",
                params![item.jellyfin_id, item.title, candidates, now],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn reviews(&self) -> Result<Vec<ReviewItem>, DbError> {
        self.call(|c| {
            let mut stmt =
                c.prepare("SELECT jellyfin_id, title, candidates FROM review ORDER BY title")?;
            let rows: Vec<(String, String, String)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            rows.into_iter()
                .map(|(jellyfin_id, title, candidates)| {
                    Ok(ReviewItem {
                        jellyfin_id,
                        title,
                        candidates: serde_json::from_str(&candidates)?,
                    })
                })
                .collect()
        })
        .await
    }

    // Outbox -----------------------------------------------------------------------------

    /// Reiht einen Schreibzugriff ein und fasst ihn mit einem offenen desselben Werks zusammen.
    pub async fn outbox_enqueue(&self, media_id: i64, op: OutboxOp) -> Result<(), DbError> {
        self.call(move |c| {
            let tx = c.transaction()?;
            let existing: Option<(String, i64)> = tx
                .query_row(
                    "SELECT op, revision FROM outbox WHERE media_id = ?1",
                    [media_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let (merged, revision) = match existing {
                Some((text, revision)) => {
                    let merged = match (serde_json::from_str::<OutboxOp>(&text)?, op) {
                        (OutboxOp::Save(mut old), OutboxOp::Save(new)) => {
                            old.merge(new);
                            OutboxOp::Save(old)
                        }
                        (_, new) => new,
                    };
                    (merged, revision + 1)
                }
                None => (op, 1),
            };
            tx.execute(
                "INSERT INTO outbox (media_id, op, revision, attempts, next_attempt_at, failed)
                 VALUES (?1, ?2, ?3, 0, 0, 0)
                 ON CONFLICT (media_id) DO UPDATE SET
                    op = excluded.op, revision = excluded.revision, attempts = 0,
                    next_attempt_at = 0, last_error = NULL, failed = 0",
                params![media_id, serde_json::to_string(&merged)?, revision],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn outbox_due(&self, now: i64) -> Result<Vec<OutboxItem>, DbError> {
        self.call(move |c| Self::read_outbox(c, "WHERE failed = 0 AND next_attempt_at <= ?1", now))
            .await
    }

    /// Alle offenen und fehlgeschlagenen Schreibzugriffe.
    pub async fn outbox_all(&self) -> Result<Vec<OutboxItem>, DbError> {
        self.call(|c| Self::read_outbox(c, "WHERE ?1 = ?1", 0))
            .await
    }

    fn read_outbox(c: &Connection, filter: &str, arg: i64) -> Result<Vec<OutboxItem>, DbError> {
        let mut stmt = c.prepare(&format!(
            "SELECT media_id, op, revision, attempts, last_error, failed
             FROM outbox {filter} ORDER BY media_id"
        ))?;
        let rows: Vec<(i64, String, i64, u32, Option<String>, bool)> = stmt
            .query_map([arg], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        rows.into_iter()
            .map(|(media_id, op, revision, attempts, last_error, failed)| {
                Ok(OutboxItem {
                    media_id,
                    op: serde_json::from_str(&op)?,
                    revision,
                    attempts,
                    last_error,
                    failed,
                })
            })
            .collect()
    }

    /// Entfernt einen erledigten Auftrag, sofern er inzwischen nicht neuer geworden ist.
    pub async fn outbox_done(&self, media_id: i64, revision: i64) -> Result<bool, DbError> {
        self.call(move |c| {
            let removed = c.execute(
                "DELETE FROM outbox WHERE media_id = ?1 AND revision = ?2",
                params![media_id, revision],
            )?;
            Ok(removed > 0)
        })
        .await
    }

    pub async fn outbox_retry_later(
        &self,
        media_id: i64,
        revision: i64,
        next_attempt_at: i64,
        error: &str,
        failed: bool,
    ) -> Result<(), DbError> {
        let error = error.to_owned();
        self.call(move |c| {
            c.execute(
                "UPDATE outbox SET attempts = attempts + 1, next_attempt_at = ?1,
                                   last_error = ?2, failed = ?3
                 WHERE media_id = ?4 AND revision = ?5",
                params![next_attempt_at, error, failed, media_id, revision],
            )?;
            Ok(())
        })
        .await
    }

    /// Macht fehlgeschlagene Aufträge wieder fällig.
    pub async fn outbox_reset_failed(&self) -> Result<(), DbError> {
        self.call(|c| {
            c.execute(
                "UPDATE outbox SET failed = 0, attempts = 0, next_attempt_at = 0, last_error = NULL
                 WHERE failed = 1",
                [],
            )?;
            Ok(())
        })
        .await
    }

    // Cache ------------------------------------------------------------------------------

    pub async fn cache_put<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        now: i64,
    ) -> Result<(), DbError> {
        let key = key.to_owned();
        let text = serde_json::to_string(value)?;
        self.call(move |c| {
            c.execute(
                "INSERT INTO cache (key, value, fetched_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (key) DO UPDATE SET
                    value = excluded.value, fetched_at = excluded.fetched_at",
                params![key, text, now],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn cache_get<T: DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
    ) -> Result<Option<T>, DbError> {
        let key = key.to_owned();
        self.call(move |c| {
            let text: Option<String> = c
                .query_row("SELECT value FROM cache WHERE key = ?1", [key], |r| {
                    r.get(0)
                })
                .optional()?;
            Ok(text.map(|t| serde_json::from_str(&t)).transpose()?)
        })
        .await
    }

    pub async fn clear_prefix(&self, prefix: &str) -> Result<(), DbError> {
        let pattern = format!("{prefix}%");
        self.call(move |c| {
            c.execute("DELETE FROM cache WHERE key LIKE ?1", [pattern])?;
            Ok(())
        })
        .await
    }

    // Protokoll --------------------------------------------------------------------------

    pub async fn log_sync(&self, at: i64, ok: bool, message: &str) -> Result<(), DbError> {
        let message = message.to_owned();
        self.call(move |c| {
            c.execute(
                "INSERT INTO sync_log (at, ok, message) VALUES (?1, ?2, ?3)",
                params![at, ok, message],
            )?;
            c.execute(
                "DELETE FROM sync_log WHERE id <= (SELECT MAX(id) - 500 FROM sync_log)",
                [],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn sync_log(&self, limit: u32) -> Result<Vec<SyncLogEntry>, DbError> {
        self.call(move |c| {
            let mut stmt =
                c.prepare("SELECT at, ok, message FROM sync_log ORDER BY id DESC LIMIT ?1")?;
            let rows = stmt.query_map([limit], |r| {
                Ok(SyncLogEntry {
                    at: r.get(0)?,
                    ok: r.get(1)?,
                    message: r.get(2)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save(media_id: i64, progress: Option<u32>, notes: Option<&str>) -> OutboxOp {
        OutboxOp::Save(EntryPatch {
            progress,
            notes: notes.map(str::to_owned),
            ..EntryPatch::new(media_id)
        })
    }

    #[tokio::test]
    async fn outbox_coalesces_saves_per_media() {
        let db = Db::open_in_memory().unwrap();
        db.outbox_enqueue(1, save(1, Some(3), Some("x")))
            .await
            .unwrap();
        db.outbox_enqueue(1, save(1, Some(4), None)).await.unwrap();

        let items = db.outbox_due(0).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].revision, 2);
        let OutboxOp::Save(patch) = &items[0].op else {
            panic!("expected save")
        };
        assert_eq!(patch.progress, Some(4));
        assert_eq!(patch.notes.as_deref(), Some("x"));
    }

    #[tokio::test]
    async fn delete_replaces_pending_save_and_save_replaces_delete() {
        let db = Db::open_in_memory().unwrap();
        db.outbox_enqueue(1, save(1, Some(3), None)).await.unwrap();
        db.outbox_enqueue(1, OutboxOp::Delete { entry_id: 9 })
            .await
            .unwrap();
        assert_eq!(
            db.outbox_due(0).await.unwrap()[0].op,
            OutboxOp::Delete { entry_id: 9 }
        );

        db.outbox_enqueue(1, save(1, Some(1), None)).await.unwrap();
        assert!(matches!(
            db.outbox_due(0).await.unwrap()[0].op,
            OutboxOp::Save(_)
        ));
    }

    #[tokio::test]
    async fn newer_revision_survives_completion_of_older_one() {
        let db = Db::open_in_memory().unwrap();
        db.outbox_enqueue(1, save(1, Some(3), None)).await.unwrap();
        let sent = db.outbox_due(0).await.unwrap().remove(0);
        db.outbox_enqueue(1, save(1, Some(4), None)).await.unwrap();

        assert!(!db.outbox_done(1, sent.revision).await.unwrap());
        assert_eq!(db.outbox_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn backoff_hides_item_until_due_and_failed_stays_out() {
        let db = Db::open_in_memory().unwrap();
        db.outbox_enqueue(1, save(1, Some(3), None)).await.unwrap();
        db.outbox_retry_later(1, 1, 100, "boom", false)
            .await
            .unwrap();
        assert!(db.outbox_due(99).await.unwrap().is_empty());
        let due = db.outbox_due(100).await.unwrap();
        assert_eq!(
            (due[0].attempts, due[0].last_error.as_deref()),
            (1, Some("boom"))
        );

        db.outbox_retry_later(1, 1, 0, "fatal", true).await.unwrap();
        assert!(db.outbox_due(1000).await.unwrap().is_empty());
        db.outbox_reset_failed().await.unwrap();
        assert_eq!(db.outbox_due(0).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn replacing_mappings_clears_review() {
        let db = Db::open_in_memory().unwrap();
        db.set_review(
            ReviewItem {
                jellyfin_id: "a".into(),
                title: "A".into(),
                candidates: vec![],
            },
            1,
        )
        .await
        .unwrap();
        assert_eq!(db.reviews().await.unwrap().len(), 1);

        let mapping = Mapping {
            jellyfin_id: "a".into(),
            season: 1,
            ep_start: 1,
            ep_end: None,
            media_id: 5,
            offset: 0,
            source: MappingSource::Manual,
            confidence: 1.0,
            title: "A".into(),
        };
        db.replace_mappings("a", vec![mapping.clone()])
            .await
            .unwrap();
        assert_eq!(db.mappings_for("a").await.unwrap(), vec![mapping]);
        assert!(db.reviews().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn cache_round_trips() {
        let db = Db::open_in_memory().unwrap();
        db.cache_put("k", &vec![1, 2, 3], 1).await.unwrap();
        assert_eq!(
            db.cache_get::<Vec<i32>>("k").await.unwrap(),
            Some(vec![1, 2, 3])
        );
        db.clear_prefix("k").await.unwrap();
        assert_eq!(db.cache_get::<Vec<i32>>("k").await.unwrap(), None);
    }
}
