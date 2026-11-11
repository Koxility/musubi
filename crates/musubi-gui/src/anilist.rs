//! AniList-Seite: Übersicht mit Airing und Anime in Progress, Listen und Bearbeitung.

use std::fmt;

use iced::widget::{
    button, center, column, container, hover, pick_list, responsive, row, text, text_input,
};
use iced::{Center, Element, Fill, Task};
use musubi_core::domain::{
    EntryPatch, FuzzyDate, LibraryItem, ListEntry, ListStatus, Media, MediaKind, ScoreFormat,
    Viewer, airing_entries, in_progress_entries,
};

use crate::app::{App, Message};
use crate::icons::Icon;
use crate::images::ImageKey;
use crate::{theme, ui};

const COVER: (f32, f32) = (124.0, 176.0);
/// Rand der Karten, damit die Fläche beim Überfahren um das Cover herum sichtbar wird.
const CARD_PADDING: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Overview,
    Anime,
    Manga,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusOption(pub ListStatus, pub MediaKind);

impl fmt::Display for StatusOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label(self.1))
    }
}

fn status_options(kind: MediaKind) -> Vec<StatusOption> {
    ListStatus::ALL
        .iter()
        .map(|s| StatusOption(*s, kind))
        .collect()
}

/// Eingaben des Dialogs als Text, geprüft wird erst beim Speichern.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorForm {
    pub status: ListStatus,
    pub score: String,
    pub progress: String,
    pub volumes: String,
    pub repeat: String,
    pub priority: String,
    pub started: String,
    pub completed: String,
    pub private: bool,
    pub hidden: bool,
    pub notes: String,
    pub advanced: Vec<(String, String)>,
    pub lists: Vec<(String, bool)>,
}

impl EditorForm {
    pub fn new(entry: &ListEntry, format: ScoreFormat, viewer: Option<&Viewer>) -> Self {
        let kind = entry.media.kind;
        let advanced = viewer
            .map(|v| v.advanced_scoring(kind).to_vec())
            .unwrap_or_default()
            .into_iter()
            .map(|name| {
                let value = entry
                    .advanced_scores
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map_or(0.0, |(_, v)| *v);
                (name, value.to_string())
            })
            .collect();
        let lists = viewer
            .map(|v| v.custom_lists(kind).to_vec())
            .unwrap_or_default()
            .into_iter()
            .map(|name| {
                let on = entry.custom_lists.contains(&name);
                (name, on)
            })
            .collect();
        Self {
            status: entry.status,
            score: if entry.score_raw == 0 {
                String::new()
            } else {
                format.display(entry.score_raw)
            },
            progress: entry.progress.to_string(),
            volumes: entry.progress_volumes.to_string(),
            repeat: entry.repeat.to_string(),
            priority: entry.priority.to_string(),
            started: entry.started_at.to_string(),
            completed: entry.completed_at.to_string(),
            private: entry.private,
            hidden: entry.hidden_from_status_lists,
            notes: entry.notes.clone(),
            advanced,
            lists,
        }
    }

    /// Enthält nur Felder, die sich gegenüber `entry` geändert haben.
    pub fn patch(&self, entry: &ListEntry, format: ScoreFormat) -> Result<EntryPatch, String> {
        fn number(name: &str, value: &str, old: u32) -> Result<Option<u32>, String> {
            let parsed: u32 = value
                .trim()
                .parse()
                .map_err(|_| format!("{name} must be a whole number"))?;
            Ok((parsed != old).then_some(parsed))
        }
        fn date(value: &str, old: FuzzyDate) -> Result<Option<FuzzyDate>, String> {
            let parsed: FuzzyDate = value
                .parse()
                .map_err(|e: musubi_core::domain::ParseDateError| e.to_string())?;
            Ok((parsed != old).then_some(parsed))
        }
        let mut patch = EntryPatch::new(entry.media.id);
        patch.status = (self.status != entry.status).then_some(self.status);
        let shown = if entry.score_raw == 0 {
            String::new()
        } else {
            format.display(entry.score_raw)
        };
        if self.score.trim() != shown {
            let raw = format.parse(&self.score).map_err(|e| e.to_string())?;
            patch.score_raw = (raw != entry.score_raw).then_some(raw);
        }
        patch.progress = number("Progress", &self.progress, entry.progress)?;
        if entry.media.kind == MediaKind::Manga {
            patch.progress_volumes = number("Volumes", &self.volumes, entry.progress_volumes)?;
        }
        patch.repeat = number("Repeats", &self.repeat, entry.repeat)?;
        patch.priority = number("Priority", &self.priority, entry.priority)?;
        patch.started_at = date(&self.started, entry.started_at)?;
        patch.completed_at = date(&self.completed, entry.completed_at)?;
        patch.private = (self.private != entry.private).then_some(self.private);
        patch.hidden_from_status_lists =
            (self.hidden != entry.hidden_from_status_lists).then_some(self.hidden);
        if self.notes != entry.notes {
            patch.notes = Some(self.notes.clone());
        }
        let advanced = self
            .advanced
            .iter()
            .map(|(name, value)| {
                value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("{name} must be a number"))
            })
            .collect::<Result<Vec<f64>, String>>()?;
        let changed = self.advanced.iter().zip(&advanced).any(|((name, _), new)| {
            let old = entry
                .advanced_scores
                .iter()
                .find(|(n, _)| n == name)
                .map_or(0.0, |(_, v)| *v);
            (new - old).abs() > f64::EPSILON
        });
        if changed {
            patch.advanced_scores = Some(advanced);
        }
        let enabled: Vec<String> = self
            .lists
            .iter()
            .filter(|(_, on)| *on)
            .map(|(name, _)| name.clone())
            .collect();
        let (mut before, mut after) = (entry.custom_lists.clone(), enabled.clone());
        before.sort();
        after.sort();
        if before != after {
            patch.custom_lists = Some(enabled);
        }
        Ok(patch)
    }
}

pub struct Editor {
    entry: ListEntry,
    format: ScoreFormat,
    form: EditorForm,
    error: Option<String>,
    confirm_delete: bool,
}

pub struct AddDialog {
    kind: MediaKind,
    query: String,
    results: Vec<Media>,
    status: ListStatus,
    searching: bool,
}

pub struct ListsDialog {
    kind: MediaKind,
    names: Vec<String>,
    new_name: String,
    renaming: Option<(String, String)>,
}

pub enum Modal {
    Editor(Box<Editor>),
    Add(AddDialog),
    Lists(ListsDialog),
}

#[derive(Default)]
pub struct State {
    tab: Tab,
    anime: Vec<ListEntry>,
    manga: Vec<ListEntry>,
    search: String,
    status: Option<ListStatus>,
    loading: bool,
    modal: Option<Modal>,
}

impl State {
    pub fn close_modal(&mut self) -> bool {
        self.modal.take().is_some()
    }

    fn list(&self, kind: MediaKind) -> &[ListEntry] {
        match kind {
            MediaKind::Anime => &self.anime,
            MediaKind::Manga => &self.manga,
        }
    }

    fn kind(&self) -> MediaKind {
        if self.tab == Tab::Manga {
            MediaKind::Manga
        } else {
            MediaKind::Anime
        }
    }
}

#[derive(Debug, Clone)]
pub enum EditorEdit {
    Status(StatusOption),
    Score(String),
    Progress(String),
    Volumes(String),
    Repeat(String),
    Priority(String),
    Started(String),
    Completed(String),
    Private(bool),
    Hidden(bool),
    Notes(String),
    Advanced(usize, String),
    CustomList(usize, bool),
}

#[derive(Debug, Clone)]
pub enum Msg {
    WantImage(ImageKey),
    Refreshed(Result<(), String>),
    Tab(Tab),
    Search(String),
    StatusFilter(Option<ListStatus>),
    Increment(ListEntry),
    OpenEditor(ListEntry),
    Edit(EditorEdit),
    SaveEditor,
    AskDelete,
    Delete,
    Close,
    OpenAdd,
    AddQuery(String),
    AddSearch,
    AddResults(Result<Vec<Media>, String>),
    AddStatus(StatusOption),
    AddMedia(Box<Media>),
    OpenLists,
    NewListName(String),
    CreateList,
    DeleteList(String),
    StartRename(String),
    RenameText(String),
    CommitRename,
}

pub fn load(app: &mut App, kind: MediaKind) -> Task<Message> {
    list_changed(app, kind);
    list_changed(app, kind.other());
    if !app.auth.anilist_logged_in {
        return Task::none();
    }
    app.anilist.loading = true;
    let core = app.core.clone();
    ui::perform(
        async move {
            core.refresh_viewer().await?;
            core.refresh_list(kind).await?;
            core.refresh_list(kind.other()).await?;
            Ok(())
        },
        |r| Message::AniList(Msg::Refreshed(r)),
    )
}

pub fn list_changed(app: &mut App, kind: MediaKind) {
    let list = app.core.list(kind);
    match kind {
        MediaKind::Anime => app.anilist.anime = list,
        MediaKind::Manga => app.anilist.manga = list,
    }
}

fn open_editor(app: &mut App, entry: ListEntry) {
    let viewer = app.core.viewer();
    let format = viewer.as_ref().map(|v| v.score_format).unwrap_or_default();
    let form = EditorForm::new(&entry, format, viewer.as_ref());
    app.anilist.modal = Some(Modal::Editor(Box::new(Editor {
        entry,
        format,
        form,
        error: None,
        confirm_delete: false,
    })));
}

/// Öffnet den Eintrag zu einem Bibliothekstitel.
/// Mit `season` gilt der Eintrag dieser Staffel, sonst der des ersten zugeordneten Werks.
pub fn open_editor_for(app: &mut App, item: &LibraryItem, season: Option<u32>) {
    let entry = app
        .library
        .rows
        .iter()
        .find(|r| r.item.id == item.id)
        .and_then(|r| {
            r.mappings
                .iter()
                .filter(|m| season.is_none_or(|s| m.season == s))
                .min_by_key(|m| (m.season, m.ep_start))
        })
        .and_then(|m| app.core.entry_for(m.media_id));
    match entry {
        Some(entry) => open_editor(app, entry),
        None => app.toast("This title is not on your AniList yet.", false),
    }
}

fn apply_edit(form: &mut EditorForm, edit: EditorEdit) {
    match edit {
        EditorEdit::Status(StatusOption(status, _)) => form.status = status,
        EditorEdit::Score(v) => form.score = v,
        EditorEdit::Progress(v) => form.progress = v,
        EditorEdit::Volumes(v) => form.volumes = v,
        EditorEdit::Repeat(v) => form.repeat = v,
        EditorEdit::Priority(v) => form.priority = v,
        EditorEdit::Started(v) => form.started = v,
        EditorEdit::Completed(v) => form.completed = v,
        EditorEdit::Private(v) => form.private = v,
        EditorEdit::Hidden(v) => form.hidden = v,
        EditorEdit::Notes(v) => form.notes = v,
        EditorEdit::Advanced(index, v) => {
            if let Some(slot) = form.advanced.get_mut(index) {
                slot.1 = v;
            }
        }
        EditorEdit::CustomList(index, v) => {
            if let Some(slot) = form.lists.get_mut(index) {
                slot.1 = v;
            }
        }
    }
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let core = app.core.clone();
    match msg {
        Msg::WantImage(key) => return app.want_image(key),
        Msg::Refreshed(result) => {
            app.anilist.loading = false;
            app.report(result);
        }
        Msg::Tab(tab) => app.anilist.tab = tab,
        Msg::Search(value) => app.anilist.search = value,
        Msg::StatusFilter(status) => app.anilist.status = status,
        Msg::Increment(entry) => {
            let progress = entry.progress + 1;
            if entry.media.total_units().is_some_and(|t| progress > t) {
                return Task::none();
            }
            let patch = EntryPatch {
                progress: Some(progress),
                ..EntryPatch::new(entry.media.id)
            };
            return ui::perform(async move { core.save_entry(patch).await }, Message::Notify);
        }
        Msg::OpenEditor(entry) => open_editor(app, entry),
        Msg::Edit(edit) => {
            if let Some(Modal::Editor(editor)) = &mut app.anilist.modal {
                apply_edit(&mut editor.form, edit);
            }
        }
        Msg::SaveEditor => {
            if let Some(Modal::Editor(editor)) = &mut app.anilist.modal {
                match editor.form.patch(&editor.entry, editor.format) {
                    Ok(patch) => {
                        app.anilist.modal = None;
                        return ui::perform(
                            async move { core.save_entry(patch).await },
                            Message::Notify,
                        );
                    }
                    Err(error) => editor.error = Some(error),
                }
            }
        }
        Msg::AskDelete => {
            if let Some(Modal::Editor(editor)) = &mut app.anilist.modal {
                editor.confirm_delete = true;
            }
        }
        Msg::Delete => {
            if let Some(Modal::Editor(editor)) = app.anilist.modal.take() {
                return ui::perform(
                    async move { core.delete_entry(&editor.entry).await },
                    Message::Notify,
                );
            }
        }
        Msg::Close => app.anilist.modal = None,
        Msg::OpenAdd => {
            app.anilist.modal = Some(Modal::Add(AddDialog {
                kind: app.anilist.kind(),
                query: String::new(),
                results: vec![],
                status: ListStatus::Planning,
                searching: false,
            }));
        }
        Msg::AddQuery(value) => {
            if let Some(Modal::Add(dialog)) = &mut app.anilist.modal {
                dialog.query = value;
            }
        }
        Msg::AddSearch => {
            if let Some(Modal::Add(dialog)) = &mut app.anilist.modal {
                dialog.searching = true;
                let (text, kind) = (dialog.query.clone(), dialog.kind);
                return ui::perform(async move { core.search_anilist(&text, kind).await }, |r| {
                    Message::AniList(Msg::AddResults(r))
                });
            }
        }
        Msg::AddResults(result) => {
            if let Some(Modal::Add(dialog)) = &mut app.anilist.modal {
                dialog.searching = false;
                match result {
                    Ok(results) => dialog.results = results,
                    Err(error) => app.toast(error, false),
                }
            }
        }
        Msg::AddStatus(StatusOption(status, _)) => {
            if let Some(Modal::Add(dialog)) = &mut app.anilist.modal {
                dialog.status = status;
            }
        }
        Msg::AddMedia(media) => {
            if let Some(Modal::Add(dialog)) = app.anilist.modal.take() {
                return ui::perform(
                    async move { core.add_to_list(*media, dialog.status).await },
                    Message::Notify,
                );
            }
        }
        Msg::OpenLists => {
            let kind = app.anilist.kind();
            let names = core
                .viewer()
                .map(|v| v.custom_lists(kind).to_vec())
                .unwrap_or_default();
            app.anilist.modal = Some(Modal::Lists(ListsDialog {
                kind,
                names,
                new_name: String::new(),
                renaming: None,
            }));
        }
        Msg::NewListName(value) => {
            if let Some(Modal::Lists(dialog)) = &mut app.anilist.modal {
                dialog.new_name = value;
            }
        }
        Msg::CreateList => {
            if let Some(Modal::Lists(dialog)) = &mut app.anilist.modal {
                let name = dialog.new_name.trim().to_owned();
                if name.is_empty() {
                    return Task::none();
                }
                let kind = dialog.kind;
                dialog.names.push(name.clone());
                dialog.new_name.clear();
                return ui::perform(
                    async move { core.create_custom_list(kind, &name).await },
                    Message::Notify,
                );
            }
        }
        Msg::DeleteList(name) => {
            if let Some(Modal::Lists(dialog)) = &mut app.anilist.modal {
                dialog.names.retain(|n| *n != name);
                let kind = dialog.kind;
                return ui::perform(
                    async move { core.delete_custom_list(kind, &name).await },
                    Message::Notify,
                );
            }
        }
        Msg::StartRename(name) => {
            if let Some(Modal::Lists(dialog)) = &mut app.anilist.modal {
                dialog.renaming = Some((name.clone(), name));
            }
        }
        Msg::RenameText(value) => {
            if let Some(Modal::Lists(ListsDialog {
                renaming: Some((_, text)),
                ..
            })) = &mut app.anilist.modal
            {
                *text = value;
            }
        }
        Msg::CommitRename => {
            if let Some(Modal::Lists(dialog)) = &mut app.anilist.modal
                && let Some((old, new)) = dialog.renaming.take()
            {
                let new = new.trim().to_owned();
                if new.is_empty() || new == old {
                    return Task::none();
                }
                if let Some(slot) = dialog.names.iter_mut().find(|n| **n == old) {
                    slot.clone_from(&new);
                }
                let kind = dialog.kind;
                return ui::perform(
                    async move { core.rename_custom_list(kind, &old, &new).await },
                    Message::Notify,
                );
            }
        }
    }
    Task::none()
}

/// Zeigt die Liste dieser Art, gefiltert auf einen Status, z. B. aus dem Profil heraus.
pub fn show_status(app: &mut App, kind: MediaKind, status: ListStatus) {
    app.anilist.tab = match kind {
        MediaKind::Anime => Tab::Anime,
        MediaKind::Manga => Tab::Manga,
    };
    app.anilist.status = Some(status);
    app.anilist.search.clear();
}

fn want(key: ImageKey) -> Msg {
    Msg::WantImage(key)
}

fn cover<'a>(app: &'a App, media: &Media, width: f32, height: f32) -> Element<'a, Msg> {
    let key = media.cover_url.clone().map(ImageKey::Cover);
    ui::picture(&app.images, key, width, height, want)
}

fn total(entry: &ListEntry) -> String {
    entry
        .media
        .total_units()
        .map_or("?".to_owned(), |t| t.to_string())
}

/// Anteil des Fortschritts, falls die Gesamtzahl bekannt ist.
fn fraction(entry: &ListEntry) -> Option<f32> {
    let total = entry.media.total_units().filter(|t| *t > 0)?;
    Some((entry.progress as f32 / total as f32).min(1.0))
}

/// Ob ein weiterer Zähler noch in die Gesamtzahl passt.
fn can_increment(entry: &ListEntry) -> bool {
    entry.media.total_units().is_none_or(|t| entry.progress < t)
}

/// Runder Knopf, der den Fortschritt um eins erhöht.
fn plus_one<'a>(entry: &ListEntry, size: f32) -> Element<'a, Msg> {
    let label = if entry.media.kind == MediaKind::Anime {
        "Watched one more episode"
    } else {
        "Read one more chapter"
    };
    ui::icon_button(
        Icon::Plus,
        size,
        theme::TRACK,
        can_increment(entry).then(|| Msg::Increment(entry.clone())),
        label,
        theme::ghost_button,
    )
}

/// Karte im Stil der AniList-Startseite, mit `+1` beim Überfahren.
fn progress_card<'a>(app: &'a App, entry: &ListEntry) -> Element<'a, Msg> {
    let image = ui::with_progress(
        cover(app, &entry.media, COVER.0, COVER.1),
        fraction(entry),
        COVER,
        theme::TRACK,
    );
    let behind = entry.episodes_behind();
    let mut info = row![ui::muted(format!("{} / {}", entry.progress, total(entry)))]
        .spacing(6)
        .align_y(Center);
    if behind > 0 {
        info = info.push(ui::tag(format!("{behind} behind"), theme::WARN));
    }
    let base = button(
        column![
            image,
            column![
                text(ui::ellipsize(entry.media.title.preferred(), 17))
                    .size(theme::T_SMALL)
                    .font(theme::MEDIUM),
                info,
            ]
            .spacing(2),
        ]
        .spacing(theme::S2),
    )
    .padding(CARD_PADDING)
    .style(theme::card_button)
    .on_press(Msg::OpenEditor(entry.clone()));
    let side = COVER.0 + 2.0 * CARD_PADDING;
    let plus = container(plus_one(entry, 15.0))
        .padding(CARD_PADDING + 6.0)
        .width(side)
        .align_right(side);
    hover(base, plus)
}

fn card_section<'a>(
    app: &'a App,
    title: &'a str,
    entries: Vec<&'a ListEntry>,
) -> Option<Element<'a, Msg>> {
    if entries.is_empty() {
        return None;
    }
    let count = entries.len();
    let cards = entries.iter().map(|e| progress_card(app, e));
    Some(ui::section(
        ui::heading_count(title, count),
        // Die Karten haben einen eigenen Rand und rücken deshalb unter die Überschrift ein.
        container(
            row(cards)
                .spacing(theme::S1)
                .wrap()
                .vertical_spacing(theme::S2),
        )
        .padding(iced::padding::left(-CARD_PADDING)),
    ))
}

/// Zeile des Sendeplans: nächste Folge, Countdown und eigener Stand.
fn airing_row<'a>(app: &'a App, entry: &'a ListEntry) -> Element<'a, Msg> {
    let next = entry.media.next_airing;
    let when = next.map_or(String::new(), |n| ui::countdown(n.airing_at - app.now));
    let episode = next.map_or(String::new(), |n| format!("Episode {}", n.episode));
    let behind = entry.episodes_behind();
    let mut state = row![ui::muted(format!("{} / {}", entry.progress, total(entry)))]
        .spacing(6)
        .align_y(Center);
    if behind > 0 {
        state = state.push(ui::tag(format!("{behind} behind"), theme::WARN));
    }
    let info = button(
        row![
            cover(app, &entry.media, 40.0, 56.0),
            column![
                text(ui::ellipsize(entry.media.title.preferred(), 30))
                    .size(theme::T_SMALL)
                    .font(theme::MEDIUM),
                state,
            ]
            .spacing(3)
            .width(Fill),
            column![
                text(when)
                    .size(theme::T_SMALL)
                    .font(theme::SEMIBOLD)
                    .color(theme::TRACK),
                ui::muted(episode),
            ]
            .spacing(3)
            .align_x(iced::Alignment::End),
        ]
        .spacing(theme::S3)
        .align_y(Center),
    )
    .width(Fill)
    .padding([6.0, theme::S2])
    .style(theme::row_button)
    .on_press(Msg::OpenEditor(entry.clone()));
    row![info, plus_one(entry, 14.0)]
        .spacing(theme::S2)
        .align_y(Center)
        .into()
}

/// Sendeplan der laufenden Titel, die nächste Folge zuerst.
fn schedule<'a>(app: &'a App, entries: Vec<&'a ListEntry>) -> Element<'a, Msg> {
    let count = entries.len();
    let rows: Vec<Element<'a, Msg>> = entries.into_iter().map(|e| airing_row(app, e)).collect();
    let body: Element<'a, Msg> = if rows.is_empty() {
        container(ui::muted(
            "Nothing you watch is airing right now. Titles appear here once a next episode is scheduled.",
        ))
        .padding(theme::S2)
        .into()
    } else {
        ui::divided(rows).into()
    };
    ui::section(
        ui::heading_count("Airing", count),
        container(body)
            .padding(theme::S2)
            .width(Fill)
            .style(theme::surface),
    )
}

/// Kennzahlen über der Übersicht.
fn overview_numbers(app: &App) -> Element<'_, Msg> {
    let state = &app.anilist;
    let watching = state
        .anime
        .iter()
        .filter(|e| matches!(e.status, ListStatus::Current | ListStatus::Repeating))
        .count();
    let reading = state
        .manga
        .iter()
        .filter(|e| matches!(e.status, ListStatus::Current | ListStatus::Repeating))
        .count();
    let airing = airing_entries(&state.anime).len();
    let behind: u32 = state.anime.iter().map(ListEntry::episodes_behind).sum();
    row![
        ui::stat(watching.to_string(), "Watching", theme::PAPER),
        ui::stat(airing.to_string(), "Airing now", theme::TRACK),
        ui::stat(
            behind.to_string(),
            "Episodes behind",
            if behind > 0 {
                theme::WARN
            } else {
                theme::PAPER
            }
        ),
        ui::stat(reading.to_string(), "Reading", theme::PAPER),
    ]
    .spacing(theme::S3)
    .into()
}

fn progress_sections(app: &App) -> Element<'_, Msg> {
    let state = &app.anilist;
    let sections = [
        card_section(app, "Anime in progress", in_progress_entries(&state.anime)),
        card_section(app, "Manga in progress", in_progress_entries(&state.manga)),
    ];
    let mut content = column![].spacing(theme::S5);
    let mut any = false;
    for section in sections.into_iter().flatten() {
        any = true;
        content = content.push(section);
    }
    if !any {
        content = content.push(ui::muted(
            "Nothing else in progress. Set a title to Watching or Reading to see it here.",
        ));
    }
    content.into()
}

fn overview(app: &App) -> Element<'_, Msg> {
    let state = &app.anilist;
    if state.anime.is_empty() && state.manga.is_empty() {
        return if state.loading {
            center(ui::muted("Loading your lists…")).into()
        } else {
            ui::empty_state(
                Icon::List,
                "Your lists are empty",
                "Add a title to start tracking what you watch and read.",
            )
        };
    }
    responsive(move |size| {
        let airing = schedule(app, airing_entries(&app.anilist.anime));
        // Breit stehen Sendeplan und Karten nebeneinander, schmal untereinander.
        let body: Element<'_, Msg> = if size.width >= 900.0 {
            row![
                container(airing).width(400),
                container(progress_sections(app)).width(Fill),
            ]
            .spacing(theme::S6)
            .into()
        } else {
            column![airing, progress_sections(app)]
                .spacing(theme::S5)
                .into()
        };
        ui::scroll(
            column![overview_numbers(app), body]
                .spacing(theme::S5)
                .padding(iced::Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: theme::S6,
                    left: 0.0,
                }),
        )
    })
    .into()
}

/// Spaltenbreiten der Listen, für Kopf und Zeilen gleich.
const SCORE_WIDTH: f32 = 64.0;
const PROGRESS_WIDTH: f32 = 150.0;
const FORMAT_WIDTH: f32 = 72.0;

fn list_head<'a>(kind: MediaKind) -> Element<'a, Msg> {
    let cell = |label: &'a str| text(label).size(theme::T_CAPTION).color(theme::FAINT);
    row![
        cell("Title").width(Fill),
        cell("Score").width(SCORE_WIDTH),
        cell(if kind == MediaKind::Anime {
            "Episodes"
        } else {
            "Chapters"
        })
        .width(PROGRESS_WIDTH),
        cell("Format").width(FORMAT_WIDTH),
    ]
    .spacing(theme::S3)
    .padding(iced::Padding {
        top: 0.0,
        right: theme::S2,
        bottom: 6.0,
        left: theme::S2 + 40.0 + theme::S3,
    })
    .into()
}

fn list_row<'a>(app: &'a App, entry: &ListEntry, format: ScoreFormat) -> Element<'a, Msg> {
    let score = if entry.score_raw == 0 {
        "–".to_owned()
    } else {
        format.display(entry.score_raw)
    };
    let mut title = row![
        text(ui::ellipsize(entry.media.title.preferred(), 64))
            .size(theme::T_BODY)
            .font(theme::MEDIUM)
    ]
    .spacing(theme::S2)
    .align_y(Center);
    if entry.episodes_behind() > 0 {
        title = title.push(ui::tag(
            format!("{} behind", entry.episodes_behind()),
            theme::WARN,
        ));
    }
    if entry.private {
        title = title.push(ui::tag("Private", theme::MUTED));
    }
    if !entry.notes.is_empty() {
        title = title.push(ui::muted(ui::ellipsize(&entry.notes, 40)));
    }
    let amount = column![
        text(format!("{} / {}", entry.progress, total(entry))).size(theme::T_SMALL),
        ui::bar(
            fraction(entry).unwrap_or(0.0),
            theme::status_color(entry.status),
            3.0
        ),
    ]
    .spacing(5)
    .width(Fill);
    let active = matches!(entry.status, ListStatus::Current | ListStatus::Repeating);
    let info = button(
        row![
            cover(app, &entry.media, 40.0, 56.0),
            container(title).width(Fill),
            text(score).size(theme::T_SMALL).width(SCORE_WIDTH),
            container(amount).width(PROGRESS_WIDTH - 44.0),
        ]
        .spacing(theme::S3)
        .align_y(Center),
    )
    .width(Fill)
    .padding([6.0, theme::S2])
    .style(theme::row_button)
    .on_press(Msg::OpenEditor(entry.clone()));
    let plus: Element<'a, Msg> = if active {
        plus_one(entry, 13.0)
    } else {
        iced::widget::Space::new().width(23.4).into()
    };
    row![
        info,
        container(plus).width(32),
        text(entry.media.format.clone().unwrap_or_default())
            .size(theme::T_LABEL)
            .color(theme::MUTED)
            .width(FORMAT_WIDTH),
    ]
    .spacing(theme::S3)
    .padding(iced::padding::right(theme::S2))
    .align_y(Center)
    .into()
}

/// Seitenleiste der Listen: Suche und Status mit Anzahl.
fn status_filter<'a>(state: &'a State, kind: MediaKind) -> Element<'a, Msg> {
    let entries = state.list(kind);
    let item = |label: &'a str, value: Option<ListStatus>, count: usize| -> Element<'a, Msg> {
        let active = state.status == value;
        let dot = value.map_or(theme::FAINT, theme::status_color);
        button(
            row![
                container(iced::widget::Space::new())
                    .width(8)
                    .height(8)
                    .style(theme::solid(dot, 4.0)),
                text(label)
                    .size(theme::T_SMALL)
                    .font(if active {
                        theme::SEMIBOLD
                    } else {
                        theme::MEDIUM
                    })
                    .width(Fill),
                text(count.to_string())
                    .size(theme::T_LABEL)
                    .color(theme::FAINT),
            ]
            .spacing(theme::S2)
            .align_y(Center),
        )
        .width(Fill)
        .padding([7.0, theme::S3])
        .style(theme::nav_button(active, theme::TRACK))
        .on_press(Msg::StatusFilter(value))
        .into()
    };
    let mut items = vec![item("All", None, entries.len())];
    items.extend(ListStatus::ALL.iter().map(|status| {
        let count = entries.iter().filter(|e| e.status == *status).count();
        item(status.label(kind), Some(*status), count)
    }));
    column![
        text_input("Filter this list", &state.search)
            .on_input(Msg::Search)
            .size(theme::T_SMALL)
            .padding([8, 10])
            .style(theme::input),
        column(items).spacing(2),
    ]
    .spacing(theme::S3)
    .width(200)
    .into()
}

fn list_view(app: &App) -> Element<'_, Msg> {
    let state = &app.anilist;
    let kind = state.kind();
    let format = app
        .core
        .viewer()
        .map(|v| v.score_format)
        .unwrap_or_default();
    let needle = state.search.trim().to_lowercase();
    let matches = |entry: &&ListEntry| {
        state.status.is_none_or(|s| entry.status == s)
            && (needle.is_empty()
                || entry
                    .media
                    .title
                    .all()
                    .any(|t| t.to_lowercase().contains(&needle)))
    };
    let mut body = column![].spacing(theme::S5).padding(iced::Padding {
        top: 0.0,
        right: 0.0,
        bottom: theme::S6,
        left: 0.0,
    });
    let mut any = false;
    for status in ListStatus::ALL {
        let entries: Vec<&ListEntry> = state
            .list(kind)
            .iter()
            .filter(|e| e.status == status)
            .filter(matches)
            .collect();
        if entries.is_empty() {
            continue;
        }
        any = true;
        let count = entries.len();
        let rows: Vec<Element<'_, Msg>> =
            entries.iter().map(|e| list_row(app, e, format)).collect();
        let heading = row![
            container(iced::widget::Space::new())
                .width(4)
                .height(18)
                .style(theme::solid(theme::status_color(status), 2.0)),
            ui::heading_count(status.label(kind), count),
        ]
        .spacing(theme::S2)
        .align_y(Center);
        body = body.push(ui::section(
            heading.into(),
            container(column![list_head(kind), ui::divided(rows)])
                .padding(iced::Padding {
                    top: theme::S3,
                    right: theme::S1,
                    bottom: theme::S1,
                    left: theme::S1,
                })
                .width(Fill)
                .style(theme::surface),
        ));
    }
    let list: Element<'_, Msg> = if any {
        ui::scroll(body)
    } else if state.list(kind).is_empty() {
        ui::empty_state(
            Icon::List,
            "This list is empty",
            "Add a title to start tracking it here.",
        )
    } else {
        ui::empty_state(
            Icon::Search,
            "No entries match",
            "Clear the filter or pick another status.",
        )
    };
    row![status_filter(state, kind), list]
        .spacing(theme::S5)
        .into()
}

fn header(state: &State) -> Element<'_, Msg> {
    let tab = |label: &str, value: Tab| (label.to_owned(), state.tab == value, Msg::Tab(value));
    column![
        row![
            ui::page_title("AniList"),
            ui::spacer(),
            ui::tool(
                Icon::List,
                "Custom lists",
                Some(Msg::OpenLists),
                theme::secondary,
                theme::PAPER
            ),
            ui::tool(
                Icon::Plus,
                "Add title",
                Some(Msg::OpenAdd),
                theme::track,
                theme::INK
            ),
        ]
        .spacing(theme::S2)
        .align_y(Center),
        column![
            ui::tabs(
                [
                    tab("Overview", Tab::Overview),
                    tab("Anime list", Tab::Anime),
                    tab("Manga list", Tab::Manga),
                ],
                theme::TRACK,
            ),
            ui::divider()
        ],
    ]
    .spacing(theme::S4)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let page: Element<'_, Msg> = if !app.auth.anilist_logged_in {
        ui::empty_state(
            Icon::List,
            "Not connected to AniList",
            "Log in to AniList in Settings to see and edit your lists.",
        )
    } else {
        let body = match app.anilist.tab {
            Tab::Overview => overview(app),
            _ => list_view(app),
        };
        column![header(&app.anilist), body]
            .spacing(theme::S5)
            .padding(iced::Padding {
                top: theme::S5,
                right: ui::PAGE_PADDING,
                bottom: 0.0,
                left: ui::PAGE_PADDING,
            })
            .into()
    };
    page.map(Message::AniList)
}

fn status_pick<'a>(
    kind: MediaKind,
    status: ListStatus,
    on_select: impl Fn(StatusOption) -> Msg + 'a,
) -> Element<'a, Msg> {
    pick_list(
        status_options(kind),
        Some(StatusOption(status, kind)),
        on_select,
    )
    .text_size(theme::T_BODY)
    .padding([8, 10])
    .width(Fill)
    .style(theme::pick)
    .menu_style(theme::pick_menu)
    .into()
}

/// Kurzangaben zu einem Werk, z. B. `TV · 2023 · 28 episodes`.
fn media_facts(media: &Media) -> String {
    let unit = if media.kind == MediaKind::Anime {
        "episodes"
    } else {
        "chapters"
    };
    [
        media.format.clone(),
        media.season_year.map(|y| y.to_string()),
        media.total_units().map(|t| format!("{t} {unit}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ")
}

fn group<'a>(name: &'a str, content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    column![
        text(name)
            .size(theme::T_SMALL)
            .font(theme::SEMIBOLD)
            .color(theme::PAPER),
        content.into()
    ]
    .spacing(theme::S2)
    .into()
}

fn editor_body<'a>(app: &'a App, editor: &'a Editor) -> iced::widget::Column<'a, Msg> {
    let form = &editor.form;
    let media = &editor.entry.media;
    let kind = media.kind;
    let edit = |wrap: fn(String) -> EditorEdit| move |v: String| Msg::Edit(wrap(v));
    let head = row![
        cover(app, media, 64.0, 90.0),
        column![
            text(ui::ellipsize(media.title.preferred(), 80))
                .size(theme::T_TITLE)
                .font(theme::DISPLAY),
            ui::muted(media_facts(media)),
            ui::tag(form.status.label(kind), theme::status_color(form.status)),
        ]
        .spacing(6),
    ]
    .spacing(theme::S4)
    .align_y(Center);
    let mut progress = row![
        column![
            ui::label("Status"),
            status_pick(kind, form.status, |o| Msg::Edit(EditorEdit::Status(o))),
        ]
        .spacing(6)
        .width(Fill),
        container(ui::field(
            if kind == MediaKind::Anime {
                "Episodes"
            } else {
                "Chapters"
            },
            &form.progress,
            edit(EditorEdit::Progress)
        ))
        .width(Fill),
    ]
    .spacing(theme::S3);
    if kind == MediaKind::Manga {
        progress = progress.push(
            container(ui::field(
                "Volumes",
                &form.volumes,
                edit(EditorEdit::Volumes),
            ))
            .width(Fill),
        );
    }
    progress = progress
        .push(container(ui::field("Score", &form.score, edit(EditorEdit::Score))).width(Fill));
    let dates = row![
        ui::field("Started", &form.started, edit(EditorEdit::Started)),
        ui::field("Completed", &form.completed, edit(EditorEdit::Completed)),
        ui::field(
            if kind == MediaKind::Anime {
                "Rewatches"
            } else {
                "Rereads"
            },
            &form.repeat,
            edit(EditorEdit::Repeat)
        ),
        ui::field("Priority", &form.priority, edit(EditorEdit::Priority)),
    ]
    .spacing(theme::S3);
    let mut body = column![
        head,
        ui::divider(),
        group("Progress", progress),
        group(
            "Dates and repeats",
            column![dates, ui::muted("Dates are written as YYYY-MM-DD.")].spacing(6)
        ),
        ui::field("Notes", &form.notes, edit(EditorEdit::Notes)),
    ]
    .spacing(theme::S4);
    if !form.advanced.is_empty() {
        let fields = form
            .advanced
            .iter()
            .enumerate()
            .map(|(index, (name, value))| {
                container(ui::field(name, value, move |v| {
                    Msg::Edit(EditorEdit::Advanced(index, v))
                }))
                .width(150)
                .into()
            });
        body = body.push(group(
            "Advanced scores",
            row(fields)
                .spacing(theme::S3)
                .wrap()
                .vertical_spacing(theme::S2),
        ));
    }
    if !form.lists.is_empty() {
        let chips = form.lists.iter().enumerate().map(|(index, (name, on))| {
            ui::chip(
                name.as_str(),
                *on,
                Msg::Edit(EditorEdit::CustomList(index, !*on)),
                theme::TRACK,
            )
        });
        body = body.push(group(
            "Custom lists",
            row(chips)
                .spacing(theme::S2)
                .wrap()
                .vertical_spacing(theme::S2),
        ));
    }
    body = body.push(group(
        "Visibility",
        row![
            ui::toggle("Private", form.private, |v| Msg::Edit(EditorEdit::Private(
                v
            ))),
            ui::toggle("Hide from status lists", form.hidden, |v| Msg::Edit(
                EditorEdit::Hidden(v)
            )),
        ]
        .spacing(theme::S5),
    ));
    if let Some(error) = &editor.error {
        body = body.push(ui::error_line(error.as_str()));
    }
    let remove = if editor.confirm_delete {
        ui::danger("Remove for good", Msg::Delete)
    } else {
        ui::quiet("Remove from list", Msg::AskDelete)
    };
    body.push(ui::divider()).push(
        row![
            remove,
            ui::spacer(),
            ui::secondary("Cancel", Msg::Close),
            ui::primary("Save entry", Msg::SaveEditor),
        ]
        .spacing(theme::S2)
        .align_y(Center),
    )
}

fn add_body<'a>(app: &'a App, dialog: &'a AddDialog) -> iced::widget::Column<'a, Msg> {
    let results: Vec<Element<'a, Msg>> = dialog
        .results
        .iter()
        .map(|media| {
            let listed = app.core.entry_for(media.id).is_some();
            let action: Element<'a, Msg> = if listed {
                ui::tag("On your list", theme::TRACK)
            } else {
                ui::primary("Add", Msg::AddMedia(Box::new(media.clone())))
            };
            row![
                cover(app, media, 40.0, 56.0),
                column![
                    text(ui::ellipsize(media.title.preferred(), 48))
                        .size(theme::T_BODY)
                        .font(theme::MEDIUM),
                    ui::muted(media_facts(media)),
                ]
                .spacing(3)
                .width(Fill),
                action,
            ]
            .spacing(theme::S3)
            .padding([theme::S2, 0.0])
            .align_y(Center)
            .into()
        })
        .collect();
    let list: Element<'a, Msg> = if dialog.searching {
        ui::muted("Searching AniList…")
    } else if results.is_empty() {
        ui::muted("Search for a title, then add it with the status chosen above.")
    } else {
        ui::divided(results).into()
    };
    column![
        row![
            text_input("Search AniList", &dialog.query)
                .on_input(Msg::AddQuery)
                .on_submit(Msg::AddSearch)
                .size(theme::T_BODY)
                .padding([8, 10])
                .style(theme::input),
            ui::primary("Search", Msg::AddSearch),
        ]
        .spacing(theme::S2)
        .align_y(Center),
        column![
            ui::label("Add with status"),
            status_pick(dialog.kind, dialog.status, Msg::AddStatus),
        ]
        .spacing(6),
        list,
        row![ui::spacer(), ui::secondary("Close", Msg::Close)],
    ]
    .spacing(theme::S4)
}

fn lists_body(dialog: &ListsDialog) -> iced::widget::Column<'_, Msg> {
    let rows: Vec<Element<'_, Msg>> = dialog
        .names
        .iter()
        .map(|name| {
            let editing = dialog.renaming.as_ref().filter(|(old, _)| old == name);
            let line: Element<'_, Msg> = match editing {
                Some((_, value)) => row![
                    text_input("", value)
                        .on_input(Msg::RenameText)
                        .on_submit(Msg::CommitRename)
                        .size(theme::T_BODY)
                        .padding([8, 10])
                        .style(theme::input),
                    ui::primary("Save name", Msg::CommitRename),
                ]
                .spacing(theme::S2)
                .align_y(Center)
                .into(),
                None => row![
                    text(name).size(theme::T_BODY).width(Fill),
                    ui::icon_button(
                        Icon::Edit,
                        15.0,
                        theme::MUTED,
                        Some(Msg::StartRename(name.clone())),
                        "Rename",
                        theme::quiet
                    ),
                    ui::icon_button(
                        Icon::Trash,
                        15.0,
                        theme::BAD,
                        Some(Msg::DeleteList(name.clone())),
                        "Delete list",
                        theme::quiet
                    ),
                ]
                .spacing(theme::S1)
                .align_y(Center)
                .into(),
            };
            container(line).padding([6.0, 0.0]).into()
        })
        .collect();
    let list: Element<'_, Msg> = if rows.is_empty() {
        ui::muted("No custom lists yet. Create one below.")
    } else {
        ui::divided(rows).into()
    };
    column![
        list,
        row![
            text_input("New list name", &dialog.new_name)
                .on_input(Msg::NewListName)
                .on_submit(Msg::CreateList)
                .size(theme::T_BODY)
                .padding([8, 10])
                .style(theme::input),
            ui::primary("Create list", Msg::CreateList),
        ]
        .spacing(theme::S2)
        .align_y(Center),
        row![ui::spacer(), ui::secondary("Close", Msg::Close)],
    ]
    .spacing(theme::S4)
}

/// Legt den offenen AniList-Dialog über die Seite.
pub fn with_modal<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let Some(modal) = &app.anilist.modal else {
        return base;
    };
    let (title, width, body) = match modal {
        Modal::Editor(editor) => ("Edit entry", 640.0, editor_body(app, editor)),
        Modal::Add(dialog) => (
            if dialog.kind == MediaKind::Anime {
                "Add anime"
            } else {
                "Add manga"
            },
            580.0,
            add_body(app, dialog),
        ),
        Modal::Lists(dialog) => (
            if dialog.kind == MediaKind::Anime {
                "Custom anime lists"
            } else {
                "Custom manga lists"
            },
            480.0,
            lists_body(dialog),
        ),
    };
    ui::modal(
        base,
        title,
        width,
        Element::from(body).map(Message::AniList),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> ListEntry {
        ListEntry {
            id: 1,
            media: Media {
                id: 7,
                kind: MediaKind::Anime,
                episodes: Some(12),
                ..Media::default()
            },
            status: ListStatus::Current,
            score_raw: 80,
            progress: 3,
            progress_volumes: 0,
            repeat: 0,
            priority: 0,
            private: false,
            hidden_from_status_lists: false,
            notes: String::new(),
            custom_lists: vec![],
            advanced_scores: vec![],
            started_at: FuzzyDate::default(),
            completed_at: FuzzyDate::default(),
            updated_at: 0,
        }
    }

    #[test]
    fn untouched_form_produces_an_empty_patch() {
        let entry = entry();
        let form = EditorForm::new(&entry, ScoreFormat::Point100, None);
        assert_eq!(
            form.patch(&entry, ScoreFormat::Point100).unwrap(),
            EntryPatch::new(7)
        );
    }

    #[test]
    fn only_changed_fields_are_sent() {
        let entry = entry();
        let mut form = EditorForm::new(&entry, ScoreFormat::Point100, None);
        form.progress = "4".into();
        form.status = ListStatus::Completed;
        let patch = form.patch(&entry, ScoreFormat::Point100).unwrap();
        assert_eq!(patch.progress, Some(4));
        assert_eq!(patch.status, Some(ListStatus::Completed));
        assert_eq!(patch.score_raw, None);
    }

    #[test]
    fn invalid_numbers_are_reported() {
        let entry = entry();
        let mut form = EditorForm::new(&entry, ScoreFormat::Point100, None);
        form.progress = "many".into();
        assert!(form.patch(&entry, ScoreFormat::Point100).is_err());
    }
}
