//! Zustand und Update-Logik der TUI. Enthält kein IO und ist dadurch direkt testbar.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use image::DynamicImage;
use musubi_core::domain::{
    EntryPatch, FuzzyDate, ItemKind, Library, LibraryFilters, LibraryItem, LibraryQuery,
    LibrarySort, ListEntry, ListStatus, Mapping, MappingSource, Media, MediaKind, ScoreFormat,
    Viewer,
};
use musubi_core::{
    AuthStatus, CoreEvent, EpisodeRow, LibraryRow, OutboxItem, PlaybackState, PlayerCommand,
    ReviewItem, Service, SyncDirection, SyncLogEntry, SyncReport, SyncRow,
};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use secrecy::SecretString;

use crate::form::{Field, Form, FormOutcome};
use crate::fuzzy;
use crate::keys::{Action, Keymap};

const SEEK_STEP: f64 = 10.0;
const PAGE: usize = 15;
const POSTER_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Library,
    AniList,
    Mapping,
    Sync,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Library, Tab::AniList, Tab::Mapping, Tab::Sync];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Library => "Library",
            Tab::AniList => "AniList",
            Tab::Mapping => "Mapping",
            Tab::Sync => "Sync",
        }
    }
}

#[derive(Debug, Clone)]
pub enum PosterSource {
    Jellyfin(String),
    Url(String),
}

/// Asynchrone Arbeit, die `effects` ausführt.
pub enum Effect {
    LoadCachedLibrary,
    LoadLibrary(LibraryQuery),
    LoadChoices,
    LoadEpisodes(String),
    ReadList(MediaKind),
    RefreshList(MediaKind),
    RefreshViewer,
    ReadViewer,
    LoadMappingData,
    Play(LibraryItem, Option<String>),
    Player(PlayerCommand),
    SetPlayed(String, bool),
    Save(EntryPatch),
    Add(Media, ListStatus),
    Delete(ListEntry),
    Search(String, MediaKind),
    MatchAll,
    MapManual(LibraryItem, i64, bool),
    SetMappings(String, Vec<Mapping>),
    Unmap(String),
    LoadDiff,
    ApplySync(Vec<SyncRow>),
    LoadSyncStatus,
    FlushOutbox,
    LoginJellyfinPassword {
        url: String,
        user: String,
        password: SecretString,
    },
    LoginJellyfinKey {
        url: String,
        user: String,
        key: SecretString,
    },
    LoginAniListBrowser(String),
    LoginAniListPin(String),
    /// Beendet die Einrichtung, optional mit der gewählten Einstellung für Auto-Sync.
    FinishSetup(Option<bool>),
    LogoutJellyfin,
    LogoutAniList,
    CreateList(MediaKind, String),
    DeleteList(MediaKind, String),
    RenameList(MediaKind, String, String),
    LoadPoster(String, PosterSource),
}

/// Ergebnisse asynchroner Arbeit.
pub enum Msg {
    Library(Vec<LibraryRow>),
    Choices(LibraryFilters, Vec<Library>),
    Episodes(String, Vec<EpisodeRow>),
    List(MediaKind, Vec<ListEntry>),
    Viewer(Option<Viewer>),
    MappingData(Vec<ReviewItem>, Vec<Mapping>),
    SearchResults(Vec<Media>),
    Diff(Vec<SyncRow>),
    SyncDone(SyncReport),
    SyncStatus(Vec<OutboxItem>, Vec<SyncLogEntry>),
    LoggedIn(String, AuthStatus),
    Poster(String, DynamicImage),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    Command,
}

pub enum Purpose {
    LoginJellyfin,
    LoginAniList,
    EditEntry(Box<ListEntry>),
    Filters,
    Status(Box<ListEntry>),
    Score(Box<ListEntry>),
    MapManual(Box<LibraryItem>),
    EditMapping(Box<Mapping>),
    SearchAdd,
    AddStatus(Box<Media>),
    SetupSync,
}

/// Schritte der geführten Einrichtung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    Jellyfin,
    AniList,
    Sync,
}

/// Vorbelegung der Einrichtung aus der Konfiguration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupInfo {
    pub jellyfin_url: Option<String>,
    pub client_id: String,
    pub redirect: String,
    pub auto_sync: bool,
}

pub enum Modal {
    Help,
    Form(Box<Form>, Purpose),
    Confirm {
        text: String,
        effects: Vec<Effect>,
    },
    SearchResults {
        results: Vec<Media>,
        selected: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub ok: bool,
    pub text: String,
}

pub struct SeriesView {
    pub item: LibraryItem,
    pub episodes: Vec<EpisodeRow>,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingView {
    Review,
    Mapped,
}

pub struct App {
    pub keymap: Keymap,
    pub tab: Tab,
    pub should_quit: bool,
    pub auth: AuthStatus,
    pub viewer: Option<Viewer>,
    pub notice: Option<Notice>,
    pub playback: Option<PlaybackState>,
    pub rate_limited: Option<Duration>,
    pub match_progress: Option<(usize, usize)>,
    pub modal: Option<Modal>,
    pub setup: Option<SetupStep>,
    pub setup_info: SetupInfo,
    pub mode: InputMode,
    pub input: String,

    pub rows: Vec<LibraryRow>,
    pub library_visible: Vec<usize>,
    pub library_selected: usize,
    pub library_filter: String,
    pub query: LibraryQuery,
    pub filters: LibraryFilters,
    pub libraries: Vec<Library>,
    pub series: Option<SeriesView>,

    pub anime: Vec<ListEntry>,
    pub manga: Vec<ListEntry>,
    pub kind: MediaKind,
    pub status_filter: Option<ListStatus>,
    pub list_visible: Vec<usize>,
    pub list_selected: usize,
    pub list_filter: String,

    pub reviews: Vec<ReviewItem>,
    pub mappings: HashMap<String, Vec<Mapping>>,
    pub mapping_rows: Vec<Mapping>,
    pub mapping_view: MappingView,
    pub mapping_selected: usize,

    pub diff: Vec<SyncRow>,
    pub diff_selected: usize,
    pub pending: Vec<OutboxItem>,
    pub log: Vec<SyncLogEntry>,

    pub picker: Option<Picker>,
    pub poster: Option<StatefulProtocol>,
    pub poster_key: Option<String>,
    poster_wanted: Option<(String, PosterSource)>,
    poster_changed: Instant,
}

fn clamp_selection(selected: &mut usize, len: usize) {
    *selected = (*selected).min(len.saturating_sub(1));
}

fn parse_opt_u32(text: &str, name: &str) -> Result<Option<u32>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse()
        .map(Some)
        .map_err(|_| format!("{name} must be a whole number"))
}

fn advanced_value(entry: &ListEntry, name: &str) -> f64 {
    entry
        .advanced_scores
        .iter()
        .find(|(n, _)| n == name)
        .map_or(0.0, |(_, value)| *value)
}

/// Liest `Story=7, Art=8`. Nicht genannte Kategorien behalten ihren Wert.
fn parse_advanced(text: &str, names: &[String], entry: &ListEntry) -> Result<Vec<f64>, String> {
    let mut given: HashMap<String, f64> = HashMap::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (name, value) = part
            .split_once('=')
            .ok_or_else(|| format!("`{part}` must look like Name=7"))?;
        let name = name.trim();
        if !names.iter().any(|n| n == name) {
            return Err(format!("unknown advanced score `{name}`"));
        }
        let value: f64 = value
            .trim()
            .parse()
            .map_err(|_| format!("`{}` is not a number", value.trim()))?;
        given.insert(name.to_owned(), value);
    }
    Ok(names
        .iter()
        .map(|name| {
            given
                .get(name)
                .copied()
                .unwrap_or_else(|| advanced_value(entry, name))
        })
        .collect())
}

impl App {
    pub fn new(keymap: Keymap, picker: Option<Picker>) -> Self {
        Self {
            keymap,
            tab: Tab::Library,
            should_quit: false,
            auth: AuthStatus::default(),
            viewer: None,
            notice: None,
            playback: None,
            rate_limited: None,
            match_progress: None,
            modal: None,
            setup: None,
            setup_info: SetupInfo::default(),
            mode: InputMode::Normal,
            input: String::new(),
            rows: vec![],
            library_visible: vec![],
            library_selected: 0,
            library_filter: String::new(),
            query: LibraryQuery::default(),
            filters: LibraryFilters::default(),
            libraries: vec![],
            series: None,
            anime: vec![],
            manga: vec![],
            kind: MediaKind::Anime,
            status_filter: None,
            list_visible: vec![],
            list_selected: 0,
            list_filter: String::new(),
            reviews: vec![],
            mappings: HashMap::new(),
            mapping_rows: vec![],
            mapping_view: MappingView::Review,
            mapping_selected: 0,
            diff: vec![],
            diff_selected: 0,
            pending: vec![],
            log: vec![],
            picker,
            poster: None,
            poster_key: None,
            poster_wanted: None,
            poster_changed: Instant::now(),
        }
    }

    /// Erste Effekte nach dem Start. Ohne Anmeldung öffnet sich direkt der Login.
    pub fn startup(&mut self, auth: &AuthStatus) -> Vec<Effect> {
        self.auth = auth.clone();
        let mut effects = vec![
            Effect::LoadMappingData,
            Effect::LoadSyncStatus,
            Effect::ReadViewer,
        ];
        if auth.jellyfin_user.is_some() {
            effects.extend([
                Effect::LoadCachedLibrary,
                Effect::LoadLibrary(self.query.clone()),
                Effect::LoadChoices,
            ]);
        } else if self.setup.is_none() {
            self.open_jellyfin_login();
        }
        if auth.anilist_logged_in {
            effects.extend([
                Effect::ReadList(MediaKind::Anime),
                Effect::ReadList(MediaKind::Manga),
                Effect::RefreshList(MediaKind::Anime),
            ]);
        }
        effects
    }

    fn advanced_names(&self, kind: MediaKind) -> Vec<String> {
        self.viewer
            .as_ref()
            .map(|v| v.advanced_scoring(kind).to_vec())
            .unwrap_or_default()
    }

    pub fn score_format(&self) -> ScoreFormat {
        self.viewer
            .as_ref()
            .map(|v| v.score_format)
            .unwrap_or_default()
    }

    pub fn entries(&self) -> &[ListEntry] {
        match self.kind {
            MediaKind::Anime => &self.anime,
            MediaKind::Manga => &self.manga,
        }
    }

    pub fn entry_for(&self, media_id: i64) -> Option<&ListEntry> {
        self.anime
            .iter()
            .chain(&self.manga)
            .find(|e| e.media.id == media_id)
    }

    pub fn entry_for_item(&self, item_id: &str) -> Option<&ListEntry> {
        let first = self.mappings.get(item_id)?.first()?;
        self.entry_for(first.media_id)
    }

    pub fn needs_review(&self, item_id: &str) -> bool {
        self.reviews.iter().any(|r| r.jellyfin_id == item_id)
    }

    pub fn selected_row(&self) -> Option<&LibraryRow> {
        self.rows
            .get(*self.library_visible.get(self.library_selected)?)
    }

    pub fn selected_entry(&self) -> Option<&ListEntry> {
        self.entries()
            .get(*self.list_visible.get(self.list_selected)?)
    }

    fn recompute_library(&mut self) {
        self.library_visible = fuzzy::filter(
            self.rows.iter().map(|r| r.item.name.as_str()),
            &self.library_filter,
        );
        clamp_selection(&mut self.library_selected, self.library_visible.len());
    }

    fn recompute_list(&mut self) {
        let status = self.status_filter;
        let entries = self.entries();
        let candidates: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| status.is_none_or(|s| e.status == s))
            .map(|(i, _)| i)
            .collect();
        let visible: Vec<usize> = fuzzy::filter(
            candidates
                .iter()
                .map(|i| entries[*i].media.title.preferred()),
            &self.list_filter,
        )
        .into_iter()
        .map(|i| candidates[i])
        .collect();
        self.list_visible = visible;
        clamp_selection(&mut self.list_selected, self.list_visible.len());
    }

    fn set_notice(&mut self, ok: bool, text: impl Into<String>) {
        self.notice = Some(Notice {
            ok,
            text: text.into(),
        });
    }

    fn want_poster(&mut self) {
        if self.picker.is_none() {
            return;
        }
        let wanted = match self.tab {
            Tab::Library => self.selected_row().filter(|r| r.item.has_poster).map(|r| {
                (
                    format!("jf:{}", r.item.id),
                    PosterSource::Jellyfin(r.item.id.clone()),
                )
            }),
            Tab::AniList => self.selected_entry().and_then(|e| {
                e.media
                    .cover_url
                    .as_ref()
                    .map(|u| (format!("al:{u}"), PosterSource::Url(u.clone())))
            }),
            _ => None,
        };
        if wanted.as_ref().map(|w| &w.0) != self.poster_key.as_ref() {
            self.poster = None;
            self.poster_key = None;
            self.poster_wanted = wanted;
            self.poster_changed = Instant::now();
        }
    }

    pub fn on_tick(&mut self) -> Vec<Effect> {
        if self.poster_changed.elapsed() >= POSTER_DEBOUNCE
            && let Some((key, source)) = self.poster_wanted.take()
        {
            return vec![Effect::LoadPoster(key, source)];
        }
        vec![]
    }

    pub fn on_msg(&mut self, message: Msg) -> Vec<Effect> {
        match message {
            Msg::Library(rows) => {
                for row in &rows {
                    if !row.mappings.is_empty() {
                        self.mappings
                            .insert(row.item.id.clone(), row.mappings.clone());
                    }
                }
                self.rows = rows;
                self.recompute_library();
                self.want_poster();
            }
            Msg::Choices(filters, libraries) => {
                self.filters = filters;
                self.libraries = libraries;
            }
            Msg::Episodes(id, episodes) => {
                if let Some(series) = self.series.as_mut().filter(|s| s.item.id == id) {
                    series.episodes = episodes;
                    clamp_selection(&mut series.selected, series.episodes.len());
                }
            }
            Msg::List(kind, entries) => {
                match kind {
                    MediaKind::Anime => self.anime = entries,
                    MediaKind::Manga => self.manga = entries,
                }
                self.recompute_list();
                self.want_poster();
            }
            Msg::Viewer(viewer) => self.viewer = viewer,
            Msg::MappingData(reviews, mappings) => {
                self.reviews = reviews;
                self.mappings.clear();
                for mapping in &mappings {
                    self.mappings
                        .entry(mapping.jellyfin_id.clone())
                        .or_default()
                        .push(mapping.clone());
                }
                self.mapping_rows = mappings;
                self.clamp_mapping();
            }
            Msg::SearchResults(results) => {
                self.modal = Some(Modal::SearchResults {
                    results,
                    selected: 0,
                });
            }
            Msg::Diff(diff) => {
                self.diff = diff;
                clamp_selection(&mut self.diff_selected, self.diff.len());
            }
            Msg::SyncDone(report) => {
                self.set_notice(
                    report.failed == 0,
                    format!(
                        "Sync: {} pushed, {} episodes marked, {} conflicts, {} failed",
                        report.pushed, report.pulled, report.conflicts, report.failed
                    ),
                );
                return vec![Effect::LoadSyncStatus];
            }
            Msg::SyncStatus(pending, log) => {
                self.pending = pending;
                self.log = log;
            }
            Msg::LoggedIn(text, auth) => {
                self.set_notice(true, text);
                let mut effects = self.startup(&auth);
                if self.auth.anilist_logged_in {
                    effects.push(Effect::RefreshViewer);
                }
                if matches!(self.modal, Some(Modal::Form(_, Purpose::LoginJellyfin)))
                    && self.auth.jellyfin_user.is_some()
                {
                    self.modal = None;
                }
                self.advance_setup_after_login();
                return effects;
            }
            Msg::Poster(key, image) => {
                if let Some(picker) = &self.picker {
                    self.poster = Some(picker.new_resize_protocol(image));
                    self.poster_key = Some(key);
                }
            }
            Msg::Error(text) => {
                if let Some(Modal::Form(form, _)) = &mut self.modal {
                    form.error = Some(text.clone());
                }
                self.set_notice(false, text);
            }
        }
        vec![]
    }

    pub fn on_core_event(&mut self, event: CoreEvent) -> Vec<Effect> {
        match event {
            CoreEvent::Playback(state) => self.playback = Some(state),
            CoreEvent::PlaybackEnded => self.playback = None,
            CoreEvent::LibraryChanged => {
                let mut effects = vec![];
                if self.auth.jellyfin_user.is_some() {
                    effects.push(Effect::LoadLibrary(self.query.clone()));
                }
                if let Some(series) = &self.series {
                    effects.push(Effect::LoadEpisodes(series.item.id.clone()));
                }
                return effects;
            }
            CoreEvent::ListChanged(kind) => return vec![Effect::ReadList(kind)],
            CoreEvent::MappingsChanged => return vec![Effect::LoadMappingData],
            CoreEvent::MatchProgress { done, total } => {
                self.match_progress = (done < total).then_some((done, total));
            }
            CoreEvent::Notice { ok, message } => {
                self.set_notice(ok, message);
                return vec![Effect::LoadSyncStatus];
            }
            CoreEvent::RateLimited(wait) => self.rate_limited = wait,
            CoreEvent::AuthRequired(service) => {
                self.set_notice(false, format!("{service} session expired, use :login"));
            }
            CoreEvent::TokenExpiring { days } => {
                self.set_notice(
                    false,
                    format!("AniList token expires in {days} days, use :login anilist"),
                );
            }
        }
        vec![]
    }

    // Eingabe ----------------------------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return vec![];
        }
        if self.modal.is_some() {
            return self.on_modal_key(key);
        }
        match self.mode {
            InputMode::Normal => match self.keymap.action(key) {
                Some(action) => self.on_action(action),
                None => vec![],
            },
            InputMode::Filter => self.on_filter_key(key),
            InputMode::Command => self.on_command_key(key),
        }
    }

    fn on_filter_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = InputMode::Normal;
            }
            KeyCode::Enter => self.mode = InputMode::Normal,
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
        match self.tab {
            Tab::AniList => {
                self.list_filter = self.input.clone();
                self.recompute_list();
            }
            _ => {
                self.library_filter = self.input.clone();
                self.recompute_library();
            }
        }
        self.want_poster();
        vec![]
    }

    fn on_command_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = InputMode::Normal;
            }
            KeyCode::Enter => {
                self.mode = InputMode::Normal;
                let line = std::mem::take(&mut self.input);
                return self.run_command(&line);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
        vec![]
    }

    /// Führt eine Zeile der Befehlszeile aus.
    pub fn run_command(&mut self, line: &str) -> Vec<Effect> {
        let line = line.trim();
        let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        match command {
            "" => vec![],
            "q" | "quit" => {
                self.should_quit = true;
                vec![]
            }
            "setup" => {
                self.begin_setup();
                vec![]
            }
            "login" => {
                match rest {
                    "anilist" => self.open_anilist_login(),
                    _ => self.open_jellyfin_login(),
                }
                vec![]
            }
            "logout" => match rest {
                "jellyfin" => vec![Effect::LogoutJellyfin],
                "anilist" => vec![Effect::LogoutAniList],
                _ => {
                    self.set_notice(false, "usage: :logout jellyfin|anilist");
                    vec![]
                }
            },
            "status" => match rest.parse::<ListStatus>() {
                Ok(status) => self.patch_selected(|p, _| p.status = Some(status)),
                Err(error) => {
                    self.set_notice(false, error.to_string());
                    vec![]
                }
            },
            "score" => match self.score_format().parse(rest) {
                Ok(raw) => self.patch_selected(|p, _| p.score_raw = Some(raw)),
                Err(error) => {
                    self.set_notice(false, error.to_string());
                    vec![]
                }
            },
            "progress" => match rest.parse::<u32>() {
                Ok(value) => self.patch_selected(|p, _| p.progress = Some(value)),
                Err(_) => {
                    self.set_notice(false, "usage: :progress <number>");
                    vec![]
                }
            },
            "notes" => {
                let notes = rest.to_owned();
                self.patch_selected(|p, _| p.notes = Some(notes))
            }
            "private" => {
                let value = rest != "off";
                self.patch_selected(|p, _| p.private = Some(value))
            }
            "sync" => {
                self.tab = Tab::Sync;
                vec![Effect::LoadDiff, Effect::LoadSyncStatus]
            }
            "flush" => vec![Effect::FlushOutbox],
            "match" => vec![Effect::MatchAll],
            "list-create" if !rest.is_empty() => {
                vec![Effect::CreateList(self.kind, rest.to_owned())]
            }
            "list-delete" if !rest.is_empty() => {
                vec![Effect::DeleteList(self.kind, rest.to_owned())]
            }
            "list-rename" => match rest.split_once("->") {
                Some((old, new)) => vec![Effect::RenameList(
                    self.kind,
                    old.trim().to_owned(),
                    new.trim().to_owned(),
                )],
                None => {
                    self.set_notice(false, "usage: :list-rename old name -> new name");
                    vec![]
                }
            },
            other => {
                self.set_notice(false, format!("unknown command `{other}`"));
                vec![]
            }
        }
    }

    fn patch_selected(&mut self, change: impl FnOnce(&mut EntryPatch, &ListEntry)) -> Vec<Effect> {
        let Some(entry) = self.current_entry().cloned() else {
            self.set_notice(false, "no AniList entry selected");
            return vec![];
        };
        let mut patch = EntryPatch::new(entry.media.id);
        change(&mut patch, &entry);
        vec![Effect::Save(patch)]
    }

    /// Der Eintrag, auf den sich Bearbeitungsbefehle im aktuellen Tab beziehen.
    fn current_entry(&self) -> Option<&ListEntry> {
        match self.tab {
            Tab::AniList => self.selected_entry(),
            Tab::Library => self.entry_for_item(&self.selected_row()?.item.id),
            _ => None,
        }
    }

    fn on_action(&mut self, action: Action) -> Vec<Effect> {
        if let Some(effects) = self.on_global_action(action) {
            return effects;
        }
        match self.tab {
            Tab::Library => self.on_library_action(action),
            Tab::AniList => self.on_list_action(action),
            Tab::Mapping => self.on_mapping_action(action),
            Tab::Sync => self.on_sync_action(action),
        }
    }

    fn switch_tab(&mut self, tab: Tab) -> Vec<Effect> {
        self.tab = tab;
        self.want_poster();
        match tab {
            Tab::Sync => vec![Effect::LoadDiff, Effect::LoadSyncStatus],
            Tab::Mapping => vec![Effect::LoadMappingData],
            _ => vec![],
        }
    }

    fn tab_offset(&self, delta: isize) -> Tab {
        let index = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0) as isize;
        Tab::ALL[(index + delta).rem_euclid(Tab::ALL.len() as isize) as usize]
    }

    fn on_global_action(&mut self, action: Action) -> Option<Vec<Effect>> {
        let player = |command| Some(vec![Effect::Player(command)]);
        match action {
            Action::Quit => {
                self.should_quit = true;
                Some(vec![])
            }
            Action::Help => {
                self.modal = Some(Modal::Help);
                Some(vec![])
            }
            Action::NextTab => Some(self.switch_tab(self.tab_offset(1))),
            Action::PrevTab => Some(self.switch_tab(self.tab_offset(-1))),
            Action::Tab1 => Some(self.switch_tab(Tab::Library)),
            Action::Tab2 => Some(self.switch_tab(Tab::AniList)),
            Action::Tab3 => Some(self.switch_tab(Tab::Mapping)),
            Action::Tab4 => Some(self.switch_tab(Tab::Sync)),
            Action::Command => {
                self.input.clear();
                self.mode = InputMode::Command;
                Some(vec![])
            }
            Action::PlayerPause => player(PlayerCommand::TogglePause),
            Action::PlayerBack => player(PlayerCommand::SeekRelative(-SEEK_STEP)),
            Action::PlayerForward => player(PlayerCommand::SeekRelative(SEEK_STEP)),
            Action::PlayerNext => player(PlayerCommand::Next),
            Action::PlayerPrevious => player(PlayerCommand::Previous),
            Action::PlayerStop => player(PlayerCommand::Stop),
            Action::PlayerSubtitle => player(PlayerCommand::CycleSubtitle),
            Action::PlayerAudio => player(PlayerCommand::CycleAudio),
            _ => None,
        }
    }

    /// Wendet Bewegung auf eine Auswahl an. Gibt `true` zurück, wenn es eine Bewegung war.
    fn apply_motion(action: Action, selected: &mut usize, len: usize) -> bool {
        let last = len.saturating_sub(1);
        match action {
            Action::Down => *selected = (*selected + 1).min(last),
            Action::Up => *selected = selected.saturating_sub(1),
            Action::Top => *selected = 0,
            Action::Bottom => *selected = last,
            Action::PageDown => *selected = (*selected + PAGE).min(last),
            Action::PageUp => *selected = selected.saturating_sub(PAGE),
            _ => return false,
        }
        true
    }

    fn start_filter(&mut self, current: String) {
        self.input = current;
        self.mode = InputMode::Filter;
    }

    fn on_library_action(&mut self, action: Action) -> Vec<Effect> {
        if self.series.is_some() {
            return self.on_series_action(action);
        }
        let len = self.library_visible.len();
        if Self::apply_motion(action, &mut self.library_selected, len) {
            self.want_poster();
            return vec![];
        }
        match action {
            Action::Filter => {
                self.start_filter(self.library_filter.clone());
                vec![]
            }
            Action::Back => {
                self.library_filter.clear();
                self.recompute_library();
                vec![]
            }
            Action::Refresh => vec![Effect::LoadLibrary(self.query.clone()), Effect::LoadChoices],
            Action::ServerFilter => {
                self.open_filter_form();
                vec![]
            }
            Action::Open => {
                let Some(item) = self.selected_row().map(|r| r.item.clone()) else {
                    return vec![];
                };
                if item.kind == ItemKind::Movie {
                    return vec![Effect::Play(item, None)];
                }
                let id = item.id.clone();
                self.series = Some(SeriesView {
                    item,
                    episodes: vec![],
                    selected: 0,
                });
                vec![Effect::LoadEpisodes(id)]
            }
            Action::Play | Action::Continue => match self.selected_row() {
                Some(row) => vec![Effect::Play(row.item.clone(), None)],
                None => vec![],
            },
            Action::TogglePlayed => match self.selected_row() {
                Some(row) => vec![Effect::SetPlayed(row.item.id.clone(), !row.item.played)],
                None => vec![],
            },
            Action::Map => {
                if let Some(item) = self.selected_row().map(|r| r.item.clone()) {
                    self.open_map_form(item);
                }
                vec![]
            }
            Action::MatchAll => vec![Effect::MatchAll],
            Action::Increment
            | Action::Decrement
            | Action::Status
            | Action::Score
            | Action::Edit => self.on_entry_action(action),
            _ => vec![],
        }
    }

    fn on_series_action(&mut self, action: Action) -> Vec<Effect> {
        let Some(series) = self.series.as_mut() else {
            return vec![];
        };
        let len = series.episodes.len();
        if Self::apply_motion(action, &mut series.selected, len) {
            return vec![];
        }
        let item = series.item.clone();
        let current = series
            .episodes
            .get(series.selected)
            .map(|e| e.episode.clone());
        match action {
            Action::Back => {
                self.series = None;
                vec![]
            }
            Action::Open | Action::Play => match current {
                Some(episode) => vec![Effect::Play(item, Some(episode.id))],
                None => vec![],
            },
            Action::Continue => vec![Effect::Play(item, None)],
            Action::TogglePlayed => match current {
                Some(episode) => vec![Effect::SetPlayed(episode.id, !episode.played)],
                None => vec![],
            },
            Action::Refresh => vec![Effect::LoadEpisodes(item.id)],
            Action::Map => {
                self.open_map_form(item);
                vec![]
            }
            _ => vec![],
        }
    }

    fn on_entry_action(&mut self, action: Action) -> Vec<Effect> {
        let Some(entry) = self.current_entry().cloned() else {
            self.set_notice(false, "no AniList entry for this title");
            return vec![];
        };
        match action {
            Action::Increment | Action::Decrement => {
                let progress = if action == Action::Increment {
                    let next = entry.progress + 1;
                    entry.media.total_units().map_or(next, |t| next.min(t))
                } else {
                    entry.progress.saturating_sub(1)
                };
                let patch = EntryPatch {
                    progress: Some(progress),
                    ..EntryPatch::new(entry.media.id)
                };
                vec![Effect::Save(patch)]
            }
            Action::Status => {
                self.open_status_form(entry);
                vec![]
            }
            Action::Score => {
                self.open_score_form(entry);
                vec![]
            }
            Action::Edit => {
                self.open_entry_form(entry);
                vec![]
            }
            _ => vec![],
        }
    }

    fn on_list_action(&mut self, action: Action) -> Vec<Effect> {
        let len = self.list_visible.len();
        if Self::apply_motion(action, &mut self.list_selected, len) {
            self.want_poster();
            return vec![];
        }
        match action {
            Action::Filter => {
                self.start_filter(self.list_filter.clone());
                vec![]
            }
            Action::Back => {
                self.list_filter.clear();
                self.status_filter = None;
                self.recompute_list();
                vec![]
            }
            Action::Refresh => vec![Effect::RefreshList(self.kind), Effect::RefreshViewer],
            Action::Toggle => {
                self.kind = self.kind.other();
                self.list_selected = 0;
                self.recompute_list();
                self.want_poster();
                if self.entries().is_empty() && self.auth.anilist_logged_in {
                    vec![Effect::RefreshList(self.kind)]
                } else {
                    vec![]
                }
            }
            Action::NextStatus | Action::PrevStatus => {
                self.cycle_status_filter(action == Action::NextStatus);
                vec![]
            }
            Action::Add => {
                self.modal = Some(Modal::Form(
                    Box::new(Form::new(
                        format!("Add {}", self.kind.label()),
                        vec![Field::text("query", "Search AniList", "")],
                    )),
                    Purpose::SearchAdd,
                ));
                vec![]
            }
            Action::Delete => {
                if let Some(entry) = self.selected_entry().cloned() {
                    self.modal = Some(Modal::Confirm {
                        text: format!("Remove “{}” from your list?", entry.media.title.preferred()),
                        effects: vec![Effect::Delete(entry)],
                    });
                }
                vec![]
            }
            Action::Open
            | Action::Edit
            | Action::Increment
            | Action::Decrement
            | Action::Status
            | Action::Score => {
                let mapped = if action == Action::Open {
                    Action::Edit
                } else {
                    action
                };
                self.on_entry_action(mapped)
            }
            _ => vec![],
        }
    }

    fn cycle_status_filter(&mut self, forward: bool) {
        let mut options: Vec<Option<ListStatus>> = vec![None];
        options.extend(ListStatus::ALL.map(Some));
        let current = options
            .iter()
            .position(|o| *o == self.status_filter)
            .unwrap_or(0);
        let len = options.len();
        let next = if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        };
        self.status_filter = options[next];
        self.list_selected = 0;
        self.recompute_list();
        self.want_poster();
    }

    fn mapping_len(&self) -> usize {
        match self.mapping_view {
            MappingView::Review => self.reviews.len(),
            MappingView::Mapped => self.mapping_rows.len(),
        }
    }

    fn clamp_mapping(&mut self) {
        let len = self.mapping_len();
        clamp_selection(&mut self.mapping_selected, len);
    }

    fn item_by_id(&self, id: &str) -> Option<LibraryItem> {
        self.rows
            .iter()
            .find(|r| r.item.id == id)
            .map(|r| r.item.clone())
    }

    fn on_mapping_action(&mut self, action: Action) -> Vec<Effect> {
        let len = self.mapping_len();
        if Self::apply_motion(action, &mut self.mapping_selected, len) {
            return vec![];
        }
        match action {
            Action::Toggle => {
                self.mapping_view = match self.mapping_view {
                    MappingView::Review => MappingView::Mapped,
                    MappingView::Mapped => MappingView::Review,
                };
                self.mapping_selected = 0;
                vec![]
            }
            Action::Refresh => vec![Effect::LoadMappingData],
            Action::MatchAll => vec![Effect::MatchAll],
            Action::Open | Action::Map if self.mapping_view == MappingView::Review => {
                let Some(review) = self.reviews.get(self.mapping_selected) else {
                    return vec![];
                };
                let id = review.jellyfin_id.clone();
                match self.item_by_id(&id) {
                    Some(item) => self.open_map_form(item),
                    None => self.set_notice(false, "title is not in the loaded library"),
                }
                vec![]
            }
            Action::Edit | Action::Open if self.mapping_view == MappingView::Mapped => {
                if let Some(mapping) = self.mapping_rows.get(self.mapping_selected).cloned() {
                    self.open_mapping_form(mapping);
                }
                vec![]
            }
            Action::Delete if self.mapping_view == MappingView::Mapped => {
                match self.mapping_rows.get(self.mapping_selected) {
                    Some(mapping) => {
                        let id = mapping.jellyfin_id.clone();
                        self.modal = Some(Modal::Confirm {
                            text: format!("Remove all mappings of “{}”?", mapping.title),
                            effects: vec![Effect::Unmap(id)],
                        });
                        vec![]
                    }
                    None => vec![],
                }
            }
            _ => vec![],
        }
    }

    fn on_sync_action(&mut self, action: Action) -> Vec<Effect> {
        let len = self.diff.len();
        if Self::apply_motion(action, &mut self.diff_selected, len) {
            return vec![];
        }
        match action {
            Action::Refresh => vec![Effect::LoadDiff, Effect::LoadSyncStatus],
            Action::Delete => {
                if let Some(row) = self.diff.get_mut(self.diff_selected) {
                    row.direction = match row.direction {
                        SyncDirection::Push => SyncDirection::Pull,
                        SyncDirection::Pull => SyncDirection::Skip,
                        SyncDirection::Skip => SyncDirection::Push,
                    };
                }
                vec![]
            }
            Action::Apply | Action::Open => {
                let rows: Vec<SyncRow> = self
                    .diff
                    .iter()
                    .filter(|r| r.direction != SyncDirection::Skip)
                    .cloned()
                    .collect();
                if rows.is_empty() {
                    return vec![];
                }
                self.modal = Some(Modal::Confirm {
                    text: format!("Apply {} sync changes?", rows.len()),
                    effects: vec![Effect::ApplySync(rows)],
                });
                vec![]
            }
            Action::Add => vec![Effect::FlushOutbox, Effect::LoadSyncStatus],
            _ => vec![],
        }
    }

    // Dialoge ----------------------------------------------------------------------------

    fn on_modal_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let Some(modal) = self.modal.take() else {
            return vec![];
        };
        match modal {
            Modal::Help => vec![],
            Modal::Confirm { text, effects } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => effects,
                KeyCode::Char('n') | KeyCode::Esc => vec![],
                _ => {
                    self.modal = Some(Modal::Confirm { text, effects });
                    vec![]
                }
            },
            Modal::SearchResults {
                results,
                mut selected,
            } => {
                match self.keymap.action(key) {
                    Some(Action::Back) | Some(Action::Quit) => return vec![],
                    Some(Action::Open) => {
                        if let Some(media) = results.get(selected).cloned() {
                            self.open_add_status_form(media);
                        }
                        return vec![];
                    }
                    Some(action) => {
                        Self::apply_motion(action, &mut selected, results.len());
                    }
                    None => {}
                }
                self.modal = Some(Modal::SearchResults { results, selected });
                vec![]
            }
            Modal::Form(mut form, purpose) => match form.on_key(key) {
                FormOutcome::Cancel => self.skip_setup_step(&purpose),
                FormOutcome::Editing => {
                    self.modal = Some(Modal::Form(form, purpose));
                    vec![]
                }
                FormOutcome::Submit => match self.submit_form(&form, &purpose) {
                    Ok((effects, keep_open)) => {
                        if keep_open {
                            form.error = None;
                            self.modal = Some(Modal::Form(form, purpose));
                        }
                        effects
                    }
                    Err(error) => {
                        form.error = Some(error);
                        self.modal = Some(Modal::Form(form, purpose));
                        vec![]
                    }
                },
            },
        }
    }

    pub fn open_jellyfin_login(&mut self) {
        let url = self.auth.jellyfin_url.clone().unwrap_or_default();
        let form = Form::new(
            "Log in to Jellyfin",
            vec![
                Field::text("url", "Server URL", url),
                Field::text("user", "Username", ""),
                Field::secret("password", "Password"),
                Field::secret("key", "or API key"),
            ],
        )
        .with_hint("The password is used once and never stored. Esc to skip.");
        self.modal = Some(Modal::Form(Box::new(form), Purpose::LoginJellyfin));
    }

    fn open_anilist_login(&mut self) {
        let form = Form::new(
            "Log in to AniList",
            vec![
                Field::text("client_id", "Client ID", self.setup_info.client_id.clone()),
                Field::secret("token", "Token or redirect URL"),
            ],
        )
        .with_hint("Leave the token empty to log in through the browser.");
        self.modal = Some(Modal::Form(Box::new(form), Purpose::LoginAniList));
    }

    /// Startet die Einrichtung beim ersten fehlenden Login.
    pub fn begin_setup(&mut self) {
        let step = if self.auth.jellyfin_user.is_some() && !self.auth.anilist_logged_in {
            SetupStep::AniList
        } else {
            SetupStep::Jellyfin
        };
        self.open_setup_step(step);
    }

    fn open_setup_step(&mut self, step: SetupStep) {
        self.setup = Some(step);
        match step {
            SetupStep::Jellyfin => {
                let url = self
                    .auth
                    .jellyfin_url
                    .clone()
                    .or_else(|| self.setup_info.jellyfin_url.clone())
                    .unwrap_or_default();
                let form = Form::new(
                    "Setup 1/3: Jellyfin",
                    vec![
                        Field::text("url", "Server URL", url),
                        Field::text(
                            "user",
                            "Username",
                            self.auth.jellyfin_user.clone().unwrap_or_default(),
                        ),
                        Field::secret("password", "Password"),
                        Field::secret("key", "or API key"),
                    ],
                )
                .with_hint(
                    "Include the port, for example 192.168.1.10:8096.\n\
                     The password is used once and never stored. Esc skips this step.",
                );
                self.modal = Some(Modal::Form(Box::new(form), Purpose::LoginJellyfin));
            }
            SetupStep::AniList => {
                let form = Form::new(
                    "Setup 2/3: AniList",
                    vec![
                        Field::text("client_id", "Client ID", self.setup_info.client_id.clone()),
                        Field::secret("token", "Token or redirect URL"),
                    ],
                )
                .with_hint(format!(
                    "Create an app at anilist.co/settings/developer\n\
                     with the redirect URL {}\n\
                     Leave the token empty to log in through the browser. Esc skips.",
                    self.setup_info.redirect
                ));
                self.modal = Some(Modal::Form(Box::new(form), Purpose::LoginAniList));
            }
            SetupStep::Sync => {
                let form = Form::new(
                    "Setup 3/3: Sync",
                    vec![Field::toggle(
                        "auto_sync",
                        "Update AniList automatically while watching",
                        self.setup_info.auto_sync,
                    )],
                )
                .with_hint(
                    "When off, AniList only changes when you apply the Sync tab.\n\
                     Enter finishes the setup.",
                );
                self.modal = Some(Modal::Form(Box::new(form), Purpose::SetupSync));
            }
        }
    }

    fn advance_setup_after_login(&mut self) {
        match self.setup {
            Some(SetupStep::Jellyfin) if self.auth.jellyfin_user.is_some() => {
                self.open_setup_step(SetupStep::AniList);
            }
            Some(SetupStep::AniList) if self.auth.anilist_logged_in => {
                self.open_setup_step(SetupStep::Sync);
            }
            _ => {}
        }
    }

    /// Esc überspringt den aktuellen Schritt, im letzten Schritt endet die Einrichtung.
    fn skip_setup_step(&mut self, purpose: &Purpose) -> Vec<Effect> {
        match (self.setup, purpose) {
            (Some(SetupStep::Jellyfin), Purpose::LoginJellyfin) => {
                self.open_setup_step(SetupStep::AniList);
                vec![]
            }
            (Some(SetupStep::AniList), Purpose::LoginAniList) => {
                self.open_setup_step(SetupStep::Sync);
                vec![]
            }
            (Some(SetupStep::Sync), Purpose::SetupSync) => {
                self.setup = None;
                vec![Effect::FinishSetup(None)]
            }
            _ => vec![],
        }
    }

    fn status_field(&self, current: ListStatus, kind: MediaKind) -> Field {
        Field::choice(
            "status",
            "Status",
            ListStatus::ALL
                .iter()
                .map(|s| s.label(kind).to_owned())
                .collect(),
            current.label(kind),
        )
    }

    fn open_status_form(&mut self, entry: ListEntry) {
        let form = Form::new(
            entry.media.title.preferred().to_owned(),
            vec![self.status_field(entry.status, entry.media.kind)],
        );
        self.modal = Some(Modal::Form(
            Box::new(form),
            Purpose::Status(Box::new(entry)),
        ));
    }

    fn open_score_form(&mut self, entry: ListEntry) {
        let format = self.score_format();
        let shown = if entry.score_raw == 0 {
            String::new()
        } else {
            format.display(entry.score_raw)
        };
        let form = Form::new(
            entry.media.title.preferred().to_owned(),
            vec![Field::text(
                "score",
                format!("Score ({})", format.hint()),
                shown,
            )],
        );
        self.modal = Some(Modal::Form(Box::new(form), Purpose::Score(Box::new(entry))));
    }

    fn open_entry_form(&mut self, entry: ListEntry) {
        let format = self.score_format();
        let shown = if entry.score_raw == 0 {
            String::new()
        } else {
            format.display(entry.score_raw)
        };
        let total = entry
            .media
            .total_units()
            .map_or("?".to_owned(), |t| t.to_string());
        let mut fields = vec![
            self.status_field(entry.status, entry.media.kind),
            Field::text("score", format!("Score ({})", format.hint()), shown),
            Field::text(
                "progress",
                format!("Progress (of {total})"),
                entry.progress.to_string(),
            ),
        ];
        if entry.media.kind == MediaKind::Manga {
            fields.push(Field::text(
                "volumes",
                "Volumes",
                entry.progress_volumes.to_string(),
            ));
        }
        let advanced = self.advanced_names(entry.media.kind);
        if !advanced.is_empty() {
            let current: Vec<String> = advanced
                .iter()
                .map(|name| format!("{name}={}", advanced_value(&entry, name)))
                .collect();
            fields.push(Field::text(
                "advanced",
                "Advanced scores",
                current.join(", "),
            ));
        }
        fields.extend([
            Field::text("repeat", "Repeats", entry.repeat.to_string()),
            Field::text("priority", "Priority", entry.priority.to_string()),
            Field::text(
                "started",
                "Started (YYYY-MM-DD)",
                entry.started_at.to_string(),
            ),
            Field::text(
                "completed",
                "Completed (YYYY-MM-DD)",
                entry.completed_at.to_string(),
            ),
            Field::toggle("private", "Private", entry.private),
            Field::toggle(
                "hidden",
                "Hide from status lists",
                entry.hidden_from_status_lists,
            ),
            Field::text(
                "lists",
                "Custom lists (comma separated)",
                entry.custom_lists.join(", "),
            ),
            Field::text("notes", "Notes", entry.notes.clone()),
        ]);
        let form = Form::new(format!("Edit: {}", entry.media.title.preferred()), fields);
        self.modal = Some(Modal::Form(
            Box::new(form),
            Purpose::EditEntry(Box::new(entry)),
        ));
    }

    fn open_filter_form(&mut self) {
        let mut library_names = vec!["All".to_owned()];
        library_names.extend(self.libraries.iter().map(|l| l.name.clone()));
        let library = self
            .query
            .library_id
            .as_ref()
            .and_then(|id| self.libraries.iter().find(|l| l.id == *id))
            .map_or("All".to_owned(), |l| l.name.clone());
        let years: Vec<String> = self.query.years.iter().map(i32::to_string).collect();
        let kind = match self.query.kind {
            Some(ItemKind::Series) => "Series",
            Some(ItemKind::Movie) => "Movies",
            None => "All",
        };
        let sort = match self.query.sort {
            LibrarySort::Name => "Name",
            LibrarySort::Year => "Year",
            LibrarySort::DateAdded => "Date added",
            LibrarySort::Rating => "Rating",
        };
        let choices = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let form = Form::new(
            "Library filters",
            vec![
                Field::choice("library", "Library", library_names, library),
                Field::text(
                    "search",
                    "Search",
                    self.query.search.clone().unwrap_or_default(),
                ),
                Field::text(
                    "genres",
                    "Genres (comma separated)",
                    self.query.genres.join(", "),
                ),
                Field::text("years", "Years (comma separated)", years.join(", ")),
                Field::choice(
                    "status",
                    "Status",
                    choices(&["Any", "Continuing", "Ended"]),
                    self.query
                        .status
                        .clone()
                        .unwrap_or_else(|| "Any".to_owned()),
                ),
                Field::choice("kind", "Type", choices(&["All", "Series", "Movies"]), kind),
                Field::toggle("unplayed", "Unplayed only", self.query.unplayed_only),
                Field::choice(
                    "sort",
                    "Sort",
                    choices(&["Name", "Year", "Date added", "Rating"]),
                    sort,
                ),
            ],
        )
        .with_hint(if self.filters.genres.is_empty() {
            String::new()
        } else {
            format!("Genres: {}", self.filters.genres.join(", "))
        });
        self.modal = Some(Modal::Form(Box::new(form), Purpose::Filters));
    }

    fn open_map_form(&mut self, item: LibraryItem) {
        let candidates = self
            .reviews
            .iter()
            .find(|r| r.jellyfin_id == item.id)
            .map(|r| {
                r.candidates
                    .iter()
                    .map(|c| format!("{}: {} ({:.2})", c.media_id, c.title, c.score))
                    .collect::<Vec<_>>()
                    .join("  |  ")
            })
            .unwrap_or_default();
        let first = self
            .reviews
            .iter()
            .find(|r| r.jellyfin_id == item.id)
            .and_then(|r| r.candidates.first());
        let form = Form::new(
            format!("Map: {}", item.name),
            vec![
                Field::text(
                    "media",
                    "AniList ID",
                    first.map(|c| c.media_id.to_string()).unwrap_or_default(),
                ),
                Field::toggle("follow", "Assign sequels to later seasons", true),
            ],
        )
        .with_hint(candidates);
        self.modal = Some(Modal::Form(
            Box::new(form),
            Purpose::MapManual(Box::new(item)),
        ));
    }

    fn open_mapping_form(&mut self, mapping: Mapping) {
        let form = Form::new(
            format!("Mapping: {}", mapping.title),
            vec![
                Field::text("media", "AniList ID", mapping.media_id.to_string()),
                Field::text("season", "Season", mapping.season.to_string()),
                Field::text("start", "First episode", mapping.ep_start.to_string()),
                Field::text(
                    "end",
                    "Last episode (empty = open)",
                    mapping.ep_end.map(|e| e.to_string()).unwrap_or_default(),
                ),
                Field::text(
                    "offset",
                    "Offset (AniList = Jellyfin + offset)",
                    mapping.offset.to_string(),
                ),
            ],
        );
        self.modal = Some(Modal::Form(
            Box::new(form),
            Purpose::EditMapping(Box::new(mapping)),
        ));
    }

    fn open_add_status_form(&mut self, media: Media) {
        let form = Form::new(
            format!("Add: {}", media.title.preferred()),
            vec![self.status_field(ListStatus::Planning, media.kind)],
        );
        self.modal = Some(Modal::Form(
            Box::new(form),
            Purpose::AddStatus(Box::new(media)),
        ));
    }

    fn parse_status(value: &str, kind: MediaKind) -> Result<ListStatus, String> {
        ListStatus::ALL
            .into_iter()
            .find(|s| s.label(kind) == value)
            .ok_or_else(|| format!("unknown status {value}"))
    }

    /// Wertet ein Formular aus. Liefert Effekte und ob der Dialog offen bleibt.
    fn submit_form(
        &mut self,
        form: &Form,
        purpose: &Purpose,
    ) -> Result<(Vec<Effect>, bool), String> {
        match purpose {
            Purpose::LoginJellyfin => {
                let url = form.value("url").trim().to_owned();
                let user = form.value("user").trim().to_owned();
                if url.is_empty() || user.is_empty() {
                    return Err("server URL and username are required".to_owned());
                }
                let key = form.value("key").to_owned();
                let effect = if key.is_empty() {
                    let password = SecretString::from(form.value("password").to_owned());
                    Effect::LoginJellyfinPassword {
                        url,
                        user,
                        password,
                    }
                } else {
                    Effect::LoginJellyfinKey {
                        url,
                        user,
                        key: SecretString::from(key),
                    }
                };
                Ok((vec![effect], true))
            }
            Purpose::LoginAniList => {
                // In der Einrichtung bleibt das Formular offen, damit Fehler dort erscheinen.
                let keep_open = self.setup.is_some();
                let token = form.value("token").trim().to_owned();
                if !token.is_empty() {
                    return Ok((vec![Effect::LoginAniListPin(token)], keep_open));
                }
                let client_id = form.value("client_id").trim().to_owned();
                if client_id.is_empty() {
                    return Err("enter a client ID or paste a token".to_owned());
                }
                self.setup_info.client_id = client_id.clone();
                self.set_notice(true, "Waiting for the browser…");
                Ok((vec![Effect::LoginAniListBrowser(client_id)], keep_open))
            }
            Purpose::SetupSync => {
                let auto_sync = form.is_on("auto_sync");
                self.setup = None;
                self.setup_info.auto_sync = auto_sync;
                self.set_notice(true, "Setup complete");
                Ok((vec![Effect::FinishSetup(Some(auto_sync))], false))
            }
            Purpose::Status(entry) => {
                let status = Self::parse_status(form.value("status"), entry.media.kind)?;
                let patch = EntryPatch {
                    status: Some(status),
                    ..EntryPatch::new(entry.media.id)
                };
                Ok((vec![Effect::Save(patch)], false))
            }
            Purpose::Score(entry) => {
                let raw = self
                    .score_format()
                    .parse(form.value("score"))
                    .map_err(|e| e.to_string())?;
                let patch = EntryPatch {
                    score_raw: Some(raw),
                    ..EntryPatch::new(entry.media.id)
                };
                Ok((vec![Effect::Save(patch)], false))
            }
            Purpose::EditEntry(entry) => {
                let patch = self.patch_from_form(form, entry)?;
                Ok((vec![Effect::Save(patch)], false))
            }
            Purpose::Filters => {
                self.apply_filter_form(form)?;
                Ok((vec![Effect::LoadLibrary(self.query.clone())], false))
            }
            Purpose::MapManual(item) => {
                let media: i64 = form
                    .value("media")
                    .trim()
                    .parse()
                    .map_err(|_| "AniList ID must be a number".to_owned())?;
                Ok((
                    vec![Effect::MapManual(
                        (**item).clone(),
                        media,
                        form.is_on("follow"),
                    )],
                    false,
                ))
            }
            Purpose::EditMapping(old) => {
                let updated = Self::mapping_from_form(form, old)?;
                let mut all: Vec<Mapping> = self
                    .mappings
                    .get(&old.jellyfin_id)
                    .cloned()
                    .unwrap_or_default();
                match all
                    .iter_mut()
                    .find(|m| m.season == old.season && m.ep_start == old.ep_start)
                {
                    Some(slot) => *slot = updated,
                    None => all.push(updated),
                }
                Ok((
                    vec![Effect::SetMappings(old.jellyfin_id.clone(), all)],
                    false,
                ))
            }
            Purpose::SearchAdd => {
                let query = form.value("query").trim().to_owned();
                if query.is_empty() {
                    return Err("enter a title".to_owned());
                }
                Ok((vec![Effect::Search(query, self.kind)], false))
            }
            Purpose::AddStatus(media) => {
                let status = Self::parse_status(form.value("status"), media.kind)?;
                Ok((vec![Effect::Add((**media).clone(), status)], false))
            }
        }
    }

    /// Baut das Update und sendet nur Felder, die sich wirklich geändert haben.
    fn patch_from_form(&self, form: &Form, entry: &ListEntry) -> Result<EntryPatch, String> {
        let mut patch = EntryPatch::new(entry.media.id);
        let status = Self::parse_status(form.value("status"), entry.media.kind)?;
        if status != entry.status {
            patch.status = Some(status);
        }
        let score = self
            .score_format()
            .parse(form.value("score"))
            .map_err(|e| e.to_string())?;
        if score != entry.score_raw && !self.same_displayed_score(form, entry) {
            patch.score_raw = Some(score);
        }
        if form.field("advanced").is_some() {
            let names = self.advanced_names(entry.media.kind);
            let values = parse_advanced(form.value("advanced"), &names, entry)?;
            let unchanged = names
                .iter()
                .zip(&values)
                .all(|(name, value)| advanced_value(entry, name) == *value);
            if !unchanged {
                patch.advanced_scores = Some(values);
            }
        }
        let number = |key: &str, name: &str, current: u32| -> Result<Option<u32>, String> {
            Ok(parse_opt_u32(form.value(key), name)?.filter(|v| *v != current))
        };
        patch.progress = number("progress", "progress", entry.progress)?;
        if form.field("volumes").is_some() {
            patch.progress_volumes = number("volumes", "volumes", entry.progress_volumes)?;
        }
        patch.repeat = number("repeat", "repeats", entry.repeat)?;
        patch.priority = number("priority", "priority", entry.priority)?;
        let date = |key: &str, current: FuzzyDate| -> Result<Option<FuzzyDate>, String> {
            let parsed: FuzzyDate = form
                .value(key)
                .parse()
                .map_err(|e: musubi_core::domain::ParseDateError| e.to_string())?;
            Ok((parsed != current).then_some(parsed))
        };
        patch.started_at = date("started", entry.started_at)?;
        patch.completed_at = date("completed", entry.completed_at)?;
        let private = form.is_on("private");
        patch.private = (private != entry.private).then_some(private);
        let hidden = form.is_on("hidden");
        patch.hidden_from_status_lists =
            (hidden != entry.hidden_from_status_lists).then_some(hidden);
        let lists: Vec<String> = form
            .value("lists")
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        let known: HashSet<&String> = entry.custom_lists.iter().collect();
        if lists.len() != entry.custom_lists.len() || !lists.iter().all(|l| known.contains(l)) {
            patch.custom_lists = Some(lists);
        }
        if form.value("notes") != entry.notes {
            patch.notes = Some(form.value("notes").to_owned());
        }
        Ok(patch)
    }

    /// Grobe Formate runden: Unveränderte Anzeige darf den genauen Rohwert nicht überschreiben.
    fn same_displayed_score(&self, form: &Form, entry: &ListEntry) -> bool {
        entry.score_raw != 0
            && form.value("score").trim() == self.score_format().display(entry.score_raw)
    }

    fn mapping_from_form(form: &Form, old: &Mapping) -> Result<Mapping, String> {
        let required = |key: &str, name: &str| -> Result<u32, String> {
            parse_opt_u32(form.value(key), name)?.ok_or_else(|| format!("{name} is required"))
        };
        let end = parse_opt_u32(form.value("end"), "last episode")?;
        let start = required("start", "first episode")?;
        if end.is_some_and(|e| e < start) {
            return Err("last episode must not be before the first".to_owned());
        }
        Ok(Mapping {
            jellyfin_id: old.jellyfin_id.clone(),
            season: required("season", "season")?,
            ep_start: start,
            ep_end: end,
            media_id: form
                .value("media")
                .trim()
                .parse()
                .map_err(|_| "AniList ID must be a number".to_owned())?,
            offset: form
                .value("offset")
                .trim()
                .parse()
                .map_err(|_| "offset must be a whole number".to_owned())?,
            source: MappingSource::Manual,
            confidence: 1.0,
            title: old.title.clone(),
        })
    }

    fn apply_filter_form(&mut self, form: &Form) -> Result<(), String> {
        let years = form
            .value("years")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().map_err(|_| format!("`{s}` is not a year")))
            .collect::<Result<Vec<i32>, String>>()?;
        let selected_library = form.value("library");
        let library_id = self
            .libraries
            .iter()
            .find(|l| l.name == selected_library)
            .map(|l| l.id.clone());
        let search = form.value("search").trim();
        self.query = LibraryQuery {
            library_id,
            search: (!search.is_empty()).then(|| search.to_owned()),
            genres: form
                .value("genres")
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
            years,
            status: Some(form.value("status").to_owned()).filter(|s| s != "Any"),
            kind: match form.value("kind") {
                "Series" => Some(ItemKind::Series),
                "Movies" => Some(ItemKind::Movie),
                _ => None,
            },
            unplayed_only: form.is_on("unplayed"),
            sort: match form.value("sort") {
                "Year" => LibrarySort::Year,
                "Date added" => LibrarySort::DateAdded,
                "Rating" => LibrarySort::Rating,
                _ => LibrarySort::Name,
            },
            start: 0,
            limit: None,
        };
        Ok(())
    }

    pub fn auth_label(&self, service: Service) -> String {
        match service {
            Service::Jellyfin => self
                .auth
                .jellyfin_user
                .clone()
                .unwrap_or_else(|| "offline".into()),
            Service::AniList => self
                .viewer
                .as_ref()
                .map(|v| v.name.clone())
                .or_else(|| self.auth.anilist_user.clone())
                .unwrap_or_else(|| "offline".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use musubi_core::domain::MediaTitle;
    use std::collections::BTreeMap;

    fn app() -> App {
        App::new(Keymap::with_overrides(&BTreeMap::new()), None)
    }

    fn press(app: &mut App, c: char) -> Vec<Effect> {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    fn press_code(app: &mut App, code: KeyCode) -> Vec<Effect> {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn entry(media: i64, title: &str, status: ListStatus, progress: u32) -> ListEntry {
        ListEntry {
            id: media * 10,
            media: Media {
                id: media,
                title: MediaTitle {
                    romaji: Some(title.into()),
                    ..MediaTitle::default()
                },
                episodes: Some(12),
                ..Media::default()
            },
            status,
            score_raw: 80,
            progress,
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

    fn list_app() -> App {
        let mut app = app();
        app.tab = Tab::AniList;
        app.on_msg(Msg::List(
            MediaKind::Anime,
            vec![
                entry(1, "Frieren", ListStatus::Current, 3),
                entry(2, "One Piece", ListStatus::Planning, 0),
                entry(3, "Bleach", ListStatus::Current, 12),
            ],
        ));
        app
    }

    fn saved(effects: Vec<Effect>) -> EntryPatch {
        match effects.into_iter().next() {
            Some(Effect::Save(patch)) => patch,
            _ => panic!("expected a save effect"),
        }
    }

    #[test]
    fn plus_and_minus_change_progress_within_bounds() {
        let mut app = list_app();
        assert_eq!(saved(press(&mut app, '+')).progress, Some(4));
        assert_eq!(saved(press(&mut app, '-')).progress, Some(2));
        press(&mut app, 'G');
        assert_eq!(
            saved(press(&mut app, '+')).progress,
            Some(12),
            "capped at the total"
        );
    }

    #[test]
    fn navigation_is_bounded() {
        let mut app = list_app();
        press(&mut app, 'k');
        assert_eq!(app.list_selected, 0);
        press(&mut app, 'j');
        press(&mut app, 'j');
        press(&mut app, 'j');
        assert_eq!(app.list_selected, 2);
        press(&mut app, 'g');
        assert_eq!(app.list_selected, 0);
    }

    #[test]
    fn incremental_filter_narrows_and_escape_restores() {
        let mut app = list_app();
        press(&mut app, '/');
        for c in "ble".chars() {
            press(&mut app, c);
        }
        assert_eq!(app.list_visible.len(), 1);
        assert_eq!(app.selected_entry().unwrap().media.id, 3);
        press_code(&mut app, KeyCode::Esc);
        assert_eq!(app.list_visible.len(), 3);
    }

    #[test]
    fn status_filter_cycles() {
        let mut app = list_app();
        press(&mut app, ']');
        assert_eq!(app.status_filter, Some(ListStatus::Current));
        assert_eq!(app.list_visible.len(), 2);
        press(&mut app, '[');
        assert_eq!(app.status_filter, None);
    }

    #[test]
    fn commands_edit_the_selected_entry() {
        let mut app = list_app();
        assert_eq!(
            saved(app.run_command("status completed")).status,
            Some(ListStatus::Completed)
        );
        app.viewer = Some(Viewer {
            score_format: ScoreFormat::Point100,
            ..Viewer::default()
        });
        assert_eq!(saved(app.run_command("score 85")).score_raw, Some(85));
        assert_eq!(
            saved(app.run_command("notes hello world")).notes.as_deref(),
            Some("hello world")
        );
        assert_eq!(saved(app.run_command("progress 7")).progress, Some(7));
        assert!(app.run_command("status nonsense").is_empty());
        assert!(!app.notice.as_ref().unwrap().ok);
    }

    #[test]
    fn delete_asks_first() {
        let mut app = list_app();
        assert!(press(&mut app, 'd').is_empty());
        assert!(matches!(app.modal, Some(Modal::Confirm { .. })));
        let effects = press(&mut app, 'y');
        assert!(matches!(effects.as_slice(), [Effect::Delete(e)] if e.media.id == 1));
        assert!(app.modal.is_none());
    }

    #[test]
    fn edit_form_sends_only_changed_fields() {
        let mut app = list_app();
        press(&mut app, 'e');
        let Some(Modal::Form(form, _)) = &mut app.modal else {
            panic!("form expected")
        };
        let notes = form.fields.iter().position(|f| f.key == "notes").unwrap();
        form.focus = notes;
        press(&mut app, 'x');
        let patch = saved(press_code(&mut app, KeyCode::Enter));
        assert_eq!(patch.notes.as_deref(), Some("x"));
        assert_eq!(
            patch,
            EntryPatch {
                notes: Some("x".into()),
                ..EntryPatch::new(1)
            }
        );
    }

    #[test]
    fn advanced_scores_are_sent_in_viewer_order() {
        let mut app = list_app();
        app.viewer = Some(Viewer {
            anime_advanced_scoring: vec!["Story".into(), "Art".into()],
            ..Viewer::default()
        });
        app.on_msg(Msg::List(MediaKind::Anime, {
            let mut e = entry(1, "Frieren", ListStatus::Current, 3);
            e.advanced_scores = vec![("Art".into(), 9.0), ("Story".into(), 7.0)];
            vec![e]
        }));
        press(&mut app, 'e');
        let Some(Modal::Form(form, _)) = &mut app.modal else {
            panic!("form expected")
        };
        let field = form
            .fields
            .iter_mut()
            .find(|f| f.key == "advanced")
            .unwrap();
        assert_eq!(field.value, "Story=7, Art=9");
        field.value = "Art=5".into();
        let patch = saved(press_code(&mut app, KeyCode::Enter));
        assert_eq!(patch.advanced_scores, Some(vec![7.0, 5.0]));
    }

    #[test]
    fn invalid_form_input_keeps_the_dialog_open() {
        let mut app = list_app();
        press(&mut app, 'e');
        let Some(Modal::Form(form, _)) = &mut app.modal else {
            panic!("form expected")
        };
        form.focus = form.fields.iter().position(|f| f.key == "started").unwrap();
        press(&mut app, 'x');
        assert!(press_code(&mut app, KeyCode::Enter).is_empty());
        let Some(Modal::Form(form, _)) = &app.modal else {
            panic!("form expected")
        };
        assert!(form.error.is_some());
    }

    #[test]
    fn sync_direction_cycles_and_apply_skips_rows() {
        let mut app = app();
        app.tab = Tab::Sync;
        let row = |id: &str| SyncRow {
            series_id: id.into(),
            series_title: id.into(),
            media_id: 1,
            media_title: id.into(),
            jellyfin_progress: 2,
            anilist_progress: 1,
            total: None,
            status: None,
            direction: SyncDirection::Push,
        };
        app.on_msg(Msg::Diff(vec![row("a"), row("b")]));
        press(&mut app, 'd');
        press(&mut app, 'd');
        assert_eq!(app.diff[0].direction, SyncDirection::Skip);
        press(&mut app, 'A');
        let effects = press(&mut app, 'y');
        match effects.as_slice() {
            [Effect::ApplySync(rows)] => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].series_id, "b");
            }
            _ => panic!("expected apply"),
        }
    }

    #[test]
    fn player_keys_control_playback_from_any_tab() {
        let mut app = list_app();
        assert!(matches!(
            press(&mut app, ' ').as_slice(),
            [Effect::Player(PlayerCommand::TogglePause)]
        ));
        assert!(matches!(
            press_code(&mut app, KeyCode::Right).as_slice(),
            [Effect::Player(PlayerCommand::SeekRelative(s))] if *s > 0.0
        ));
    }

    #[test]
    fn missing_login_opens_form_and_ctrl_c_always_quits() {
        let mut app = app();
        app.startup(&AuthStatus::default());
        assert!(matches!(
            app.modal,
            Some(Modal::Form(_, Purpose::LoginJellyfin))
        ));
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn failed_login_shows_error_in_open_form() {
        let mut app = app();
        app.open_jellyfin_login();
        app.on_msg(Msg::Error("nope".into()));
        let Some(Modal::Form(form, _)) = &app.modal else {
            panic!("form expected")
        };
        assert_eq!(form.error.as_deref(), Some("nope"));
    }

    fn logged_in(jellyfin: bool, anilist: bool) -> AuthStatus {
        AuthStatus {
            jellyfin_user: jellyfin.then(|| "user".to_owned()),
            jellyfin_url: jellyfin.then(|| "http://server:30013".to_owned()),
            anilist_logged_in: anilist,
            ..AuthStatus::default()
        }
    }

    fn form_purpose(app: &App) -> &Purpose {
        match &app.modal {
            Some(Modal::Form(_, purpose)) => purpose,
            _ => panic!("form expected"),
        }
    }

    #[test]
    fn setup_starts_at_the_first_missing_login() {
        let mut app = app();
        app.setup_info.client_id = "56388".into();
        app.startup(&logged_in(true, false));
        app.begin_setup();
        assert_eq!(app.setup, Some(SetupStep::AniList));
        let Some(Modal::Form(form, Purpose::LoginAniList)) = &app.modal else {
            panic!("AniList form expected")
        };
        assert_eq!(form.value("client_id"), "56388");
    }

    #[test]
    fn esc_walks_through_setup_and_finishes_without_changing_sync() {
        let mut app = app();
        app.startup(&AuthStatus::default());
        app.begin_setup();
        assert!(matches!(form_purpose(&app), Purpose::LoginJellyfin));
        press_code(&mut app, KeyCode::Esc);
        assert!(matches!(form_purpose(&app), Purpose::LoginAniList));
        press_code(&mut app, KeyCode::Esc);
        assert!(matches!(form_purpose(&app), Purpose::SetupSync));
        let effects = press_code(&mut app, KeyCode::Esc);
        assert!(matches!(effects.as_slice(), [Effect::FinishSetup(None)]));
        assert_eq!(app.setup, None);
        assert!(app.modal.is_none());
    }

    #[test]
    fn successful_logins_advance_setup_and_enter_saves_auto_sync() {
        let mut app = app();
        app.setup_info.auto_sync = true;
        app.startup(&AuthStatus::default());
        app.begin_setup();
        app.on_msg(Msg::LoggedIn("ok".into(), logged_in(true, false)));
        assert!(matches!(form_purpose(&app), Purpose::LoginAniList));
        app.on_msg(Msg::LoggedIn("ok".into(), logged_in(true, true)));
        assert!(matches!(form_purpose(&app), Purpose::SetupSync));
        press_code(&mut app, KeyCode::Char(' '));
        let effects = press_code(&mut app, KeyCode::Enter);
        assert!(matches!(
            effects.as_slice(),
            [Effect::FinishSetup(Some(false))]
        ));
        assert_eq!(app.setup, None);
    }

    #[test]
    fn anilist_login_during_setup_keeps_the_form_for_errors() {
        let mut app = app();
        app.setup_info.client_id = "56388".into();
        app.startup(&logged_in(true, false));
        app.begin_setup();
        let effects = press_code(&mut app, KeyCode::Enter);
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoginAniListBrowser(id)] if id == "56388"
        ));
        app.on_msg(Msg::Error("port in use".into()));
        let Some(Modal::Form(form, Purpose::LoginAniList)) = &app.modal else {
            panic!("form expected")
        };
        assert_eq!(form.error.as_deref(), Some("port in use"));
    }

    #[test]
    fn anilist_login_during_setup_does_not_reopen_jellyfin() {
        let mut app = app();
        app.startup(&AuthStatus::default());
        app.begin_setup();
        press_code(&mut app, KeyCode::Esc);
        app.on_msg(Msg::LoggedIn("ok".into(), logged_in(false, true)));
        assert!(matches!(form_purpose(&app), Purpose::SetupSync));
    }

    #[test]
    fn setup_command_opens_the_wizard() {
        let mut app = app();
        app.run_command("setup");
        assert_eq!(app.setup, Some(SetupStep::Jellyfin));
    }
}
