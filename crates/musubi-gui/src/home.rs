//! Startseite im Stil eines Mediacenters: großes Titelbild und Regale.

use iced::widget::{Space, button, center, column, container, hover, row, stack, text};
use iced::{Center, Element, Fill, Padding, Task};
use musubi_core::HomeShelves;
use musubi_core::domain::{LibraryItem, ShelfItem};

use crate::app::{App, Message, Page};
use crate::icons::Icon;
use crate::images::ImageKey;
use crate::{library, theme, ui};

const SHELF_PADDING: Padding = Padding {
    top: 0.0,
    right: theme::S6,
    bottom: 0.0,
    left: theme::S6,
};
const HERO_HEIGHT: f32 = 460.0;
const WIDE: (f32, f32) = (288.0, 162.0);
const POSTER: (f32, f32) = (152.0, 228.0);
/// Rand der Karten, damit die Fläche beim Überfahren um das Bild herum sichtbar wird.
const CARD_PADDING: f32 = 6.0;

#[derive(Default)]
pub struct State {
    shelves: HomeShelves,
    loading: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    WantImage(ImageKey),
    Loaded(Result<HomeShelves, String>),
    Play(ShelfItem),
    Open(ShelfItem),
    /// Mit der Navigations-Generation beim Klick: Wer inzwischen die Seite gewechselt hat,
    /// soll nicht in eine Detailseite zurückgeworfen werden.
    Opened(u64, Result<LibraryItem, String>),
    /// Blättert ein Regal waagerecht.
    Scroll(&'static str, f32),
}

pub fn load(app: &mut App) -> Task<Message> {
    if app.auth.jellyfin_user.is_none() {
        return Task::none();
    }
    app.home.loading = true;
    let core = app.core.clone();
    ui::perform(async move { core.home().await }, |r| {
        Message::Home(Msg::Loaded(r))
    })
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    match msg {
        Msg::WantImage(key) => return app.want_image(key),
        Msg::Loaded(result) => {
            app.home.loading = false;
            match result {
                Ok(shelves) => app.home.shelves = shelves,
                Err(error) => app.toast(error, false),
            }
        }
        Msg::Play(item) => {
            let core = app.core.clone();
            return ui::perform(
                async move {
                    let library = core.library_item(&item.library_id).await?;
                    let episode = item.is_episode.then_some(item.id.as_str());
                    core.play(&library, episode).await
                },
                Message::Notify,
            );
        }
        Msg::Open(item) => {
            let core = app.core.clone();
            let generation = app.nav_generation;
            return ui::perform(
                async move { core.library_item(&item.library_id).await },
                move |r| Message::Home(Msg::Opened(generation, r)),
            );
        }
        Msg::Opened(generation, _) if generation != app.nav_generation => {}
        Msg::Opened(_, Ok(item)) if app.page == Page::Home => {
            return library::open_detail(app, item);
        }
        Msg::Opened(_, Ok(_)) => {}
        Msg::Opened(_, Err(error)) => app.toast(error, false),
        Msg::Scroll(id, step) => return ui::scroll_shelf(id, step),
    }
    Task::none()
}

fn want(key: ImageKey) -> Msg {
    Msg::WantImage(key)
}

fn wide_image(item: &ShelfItem) -> ImageKey {
    if item.has_image {
        ImageKey::Poster(item.id.clone())
    } else {
        ImageKey::Backdrop(item.library_id.clone())
    }
}

/// Restzeit einer angefangenen Folge, z. B. `12 min left`.
fn remaining(item: &ShelfItem) -> Option<String> {
    let runtime = item.runtime_secs.filter(|r| *r > 0)?;
    (item.position_secs > 0).then(|| {
        let minutes = runtime.saturating_sub(item.position_secs).div_ceil(60);
        format!("{minutes} min left")
    })
}

fn play_overlay<'a>(item: &ShelfItem, size: (f32, f32)) -> Element<'a, Msg> {
    center(
        button(
            center(ui::icon(Icon::Play, 22.0, theme::INK))
                .width(48)
                .height(48),
        )
        .padding(0)
        .style(theme::play_overlay)
        .on_press(Msg::Play(item.clone())),
    )
    .width(size.0 + 2.0 * CARD_PADDING)
    .height(size.1 + 2.0 * CARD_PADDING)
    .into()
}

fn wide_card<'a>(app: &'a App, item: &ShelfItem) -> Element<'a, Msg> {
    let picture = ui::picture(&app.images, Some(wide_image(item)), WIDE.0, WIDE.1, want);
    let image = ui::with_progress(picture, item.progress(), WIDE, theme::WATCH);
    let detail = match (item.subtitle.as_deref(), remaining(item)) {
        (Some(subtitle), Some(left)) => format!("{} · {left}", ui::ellipsize(subtitle, 28)),
        (Some(subtitle), None) => ui::ellipsize(subtitle, 42),
        (None, Some(left)) => left,
        (None, None) => String::new(),
    };
    let base = button(
        column![
            image,
            column![
                text(ui::ellipsize(&item.title, 36))
                    .size(theme::T_BODY)
                    .font(theme::MEDIUM),
                ui::muted(detail),
            ]
            .spacing(2),
        ]
        .spacing(theme::S2),
    )
    .padding(CARD_PADDING)
    .style(theme::card_button)
    .on_press(Msg::Open(item.clone()));
    hover(base, play_overlay(item, WIDE))
}

fn poster_card<'a>(app: &'a App, item: &ShelfItem) -> Element<'a, Msg> {
    let key = ImageKey::Poster(item.library_id.clone());
    let year = item.year.map(|y| y.to_string()).unwrap_or_default();
    button(
        column![
            ui::picture(&app.images, Some(key), POSTER.0, POSTER.1, want),
            column![
                text(ui::ellipsize(&item.title, 19))
                    .size(theme::T_SMALL)
                    .font(theme::MEDIUM),
                ui::muted(year),
            ]
            .spacing(2),
        ]
        .spacing(theme::S2),
    )
    .padding(CARD_PADDING)
    .style(theme::card_button)
    .on_press(Msg::Open(item.clone()))
    .into()
}

/// Großes Titelbild für den Eintrag, mit dem es am ehesten weitergeht.
fn hero<'a>(app: &'a App, item: &'a ShelfItem, reason: &'a str) -> Element<'a, Msg> {
    let backdrop = ui::picture(
        &app.images,
        Some(ImageKey::Backdrop(item.library_id.clone())),
        Fill,
        HERO_HEIGHT,
        want,
    );
    let overview = item.overview.as_deref().unwrap_or("");
    let label = if item.position_secs > 0 {
        "Resume"
    } else {
        "Play"
    };
    let mut meta = row![ui::tag(reason, theme::WATCH)]
        .spacing(theme::S3)
        .align_y(Center);
    if let Some(subtitle) = &item.subtitle {
        meta = meta.push(
            text(ui::ellipsize(subtitle, 60))
                .size(theme::T_BODY)
                .font(theme::MEDIUM)
                .color(theme::PAPER),
        );
    }
    if let Some(left) = remaining(item) {
        meta = meta.push(ui::muted(left));
    }
    let mut info = column![
        meta,
        text(ui::ellipsize(&item.title, 60))
            .size(theme::T_DISPLAY)
            .font(theme::DISPLAY_BOLD)
            .line_height(1.15),
    ]
    .spacing(theme::S2);
    if let Some(progress) = item.progress() {
        info = info.push(ui::progress(progress, 280, theme::WATCH));
    }
    if !overview.is_empty() {
        info = info.push(
            container(
                text(ui::ellipsize(overview, 240))
                    .size(theme::T_BODY)
                    .line_height(1.5)
                    .color(theme::alpha(theme::PAPER, 0.82)),
            )
            .max_width(560),
        );
    }
    info = info.push(
        container(
            row![
                ui::icon_label(
                    Icon::Play,
                    label,
                    Msg::Play(item.clone()),
                    theme::watch_pill,
                    theme::INK
                ),
                ui::secondary("Details", Msg::Open(item.clone())),
            ]
            .spacing(theme::S3)
            .align_y(Center),
        )
        .padding(iced::padding::top(theme::S2)),
    );
    let layer = |style: fn(&iced::Theme) -> container::Style| {
        container(Space::new())
            .width(Fill)
            .height(HERO_HEIGHT)
            .style(style)
    };
    stack![
        backdrop,
        layer(theme::hero_side),
        layer(theme::hero_fade),
        container(info)
            .padding(iced::Padding {
                top: 0.0,
                right: theme::S6,
                bottom: theme::S5,
                left: theme::S6,
            })
            .height(HERO_HEIGHT)
            .align_y(iced::Bottom),
    ]
    .height(HERO_HEIGHT)
    .into()
}

fn page(app: &App) -> Element<'_, Msg> {
    let shelves = &app.home.shelves;
    if app.auth.jellyfin_user.is_none() {
        return ui::empty_state(
            Icon::Home,
            "No Jellyfin server yet",
            "Log in to Jellyfin in Settings to see what to watch next.",
        );
    }
    let featured = shelves
        .continue_watching
        .first()
        .map(|i| (i, "Continue watching"))
        .or(shelves.next_up.first().map(|i| (i, "Next up")))
        .or(shelves.latest.first().map(|i| (i, "Recently added")));
    let Some((featured, reason)) = featured else {
        return if app.home.loading {
            center(ui::muted("Loading your library…")).into()
        } else {
            ui::empty_state(
                Icon::Home,
                "Nothing to watch yet",
                "Titles appear here once your Jellyfin libraries have content.",
            )
        };
    };
    let mut content = column![hero(app, featured, reason)].spacing(theme::S5);
    let wide = |items: &'_ [ShelfItem]| -> Vec<Element<'_, Msg>> {
        items.iter().map(|i| wide_card(app, i)).collect()
    };
    let scroll: fn(&'static str, f32) -> Msg = Msg::Scroll;
    if !shelves.continue_watching.is_empty() {
        content = content.push(ui::shelf(
            "Continue watching",
            "home-continue",
            wide(&shelves.continue_watching),
            SHELF_PADDING,
            CARD_PADDING,
            scroll,
        ));
    }
    if !shelves.next_up.is_empty() {
        content = content.push(ui::shelf(
            "Next up",
            "home-next",
            wide(&shelves.next_up),
            SHELF_PADDING,
            CARD_PADDING,
            scroll,
        ));
    }
    if !shelves.latest.is_empty() {
        let posters = shelves.latest.iter().map(|i| poster_card(app, i)).collect();
        content = content.push(ui::shelf(
            "Recently added",
            "home-latest",
            posters,
            SHELF_PADDING,
            CARD_PADDING,
            scroll,
        ));
    }
    ui::scroll(content.padding(iced::padding::bottom(theme::S6)))
}

pub fn view(app: &App) -> Element<'_, Message> {
    page(app).map(Message::Home)
}
