//! Startseite im Stil eines Mediacenters: großes Titelbild und Regale.

use iced::widget::{
    Space, button, center, column, container, hover, progress_bar, row, scrollable, stack, text,
};
use iced::{Center, Element, Fill, Length, Padding, Task};
use musubi_core::HomeShelves;
use musubi_core::domain::{LibraryItem, ShelfItem};

use crate::app::{App, Message, Page};
use crate::images::ImageKey;
use crate::{library, theme, ui};

const SHELF_PADDING: Padding = Padding {
    top: 0.0,
    right: 32.0,
    bottom: 0.0,
    left: 32.0,
};
const HERO_HEIGHT: f32 = 420.0;
const WIDE: (f32, f32) = (300.0, 169.0);
const POSTER: (f32, f32) = (160.0, 240.0);

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

fn play_overlay<'a>(item: &ShelfItem, size: (f32, f32)) -> Element<'a, Msg> {
    center(
        button(text("▶").size(22))
            .padding([10, 16])
            .style(theme::primary)
            .on_press(Msg::Play(item.clone())),
    )
    .width(size.0)
    .height(size.1)
    .into()
}

fn wide_card<'a>(app: &'a App, item: &ShelfItem) -> Element<'a, Msg> {
    let picture = ui::picture(&app.images, Some(wide_image(item)), WIDE.0, WIDE.1, want);
    let image: Element<'a, Msg> = match item.progress() {
        Some(progress) => stack![
            picture,
            iced::widget::bottom(
                progress_bar(0.0..=1.0, progress)
                    .length(Length::Fixed(WIDE.0))
                    .girth(4)
            )
            .width(WIDE.0)
            .height(WIDE.1)
        ]
        .into(),
        None => picture,
    };
    let base = button(
        column![
            image,
            text(ui::ellipsize(&item.title, 38)).size(15),
            ui::muted(ui::ellipsize(item.subtitle.as_deref().unwrap_or(""), 44)),
        ]
        .spacing(6),
    )
    .padding(4)
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
            text(ui::ellipsize(&item.title, 20)).size(14),
            ui::muted(year),
        ]
        .spacing(5),
    )
    .padding(4)
    .style(theme::card_button)
    .on_press(Msg::Open(item.clone()))
    .into()
}

fn hero<'a>(app: &'a App, item: &'a ShelfItem) -> Element<'a, Msg> {
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
    let info = container(
        column![
            text(ui::ellipsize(&item.title, 60)).size(38),
            ui::muted(item.subtitle.clone().unwrap_or_default()),
            container(text(ui::ellipsize(overview, 260)).size(15)).max_width(640),
            row![
                ui::primary(label, Msg::Play(item.clone())),
                ui::secondary("Details", Msg::Open(item.clone())),
            ]
            .spacing(10),
        ]
        .spacing(10),
    )
    .padding(32)
    .height(HERO_HEIGHT)
    .align_y(iced::Bottom);
    stack![
        backdrop,
        container(Space::new())
            .width(Fill)
            .height(HERO_HEIGHT)
            .style(theme::hero_fade),
        info,
    ]
    .height(HERO_HEIGHT)
    .into()
}

fn page(app: &App) -> Element<'_, Msg> {
    let shelves = &app.home.shelves;
    if app.auth.jellyfin_user.is_none() {
        return center(ui::muted(
            "Log in to Jellyfin in Settings to see your library.",
        ))
        .into();
    }
    let featured = shelves
        .continue_watching
        .first()
        .or(shelves.next_up.first())
        .or(shelves.latest.first());
    let Some(featured) = featured else {
        return center(ui::muted(if app.home.loading {
            "Loading…"
        } else {
            "Nothing to show yet."
        }))
        .into();
    };
    let mut content = column![hero(app, featured)].spacing(26);
    let wide = |items: &'_ [ShelfItem]| -> Vec<Element<'_, Msg>> {
        items.iter().map(|i| wide_card(app, i)).collect()
    };
    if !shelves.continue_watching.is_empty() {
        content = content.push(ui::shelf(
            "Continue Watching",
            wide(&shelves.continue_watching),
            SHELF_PADDING,
        ));
    }
    if !shelves.next_up.is_empty() {
        content = content.push(ui::shelf("Next Up", wide(&shelves.next_up), SHELF_PADDING));
    }
    if !shelves.latest.is_empty() {
        let posters = shelves.latest.iter().map(|i| poster_card(app, i)).collect();
        content = content.push(ui::shelf("Recently Added", posters, SHELF_PADDING));
    }
    scrollable(content.padding(iced::padding::bottom(28)))
        .width(Fill)
        .height(Fill)
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let body = page(app).map(Message::Home);
    container(body)
        .width(Fill)
        .height(Fill)
        .align_y(Center)
        .into()
}
