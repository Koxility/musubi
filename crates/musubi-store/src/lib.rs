//! Konfiguration, Secrets und lokale Datenbank.

pub mod config;
pub mod db;
pub mod paths;
pub mod secrets;

pub use config::*;
pub use db::{Db, DbError, OutboxItem, OutboxOp, ReviewCandidate, ReviewItem, SyncLogEntry};
pub use paths::Paths;
pub use secrets::{SecretError, SecretKey, SecretStore, open_secret_store};

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Schreibt eine Datei atomar (Temp-Datei + rename) mit den angegebenen Rechten.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("path has no parent directory"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("path has no file name"))?
        .to_string_lossy();
    let tmp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
