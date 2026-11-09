//! Farben und Widget-Stile.

use iced::widget::{button, container, text_input};
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
