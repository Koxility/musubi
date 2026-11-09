//! Hauptzustand: verbindet Seiten, Kern, Tastenkürzel und Dialoge.

use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, key::Named};
use iced::widget::image::Handle;
use iced::widget::{button, column, container, row, stack, text};
use iced::{Center, Element, Fill, Subscription, Task, Theme};
use musubi_core::domain::MediaKind;
use musubi_core::{AuthStatus, Config, Core, CoreEvent, PlaybackState, PlayerCommand, Service};
use tokio::sync::broadcast;

use crate::images::{ImageCache, ImageKey};
use crate::ui;
use crate::{anilist, home, library, mapping, player, settings, setup, sync, theme};

const TOAST_SECONDS: u64 = 5;
const SEEK_STEP: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Library,
    AniList,
    Mapping,
    Sync,
    Settings,
}

const NAVIGATION: [(Page, &str); 6] = [
    (Page::Home, "Home"),
    (Page::Library, "Library"),
    (Page::AniList, "AniList"),
    (Page::Mapping, "Mapping"),
    (Page::Sync, "Sync"),
    (Page::Settings, "Settings"),
];

const SHORTCUTS: [(&str, &str); 10] = [
    ("Ctrl+1 to 6", "Switch page"),
    ("Ctrl+R", "Refresh"),
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
    Setup(setup::Msg),
    Player(PlayerCommand),
    SeekPreview(f64),
    SeekCommit,
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
    pub setup: Option<setup::State>,
    pub playback: Option<PlaybackState>,
    pub seek_preview: Option<f64>,
    pub now: i64,
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
    pub fn boot(core: Core) -> (Self, Task<Message>) {
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
            images: ImageCache::default(),
            home: home::State::default(),
            library: library::State::default(),
            anilist: anilist::State::default(),
            mapping: mapping::State::default(),
            sync: sync::State::default(),
            settings: settings::State::new(&config),
            playback: None,
            seek_preview: None,
            now: unix_now(),
            toast: None,
            help: false,
            auth,
            config,
            core,
        };
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
            CoreEvent::Playback(state) => self.playback = Some(state),
            CoreEvent::PlaybackEnded => {
                self.playback = None;
                self.seek_preview = None;
                return home::load(self);
            }
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

    fn on_escape(&mut self) {
        if self.help {
            self.help = false;
        } else if self.anilist.close_modal() || self.mapping.close_modal() {
        } else if self.page == Page::Home || self.page == Page::Library {
            self.library.detail = None;
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(page) => {
                self.page = page;
                if page == Page::Sync {
                    return sync::load(self);
                }
            }
            Message::Refresh => return self.load_everything(),
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
            Message::Setup(msg) => return setup::update(self, msg),
            Message::Player(command) => {
                let core = self.core.clone();
                return ui::perform(
                    async move { core.player_command(command).await },
                    Message::Notify,
                );
            }
            Message::SeekPreview(position) => self.seek_preview = Some(position),
            Message::SeekCommit => {
                if let Some(position) = self.seek_preview.take() {
                    return self.update(Message::Player(PlayerCommand::SeekAbsolute(position)));
                }
            }
        }
        Task::none()
    }

    fn navigation(&self) -> Element<'_, Message> {
        let items = NAVIGATION.iter().map(|(page, label)| {
            button(text(*label).size(15))
                .width(Fill)
                .padding([10, 14])
                .style(theme::nav_button(self.page == *page))
                .on_press(Message::Navigate(*page))
                .into()
        });
        container(
            column![
                container(text("Musubi").size(22).color(theme::ACCENT)).padding([8, 14]),
                column(items).spacing(4),
            ]
            .spacing(18),
        )
        .padding(12)
        .width(190)
        .height(Fill)
        .style(theme::rail)
        .into()
    }

    fn page_view(&self) -> Element<'_, Message> {
        match self.page {
            Page::Home if self.library.detail.is_some() => library::detail_view(self),
            Page::Home => home::view(self),
            Page::Library if self.library.detail.is_some() => library::detail_view(self),
            Page::Library => library::view(self),
            Page::AniList => anilist::view(self),
            Page::Mapping => mapping::view(self),
            Page::Sync => sync::view(self),
            Page::Settings => settings::view(self),
        }
    }

    fn help_body(&self) -> Element<'_, Message> {
        let rows = SHORTCUTS.iter().map(|(keys, action)| {
            row![
                text(*keys).size(14).width(170).color(theme::ACCENT),
                text(*action).size(14),
            ]
            .into()
        });
        column(rows).spacing(8).into()
    }

    fn toast_view(&self) -> Option<Element<'_, Message>> {
        let (message, ok, _) = self.toast.as_ref()?;
        Some(
            iced::widget::bottom_right(
                container(
                    text(message)
                        .size(14)
                        .color(if *ok { theme::TEXT } else { theme::RED }),
                )
                .padding([10, 16])
                .style(theme::surface),
            )
            .padding(iced::padding::bottom(84).right(20))
            .into(),
        )
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = column![self.page_view(), player::view(self)];
        let shell: Element<'_, Message> = row![
            self.navigation(),
            container(content).width(Fill).height(Fill)
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
                .spacing(16)
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

    #[test]
    fn number_shortcuts_switch_pages_and_ignore_zero() {
        let two = press(Key::Character("2".into()), keyboard::Modifiers::CTRL);
        assert!(matches!(two, Some(Message::Navigate(Page::Library))));
        assert!(press(Key::Character("0".into()), keyboard::Modifiers::CTRL).is_none());
        assert!(press(Key::Character("2".into()), keyboard::Modifiers::empty()).is_none());
    }
}
