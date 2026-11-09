//! Wiederverwendbare Bausteine der Oberfläche.

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::tooltip::Position;
use iced::widget::{
    Space, button, center, column, container, image, opaque, row, scrollable, sensor, space, stack,
    svg, text, text_input, tooltip,
};
use iced::{Center, Color, ContentFit, Element, Fill, Font, Length, Padding, Task, font};

use crate::icons::Icon;
use crate::images::{ImageCache, ImageKey};
use crate::theme;

/// Halbfette Variante der Standardschrift für Titel.
pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};

/// Button-Stil als Funktion, wie ihn `button::style` erwartet.
pub type ButtonStyle = fn(&iced::Theme, button::Status) -> button::Style;

/// Ein Icon in fester Größe und Farbe.
pub fn icon<'a, M: 'a>(icon: Icon, size: f32, color: Color) -> Element<'a, M> {
    svg(icon.handle())
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

/// Hängt einen kurzen Hinweis an, der beim Überfahren erscheint.
pub fn with_tip<'a, M: 'a>(content: impl Into<Element<'a, M>>, label: &'a str) -> Element<'a, M> {
    tooltip(
        content,
        container(text(label).size(12))
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
        Color {
            a: color.a * 0.35,
            ..color
        }
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
            text(label).size(15).font(SEMIBOLD).color(color)
        ]
        .spacing(8)
        .align_y(Center),
    )
    .padding([10, 22])
    .style(style)
    .on_press(message)
    .into()
}

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

/// Bild mit Platzhalter, das beim ersten Sichtbarwerden nachgeladen wird.
pub fn picture<'a, M: Clone + 'a>(
    images: &'a ImageCache,
    key: Option<ImageKey>,
    width: impl Into<Length>,
    height: impl Into<Length>,
    want: fn(ImageKey) -> M,
) -> Element<'a, M> {
    let (width, height) = (width.into(), height.into());
    let Some(key) = key else {
        return container(Space::new())
            .width(width)
            .height(height)
            .style(theme::placeholder)
            .into();
    };
    let content: Element<'a, M> = match images.get(&key) {
        Some(handle) => image(handle.clone())
            .width(width)
            .height(height)
            .content_fit(ContentFit::Cover)
            .border_radius(6.0)
            .into(),
        None => container(Space::new())
            .width(width)
            .height(height)
            .style(theme::placeholder)
            .into(),
    };
    sensor(content)
        .key(key.clone())
        .anticipate(600)
        .on_show(move |_| want(key.clone()))
        .into()
}

pub fn heading<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(20).color(theme::TEXT).into()
}

pub fn muted<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(13).color(theme::MUTED).into()
}

pub fn error_line<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(14).color(theme::RED).into()
}

pub fn primary<'a, M: Clone + 'a>(label: &'a str, message: M) -> Element<'a, M> {
    button(text(label).size(14))
        .padding([8, 16])
        .style(theme::primary)
        .on_press(message)
        .into()
}

pub fn secondary<'a, M: Clone + 'a>(label: &'a str, message: M) -> Element<'a, M> {
    button(text(label).size(14))
        .padding([8, 16])
        .style(theme::secondary)
        .on_press(message)
        .into()
}

pub fn danger<'a, M: Clone + 'a>(label: &'a str, message: M) -> Element<'a, M> {
    button(text(label).size(14))
        .padding([8, 16])
        .style(theme::danger)
        .on_press(message)
        .into()
}

pub fn field<'a, M: Clone + 'a>(
    label: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    column![
        text(label).size(12).color(theme::MUTED),
        text_input("", value)
            .on_input(on_input)
            .padding(8)
            .style(theme::input),
    ]
    .spacing(4)
    .into()
}

pub fn secret_field<'a, M: Clone + 'a>(
    label: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> M + 'a,
    on_submit: Option<M>,
) -> Element<'a, M> {
    column![
        text(label).size(12).color(theme::MUTED),
        text_input("", value)
            .secure(true)
            .on_input(on_input)
            .on_submit_maybe(on_submit)
            .padding(8)
            .style(theme::input),
    ]
    .spacing(4)
    .into()
}

/// Waagerechte Leiste ohne sichtbaren Scrollbalken.
pub fn shelf<'a, M: 'a>(
    title: &'a str,
    items: Vec<Element<'a, M>>,
    padding: Padding,
) -> Element<'a, M> {
    column![
        container(heading(title)).padding(padding),
        scrollable(container(row(items).spacing(14)).padding(padding))
            .direction(Direction::Horizontal(Scrollbar::hidden()))
            .width(Fill),
    ]
    .spacing(10)
    .into()
}

pub fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(16)
        .width(Fill)
        .style(theme::surface)
        .into()
}

/// Legt einen Dialog mittig über die Seite und sperrt alles dahinter.
pub fn modal<'a, M: 'a>(
    base: Element<'a, M>,
    title: &'a str,
    width: f32,
    body: Element<'a, M>,
) -> Element<'a, M> {
    let dialog = container(
        scrollable(
            column![text(title).size(20), body]
                .spacing(14)
                .padding(iced::padding::right(12)),
        )
        .height(Length::Shrink),
    )
    .width(width)
    .max_height(760)
    .padding(22)
    .style(theme::surface);
    stack![base, opaque(center(dialog).padding(24).style(theme::dim))].into()
}

pub fn spacer() -> Space {
    space().width(Fill)
}

pub fn badge_row<'a, M: 'a>(label: String, color: iced::Color) -> Element<'a, M> {
    container(text(label).size(11))
        .padding([2, 6])
        .style(theme::badge(color))
        .align_y(Center)
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
