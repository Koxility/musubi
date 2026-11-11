//! Widget-Stile, ausschließlich aus den Werten in [`super::tokens`] gebaut.

use iced::overlay::menu;
use iced::widget::{
    button, container, pick_list, progress_bar, scrollable, slider, text_input, toggler,
};
use iced::{Background, Border, Color, Gradient, Radians, Shadow, Theme, Vector, gradient};

use super::tokens::*;

pub fn rounded(radius: f32) -> Border {
    Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: radius.into(),
    }
}

fn outlined(color: Color, radius: f32) -> Border {
    Border {
        color,
        width: 1.0,
        radius: radius.into(),
    }
}

fn white(a: f32) -> Color {
    alpha(Color::WHITE, a)
}

/// Schatten für alles, was über der Seite schwebt: Dialoge, Menüs, Hinweise.
fn floating() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
        offset: Vector::new(0.0, 8.0),
        blur_radius: 28.0,
    }
}

// Flächen -------------------------------------------------------------------------------

/// Ruhige Fläche für Gruppen von Zeilen.
pub fn surface(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: outlined(LINE, R_MD),
        ..container::Style::default()
    }
}

/// Fläche innerhalb einer Fläche, z. B. Kennzahlen.
pub fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(RAISED.into()),
        border: rounded(R_MD),
        ..container::Style::default()
    }
}

pub fn rail(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        ..container::Style::default()
    }
}

/// Haarlinie als ein Pixel hoher oder breiter Container.
pub fn line(_: &Theme) -> container::Style {
    container::Style {
        background: Some(LINE.into()),
        ..container::Style::default()
    }
}

/// Vollfarbige Fläche, z. B. Markierungen und Balken.
pub fn solid(color: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        border: rounded(radius),
        ..container::Style::default()
    }
}

pub fn placeholder(_: &Theme) -> container::Style {
    container::Style {
        background: Some(RAISED.into()),
        border: rounded(R_SM),
        ..container::Style::default()
    }
}

/// Kennzeichen auf Bildern: deckend, damit es auf jedem Poster lesbar bleibt.
pub fn badge(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        text_color: Some(INK),
        border: rounded(R_SM),
        ..container::Style::default()
    }
}

/// Kennzeichen im Text: getönt statt deckend.
pub fn tag(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(alpha(color, 0.14).into()),
        text_color: Some(color),
        border: rounded(R_SM),
        ..container::Style::default()
    }
}

/// Halbtransparenter Streifen über einem Titelbild, links mit Kordelfarbe markiert.
pub fn strip(cord: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(alpha(SURFACE, 0.72).into()),
        border: outlined(alpha(cord, 0.35), R_MD),
        ..container::Style::default()
    }
}

pub fn dialog(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: outlined(LINE_STRONG, R_LG),
        shadow: floating(),
        ..container::Style::default()
    }
}

pub fn toast(ok: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(OVERLAY.into()),
        text_color: Some(PAPER),
        border: outlined(alpha(if ok { TRACK } else { BAD }, 0.55), R_MD),
        shadow: floating(),
        ..container::Style::default()
    }
}

pub fn tooltip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(OVERLAY.into()),
        text_color: Some(PAPER),
        border: outlined(LINE_STRONG, R_SM),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..container::Style::default()
    }
}

pub fn video(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::BLACK.into()),
        ..container::Style::default()
    }
}

/// Dunkelt die Seite hinter einem Dialog ab.
pub fn dim(_: &Theme) -> container::Style {
    container::Style {
        background: Some(alpha(Color::from_rgb8(0x05, 0x08, 0x10), 0.72).into()),
        ..container::Style::default()
    }
}

/// Streifen über einem Bild, auf dem Text steht.
pub fn caption_strip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(alpha(Color::from_rgb8(0x05, 0x08, 0x10), 0.78).into()),
        text_color: Some(PAPER),
        ..container::Style::default()
    }
}

fn vertical(stops: &[(f32, Color)]) -> container::Style {
    let mut linear = gradient::Linear::new(Radians(std::f32::consts::PI));
    for (offset, color) in stops {
        linear = linear.add_stop(*offset, *color);
    }
    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(linear))),
        ..container::Style::default()
    }
}

/// Blendet ein Hintergrundbild nach unten in den Seitenhintergrund über.
pub fn hero_fade(_: &Theme) -> container::Style {
    vertical(&[
        (0.0, alpha(INK, 0.30)),
        (0.45, alpha(INK, 0.55)),
        (0.82, alpha(INK, 0.92)),
        (1.0, INK),
    ])
}

/// Dunkelt die linke Seite eines Hintergrundbilds ab, damit Text darauf lesbar ist.
pub fn hero_side(_: &Theme) -> container::Style {
    let linear = gradient::Linear::new(Radians(std::f32::consts::FRAC_PI_2))
        .add_stop(0.0, alpha(INK, 0.86))
        .add_stop(0.55, alpha(INK, 0.35))
        .add_stop(1.0, alpha(INK, 0.0));
    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(linear))),
        ..container::Style::default()
    }
}

/// Verlauf hinter der oberen Leiste des Players.
pub fn scrim_top(_: &Theme) -> container::Style {
    let dark = Color::from_rgba(0.0, 0.0, 0.0, 0.82);
    vertical(&[
        (0.0, dark),
        (0.5, alpha(Color::BLACK, 0.38)),
        (1.0, Color::TRANSPARENT),
    ])
}

/// Verlauf hinter den Bedienelementen unten.
pub fn scrim_bottom(_: &Theme) -> container::Style {
    let dark = Color::from_rgba(0.0, 0.0, 0.0, 0.82);
    vertical(&[
        (0.0, Color::TRANSPARENT),
        (0.5, alpha(Color::BLACK, 0.38)),
        (1.0, dark),
    ])
}

// Buttons -------------------------------------------------------------------------------

fn flat(background: Color, text_color: Color, border: Border) -> button::Style {
    button::Style {
        background: Some(background.into()),
        text_color,
        border,
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Gefüllter Button in einer Kordelfarbe.
fn filled(color: Color, status: button::Status, radius: f32) -> button::Style {
    match status {
        button::Status::Disabled => flat(RAISED, FAINT, rounded(radius)),
        button::Status::Hovered => flat(mix(color, Color::WHITE, 0.14), INK, rounded(radius)),
        button::Status::Pressed => flat(mix(color, INK, 0.16), INK, rounded(radius)),
        button::Status::Active => flat(color, INK, rounded(radius)),
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a,
    }
}

/// Hauptaktion rund ums Ansehen: Play, Resume.
pub fn watch(_: &Theme, status: button::Status) -> button::Style {
    filled(WATCH, status, R_MD)
}

/// Hauptaktion rund ums Verfolgen: speichern, hinzufügen, abgleichen.
pub fn track(_: &Theme, status: button::Status) -> button::Style {
    filled(TRACK, status, R_MD)
}

pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let border = outlined(LINE_STRONG, R_MD);
    match status {
        button::Status::Hovered => flat(OVERLAY, PAPER, border),
        button::Status::Pressed => flat(RAISED, PAPER, border),
        button::Status::Disabled => flat(Color::TRANSPARENT, FAINT, outlined(LINE, R_MD)),
        button::Status::Active => flat(RAISED, PAPER, border),
    }
}

/// Unauffällige Aktion ohne Fläche.
pub fn quiet(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(RAISED, PAPER, rounded(R_MD)),
        button::Status::Disabled => flat(Color::TRANSPARENT, FAINT, rounded(R_MD)),
        button::Status::Active => flat(Color::TRANSPARENT, MUTED, rounded(R_MD)),
    }
}

pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    let border = outlined(alpha(BAD, 0.5), R_MD);
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(BAD, INK, rounded(R_MD)),
        _ => flat(alpha(BAD, 0.10), BAD, border),
    }
}

/// Klickbare Karte: ohne Fläche, beim Überfahren leicht angehoben.
pub fn card_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(RAISED, PAPER, rounded(R_MD)),
        _ => flat(Color::TRANSPARENT, PAPER, rounded(R_MD)),
    }
}

/// Klickbare Zeile in Listen.
pub fn row_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            flat(alpha(PAPER, 0.05), PAPER, rounded(R_SM))
        }
        _ => flat(Color::TRANSPARENT, PAPER, rounded(R_SM)),
    }
}

/// Eintrag in der Seitenleiste. Die Kordelfarbe zeigt die Gruppe der aktiven Seite.
pub fn nav_button(active: bool, cord: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| match (active, status) {
        (true, _) => flat(alpha(cord, 0.12), PAPER, rounded(R_MD)),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            flat(alpha(PAPER, 0.05), PAPER, rounded(R_MD))
        }
        _ => flat(Color::TRANSPARENT, MUTED, rounded(R_MD)),
    }
}

/// Reiter. Die Unterstreichung zeichnet `ui::tabs`.
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let color = match (active, status) {
            (true, _) => PAPER,
            (false, button::Status::Hovered | button::Status::Pressed) => PAPER,
            _ => MUTED,
        };
        flat(Color::TRANSPARENT, color, rounded(0.0))
    }
}

/// Auswahl-Chip, z. B. Filter.
pub fn chip(active: bool, cord: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| match (active, status) {
        (true, _) => flat(alpha(cord, 0.16), cord, outlined(alpha(cord, 0.5), PILL)),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            flat(RAISED, PAPER, outlined(LINE_STRONG, PILL))
        }
        _ => flat(Color::TRANSPARENT, MUTED, outlined(LINE_STRONG, PILL)),
    }
}

/// Runder Icon-Button ohne Fläche, der beim Überfahren eine dezente Fläche bekommt.
pub fn icon_button(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => white(0.14),
        button::Status::Pressed => white(0.24),
        _ => Color::TRANSPARENT,
    };
    flat(background, ON_VIDEO, rounded(PILL))
}

/// Großer runder Wiedergabe-Button in der Bildmitte.
pub fn play_button(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => WATCH,
        _ => Color::from_rgba(0.0, 0.0, 0.0, 0.45),
    };
    flat(background, ON_VIDEO, outlined(white(0.22), PILL))
}

/// Wiedergabe-Knopf auf einem Vorschaubild.
pub fn play_overlay(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => mix(WATCH, Color::WHITE, 0.14),
        _ => WATCH,
    };
    flat(background, INK, rounded(PILL))
}

/// Hauptaktion mit Icon auf Titelbildern.
pub fn watch_pill(_: &Theme, status: button::Status) -> button::Style {
    filled(WATCH, status, PILL)
}

/// Icon-Button auf normalem Hintergrund, z. B. auf der Detailseite.
pub fn ghost_button(_: &Theme, status: button::Status) -> button::Style {
    let border = outlined(LINE_STRONG, PILL);
    match status {
        button::Status::Hovered => flat(OVERLAY, PAPER, border),
        button::Status::Pressed => flat(RAISED, PAPER, border),
        _ => flat(alpha(SURFACE, 0.6), PAPER, border),
    }
}

// Eingaben ------------------------------------------------------------------------------

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let border = match status {
        text_input::Status::Focused { .. } => TRACK,
        text_input::Status::Hovered => alpha(PAPER, 0.28),
        _ => LINE_STRONG,
    };
    text_input::Style {
        background: INK.into(),
        border: outlined(border, R_MD),
        icon: MUTED,
        placeholder: FAINT,
        value: PAPER,
        selection: alpha(TRACK, 0.35),
    }
}

pub fn pick(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    let (background, border) = match status {
        pick_list::Status::Active => (RAISED, LINE_STRONG),
        pick_list::Status::Hovered => (OVERLAY, LINE_STRONG),
        pick_list::Status::Opened { .. } => (OVERLAY, TRACK),
    };
    pick_list::Style {
        text_color: PAPER,
        placeholder_color: FAINT,
        handle_color: MUTED,
        background: background.into(),
        border: outlined(border, R_MD),
    }
}

pub fn pick_menu(_: &Theme) -> menu::Style {
    menu::Style {
        background: OVERLAY.into(),
        border: outlined(LINE_STRONG, R_MD),
        text_color: PAPER,
        selected_text_color: INK,
        selected_background: TRACK.into(),
        shadow: floating(),
    }
}

pub fn toggle(_: &Theme, status: toggler::Status) -> toggler::Style {
    let (on, hovered) = match status {
        toggler::Status::Active { is_toggled } => (is_toggled, false),
        toggler::Status::Hovered { is_toggled } => (is_toggled, true),
        toggler::Status::Disabled { is_toggled } => (is_toggled, false),
    };
    let background = match (on, hovered) {
        (true, false) => TRACK,
        (true, true) => mix(TRACK, Color::WHITE, 0.14),
        (false, false) => OVERLAY,
        (false, true) => mix(OVERLAY, PAPER, 0.12),
    };
    toggler::Style {
        background: background.into(),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: if on { INK } else { MUTED }.into(),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(PAPER),
        border_radius: None,
        padding_ratio: 0.18,
    }
}

/// Schmale Scrollleiste, die erst beim Überfahren deutlich wird.
pub fn scroll(_: &Theme, status: scrollable::Status) -> scrollable::Style {
    let (engaged, idle) = match status {
        scrollable::Status::Active {
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_disabled,
        } => (
            false,
            is_horizontal_scrollbar_disabled && is_vertical_scrollbar_disabled,
        ),
        scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_disabled,
        } => (
            is_horizontal_scrollbar_hovered || is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_disabled && is_vertical_scrollbar_disabled,
        ),
        scrollable::Status::Dragged { .. } => (true, false),
    };
    // Passt der Inhalt ganz in den Bereich, bleibt die Leiste unsichtbar.
    let strength = match (idle, engaged) {
        (true, _) => 0.0,
        (false, true) => 0.38,
        (false, false) => 0.14,
    };
    let rail = scrollable::Rail {
        background: None,
        border: rounded(PILL),
        scroller: scrollable::Scroller {
            background: alpha(PAPER, strength).into(),
            border: rounded(PILL),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: OVERLAY.into(),
            border: outlined(LINE_STRONG, PILL),
            shadow: Shadow::default(),
            icon: PAPER,
        },
    }
}

/// Fortschritt in einer Kordelfarbe auf dunkler Spur.
pub fn progress(color: Color) -> impl Fn(&Theme) -> progress_bar::Style {
    move |_| progress_bar::Style {
        background: alpha(Color::from_rgb8(0x05, 0x08, 0x10), 0.6).into(),
        bar: color.into(),
        border: rounded(0.0),
    }
}

fn slider_track(status: slider::Status, fill: Color, thin: f32, thick: f32) -> slider::Style {
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
    slider_track(status, WATCH, 4.0, 6.0)
}

pub fn volume_slider(_: &Theme, status: slider::Status) -> slider::Style {
    slider_track(status, Color::WHITE, 3.0, 4.0)
}

/// Regler in Formularen.
pub fn form_slider(_: &Theme, _: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (TRACK.into(), OVERLAY.into()),
            width: 4.0,
            border: rounded(2.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: PAPER.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

// Übergangsnamen, bis alle Seiten umgestellt sind -----------------------------------------

pub const BG: Color = INK;
pub const ACCENT: Color = TRACK;
pub const TEXT: Color = PAPER;
pub const GREEN: Color = OK;
pub const ORANGE: Color = WARN;
pub const RED: Color = BAD;

pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    track(theme, status)
}

pub fn primary_pill(theme: &Theme, status: button::Status) -> button::Style {
    watch_pill(theme, status)
}
