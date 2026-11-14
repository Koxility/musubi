use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Jellyfin,
    AniList,
}

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Jellyfin => "Jellyfin",
            Self::AniList => "AniList",
        })
    }
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("not logged in to {0}")]
    NotLoggedIn(Service),
    #[error(transparent)]
    Jellyfin(#[from] musubi_jellyfin::Error),
    #[error(transparent)]
    AniList(#[from] musubi_anilist::Error),
    #[error(transparent)]
    Player(#[from] musubi_player::PlayerError),
    #[error(transparent)]
    Database(#[from] musubi_store::DbError),
    #[error(transparent)]
    Config(#[from] musubi_store::ConfigError),
    #[error(transparent)]
    Secret(#[from] musubi_store::SecretError),
    #[error("{0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("download failed: {0}")]
    Download(#[from] reqwest::Error),
    #[error("update failed: {0}")]
    Update(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
