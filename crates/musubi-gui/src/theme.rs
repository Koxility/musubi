//! Farben und Widget-Stile.

use iced::widget::{button, container, slider, text_input};
use iced::{Background, Border, Color, Gradient, Radians, Shadow, Theme, gradient};

pub const BG: Color = Color::from_rgb8(0x0b, 0x16, 0x22);
pub const SURFACE: Color = Color::from_rgb8(0x15, 0x1f, 0x2e);
pub const RAISED: Color = Color::from_rgb8(0x1f, 0x2b, 0x3d);
pub const ACCENT: Color = Color::from_rgb8(0x3d, 0xb4, 0xf2);
pub const TEXT: Color = Color::from_rgb8(0xe6, 0xee, 0xf5);
pub const MUTED: Color = Color::from_rgb8(0x8b, 0xa0, 0xb2);
pub const GREEN: Color = Color::from_rgb8(0x68, 0xd6, 0x39);
pub const ORANGE: Color = Color::from_rgb8(0xf7, 0x9a, 0x63);
pub const RED: Color = Color::from_rgb8(0xe8, 0x5d, 0x75);

pub fn app_theme() -> Theme {
    Theme::custom(
        "Musubi",
        iced::theme::Palette {
            background: BG,
            text: TEXT,
            primary: ACCENT,
            success: GREEN,
            warning: ORANGE,
            danger: RED,
        },
    )
}

pub fn rounded(radius: f32) -> Border {
    Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: radius.into(),
    }
}

pub fn surface(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: rounded(8.0),
        ..container::Style::default()
    }
}

pub fn rail(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        ..container::Style::default()
    }
}

pub fn placeholder(_: &Theme) -> container::Style {
    container::Style {
        background: Some(RAISED.into()),
        border: rounded(6.0),
        ..container::Style::default()
    }
}

pub fn badge(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        text_color: Some(BG),
        border: rounded(4.0),
        ..container::Style::default()
    }
}

pub fn video(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::BLACK.into()),
        ..container::Style::default()
    }
}

pub fn dim(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgba8(0, 0, 0, 0.65).into()),
        ..container::Style::default()
    }
}

/// Dunkelt ein Hintergrundbild nach unten zum Seitenhintergrund ab.
pub fn hero_fade(_: &Theme) -> container::Style {
    let linear = gradient::Linear::new(Radians(std::f32::consts::PI))
        .add_stop(0.0, Color { a: 0.15, ..BG })
        .add_stop(0.55, Color { a: 0.65, ..BG })
        .add_stop(1.0, BG);
    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(linear))),
        ..container::Style::default()
    }
}

pub fn card_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered | button::Status::Pressed => RAISED,
                _ => Color::TRANSPARENT,
            }
            .into(),
        ),
        text_color: TEXT,
        border: rounded(8.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn nav_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(
            match (active, status) {
                (true, _) => RAISED,
                (false, button::Status::Hovered) => Color { a: 0.5, ..RAISED },
                _ => Color::TRANSPARENT,
            }
            .into(),
        ),
        text_color: if active { ACCENT } else { MUTED },
        border: rounded(6.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered => Color { a: 0.85, ..ACCENT },
                button::Status::Disabled => RAISED,
                _ => ACCENT,
            }
            .into(),
        ),
        text_color: BG,
        border: rounded(6.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered => Color { a: 0.7, ..RAISED },
                _ => RAISED,
            }
            .into(),
        ),
        text_color: TEXT,
        border: rounded(6.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered => Color { a: 0.85, ..RED },
                _ => RED,
            }
            .into(),
        ),
        text_color: BG,
        border: rounded(6.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: RAISED.into(),
        border: Border {
            color: if focused { ACCENT } else { Color::TRANSPARENT },
            width: 1.0,
            radius: 6.0.into(),
        },
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: Color { a: 0.4, ..ACCENT },
    }
}

// Player --------------------------------------------------------------------------------

/// Text und Icons über dem Video.
pub const ON_VIDEO: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.94);
pub const ON_VIDEO_MUTED: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.62);

fn white(alpha: f32) -> Color {
    Color {
        a: alpha,
        ..Color::WHITE
    }
}

fn scrim(top: bool) -> container::Style {
    let dark = Color::from_rgba(0.0, 0.0, 0.0, 0.82);
    let mid = Color::from_rgba(0.0, 0.0, 0.0, 0.38);
    let (start, end) = if top {
        (dark, Color::TRANSPARENT)
    } else {
        (Color::TRANSPARENT, dark)
    };
    let linear = gradient::Linear::new(Radians(std::f32::consts::PI))
        .add_stop(0.0, start)
        .add_stop(0.5, mid)
        .add_stop(1.0, end);
    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(linear))),
        ..container::Style::default()
    }
}

/// Verlauf hinter der oberen Leiste des Players.
pub fn scrim_top(_: &Theme) -> container::Style {
    scrim(true)
}

/// Verlauf hinter den Bedienelementen unten.
pub fn scrim_bottom(_: &Theme) -> container::Style {
    scrim(false)
}

/// Runder Icon-Button ohne Fläche, der beim Überfahren eine dezente Fläche bekommt.
pub fn icon_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered => white(0.14),
                button::Status::Pressed => white(0.24),
                _ => Color::TRANSPARENT,
            }
            .into(),
        ),
        text_color: ON_VIDEO,
        border: rounded(999.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Großer runder Wiedergabe-Button in der Bildmitte.
pub fn play_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered | button::Status::Pressed => ACCENT,
                _ => Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            }
            .into(),
        ),
        text_color: ON_VIDEO,
        border: Border {
            color: white(0.22),
            width: 1.0,
            radius: 999.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Hauptaktion mit Icon, abgerundet wie bei Streaming-Diensten.
pub fn primary_pill(theme: &Theme, status: button::Status) -> button::Style {
    let base = primary(theme, status);
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered => Color::WHITE,
                button::Status::Pressed => Color { a: 0.85, ..ACCENT },
                _ => ACCENT,
            }
            .into(),
        ),
        border: rounded(999.0),
        ..base
    }
}

/// Runder Button in der Akzentfarbe, z. B. Wiedergabe in der mpv-Leiste.
pub fn primary_round(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        border: rounded(999.0),
        ..primary(theme, status)
    }
}

/// Icon-Button auf normalem Hintergrund, z. B. auf der Detailseite.
pub fn ghost_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered => RAISED,
                button::Status::Pressed => Color { a: 0.7, ..RAISED },
                _ => Color { a: 0.55, ..SURFACE },
            }
            .into(),
        ),
        text_color: TEXT,
        border: Border {
            color: white(0.08),
            width: 1.0,
            radius: 999.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn track(status: slider::Status, fill: Color, thin: f32, thick: f32) -> slider::Style {
    let engaged = !matches!(status, slider::Status::Active);
    slider::Style {
        rail: slider::Rail {
            backgrounds: (fill.into(), white(0.25).into()),
            width: if engaged { thick } else { thin },
            border: rounded(thick / 2.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle {
                radius: if engaged { thick + 2.0 } else { thin },
            },
            background: if engaged { Color::WHITE } else { fill }.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

/// Fortschrittsleiste: schmal, beim Überfahren dicker mit sichtbarem Griff.
pub fn seek_slider(_: &Theme, status: slider::Status) -> slider::Style {
    track(status, ACCENT, 4.0, 6.0)
}

pub fn volume_slider(_: &Theme, status: slider::Status) -> slider::Style {
    track(status, Color::WHITE, 3.0, 4.0)
}

pub fn tooltip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgba8(0x0a, 0x10, 0x18, 0.94).into()),
        text_color: Some(TEXT),
        border: Border {
            color: white(0.08),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..container::Style::default()
    }
}

/// Leiste der externen mpv-Wiedergabe.
pub fn now_playing(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(0x0f, 0x19, 0x26).into()),
        border: Border {
            color: white(0.06),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}
