//! Named values for colour, spacing, radius, font size and motion.
//!
//! The neutral scale, the state colours, the radius and the type sizes are the ones of
//! Kura's `tokens.css`, so both apps read as one family. The two cord colours are Musubi's.

use std::time::Duration;

use iced::{Color, Theme};
use musubi_core::domain::ListStatus;

// Surfaces, from back to front.
/// The page.
pub const BG: Color = Color::from_rgb8(0x0a, 0x0a, 0x0a);
/// One step lighter: the sidebar, cards, inputs, dialogs.
pub const SURFACE: Color = Color::from_rgb8(0x14, 0x14, 0x14);
/// Hovered and selected things, and fills inside a surface.
pub const HOVER: Color = Color::from_rgb8(0x1c, 0x1c, 0x1c);
/// Hairlines and borders.
pub const LINE: Color = Color::from_rgb8(0x26, 0x26, 0x26);

// Text. `TEXT_3` is also the border of a hovered or focused control.
pub const TEXT: Color = Color::from_rgb8(0xf5, 0xf5, 0xf5);
pub const TEXT_2: Color = Color::from_rgb8(0xa1, 0xa1, 0xa1);
pub const TEXT_3: Color = Color::from_rgb8(0x6b, 0x6b, 0x6b);

// State only. There is no brand colour: primary actions are `TEXT` on `BG`.
pub const OK: Color = Color::from_rgb8(0x3e, 0xcf, 0x8e);
pub const WARN: Color = Color::from_rgb8(0xf5, 0xa5, 0x24);
pub const BAD: Color = Color::from_rgb8(0xf0, 0x44, 0x38);

// The two cords. They mark what belongs to which service and never fill a large area:
// progress, connection dots, tags, the active page, the "+1" pill.
/// "Watch": playback and progress in Jellyfin. A rose red, apart from the orange `BAD`.
pub const WATCH: Color = Color::from_rgb8(0xf2, 0x5f, 0x7a);
/// "Track": lists, status and sync with AniList. A cyan teal, apart from the green `OK`.
pub const TRACK: Color = Color::from_rgb8(0x38, 0xc6, 0xd0);

const REPEAT: Color = Color::from_rgb8(0xa4, 0x8c, 0xf2);
const PLANNED: Color = Color::from_rgb8(0x8a, 0x94, 0xa6);

/// Text and icons over the video.
pub const ON_VIDEO: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.94);
pub const ON_VIDEO_MUTED: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.62);

// Spacing.
pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
pub const S5: f32 = 24.0;
pub const S6: f32 = 32.0;

// Radii.
/// Posters and other pictures.
pub const R_COVER: f32 = 6.0;
/// Buttons, inputs, rows, cards.
pub const RADIUS: f32 = 8.0;
/// Dialogs and other panels that float above the page.
pub const R_PANEL: f32 = 12.0;
pub const PILL: f32 = 999.0;

// Font sizes.
pub const T_CAPTION: f32 = 11.0;
pub const T_LABEL: f32 = 12.0;
pub const T_SMALL: f32 = 13.0;
pub const T_BODY: f32 = 15.0;
/// Titles of the large cards on Home.
pub const T_CARD: f32 = 16.0;
pub const T_TITLE: f32 = 18.0;
pub const T_SECTION: f32 = 20.0;
pub const T_PAGE: f32 = 32.0;
/// Title on the Home hero.
pub const T_HERO: f32 = 48.0;

// Motion. Short and eased out: things settle, they do not bounce.
/// Layout changes such as the sidebar sliding away.
pub const MOTION: Duration = Duration::from_millis(220);

/// Fades of pages and covers.
pub const FADE: Duration = Duration::from_millis(140);

/// How far a fade that started `elapsed` ago has come, from 0 to 1, eased out.
pub fn fade_in(elapsed: Duration) -> f32 {
    let t = (elapsed.as_secs_f32() / FADE.as_secs_f32()).min(1.0);
    1.0 - (1.0 - t).powi(3)
}

/// The same colour with a different opacity.
pub const fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

/// The colour of a list status, the same everywhere: sidebar, rows, statistics.
pub fn status_color(status: ListStatus) -> Color {
    match status {
        ListStatus::Current => TRACK,
        ListStatus::Planning => PLANNED,
        ListStatus::Completed => OK,
        ListStatus::Paused => WARN,
        ListStatus::Dropped => BAD,
        ListStatus::Repeating => REPEAT,
    }
}

pub fn app_theme() -> Theme {
    Theme::custom(
        "Musubi",
        iced::theme::Palette {
            background: BG,
            text: TEXT,
            primary: TEXT,
            success: OK,
            warning: WARN,
            danger: BAD,
        },
    )
}
