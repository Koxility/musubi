//! Bibliotheksraster und Detailseite einer Serie.

use iced::widget::{
    Space, button, column, container, pick_list, progress_bar, responsive, row, scrollable, stack,
    text, text_input, toggler,
};
use iced::{Bottom, Center, Element, Fill, Length, Task};
use musubi_core::domain::{
    ItemKind, Library, LibraryFilters, LibraryItem, LibraryQuery, LibrarySort, Mapping,
    MappingSource, is_unsure, season_mappings,
};
use musubi_core::{EpisodeRow, LibraryRow};

use crate::app::{App, Message, Page};
use crate::icons::Icon;
use crate::images::ImageKey;
use crate::{anilist, mapping, theme, ui};

const CARD_WIDTH: f32 = 170.0;
const CARD_HEIGHT: f32 = 255.0;
const GAP: f32 = 16.0;
const BACKDROP_HEIGHT: f32 = 480.0;
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
    pick_list(options, Some(selected.to_owned()), move |v| {
        Msg::Filter(wrap(v))
    })
    .text_size(13)
    .padding([6, 10])
    .into()
}

fn with_all(label: &str, values: impl IntoIterator<Item = String>) -> Vec<String> {
    std::iter::once(label.to_owned()).chain(values).collect()
}

fn fixed(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

fn toolbar(state: &State) -> Element<'_, Msg> {
    let libraries = with_all(
        ALL_LIBRARIES,
        state.libraries.iter().map(|l| l.name.clone()),
    );
    let genres = with_all(ALL_GENRES, state.filters.genres.iter().cloned());
    let years = with_all(ALL_YEARS, state.filters.years.iter().map(i32::to_string));
    row![
        text_input("Search your library", &state.search)
            .on_input(Msg::SearchChanged)
            .on_submit(Msg::Search)
            .padding(8)
            .width(240)
            .style(theme::input),
        choice(libraries, &state.library, Filter::Library),
        choice(genres, &state.genre, Filter::Genre),
        choice(years, &state.year, Filter::Year),
        choice(fixed(&STATUSES), &state.status, Filter::Status),
        choice(fixed(&TYPES), &state.kind, Filter::Kind),
        choice(fixed(&SORTS), &state.sort, Filter::Sort),
        toggler(state.unplayed)
            .label("Unwatched")
            .text_size(13)
            .on_toggle(|v| Msg::Filter(Filter::Unplayed(v))),
    ]
    .spacing(10)
    .align_y(Center)
    .wrap()
    .vertical_spacing(8)
    .into()
}

fn poster_card<'a>(app: &'a App, row_data: &'a LibraryRow) -> Element<'a, Msg> {
    let item = &row_data.item;
    let key = item.has_poster.then(|| ImageKey::Poster(item.id.clone()));
    let image = ui::picture(&app.images, key, CARD_WIDTH, CARD_HEIGHT, want);
    let mut badges = row![].spacing(4);
    if let Some(count) = item.unplayed_count.filter(|c| *c > 0) {
        badges = badges.push(ui::badge_row(count.to_string(), theme::ACCENT));
    }
    if item.played {
        badges = badges.push(ui::badge_row("✓".to_owned(), theme::GREEN));
    }
    if row_data.unsure {
        badges = badges.push(ui::badge_row("?".to_owned(), theme::ORANGE));
    }
    let subtitle = match (&row_data.entry, item.year) {
        (Some(entry), _) => format!(
            "{} · {}/{}",
            entry.status.label(entry.media.kind),
            entry.progress,
            entry
                .media
                .total_units()
                .map_or("?".to_owned(), |t| t.to_string())
        ),
        (None, Some(year)) => year.to_string(),
        (None, None) => String::new(),
    };
    button(
        column![
            stack![
                image,
                container(badges)
                    .width(CARD_WIDTH)
                    .padding(6)
                    .align_right(CARD_WIDTH),
            ],
            text(ui::ellipsize(&item.name, 22)).size(14),
            ui::muted(ui::ellipsize(&subtitle, 26)),
        ]
        .spacing(5),
    )
    .padding(4)
    .style(theme::card_button)
    .on_press(Msg::Open(item.clone()))
    .into()
}

fn grid(app: &App) -> Element<'_, Msg> {
    let needle = app.library.search.trim().to_lowercase();
    let rows: Vec<&LibraryRow> = app
        .library
        .rows
        .iter()
        .filter(|r| needle.is_empty() || r.item.name.to_lowercase().contains(&needle))
        .collect();
    if rows.is_empty() {
        return container(ui::muted(if app.auth.jellyfin_user.is_some() {
            "No titles match these filters."
        } else {
            "Log in to Jellyfin in Settings to see your library."
        }))
        .padding(32)
        .into();
    }
    responsive(move |size| {
        let columns = (((size.width + GAP) / (CARD_WIDTH + 8.0 + GAP)).floor() as usize).max(1);
        let lines = rows.chunks(columns).map(|chunk| {
            row(chunk.iter().map(|r| poster_card(app, r)))
                .spacing(GAP)
                .into()
        });
        scrollable(column(lines).spacing(GAP).width(Fill))
            .width(Fill)
            .height(Fill)
            .into()
    })
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let page: Element<'_, Msg> = column![toolbar(&app.library), grid(app)]
        .spacing(20)
        .padding(24)
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
    let image = ui::picture(&app.images, Some(key), 200, 112, want);
    let progress = episode
        .runtime_secs
        .filter(|r| *r > 0 && episode.position_secs > 0)
        .map(|r| episode.position_secs as f32 / r as f32);
    let thumb: Element<'a, Msg> = match progress {
        Some(value) => stack![
            image,
            container(
                progress_bar(0.0..=1.0, value.min(1.0))
                    .length(Length::Fixed(200.0))
                    .girth(4)
            )
            .height(112)
            .align_y(Bottom),
        ]
        .into(),
        None => image,
    };
    let mut meta = row![ui::muted(
        episode
            .runtime_secs
            .map(|r| format!("{} min", r / 60))
            .unwrap_or_default()
    )]
    .spacing(8);
    if row_data.counted_on_anilist {
        meta = meta.push(ui::badge_row("AniList".to_owned(), theme::GREEN));
    }
    if episode.played {
        meta = meta.push(ui::badge_row("Watched".to_owned(), theme::ACCENT));
    }
    let (watched_icon, watched_color, watched_tip) = if episode.played {
        (Icon::CheckCircle, theme::ACCENT, "Mark as unwatched")
    } else {
        (Icon::CheckCircle, theme::MUTED, "Mark as watched")
    };
    container(
        row![
            button(thumb)
                .padding(0)
                .style(theme::card_button)
                .on_press(Msg::Play(item.clone(), Some(episode.id.clone()))),
            column![
                text(format!(
                    "{}. {}",
                    episode.number,
                    ui::ellipsize(&episode.name, 70)
                ))
                .size(16),
                meta,
                ui::muted(ui::ellipsize(
                    episode.overview.as_deref().unwrap_or(""),
                    220
                )),
            ]
            .spacing(6)
            .width(Fill),
            row![
                ui::icon_button(
                    Icon::Play,
                    20.0,
                    theme::TEXT,
                    Some(Msg::Play(item.clone(), Some(episode.id.clone()))),
                    "Play episode",
                    theme::ghost_button,
                ),
                ui::icon_button(
                    watched_icon,
                    22.0,
                    watched_color,
                    Some(Msg::SetPlayed(episode.id.clone(), !episode.played)),
                    watched_tip,
                    theme::ghost_button,
                ),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .align_y(Center),
    )
    .padding(10)
    .style(theme::surface)
    .into()
}

/// Bewertung mit Stern-Icon, danach die übrigen Angaben.
fn facts_line<'a>(rating: Option<f32>, facts: String) -> Element<'a, Msg> {
    let mut line = row![].spacing(6).align_y(Center);
    if let Some(rating) = rating {
        line = line
            .push(ui::icon(Icon::Star, 14.0, theme::ORANGE))
            .push(text(format!("{rating:.1}")).size(13).color(theme::TEXT));
        if !facts.is_empty() {
            line = line.push(ui::muted("·"));
        }
    }
    line.push(ui::muted(facts)).into()
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
    let tabs = seasons(detail).into_iter().map(|number| {
        let label = if number == 0 {
            "Specials".to_owned()
        } else {
            format!("Season {number}")
        };
        button(text(label).size(14))
            .padding([8, 14])
            .style(theme::tab(detail.season == Some(number)))
            .on_press(Msg::Season(number))
            .into()
    });
    row(tabs).spacing(6).wrap().into()
}

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
    let summary = match (mapping, &entry) {
        (Some(mapping), Some(entry)) => format!(
            "AniList: {} · {} {}/{}",
            ui::ellipsize(&mapping.title, 44),
            entry.status.label(entry.media.kind),
            entry.progress,
            entry
                .media
                .total_units()
                .map_or("?".to_owned(), |t| t.to_string())
        ),
        (Some(mapping), None) => format!(
            "AniList: {} · not on your list",
            ui::ellipsize(&mapping.title, 44)
        ),
        (None, _) if season.is_some() && row_data.is_some_and(|r| !r.mappings.is_empty()) => {
            "AniList: this season has no match".to_owned()
        }
        (None, _) => "AniList: no match".to_owned(),
    };
    let mut actions = row![ui::muted(summary)].spacing(10).align_y(Center);
    if unsure {
        actions = actions
            .push(ui::badge_row(
                "? unsure, not synced".to_owned(),
                theme::ORANGE,
            ))
            .push(ui::primary("Confirm", Msg::ConfirmMapping(item.id.clone())));
    }
    if entry.is_some() {
        actions = actions.push(ui::secondary("Edit entry", Msg::EditEntry(item.clone())));
    }
    actions
        .push(ui::secondary("Change…", Msg::Map(item.clone())))
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
    let facts = [
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
    .collect::<Vec<_>>()
    .join("  ·  ");
    let next_episode = detail
        .episodes
        .iter()
        .find(|e| !e.episode.played)
        .map(|e| e.episode.id.clone());
    let play_label = if item.kind == ItemKind::Movie || next_episode.is_some() {
        "Play"
    } else {
        "Play again"
    };
    let header = container(
        row![
            ui::picture(&app.images, poster_key, 200, 300, want),
            column![
                text(ui::ellipsize(&item.name, 70))
                    .size(34)
                    .font(ui::SEMIBOLD),
                facts_line(item.community_rating, facts),
                container(
                    text(ui::ellipsize(item.overview.as_deref().unwrap_or(""), 520)).size(15)
                )
                .max_width(760),
                row![
                    ui::icon_label(
                        Icon::Play,
                        play_label,
                        Msg::Play(item.clone(), next_episode),
                        theme::primary_pill,
                        theme::BG,
                    ),
                    ui::icon_button(
                        Icon::CheckCircle,
                        22.0,
                        if item.played {
                            theme::ACCENT
                        } else {
                            theme::TEXT
                        },
                        Some(Msg::SetPlayed(item.id.clone(), !item.played)),
                        if item.played {
                            "Mark as unwatched"
                        } else {
                            "Mark as watched"
                        },
                        theme::ghost_button,
                    ),
                ]
                .spacing(12)
                .align_y(Center),
                anilist_summary(app, item),
            ]
            .spacing(12)
            .width(Fill),
        ]
        .spacing(24)
        .align_y(Bottom),
    )
    .padding(32);
    let hero = stack![
        backdrop,
        container(Space::new())
            .width(Fill)
            .height(BACKDROP_HEIGHT)
            .style(theme::hero_fade),
        column![
            container(ui::icon_button(
                Icon::ArrowBack,
                22.0,
                theme::TEXT,
                Some(Msg::CloseDetail),
                "Back (Esc)",
                theme::ghost_button,
            ))
            .padding(16),
            container(header).height(Fill).align_y(Bottom),
        ]
        .height(BACKDROP_HEIGHT),
    ]
    .height(BACKDROP_HEIGHT);
    let mut body = column![hero].spacing(18);
    if item.kind == ItemKind::Series {
        let episodes = detail
            .episodes
            .iter()
            .filter(|e| Some(e.episode.season_number) == detail.season)
            .map(|e| episode_row(app, item, e));
        let list: Element<'_, Msg> = if detail.loading && detail.episodes.is_empty() {
            ui::muted("Loading episodes…")
        } else {
            column(episodes).spacing(10).into()
        };
        body =
            body.push(container(column![season_tabs(detail), list].spacing(14)).padding([0, 32]));
    }
    let page: Element<'_, Msg> = scrollable(body.padding(iced::padding::bottom(28)))
        .width(Fill)
        .height(Fill)
        .into();
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
