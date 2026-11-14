//! Wiederverwendbare Bausteine der Oberfläche.

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::tooltip::Position;
use iced::widget::{
    Column, Row, Space, button, center, column, container, image, mouse_area, opaque, pick_list,
    progress_bar, row, scrollable, sensor, space, stack, svg, text, text_input, toggler, tooltip,
};
use iced::{Center, Color, ContentFit, Element, Fill, FillPortion, Length, Padding, Task};

use musubi_core::{ViewConfig, ViewMode, ViewSize};

use crate::icons::Icon;
use crate::images::{ImageCache, ImageKey};
use crate::theme::{self, S1, S2, S3, S4, S5, S6};

pub use crate::theme::SEMIBOLD;

/// Button-Stil als Funktion, wie ihn `button::style` erwartet.
pub type ButtonStyle = fn(&iced::Theme, button::Status) -> button::Style;

/// Seitenrand, den alle Seiten ohne Titelbild teilen.
pub const PAGE_PADDING: f32 = S6;

// Icons ---------------------------------------------------------------------------------

/// Ein Icon in fester Größe und Farbe.
pub fn icon<'a, M: 'a>(icon: Icon, size: f32, color: Color) -> Element<'a, M> {
    svg(icon.handle())
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

/// Attaches a short hint that shows on hover.
pub fn with_tip<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    label: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    tooltip(
        content,
        container(text(label).size(theme::T_LABEL))
            .padding([5, 9])
            .style(theme::tooltip),
        Position::Top,
    )
    .gap(8)
    .into()
}

/// Runder Icon-Button. Ohne Nachricht ist er sichtbar abgeschwächt und nicht klickbar.
pub fn icon_button<'a, M: Clone + 'a>(
    glyph: Icon,
    size: f32,
    color: Color,
    message: Option<M>,
    label: &'a str,
    style: ButtonStyle,
) -> Element<'a, M> {
    let tint = if message.is_some() {
        color
    } else {
        theme::alpha(color, color.a * 0.35)
    };
    let side = size * 1.8;
    let content = button(center(icon(glyph, size, tint)).width(side).height(side))
        .padding(0)
        .style(style)
        .on_press_maybe(message);
    with_tip(content, label)
}

/// Button mit Icon und Beschriftung, z. B. „Play“ auf der Detailseite.
pub fn icon_label<'a, M: Clone + 'a>(
    glyph: Icon,
    label: &'a str,
    message: M,
    style: ButtonStyle,
    color: Color,
) -> Element<'a, M> {
    button(
        row![
            icon(glyph, 18.0, color),
            text(label).size(theme::T_BODY).font(SEMIBOLD).color(color)
        ]
        .spacing(S2)
        .align_y(Center),
    )
    .padding([10, 22])
    .style(style)
    .on_press(message)
    .into()
}

/// The main action on the Home hero: a large light pill.
pub fn hero_action<'a, M: Clone + 'a>(glyph: Icon, label: &'a str, message: M) -> Element<'a, M> {
    button(
        row![
            icon(glyph, 22.0, theme::BG),
            text(label)
                .size(theme::T_TITLE)
                .font(SEMIBOLD)
                .color(theme::BG)
        ]
        .spacing(S2)
        .align_y(Center),
    )
    .padding([13, 30])
    .style(theme::primary_pill)
    .on_press(message)
    .into()
}

/// Kleiner Button mit Icon und Beschriftung für Werkzeugleisten.
pub fn tool<'a, M: Clone + 'a>(
    glyph: Icon,
    label: &'a str,
    message: Option<M>,
    style: ButtonStyle,
    color: Color,
) -> Element<'a, M> {
    button(
        row![
            icon(glyph, 15.0, color),
            text(label).size(theme::T_SMALL).font(theme::MEDIUM)
        ]
        .spacing(6)
        .align_y(Center),
    )
    .padding([7, 12])
    .style(style)
    .on_press_maybe(message)
    .into()
}

/// The "+1" action of a list entry. Always solid and labelled, so it is easy to spot and
/// to hit on top of a cover. Without a message it is visibly disabled.
pub fn plus_one<'a, M: Clone + 'a>(message: Option<M>, tip: &'a str) -> Element<'a, M> {
    let color = if message.is_some() {
        theme::BG
    } else {
        theme::TEXT_3
    };
    let content = button(
        row![
            icon(Icon::Plus, 14.0, color),
            text("1").size(theme::T_BODY).font(SEMIBOLD).color(color)
        ]
        .spacing(2)
        .align_y(Center),
    )
    .padding([4.0, 11.0])
    .style(theme::count_up)
    .on_press_maybe(message);
    with_tip(content, tip)
}

/// Switch between grid and list and between the size presets. `modes` is false where a
/// page only has one layout.
pub fn view_controls<'a, M: Clone + 'a>(
    view: ViewConfig,
    modes: bool,
    on_change: impl Fn(ViewConfig) -> M + 'a,
) -> Element<'a, M> {
    let frame = |options: Vec<Element<'a, M>>| {
        container(row(options).spacing(2))
            .padding(3)
            .style(theme::segmented)
    };
    let segment = |content: Element<'a, M>, active: bool, message: M, tip: &'a str| {
        with_tip(
            button(center(content).width(30).height(26))
                .padding(0)
                .style(theme::segment(active))
                .on_press(message),
            tip,
        )
    };
    let mut controls = Row::new().spacing(S2).align_y(Center);
    if modes {
        let mode = |glyph, value: ViewMode, tip| {
            let active = view.mode == value;
            let color = if active { theme::TEXT } else { theme::TEXT_2 };
            segment(
                icon(glyph, 16.0, color),
                active,
                on_change(ViewConfig {
                    mode: value,
                    ..view
                }),
                tip,
            )
        };
        controls = controls.push(frame(vec![
            mode(Icon::Grid, ViewMode::Grid, "Grid view"),
            mode(Icon::List, ViewMode::List, "List view"),
        ]));
    }
    let sizes = ViewSize::ALL
        .into_iter()
        .map(|value| {
            let (letter, tip) = match value {
                ViewSize::Small => ("S", "Small"),
                ViewSize::Medium => ("M", "Medium"),
                ViewSize::Large => ("L", "Large"),
            };
            let active = view.size == value;
            segment(
                text(letter)
                    .size(theme::T_LABEL)
                    .font(if active { SEMIBOLD } else { theme::MEDIUM })
                    .into(),
                active,
                on_change(ViewConfig {
                    size: value,
                    ..view
                }),
                tip,
            )
        })
        .collect();
    controls.push(frame(sizes)).into()
}

// Texthilfen ----------------------------------------------------------------------------

/// Teilt Wiedergabetitel wie „Serie S01E02 - Name“ in Serie und Folge.
pub fn split_title(title: &str) -> (&str, Option<String>) {
    let found = title.char_indices().find_map(|(i, c)| {
        let rest = &title[i..];
        let code = rest.get(..7)?;
        let bytes = code.as_bytes();
        let shape = c == 'S'
            && bytes[1..3].iter().all(u8::is_ascii_digit)
            && bytes[3] == b'E'
            && bytes[4..6].iter().all(u8::is_ascii_digit);
        (shape && i > 0 && title[..i].ends_with(' ')).then_some(i)
    });
    let Some(at) = found else {
        return (title, None);
    };
    let series = title[..at].trim_end();
    let code = &title[at..];
    let season: u32 = code[1..3].parse().unwrap_or(0);
    let episode: u32 = code[4..6].trim_start_matches('0').parse().unwrap_or(0);
    let name = code
        .get(6..)
        .map(|r| r.trim_start_matches(|c: char| c.is_ascii_digit()))
        .map(|r| r.trim_start_matches(" - ").trim())
        .unwrap_or("");
    let label = if name.is_empty() {
        format!("S{season}:E{episode}")
    } else {
        format!("S{season}:E{episode} · {name}")
    };
    (series, Some(label))
}

/// Kürzt Text mit Auslassungszeichen, weil Iced Text nicht selbst abschneidet.
pub fn ellipsize(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_owned()
    } else {
        let mut short: String = value.chars().take(max.saturating_sub(1)).collect();
        short.push('…');
        short
    }
}

/// Zeit bis zu einem Ereignis, z. B. `2d 3h` oder `12m`.
pub fn countdown(seconds: i64) -> String {
    if seconds <= 0 {
        return "airing".to_owned();
    }
    let (days, hours, minutes) = (
        seconds / 86_400,
        seconds % 86_400 / 3_600,
        seconds % 3_600 / 60,
    );
    match (days, hours) {
        (0, 0) => format!("{}m", minutes.max(1)),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}

pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

// Text ----------------------------------------------------------------------------------

/// Seitentitel.
pub fn page_title<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label)
        .size(theme::T_PAGE)
        .font(theme::DISPLAY)
        .color(theme::TEXT)
        .into()
}

/// Überschrift eines Abschnitts.
pub fn heading<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label)
        .size(theme::T_TITLE)
        .font(theme::DISPLAY)
        .color(theme::TEXT)
        .into()
}

/// Überschrift mit einer Zahl dahinter, z. B. „Watching 12“.
pub fn heading_count<'a, M: 'a>(
    label: impl text::IntoFragment<'a>,
    count: usize,
) -> Element<'a, M> {
    row![
        heading(label),
        text(count.to_string())
            .size(theme::T_SMALL)
            .font(theme::MEDIUM)
            .color(theme::TEXT_3),
    ]
    .spacing(S2)
    .align_y(iced::Bottom)
    .into()
}

pub fn muted<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(theme::T_SMALL).color(theme::TEXT_2).into()
}

/// Beschriftung über einem Feld oder einer Kennzahl.
pub fn label<'a, M: 'a>(value: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(value)
        .size(theme::T_LABEL)
        .font(theme::MEDIUM)
        .color(theme::TEXT_2)
        .into()
}

pub fn error_line<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(theme::T_SMALL).color(theme::BAD).into()
}

// Buttons -------------------------------------------------------------------------------

fn text_button<'a, M: Clone + 'a>(
    label: &'a str,
    message: M,
    style: ButtonStyle,
) -> Element<'a, M> {
    button(text(label).size(theme::T_SMALL).font(theme::MEDIUM))
        .padding([S2, 14.0])
        .style(style)
        .on_press(message)
        .into()
}

/// The main action of a view or dialog.
pub fn primary<'a, M: Clone + 'a>(label: &'a str, message: M) -> Element<'a, M> {
    text_button(label, message, theme::primary)
}

pub fn secondary<'a, M: Clone + 'a>(label: &'a str, message: M) -> Element<'a, M> {
    text_button(label, message, theme::secondary)
}

pub fn quiet<'a, M: Clone + 'a>(label: &'a str, message: M) -> Element<'a, M> {
    text_button(label, message, theme::quiet)
}

pub fn danger<'a, M: Clone + 'a>(label: &'a str, message: M) -> Element<'a, M> {
    text_button(label, message, theme::danger)
}

/// Choice chip; the active one is edged in the cord colour.
pub fn chip<'a, M: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    active: bool,
    message: M,
    cord: Color,
) -> Element<'a, M> {
    button(text(label).size(theme::T_SMALL).font(theme::MEDIUM))
        .padding([5, 12])
        .style(theme::chip(active, cord))
        .on_press(message)
        .into()
}

/// Tabs, the active one underlined in the cord colour.
pub fn tabs<'a, M: Clone + 'a>(
    items: impl IntoIterator<Item = (String, bool, M)>,
    cord: Color,
) -> Element<'a, M> {
    let tabs = items.into_iter().map(|(label, active, message)| {
        let marker = if active { cord } else { Color::TRANSPARENT };
        button(
            column![
                text(label)
                    .size(theme::T_BODY)
                    .font(if active { SEMIBOLD } else { theme::MEDIUM }),
                container(Space::new())
                    .width(Fill)
                    .height(2)
                    .style(theme::solid(marker, 1.0)),
            ]
            .spacing(7)
            .width(Length::Shrink),
        )
        .padding(iced::padding::top(6))
        .style(theme::tab(active))
        .on_press(message)
        .into()
    });
    row(tabs).spacing(S5).into()
}

// Inputs --------------------------------------------------------------------------------

pub fn field<'a, M: Clone + 'a>(
    name: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    column![
        label(name),
        text_input("", value)
            .on_input(on_input)
            .size(theme::T_BODY)
            .padding([10, 12])
            .style(theme::input),
    ]
    .spacing(6)
    .into()
}

pub fn secret_field<'a, M: Clone + 'a>(
    name: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> M + 'a,
    on_submit: Option<M>,
) -> Element<'a, M> {
    column![
        label(name),
        text_input("", value)
            .secure(true)
            .on_input(on_input)
            .on_submit_maybe(on_submit)
            .size(theme::T_BODY)
            .padding([10, 12])
            .style(theme::input),
    ]
    .spacing(6)
    .into()
}

/// Pick list in the app's style.
pub fn pick<'a, M: Clone + 'a>(
    options: Vec<String>,
    selected: Option<String>,
    on_select: impl Fn(String) -> M + 'a,
) -> iced::widget::PickList<'a, String, Vec<String>, String, M> {
    pick_list(options, selected, on_select)
        .text_size(theme::T_SMALL)
        .padding([7, 12])
        .style(theme::pick)
        .menu_style(theme::pick_menu)
}

/// Schalter mit Beschriftung.
pub fn toggle<'a, M: 'a>(
    name: &'a str,
    value: bool,
    on_toggle: impl Fn(bool) -> M + 'a,
) -> Element<'a, M> {
    toggler(value)
        .label(name)
        .size(18)
        .spacing(S3)
        .text_size(theme::T_BODY)
        .style(theme::toggle)
        .on_toggle(on_toggle)
        .into()
}

// Bilder und Fortschritt ----------------------------------------------------------------

/// Picture with a placeholder, loaded when it first scrolls into view.
pub fn picture<'a, M: Clone + 'a>(
    images: &'a ImageCache,
    key: Option<ImageKey>,
    width: impl Into<Length>,
    height: impl Into<Length>,
    want: fn(ImageKey) -> M,
) -> Element<'a, M> {
    let (width, height) = (width.into(), height.into());
    let placeholder = || -> Element<'a, M> {
        container(Space::new())
            .width(width)
            .height(height)
            .style(theme::placeholder)
            .into()
    };
    let Some(key) = key else {
        return placeholder();
    };
    let content: Element<'a, M> = match images.get(&key) {
        Some(handle) => {
            let picture = image(handle.clone())
                .opacity(images.opacity(&key))
                .width(width)
                .height(height)
                .content_fit(ContentFit::Cover)
                .border_radius(theme::R_COVER);
            if width == Length::Fill {
                // A picture that fills the width is cropped to its frame, often by a lot.
                // The software renderer ignores the image's own clip, so the part outside
                // the frame would cover what follows. The upper layer of a clipping stack
                // is a real clip in every renderer.
                stack![Space::new().width(width).height(height), picture]
                    .clip(true)
                    .into()
            } else {
                container(picture)
                    .width(width)
                    .height(height)
                    .clip(true)
                    .into()
            }
        }
        None => placeholder(),
    };
    sensor(content)
        .key(key.clone())
        .anticipate(600)
        .on_show(move |_| want(key.clone()))
        .into()
}

/// Dünner Fortschrittsbalken in einer Kordelfarbe.
pub fn progress<'a, M: 'a>(value: f32, width: impl Into<Length>, color: Color) -> Element<'a, M> {
    progress_bar(0.0..=1.0, value.clamp(0.0, 1.0))
        .length(width)
        .girth(3)
        .style(theme::progress(color))
        .into()
}

/// Legt einen Fortschrittsbalken an den unteren Rand eines Bildes.
pub fn with_progress<'a, M: 'a>(
    picture: Element<'a, M>,
    value: Option<f32>,
    size: (f32, f32),
    color: Color,
) -> Element<'a, M> {
    match value {
        Some(value) => stack![
            picture,
            iced::widget::bottom(progress(value, size.0, color))
                .width(size.0)
                .height(size.1)
        ]
        .into(),
        None => picture,
    }
}

// Aufbau --------------------------------------------------------------------------------

/// Senkrecht scrollender Bereich mit der schmalen Leiste der App.
pub fn scroll<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    scrollable(content)
        .width(Fill)
        .height(Fill)
        .style(theme::scroll)
        .into()
}

/// Kopf einer Seite: Titel, kurze Erklärung und Aktionen rechts.
pub fn page_header<'a, M: 'a>(
    title: &'a str,
    hint: Option<String>,
    actions: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let mut titles = column![page_title(title)].spacing(S1);
    if let Some(hint) = hint {
        titles = titles.push(container(muted(hint)).max_width(640));
    }
    row![titles, spacer(), row(actions).spacing(S2).align_y(Center)]
        .align_y(Center)
        .spacing(S4)
        .into()
}

/// Abschnitt aus Überschrift und Inhalt.
pub fn section<'a, M: 'a>(
    title: Element<'a, M>,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    column![title, content.into()].spacing(S3).into()
}

/// Horizontal row of cards. It scrolls by dragging the cards, with a touchpad swipe, Shift
/// and the mouse wheel, the thin bar below the cards, and the arrows in its header, because
/// a plain mouse wheel only scrolls the page. `on_hover` reports the shelf under the pointer;
/// the page needs it to know which shelf a drag belongs to.
pub fn shelf<'a, M: Clone + 'a>(
    title: &'a str,
    id: &'static str,
    items: Vec<Element<'a, M>>,
    padding: Padding,
    card_inset: f32,
    on_scroll: fn(&'static str, f32) -> M,
    on_hover: fn(Option<&'static str>) -> M,
) -> Element<'a, M> {
    let count = items.len();
    // Cards have their own inset; their picture should still line up with the heading.
    let cards = Padding {
        left: padding.left - card_inset,
        bottom: S2,
        ..padding
    };
    let arrow = |glyph, label, step: f32| {
        icon_button(
            glyph,
            17.0,
            theme::TEXT,
            Some(on_scroll(id, step)),
            label,
            theme::ghost_button,
        )
    };
    let heading = row![
        text(title)
            .size(theme::T_SECTION)
            .font(theme::DISPLAY)
            .color(theme::TEXT),
        text(count.to_string())
            .size(theme::T_BODY)
            .font(theme::MEDIUM)
            .color(theme::TEXT_3),
    ]
    .spacing(S2)
    .align_y(iced::Bottom);
    let bar = Scrollbar::new().width(4).scroller_width(4).margin(0);
    column![
        container(
            row![
                heading,
                spacer(),
                arrow(Icon::ChevronLeft, "Scroll back", -SHELF_STEP),
                arrow(Icon::ChevronRight, "Scroll on", SHELF_STEP),
            ]
            .spacing(S2)
            .align_y(Center)
        )
        .padding(padding),
        mouse_area(
            scrollable(container(row(items).spacing(S3)).padding(cards))
                .id(id)
                .direction(Direction::Horizontal(bar))
                .style(theme::scroll)
                .width(Fill)
        )
        .on_enter(on_hover(Some(id)))
        .on_exit(on_hover(None)),
    ]
    .spacing(S2)
    .into()
}

/// Scrolls a shelf by `step` pixels.
pub fn scroll_shelf<M: Send + 'static>(id: &'static str, step: f32) -> Task<M> {
    iced::widget::operation::scroll_by(
        id,
        iced::widget::scrollable::AbsoluteOffset { x: step, y: 0.0 },
    )
}

/// How far the arrows of a shelf scroll.
const SHELF_STEP: f32 = 820.0;

/// Fläche für eine Gruppe zusammengehöriger Zeilen.
pub fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(S4)
        .width(Fill)
        .style(theme::surface)
        .into()
}

/// Zeilen mit Haarlinien dazwischen.
pub fn divided<'a, M: 'a>(rows: Vec<Element<'a, M>>) -> Column<'a, M> {
    let mut list = Column::new();
    for (index, item) in rows.into_iter().enumerate() {
        if index > 0 {
            list = list.push(divider());
        }
        list = list.push(item);
    }
    list
}

pub fn divider<'a, M: 'a>() -> Element<'a, M> {
    container(Space::new())
        .width(Fill)
        .height(1)
        .style(theme::line)
        .into()
}

/// Kennzahl mit Beschriftung darunter.
pub fn stat<'a, M: 'a>(value: String, name: &'a str, color: Color) -> Element<'a, M> {
    container(
        column![
            text(value)
                .size(theme::T_SECTION)
                .font(theme::DISPLAY)
                .color(color),
            label(name),
        ]
        .spacing(2),
    )
    .padding([S3, S4])
    .width(Fill)
    .style(theme::inset)
    .into()
}

/// Waagerechter Balken: Anteil `fraction` in Farbe, der Rest als Spur.
pub fn bar<'a, M: 'a>(fraction: f32, color: Color, height: f32) -> Element<'a, M> {
    let filled = (fraction.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let mut parts = Row::new().height(height);
    if filled > 0 {
        parts = parts.push(
            container(Space::new())
                .width(FillPortion(filled))
                .height(height)
                .style(theme::solid(color, height / 2.0)),
        );
    }
    if filled < 1000 {
        parts = parts.push(Space::new().width(FillPortion(1000 - filled)));
    }
    container(parts)
        .width(Fill)
        .style(theme::solid(theme::alpha(theme::TEXT, 0.06), height / 2.0))
        .into()
}

/// Leere Seite mit Icon, Aussage und nächstem Schritt.
pub fn empty_state<'a, M: 'a>(glyph: Icon, title: &'a str, hint: &'a str) -> Element<'a, M> {
    center(
        column![
            icon(glyph, 36.0, theme::TEXT_3),
            text(title)
                .size(theme::T_TITLE)
                .font(theme::DISPLAY)
                .color(theme::TEXT),
            container(muted(hint)).max_width(420),
        ]
        .spacing(S2)
        .align_x(Center),
    )
    .padding(S6)
    .into()
}

/// Puts a dialog in the middle of the page and blocks everything behind it.
pub fn modal<'a, M: 'a>(
    base: Element<'a, M>,
    title: impl text::IntoFragment<'a>,
    width: f32,
    body: Element<'a, M>,
) -> Element<'a, M> {
    let dialog = container(
        scrollable(
            column![
                text(title).size(theme::T_SECTION).font(theme::DISPLAY),
                body
            ]
            .spacing(S4)
            .padding(iced::padding::right(S3)),
        )
        .height(Length::Shrink)
        .style(theme::scroll),
    )
    .width(width)
    .max_height(780)
    .padding(S5)
    .style(theme::dialog);
    stack![base, opaque(center(dialog).padding(S5).style(theme::dim))].into()
}

pub fn spacer() -> Space {
    space().width(Fill)
}

/// Deckendes Kennzeichen, lesbar auf Bildern.
pub fn badge_row<'a, M: 'a>(label: String, color: iced::Color) -> Element<'a, M> {
    container(text(label).size(theme::T_CAPTION).font(SEMIBOLD))
        .padding([2, 6])
        .style(theme::badge(color))
        .align_y(Center)
        .into()
}

/// Getöntes Kennzeichen im Fließtext.
pub fn tag<'a, M: 'a>(label: impl text::IntoFragment<'a>, color: iced::Color) -> Element<'a, M> {
    container(text(label).size(theme::T_CAPTION).font(SEMIBOLD))
        .padding([2, 7])
        .style(theme::tag(color))
        .align_y(Center)
        .into()
}

/// Kurze Meldung unten rechts.
pub fn toast<'a, M: 'a>(message: &'a str, ok: bool) -> Element<'a, M> {
    let (glyph, color) = if ok {
        (Icon::CheckCircle, theme::TRACK)
    } else {
        (Icon::Question, theme::BAD)
    };
    container(
        row![icon(glyph, 16.0, color), text(message).size(theme::T_SMALL)]
            .spacing(S2)
            .align_y(Center),
    )
    .padding([S3, S4])
    .max_width(520)
    .style(theme::toast(ok))
    .into()
}

/// Führt ein Kernergebnis in eine Nachricht über.
pub fn perform<T: Send + 'static, M: Send + 'static>(
    future: impl Future<Output = musubi_core::Result<T>> + Send + 'static,
    wrap: impl Fn(Result<T, String>) -> M + Send + 'static,
) -> Task<M> {
    Task::perform(async move { future.await.map_err(|e| e.to_string()) }, wrap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_picks_the_two_largest_units() {
        assert_eq!(countdown(0), "airing");
        assert_eq!(countdown(30), "1m");
        assert_eq!(countdown(3_720), "1h 2m");
        assert_eq!(countdown(2 * 86_400 + 3 * 3_600 + 59), "2d 3h");
    }

    #[test]
    fn playback_titles_split_into_series_and_episode() {
        assert_eq!(
            split_title("Frieren S01E07 - Like a Fairy Tale"),
            ("Frieren", Some("S1:E7 · Like a Fairy Tale".to_owned()))
        );
        assert_eq!(
            split_title("Show S02E10 - "),
            ("Show", Some("S2:E10".to_owned()))
        );
        assert_eq!(split_title("Akira"), ("Akira", None));
        assert_eq!(split_title("SPY x FAMILY"), ("SPY x FAMILY", None));
    }

    #[test]
    fn ellipsize_respects_unicode_and_limit() {
        assert_eq!(ellipsize("kurz", 10), "kurz");
        assert_eq!(ellipsize("ずっと真夜中でいいのに", 4), "ずっと…");
    }
}
