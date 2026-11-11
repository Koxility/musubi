//! Anmeldungen und Einstellungen.

use iced::widget::{column, container, row, slider, svg, text};
use iced::{Center, Element, Fill, Task};
use musubi_core::Config;
use musubi_core::oauth;

use crate::app::{App, COMMIT, Message, VERSION};
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
    autoplay: bool,
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
        self.autoplay = config.player.autoplay_next;
    }

    fn apply(&self, config: &mut Config) {
        config.sync.auto_sync = self.auto_sync;
        config.sync.watched_threshold = self.threshold / 100.0;
        config.player.autoplay_next = self.autoplay;
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
    Autoplay(bool),
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
                Edit::Autoplay(v) => s.autoplay = v,
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

/// Kopf eines Abschnitts: Name links, Verbindungsstand rechts in der Kordelfarbe.
fn service_head<'a>(
    name: &'a str,
    connected: bool,
    detail: String,
    cord: iced::Color,
) -> Element<'a, Msg> {
    row![
        ui::heading(name),
        ui::spacer(),
        ui::muted(detail),
        ui::tag(
            if connected {
                "Connected"
            } else {
                "Not connected"
            },
            if connected { cord } else { theme::MUTED }
        ),
    ]
    .spacing(theme::S3)
    .align_y(Center)
    .into()
}

/// Version und Git-Stand dieses Builds, damit Fehlerberichte eindeutig sind.
fn half<'a>(field: Element<'a, Msg>) -> Element<'a, Msg> {
    container(field).width(Fill).into()
}

fn about<'a>() -> Element<'a, Msg> {
    static MARK: std::sync::LazyLock<svg::Handle> = std::sync::LazyLock::new(|| {
        svg::Handle::from_memory(include_bytes!("../assets/mark.svg").as_slice())
    });
    let fact = |name: &'a str, value: &'a str| {
        row![
            ui::muted(name),
            text(value).size(theme::T_SMALL).font(theme::SEMIBOLD),
        ]
        .spacing(6)
        .align_y(Center)
    };
    row![
        svg(MARK.clone()).width(45).height(24),
        column![
            text("Musubi").size(theme::T_BODY).font(theme::DISPLAY),
            ui::muted("Watch on Jellyfin, track on AniList."),
        ]
        .spacing(1),
        ui::spacer(),
        fact("Version", VERSION),
        fact("Commit", COMMIT),
    ]
    .spacing(theme::S4)
    .align_y(Center)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let s = &app.settings;
    let edit = |wrap: fn(String) -> Edit| move |v: String| Msg::Edit(wrap(v));
    let jellyfin = column![
        service_head(
            "Jellyfin",
            app.auth.jellyfin_user.is_some(),
            setup::jellyfin_status(&app.auth),
            theme::WATCH
        ),
        ui::field("Server URL", &s.url, edit(Edit::Url)),
        row![
            half(ui::field("Username", &s.user, edit(Edit::User))),
            half(ui::secret_field(
                "Password (never stored)",
                &s.password,
                edit(Edit::Password),
                Some(Msg::LoginJellyfin)
            )),
        ]
        .spacing(theme::S3),
        ui::secret_field(
            "Or an API key instead of the password",
            &s.api_key,
            edit(Edit::ApiKey),
            None
        ),
        row![
            ui::watch("Log in to Jellyfin", Msg::LoginJellyfin),
            ui::quiet("Log out", Msg::LogoutJellyfin),
        ]
        .spacing(theme::S2)
        .align_y(Center),
    ]
    .spacing(theme::S3);
    let anilist = column![
        service_head(
            "AniList",
            app.auth.anilist_logged_in,
            setup::anilist_status(&app.auth),
            theme::TRACK
        ),
        ui::muted("Create an app at anilist.co/settings/developer and enter its client ID."),
        row![
            half(ui::field("Client ID", &s.client_id, edit(Edit::ClientId))),
            half(
                column![
                    ui::label("Redirect URL for that app"),
                    container(text(&s.redirect).size(theme::T_SMALL).color(theme::MUTED))
                        .padding([9, 10])
                        .width(Fill)
                        .style(theme::inset),
                ]
                .spacing(6)
                .into()
            ),
        ]
        .spacing(theme::S3),
        ui::secret_field(
            "Token or redirect URL, if you log in without the browser flow",
            &s.token,
            edit(Edit::Token),
            None
        ),
        row![
            ui::primary("Log in with browser", Msg::LoginAniListBrowser),
            ui::secondary("Use pasted token", Msg::LoginAniListToken),
            ui::quiet("Log out", Msg::LogoutAniList),
        ]
        .spacing(theme::S2)
        .align_y(Center),
    ]
    .spacing(theme::S3);
    let preferences = column![
        ui::heading("Sync and player"),
        ui::toggle(
            "Update AniList automatically while watching",
            s.auto_sync,
            |v| Msg::Edit(Edit::AutoSync(v))
        ),
        ui::toggle("Play the next episode automatically", s.autoplay, |v| {
            Msg::Edit(Edit::Autoplay(v))
        }),
        column![
            row![
                text("Count an episode as watched at").size(theme::T_BODY),
                text(format!("{:.0}%", s.threshold))
                    .size(theme::T_BODY)
                    .font(theme::SEMIBOLD)
                    .color(theme::TRACK),
            ]
            .spacing(6),
            slider(50.0..=100.0, s.threshold, |v| Msg::Edit(Edit::Threshold(v)))
                .step(1.0)
                .style(theme::form_slider),
        ]
        .spacing(theme::S2),
        row![ui::primary("Save settings", Msg::Save)],
    ]
    .spacing(theme::S4);
    let body = column![
        ui::page_header(
            "Settings",
            None,
            vec![ui::secondary("Run setup again", Msg::RunSetup)]
        ),
        ui::card(jellyfin),
        ui::card(anilist),
        ui::card(preferences),
        ui::divider(),
        about(),
    ]
    .spacing(theme::S4)
    .max_width(820)
    .padding(iced::Padding {
        top: theme::S5,
        right: ui::PAGE_PADDING,
        bottom: theme::S6,
        left: ui::PAGE_PADDING,
    });
    let page: Element<'_, Msg> = ui::scroll(body);
    page.map(Message::Settings)
}
