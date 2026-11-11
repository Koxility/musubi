//! Automatische Zuordnungen ansehen und nachträglich ändern.

use std::fmt;

use iced::widget::{Column, column, container, pick_list, row, text, text_input};
use iced::{Center, Element, Fill, Task};
use musubi_core::domain::{LibraryItem, Mapping, MappingSource, Media, MediaKind};
use musubi_core::{ReviewCandidate, ReviewItem};

use crate::app::{App, Message};
use crate::icons::Icon;
use crate::{library, theme, ui};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MappingForm {
    pub media: String,
    pub season: String,
    pub start: String,
    pub end: String,
    pub offset: String,
}

impl MappingForm {
    pub fn new(mapping: &Mapping) -> Self {
        Self {
            media: Some(mapping.media_id)
                .filter(|id| *id != 0)
                .map(|id| id.to_string())
                .unwrap_or_default(),
            season: mapping.season.to_string(),
            start: mapping.ep_start.to_string(),
            end: mapping.ep_end.map(|e| e.to_string()).unwrap_or_default(),
            offset: mapping.offset.to_string(),
        }
    }

    pub fn build(&self, original: &Mapping) -> Result<Mapping, String> {
        let whole = |value: &str, name: &str| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("{name} must be a whole number"))
        };
        let last = match self.end.trim() {
            "" => None,
            value => Some(whole(value, "Last episode")?),
        };
        let first = whole(&self.start, "First episode")?;
        if last.is_some_and(|l| l < first) {
            return Err("The last episode must not be before the first.".to_owned());
        }
        Ok(Mapping {
            jellyfin_id: original.jellyfin_id.clone(),
            season: whole(&self.season, "Season")?,
            ep_start: first,
            ep_end: last,
            media_id: self
                .media
                .trim()
                .parse()
                .map_err(|_| "The ID must be a number".to_owned())?,
            offset: self
                .offset
                .trim()
                .parse()
                .map_err(|_| "Offset must be a whole number".to_owned())?,
            source: MappingSource::Manual,
            confidence: 1.0,
            title: original.title.clone(),
        })
    }
}

/// Welche Staffeln der Ändern-Dialog betrifft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope(pub Option<u32>);

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => f.write_str("All seasons"),
            Some(0) => f.write_str("Specials"),
            Some(n) => write!(f, "Season {n}"),
        }
    }
}

pub enum Modal {
    Change {
        item: Box<LibraryItem>,
        scope: Scope,
        seasons: Vec<u32>,
        query: String,
        results: Vec<Media>,
        searching: bool,
        id: String,
        follow: bool,
        error: Option<String>,
    },
    Edit {
        original: Box<Mapping>,
        form: MappingForm,
        error: Option<String>,
    },
}

#[derive(Default)]
pub struct State {
    reviews: Vec<ReviewItem>,
    pub mappings: Vec<Mapping>,
    progress: Option<(usize, usize)>,
    modal: Option<Modal>,
}

impl State {
    pub fn close_modal(&mut self) -> bool {
        self.modal.take().is_some()
    }

    pub fn set_progress(&mut self, done: usize, total: usize) {
        self.progress = (done < total).then_some((done, total));
    }

    pub fn progress(&self) -> Option<(usize, usize)> {
        self.progress
    }
}

#[derive(Debug, Clone)]
pub enum FormEdit {
    Media(String),
    Season(String),
    Start(String),
    End(String),
    Offset(String),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Result<(Vec<ReviewItem>, Vec<Mapping>), String>),
    MatchAgain,
    OpenChange(Box<LibraryItem>, Option<u32>),
    Scope(Scope),
    Query(String),
    Search,
    Results(Result<Vec<Media>, String>),
    MapId(String),
    MapFollow(bool),
    UseMedia(i64),
    SubmitId,
    Confirm(String, Option<u32>),
    OpenEdit(Box<Mapping>),
    Edit(FormEdit),
    SubmitEdit,
    Unmap(String),
    /// Hands a title back to automatic matching, replacing whatever it is matched to now.
    AutoMatch(Box<LibraryItem>),
    UnmapSeason(String, u32),
    Close,
}

pub fn load(app: &App) -> Task<Message> {
    let core = app.core.clone();
    ui::perform(
        async move { Ok((core.reviews().await?, core.all_mappings().await?)) },
        |r| Message::Mapping(Msg::Loaded(r)),
    )
}

/// Ordnet neue Titel im Hintergrund zu, sobald beide Dienste angemeldet sind.
pub fn auto_match(app: &App) -> Task<Message> {
    if app.auth.jellyfin_user.is_none() || !app.auth.anilist_logged_in {
        return Task::none();
    }
    let core = app.core.clone();
    ui::perform(async move { core.match_all(false).await }, Message::Notify)
}

/// Suchtext für eine Staffel. Spätere Staffeln nennen ihre Nummer, sonst findet AniList zuerst
/// den Anfang der Serie.
fn query_for(item: &LibraryItem, season: Option<u32>) -> String {
    match season {
        Some(n) if n > 1 => format!("{} Season {n}", item.name),
        _ => item.name.clone(),
    }
}

/// Staffeln, die der Dialog anbietet: bekannte Zuordnungen und die geöffnete Detailseite.
fn known_seasons(app: &App, item: &LibraryItem, season: Option<u32>) -> Vec<u32> {
    let mut seasons: Vec<u32> = app
        .mapping
        .mappings
        .iter()
        .filter(|m| m.jellyfin_id == item.id)
        .map(|m| m.season)
        .chain(library::detail_seasons(app, &item.id))
        .chain(season)
        .collect();
    seasons.sort_by_key(|n| (*n == 0, *n));
    seasons.dedup();
    seasons
}

/// Öffnet den Ändern-Dialog. `season` wählt eine einzelne Staffel vor, `None` alle.
pub fn open_change_dialog(app: &mut App, item: LibraryItem, season: Option<u32>) {
    let seasons = known_seasons(app, &item, season);
    // Ein Film oder eine Serie mit nur einer Staffel braucht keine Auswahl.
    let season = season.filter(|_| seasons.len() > 1 || season.is_some_and(|n| n != 1));
    app.mapping.modal = Some(Modal::Change {
        query: query_for(&item, season),
        item: Box::new(item),
        scope: Scope(season),
        seasons,
        results: vec![],
        searching: false,
        id: String::new(),
        follow: true,
        error: None,
    });
}

/// Macht aus unsicheren Treffern feste Zuordnungen, mit `season` nur für diese Staffel.
pub fn confirm(app: &App, id: String, season: Option<u32>) -> Task<Message> {
    let core = app.core.clone();
    ui::perform(
        async move { core.confirm_season(&id, season).await },
        Message::Notify,
    )
}

/// Schließt den Ändern-Dialog und ordnet das gewählte Werk zu.
fn use_media(app: &mut App, media: i64) -> Task<Message> {
    let Some(Modal::Change {
        item,
        follow,
        scope,
        ..
    }) = app.mapping.modal.take()
    else {
        return Task::none();
    };
    let core = app.core.clone();
    ui::perform(
        async move {
            core.map_season_manually(&item, scope.0, media, follow)
                .await
        },
        Message::Notify,
    )
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let core = app.core.clone();
    match msg {
        Msg::Loaded(Ok((reviews, mappings))) => {
            app.mapping.reviews = reviews;
            app.mapping.mappings = mappings;
            library::sync_mappings(app);
        }
        Msg::Loaded(Err(error)) => app.toast(error, false),
        Msg::MatchAgain => {
            return ui::perform(async move { core.match_all(true).await }, Message::Notify);
        }
        Msg::OpenChange(item, season) => open_change_dialog(app, *item, season),
        Msg::Scope(chosen) => {
            if let Some(Modal::Change {
                scope, query, item, ..
            }) = &mut app.mapping.modal
            {
                // Nur einen unveränderten Vorschlag anpassen, eigene Eingaben bleiben.
                if *query == query_for(item, scope.0) {
                    *query = query_for(item, chosen.0);
                }
                *scope = chosen;
            }
        }
        Msg::Query(value) => {
            if let Some(Modal::Change { query, .. }) = &mut app.mapping.modal {
                *query = value;
            }
        }
        Msg::Search => {
            if let Some(Modal::Change {
                query, searching, ..
            }) = &mut app.mapping.modal
            {
                *searching = true;
                let text = query.clone();
                return ui::perform(
                    async move { core.search_anilist(&text, MediaKind::Anime).await },
                    |r| Message::Mapping(Msg::Results(r)),
                );
            }
        }
        Msg::Results(result) => {
            if let Some(Modal::Change {
                results,
                searching,
                error,
                ..
            }) = &mut app.mapping.modal
            {
                *searching = false;
                match result {
                    Ok(found) => *results = found,
                    Err(message) => *error = Some(message),
                }
            }
        }
        Msg::MapId(value) => {
            if let Some(Modal::Change { id, .. }) = &mut app.mapping.modal {
                *id = value;
            }
        }
        Msg::MapFollow(value) => {
            if let Some(Modal::Change { follow, .. }) = &mut app.mapping.modal {
                *follow = value;
            }
        }
        Msg::UseMedia(media) => return use_media(app, media),
        Msg::SubmitId => {
            if let Some(Modal::Change { id, error, .. }) = &mut app.mapping.modal {
                match id.trim().parse::<i64>() {
                    Ok(media) => return use_media(app, media),
                    Err(_) => *error = Some("The ID must be a number.".to_owned()),
                }
            }
        }
        Msg::Confirm(id, season) => return confirm(app, id, season),
        Msg::OpenEdit(mapping) => {
            app.mapping.modal = Some(Modal::Edit {
                form: MappingForm::new(&mapping),
                original: mapping,
                error: None,
            });
        }
        Msg::Edit(edit) => {
            if let Some(Modal::Edit { form, .. }) = &mut app.mapping.modal {
                match edit {
                    FormEdit::Media(v) => form.media = v,
                    FormEdit::Season(v) => form.season = v,
                    FormEdit::Start(v) => form.start = v,
                    FormEdit::End(v) => form.end = v,
                    FormEdit::Offset(v) => form.offset = v,
                }
            }
        }
        Msg::SubmitEdit => {
            let siblings = app.mapping.mappings.clone();
            if let Some(Modal::Edit {
                original,
                form,
                error,
            }) = &mut app.mapping.modal
            {
                match form.build(original) {
                    Ok(mut updated) => {
                        // The stored title belongs to the old entry.
                        if updated.media_id != original.media_id {
                            updated.title = core.entry_for(updated.media_id).map_or_else(
                                || format!("AniList entry {}", updated.media_id),
                                |entry| entry.media.title.preferred().to_owned(),
                            );
                        }
                        let id = original.jellyfin_id.clone();
                        let mut all: Vec<Mapping> = siblings
                            .into_iter()
                            .filter(|m| m.jellyfin_id == id)
                            .collect();
                        match all.iter_mut().find(|m| {
                            m.season == original.season && m.ep_start == original.ep_start
                        }) {
                            Some(slot) => *slot = updated,
                            None => all.push(updated),
                        }
                        app.mapping.modal = None;
                        return ui::perform(
                            async move { core.set_mappings(&id, all).await },
                            Message::Notify,
                        );
                    }
                    Err(message) => *error = Some(message),
                }
            }
        }
        Msg::AutoMatch(item) => {
            app.mapping.modal = None;
            return ui::perform(
                async move { core.auto_match(&item).await.map(|_| ()) },
                Message::Notify,
            );
        }
        Msg::Unmap(id) => {
            app.mapping.modal = None;
            return ui::perform(async move { core.unmap(&id).await }, Message::Notify);
        }
        Msg::UnmapSeason(id, season) => {
            app.mapping.modal = None;
            return ui::perform(
                async move { core.unmap_season(&id, season).await },
                Message::Notify,
            );
        }
        Msg::Close => app.mapping.modal = None,
    }
    Task::none()
}

fn item_for<'a>(app: &'a App, id: &str) -> Option<&'a LibraryItem> {
    app.library
        .rows
        .iter()
        .find(|r| r.item.id == id)
        .map(|r| &r.item)
}

fn season_label(season: u32) -> String {
    Scope(Some(season)).to_string()
}

/// Eine Zeile pro Staffel: Bereich, Werk und Aktionen nur für diese Staffel.
fn season_line<'a>(
    item: Option<&'a LibraryItem>,
    season: u32,
    mappings: &[&'a Mapping],
) -> Element<'a, Msg> {
    let parts: Vec<String> = mappings.iter().map(|m| range_label(m)).collect();
    let unsure = mappings.iter().any(|m| m.source == MappingSource::Guess);
    let mut heading = row![
        text(season_label(season))
            .size(theme::T_SMALL)
            .font(theme::MEDIUM)
            .width(90)
    ]
    .spacing(theme::S2)
    .align_y(Center);
    heading = heading.push(
        column(parts.into_iter().map(ui::muted))
            .spacing(2)
            .width(Fill),
    );
    if unsure {
        heading = heading.push(ui::tag("Unsure, not synced", theme::WARN));
    }
    let id = mappings
        .first()
        .map(|m| m.jellyfin_id.clone())
        .unwrap_or_default();
    if unsure {
        heading = heading.push(ui::primary(
            "Confirm",
            Msg::Confirm(id.clone(), Some(season)),
        ));
    }
    heading = heading.push(match (item, mappings.first()) {
        (Some(item), _) => ui::secondary(
            "Change…",
            Msg::OpenChange(Box::new(item.clone()), Some(season)),
        ),
        (None, Some(first)) => ui::secondary("Edit…", Msg::OpenEdit(Box::new((*first).clone()))),
        (None, None) => ui::muted(""),
    });
    heading
        .push(ui::icon_button(
            Icon::Trash,
            15.0,
            theme::BAD,
            Some(Msg::UnmapSeason(id, season)),
            "Remove this match",
            theme::quiet,
        ))
        .padding(iced::padding::left(30))
        .into()
}

/// Karte eines Titels mit einer Zeile je zugeordneter Staffel.
fn mapped_title<'a>(app: &'a App, id: &'a str, mappings: Vec<&'a Mapping>) -> Element<'a, Msg> {
    let item = item_for(app, id);
    let name = item.map_or(id, |i| i.name.as_str());
    let mut seasons: Vec<u32> = mappings.iter().map(|m| m.season).collect();
    seasons.sort_by_key(|n| (*n == 0, *n));
    seasons.dedup();
    let lines = seasons.into_iter().map(|season| {
        let of_season: Vec<&Mapping> = mappings
            .iter()
            .copied()
            .filter(|m| m.season == season)
            .collect();
        season_line(item, season, &of_season)
    });
    let unsure = mappings.iter().any(|m| m.source == MappingSource::Guess);
    let mark = if unsure {
        ui::icon(Icon::Question, 18.0, theme::WARN)
    } else {
        ui::icon(Icon::Knot, 18.0, theme::TRACK)
    };
    let mut heading = row![
        mark,
        text(ui::ellipsize(name, 64))
            .size(theme::T_BODY)
            .font(theme::SEMIBOLD)
            .width(Fill)
    ]
    .spacing(theme::S3)
    .align_y(Center);
    if let Some(item) = item {
        heading = heading.push(ui::secondary(
            "All seasons…",
            Msg::OpenChange(Box::new(item.clone()), None),
        ));
    }
    container(column![heading, column(lines).spacing(theme::S1)].spacing(theme::S2))
        .padding([theme::S3, theme::S2])
        .into()
}

fn unmatched_row(item: &LibraryItem) -> Element<'_, Msg> {
    row![
        ui::icon(Icon::Link, 18.0, theme::FAINT),
        text(ui::ellipsize(&item.name, 80))
            .size(theme::T_BODY)
            .font(theme::MEDIUM)
            .width(Fill),
        ui::secondary(
            "Choose match…",
            Msg::OpenChange(Box::new(item.clone()), None)
        ),
    ]
    .spacing(theme::S3)
    .padding([theme::S2, theme::S2])
    .align_y(Center)
    .into()
}

fn group<'a>(rows: Vec<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(ui::divided(rows))
        .padding([theme::S1, theme::S2])
        .width(Fill)
        .style(theme::surface)
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let state = &app.mapping;
    let mut body = column![ui::page_header(
        "Mapping",
        Some(
            "Jellyfin titles are matched to AniList automatically. Fix a match here or on the title's page; unsure matches are not synced until you confirm them."
                .to_owned()
        ),
        vec![ui::tool(
            Icon::Sync,
            "Match again",
            Some(Msg::MatchAgain),
            theme::secondary,
            theme::PAPER
        )],
    )]
    .spacing(theme::S5)
    .padding(iced::Padding {
        top: theme::S5,
        right: ui::PAGE_PADDING,
        bottom: theme::S6,
        left: ui::PAGE_PADDING,
    });
    if let Some((done, total)) = state.progress {
        body = body.push(
            row![
                ui::bar(done as f32 / total.max(1) as f32, theme::TRACK, 6.0),
                ui::muted(format!("Matching {done} of {total}")),
            ]
            .spacing(theme::S3)
            .align_y(Center),
        );
    }
    let unmatched: Vec<Element<'_, Msg>> = app
        .library
        .rows
        .iter()
        .filter(|r| r.mappings.is_empty())
        .map(|r| unmatched_row(&r.item))
        .collect();
    if !unmatched.is_empty() {
        let count = unmatched.len();
        body = body.push(ui::section(
            ui::heading_count("Without a match", count),
            group(unmatched),
        ));
    }
    // Titel mit unsicheren Staffeln zuerst, sonst in der Reihenfolge der Datenbank.
    let mut titles: Vec<(&str, Vec<&Mapping>)> = Vec::new();
    for mapping in &state.mappings {
        match titles.iter_mut().find(|(id, _)| *id == mapping.jellyfin_id) {
            Some((_, list)) => list.push(mapping),
            None => titles.push((&mapping.jellyfin_id, vec![mapping])),
        }
    }
    titles.sort_by_key(|(_, list)| !list.iter().any(|m| m.source == MappingSource::Guess));
    let mapped: Vec<Element<'_, Msg>> = titles
        .into_iter()
        .map(|(id, list)| mapped_title(app, id, list))
        .collect();
    let count = mapped.len();
    let matched: Element<'_, Msg> = if mapped.is_empty() {
        ui::muted("Nothing matched yet. Matches appear here once both services are connected.")
    } else {
        group(mapped)
    };
    body = body.push(ui::section(
        ui::heading_count("Matched titles", count),
        matched,
    ));
    let page: Element<'_, Msg> = ui::scroll(body);
    page.map(Message::Mapping)
}

fn choice_line<'a>(title: &str, details: String, media: i64) -> Element<'a, Msg> {
    row![
        column![
            text(ui::ellipsize(title, 56))
                .size(theme::T_BODY)
                .font(theme::MEDIUM),
            ui::muted(details),
        ]
        .spacing(2)
        .width(Fill),
        ui::primary("Use this", Msg::UseMedia(media)),
    ]
    .spacing(theme::S3)
    .padding([6.0, 0.0])
    .align_y(Center)
    .into()
}

fn candidate_line<'a>(candidate: &ReviewCandidate) -> Element<'a, Msg> {
    let details = format!(
        "{}, {}, {} episodes, {:.0}% match",
        candidate.format.as_deref().unwrap_or("?"),
        candidate.year.map_or("?".to_owned(), |y| y.to_string()),
        candidate.episodes.map_or("?".to_owned(), |e| e.to_string()),
        candidate.score * 100.0
    );
    choice_line(&candidate.title, details, candidate.media_id)
}

fn result_line<'a>(media: &Media) -> Element<'a, Msg> {
    let details = format!(
        "{}, {}, {} episodes",
        media.format.as_deref().unwrap_or("?"),
        media.season_year.map_or("?".to_owned(), |y| y.to_string()),
        media.episodes.map_or("?".to_owned(), |e| e.to_string()),
    );
    choice_line(media.title.preferred(), details, media.id)
}

struct ChangeView<'a> {
    item: &'a LibraryItem,
    scope: Scope,
    seasons: &'a [u32],
    query: &'a str,
    results: &'a [Media],
    searching: bool,
    id: &'a str,
    follow: bool,
    error: Option<&'a str>,
}

fn change_body<'a>(app: &'a App, view: ChangeView<'a>) -> Column<'a, Msg> {
    let state = &app.mapping;
    let current: Vec<&Mapping> = state
        .mappings
        .iter()
        .filter(|m| m.jellyfin_id == view.item.id)
        .filter(|m| view.scope.0.is_none_or(|s| m.season == s))
        .collect();
    let mut titles: Vec<&str> = current.iter().map(|m| m.title.as_str()).collect();
    titles.dedup();
    let summary = if titles.is_empty() {
        "Not matched yet.".to_owned()
    } else {
        format!("Currently: {}", ui::ellipsize(&titles.join(", "), 80))
    };
    let label = |name: &'a str| {
        text(name)
            .size(theme::T_SMALL)
            .font(theme::SEMIBOLD)
            .color(theme::PAPER)
    };
    let mut body = column![].spacing(theme::S3);
    if view.seasons.len() > 1 || view.scope.0.is_some() {
        let mut choices = vec![Scope(None)];
        choices.extend(view.seasons.iter().map(|s| Scope(Some(*s))));
        body = body.push(
            row![
                ui::muted("Apply to"),
                pick_list(choices, Some(view.scope), Msg::Scope)
                    .text_size(theme::T_SMALL)
                    .padding([7, 12])
                    .style(theme::pick)
                    .menu_style(theme::pick_menu),
            ]
            .spacing(theme::S3)
            .align_y(Center),
        );
    }
    body = body.push(ui::muted(summary));
    // Die gespeicherten Vorschläge stammen aus der Suche nach dem Serientitel.
    let fits_suggestions = view.scope.0.is_none_or(|s| s <= 1);
    let suggestions: Vec<Element<'a, Msg>> = state
        .reviews
        .iter()
        .filter(|_| fits_suggestions)
        .find(|r| r.jellyfin_id == view.item.id)
        .map(|r| r.candidates.iter().map(candidate_line).collect())
        .unwrap_or_default();
    if !suggestions.is_empty() {
        body = body
            .push(label("Suggestions"))
            .push(ui::divided(suggestions));
    }
    body = body.push(label("Search AniList")).push(
        row![
            text_input("Title", view.query)
                .on_input(Msg::Query)
                .on_submit(Msg::Search)
                .size(theme::T_BODY)
                .padding([8, 10])
                .style(theme::input),
            ui::secondary("Search", Msg::Search),
        ]
        .spacing(theme::S2)
        .align_y(Center),
    );
    if view.searching {
        body = body.push(ui::muted("Searching AniList…"));
    } else if !view.results.is_empty() {
        body = body.push(ui::divided(view.results.iter().map(result_line).collect()));
    }
    body = body
        .push(
            row![
                container(ui::field(
                    "Or enter the AniList ID (the number in the URL)",
                    view.id,
                    Msg::MapId
                ))
                .width(Fill),
                ui::secondary("Use ID", Msg::SubmitId),
            ]
            .spacing(theme::S2)
            .align_y(iced::Bottom),
        )
        .push(ui::toggle(
            if view.scope.0.is_some() {
                "Split the season over several AniList entries"
            } else {
                "Assign sequels to later seasons"
            },
            view.follow,
            Msg::MapFollow,
        ));
    if let Some(error) = view.error {
        body = body.push(ui::error_line(error));
    }
    let mut actions = row![ui::with_tip(
        ui::secondary(
            "Match automatically",
            Msg::AutoMatch(Box::new(view.item.clone())),
        ),
        "Matches every season of this title again and replaces its current matches",
    )]
    .spacing(theme::S2)
    .align_y(Center);
    if let Some(first) = current.first() {
        let remove = match view.scope.0 {
            Some(season) => Msg::UnmapSeason(view.item.id.clone(), season),
            None => Msg::Unmap(view.item.id.clone()),
        };
        actions = actions.push(ui::danger("Remove match", remove));
        match view.scope.0 {
            // One season: every entry of it can be edited, and another one added after the last.
            Some(season) => {
                let lines = current.iter().map(|mapping| {
                    row![
                        ui::muted(range_label(mapping)),
                        ui::spacer(),
                        ui::secondary("Edit…", Msg::OpenEdit(Box::new((*mapping).clone()))),
                    ]
                    .spacing(theme::S2)
                    .align_y(Center)
                    .into()
                });
                body = body
                    .push(label("Entries of this season"))
                    .push(ui::divided(lines.collect()));
                actions = actions.push(ui::secondary(
                    "Add entry…",
                    Msg::OpenEdit(Box::new(entry_after(&view.item.id, season, &current))),
                ));
            }
            None => {
                actions = actions.push(ui::secondary(
                    "Exact editor…",
                    Msg::OpenEdit(Box::new((*first).clone())),
                ));
            }
        }
    }
    body.push(ui::divider()).push(
        actions
            .push(ui::spacer())
            .push(ui::secondary("Close", Msg::Close)),
    )
}

/// The AniList episodes a match stands for, e.g. "episodes 21–41". `None` when they are the
/// Jellyfin numbers anyway, so only matches that count differently say so.
pub fn anilist_range(mapping: &Mapping) -> Option<String> {
    if mapping.offset == 0 {
        return None;
    }
    let first = mapping.anilist_episode(mapping.ep_start)?;
    Some(
        match mapping.ep_end.and_then(|end| mapping.anilist_episode(end)) {
            Some(last) if last == first => format!("episode {first}"),
            Some(last) => format!("episodes {first}–{last}"),
            None => format!("from episode {first}"),
        },
    )
}

/// One match in words, the same wherever it is shown: the Jellyfin episodes, the AniList
/// entry and, where they differ, the entry's episodes. A season in the middle of a long
/// entry would otherwise read as if it started at the entry's first episode.
pub fn range_label(mapping: &Mapping) -> String {
    let label = format!(
        "E{}–{} → {}",
        mapping.ep_start,
        mapping.ep_end.map_or("…".to_owned(), |e| e.to_string()),
        ui::ellipsize(&mapping.title, 54),
    );
    match anilist_range(mapping) {
        Some(range) => format!("{label}, {range}"),
        None => label,
    }
}

/// Template for one more AniList entry in a season, starting after its last mapped episode.
/// The AniList ID 0 stands for "not chosen yet".
fn entry_after(jellyfin_id: &str, season: u32, current: &[&Mapping]) -> Mapping {
    let start = current
        .iter()
        .filter(|m| m.season == season)
        .map(|m| m.ep_end.unwrap_or(m.ep_start) + 1)
        .max()
        .unwrap_or(1);
    Mapping {
        jellyfin_id: jellyfin_id.to_owned(),
        season,
        ep_start: start,
        ep_end: None,
        media_id: 0,
        // The new entry counts from its own first episode.
        offset: 1 - start as i32,
        source: MappingSource::Manual,
        confidence: 1.0,
        title: String::new(),
    }
}

fn edit_body<'a>(form: &'a MappingForm, error: Option<&'a str>) -> Column<'a, Msg> {
    let edit = |wrap: fn(String) -> FormEdit| move |v: String| Msg::Edit(wrap(v));
    let mut body = column![
        ui::muted("The AniList episode is the Jellyfin episode plus the offset."),
        ui::field("AniList ID", &form.media, edit(FormEdit::Media)),
        ui::field("Season", &form.season, edit(FormEdit::Season)),
        ui::field("First episode", &form.start, edit(FormEdit::Start)),
        ui::field(
            "Last episode (leave empty for no end)",
            &form.end,
            edit(FormEdit::End)
        ),
        ui::field("Offset", &form.offset, edit(FormEdit::Offset)),
    ]
    .spacing(12);
    if let Some(error) = error {
        body = body.push(ui::error_line(error));
    }
    body.push(
        row![
            ui::spacer(),
            ui::secondary("Cancel", Msg::Close),
            ui::primary("Save match", Msg::SubmitEdit),
        ]
        .spacing(theme::S2),
    )
}

pub fn with_modal<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let Some(modal) = &app.mapping.modal else {
        return base;
    };
    match modal {
        Modal::Change {
            item,
            scope,
            seasons,
            query,
            results,
            searching,
            id,
            follow,
            error,
        } => ui::modal(
            base,
            &item.name,
            560.0,
            Element::from(change_body(
                app,
                ChangeView {
                    item,
                    scope: *scope,
                    seasons,
                    query,
                    results,
                    searching: *searching,
                    id,
                    follow: *follow,
                    error: error.as_deref(),
                },
            ))
            .map(Message::Mapping),
        ),
        Modal::Edit {
            original,
            form,
            error,
        } => ui::modal(
            base,
            &original.title,
            460.0,
            Element::from(edit_body(form, error.as_deref())).map(Message::Mapping),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping() -> Mapping {
        Mapping {
            jellyfin_id: "s".into(),
            season: 1,
            ep_start: 1,
            ep_end: Some(12),
            media_id: 9,
            offset: 0,
            source: MappingSource::Title,
            confidence: 0.8,
            title: "Show".into(),
        }
    }

    #[test]
    fn a_match_names_the_anilist_episodes_where_they_differ() {
        let at = |ep_start, ep_end, offset| Mapping {
            ep_start,
            ep_end,
            offset,
            ..mapping()
        };
        // Season 2 of a long entry, a season's first entry, a later part, an open end.
        assert_eq!(
            range_label(&at(1, Some(21), 20)),
            "E1–21 → Show, episodes 21–41"
        );
        assert_eq!(range_label(&at(1, Some(20), 0)), "E1–20 → Show");
        assert_eq!(
            range_label(&at(14, Some(26), -13)),
            "E14–26 → Show, episodes 1–13"
        );
        assert_eq!(
            range_label(&at(1, None, 40)),
            "E1–… → Show, from episode 41"
        );
        assert_eq!(range_label(&at(5, Some(5), 2)), "E5–5 → Show, episode 7");
    }

    #[test]
    fn a_new_entry_starts_after_the_last_one_and_counts_from_one() {
        let first = mapping();
        let added = entry_after("s", 1, &[&first]);
        assert_eq!((added.ep_start, added.ep_end), (13, None));
        assert_eq!(added.anilist_episode(13), Some(1));
        // The ID is left for the user to fill in.
        assert_eq!(MappingForm::new(&added).media, "");
    }

    #[test]
    fn empty_last_episode_means_open_end_and_marks_manual() {
        let original = mapping();
        let mut form = MappingForm::new(&original);
        form.end.clear();
        let built = form.build(&original).unwrap();
        assert_eq!(built.ep_end, None);
        assert_eq!(built.source, MappingSource::Manual);
    }

    #[test]
    fn last_episode_before_first_is_rejected() {
        let original = mapping();
        let mut form = MappingForm::new(&original);
        form.start = "5".into();
        form.end = "2".into();
        assert!(form.build(&original).is_err());
    }
}
