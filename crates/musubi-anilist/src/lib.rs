//! Client für die AniList-GraphQL-API inklusive Login und Rate-Limiter.

mod client;
mod dto;
pub mod oauth;
mod queries;
pub mod rate;

pub use client::{Client, MEDIA_PAGE, Relation};
pub use rate::RateLimiter;

use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("AniList rejected the access token, please log in again")]
    Unauthorized,
    #[error("not found on AniList")]
    NotFound,
    #[error("AniList rate limit reached, retry in {} s", retry_after.as_secs())]
    RateLimited { retry_after: Duration },
    #[error("could not reach AniList: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("AniList error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("unexpected AniList response: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("login failed: {0}")]
    Login(String),
}

impl Error {
    /// Vorübergehende Fehler lohnen einen weiteren Versuch.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Transport(_) | Self::RateLimited { .. } => true,
            Self::Api { status, .. } => *status >= 500,
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
