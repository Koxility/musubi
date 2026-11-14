//! Widget styles, built from the values in [`super::tokens`] and nothing else.
//!
//! The look follows Kura: flat surfaces one step apart, hairline borders, no brand colour.
//! A primary action is light text on a dark page turned around; everything else is
//! outlined or bare. The cord colours only appear where a style takes one as a parameter
//! or marks watching or tracking itself.

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

/// Shadow for what floats above the page: dialogs, menus, notices.
fn floating() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
        offset: Vector::new(0.0, 8.0),
        blur_radius: 24.0,
    }
}

// Surfaces ------------------------------------------------------------------------------

/// Quiet surface for groups of rows.
pub fn surface(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: outlined(LINE, RADIUS),
        ..container::Style::default()
    }
}

/// Surface inside a surface, e.g. key figures.
pub fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(HOVER.into()),
        border: rounded(RADIUS),
        ..container::Style::default()
    }
}

pub fn rail(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        ..container::Style::default()
    }
}

/// Hairline, as a container one pixel high or wide.
pub fn line(_: &Theme) -> container::Style {
    container::Style {
        background: Some(LINE.into()),
        ..container::Style::default()
    }
}

/// Solid area, e.g. marks and bars.
pub fn solid(color: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        border: rounded(radius),
        ..container::Style::default()
    }
}

pub fn placeholder(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: rounded(R_COVER),
        ..container::Style::default()
    }
}

/// Mark on a picture: opaque, so it stays readable on any poster.
pub fn badge(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        text_color: Some(BG),
        border: rounded(PILL),
        ..container::Style::default()
    }
}

/// Mark in text: an outlined pill, tinted with its colour.
pub fn tag(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(alpha(color, 0.10).into()),
        text_color: Some(color),
        border: outlined(alpha(color, 0.35), PILL),
        ..container::Style::default()
    }
}

/// Half-transparent strip over a backdrop, edged in a cord colour.
pub fn strip(cord: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(alpha(SURFACE, 0.78).into()),
        border: outlined(alpha(cord, 0.35), RADIUS),
        ..container::Style::default()
    }
}

pub fn dialog(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: outlined(LINE, R_PANEL),
        shadow: floating(),
        ..container::Style::default()
    }
}

pub fn toast(ok: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(HOVER.into()),
        text_color: Some(TEXT),
        border: outlined(if ok { LINE } else { alpha(BAD, 0.55) }, RADIUS),
        shadow: floating(),
        ..container::Style::default()
    }
}

pub fn tooltip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(HOVER.into()),
        text_color: Some(TEXT),
        border: outlined(LINE, R_COVER),
        ..container::Style::default()
    }
}

pub fn video(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::BLACK.into()),
        ..container::Style::default()
    }
}

/// The page background at `opacity`, laid over a view that is fading in.
pub fn veil(opacity: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: (opacity > 0.0).then(|| alpha(BG, opacity).into()),
        ..container::Style::default()
    }
}

/// Darkens the page behind a dialog.
pub fn dim(_: &Theme) -> container::Style {
    container::Style {
        background: Some(alpha(Color::BLACK, 0.72).into()),
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

/// Fades a backdrop downwards into the page background.
pub fn hero_fade(_: &Theme) -> container::Style {
    vertical(&[
        (0.0, alpha(BG, 0.30)),
        (0.45, alpha(BG, 0.55)),
        (0.82, alpha(BG, 0.92)),
        (1.0, BG),
    ])
}

/// Fades only the lower part of the Home hero into the page, so the picture itself stays
/// clear and the text at the bottom stays readable.
pub fn hero_floor(_: &Theme) -> container::Style {
    vertical(&[
        (0.0, alpha(BG, 0.18)),
        (0.4, alpha(BG, 0.06)),
        (0.68, alpha(BG, 0.52)),
        (0.9, alpha(BG, 0.94)),
        (1.0, BG),
    ])
}

/// Darkens the left side of a backdrop, so text on it stays readable.
pub fn hero_side(_: &Theme) -> container::Style {
    let linear = gradient::Linear::new(Radians(std::f32::consts::FRAC_PI_2))
        .add_stop(0.0, alpha(BG, 0.86))
        .add_stop(0.55, alpha(BG, 0.35))
        .add_stop(1.0, alpha(BG, 0.0));
    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(linear))),
        ..container::Style::default()
    }
}

/// Gradient behind the player's top bar.
pub fn scrim_top(_: &Theme) -> container::Style {
    vertical(&[
        (0.0, alpha(Color::BLACK, 0.82)),
        (0.5, alpha(Color::BLACK, 0.38)),
        (1.0, Color::TRANSPARENT),
    ])
}

/// Gradient behind the controls at the bottom.
pub fn scrim_bottom(_: &Theme) -> container::Style {
    vertical(&[
        (0.0, Color::TRANSPARENT),
        (0.5, alpha(Color::BLACK, 0.38)),
        (1.0, alpha(Color::BLACK, 0.82)),
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

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a,
    }
}

/// How much a disabled control keeps of itself.
const DISABLED: f32 = 0.4;

/// The one filled button: the text colour as the surface, the page colour as the text.
fn inverted(status: button::Status, radius: f32) -> button::Style {
    match status {
        button::Status::Disabled => flat(
            alpha(TEXT, DISABLED),
            BG,
            outlined(Color::TRANSPARENT, radius),
        ),
        button::Status::Hovered | button::Status::Pressed => flat(
            mix(TEXT, BG, 0.14),
            BG,
            outlined(mix(TEXT, BG, 0.14), radius),
        ),
        button::Status::Active => flat(TEXT, BG, outlined(TEXT, radius)),
    }
}

/// The main action of a view. There is one look for it, whichever service it belongs to.
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    inverted(status, RADIUS)
}

pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let border = outlined(LINE, RADIUS);
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(HOVER, TEXT, border),
        button::Status::Disabled => flat(Color::TRANSPARENT, alpha(TEXT, DISABLED), border),
        button::Status::Active => flat(Color::TRANSPARENT, TEXT, border),
    }
}

/// Unobtrusive action without a surface.
pub fn quiet(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(HOVER, TEXT, rounded(RADIUS)),
        button::Status::Disabled => {
            flat(Color::TRANSPARENT, alpha(TEXT_2, DISABLED), rounded(RADIUS))
        }
        button::Status::Active => flat(Color::TRANSPARENT, TEXT_2, rounded(RADIUS)),
    }
}

pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    let border = outlined(alpha(BAD, 0.45), RADIUS);
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(alpha(BAD, 0.14), BAD, border),
        button::Status::Disabled => flat(Color::TRANSPARENT, alpha(BAD, DISABLED), border),
        button::Status::Active => flat(Color::TRANSPARENT, BAD, border),
    }
}

/// An available update: the one place the info blue fills a button. Disabled, it is as
/// quiet as any other disabled button, so "nothing to do" does not draw the eye.
pub fn info(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Disabled => flat(
            Color::TRANSPARENT,
            alpha(TEXT, DISABLED),
            outlined(LINE, RADIUS),
        ),
        button::Status::Hovered | button::Status::Pressed => flat(
            mix(INFO, Color::WHITE, 0.12),
            BG,
            outlined(mix(INFO, Color::WHITE, 0.12), RADIUS),
        ),
        button::Status::Active => flat(INFO, BG, outlined(INFO, RADIUS)),
    }
}

/// The small "Update" pill under the version: tinted, not filled, as in Kura.
pub fn update_badge(_: &Theme, status: button::Status) -> button::Style {
    let fill = match status {
        button::Status::Hovered | button::Status::Pressed => 0.24,
        _ => 0.14,
    };
    flat(alpha(INFO, fill), INFO, outlined(alpha(INFO, 0.45), PILL))
}

/// Clickable card: no surface, lifted slightly on hover.
pub fn card_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(HOVER, TEXT, rounded(RADIUS)),
        _ => flat(Color::TRANSPARENT, TEXT, rounded(RADIUS)),
    }
}

/// Clickable row in lists.
pub fn row_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(HOVER, TEXT, rounded(RADIUS)),
        _ => flat(Color::TRANSPARENT, TEXT, rounded(RADIUS)),
    }
}

/// Entry in the sidebar.
pub fn nav_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| match (active, status) {
        (true, _) => flat(HOVER, TEXT, rounded(RADIUS)),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            flat(Color::TRANSPARENT, TEXT, rounded(RADIUS))
        }
        _ => flat(Color::TRANSPARENT, TEXT_2, rounded(RADIUS)),
    }
}

/// Tab. `ui::tabs` draws the underline.
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let color = match (active, status) {
            (true, _) => TEXT,
            (false, button::Status::Hovered | button::Status::Pressed) => TEXT,
            _ => TEXT_2,
        };
        flat(Color::TRANSPARENT, color, rounded(0.0))
    }
}

/// Choice chip, e.g. a filter. The active one carries its cord colour on the edge.
pub fn chip(active: bool, cord: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| match (active, status) {
        (true, _) => flat(HOVER, TEXT, outlined(alpha(cord, 0.6), PILL)),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            flat(HOVER, TEXT, outlined(LINE, PILL))
        }
        _ => flat(Color::TRANSPARENT, TEXT_2, outlined(LINE, PILL)),
    }
}

/// Round icon button over the video, with a faint surface on hover.
pub fn icon_button(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => white(0.14),
        button::Status::Pressed => white(0.24),
        _ => Color::TRANSPARENT,
    };
    flat(background, ON_VIDEO, rounded(PILL))
}

/// Large round play button in the middle of the picture.
pub fn play_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(TEXT, BG, outlined(TEXT, PILL)),
        _ => flat(
            alpha(Color::BLACK, 0.45),
            ON_VIDEO,
            outlined(white(0.22), PILL),
        ),
    }
}

/// Play button on a thumbnail.
pub fn play_overlay(_: &Theme, status: button::Status) -> button::Style {
    inverted(status, PILL)
}

/// Main action with an icon on backdrops.
pub fn primary_pill(_: &Theme, status: button::Status) -> button::Style {
    inverted(status, PILL)
}

/// Icon button on the normal background, e.g. on the detail page.
pub fn ghost_button(_: &Theme, status: button::Status) -> button::Style {
    let border = outlined(LINE, PILL);
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(HOVER, TEXT, border),
        _ => flat(alpha(SURFACE, 0.7), TEXT, border),
    }
}

/// The "+1" action on covers and rows: solid teal with a dark ring, so it reads on any
/// artwork.
pub fn count_up(_: &Theme, status: button::Status) -> button::Style {
    let ring = Border {
        color: alpha(BG, 0.78),
        width: 2.0,
        radius: PILL.into(),
    };
    let (background, text_color) = match status {
        button::Status::Disabled => (HOVER, TEXT_3),
        button::Status::Hovered => (mix(TRACK, Color::WHITE, 0.22), BG),
        button::Status::Pressed => (mix(TRACK, BG, 0.16), BG),
        button::Status::Active => (TRACK, BG),
    };
    flat(background, text_color, ring)
}

/// Frame around a segmented control such as the view switch.
pub fn segmented(_: &Theme) -> container::Style {
    container::Style {
        border: outlined(LINE, RADIUS + 3.0),
        ..container::Style::default()
    }
}

/// One option of a segmented control.
pub fn segment(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| match (active, status) {
        (true, _) => flat(HOVER, TEXT, rounded(RADIUS)),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            flat(Color::TRANSPARENT, TEXT, rounded(RADIUS))
        }
        _ => flat(Color::TRANSPARENT, TEXT_2, rounded(RADIUS)),
    }
}

// Inputs --------------------------------------------------------------------------------

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let border = match status {
        text_input::Status::Focused { .. } | text_input::Status::Hovered => TEXT_3,
        _ => LINE,
    };
    text_input::Style {
        background: SURFACE.into(),
        border: outlined(border, RADIUS),
        icon: TEXT_2,
        placeholder: TEXT_3,
        value: TEXT,
        selection: alpha(TEXT, 0.22),
    }
}

pub fn pick(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    let border = match status {
        pick_list::Status::Active => LINE,
        pick_list::Status::Hovered | pick_list::Status::Opened { .. } => TEXT_3,
    };
    pick_list::Style {
        text_color: TEXT,
        placeholder_color: TEXT_3,
        handle_color: TEXT_2,
        background: SURFACE.into(),
        border: outlined(border, RADIUS),
    }
}

pub fn pick_menu(_: &Theme) -> menu::Style {
    menu::Style {
        background: SURFACE.into(),
        border: outlined(LINE, RADIUS),
        text_color: TEXT_2,
        selected_text_color: TEXT,
        selected_background: HOVER.into(),
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
        (true, false) => TEXT,
        (true, true) => mix(TEXT, BG, 0.14),
        (false, false) => LINE,
        (false, true) => mix(LINE, TEXT, 0.12),
    };
    toggler::Style {
        background: background.into(),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: if on { BG } else { TEXT_2 }.into(),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(TEXT),
        border_radius: None,
        padding_ratio: 0.18,
    }
}

/// Narrow scrollbar that only becomes distinct on hover.
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
    // When the content fits, the bar stays invisible.
    let strength = match (idle, engaged) {
        (true, _) => 0.0,
        (false, true) => 0.38,
        (false, false) => 0.14,
    };
    let rail = scrollable::Rail {
        background: None,
        border: rounded(PILL),
        scroller: scrollable::Scroller {
            background: alpha(TEXT, strength).into(),
            border: rounded(PILL),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: HOVER.into(),
            border: outlined(LINE, PILL),
            shadow: Shadow::default(),
            icon: TEXT,
        },
    }
}

/// Progress in a cord colour on a dark track.
pub fn progress(color: Color) -> impl Fn(&Theme) -> progress_bar::Style {
    move |_| progress_bar::Style {
        background: LINE.into(),
        bar: color.into(),
        border: rounded(PILL),
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

/// Seek bar: narrow, thicker with a visible handle on hover.
pub fn seek_slider(_: &Theme, status: slider::Status) -> slider::Style {
    slider_track(status, WATCH, 4.0, 6.0)
}

pub fn volume_slider(_: &Theme, status: slider::Status) -> slider::Style {
    slider_track(status, Color::WHITE, 3.0, 4.0)
}

/// Slider in forms.
pub fn form_slider(_: &Theme, _: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (TEXT.into(), LINE.into()),
            width: 4.0,
            border: rounded(2.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: TEXT.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}
