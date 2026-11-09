//! Anmeldungen und Einstellungen.

use iced::widget::{column, container, row, scrollable, slider, text, toggler};
use iced::{Center, Element, Fill, Task};
use musubi_core::oauth;
use musubi_core::{Config, PlayerKind};

use crate::app::{App, Message};
use crate::{setup, theme, ui};

#[derive(Default)]
pub struct State {
    url: String,
    user: String,
    password: String,
    api_key: String,
    client_id: String,
    token: String,
    redirect: String,
    auto_sync: bool,
    threshold: f64,
    mpv_path: String,
    hwdec: String,
    autoplay: bool,
    embedded: bool,
}

impl State {
    pub fn new(config: &Config) -> Self {
        let mut state = Self::default();
        state.reload(config);
        state
    }

    /// Übernimmt die Konfiguration. Nicht gespeicherte Passwörter bleiben nicht erhalten.
    pub fn reload(&mut self, config: &Config) {
        self.url = config.jellyfin.url.clone().unwrap_or_default();
        self.password.clear();
        self.api_key.clear();
        self.token.clear();
        self.client_id = config.anilist.client_id.clone().unwrap_or_default();
        self.redirect = oauth::loopback_redirect(config.anilist.loopback_port);
        self.auto_sync = config.sync.auto_sync;
        self.threshold = config.sync.watched_threshold * 100.0;
        self.mpv_path.clone_from(&config.player.mpv_path);
        self.hwdec.clone_from(&config.player.hwdec);
        self.autoplay = config.player.autoplay_next;
        self.embedded = config.player.backend == PlayerKind::Embedded;
    }

    fn apply(&self, config: &mut Config) {
        config.sync.auto_sync = self.auto_sync;
        config.sync.watched_threshold = self.threshold / 100.0;
        config.player.mpv_path = self.mpv_path.trim().to_owned();
        config.player.hwdec = self.hwdec.trim().to_owned();
        config.player.autoplay_next = self.autoplay;
        config.player.backend = if self.embedded {
            PlayerKind::Embedded
        } else {
            PlayerKind::Mpv
        };
        let client_id = self.client_id.trim().to_owned();
        config.anilist.client_id = (!client_id.is_empty()).then_some(client_id);
    }
}

#[derive(Debug, Clone)]
pub enum Edit {
    Url(String),
    User(String),
    Password(String),
    ApiKey(String),
    ClientId(String),
    Token(String),
    AutoSync(bool),
    Threshold(f64),
    MpvPath(String),
    Hwdec(String),
    Autoplay(bool),
    Embedded(bool),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Edit(Edit),
    LoginJellyfin,
    LoginAniListBrowser,
    LoginAniListToken,
    LogoutJellyfin,
    LogoutAniList,
    LoggedIn(Result<String, String>),
    Save,
    RunSetup,
}

fn logged_in(result: Result<String, String>) -> Message {
    Message::Settings(Msg::LoggedIn(result))
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let core = app.core.clone();
    match msg {
        Msg::Edit(edit) => {
            let s = &mut app.settings;
            match edit {
                Edit::Url(v) => s.url = v,
                Edit::User(v) => s.user = v,
                Edit::Password(v) => s.password = v,
                Edit::ApiKey(v) => s.api_key = v,
                Edit::ClientId(v) => s.client_id = v,
                Edit::Token(v) => s.token = v,
                Edit::AutoSync(v) => s.auto_sync = v,
                Edit::Threshold(v) => s.threshold = v,
                Edit::MpvPath(v) => s.mpv_path = v,
                Edit::Hwdec(v) => s.hwdec = v,
                Edit::Autoplay(v) => s.autoplay = v,
                Edit::Embedded(v) => s.embedded = v,
            }
        }
        Msg::LoginJellyfin => {
            let s = &mut app.settings;
            if s.url.trim().is_empty() || s.user.trim().is_empty() {
                app.toast("Server URL and username are required.", false);
                return Task::none();
            }
            let credentials = setup::JellyfinLogin {
                url: s.url.trim().to_owned(),
                user: s.user.trim().to_owned(),
                password: std::mem::take(&mut s.password),
                api_key: std::mem::take(&mut s.api_key),
            };
            return ui::perform(credentials.run(core), logged_in);
        }
        Msg::LoginAniListBrowser => {
            let id = app.settings.client_id.trim().to_owned();
            if id.is_empty() {
                app.toast("Enter the client ID of your AniList app.", false);
                return Task::none();
            }
            app.toast("Waiting for the browser…", true);
            return ui::perform(setup::login_anilist_browser(core, id), logged_in);
        }
        Msg::LoginAniListToken => {
            let token = std::mem::take(&mut app.settings.token);
            return ui::perform(
                async move { Ok(core.login_anilist_pin(&token).await?.name) },
                logged_in,
            );
        }
        Msg::LogoutJellyfin => {
            return ui::perform(
                async move {
                    core.logout_jellyfin().await?;
                    Ok("logged out".to_owned())
                },
                logged_in,
            );
        }
        Msg::LogoutAniList => {
            return ui::perform(
                async move {
                    core.logout_anilist().await?;
                    Ok("logged out".to_owned())
                },
                logged_in,
            );
        }
        Msg::LoggedIn(Ok(name)) => {
            app.toast(format!("Done: {name}"), true);
            return app.after_auth_change();
        }
        Msg::LoggedIn(Err(error)) => app.toast(error, false),
        Msg::Save => {
            let result = core.update_config(|c| app.settings.apply(c));
            match result {
                Ok(()) => {
                    app.config = core.config();
                    app.toast("Settings saved", true);
                }
                Err(error) => app.toast(error.to_string(), false),
            }
        }
        Msg::RunSetup => app.setup = Some(setup::State::new(&app.config, &app.auth)),
    }
    Task::none()
}

fn status(label: &str, value: String) -> Element<'_, Msg> {
    row![
        text(label).size(14).width(90).color(theme::MUTED),
        text(value).size(14)
    ]
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let s = &app.settings;
    let edit = |wrap: fn(String) -> Edit| move |v: String| Msg::Edit(wrap(v));
    let jellyfin = column![
        ui::heading("Jellyfin"),
        status("Status", setup::jellyfin_status(&app.auth)),
        ui::field("Server URL", &s.url, edit(Edit::Url)),
        ui::field("Username", &s.user, edit(Edit::User)),
        ui::secret_field(
            "Password (never stored)",
            &s.password,
            edit(Edit::Password),
            Some(Msg::LoginJellyfin)
        ),
        ui::secret_field("or API key", &s.api_key, edit(Edit::ApiKey), None),
        row![
            ui::primary("Log in", Msg::LoginJellyfin),
            ui::secondary("Log out", Msg::LogoutJellyfin),
        ]
        .spacing(8),
    ]
    .spacing(12);
    let anilist = column![
        ui::heading("AniList"),
        status("Status", setup::anilist_status(&app.auth)),
        ui::muted("Create an app at anilist.co/settings/developer and enter its client ID."),
        ui::field("Client ID", &s.client_id, edit(Edit::ClientId)),
        ui::muted(format!("Redirect URL: {}", s.redirect)),
        ui::secret_field(
            "Token or redirect URL (PIN flow)",
            &s.token,
            edit(Edit::Token),
            None
        ),
        row![
            ui::primary("Log in with browser", Msg::LoginAniListBrowser),
            ui::secondary("Use pasted token", Msg::LoginAniListToken),
            ui::secondary("Log out", Msg::LogoutAniList),
        ]
        .spacing(8),
    ]
    .spacing(12);
    let preferences = column![
        ui::heading("Sync and player"),
        toggler(s.auto_sync)
            .label("Update AniList automatically while watching")
            .text_size(14)
            .on_toggle(|v| Msg::Edit(Edit::AutoSync(v))),
        column![
            ui::muted(format!("Count as watched at {:.0}%", s.threshold)),
            slider(50.0..=100.0, s.threshold, |v| Msg::Edit(Edit::Threshold(v))).step(1.0),
        ]
        .spacing(4),
        toggler(s.embedded)
            .label("Play videos inside Musubi (off: external mpv window)")
            .text_size(14)
            .on_toggle(|v| Msg::Edit(Edit::Embedded(v))),
        ui::field("mpv binary", &s.mpv_path, edit(Edit::MpvPath)),
        ui::field(
            "Hardware decoding (--hwdec, empty = mpv.conf)",
            &s.hwdec,
            edit(Edit::Hwdec)
        ),
        toggler(s.autoplay)
            .label("Play next episode automatically")
            .text_size(14)
            .on_toggle(|v| Msg::Edit(Edit::Autoplay(v))),
        ui::primary("Save settings", Msg::Save),
    ]
    .spacing(12);
    let body = column![
        row![
            ui::heading("Settings"),
            ui::spacer(),
            ui::secondary("Run setup", Msg::RunSetup),
        ]
        .align_y(Center),
        ui::card(jellyfin),
        ui::card(anilist),
        ui::card(preferences),
    ]
    .spacing(18)
    .max_width(760)
    .padding(iced::padding::right(14));
    let page: Element<'_, Msg> = container(scrollable(body).width(Fill).height(Fill))
        .padding(24)
        .center_x(Fill)
        .into();
    page.map(Message::Settings)
}
