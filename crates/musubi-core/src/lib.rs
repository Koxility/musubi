//! Fachlogik von Musubi und die Schnittstelle für die GUI.

mod anilist_ops;
mod auth;
mod error;
mod events;
mod images;
mod inner;
mod library;
mod mapping_ops;
pub mod matching;
mod outbox;
mod playback;
pub mod sync;
mod sync_ops;

pub use error::{CoreError, Result, Service};
pub use events::{CoreEvent, PlaybackState};
pub use inner::{AuthStatus, Core, CoreOptions};
pub use library::{EpisodeRow, HomeShelves, LibraryRow};
pub use mapping_ops::MatchOutcome;
pub use sync::{SyncDecision, SyncDirection, SyncRow};
pub use sync_ops::SyncReport;

pub use musubi_anilist::oauth;
pub use musubi_domain as domain;
pub use musubi_player::{
    EndReason, OpenRequest, PlayItem, PlayerBackend, PlayerCommand, PlayerError, PlayerEvent,
    PlayerSession, SessionHandle,
};
pub use musubi_store::{
    AniListLoginFlow, Config, OutboxItem, OutboxOp, Paths, ReviewCandidate, ReviewItem,
    SyncLogEntry, ViewConfig, ViewMode, ViewSize,
};
