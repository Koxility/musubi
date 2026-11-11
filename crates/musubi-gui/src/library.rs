//! Bibliotheksraster und Detailseite einer Serie.

use iced::widget::{
    Space, button, center, column, container, responsive, row, stack, text, text_input,
};
use iced::{Bottom, Center, Color, Element, Fill, Task};
use musubi_core::domain::ListEntry;
use musubi_core::domain::{
    ItemKind, Library, LibraryFilters, LibraryItem, LibraryQuery, LibrarySort, Mapping,
    MappingSource, is_unsure, season_mappings,
};
use musubi_core::{EpisodeRow, LibraryRow};

use crate::app::{App, Message, Page};
use crate::icons::Icon;
use crate::images::ImageKey;
use crate::{anilist, mapping, theme, ui};

const CARD_WIDTH: f32 = 168.0;
const CARD_HEIGHT: f32 = 252.0;
/// Rand der Karten, damit die Fläche beim Überfahren um das Poster herum sichtbar wird.
const CARD_PADDING: f32 = 6.0;
const GAP: f32 = theme::S2;
const BACKDROP_HEIGHT: f32 = 500.0;
const THUMB: (f32, f32) = (208.0, 117.0);
const ALL_LIBRARIES: &str = "All libraries";
const ALL_GENRES: &str = "All genres";
const ALL_YEARS: &str = "All years";
const ALL_STATUSES: &str = "All statuses";
const ALL_TYPES: &str = "All types";
const SORTS: [&str; 4] = ["Name", "Year", "Date added", "Rating"];
const TYPES: [&str; 3] = [ALL_TYPES, "Series", "Movies"];
const STATUSES: [&str; 3] = [ALL_STATUSES, "Continuing", "Ended"];

pub struct Detail {
    pub item: LibraryItem,
    /// Die Seite, auf der die Detailansicht geöffnet wurde. Nur dort wird sie gezeigt.
    pub origin: Page,
    /// Navigations-Generation beim Öffnen. Nach jedem Klick in der Navigation ist sie veraltet,
    /// und die Detailseite bleibt unsichtbar, egal wie ihr Zustand dorthin gekommen ist.
    pub opened_at: u64,
    episodes: Vec<EpisodeRow>,
    loading: bool,
    season: Option<u32>,
}

pub struct State {
    pub rows: Vec<LibraryRow>,
    filters: LibraryFilters,
    libraries: Vec<Library>,
    search: String,
    library: String,
    genre: String,
    year: String,
    status: String,
    kind: String,
    sort: String,
    unplayed: bool,
    generation: u64,
    applied: u64,
    pub detail: Option<Detail>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            rows: vec![],
            filters: LibraryFilters::default(),
            libraries: vec![],
            search: String::new(),
            library: ALL_LIBRARIES.to_owned(),
            genre: ALL_GENRES.to_owned(),
            year: ALL_YEARS.to_owned(),
            status: ALL_STATUSES.to_owned(),
            kind: ALL_TYPES.to_owned(),
            sort: SORTS[0].to_owned(),
            unplayed: false,
            generation: 0,
            applied: 0,
            detail: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Filter {
    Library(String),
    Genre(String),
    Year(String),
    Status(String),
    Kind(String),
    Sort(String),
    Unplayed(bool),
}

#[derive(Debug, Clone)]
pub enum Msg {
    WantImage(ImageKey),
    Loaded(u64, Result<Vec<LibraryRow>, String>),
    Choices(Result<(LibraryFilters, Vec<Library>), String>),
    SearchChanged(String),
    Search,
    Filter(Filter),
    Open(LibraryItem),
    CloseDetail,
    Episodes(String, Result<Vec<EpisodeRow>, String>),
    Season(u32),
    Play(LibraryItem, Option<String>),
    SetPlayed(String, bool),
    EditEntry(LibraryItem),
    Map(LibraryItem),
    ConfirmMapping(String),
}

impl State {
    fn query(&self) -> LibraryQuery {
        LibraryQuery {
            library_id: self
                .libraries
                .iter()
                .find(|l| l.name == self.library)
                .map(|l| l.id.clone()),
            search: Some(self.search.trim().to_owned()).filter(|s| !s.is_empty()),
            genres: (self.genre != ALL_GENRES)
                .then(|| self.genre.clone())
                .into_iter()
                .collect(),
            years: self.year.parse().ok().into_iter().collect(),
            status: (self.status != ALL_STATUSES).then(|| self.status.clone()),
            kind: match self.kind.as_str() {
                "Series" => Some(ItemKind::Series),
                "Movies" => Some(ItemKind::Movie),
                _ => None,
            },
            unplayed_only: self.unplayed,
            sort: match self.sort.as_str() {
                "Year" => LibrarySort::Year,
                "Date added" => LibrarySort::DateAdded,
                "Rating" => LibrarySort::Rating,
                _ => LibrarySort::Name,
            },
            start: 0,
            limit: None,
        }
    }
}

pub fn load(app: &mut App) -> Task<Message> {
    if app.auth.jellyfin_user.is_none() {
        return Task::none();
    }
    app.library.generation += 1;
    let generation = app.library.generation;
    let (core, query) = (app.core.clone(), app.library.query());
    ui::perform(async move { core.library(&query).await }, move |r| {
        Message::Library(Msg::Loaded(generation, r))
    })
}

pub fn load_cached(app: &App) -> Task<Message> {
    let core = app.core.clone();
    ui::perform(async move { core.cached_library().await }, |r| {
        Message::Library(Msg::Loaded(0, r))
    })
}

pub fn load_choices(app: &App) -> Task<Message> {
    if app.auth.jellyfin_user.is_none() {
        return Task::none();
    }
    let core = app.core.clone();
    ui::perform(
        async move { Ok((core.filters(None).await?, core.libraries().await?)) },
        |r| Message::Library(Msg::Choices(r)),
    )
}

fn load_episodes(app: &App, id: String) -> Task<Message> {
    let core = app.core.clone();
    let key = id.clone();
    ui::perform(async move { core.episodes(&id).await }, move |r| {
        Message::Library(Msg::Episodes(key.clone(), r))
    })
}

/// Lädt Raster und offene Detailseite nach einer Änderung neu.
pub fn reload_view(app: &mut App) -> Task<Message> {
    let detail = app
        .library
        .detail
        .as_ref()
        .filter(|d| d.item.kind == ItemKind::Series)
        .map(|d| d.item.id.clone());
    let episodes = detail.map_or(Task::none(), |id| load_episodes(app, id));
    Task::batch([load(app), episodes])
}

pub fn open_detail(app: &mut App, item: LibraryItem) -> Task<Message> {
    let id = item.id.clone();
    let series = item.kind == ItemKind::Series;
    app.view_changed();
    app.library.detail = Some(Detail {
        item,
        origin: app.page,
        opened_at: app.nav_generation,
        episodes: vec![],
        loading: series,
        season: None,
    });
    if series {
        load_episodes(app, id)
    } else {
        Task::none()
    }
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    match msg {
        Msg::WantImage(key) => return app.want_image(key),
        Msg::Loaded(generation, _) if generation < app.library.applied => {}
        Msg::Loaded(generation, Ok(rows)) => {
            app.library.applied = generation;
            app.library.rows = rows;
        }
        Msg::Loaded(_, Err(error)) => app.toast(error, false),
        Msg::Choices(Ok((filters, libraries))) => {
            app.library.filters = filters;
            app.library.libraries = libraries;
        }
        Msg::Choices(Err(error)) => app.toast(error, false),
        Msg::SearchChanged(value) => app.library.search = value,
        Msg::Search => return load(app),
        Msg::Filter(filter) => {
            let state = &mut app.library;
            match filter {
                Filter::Library(v) => state.library = v,
                Filter::Genre(v) => state.genre = v,
                Filter::Year(v) => state.year = v,
                Filter::Status(v) => state.status = v,
                Filter::Kind(v) => state.kind = v,
                Filter::Sort(v) => state.sort = v,
                Filter::Unplayed(v) => state.unplayed = v,
            }
            return load(app);
        }
        Msg::Open(item) => return open_detail(app, item),
        Msg::CloseDetail => app.close_detail(),
        Msg::Episodes(id, result) => match (&mut app.library.detail, result) {
            (Some(detail), Ok(episodes)) if detail.item.id == id => {
                detail.loading = false;
                detail.season = detail.season.or_else(|| first_open_season(&episodes));
                detail.episodes = episodes;
            }
            (Some(detail), Err(error)) if detail.item.id == id => {
                detail.loading = false;
                app.toast(error, false);
            }
            _ => {}
        },
        Msg::Season(season) => {
            if let Some(detail) = &mut app.library.detail {
                detail.season = Some(season);
            }
        }
        Msg::Play(item, episode) => {
            let core = app.core.clone();
            return ui::perform(
                async move { core.play(&item, episode.as_deref()).await },
                Message::Notify,
            );
        }
        Msg::SetPlayed(id, played) => {
            let core = app.core.clone();
            return ui::perform(
                async move { core.set_played(&id, played).await },
                Message::Notify,
            );
        }
        Msg::EditEntry(item) => {
            let season = selected_season(app, &item.id);
            anilist::open_editor_for(app, &item, season);
        }
        Msg::Map(item) => {
            let season = selected_season(app, &item.id);
            mapping::open_change_dialog(app, item, season);
        }
        Msg::ConfirmMapping(id) => {
            let season = selected_season(app, &id);
            return mapping::confirm(app, id, season);
        }
    }
    Task::none()
}

/// Überträgt geänderte Zuordnungen auf die geladenen Zeilen, ohne die Bibliothek neu zu laden.
pub fn sync_mappings(app: &mut App) {
    let core = &app.core;
    let mappings = &app.mapping.mappings;
    for row in &mut app.library.rows {
        row.mappings = mappings
            .iter()
            .filter(|m| m.jellyfin_id == row.item.id)
            .cloned()
            .collect();
        row.unsure = is_unsure(&row.mappings);
        row.entry = row
            .mappings
            .first()
            .and_then(|m| core.entry_for(m.media_id));
    }
}

/// Die auf der offenen Detailseite gewählte Staffel dieses Titels.
pub fn selected_season(app: &App, id: &str) -> Option<u32> {
    app.library
        .detail
        .as_ref()
        .filter(|d| d.item.id == id && d.item.kind == ItemKind::Series)
        .and_then(|d| d.season)
}

/// Staffeln, die die offene Detailseite dieses Titels kennt.
pub fn detail_seasons(app: &App, id: &str) -> Vec<u32> {
    app.library
        .detail
        .as_ref()
        .filter(|d| d.item.id == id)
        .map(seasons)
        .unwrap_or_default()
}

/// Staffel der ersten ungesehenen Episode, sonst die erste vorhandene.
fn first_open_season(episodes: &[EpisodeRow]) -> Option<u32> {
    episodes
        .iter()
        .find(|e| !e.episode.played && e.episode.season_number > 0)
        .or(episodes.first())
        .map(|e| e.episode.season_number)
}

fn want(key: ImageKey) -> Msg {
    Msg::WantImage(key)
}

fn choice<'a>(
    options: Vec<String>,
    selected: &'a str,
    wrap: fn(String) -> Filter,
) -> Element<'a, Msg> {
    ui::pick(options, Some(selected.to_owned()), move |v| {
        Msg::Filter(wrap(v))
    })
    .into()
}

fn with_all(label: &str, values: impl IntoIterator<Item = String>) -> Vec<String> {
    std::iter::once(label.to_owned()).chain(values).collect()
}

fn fixed(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

/// Kopf der Bibliothek: Titel, Suche, Bibliotheken als Reiter und Filter.
fn toolbar(state: &State, shown: usize) -> Element<'_, Msg> {
    let libraries = with_all(
        ALL_LIBRARIES,
        state.libraries.iter().map(|l| l.name.clone()),
    );
    let genres = with_all(ALL_GENRES, state.filters.genres.iter().cloned());
    let years = with_all(ALL_YEARS, state.filters.years.iter().map(i32::to_string));
    let search = text_input("Search your library", &state.search)
        .on_input(Msg::SearchChanged)
        .on_submit(Msg::Search)
        .size(theme::T_BODY)
        .padding([8, 12])
        .width(300)
        .style(theme::input);
    let count = match shown {
        1 => "1 title".to_owned(),
        n => format!("{n} titles"),
    };
    let title = row![
        ui::page_title("Library"),
        container(ui::muted(count)).padding(iced::padding::bottom(5)),
        ui::spacer(),
        search,
    ]
    .spacing(theme::S3)
    .align_y(Bottom);
    // Wenige Bibliotheken stehen als Reiter da, viele passen nur in eine Auswahlliste.
    let library: Element<'_, Msg> = if libraries.len() <= 6 {
        ui::tabs(
            libraries.into_iter().map(|name| {
                let active = name == state.library;
                (name.clone(), active, Msg::Filter(Filter::Library(name)))
            }),
            theme::WATCH,
        )
    } else {
        choice(libraries, &state.library, Filter::Library)
    };
    let filters = row![
        row![
            choice(genres, &state.genre, Filter::Genre),
            choice(years, &state.year, Filter::Year),
            choice(fixed(&STATUSES), &state.status, Filter::Status),
            choice(fixed(&TYPES), &state.kind, Filter::Kind),
            ui::chip(
                "Unwatched",
                state.unplayed,
                Msg::Filter(Filter::Unplayed(!state.unplayed)),
                theme::WATCH,
            ),
        ]
        .spacing(theme::S2)
        .align_y(Center)
        .wrap()
        .vertical_spacing(theme::S2),
        ui::spacer(),
        row![
            ui::muted("Sort by"),
            choice(fixed(&SORTS), &state.sort, Filter::Sort),
        ]
        .spacing(theme::S2)
        .align_y(Center),
    ]
    .spacing(theme::S3)
    .align_y(Center);
    column![title, column![library, ui::divider()], filters]
        .spacing(theme::S4)
        .into()
}

/// Rundes Zeichen auf einem Poster.
fn poster_mark<'a>(glyph: Icon, color: Color, background: Color) -> Element<'a, Msg> {
    container(center(ui::icon(glyph, 12.0, color)).width(22).height(22))
        .style(theme::solid(background, theme::PILL))
        .into()
}

/// Kurzform des AniList-Eintrags, z. B. `Watching 2/28`.
fn entry_summary(entry: &ListEntry) -> String {
    format!(
        "{} {}/{}",
        entry.status.label(entry.media.kind),
        entry.progress,
        entry
            .media
            .total_units()
            .map_or("?".to_owned(), |t| t.to_string())
    )
}

fn poster_card<'a>(app: &'a App, row_data: &'a LibraryRow) -> Element<'a, Msg> {
    let item = &row_data.item;
    let key = item.has_poster.then(|| ImageKey::Poster(item.id.clone()));
    let image = ui::picture(&app.images, key, CARD_WIDTH, CARD_HEIGHT, want);
    let progress = item
        .runtime_secs
        .filter(|r| *r > 0 && item.position_secs > 0)
        .map(|r| item.position_secs as f32 / r as f32);
    let image = ui::with_progress(image, progress, (CARD_WIDTH, CARD_HEIGHT), theme::WATCH);
    // Oben rechts der Stand in Jellyfin, unten links die Verbindung zu AniList.
    let watch: Element<'a, Msg> = match item.unplayed_count.filter(|c| *c > 0) {
        Some(count) => ui::badge_row(count.to_string(), theme::WATCH),
        None if item.played => poster_mark(Icon::Check, theme::INK, theme::OK),
        None => Space::new().into(),
    };
    let dark = theme::alpha(theme::INK, 0.82);
    let track: Element<'a, Msg> = if row_data.unsure {
        poster_mark(Icon::Question, theme::WARN, dark)
    } else if !row_data.mappings.is_empty() {
        poster_mark(Icon::Knot, theme::TRACK, dark)
    } else {
        Space::new().into()
    };
    let subtitle: Element<'a, Msg> = match (&row_data.entry, item.year) {
        (Some(entry), _) => row![
            container(Space::new())
                .width(6)
                .height(6)
                .style(theme::solid(theme::status_color(entry.status), 3.0)),
            ui::muted(ui::ellipsize(&entry_summary(entry), 22)),
        ]
        .spacing(6)
        .align_y(Center)
        .into(),
        (None, Some(year)) => ui::muted(year.to_string()),
        (None, None) => ui::muted(""),
    };
    button(
        column![
            stack![
                image,
                container(watch)
                    .width(CARD_WIDTH)
                    .padding(6)
                    .align_right(CARD_WIDTH),
                container(track)
                    .width(CARD_WIDTH)
                    .height(CARD_HEIGHT)
                    .padding(6)
                    .align_y(Bottom),
            ],
            column![
                text(ui::ellipsize(&item.name, 21))
                    .size(theme::T_SMALL)
                    .font(theme::MEDIUM),
                subtitle,
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

/// Die Zeilen, die zur Suche passen.
fn visible(app: &App) -> Vec<&LibraryRow> {
    let needle = app.library.search.trim().to_lowercase();
    app.library
        .rows
        .iter()
        .filter(|r| needle.is_empty() || r.item.name.to_lowercase().contains(&needle))
        .collect()
}

fn grid<'a>(app: &'a App, rows: Vec<&'a LibraryRow>) -> Element<'a, Msg> {
    if rows.is_empty() {
        return if app.auth.jellyfin_user.is_some() {
            ui::empty_state(
                Icon::Search,
                "No titles match",
                "Change the search or clear a filter to see more of your library.",
            )
        } else {
            ui::empty_state(
                Icon::Library,
                "No Jellyfin server yet",
                "Log in to Jellyfin in Settings to browse your library.",
            )
        };
    }
    responsive(move |size| {
        let card = CARD_WIDTH + 2.0 * CARD_PADDING;
        let columns = (((size.width + GAP) / (card + GAP)).floor() as usize).max(1);
        let lines = rows.chunks(columns).map(|chunk| {
            row(chunk.iter().map(|r| poster_card(app, r)))
                .spacing(GAP)
                .into()
        });
        ui::scroll(
            column(lines)
                .spacing(theme::S3)
                .width(Fill)
                .padding(iced::padding::bottom(theme::S6)),
        )
    })
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let rows = visible(app);
    let page: Element<'_, Msg> = column![
        container(toolbar(&app.library, rows.len())).padding(iced::Padding {
            top: theme::S5,
            right: ui::PAGE_PADDING,
            bottom: 0.0,
            left: ui::PAGE_PADDING,
        }),
        // Die Karten haben einen eigenen Rand, deshalb rückt das Raster um diesen ein.
        container(grid(app, rows)).padding(iced::Padding {
            top: 0.0,
            right: ui::PAGE_PADDING - CARD_PADDING,
            bottom: 0.0,
            left: ui::PAGE_PADDING - CARD_PADDING,
        }),
    ]
    .spacing(theme::S4)
    .into();
    page.map(Message::Library)
}

fn episode_row<'a>(
    app: &'a App,
    item: &'a LibraryItem,
    row_data: &'a EpisodeRow,
) -> Element<'a, Msg> {
    let episode = &row_data.episode;
    let key = if episode.has_image {
        ImageKey::Poster(episode.id.clone())
    } else {
        ImageKey::Backdrop(item.id.clone())
    };
    let image = ui::picture(&app.images, Some(key), THUMB.0, THUMB.1, want);
    let progress = episode
        .runtime_secs
        .filter(|r| *r > 0 && episode.position_secs > 0)
        .map(|r| episode.position_secs as f32 / r as f32);
    let thumb = ui::with_progress(image, progress, THUMB, theme::WATCH);
    let mut meta = row![].spacing(theme::S2).align_y(Center);
    if let Some(runtime) = episode.runtime_secs {
        meta = meta.push(ui::muted(format!("{} min", runtime / 60)));
    }
    if episode.played {
        meta = meta.push(ui::tag("Watched", theme::OK));
    }
    if row_data.counted_on_anilist {
        meta = meta.push(ui::tag("On AniList", theme::TRACK));
    }
    let (watched_color, watched_tip) = if episode.played {
        (theme::OK, "Mark as unwatched")
    } else {
        (theme::FAINT, "Mark as watched")
    };
    let play = Msg::Play(item.clone(), Some(episode.id.clone()));
    row![
        button(thumb)
            .padding(0)
            .style(theme::card_button)
            .on_press(play.clone()),
        column![
            row![
                text(episode.number.to_string())
                    .size(theme::T_BODY)
                    .font(theme::DISPLAY)
                    .color(theme::FAINT)
                    .width(28),
                text(ui::ellipsize(&episode.name, 70))
                    .size(theme::T_BODY)
                    .font(theme::SEMIBOLD),
            ]
            .align_y(Center),
            container(
                column![
                    meta,
                    text(ui::ellipsize(
                        episode.overview.as_deref().unwrap_or(""),
                        210
                    ))
                    .size(theme::T_SMALL)
                    .line_height(1.45)
                    .color(theme::MUTED),
                ]
                .spacing(6)
            )
            .padding(iced::padding::left(28))
            .max_width(760),
        ]
        .spacing(6)
        .width(Fill),
        row![
            ui::icon_button(
                Icon::Play,
                16.0,
                theme::PAPER,
                Some(play),
                "Play episode",
                theme::ghost_button,
            ),
            ui::icon_button(
                Icon::CheckCircle,
                18.0,
                watched_color,
                Some(Msg::SetPlayed(episode.id.clone(), !episode.played)),
                watched_tip,
                theme::quiet,
            ),
        ]
        .spacing(theme::S2)
        .align_y(Center),
    ]
    .spacing(theme::S4)
    .padding([theme::S3, 0.0])
    .align_y(Center)
    .into()
}

/// Bewertung mit Stern-Icon, danach die übrigen Angaben.
fn facts_line<'a>(rating: Option<f32>, facts: Vec<String>) -> Element<'a, Msg> {
    let mut line = row![].spacing(theme::S3).align_y(Center);
    if let Some(rating) = rating {
        line = line.push(
            row![
                ui::icon(Icon::Star, 14.0, theme::WARN),
                text(format!("{rating:.1}"))
                    .size(theme::T_BODY)
                    .font(theme::SEMIBOLD)
                    .color(theme::PAPER)
            ]
            .spacing(5)
            .align_y(Center),
        );
    }
    for fact in facts {
        line = line.push(
            text(fact)
                .size(theme::T_BODY)
                .color(theme::alpha(theme::PAPER, 0.78)),
        );
    }
    line.into()
}

fn seasons(detail: &Detail) -> Vec<u32> {
    let mut numbers: Vec<u32> = detail
        .episodes
        .iter()
        .map(|e| e.episode.season_number)
        .collect();
    numbers.sort_by_key(|n| (*n == 0, *n));
    numbers.dedup();
    numbers
}

fn season_tabs(detail: &Detail) -> Element<'_, Msg> {
    ui::tabs(
        seasons(detail).into_iter().map(|number| {
            let label = if number == 0 {
                "Specials".to_owned()
            } else {
                format!("Season {number}")
            };
            (label, detail.season == Some(number), Msg::Season(number))
        }),
        theme::WATCH,
    )
}

/// Streifen mit dem Stand auf AniList und den Aktionen dazu.
fn anilist_summary<'a>(app: &'a App, item: &'a LibraryItem) -> Element<'a, Msg> {
    let row_data = app.library.rows.iter().find(|r| r.item.id == item.id);
    let season = selected_season(app, &item.id);
    // Jede Staffel hat ihren eigenen AniList-Eintrag, gezeigt wird der der gewählten Staffel.
    let season_maps: Vec<&Mapping> = row_data
        .map(|r| match season {
            Some(season) => season_mappings(&r.mappings, season),
            None => r.mappings.iter().collect(),
        })
        .unwrap_or_default();
    let mapping = season_maps.first().copied();
    let unsure = season_maps.iter().any(|m| m.source == MappingSource::Guess);
    let entry = mapping.and_then(|m| app.core.entry_for(m.media_id));
    let cord = if unsure { theme::WARN } else { theme::TRACK };
    let mut summary = row![ui::icon(
        if unsure { Icon::Question } else { Icon::Knot },
        18.0,
        if mapping.is_some() {
            cord
        } else {
            theme::FAINT
        },
    )]
    .spacing(theme::S3)
    .align_y(Center);
    match (mapping, &entry) {
        (Some(mapping), Some(entry)) => {
            let mut facts = row![
                ui::tag(
                    entry.status.label(entry.media.kind),
                    theme::status_color(entry.status)
                ),
                ui::muted(format!(
                    "{} of {}",
                    entry.progress,
                    entry
                        .media
                        .total_units()
                        .map_or("?".to_owned(), |t| t.to_string())
                )),
            ]
            .spacing(theme::S2)
            .align_y(Center);
            if entry.score_raw > 0
                && let Some(viewer) = app.core.viewer()
            {
                facts = facts.push(ui::muted(format!(
                    "Score {}",
                    viewer.score_format.display(entry.score_raw)
                )));
            }
            summary = summary.push(
                column![
                    text(ui::ellipsize(&mapping.title, 48))
                        .size(theme::T_BODY)
                        .font(theme::MEDIUM),
                    facts,
                ]
                .spacing(3),
            );
        }
        (Some(mapping), None) => {
            summary = summary.push(
                column![
                    text(ui::ellipsize(&mapping.title, 48))
                        .size(theme::T_BODY)
                        .font(theme::MEDIUM),
                    ui::muted("Not on your AniList yet"),
                ]
                .spacing(3),
            );
        }
        (None, _) => {
            let partly = season.is_some() && row_data.is_some_and(|r| !r.mappings.is_empty());
            summary = summary.push(ui::muted(if partly {
                "This season has no AniList match"
            } else {
                "No AniList match"
            }));
        }
    }
    let mut actions = row![].spacing(theme::S2).align_y(Center);
    if unsure {
        actions = actions
            .push(ui::tag("Unsure, not synced", theme::WARN))
            .push(ui::primary("Confirm", Msg::ConfirmMapping(item.id.clone())));
    }
    if entry.is_some() {
        actions = actions.push(ui::secondary("Edit entry", Msg::EditEntry(item.clone())));
    }
    actions = actions.push(ui::secondary(
        if mapping.is_some() {
            "Change match…"
        } else {
            "Choose match…"
        },
        Msg::Map(item.clone()),
    ));
    container(
        row![summary, ui::spacer(), actions]
            .spacing(theme::S4)
            .align_y(Center),
    )
    .padding([theme::S3, theme::S4])
    .max_width(820)
    .style(theme::strip(cord))
    .into()
}

pub fn detail_view(app: &App) -> Element<'_, Message> {
    let Some(detail) = &app.library.detail else {
        return Space::new().into();
    };
    let item = &detail.item;
    let backdrop_key = item
        .has_backdrop
        .then(|| ImageKey::Backdrop(item.id.clone()));
    let backdrop = ui::picture(&app.images, backdrop_key, Fill, BACKDROP_HEIGHT, want);
    let poster_key = item.has_poster.then(|| ImageKey::Poster(item.id.clone()));
    let facts: Vec<String> = [
        item.year.map(|y| y.to_string()),
        item.status.clone(),
        item.runtime_secs
            .filter(|_| item.kind == ItemKind::Movie)
            .map(|r| format!("{} min", r / 60)),
        Some(
            item.genres
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        )
        .filter(|g| !g.is_empty()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let next_episode = detail
        .episodes
        .iter()
        .find(|e| !e.episode.played)
        .map(|e| e.episode.id.clone());
    let play_label = if item.kind == ItemKind::Movie || next_episode.is_some() {
        if item.position_secs > 0 {
            "Resume"
        } else {
            "Play"
        }
    } else {
        "Play again"
    };
    let header = row![
        ui::picture(&app.images, poster_key, 184, 276, want),
        column![
            text(ui::ellipsize(&item.name, 70))
                .size(36)
                .font(theme::DISPLAY_BOLD)
                .line_height(1.15),
            facts_line(item.community_rating, facts),
            container(
                text(ui::ellipsize(item.overview.as_deref().unwrap_or(""), 420))
                    .size(theme::T_BODY)
                    .line_height(1.5)
                    .color(theme::alpha(theme::PAPER, 0.82))
            )
            .max_width(720),
            row![
                ui::icon_label(
                    Icon::Play,
                    play_label,
                    Msg::Play(item.clone(), next_episode),
                    theme::watch_pill,
                    theme::INK,
                ),
                ui::icon_button(
                    Icon::CheckCircle,
                    20.0,
                    if item.played { theme::OK } else { theme::PAPER },
                    Some(Msg::SetPlayed(item.id.clone(), !item.played)),
                    if item.played {
                        "Mark as unwatched"
                    } else {
                        "Mark as watched"
                    },
                    theme::ghost_button,
                ),
            ]
            .spacing(theme::S3)
            .align_y(Center),
            anilist_summary(app, item),
        ]
        .spacing(theme::S3)
        .width(Fill),
    ]
    .spacing(theme::S5)
    .align_y(Bottom);
    let layer = |style: fn(&iced::Theme) -> container::Style| {
        container(Space::new())
            .width(Fill)
            .height(BACKDROP_HEIGHT)
            .style(style)
    };
    let hero = stack![
        backdrop,
        layer(theme::hero_side),
        layer(theme::hero_fade),
        column![
            container(ui::icon_button(
                Icon::ArrowBack,
                20.0,
                theme::PAPER,
                Some(Msg::CloseDetail),
                "Back (Esc)",
                theme::ghost_button,
            ))
            .padding([theme::S4, theme::S5]),
            container(header)
                .height(Fill)
                .align_y(Bottom)
                .padding([0.0, ui::PAGE_PADDING]),
        ]
        .height(BACKDROP_HEIGHT),
    ]
    .height(BACKDROP_HEIGHT);
    let mut body = column![hero].spacing(theme::S5);
    if item.kind == ItemKind::Series {
        let episodes: Vec<Element<'_, Msg>> = detail
            .episodes
            .iter()
            .filter(|e| Some(e.episode.season_number) == detail.season)
            .map(|e| episode_row(app, item, e))
            .collect();
        let list: Element<'_, Msg> = if detail.loading && detail.episodes.is_empty() {
            container(ui::muted("Loading episodes…"))
                .padding([theme::S4, 0.0])
                .into()
        } else {
            ui::divided(episodes).into()
        };
        body = body.push(
            container(column![season_tabs(detail), ui::divider(), list])
                .padding([0.0, ui::PAGE_PADDING]),
        );
    }
    let page: Element<'_, Msg> = ui::scroll(body.padding(iced::padding::bottom(theme::S6)));
    page.map(Message::Library)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_reflects_filters_and_ignores_all() {
        let mut state = State::default();
        assert_eq!(state.query(), LibraryQuery::default());
        state.genre = "Fantasy".into();
        state.year = "2023".into();
        state.kind = "Movies".into();
        state.sort = "Date added".into();
        let query = state.query();
        assert_eq!(query.genres, vec!["Fantasy".to_owned()]);
        assert_eq!(query.years, vec![2023]);
        assert_eq!(query.kind, Some(ItemKind::Movie));
        assert_eq!(query.sort, LibrarySort::DateAdded);
    }
}
