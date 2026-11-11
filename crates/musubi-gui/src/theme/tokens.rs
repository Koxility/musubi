//! Benannte Werte für Farbe, Abstand, Radius und Schriftgröße.

use iced::{Color, Theme};
use musubi_core::domain::ListStatus;

// Flächen, von hinten nach vorn.
pub const INK: Color = Color::from_rgb8(0x0d, 0x12, 0x20);
pub const SURFACE: Color = Color::from_rgb8(0x13, 0x1a, 0x2a);
pub const RAISED: Color = Color::from_rgb8(0x1c, 0x25, 0x38);
pub const OVERLAY: Color = Color::from_rgb8(0x25, 0x30, 0x49);

// Schrift.
pub const PAPER: Color = Color::from_rgb8(0xec, 0xe6, 0xda);
pub const MUTED: Color = Color::from_rgb8(0x8e, 0x97, 0xa8);
pub const FAINT: Color = Color::from_rgb8(0x5d, 0x66, 0x79);

/// Haarlinie zwischen Flächen.
pub const LINE: Color = Color::from_rgba8(0xec, 0xe6, 0xda, 0.08);
/// Etwas kräftigere Linie für Ränder von Eingabefeldern und Dialogen.
pub const LINE_STRONG: Color = Color::from_rgba8(0xec, 0xe6, 0xda, 0.16);

/// Kordel „Ansehen“: Wiedergabe und Fortschritt in Jellyfin.
pub const WATCH: Color = Color::from_rgb8(0xe0, 0x56, 0x3f);
/// Kordel „Verfolgen“: Listen, Status und Abgleich mit AniList.
pub const TRACK: Color = Color::from_rgb8(0x5e, 0xc2, 0xc4);

pub const OK: Color = Color::from_rgb8(0x8f, 0xbf, 0x6a);
pub const WARN: Color = Color::from_rgb8(0xe3, 0xb0, 0x4b);
pub const BAD: Color = Color::from_rgb8(0xf2, 0x7b, 0x8a);
const REPEAT: Color = Color::from_rgb8(0xa7, 0x8b, 0xda);
const PLANNED: Color = Color::from_rgb8(0x7c, 0x8d, 0xb5);

/// Text und Icons über dem Video.
pub const ON_VIDEO: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.94);
pub const ON_VIDEO_MUTED: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.62);

// Abstände.
pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
pub const S5: f32 = 24.0;
pub const S6: f32 = 32.0;

// Radien. Poster bleiben bewusst kantig, nur Bedienelemente des Players sind rund.
pub const R_SM: f32 = 3.0;
pub const R_MD: f32 = 6.0;
pub const R_LG: f32 = 10.0;
pub const PILL: f32 = 999.0;

// Font sizes.
pub const T_CAPTION: f32 = 11.0;
pub const T_LABEL: f32 = 12.0;
pub const T_SMALL: f32 = 13.0;
pub const T_BODY: f32 = 15.0;
/// Titles of the large cards on Home.
pub const T_CARD: f32 = 16.0;
pub const T_TITLE: f32 = 18.0;
pub const T_SECTION: f32 = 22.0;
pub const T_PAGE: f32 = 28.0;
/// Title on the Home hero.
pub const T_HERO: f32 = 52.0;

/// Dieselbe Farbe mit anderer Deckkraft.
pub const fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

/// Farbe eines Listenstatus, überall gleich: Seitenleiste, Zeilen, Statistik.
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
            background: INK,
            text: PAPER,
            primary: TRACK,
            success: OK,
            warning: WARN,
            danger: BAD,
        },
    )
}
