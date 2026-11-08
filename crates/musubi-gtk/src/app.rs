//! Hauptfenster: verbindet Seiten, Kern und Tastenkürzel.

use std::collections::HashMap;

use musubi_core::domain::{
    EntryPatch, Library, LibraryFilters, LibraryItem, ListEntry, ListStatus, Mapping, Media,
    MediaKind,
};
use musubi_core::{
    Core, CoreEvent, EpisodeRow, LibraryRow, OutboxItem, PlayerCommand, ReviewItem, Service,
    SyncLogEntry, SyncReport, SyncRow,
};
use relm4::adw::prelude::*;
use relm4::{Component, ComponentParts, ComponentSender, adw, gtk};
use secrecy::SecretString;
use tokio::sync::broadcast;

use crate::library::LibraryPage;
use crate::list::{self, AddDialog, ListPage};
use crate::mapping::{self, MappingPage};
use crate::player::PlayerBar;
use crate::settings::SettingsPage;
use crate::setup::SetupDialog;
use crate::sync::SyncPage;
use crate::util::{self, MarginExt, task};

const PAGES: [(&str, &str, &str); 5] = [
    ("library", "Library", "video-x-generic-symbolic"),
    ("anilist", "AniList", "view-list-symbolic"),
    ("mapping", "Mapping", "insert-link-symbolic"),
    ("sync", "Sync", "view-refresh-symbolic"),
    ("settings", "Settings", "emblem-system-symbolic"),
];
const SETTINGS_PAGE: usize = 4;

#[derive(Debug)]
#[allow(clippy::large_enum_variant, reason = "UI messages are short-lived")]
pub enum Msg {
    Navigate(usize),
    FocusSearch,
    Refresh,
    ShowShortcuts,
    Core(CoreEvent),
    // Bibliothek
    ReloadLibrary,
    ActivateItem(u32),
    Play(LibraryItem, Option<String>),
    SetPlayed(String, bool),
    OpenMapDialog(LibraryItem),
    MapTo(Box<LibraryItem>, i64, bool),
    EditEntryFor(LibraryItem),
    // AniList
    KindChanged,
    ListFilterChanged,
    ActivateEntry(u32),
    SaveEntry(EntryPatch),
    DeleteEntry(ListEntry),
    OpenAddDialog,
    SearchAdd(String),
    AddMedia(Box<Media>, ListStatus),
    RefreshList,
    OpenListsDialog,
    CreateList(String),
    DeleteList(String),
    RenameList(String, String),
    // Zuordnung
    MatchAll,
    Unmap(String),
    OpenMappingEditor(Box<Mapping>),
    SetMappings(String, Vec<Mapping>),
    // Sync
    ReloadSync,
    ApplySync(Vec<SyncRow>),
    FlushOutbox,
    // Anmeldung
    LoginJellyfin(String, String, SecretString),
    LoginJellyfinKey(String, String, SecretString),
    LoginAniListBrowser(String),
    LoginAniListToken(String),
    LogoutJellyfin,
    LogoutAniList,
    SaveSettings,
    // Einrichtung
    OpenSetup,
    SetupBack,
    SetupNext,
    SetupClosed,
    Player(PlayerCommand),
}

#[derive(Debug)]
pub enum Cmd {
    /// Laufnummer der Abfrage und Ergebnis.
    Library(u64, Result<Vec<LibraryRow>, String>),
    Choices(Result<(LibraryFilters, Vec<Library>), String>),
    Episodes(String, Result<Vec<EpisodeRow>, String>),
    Searched(Result<Vec<Media>, String>),
    MappingData(Result<(Vec<ReviewItem>, Vec<Mapping>), String>),
    Diff(Result<Vec<SyncRow>, String>),
    SyncStatus(Result<(Vec<OutboxItem>, Vec<SyncLogEntry>), String>),
    SyncApplied(Result<SyncReport, String>),
    ListLoaded(Result<(), String>),
    LoggedIn(Result<String, String>),
    /// Abgeschlossene Aktion ohne Rückgabewert, Fehler werden angezeigt.
    Done(Result<(), String>),
}

pub struct App {
    core: Core,
    toasts: adw::ToastOverlay,
    split: adw::NavigationSplitView,
    content_page: adw::NavigationPage,
    stack: gtk::Stack,
    sidebar: gtk::ListBox,
    library: LibraryPage,
    list: ListPage,
    mapping: MappingPage,
    sync: SyncPage,
    settings: SettingsPage,
    player: PlayerBar,
    add_dialog: Option<AddDialog>,
    setup: Option<SetupDialog>,
    items: HashMap<String, LibraryItem>,
    reviews: Vec<ReviewItem>,
    mappings: Vec<Mapping>,
    /// Laufende Bibliotheksabfrage: nur die neueste zählt.
    library_generation: u64,
    library_applied: u64,
}

fn sidebar_row(title: &str, icon: &str) -> gtk::ListBoxRow {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_all(6);
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(title)));
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&content));
    row
}

fn shortcut(
    controller: &gtk::ShortcutController,
    trigger: &str,
    sender: &ComponentSender<App>,
    make: impl Fn() -> Msg + 'static,
) {
    let input = sender.input_sender().clone();
    let action = gtk::CallbackAction::new(move |_, _| {
        input.emit(make());
        gtk::glib::Propagation::Stop
    });
    controller.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string(trigger),
        Some(action),
    ));
}

fn install_shortcuts(window: &adw::ApplicationWindow, sender: &ComponentSender<App>) {
    let controller = gtk::ShortcutController::new();
    controller.set_scope(gtk::ShortcutScope::Global);
    for page in 0..PAGES.len() {
        shortcut(
            &controller,
            &format!("<Control>{}", page + 1),
            sender,
            move || Msg::Navigate(page),
        );
    }
    shortcut(&controller, "<Control>f", sender, || Msg::FocusSearch);
    shortcut(&controller, "<Control>r", sender, || Msg::Refresh);
    shortcut(&controller, "F1", sender, || Msg::ShowShortcuts);
    let player: [(&str, PlayerCommand); 7] = [
        ("<Control>p", PlayerCommand::TogglePause),
        ("<Alt>Left", PlayerCommand::SeekRelative(-10.0)),
        ("<Alt>Right", PlayerCommand::SeekRelative(10.0)),
        ("<Control>period", PlayerCommand::Next),
        ("<Control>comma", PlayerCommand::Previous),
        ("<Control>j", PlayerCommand::CycleSubtitle),
        ("<Control>k", PlayerCommand::CycleAudio),
    ];
    for (trigger, command) in player {
        shortcut(&controller, trigger, sender, move || Msg::Player(command));
    }
    shortcut(&controller, "<Control><Shift>s", sender, || {
        Msg::Player(PlayerCommand::Stop)
    });
    window.add_controller(controller);
}

const SHORTCUTS: [(&str, &str); 14] = [
    ("Ctrl+1 to 5", "Switch page"),
    ("Ctrl+F", "Search"),
    ("Ctrl+R", "Refresh"),
    ("Ctrl+P", "Play or pause"),
    ("Alt+Left / Right", "Seek 10 seconds"),
    ("Ctrl+. / Ctrl+,", "Next or previous episode"),
    ("Ctrl+J", "Cycle subtitles"),
    ("Ctrl+K", "Cycle audio"),
    ("Ctrl+Shift+S", "Stop playback"),
    ("F1", "Show this list"),
    ("Space", "Play or pause (in the mpv window)"),
    ("Left / Right", "Seek (in the mpv window)"),
    ("< / >", "Previous or next episode (in the mpv window)"),
    ("v / #", "Toggle subtitles, cycle audio (in the mpv window)"),
];

fn show_shortcuts() {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_all(18);
    let group = adw::PreferencesGroup::new();
    for (keys, text) in SHORTCUTS {
        let row = adw::ActionRow::builder().title(text).build();
        let label = gtk::Label::new(Some(keys));
        label.add_css_class("dim-label");
        row.add_suffix(&label);
        group.add(&row);
    }
    content.append(&group);
    util::present(&util::dialog("Keyboard shortcuts", 460, 560, &content));
}

impl App {
    fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    fn report(&self, result: Result<(), String>) {
        if let Err(error) = result {
            self.toast(&error);
        }
    }

    fn load_library(&mut self, sender: &ComponentSender<App>) {
        if self.core.auth_status().jellyfin_user.is_none() {
            return;
        }
        self.library_generation += 1;
        let (core, query) = (self.core.clone(), self.library.query());
        let generation = self.library_generation;
        task(
            sender,
            async move { core.library(&query).await },
            move |r| Cmd::Library(generation, r),
        );
    }

    fn load_choices(&self, sender: &ComponentSender<App>) {
        if self.core.auth_status().jellyfin_user.is_none() {
            return;
        }
        let core = self.core.clone();
        task(
            sender,
            async move { Ok((core.filters(None).await?, core.libraries().await?)) },
            Cmd::Choices,
        );
    }

    fn load_mapping(&self, sender: &ComponentSender<App>) {
        let core = self.core.clone();
        task(
            sender,
            async move { Ok((core.reviews().await?, core.all_mappings().await?)) },
            Cmd::MappingData,
        );
    }

    fn load_sync(&self, sender: &ComponentSender<App>) {
        let core = self.core.clone();
        if core.auth_status().jellyfin_user.is_some() && core.auth_status().anilist_logged_in {
            let c = core.clone();
            task(sender, async move { c.sync_diff().await }, Cmd::Diff);
        }
        self.load_sync_status(sender);
    }

    fn load_sync_status(&self, sender: &ComponentSender<App>) {
        let core = self.core.clone();
        task(
            sender,
            async move { Ok((core.pending_writes().await?, core.sync_log(50).await?)) },
            Cmd::SyncStatus,
        );
    }

    fn load_episodes(&self, id: String, sender: &ComponentSender<App>) {
        let core = self.core.clone();
        let key = id.clone();
        task(sender, async move { core.episodes(&id).await }, move |r| {
            Cmd::Episodes(key, r)
        });
    }

    fn refresh_list(&self, kind: MediaKind, sender: &ComponentSender<App>) {
        if !self.core.auth_status().anilist_logged_in {
            return;
        }
        let core = self.core.clone();
        task(
            sender,
            async move {
                core.refresh_viewer().await?;
                core.refresh_list(kind).await?;
                Ok(())
            },
            Cmd::ListLoaded,
        );
    }

    fn show_list(&mut self) {
        let format = self
            .core
            .viewer()
            .map(|v| v.score_format)
            .unwrap_or_default();
        let kind = self.list.kind();
        self.list.set_entries(self.core.list(kind), format);
    }

    fn update_detail_summary(&self) {
        let Some(item) = self.library.current_detail() else {
            return;
        };
        let first = self.mappings.iter().find(|m| m.jellyfin_id == item.id);
        let entry = first.and_then(|m| self.core.entry_for(m.media_id));
        self.library
            .set_anilist_summary(entry.as_ref(), first.is_some());
    }

    fn after_login(&mut self, sender: &ComponentSender<App>) {
        self.settings.set_auth(&self.core.auth_status());
        self.load_library(sender);
        self.load_choices(sender);
        self.refresh_list(MediaKind::Anime, sender);
    }

    fn done(
        &self,
        sender: &ComponentSender<App>,
        future: impl std::future::Future<Output = musubi_core::Result<()>> + Send + 'static,
    ) {
        task(sender, future, Cmd::Done);
    }

    fn on_core_event(&mut self, event: CoreEvent, sender: &ComponentSender<App>) {
        match event {
            CoreEvent::Playback(state) => self.player.update(&state),
            CoreEvent::PlaybackEnded => self.player.hide(),
            CoreEvent::LibraryChanged => {
                self.load_library(sender);
                if let Some(item) = self.library.current_detail() {
                    self.load_episodes(item.id, sender);
                }
            }
            CoreEvent::ListChanged(kind) => {
                if kind == self.list.kind() {
                    self.show_list();
                }
                self.update_detail_summary();
            }
            CoreEvent::MappingsChanged => self.load_mapping(sender),
            CoreEvent::MatchProgress { done, total } => self.mapping.set_progress(done, total),
            CoreEvent::Notice { message, .. } => {
                self.toast(&message);
                self.load_sync_status(sender);
            }
            CoreEvent::RateLimited(Some(wait)) => {
                self.toast(&format!(
                    "AniList rate limit reached, waiting {} s",
                    wait.as_secs()
                ));
            }
            CoreEvent::RateLimited(None) => {}
            CoreEvent::AuthRequired(Service::AniList) => {
                self.toast("AniList session expired, please log in again");
            }
            CoreEvent::AuthRequired(Service::Jellyfin) => {
                self.toast("Jellyfin session expired, please log in again");
            }
            CoreEvent::TokenExpiring { days } => {
                self.toast(&format!(
                    "AniList token expires in {days} days, please log in again"
                ));
            }
        }
    }
}

impl Component for App {
    type Init = Core;
    type Input = Msg;
    type Output = ();
    type CommandOutput = Cmd;
    type Root = adw::ApplicationWindow;
    type Widgets = ();

    fn init_root() -> adw::ApplicationWindow {
        adw::ApplicationWindow::builder()
            .title("Musubi")
            .default_width(1200)
            .default_height(800)
            .build()
    }

    fn init(
        core: Core,
        root: adw::ApplicationWindow,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let library = LibraryPage::new(&sender);
        let list = ListPage::new(&sender);
        let mapping = MappingPage::new(&sender);
        let sync = SyncPage::new(&sender);
        let settings = SettingsPage::new(&sender);
        let player = PlayerBar::new(&sender);

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.add_named(&library.root, Some(PAGES[0].0));
        stack.add_named(&list.root, Some(PAGES[1].0));
        stack.add_named(&mapping.root, Some(PAGES[2].0));
        stack.add_named(&sync.root, Some(PAGES[3].0));
        stack.add_named(&settings.root, Some(PAGES[4].0));

        let sidebar = gtk::ListBox::new();
        sidebar.add_css_class("navigation-sidebar");
        for (_, title, icon) in PAGES {
            sidebar.append(&sidebar_row(title, icon));
        }
        let input = sender.input_sender().clone();
        sidebar
            .connect_row_activated(move |_, row| input.emit(Msg::Navigate(row.index() as usize)));

        let side_toolbar = adw::ToolbarView::new();
        let side_header = adw::HeaderBar::new();
        side_header.set_title_widget(Some(&adw::WindowTitle::new("Musubi", "")));
        side_toolbar.add_top_bar(&side_header);
        side_toolbar.set_content(Some(&sidebar));

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        stack.set_vexpand(true);
        content_box.append(&stack);
        content_box.append(&player.root);

        let split = adw::NavigationSplitView::new();
        split.set_sidebar(Some(&adw::NavigationPage::new(&side_toolbar, "Musubi")));
        let content_page = adw::NavigationPage::new(&content_box, PAGES[0].1);
        split.set_content(Some(&content_page));
        split.set_max_sidebar_width(220.0);

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&split));
        root.set_content(Some(&toasts));

        // Schmale Fenster falten die Seitenleiste ein.
        let breakpoint = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 640sp").expect("static condition is valid"),
        );
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        root.add_breakpoint(breakpoint);

        install_shortcuts(&root, &sender);

        let mut events = core.subscribe();
        let input = sender.input_sender().clone();
        relm4::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => input.emit(Msg::Core(event)),
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });

        let auth = core.auth_status();
        settings.load(&core.config());
        settings.set_auth(&auth);

        let mut model = App {
            core,
            toasts,
            split,
            content_page,
            stack,
            sidebar,
            library,
            list,
            mapping,
            sync,
            settings,
            player,
            add_dialog: None,
            setup: None,
            items: HashMap::new(),
            reviews: vec![],
            mappings: vec![],
            library_generation: 0,
            library_applied: 0,
        };

        if auth.jellyfin_user.is_none() && !auth.anilist_logged_in {
            sender.input(Msg::Navigate(SETTINGS_PAGE));
        } else {
            sender.input(Msg::Navigate(0));
        }
        if model.core.setup_needed() {
            sender.input(Msg::OpenSetup);
        }
        model.show_list();
        let cached = model.core.clone();
        task(&sender, async move { cached.cached_library().await }, |r| {
            Cmd::Library(0, r)
        });
        model.load_library(&sender);
        model.load_choices(&sender);
        model.load_mapping(&sender);
        model.load_sync_status(&sender);
        model.refresh_list(MediaKind::Anime, &sender);
        if let Err(error) = model.core.warn_if_expiring() {
            tracing::warn!(%error, "could not check the token expiry");
        }
        ComponentParts { model, widgets: () }
    }

    fn update(
        &mut self,
        message: Msg,
        sender: ComponentSender<Self>,
        root: &adw::ApplicationWindow,
    ) {
        match message {
            Msg::Navigate(page) => {
                let Some((name, title, _)) = PAGES.get(page) else {
                    return;
                };
                self.stack.set_visible_child_name(name);
                self.content_page.set_title(title);
                self.sidebar
                    .select_row(self.sidebar.row_at_index(page as i32).as_ref());
                self.split.set_show_content(true);
                match page {
                    2 => self.load_mapping(&sender),
                    3 => self.load_sync(&sender),
                    _ => {}
                }
            }
            Msg::FocusSearch => match self.stack.visible_child_name().as_deref() {
                Some("anilist") => {
                    self.list.search.grab_focus();
                }
                _ => {
                    self.stack.set_visible_child_name("library");
                    self.library.search.grab_focus();
                }
            },
            Msg::Refresh => match self.stack.visible_child_name().as_deref() {
                Some("anilist") => self.refresh_list(self.list.kind(), &sender),
                Some("mapping") => self.load_mapping(&sender),
                Some("sync") => self.load_sync(&sender),
                _ => self.load_library(&sender),
            },
            Msg::ShowShortcuts => show_shortcuts(),
            Msg::Core(event) => self.on_core_event(event, &sender),

            Msg::ReloadLibrary => self.load_library(&sender),
            Msg::ActivateItem(position) => {
                let Some(item) = self.library.item_at(position) else {
                    return;
                };
                if item.kind == musubi_core::domain::ItemKind::Movie {
                    sender.input(Msg::Play(item, None));
                    return;
                }
                let id = item.id.clone();
                self.library.open_detail(item, &sender);
                self.update_detail_summary();
                self.load_episodes(id, &sender);
            }
            Msg::Play(item, episode) => {
                let core = self.core.clone();
                self.done(&sender, async move {
                    core.play(&item, episode.as_deref()).await
                });
            }
            Msg::SetPlayed(id, played) => {
                let core = self.core.clone();
                self.done(&sender, async move { core.set_played(&id, played).await });
            }
            Msg::OpenMapDialog(item) => mapping::open_map_dialog(item, &sender),
            Msg::MapTo(item, media, follow) => {
                let core = self.core.clone();
                self.done(&sender, async move {
                    core.map_manually(&item, media, follow).await
                });
            }
            Msg::EditEntryFor(item) => {
                let first = self.mappings.iter().find(|m| m.jellyfin_id == item.id);
                match first.and_then(|m| self.core.entry_for(m.media_id)) {
                    Some(entry) => list::open_entry_dialog(entry, self.core.viewer(), &sender),
                    None if first.is_none() => mapping::open_map_dialog(item, &sender),
                    None => self.toast("This title is mapped but not on your list yet."),
                }
            }

            Msg::KindChanged => {
                self.show_list();
                let kind = self.list.kind();
                if self.core.list(kind).is_empty() {
                    self.refresh_list(kind, &sender);
                }
            }
            Msg::ListFilterChanged => self.show_list(),
            Msg::ActivateEntry(position) => {
                if let Some(entry) = self.list.entry_at(position) {
                    list::open_entry_dialog(entry, self.core.viewer(), &sender);
                }
            }
            Msg::SaveEntry(patch) => {
                let core = self.core.clone();
                self.done(&sender, async move { core.save_entry(patch).await });
            }
            Msg::DeleteEntry(entry) => {
                let core = self.core.clone();
                self.done(&sender, async move { core.delete_entry(&entry).await });
            }
            Msg::OpenAddDialog => {
                self.add_dialog = Some(AddDialog::open(self.list.kind(), &sender));
            }
            Msg::SearchAdd(text) => {
                let Some(dialog) = &self.add_dialog else {
                    return;
                };
                let (core, kind) = (self.core.clone(), dialog.kind());
                task(
                    &sender,
                    async move { core.search_anilist(&text, kind).await },
                    Cmd::Searched,
                );
            }
            Msg::AddMedia(media, status) => {
                let core = self.core.clone();
                self.done(
                    &sender,
                    async move { core.add_to_list(*media, status).await },
                );
            }
            Msg::RefreshList => self.refresh_list(self.list.kind(), &sender),
            Msg::OpenListsDialog => {
                let kind = self.list.kind();
                let names = self
                    .core
                    .viewer()
                    .map(|v| v.custom_lists(kind).to_vec())
                    .unwrap_or_default();
                list::open_lists_dialog(kind, names, &sender);
            }
            Msg::CreateList(name) => {
                let (core, kind) = (self.core.clone(), self.list.kind());
                self.done(&sender, async move {
                    core.create_custom_list(kind, &name).await
                });
            }
            Msg::DeleteList(name) => {
                let (core, kind) = (self.core.clone(), self.list.kind());
                self.done(&sender, async move {
                    core.delete_custom_list(kind, &name).await
                });
            }
            Msg::RenameList(old, new) => {
                let (core, kind) = (self.core.clone(), self.list.kind());
                self.done(&sender, async move {
                    core.rename_custom_list(kind, &old, &new).await
                });
            }

            Msg::MatchAll => {
                let core = self.core.clone();
                self.done(&sender, async move { core.match_all().await });
            }
            Msg::Unmap(id) => {
                let core = self.core.clone();
                self.done(&sender, async move { core.unmap(&id).await });
            }
            Msg::OpenMappingEditor(target) => {
                let siblings: Vec<Mapping> = self
                    .mappings
                    .iter()
                    .filter(|m| m.jellyfin_id == target.jellyfin_id)
                    .cloned()
                    .collect();
                mapping::open_mapping_editor(*target, siblings, &sender);
            }
            Msg::SetMappings(id, mappings) => {
                let core = self.core.clone();
                self.done(
                    &sender,
                    async move { core.set_mappings(&id, mappings).await },
                );
            }

            Msg::ReloadSync => self.load_sync(&sender),
            Msg::ApplySync(rows) => {
                let core = self.core.clone();
                task(
                    &sender,
                    async move { core.apply_sync(&rows).await },
                    Cmd::SyncApplied,
                );
            }
            Msg::FlushOutbox => {
                let core = self.core.clone();
                self.done(&sender, async move { core.flush_outbox().await });
            }

            Msg::LoginJellyfin(url, user, password) => {
                let core = self.core.clone();
                task(
                    &sender,
                    async move { core.login_jellyfin(&url, &user, &password).await },
                    Cmd::LoggedIn,
                );
            }
            Msg::LoginJellyfinKey(url, user, key) => {
                let core = self.core.clone();
                task(
                    &sender,
                    async move { core.login_jellyfin_api_key(&url, key, &user).await },
                    Cmd::LoggedIn,
                );
            }
            Msg::LoginAniListBrowser(client_id) => {
                let core = self.core.clone();
                self.toast("Waiting for the browser…");
                task(
                    &sender,
                    async move {
                        core.update_config(|c| c.anilist.client_id = Some(client_id))?;
                        Ok(core.login_anilist_loopback().await?.name)
                    },
                    Cmd::LoggedIn,
                );
            }
            Msg::LoginAniListToken(token) => {
                let core = self.core.clone();
                task(
                    &sender,
                    async move { Ok(core.login_anilist_pin(&token).await?.name) },
                    Cmd::LoggedIn,
                );
            }
            Msg::LogoutJellyfin => {
                let core = self.core.clone();
                task(
                    &sender,
                    async move {
                        core.logout_jellyfin()
                            .await
                            .map(|_| "logged out".to_owned())
                    },
                    Cmd::LoggedIn,
                );
            }
            Msg::LogoutAniList => {
                let core = self.core.clone();
                task(
                    &sender,
                    async move { core.logout_anilist().await.map(|_| "logged out".to_owned()) },
                    Cmd::LoggedIn,
                );
            }
            Msg::SaveSettings => {
                let result = self.core.update_config(|c| self.settings.apply(c));
                match result {
                    Ok(()) => self.toast("Settings saved"),
                    Err(error) => self.toast(&error.to_string()),
                }
            }
            Msg::OpenSetup => {
                if self.setup.is_none() {
                    let auth = self.core.auth_status();
                    self.setup = Some(SetupDialog::open(&self.core.config(), &auth, &sender, root));
                }
            }
            Msg::SetupBack => {
                if let Some(setup) = &self.setup {
                    setup.go_back();
                }
            }
            Msg::SetupNext => {
                if let Some(setup) = &self.setup
                    && setup.advance()
                {
                    let auto_sync = setup.auto_sync();
                    match self.core.update_config(|c| c.sync.auto_sync = auto_sync) {
                        Ok(()) => setup.close(),
                        Err(error) => setup.show_error(&error.to_string()),
                    }
                }
            }
            Msg::SetupClosed => {
                self.setup = None;
                if let Err(error) = self.core.update_config(|c| c.ui.setup_completed = true) {
                    self.toast(&error.to_string());
                }
                self.settings.load(&self.core.config());
            }
            Msg::Player(command) => {
                let core = self.core.clone();
                self.done(&sender, async move { core.player_command(command).await });
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Cmd,
        sender: ComponentSender<Self>,
        _root: &adw::ApplicationWindow,
    ) {
        match message {
            Cmd::Library(generation, _) if generation < self.library_applied => {}
            Cmd::Library(generation, Ok(rows)) => {
                self.library_applied = generation;
                self.items = rows
                    .iter()
                    .map(|r| (r.item.id.clone(), r.item.clone()))
                    .collect();
                self.library.set_rows(rows, &self.core);
                self.load_mapping(&sender);
            }
            Cmd::Library(_, Err(error)) => self.toast(&error),
            Cmd::Choices(Ok((filters, libraries))) => self.library.set_choices(filters, libraries),
            Cmd::Choices(Err(error)) => self.toast(&error),
            Cmd::Episodes(id, Ok(episodes)) => self.library.set_episodes(&id, episodes, &sender),
            Cmd::Episodes(_, Err(error)) => self.toast(&error),
            Cmd::Searched(Ok(media)) => {
                if let Some(dialog) = &self.add_dialog {
                    dialog.show_results(media, &sender);
                }
            }
            Cmd::Searched(Err(error)) => self.toast(&error),
            Cmd::MappingData(Ok((reviews, mappings))) => {
                self.reviews = reviews.clone();
                self.mappings = mappings.clone();
                self.mapping
                    .set_data(reviews, mappings, &self.items, &sender);
                self.update_detail_summary();
            }
            Cmd::MappingData(Err(error)) => self.toast(&error),
            Cmd::Diff(Ok(rows)) => self.sync.set_diff(rows),
            Cmd::Diff(Err(error)) => self.toast(&error),
            Cmd::SyncStatus(Ok((pending, log))) => self.sync.set_status(pending, log),
            Cmd::SyncStatus(Err(error)) => self.toast(&error),
            Cmd::SyncApplied(Ok(report)) => {
                self.toast(&format!(
                    "Sync: {} pushed, {} episodes marked, {} conflicts, {} failed",
                    report.pushed, report.pulled, report.conflicts, report.failed
                ));
                self.load_sync(&sender);
            }
            Cmd::SyncApplied(Err(error)) => self.toast(&error),
            Cmd::ListLoaded(result) => {
                self.report(result);
                self.show_list();
                self.load_library(&sender);
            }
            Cmd::LoggedIn(Ok(name)) => {
                self.toast(&format!("Done: {name}"));
                self.after_login(&sender);
                if let Some(setup) = &self.setup {
                    setup.logged_in(&self.core.auth_status());
                }
            }
            Cmd::LoggedIn(Err(error)) => match &self.setup {
                Some(setup) => setup.show_error(&error),
                None => self.toast(&error),
            },
            Cmd::Done(result) => self.report(result),
        }
    }
}
