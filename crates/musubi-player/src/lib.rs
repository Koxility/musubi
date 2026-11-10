//! Player-Abstraktion: Wiedergabeliste, Befehle und Ereignisse.
//!
//! Der Kern spricht nur mit [`PlayerBackend`]. Die GUI liefert die Umsetzung, die Videos über
//! GStreamer im App-Fenster abspielt.

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum PlayerError {
    /// Kein Player verfügbar, etwa weil GStreamer-Plugins fehlen.
    #[error("{0}")]
    Unavailable(String),
    #[error("connection to the player was lost")]
    Disconnected,
}

pub type Result<T> = std::result::Result<T, PlayerError>;

#[derive(Debug, Clone)]
pub struct SubtitleTrack {
    pub url: SecretString,
    pub title: Option<String>,
    pub language: Option<String>,
}

/// Ein Eintrag der Wiedergabeliste. Die URL kann Zugangsdaten enthalten.
#[derive(Debug, Clone)]
pub struct PlayItem {
    pub url: SecretString,
    pub title: String,
    pub start_secs: Option<f64>,
    pub subtitles: Vec<SubtitleTrack>,
}

#[derive(Debug, Clone)]
pub struct OpenRequest {
    /// HTTP-Header für alle Streams, z. B. die Jellyfin-Anmeldung.
    pub http_headers: Vec<SecretString>,
    pub first: PlayItem,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerCommand {
    TogglePause,
    SeekRelative(f64),
    SeekAbsolute(f64),
    CycleSubtitle,
    CycleAudio,
    Next,
    Previous,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndReason {
    Eof,
    Stopped,
    Quit,
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    /// Eintrag `index` der Wiedergabeliste wurde geladen.
    Started {
        index: usize,
    },
    Position {
        position: f64,
        duration: Option<f64>,
        paused: bool,
    },
    Paused(bool),
    Seeked,
    Ended {
        index: usize,
        reason: EndReason,
    },
    /// Die Wiedergabe ist beendet und der Player geschlossen.
    Exited,
}

#[async_trait]
pub trait PlayerSession: Send + Sync {
    /// Hängt einen Eintrag an die Wiedergabeliste an.
    async fn append(&self, item: PlayItem) -> Result<()>;
    async fn command(&self, command: PlayerCommand) -> Result<()>;
}

pub type SessionHandle = Arc<dyn PlayerSession>;

/// Austauschpunkt für Player, derzeit der eingebettete Player der GUI.
#[async_trait]
pub trait PlayerBackend: Send + Sync {
    async fn open(
        &self,
        request: OpenRequest,
    ) -> Result<(SessionHandle, mpsc::Receiver<PlayerEvent>)>;
}
