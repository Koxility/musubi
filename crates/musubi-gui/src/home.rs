//! Home in the style of a media centre: a large hero and shelves.

use std::time::{Duration, Instant};

use iced::widget::{Space, button, center, column, container, hover, responsive, row, stack, text};
use iced::{Center, Element, Fill, Padding, Size, Subscription, Task, mouse};
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

/// The pointer must travel this far before a press is a drag and not a click on a card.
const DRAG_THRESHOLD: f32 = 6.0;
/// Seconds in which a glide covers about two thirds of what is left of it.
const GLIDE_TAU: f32 = 0.12;
/// A flick carries the shelf on by this many seconds of its speed.
const FLICK: f32 = 0.3;
const FLICK_MAX: f32 = 2400.0;
/// A pointer that rested this long before the release was placed, not flicked.
const REST: Duration = Duration::from_millis(80);

/// A shelf held with the mouse button.
struct Drag {
    shelf: &'static str,
    /// Pointer position and time of the last movement; `None` until the first one.
    last: Option<(f32, Instant)>,
    travelled: f32,
    /// Pixels per second, smoothed.
    velocity: f32,
}

#[derive(Default)]
pub struct State {
    shelves: HomeShelves,
    loading: bool,
    /// The shelf under the pointer.
    hovered: Option<&'static str>,
    drag: Option<Drag>,
    /// The last press turned into a drag, so the click that ends it must not open a card.
    dragged: bool,
    /// A shelf easing towards its target and the distance it still has to go.
    glide: Option<(&'static str, f32)>,
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
    /// The pointer entered a shelf, or left it.
    Hover(Option<&'static str>),
    Pressed,
    /// The pointer's horizontal position while a shelf is held.
    Moved(f32),
    Released,
}

fn on_press(event: iced::Event, _: iced::event::Status, _: iced::window::Id) -> Option<Message> {
    matches!(
        event,
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
    )
    .then_some(Message::Home(Msg::Pressed))
}

fn on_drag(event: iced::Event, _: iced::event::Status, _: iced::window::Id) -> Option<Message> {
    let iced::Event::Mouse(event) = event else {
        return None;
    };
    let msg = match event {
        mouse::Event::CursorMoved { position } => Msg::Moved(position.x),
        mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft => {
            Msg::Released
        }
        _ => return None,
    };
    Some(Message::Home(msg))
}

/// Raw pointer events for dragging a shelf. Cards are buttons and swallow the press, so a
/// widget around them never sees it. Movement is only listened to while a shelf is held.
pub fn subscription(state: &State) -> Subscription<Message> {
    if state.drag.is_some() {
        iced::event::listen_with(on_drag)
    } else if state.hovered.is_some() {
        iced::event::listen_with(on_press)
    } else {
        Subscription::none()
    }
}

/// How far a glide with `pending` pixels left moves within `dt`: fast first, then settling.
fn glide_step(pending: f32, dt: Duration) -> f32 {
    pending * (1.0 - (-dt.as_secs_f32() / GLIDE_TAU).exp())
}

/// Sends a shelf on its way by `distance` pixels, on top of what it still has to go.
fn glide(app: &mut App, shelf: &'static str, distance: f32) {
    let pending = match app.home.glide {
        Some((gliding, pending)) if gliding == shelf => pending,
        _ => 0.0,
    };
    app.home.glide = Some((shelf, pending + distance));
    app.frame = Instant::now();
}

pub fn gliding(state: &State) -> bool {
    state.glide.is_some()
}

/// Moves a gliding shelf on by one frame.
pub fn frame(app: &mut App, dt: Duration) -> Task<Message> {
    let Some((shelf, pending)) = app.home.glide else {
        return Task::none();
    };
    let step = glide_step(pending, dt);
    let left = pending - step;
    app.home.glide = (left.abs() > 0.5).then_some((shelf, left));
    ui::scroll_shelf(shelf, step)
}

pub fn load(app: &mut App) -> Task<Message> {
    if app.auth.jellyfin_user.is_none() {
        return Task::none();
    }
    app.home.loading = true;
    // The pointer may be anywhere by now; the shelf reports itself again when it is over one.
    app.home.hovered = None;
    app.home.drag = None;
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
        Msg::Play(_) | Msg::Open(_) if app.home.dragged => {}
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
            app.home.hovered = None;
            return library::open_detail(app, item);
        }
        Msg::Opened(_, Ok(_)) => {}
        Msg::Opened(_, Err(error)) => app.toast(error, false),
        Msg::Scroll(id, step) => glide(app, id, step),
        Msg::Hover(shelf) => {
            app.home.hovered = shelf;
            if shelf.is_none() && app.home.drag.is_none() {
                app.home.dragged = false;
            }
        }
        Msg::Pressed => {
            if let Some(shelf) = app.home.hovered {
                app.home.dragged = false;
                // Grabbing a shelf stops it.
                app.home.glide = None;
                app.home.drag = Some(Drag {
                    shelf,
                    last: None,
                    travelled: 0.0,
                    velocity: 0.0,
                });
            }
        }
        Msg::Moved(x) => {
            let Some(drag) = &mut app.home.drag else {
                return Task::none();
            };
            let now = Instant::now();
            let Some((last_x, at)) = drag.last.replace((x, now)) else {
                return Task::none();
            };
            // The cards follow the pointer, so the shelf scrolls the other way.
            let dx = last_x - x;
            let dt = now.duration_since(at).as_secs_f32().max(0.001);
            drag.travelled += dx.abs();
            drag.velocity = 0.6 * drag.velocity + 0.4 * dx / dt;
            if drag.travelled > DRAG_THRESHOLD {
                let shelf = drag.shelf;
                app.home.dragged = true;
                return ui::scroll_shelf(shelf, dx);
            }
        }
        Msg::Released => {
            if let Some(drag) = app.home.drag.take()
                && app.home.dragged
                && drag.last.is_some_and(|(_, at)| at.elapsed() < REST)
            {
                let flick = (drag.velocity * FLICK).clamp(-FLICK_MAX, FLICK_MAX);
                glide(app, drag.shelf, flick);
            }
        }
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
            center(ui::icon(Icon::Play, 26.0, theme::BG))
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
                text(detail).size(theme::T_SMALL).color(theme::TEXT_2),
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
                .color(theme::TEXT),
        );
    }
    if let Some(left) = remaining(item) {
        meta = meta.push(
            text(left)
                .size(theme::T_BODY)
                .color(theme::alpha(theme::TEXT, 0.7)),
        );
    }
    let mut info = column![
        meta,
        container(
            text(ui::ellipsize(&item.title, 70))
                .size(theme::T_HERO)
                .font(theme::DISPLAY)
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
                    .color(theme::alpha(theme::TEXT, 0.86)),
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
    let hover: fn(Option<&'static str>) -> Msg = Msg::Hover;
    if !shelves.continue_watching.is_empty() {
        content = content.push(ui::shelf(
            "Continue watching",
            "home-continue",
            wide(&shelves.continue_watching),
            SHELF_PADDING,
            CARD_PADDING,
            scroll,
            hover,
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
            hover,
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
            hover,
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

    #[test]
    fn a_glide_starts_fast_settles_and_covers_its_distance() {
        let frame = Duration::from_millis(16);
        let mut pending = 820.0_f32;
        let first = glide_step(pending, frame);
        let mut frames = 0;
        while pending.abs() > 0.5 {
            let step = glide_step(pending, frame);
            assert!(step <= first && step > 0.0);
            pending -= step;
            frames += 1;
        }
        // Done in well under a second at 60 frames per second, without overshooting.
        assert!((20..60).contains(&frames), "{frames} frames");
        assert!(pending >= 0.0);
        // A long frame never jumps past the target, and a backwards glide mirrors it.
        assert!(glide_step(820.0, Duration::from_secs(5)) <= 820.0);
        assert_eq!(glide_step(-820.0, frame), -first);
    }
}
