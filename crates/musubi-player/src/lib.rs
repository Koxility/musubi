//! Player-Abstraktion und mpv-Backend über JSON-IPC.
//!
//! mpv läuft als eigener Prozess. Das liefert Hardware-Decoding, Untertitel-Rendering und
//! Wayland-/X11-Unterstützung ohne FFI, und ein Player-Absturz betrifft die App nicht.

mod mpv;

pub use mpv::{MpvBackend, MpvOptions};

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("could not start mpv (`{path}`): {source}")]
    Spawn {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("mpv did not open its control socket in time")]
    ConnectTimeout,
    #[error("connection to mpv was lost")]
    Disconnected,
    #[error("mpv rejected the command: {0}")]
    Command(String),
    #[error("mpv I/O error: {0}")]
    Io(#[from] std::io::Error),
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
    /// HTTP-Header für alle Streams. Werden per IPC übergeben, nie als Prozess-Argument.
    pub http_headers: Vec<SecretString>,
    pub first: PlayItem,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
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
    /// Der Player-Prozess ist beendet.
    Exited,
}

#[async_trait]
pub trait PlayerSession: Send + Sync {
    /// Hängt einen Eintrag an die Wiedergabeliste an.
    async fn append(&self, item: PlayItem) -> Result<()>;
    async fn command(&self, command: PlayerCommand) -> Result<()>;
}

pub type SessionHandle = Arc<dyn PlayerSession>;

/// Austauschpunkt für künftige Backends, etwa eingebettetes libmpv.
#[async_trait]
pub trait PlayerBackend: Send + Sync {
    async fn open(
        &self,
        request: OpenRequest,
    ) -> Result<(SessionHandle, mpsc::Receiver<PlayerEvent>)>;
}
