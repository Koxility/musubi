//! The design system: colours, measures, fonts and widget styles.
//!
//! Musubi shares its look with Kura: near-black neutral surfaces, Geist, one radius, and no
//! brand colour. Two cord colours carry meaning on top of that. Red stands for watching
//! (Jellyfin), teal for tracking (AniList); they mark, they do not decorate. Blue is kept
//! for updates.

mod fonts;
mod styles;
mod tokens;

pub use fonts::*;
pub use styles::*;
pub use tokens::*;
