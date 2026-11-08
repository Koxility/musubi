use std::time::Duration;

use musubi_domain::MediaKind;
use serde::Serialize;

use crate::Service;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaybackState {
    pub item_id: String,
    pub title: String,
    pub position: f64,
    pub duration: Option<f64>,
    pub paused: bool,
}

/// Benachrichtigungen an die Oberflächen.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum CoreEvent {
    Playback(PlaybackState),
    PlaybackEnded,
    LibraryChanged,
    ListChanged(MediaKind),
    MappingsChanged,
    /// Fortschritt der automatischen Zuordnung.
    MatchProgress {
        done: usize,
        total: usize,
    },
    /// Ergebnis eines Sync-Schritts für Statuszeile und Protokoll.
    Notice {
        ok: bool,
        message: String,
    },
    /// Das Rate-Limit greift, `None` hebt die Meldung auf.
    RateLimited(Option<Duration>),
    AuthRequired(Service),
    TokenExpiring {
        days: i64,
    },
}
