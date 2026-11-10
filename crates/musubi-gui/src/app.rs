//! Hauptzustand: verbindet Seiten, Kern, Tastenkürzel und Dialoge.

use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, key::Named};
use iced::widget::image::Handle;
use iced::widget::{Space, button, column, container, row, stack, svg, text};
use iced::{Center, Color, Element, Fill, Subscription, Task, Theme};
use musubi_core::domain::MediaKind;
use musubi_core::{AuthStatus, Config, Core, CoreEvent, PlayerCommand, Service};
use tokio::sync::broadcast;

use crate::icons::Icon;
use crate::images::{ImageCache, ImageKey};
use crate::ui;
use crate::{anilist, home, library, mapping, profile, settings, setup, sync, theme, video};

const TOAST_SECONDS: u64 = 5;
/// Version dieses Builds.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Git-Stand dieses Builds, gesetzt von `build.rs`.
pub const COMMIT: &str = env!("MUSUBI_COMMIT");
/// Breite der Seitenleiste.
const RAIL_WIDTH: f32 = 216.0;
const SEEK_STEP: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Library,
    AniList,
    Mapping,
    Sync,
    Settings,
    Profile,
}

/// Reihenfolge der Tastenkürzel Ctrl+1 bis Ctrl+7.
const NAVIGATION: [(Page, &str); 7] = [
    (Page::Home, "Home"),
    (Page::Library, "Library"),
    (Page::AniList, "AniList"),
    (Page::Mapping, "Mapping"),
    (Page::Sync, "Sync"),
    (Page::Settings, "Settings"),
    (Page::Profile, "Profile"),
];

/// Die Seitenleiste in drei Gruppen, jede mit ihrer Kordelfarbe.
const RAIL: [(&str, Color, &[(Page, Icon)]); 3] = [
    (
        "Watch",
        theme::WATCH,
        &[(Page::Home, Icon::Home), (Page::Library, Icon::Library)],
    ),
    (
        "Track",
        theme::TRACK,
        &[(Page::AniList, Icon::List), (Page::Profile, Icon::Profile)],
    ),
    (
        "Connect",
        theme::PAPER,
        &[(Page::Mapping, Icon::Link), (Page::Sync, Icon::Sync)],
    ),
];

const SHORTCUTS: [(&str, &str); 11] = [
    ("Ctrl+1 to 7", "Switch page"),
    ("Ctrl+R", "Refresh"),
    (
        "Space, Left / Right, F, M",
        "In the player: pause, seek, fullscreen, mute",
    ),
    ("Ctrl+P", "Play or pause"),
    ("Alt+Left / Right", "Seek 10 seconds"),
    ("Ctrl+. / Ctrl+,", "Next or previous episode"),
    ("Ctrl+J", "Cycle subtitles"),
    ("Ctrl+K", "Cycle audio"),
    ("Ctrl+Shift+S", "Stop playback"),
    ("F1", "Show this list"),
    ("Esc", "Close dialog or go back"),
];

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant, reason = "UI messages are short-lived")]
pub enum Message {
    Navigate(Page),
    Refresh,
    Tick,
    Escape,
    ToggleHelp,
    Core(CoreEvent),
    ImageLoaded(ImageKey, Result<Handle, String>),
    Notify(Result<(), String>),
    Home(home::Msg),
    Library(library::Msg),
    AniList(anilist::Msg),
    Mapping(mapping::Msg),
    Sync(sync::Msg),
    Settings(settings::Msg),
    Profile(profile::Msg),
    Setup(setup::Msg),
    Video(video::Msg),
    Player(PlayerCommand),
}

pub struct App {
    pub core: Core,
    pub page: Page,
    pub auth: AuthStatus,
    pub config: Config,
    pub images: ImageCache,
    pub home: home::State,
    pub library: library::State,
    pub anilist: anilist::State,
    pub mapping: mapping::State,
    pub sync: sync::State,
    pub settings: settings::State,
    pub profile: profile::State,
    pub setup: Option<setup::State>,
    pub video: video::State,
    video_link: Option<video::VideoLink>,
    pub now: i64,
    /// Zählt Seitenwechsel. Asynchrone Ergebnisse von vorher dürfen die Ansicht nicht mehr
    /// umstellen, und die Seite bekommt bei jedem Wechsel frischen Widget-Zustand.
    pub nav_generation: u64,
    /// Zählt jeden Wechsel der sichtbaren Ansicht (Seite, Detailseite auf oder zu).
    view_epoch: u64,
    toast: Option<(String, bool, Instant)>,
    help: bool,
}

/// Macht den Kern zum Schlüssel einer Subscription. Es gibt immer genau einen.
#[derive(Clone)]
struct CoreHandle(Core);

impl Hash for CoreHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "musubi-core".hash(state);
    }
}

fn core_events(handle: &CoreHandle) -> impl iced::futures::Stream<Item = Message> + use<> {
    let mut events = handle.0.subscribe();
    iced::stream::channel(64, async move |mut output| {
        use iced::futures::SinkExt;
        loop {
            match events.recv().await {
                Ok(event) => {
                    if output.send(Message::Core(event)).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    })
}

fn on_key(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    let player = |command| Some(Message::Player(command));
    if modifiers.is_empty() {
        let video_key = match key.as_ref() {
            Key::Named(Named::Space) => Some(video::Key::Pause),
            Key::Named(Named::ArrowLeft) => Some(video::Key::Back),
            Key::Named(Named::ArrowRight) => Some(video::Key::Forward),
            Key::Character("f") => Some(video::Key::Fullscreen),
            Key::Character("m") => Some(video::Key::Mute),
            _ => None,
        };
        if let Some(video_key) = video_key {
            return Some(Message::Video(video::Msg::Key(video_key)));
        }
    }
    match key.as_ref() {
        Key::Named(Named::Escape) => Some(Message::Escape),
        Key::Named(Named::F1) => Some(Message::ToggleHelp),
        Key::Named(Named::ArrowLeft) if modifiers.alt() => {
            player(PlayerCommand::SeekRelative(-SEEK_STEP))
        }
        Key::Named(Named::ArrowRight) if modifiers.alt() => {
            player(PlayerCommand::SeekRelative(SEEK_STEP))
        }
        Key::Character("s") | Key::Character("S") if modifiers.control() && modifiers.shift() => {
            player(PlayerCommand::Stop)
        }
        Key::Character(c) if modifiers.control() => match c {
            "r" => Some(Message::Refresh),
            "p" => player(PlayerCommand::TogglePause),
            "." => player(PlayerCommand::Next),
            "," => player(PlayerCommand::Previous),
            "j" => player(PlayerCommand::CycleSubtitle),
            "k" => player(PlayerCommand::CycleAudio),
            number => number
                .parse::<usize>()
                .ok()
                .and_then(|n| NAVIGATION.get(n.checked_sub(1)?))
                .map(|(page, _)| Message::Navigate(*page)),
        },
        _ => None,
    }
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl App {
    pub fn boot(core: Core, video: Option<video::VideoLink>) -> (Self, Task<Message>) {
        let auth = core.auth_status();
        let config = core.config();
        let page = if auth.jellyfin_user.is_some() {
            Page::Home
        } else if auth.anilist_logged_in {
            Page::AniList
        } else {
            Page::Settings
        };
        let setup = core
            .setup_needed()
            .then(|| setup::State::new(&config, &auth));
        let mut app = Self {
            page,
            setup,
            video: video::State::default(),
            video_link: video.clone(),
            images: ImageCache::default(),
            home: home::State::default(),
            library: library::State::default(),
            anilist: anilist::State::default(),
            mapping: mapping::State::default(),
            sync: sync::State::default(),
            settings: settings::State::new(&config),
            profile: profile::State::default(),
            now: unix_now(),
            nav_generation: 0,
            view_epoch: 0,
            toast: None,
            help: false,
            auth,
            config,
            core,
        };
        if video.is_none() {
            app.toast(
                "GStreamer is not available, video playback is disabled. See the README.",
                false,
            );
        }
        if let Err(error) = app.core.warn_if_expiring() {
            tracing::warn!(%error, "could not check the token expiry");
        }
        let task = Task::batch([app.load_everything(), library::load_cached(&app)]);
        (app, task)
    }

    pub fn theme(&self) -> Theme {
        theme::app_theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::run_with(CoreHandle(self.core.clone()), core_events),
            keyboard::listen().filter_map(on_key),
            iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick),
            video::subscription(&self.video, self.video_link.as_ref()),
        ])
    }

    fn load_everything(&mut self) -> Task<Message> {
        Task::batch([
            home::load(self),
            library::load(self),
            library::load_choices(self),
            anilist::load(self, MediaKind::Anime),
            mapping::load(self),
            sync::load(self),
            mapping::auto_match(self),
        ])
    }

    pub fn toast(&mut self, text: impl Into<String>, ok: bool) {
        self.toast = Some((text.into(), ok, Instant::now()));
    }

    pub fn report(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.toast(error, false);
        }
    }

    /// Lädt nach einer Anmeldung Status, Konfiguration und alle Daten neu.
    pub fn after_auth_change(&mut self) -> Task<Message> {
        self.auth = self.core.auth_status();
        self.config = self.core.config();
        self.settings.reload(&self.config);
        self.load_everything()
    }

    pub fn want_image(&mut self, key: ImageKey) -> Task<Message> {
        match self.images.request(&self.core, key) {
            Some(task) => task.map(|(key, result)| Message::ImageLoaded(key, result)),
            None => Task::none(),
        }
    }

    fn on_core_event(&mut self, event: CoreEvent) -> Task<Message> {
        match event {
            // Der Player zeichnet Position und Pause selbst.
            CoreEvent::Playback(_) => {}
            CoreEvent::PlaybackEnded => return home::load(self),
            CoreEvent::LibraryChanged => {
                return Task::batch([home::load(self), library::reload_view(self)]);
            }
            CoreEvent::ListChanged(kind) => anilist::list_changed(self, kind),
            CoreEvent::MappingsChanged => return mapping::load(self),
            CoreEvent::MatchProgress { done, total } => self.mapping.set_progress(done, total),
            CoreEvent::Notice { ok, message } => {
                self.toast(message, ok);
                return sync::load_status(self);
            }
            CoreEvent::RateLimited(Some(wait)) => {
                self.toast(
                    format!("AniList rate limit reached, waiting {} s", wait.as_secs()),
                    false,
                );
            }
            CoreEvent::RateLimited(None) => {}
            CoreEvent::AuthRequired(Service::AniList) => {
                self.toast("AniList session expired, please log in again", false);
            }
            CoreEvent::AuthRequired(Service::Jellyfin) => {
                self.toast("Jellyfin session expired, please log in again", false);
            }
            CoreEvent::TokenExpiring { days } => {
                self.toast(
                    format!("AniList token expires in {days} days, please log in again"),
                    false,
                );
            }
        }
        Task::none()
    }

    /// Setzt die Ansicht beim Wechsel der Seite zurück, auch beim erneuten Klick auf die
    /// aktuelle Seite: Detailseite und Dialoge schließen, Scroll-Position verwerfen.
    fn reset_view(&mut self, page: Page) {
        self.page = page;
        self.nav_generation += 1;
        self.view_epoch += 1;
        self.library.detail = None;
        self.anilist.close_modal();
        self.mapping.close_modal();
        self.help = false;
    }

    /// Wechselt die Seite und lädt ihre Daten neu.
    fn navigate(&mut self, page: Page) -> Task<Message> {
        self.reset_view(page);
        match page {
            Page::Home => home::load(self),
            Page::Library => library::load(self),
            Page::Mapping => mapping::load(self),
            Page::Sync => sync::load(self),
            Page::Profile => profile::load(self),
            Page::AniList | Page::Settings => Task::none(),
        }
    }

    fn on_escape(&mut self) {
        if self.help {
            self.help = false;
        } else if self.anilist.close_modal() || self.mapping.close_modal() {
        } else if self.page == Page::Home || self.page == Page::Library {
            self.close_detail();
        }
    }

    /// Schließt die Detailseite. Die Liste dahinter bekommt frischen Widget-Zustand.
    pub fn close_detail(&mut self) {
        if self.library.detail.take().is_some() {
            self.view_epoch += 1;
        }
    }

    /// Meldet eine neu geöffnete Ansicht, z. B. eine Detailseite.
    pub fn view_changed(&mut self) {
        self.view_epoch += 1;
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(page) => return self.navigate(page),
            Message::Refresh => {
                let profile = if self.page == Page::Profile {
                    profile::load(self)
                } else {
                    Task::none()
                };
                return Task::batch([self.load_everything(), profile]);
            }
            Message::Tick => {
                self.now = unix_now();
                if self
                    .toast
                    .as_ref()
                    .is_some_and(|(_, _, at)| at.elapsed().as_secs() >= TOAST_SECONDS)
                {
                    self.toast = None;
                }
            }
            Message::Escape if self.video.is_active() => {
                return video::update(self, video::Msg::Stop);
            }
            Message::Escape => self.on_escape(),
            Message::ToggleHelp => self.help = !self.help,
            Message::Core(event) => return self.on_core_event(event),
            Message::ImageLoaded(key, result) => self.images.loaded(key, result),
            Message::Notify(result) => self.report(result),
            Message::Home(msg) => return home::update(self, msg),
            Message::Library(msg) => return library::update(self, msg),
            Message::AniList(msg) => return anilist::update(self, msg),
            Message::Mapping(msg) => return mapping::update(self, msg),
            Message::Sync(msg) => return sync::update(self, msg),
            Message::Settings(msg) => return settings::update(self, msg),
            Message::Profile(msg) => return profile::update(self, msg),
            Message::Setup(msg) => return setup::update(self, msg),
            Message::Video(msg) => return video::update(self, msg),
            Message::Player(command) => {
                let core = self.core.clone();
                return ui::perform(
                    async move { core.player_command(command).await },
                    Message::Notify,
                );
            }
        }
        Task::none()
    }

    fn label(page: Page) -> &'static str {
        NAVIGATION
            .iter()
            .find(|(p, _)| *p == page)
            .map_or("", |(_, label)| label)
    }

    fn nav_item(&self, page: Page, glyph: Icon, cord: Color) -> Element<'_, Message> {
        let active = self.page == page;
        let tint = if active { cord } else { theme::MUTED };
        // Der schmale Streifen links zeigt die aktive Seite in der Farbe ihrer Gruppe.
        let marker = container(Space::new())
            .width(3)
            .height(18)
            .style(theme::solid(
                if active { cord } else { Color::TRANSPARENT },
                2.0,
            ));
        button(
            row![
                marker,
                ui::icon(glyph, 18.0, tint),
                text(Self::label(page)).size(theme::T_BODY).font(if active {
                    theme::SEMIBOLD
                } else {
                    theme::MEDIUM
                }),
            ]
            .spacing(theme::S3)
            .align_y(Center),
        )
        .width(Fill)
        .padding(iced::Padding {
            top: 9.0,
            right: theme::S3,
            bottom: 9.0,
            left: 0.0,
        })
        .style(theme::nav_button(active, cord))
        .on_press(Message::Navigate(page))
        .into()
    }

    fn account_line<'a>(
        service: &'a str,
        user: Option<&'a str>,
        cord: Color,
    ) -> Element<'a, Message> {
        row![
            container(Space::new())
                .width(6)
                .height(6)
                .style(theme::solid(
                    if user.is_some() { cord } else { theme::FAINT },
                    3.0,
                )),
            text(ui::ellipsize(user.unwrap_or(service), 22))
                .size(theme::T_LABEL)
                .color(if user.is_some() {
                    theme::MUTED
                } else {
                    theme::FAINT
                }),
        ]
        .spacing(theme::S2)
        .align_y(Center)
        .into()
    }

    fn navigation(&self) -> Element<'_, Message> {
        static MARK: std::sync::LazyLock<svg::Handle> = std::sync::LazyLock::new(|| {
            svg::Handle::from_memory(include_bytes!("../assets/mark.svg").as_slice())
        });
        let brand = row![
            svg(MARK.clone()).width(41).height(22),
            text("Musubi")
                .size(theme::T_TITLE)
                .font(theme::DISPLAY_BOLD)
                .color(theme::PAPER),
        ]
        .spacing(theme::S3)
        .align_y(Center);
        let mut groups = column![].spacing(theme::S5);
        for (name, cord, pages) in RAIL {
            let items = pages
                .iter()
                .map(|(page, glyph)| self.nav_item(*page, *glyph, cord));
            groups = groups.push(
                column![
                    container(
                        text(name)
                            .size(theme::T_CAPTION)
                            .font(theme::SEMIBOLD)
                            .color(theme::FAINT)
                    )
                    .padding(iced::padding::left(theme::S4 - 1.0)),
                    column(items).spacing(2),
                ]
                .spacing(theme::S2),
            );
        }
        let matching: Element<'_, Message> = match self.mapping.progress() {
            Some((done, total)) => container(
                column![
                    ui::muted(format!("Matching {done} of {total}")),
                    ui::bar(done as f32 / total.max(1) as f32, theme::TRACK, 3.0),
                ]
                .spacing(6),
            )
            .padding([0.0, theme::S4])
            .into(),
            None => Space::new().into(),
        };
        let footer = column![
            Self::account_line("Jellyfin", self.auth.jellyfin_user.as_deref(), theme::WATCH),
            Self::account_line("AniList", self.auth.anilist_user.as_deref(), theme::TRACK),
            // Zeigt, welcher Build läuft, z. B. ob der Starter noch eine alte Binary öffnet.
            text(format!("v{VERSION} · {COMMIT}"))
                .size(theme::T_CAPTION)
                .color(theme::FAINT),
        ]
        .spacing(6)
        .padding([0.0, theme::S4]);
        let rail = container(
            column![
                container(brand).padding(iced::Padding {
                    top: theme::S2,
                    right: 0.0,
                    bottom: theme::S3,
                    left: theme::S4 - 1.0,
                }),
                groups,
                Space::new().height(Fill),
                matching,
                self.nav_item(Page::Settings, Icon::Settings, theme::PAPER),
                ui::divider(),
                footer,
            ]
            .spacing(theme::S4),
        )
        .padding(iced::Padding {
            top: theme::S4,
            right: theme::S2,
            bottom: theme::S4,
            left: theme::S2,
        })
        .width(RAIL_WIDTH)
        .height(Fill)
        .style(theme::rail);
        row![
            rail,
            container(Space::new())
                .width(1)
                .height(Fill)
                .style(theme::line)
        ]
        .into()
    }

    /// Ob die Detailansicht zur aktuellen Seite gehört.
    fn shows_detail(&self) -> bool {
        self.library
            .detail
            .as_ref()
            .is_some_and(|d| d.origin == self.page && d.opened_at == self.nav_generation)
    }

    /// Die aktuelle Seite. Iced übernimmt Widget-Zustand wie die Scroll-Position, solange an
    /// derselben Stelle derselbe Widget-Typ steht. Mit jedem Ansichtswechsel wechselt deshalb
    /// die Verschachtelungstiefe, und die neue Ansicht beginnt garantiert oben und frisch.
    fn page_view(&self) -> Element<'_, Message> {
        let content = self.page_content();
        let content = if self.view_epoch.is_multiple_of(2) {
            content
        } else {
            container(content).width(Fill).height(Fill).into()
        };
        container(content).width(Fill).height(Fill).into()
    }

    fn page_content(&self) -> Element<'_, Message> {
        match self.page {
            Page::Home | Page::Library if self.shows_detail() => library::detail_view(self),
            Page::Home => home::view(self),
            Page::Library => library::view(self),
            Page::AniList => anilist::view(self),
            Page::Mapping => mapping::view(self),
            Page::Sync => sync::view(self),
            Page::Settings => settings::view(self),
            Page::Profile => profile::view(self),
        }
    }

    fn help_body(&self) -> Element<'_, Message> {
        let rows = SHORTCUTS
            .iter()
            .map(|(keys, action)| {
                row![
                    text(*keys)
                        .size(theme::T_SMALL)
                        .font(theme::SEMIBOLD)
                        .width(190)
                        .color(theme::PAPER),
                    text(*action).size(theme::T_SMALL).color(theme::MUTED),
                ]
                .padding([7, 0])
                .into()
            })
            .collect();
        ui::divided(rows).into()
    }

    fn toast_view(&self) -> Option<Element<'_, Message>> {
        let (message, ok, _) = self.toast.as_ref()?;
        Some(
            iced::widget::bottom_right(ui::toast(message, *ok))
                .padding(iced::padding::bottom(84).right(theme::S5))
                .into(),
        )
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.video.is_active() {
            let player = video::view(self);
            return match self.toast_view() {
                Some(toast) => stack![player, toast].into(),
                None => player,
            };
        }
        let shell: Element<'_, Message> = row![
            self.navigation(),
            container(self.page_view()).width(Fill).height(Fill)
        ]
        .align_y(Center)
        .into();
        let mut layered = anilist::with_modal(self, shell);
        layered = mapping::with_modal(self, layered);
        if self.help {
            layered = ui::modal(
                layered,
                "Keyboard shortcuts",
                460.0,
                column![
                    self.help_body(),
                    ui::secondary("Close", Message::ToggleHelp)
                ]
                .spacing(theme::S4)
                .into(),
            );
        }
        if let Some(state) = &self.setup {
            layered = setup::with_modal(self, state, layered);
        }
        match self.toast_view() {
            Some(toast) => stack![layered, toast].into(),
            None => layered,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key, modifiers: keyboard::Modifiers) -> Option<Message> {
        on_key(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }

    use musubi_core::domain::{ItemKind, LibraryItem};
    use musubi_core::{CoreOptions, Paths};

    async fn app(dir: &std::path::Path) -> App {
        let options = CoreOptions {
            anilist_endpoint: None,
            player: None,
        };
        let core = Core::open(Paths::under(dir).unwrap(), options)
            .await
            .unwrap();
        App::boot(core, None).0
    }

    fn show() -> LibraryItem {
        LibraryItem {
            id: "s1".into(),
            kind: ItemKind::Series,
            name: "Show".into(),
            overview: None,
            year: None,
            status: None,
            genres: vec![],
            community_rating: None,
            runtime_secs: None,
            provider_ids: Default::default(),
            has_poster: false,
            has_backdrop: false,
            played: false,
            unplayed_count: None,
            position_secs: 0,
        }
    }

    #[tokio::test]
    async fn every_page_switch_closes_the_detail_view_and_dialogs() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path()).await;
        for (from, to) in [
            (Page::Home, Page::Library),
            (Page::Library, Page::AniList),
            (Page::Library, Page::Library),
            (Page::Home, Page::Home),
        ] {
            app.page = from;
            let _ = library::open_detail(&mut app, show());
            mapping::open_change_dialog(&mut app, show(), Some(2));
            assert!(app.shows_detail());

            let _ = app.update(Message::Navigate(to));

            assert_eq!(app.page, to);
            assert!(app.library.detail.is_none(), "{from:?} -> {to:?}");
            assert!(!app.shows_detail());
            assert!(!app.mapping.close_modal(), "dialog stayed open");
        }
    }

    #[tokio::test]
    async fn every_view_change_gets_fresh_widget_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path()).await;
        app.page = Page::Library;
        let mut last = app.view_epoch;
        let mut changed = |app: &App| {
            let flipped = app.view_epoch % 2 != last % 2;
            last = app.view_epoch;
            flipped
        };
        let _ = library::open_detail(&mut app, show());
        assert!(changed(&app), "opening the detail view");
        app.close_detail();
        assert!(changed(&app), "closing the detail view");
        let _ = library::open_detail(&mut app, show());
        assert!(changed(&app));
        // Seitenwechsel bei offener Detailseite ist ein einziger Wechsel, kein doppelter.
        let _ = app.update(Message::Navigate(Page::Home));
        assert!(changed(&app), "switching the page");
    }

    #[tokio::test]
    async fn detail_only_shows_on_the_page_that_opened_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path()).await;
        app.page = Page::Home;
        let _ = library::open_detail(&mut app, show());
        // Ein Pfad, der die Seite ohne Reset wechselt, darf keine fremde Detailseite zeigen.
        app.page = Page::Library;
        assert!(!app.shows_detail());
    }

    #[tokio::test]
    async fn a_detail_from_before_the_last_click_never_shows_again() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path()).await;
        app.page = Page::Home;
        let _ = library::open_detail(&mut app, show());
        let stale = app.library.detail.take();

        let _ = app.update(Message::Navigate(Page::Home));
        // Selbst wenn ein Pfad den alten Zustand nach dem Klick zurückschreibt, bleibt die
        // Startseite sichtbar.
        app.library.detail = stale;

        assert_eq!(app.page, Page::Home);
        assert!(!app.shows_detail());
        let _ = library::open_detail(&mut app, show());
        assert!(app.shows_detail(), "opening it again after the click works");
    }

    #[tokio::test]
    async fn a_late_home_result_does_not_pull_back_to_a_detail_view() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path()).await;
        app.page = Page::Home;
        let clicked_at = app.nav_generation;
        let _ = app.update(Message::Navigate(Page::Library));

        let _ = app.update(Message::Home(home::Msg::Opened(clicked_at, Ok(show()))));

        assert_eq!(app.page, Page::Library);
        assert!(app.library.detail.is_none());

        // Ohne Seitenwechsel öffnet das Ergebnis die Detailseite wie gewohnt.
        let _ = app.update(Message::Navigate(Page::Home));
        let now = app.nav_generation;
        let _ = app.update(Message::Home(home::Msg::Opened(now, Ok(show()))));
        assert!(app.shows_detail());
    }

    #[test]
    fn number_shortcuts_switch_pages_and_ignore_zero() {
        let two = press(Key::Character("2".into()), keyboard::Modifiers::CTRL);
        assert!(matches!(two, Some(Message::Navigate(Page::Library))));
        assert!(press(Key::Character("0".into()), keyboard::Modifiers::CTRL).is_none());
        let seven = press(Key::Character("7".into()), keyboard::Modifiers::CTRL);
        assert!(matches!(seven, Some(Message::Navigate(Page::Profile))));
        assert!(press(Key::Character("8".into()), keyboard::Modifiers::CTRL).is_none());
        assert!(press(Key::Character("2".into()), keyboard::Modifiers::empty()).is_none());
    }
}
