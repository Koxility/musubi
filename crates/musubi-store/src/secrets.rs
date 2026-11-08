//! Ablage der Zugangstokens.
//!
//! Standard ist `credentials.toml` mit Rechten 0600. Optional wird die Secret-Service-API
//! (KWallet, GNOME Keyring) genutzt. Passwörter werden nirgends gespeichert.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

use crate::config::SecretBackend;
use crate::{Paths, atomic_write};

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("could not access credentials file: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "{path} is readable by other users (mode {mode:o}); run `chmod 600` on it before continuing"
    )]
    InsecurePermissions { path: PathBuf, mode: u32 },
    #[error("credentials file is malformed: {0}")]
    Malformed(String),
    #[error("secret service error: {0}")]
    Keyring(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKey {
    JellyfinToken,
    AniListToken,
}

impl SecretKey {
    fn name(self) -> &'static str {
        match self {
            Self::JellyfinToken => "jellyfin_token",
            Self::AniListToken => "anilist_token",
        }
    }
}

pub trait SecretStore: Send + Sync {
    fn get(&self, key: SecretKey) -> Result<Option<SecretString>, SecretError>;
    fn set(&self, key: SecretKey, value: &SecretString) -> Result<(), SecretError>;
    fn delete(&self, key: SecretKey) -> Result<(), SecretError>;
}

pub fn open_secret_store(
    backend: SecretBackend,
    paths: &Paths,
) -> Result<Box<dyn SecretStore>, SecretError> {
    Ok(match backend {
        SecretBackend::File => Box::new(FileSecretStore::new(paths.credentials_file())),
        SecretBackend::SecretService => Box::new(KeyringSecretStore::new()?),
    })
}

/// TOML-Datei, die nur der Eigentümer lesen darf.
pub struct FileSecretStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileSecretStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    fn read(path: &Path) -> Result<BTreeMap<String, String>, SecretError> {
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(e.into()),
        };
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(SecretError::InsecurePermissions {
                path: path.to_path_buf(),
                mode,
            });
        }
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| SecretError::Malformed(e.message().to_owned()))
    }

    fn write(&self, values: &BTreeMap<String, String>) -> Result<(), SecretError> {
        let text = toml::to_string(values).map_err(|e| SecretError::Malformed(e.to_string()))?;
        atomic_write(&self.path, text.as_bytes(), 0o600)?;
        Ok(())
    }
}

impl SecretStore for FileSecretStore {
    fn get(&self, key: SecretKey) -> Result<Option<SecretString>, SecretError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        Ok(Self::read(&self.path)?
            .remove(key.name())
            .map(SecretString::from))
    }

    fn set(&self, key: SecretKey, value: &SecretString) -> Result<(), SecretError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut values = Self::read(&self.path)?;
        values.insert(key.name().to_owned(), value.expose_secret().to_owned());
        self.write(&values)
    }

    fn delete(&self, key: SecretKey) -> Result<(), SecretError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut values = Self::read(&self.path)?;
        if values.remove(key.name()).is_some() {
            self.write(&values)?;
        }
        Ok(())
    }
}

const KEYRING_SERVICE: &str = "musubi";

/// Secret-Service-Anbindung. Die Aufrufe blockieren und laufen deshalb auf einem eigenen
/// Thread, damit sie nicht innerhalb des Tokio-Runtimes ein weiteres `block_on` auslösen.
pub struct KeyringSecretStore;

fn off_runtime<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, SecretError> + Send + 'static,
) -> Result<T, SecretError> {
    std::thread::spawn(f)
        .join()
        .map_err(|_| SecretError::Keyring("secret service thread panicked".to_owned()))?
}

fn keyring_err(e: keyring_core::Error) -> SecretError {
    SecretError::Keyring(e.to_string())
}

impl KeyringSecretStore {
    pub fn new() -> Result<Self, SecretError> {
        off_runtime(|| {
            let store = zbus_secret_service_keyring_store::Store::new().map_err(keyring_err)?;
            keyring_core::set_default_store(store);
            Ok(Self)
        })
    }
}

impl SecretStore for KeyringSecretStore {
    fn get(&self, key: SecretKey) -> Result<Option<SecretString>, SecretError> {
        off_runtime(move || {
            let entry =
                keyring_core::Entry::new(KEYRING_SERVICE, key.name()).map_err(keyring_err)?;
            match entry.get_password() {
                Ok(value) => Ok(Some(SecretString::from(value))),
                Err(keyring_core::Error::NoEntry) => Ok(None),
                Err(e) => Err(keyring_err(e)),
            }
        })
    }

    fn set(&self, key: SecretKey, value: &SecretString) -> Result<(), SecretError> {
        let value = value.clone();
        off_runtime(move || {
            keyring_core::Entry::new(KEYRING_SERVICE, key.name())
                .and_then(|entry| entry.set_password(value.expose_secret()))
                .map_err(keyring_err)
        })
    }

    fn delete(&self, key: SecretKey) -> Result<(), SecretError> {
        off_runtime(move || {
            let entry =
                keyring_core::Entry::new(KEYRING_SERVICE, key.name()).map_err(keyring_err)?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
                Err(e) => Err(keyring_err(e)),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_writes_owner_only_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        let store = FileSecretStore::new(path.clone());

        assert!(store.get(SecretKey::AniListToken).unwrap().is_none());
        store
            .set(SecretKey::AniListToken, &SecretString::from("abc"))
            .unwrap();
        store
            .set(SecretKey::JellyfinToken, &SecretString::from("def"))
            .unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let value = store.get(SecretKey::AniListToken).unwrap().unwrap();
        assert_eq!(value.expose_secret(), "abc");

        store.delete(SecretKey::AniListToken).unwrap();
        assert!(store.get(SecretKey::AniListToken).unwrap().is_none());
        assert!(store.get(SecretKey::JellyfinToken).unwrap().is_some());
    }

    #[test]
    fn file_store_refuses_world_readable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        std::fs::write(&path, "anilist_token = \"abc\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let store = FileSecretStore::new(path);
        assert!(matches!(
            store.get(SecretKey::AniListToken),
            Err(SecretError::InsecurePermissions { mode: 0o644, .. })
        ));
    }
}
