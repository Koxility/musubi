//! Embedded fonts: Geist for the interface, Geist Mono for versions and other fixed text.
//!
//! The static cuts are embedded rather than the variable font, because the text stack picks
//! a weight by choosing a file, not by setting a variation axis.

use iced::{Font, font};

/// The font files, as they are loaded at startup.
pub const FONT_FILES: [&[u8]; 4] = [
    include_bytes!("../../fonts/Geist-Regular.ttf"),
    include_bytes!("../../fonts/Geist-Medium.ttf"),
    include_bytes!("../../fonts/Geist-SemiBold.ttf"),
    include_bytes!("../../fonts/GeistMono-Regular.ttf"),
];

pub const BODY: Font = Font::with_name("Geist");
pub const MEDIUM: Font = Font {
    weight: font::Weight::Medium,
    ..BODY
};
pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..BODY
};
/// Titles and headings. The same family as the text, set apart by weight and size alone.
pub const DISPLAY: Font = SEMIBOLD;
/// Versions, commits and other text that should not shift as its digits change.
pub const MONO: Font = Font::with_name("Geist Mono");
