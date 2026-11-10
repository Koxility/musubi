//! Eingebettete Schriften: Sora für Titel, Hanken Grotesk für alles andere.

use iced::{Font, font};

/// Die Schriftdateien, wie sie beim Start geladen werden.
pub const FONT_FILES: [&[u8]; 5] = [
    include_bytes!("../../fonts/HankenGrotesk-Regular.ttf"),
    include_bytes!("../../fonts/HankenGrotesk-Medium.ttf"),
    include_bytes!("../../fonts/HankenGrotesk-SemiBold.ttf"),
    include_bytes!("../../fonts/Sora-SemiBold.ttf"),
    include_bytes!("../../fonts/Sora-Bold.ttf"),
];

pub const BODY: Font = Font::with_name("Hanken Grotesk");
pub const MEDIUM: Font = Font {
    weight: font::Weight::Medium,
    ..BODY
};
pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..BODY
};
pub const DISPLAY: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::with_name("Sora")
};
pub const DISPLAY_BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::with_name("Sora")
};
