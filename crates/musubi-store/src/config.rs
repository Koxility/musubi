//! Nicht geheime Einstellungen aus `config.toml`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::atomic_write;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not access config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid config file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("could not serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub jellyfin: JellyfinConfig,
    pub anilist: AniListConfig,
    pub sync: SyncConfig,
    pub player: PlayerConfig,
    pub secrets: SecretsConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JellyfinConfig {
    /// Basis-URL des Servers, z. B. `https://jellyfin.example.org`.
    pub url: Option<String>,
    /// Bibliotheken, die Musubi anzeigt. Leer bedeutet alle.
    pub library_ids: Vec<String>,
    /// Maximale Bitrate in bit/s. Ohne Wert entscheidet der Server.
    pub max_bitrate: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AniListLoginFlow {
    #[default]
    Loopback,
    Pin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AniListConfig {
    /// Client-ID der eigenen AniList-Anwendung.
    pub client_id: Option<String>,
    pub login_flow: AniListLoginFlow,
    pub loopback_port: u16,
}

impl Default for AniListConfig {
    fn default() -> Self {
        Self {
            client_id: None,
            login_flow: AniListLoginFlow::default(),
            loopback_port: 47823,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// Fortschritt beim Abspielen automatisch an AniList melden.
    pub auto_sync: bool,
    /// Anteil der Laufzeit, ab dem eine Episode als gesehen gilt (0.0..=1.0).
    pub watched_threshold: f64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            auto_sync: true,
            watched_threshold: 0.85,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    pub mpv_path: String,
    /// Wert für `--hwdec`. Leer lässt die Einstellung aus der eigenen `mpv.conf` gelten.
    pub hwdec: String,
    pub extra_args: Vec<String>,
    /// Folgeepisoden automatisch einreihen.
    pub autoplay_next: bool,
    pub backend: PlayerKind,
}

/// Wo Videos laufen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerKind {
    /// Im Fenster der App.
    #[default]
    Embedded,
    /// Im eigenen Fenster von mpv.
    Mpv,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            mpv_path: "mpv".to_owned(),
            hwdec: "auto-safe".to_owned(),
            extra_args: Vec::new(),
            autoplay_next: true,
            backend: PlayerKind::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecretBackend {
    /// `credentials.toml` mit Dateirechten 0600.
    #[default]
    File,
    /// Secret-Service-API (KWallet, GNOME Keyring).
    SecretService,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SecretsConfig {
    pub backend: SecretBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Die geführte Einrichtung wurde abgeschlossen oder bewusst beendet.
    pub setup_completed: bool,
}

impl Config {
    /// Lädt die Konfiguration. Eine fehlende Datei ergibt die Standardwerte.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        atomic_write(path, text.as_bytes(), 0o600)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults_and_partial_files_merge() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert_eq!(Config::load(&path).unwrap(), Config::default());

        std::fs::write(&path, "[sync]\nauto_sync = false\n").unwrap();
        let config = Config::load(&path).unwrap();
        assert!(!config.sync.auto_sync);
        assert_eq!(config.sync.watched_threshold, 0.85);
        assert_eq!(config.player.mpv_path, "mpv");
    }

    #[test]
    fn settings_of_the_removed_tui_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[ui]\ndefault = \"tui\"\nsetup_completed = true\n\n[tui]\nposters = true\n\n[tui.keys]\nplay = \"p\"\n",
        )
        .unwrap();
        assert!(Config::load(&path).unwrap().ui.setup_completed);
    }

    #[test]
    fn config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.jellyfin.url = Some("https://example.org".into());
        config.secrets.backend = SecretBackend::SecretService;
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }
}
