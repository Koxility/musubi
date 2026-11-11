//! Home in the style of a media centre: a large hero and shelves.

use iced::widget::{Space, button, center, column, container, hover, responsive, row, stack, text};
use iced::{Center, Element, Fill, Padding, Size, Task};
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
const WIDE: (f32, f32) = (368.0, 207.0);
const POSTER: (f32, f32) = (192.0, 288.0);
/// Inset of the cards, so the hover surface shows around the picture.
const CARD_PADDING: f32 = 6.0;
/// Backdrops are 16:9. At that ratio the hero shows the whole picture.
const BACKDROP_RATIO: f32 = 9.0 / 16.0;
/// Share of the window height the hero may take, so the first shelf stays in view below.
const HERO_SHARE: f32 = 0.68;
const HERO_MIN: f32 = 380.0;

/// Height of the hero for a page of this size: as tall as the backdrop needs to show
/// uncropped, but never so tall that it pushes the shelves out of the window.
fn hero_height(page: Size) -> f32 {
    (page.width * BACKDROP_RATIO)
        .min(page.height * HERO_SHARE)
        .max(HERO_MIN)
}

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
    /// Carries the navigation generation of the click: whoever switched pages meanwhile
    /// must not be thrown back into a detail page.
    Opened(u64, Result<LibraryItem, String>),
    /// Scrolls a shelf sideways.
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

/// Time left of a started episode, e.g. `12 min left`.
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
            center(ui::icon(Icon::Play, 26.0, theme::INK))
                .width(58)
                .height(58),
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
        (Some(subtitle), Some(left)) => format!("{} · {left}", ui::ellipsize(subtitle, 34)),
        (Some(subtitle), None) => ui::ellipsize(subtitle, 50),
        (None, Some(left)) => left,
        (None, None) => String::new(),
    };
    let base = button(
        column![
            image,
            column![
                text(ui::ellipsize(&item.title, 40))
                    .size(theme::T_CARD)
                    .font(theme::SEMIBOLD),
                text(detail).size(theme::T_SMALL).color(theme::MUTED),
            ]
            .spacing(3),
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
                text(ui::ellipsize(&item.title, 22))
                    .size(theme::T_BODY)
                    .font(theme::MEDIUM),
                ui::muted(year),
            ]
            .spacing(3),
        ]
        .spacing(theme::S2),
    )
    .padding(CARD_PADDING)
    .style(theme::card_button)
    .on_press(Msg::Open(item.clone()))
    .into()
}

/// Hero for the title the user most likely continues with. The backdrop keeps its 16:9
/// frame where the window allows, so faces and logos are not cut off.
fn hero<'a>(app: &'a App, item: &'a ShelfItem, reason: &'a str, height: f32) -> Element<'a, Msg> {
    let backdrop = ui::picture(
        &app.images,
        Some(ImageKey::Backdrop(item.library_id.clone())),
        Fill,
        height,
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
                .size(theme::T_TITLE)
                .font(theme::MEDIUM)
                .color(theme::PAPER),
        );
    }
    if let Some(left) = remaining(item) {
        meta = meta.push(
            text(left)
                .size(theme::T_BODY)
                .color(theme::alpha(theme::PAPER, 0.7)),
        );
    }
    let mut info = column![
        meta,
        container(
            text(ui::ellipsize(&item.title, 70))
                .size(theme::T_HERO)
                .font(theme::DISPLAY_BOLD)
                .line_height(1.1)
        )
        .max_width(760),
    ]
    .spacing(theme::S3);
    if let Some(progress) = item.progress() {
        info = info.push(ui::progress(progress, 360, theme::WATCH));
    }
    if !overview.is_empty() {
        info = info.push(
            container(
                text(ui::ellipsize(overview, 260))
                    .size(theme::T_CARD)
                    .line_height(1.5)
                    .color(theme::alpha(theme::PAPER, 0.86)),
            )
            .max_width(620),
        );
    }
    info = info.push(
        container(
            row![
                ui::hero_action(Icon::Play, label, Msg::Play(item.clone())),
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
            .height(height)
            .style(style)
    };
    stack![
        backdrop,
        layer(theme::hero_side),
        layer(theme::hero_floor),
        container(info)
            .padding(iced::Padding {
                top: 0.0,
                right: theme::S6,
                bottom: theme::S6,
                left: theme::S6,
            })
            .height(height)
            .align_y(iced::Bottom),
    ]
    .height(height)
    .into()
}

fn page(app: &App, size: Size) -> Element<'_, Msg> {
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
    let mut content = column![hero(app, featured, reason, hero_height(size))].spacing(theme::S6);
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
    // The hero follows the window, so the page needs to know its size.
    let page: Element<'_, Msg> = responsive(move |size| page(app, size)).into();
    page.map(Message::Home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hero_shows_the_whole_backdrop_but_leaves_room_for_shelves() {
        // A narrow page: the 16:9 frame fits, nothing is cropped.
        assert_eq!(hero_height(Size::new(960.0, 900.0)), 540.0);
        // A wide page: capped, so the first shelf still shows below.
        assert_eq!(hero_height(Size::new(1600.0, 800.0)), 544.0);
        // A tiny window still gets a usable hero.
        assert_eq!(hero_height(Size::new(500.0, 300.0)), HERO_MIN);
    }
}
