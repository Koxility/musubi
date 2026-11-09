//! Automatische Zuordnungen ansehen und nachträglich ändern.

use std::fmt;

use iced::widget::{
    Column, column, container, pick_list, progress_bar, row, scrollable, text, text_input, toggler,
};
use iced::{Center, Element, Fill, Length, Task};
use musubi_core::domain::{LibraryItem, Mapping, MappingSource, Media, MediaKind};
use musubi_core::{ReviewCandidate, ReviewItem};

use crate::app::{App, Message};
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
            media: mapping.media_id.to_string(),
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
                    Ok(updated) => {
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
    let parts: Vec<String> = mappings
        .iter()
        .map(|m| {
            format!(
                "E{}–{} → {}{}",
                m.ep_start,
                m.ep_end.map_or("…".to_owned(), |e| e.to_string()),
                ui::ellipsize(&m.title, 54),
                if m.offset == 0 {
                    String::new()
                } else {
                    format!(" ({:+})", m.offset)
                },
            )
        })
        .collect();
    let unsure = mappings.iter().any(|m| m.source == MappingSource::Guess);
    let mut heading = row![text(season_label(season)).size(14).width(90)]
        .spacing(8)
        .align_y(Center);
    heading = heading.push(
        column(parts.into_iter().map(ui::muted))
            .spacing(2)
            .width(Fill),
    );
    if unsure {
        heading = heading.push(ui::badge_row("unsure".to_owned(), theme::ORANGE));
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
        .push(ui::secondary("Remove", Msg::UnmapSeason(id, season)))
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
    let mut heading = row![text(ui::ellipsize(name, 64)).size(16).width(Fill)]
        .spacing(8)
        .align_y(Center);
    if let Some(item) = item {
        heading = heading.push(ui::secondary(
            "All seasons…",
            Msg::OpenChange(Box::new(item.clone()), None),
        ));
    }
    container(column![heading, column(lines).spacing(6)].spacing(8))
        .padding(6)
        .into()
}

fn unmatched_row(item: &LibraryItem) -> Element<'_, Msg> {
    row![
        text(ui::ellipsize(&item.name, 80)).size(15).width(Fill),
        ui::secondary("Choose…", Msg::OpenChange(Box::new(item.clone()), None)),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let state = &app.mapping;
    let mut body = column![
        row![
            ui::heading("Mapping"),
            ui::spacer(),
            ui::secondary("Match again", Msg::MatchAgain),
        ]
        .align_y(Center),
        ui::muted("Titles are matched automatically. Change anything here or on a title's page."),
    ]
    .spacing(18)
    .padding(iced::padding::right(14));
    if let Some((done, total)) = state.progress {
        body = body.push(
            row![
                progress_bar(0.0..=total.max(1) as f32, done as f32)
                    .length(Length::Fill)
                    .girth(8),
                ui::muted(format!("{done} / {total}")),
            ]
            .spacing(10)
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
        body = body
            .push(text(format!("Without a match ({})", unmatched.len())).size(16))
            .push(ui::card(column(unmatched).spacing(8)));
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
    body = body
        .push(text(format!("Matched titles ({})", mapped.len())).size(16))
        .push(ui::card(column(mapped).spacing(4)));
    let page: Element<'_, Msg> = scrollable(body).width(Fill).height(Fill).into();
    let page = container(page).padding(24);
    Element::from(page).map(Message::Mapping)
}

fn choice_line<'a>(title: &str, details: String, media: i64) -> Element<'a, Msg> {
    row![
        column![text(ui::ellipsize(title, 56)).size(14), ui::muted(details),].width(Fill),
        ui::primary("Use", Msg::UseMedia(media)),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

fn candidate_line<'a>(candidate: &ReviewCandidate) -> Element<'a, Msg> {
    let details = format!(
        "{} · {} · {} eps · match {:.0}%",
        candidate.format.as_deref().unwrap_or("?"),
        candidate.year.map_or("?".to_owned(), |y| y.to_string()),
        candidate.episodes.map_or("?".to_owned(), |e| e.to_string()),
        candidate.score * 100.0
    );
    choice_line(&candidate.title, details, candidate.media_id)
}

fn result_line<'a>(media: &Media) -> Element<'a, Msg> {
    let details = format!(
        "{} · {} · {} eps",
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
    let mut body = column![].spacing(12);
    if view.seasons.len() > 1 || view.scope.0.is_some() {
        let mut choices = vec![Scope(None)];
        choices.extend(view.seasons.iter().map(|s| Scope(Some(*s))));
        body = body.push(
            row![
                text("Apply to").size(14).color(theme::MUTED),
                pick_list(choices, Some(view.scope), Msg::Scope)
                    .text_size(14)
                    .padding([6, 10]),
            ]
            .spacing(10)
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
            .push(text("Suggestions").size(16))
            .push(column(suggestions).spacing(8));
    }
    body = body.push(text("Search AniList").size(16)).push(
        row![
            text_input("Title", view.query)
                .on_input(Msg::Query)
                .on_submit(Msg::Search)
                .padding(8)
                .style(theme::input),
            ui::secondary("Search", Msg::Search),
        ]
        .spacing(8),
    );
    if view.searching {
        body = body.push(ui::muted("Searching…"));
    } else if !view.results.is_empty() {
        body = body.push(column(view.results.iter().map(result_line)).spacing(8));
    }
    body = body
        .push(
            row![
                container(ui::field(
                    "or AniList ID (the number in the URL)",
                    view.id,
                    Msg::MapId
                ))
                .width(Fill),
                ui::secondary("Use ID", Msg::SubmitId),
            ]
            .spacing(8)
            .align_y(iced::Bottom),
        )
        .push(
            toggler(view.follow)
                .label(if view.scope.0.is_some() {
                    "Split the season over several AniList entries"
                } else {
                    "Assign sequels to later seasons"
                })
                .text_size(14)
                .on_toggle(Msg::MapFollow),
        );
    if let Some(error) = view.error {
        body = body.push(ui::error_line(error));
    }
    let mut actions = row![].spacing(8);
    if let Some(first) = current.first() {
        let remove = match view.scope.0 {
            Some(season) => Msg::UnmapSeason(view.item.id.clone(), season),
            None => Msg::Unmap(view.item.id.clone()),
        };
        actions = actions
            .push(ui::danger("Remove match", remove))
            .push(ui::secondary(
                "Exact editor…",
                Msg::OpenEdit(Box::new((*first).clone())),
            ));
    }
    body.push(
        actions
            .push(ui::spacer())
            .push(ui::secondary("Close", Msg::Close)),
    )
}

fn edit_body<'a>(form: &'a MappingForm, error: Option<&'a str>) -> Column<'a, Msg> {
    let edit = |wrap: fn(String) -> FormEdit| move |v: String| Msg::Edit(wrap(v));
    let mut body = column![
        ui::muted("AniList episode = Jellyfin episode + offset"),
        ui::field("AniList ID", &form.media, edit(FormEdit::Media)),
        ui::field("Season", &form.season, edit(FormEdit::Season)),
        ui::field("First episode", &form.start, edit(FormEdit::Start)),
        ui::field(
            "Last episode (empty = open)",
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
            ui::primary("Save", Msg::SubmitEdit),
        ]
        .spacing(8),
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
