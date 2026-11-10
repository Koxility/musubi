//! AniList-Seite: Übersicht mit Airing und Anime in Progress, Listen und Bearbeitung.

use std::fmt;

use iced::widget::{
    button, center, column, container, hover, pick_list, row, scrollable, stack, text, text_input,
    toggler,
};
use iced::{Bottom, Center, Element, Fill, Task};
use musubi_core::domain::{
    EntryPatch, FuzzyDate, LibraryItem, ListEntry, ListStatus, Media, MediaKind, ScoreFormat,
    Viewer, airing_entries, in_progress_entries,
};

use crate::app::{App, Message};
use crate::images::ImageKey;
use crate::{theme, ui};

const COVER: (f32, f32) = (130.0, 186.0);

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

fn want(key: ImageKey) -> Msg {
    Msg::WantImage(key)
}

fn cover<'a>(app: &'a App, entry: &ListEntry, width: f32, height: f32) -> Element<'a, Msg> {
    let key = entry.media.cover_url.clone().map(ImageKey::Cover);
    ui::picture(&app.images, key, width, height, want)
}

fn total(entry: &ListEntry) -> String {
    entry
        .media
        .total_units()
        .map_or("?".to_owned(), |t| t.to_string())
}

/// Karte im Stil der AniList-Startseite, mit `+` beim Überfahren.
fn progress_card<'a>(app: &'a App, entry: &ListEntry) -> Element<'a, Msg> {
    let strip: Element<'a, Msg> = match entry.media.next_airing {
        Some(next) => container(
            column![
                text(format!("Ep {}", next.episode)).size(12),
                text(ui::countdown(next.airing_at - app.now))
                    .size(12)
                    .color(theme::ACCENT),
            ]
            .align_x(Center),
        )
        .width(COVER.0)
        .padding(4)
        .style(theme::dim)
        .align_x(Center)
        .into(),
        None => iced::widget::Space::new().into(),
    };
    let image = stack![
        cover(app, entry, COVER.0, COVER.1),
        container(strip).height(COVER.1).align_y(Bottom),
    ];
    let behind = entry.episodes_behind();
    let mut info = row![text(format!("{}/{}", entry.progress, total(entry))).size(13)].spacing(6);
    if behind > 0 {
        info = info.push(ui::badge_row(format!("{behind} behind"), theme::ORANGE));
    }
    let base = button(
        column![
            image,
            text(ui::ellipsize(entry.media.title.preferred(), 18)).size(13),
            info,
        ]
        .spacing(5),
    )
    .padding(4)
    .style(theme::card_button)
    .on_press(Msg::OpenEditor(entry.clone()));
    let plus = container(
        button(text("+").size(18))
            .padding([2, 12])
            .style(theme::primary)
            .on_press(Msg::Increment(entry.clone())),
    )
    .padding(10)
    .width(COVER.0 + 8.0)
    .align_right(COVER.0 + 8.0);
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
    let cards = entries.iter().map(|e| progress_card(app, e));
    Some(
        column![
            ui::heading(title),
            row(cards).spacing(12).wrap().vertical_spacing(14)
        ]
        .spacing(12)
        .into(),
    )
}

fn overview(app: &App) -> Element<'_, Msg> {
    let state = &app.anilist;
    let mut content = column![].spacing(30).padding(iced::padding::right(14));
    let sections = [
        card_section(app, "Airing", airing_entries(&state.anime)),
        card_section(app, "Anime in Progress", in_progress_entries(&state.anime)),
        card_section(app, "Manga in Progress", in_progress_entries(&state.manga)),
    ];
    let mut empty = true;
    for section in sections.into_iter().flatten() {
        empty = false;
        content = content.push(section);
    }
    if empty {
        return center(ui::muted(if state.loading {
            "Loading your lists…"
        } else {
            "Nothing in progress. Add something to watch."
        }))
        .into();
    }
    scrollable(content).width(Fill).height(Fill).into()
}

fn list_row<'a>(app: &'a App, entry: &ListEntry, format: ScoreFormat) -> Element<'a, Msg> {
    let score = if entry.score_raw == 0 {
        "–".to_owned()
    } else {
        format.display(entry.score_raw)
    };
    let mut progress = row![text(format!("{}/{}", entry.progress, total(entry))).size(14)]
        .spacing(8)
        .align_y(Center);
    if matches!(entry.status, ListStatus::Current | ListStatus::Repeating) {
        progress = progress.push(
            button(text("+").size(13))
                .padding([1, 8])
                .style(theme::secondary)
                .on_press(Msg::Increment(entry.clone())),
        );
    }
    button(
        row![
            cover(app, entry, 46.0, 66.0),
            text(ui::ellipsize(entry.media.title.preferred(), 70))
                .size(15)
                .width(Fill),
            text(score).size(14).width(70),
            container(progress).width(120),
            text(entry.media.format.clone().unwrap_or_default())
                .size(13)
                .color(theme::MUTED)
                .width(70),
        ]
        .spacing(14)
        .align_y(Center),
    )
    .width(Fill)
    .padding(6)
    .style(theme::card_button)
    .on_press(Msg::OpenEditor(entry.clone()))
    .into()
}

fn status_filter<'a>(state: &State, kind: MediaKind) -> Element<'a, Msg> {
    let tab = |label: String, value: Option<ListStatus>, active: bool| -> Element<'a, Msg> {
        button(text(label).size(14))
            .width(Fill)
            .padding([7, 12])
            .style(theme::tab(active))
            .on_press(Msg::StatusFilter(value))
            .into()
    };
    let mut tabs = vec![tab("All".to_owned(), None, state.status.is_none())];
    tabs.extend(ListStatus::ALL.iter().map(|status| {
        tab(
            status.label(kind).to_owned(),
            Some(*status),
            state.status == Some(*status),
        )
    }));
    container(column(tabs).spacing(3)).width(170).into()
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
    let mut body = column![].spacing(20).padding(iced::padding::right(14));
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
        let rows = entries.iter().map(|e| list_row(app, e, format));
        body = body.push(
            column![
                ui::heading(status.label(kind)),
                ui::card(column(rows).spacing(2)),
            ]
            .spacing(8),
        );
    }
    row![
        column![
            text_input("Filter", &state.search)
                .on_input(Msg::Search)
                .padding(8)
                .style(theme::input),
            status_filter(state, kind),
        ]
        .spacing(12)
        .width(170),
        scrollable(body).width(Fill).height(Fill),
    ]
    .spacing(20)
    .into()
}

fn tabs(state: &State) -> Element<'_, Msg> {
    let tab = |label: &'static str, value: Tab| {
        button(text(label).size(15))
            .padding([8, 16])
            .style(theme::tab(state.tab == value))
            .on_press(Msg::Tab(value))
    };
    row![
        tab("Overview", Tab::Overview),
        tab("Anime List", Tab::Anime),
        tab("Manga List", Tab::Manga),
        ui::spacer(),
        ui::secondary("Custom lists", Msg::OpenLists),
        ui::primary("Add title", Msg::OpenAdd),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let page: Element<'_, Msg> = if !app.auth.anilist_logged_in {
        center(ui::muted(
            "Log in to AniList in Settings to see your lists.",
        ))
        .into()
    } else {
        let body = match app.anilist.tab {
            Tab::Overview => overview(app),
            _ => list_view(app),
        };
        column![tabs(&app.anilist), body]
            .spacing(20)
            .padding(24)
            .into()
    };
    page.map(Message::AniList)
}

fn editor_body(editor: &Editor) -> iced::widget::Column<'_, Msg> {
    let form = &editor.form;
    let kind = editor.entry.media.kind;
    let edit = |wrap: fn(String) -> EditorEdit| move |v: String| Msg::Edit(wrap(v));
    let mut body = column![
        column![
            text("Status").size(12).color(theme::MUTED),
            pick_list(
                status_options(kind),
                Some(StatusOption(form.status, kind)),
                |o| Msg::Edit(EditorEdit::Status(o))
            )
            .padding(8),
        ]
        .spacing(4),
        row![
            ui::field(
                if kind == MediaKind::Anime {
                    "Episodes"
                } else {
                    "Chapters"
                },
                &form.progress,
                edit(EditorEdit::Progress)
            ),
            ui::field("Score", &form.score, edit(EditorEdit::Score)),
        ]
        .spacing(10),
    ]
    .spacing(12);
    if kind == MediaKind::Manga {
        body = body.push(ui::field(
            "Volumes",
            &form.volumes,
            edit(EditorEdit::Volumes),
        ));
    }
    body = body
        .push(
            row![
                ui::field("Repeats", &form.repeat, edit(EditorEdit::Repeat)),
                ui::field("Priority", &form.priority, edit(EditorEdit::Priority)),
            ]
            .spacing(10),
        )
        .push(
            row![
                ui::field(
                    "Started (YYYY-MM-DD)",
                    &form.started,
                    edit(EditorEdit::Started)
                ),
                ui::field(
                    "Completed (YYYY-MM-DD)",
                    &form.completed,
                    edit(EditorEdit::Completed)
                ),
            ]
            .spacing(10),
        )
        .push(ui::field("Notes", &form.notes, edit(EditorEdit::Notes)))
        .push(
            toggler(form.private)
                .label("Private")
                .text_size(14)
                .on_toggle(|v| Msg::Edit(EditorEdit::Private(v))),
        )
        .push(
            toggler(form.hidden)
                .label("Hide from status lists")
                .text_size(14)
                .on_toggle(|v| Msg::Edit(EditorEdit::Hidden(v))),
        );
    for (index, (name, value)) in form.advanced.iter().enumerate() {
        body = body.push(ui::field(name, value, move |v| {
            Msg::Edit(EditorEdit::Advanced(index, v))
        }));
    }
    for (index, (name, on)) in form.lists.iter().enumerate() {
        body = body.push(
            toggler(*on)
                .label(name.as_str())
                .text_size(14)
                .on_toggle(move |v| Msg::Edit(EditorEdit::CustomList(index, v))),
        );
    }
    if let Some(error) = &editor.error {
        body = body.push(ui::error_line(error.as_str()));
    }
    let remove = if editor.confirm_delete {
        ui::danger("Really remove", Msg::Delete)
    } else {
        ui::secondary("Remove from list", Msg::AskDelete)
    };
    body.push(
        row![
            remove,
            ui::spacer(),
            ui::secondary("Cancel", Msg::Close),
            ui::primary("Save", Msg::SaveEditor),
        ]
        .spacing(8),
    )
}

fn add_body(dialog: &AddDialog) -> iced::widget::Column<'_, Msg> {
    let results = dialog.results.iter().map(|media| {
        let details = [
            media.format.clone(),
            media.season_year.map(|y| y.to_string()),
            media.total_units().map(|t| format!("{t} units")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        row![
            column![
                text(ui::ellipsize(media.title.preferred(), 52)).size(14),
                ui::muted(details),
            ]
            .width(Fill),
            ui::primary("Add", Msg::AddMedia(Box::new(media.clone()))),
        ]
        .align_y(Center)
        .into()
    });
    column![
        row![
            text_input("Search AniList", &dialog.query)
                .on_input(Msg::AddQuery)
                .on_submit(Msg::AddSearch)
                .padding(8)
                .style(theme::input),
            ui::primary("Search", Msg::AddSearch),
        ]
        .spacing(8),
        pick_list(
            status_options(dialog.kind),
            Some(StatusOption(dialog.status, dialog.kind)),
            Msg::AddStatus
        )
        .padding(8),
        if dialog.searching {
            ui::muted("Searching…")
        } else {
            column(results).spacing(10).into()
        },
        ui::secondary("Close", Msg::Close),
    ]
    .spacing(12)
}

fn lists_body(dialog: &ListsDialog) -> iced::widget::Column<'_, Msg> {
    let rows = dialog.names.iter().map(|name| {
        let editing = dialog.renaming.as_ref().filter(|(old, _)| old == name);
        match editing {
            Some((_, value)) => row![
                text_input("", value)
                    .on_input(Msg::RenameText)
                    .on_submit(Msg::CommitRename)
                    .padding(8)
                    .style(theme::input),
                ui::primary("Save", Msg::CommitRename),
            ]
            .spacing(8)
            .into(),
            None => row![
                text(name).size(14).width(Fill),
                ui::secondary("Rename", Msg::StartRename(name.clone())),
                ui::danger("Delete", Msg::DeleteList(name.clone())),
            ]
            .spacing(8)
            .align_y(Center)
            .into(),
        }
    });
    column![
        column(rows).spacing(8),
        row![
            text_input("New list", &dialog.new_name)
                .on_input(Msg::NewListName)
                .on_submit(Msg::CreateList)
                .padding(8)
                .style(theme::input),
            ui::primary("Create", Msg::CreateList),
        ]
        .spacing(8),
        ui::secondary("Close", Msg::Close),
    ]
    .spacing(14)
}

/// Legt den offenen AniList-Dialog über die Seite.
pub fn with_modal<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let Some(modal) = &app.anilist.modal else {
        return base;
    };
    let (title, width, body) = match modal {
        Modal::Editor(editor) => (
            editor.entry.media.title.preferred(),
            560.0,
            editor_body(editor),
        ),
        Modal::Add(dialog) => ("Add to list", 560.0, add_body(dialog)),
        Modal::Lists(dialog) => ("Custom lists", 480.0, lists_body(dialog)),
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
