//! Reine Fachtypen von Musubi. Diese Crate macht kein IO und kennt weder HTTP noch UI.

pub mod anilist;
pub mod jellyfin;
pub mod mapping;

pub use anilist::*;
pub use jellyfin::*;
pub use mapping::*;
