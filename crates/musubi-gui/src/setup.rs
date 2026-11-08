//! Geführte Einrichtung: Jellyfin, AniList und Sync in drei Schritten.

use iced::widget::{Column, column, row, text, toggler};
use iced::{Center, Element, Task};
use musubi_core::{AuthStatus, Config, Core, oauth};
use secrecy::SecretString;

use crate::app::{App, Message};
use crate::{theme, ui};

const DEVELOPER_URL: &str = "https://anilist.co/settings/developer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Jellyfin,
    AniList,
    Sync,
}

impl Step {
    fn number(self) -> u8 {
        match self {
            Self::Jellyfin => 1,
            Self::AniList => 2,
            Self::Sync => 3,
        }
    }
}

pub struct State {
    step: Step,
    url: String,
    user: String,
    password: String,
    api_key: String,
    client_id: String,
    token: String,
    redirect: String,
    auto_sync: bool,
    message: Option<(String, bool)>,
}

/// Zugangsdaten einer Jellyfin-Anmeldung. Ein API-Key hat Vorrang vor dem Passwort.
pub struct JellyfinLogin {
    pub url: String,
    pub user: String,
    pub password: String,
    pub api_key: String,
}

impl JellyfinLogin {
    pub async fn run(self, core: Core) -> musubi_core::Result<String> {
        if self.api_key.is_empty() {
            let password = SecretString::from(self.password);
            core.login_jellyfin(&self.url, &self.user, &password).await
        } else {
            let key = SecretString::from(self.api_key);
            core.login_jellyfin_api_key(&self.url, key, &self.user)
                .await
        }
    }
}

pub async fn login_anilist_browser(core: Core, client_id: String) -> musubi_core::Result<String> {
    core.update_config(|c| c.anilist.client_id = Some(client_id))?;
    Ok(core.login_anilist_loopback().await?.name)
}

pub fn jellyfin_status(auth: &AuthStatus) -> String {
    match (&auth.jellyfin_user, &auth.jellyfin_url) {
        (Some(user), Some(url)) => format!("{user} on {url}"),
        _ => "Not logged in".to_owned(),
    }
}

pub fn anilist_status(auth: &AuthStatus) -> String {
    match (&auth.anilist_user, auth.anilist_logged_in) {
        (Some(user), _) => user.clone(),
        (None, true) => "Logged in".to_owned(),
        _ => "Not logged in".to_owned(),
    }
}

impl State {
    pub fn new(config: &Config, auth: &AuthStatus) -> Self {
        Self {
            step: if auth.jellyfin_user.is_some() && !auth.anilist_logged_in {
                Step::AniList
            } else {
                Step::Jellyfin
            },
            url: config.jellyfin.url.clone().unwrap_or_default(),
            user: auth.jellyfin_user.clone().unwrap_or_default(),
            password: String::new(),
            api_key: String::new(),
            client_id: config.anilist.client_id.clone().unwrap_or_default(),
            token: String::new(),
            redirect: oauth::loopback_redirect(config.anilist.loopback_port),
            auto_sync: config.sync.auto_sync,
            message: None,
        }
    }

    fn info(&mut self, text: &str) {
        self.message = Some((text.to_owned(), false));
    }

    fn error(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), true));
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
}

#[derive(Debug, Clone)]
pub enum Msg {
    Edit(Edit),
    LoginJellyfin,
    OpenDeveloperPage,
    LoginAniListBrowser,
    LoginAniListToken,
    LoggedIn(Result<String, String>),
    Back,
    Next,
    Close,
}

fn logged_in(result: Result<String, String>) -> Message {
    Message::Setup(Msg::LoggedIn(result))
}

fn finish(app: &mut App, auto_sync: Option<bool>) -> Task<Message> {
    let result = app.core.update_config(|c| {
        c.ui.setup_completed = true;
        if let Some(enabled) = auto_sync {
            c.sync.auto_sync = enabled;
        }
    });
    match result {
        Ok(()) => {
            app.setup = None;
            app.config = app.core.config();
            app.settings.reload(&app.config);
        }
        Err(error) => {
            if let Some(state) = &mut app.setup {
                state.error(error.to_string());
            }
        }
    }
    Task::none()
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let core = app.core.clone();
    match msg {
        Msg::Close => return finish(app, None),
        Msg::Next if app.setup.as_ref().is_some_and(|s| s.step == Step::Sync) => {
            let auto_sync = app.setup.as_ref().map(|s| s.auto_sync);
            return finish(app, auto_sync);
        }
        Msg::LoggedIn(Ok(name)) => {
            app.toast(format!("Done: {name}"), true);
            app.auth = core.auth_status();
            let auth = app.auth.clone();
            if let Some(state) = &mut app.setup {
                state.message = None;
                match state.step {
                    Step::Jellyfin if auth.jellyfin_user.is_some() => state.step = Step::AniList,
                    Step::AniList if auth.anilist_logged_in => state.step = Step::Sync,
                    _ => {}
                }
            }
            return app.after_auth_change();
        }
        _ => {}
    }
    let Some(state) = &mut app.setup else {
        return Task::none();
    };
    match msg {
        Msg::Edit(edit) => match edit {
            Edit::Url(v) => state.url = v,
            Edit::User(v) => state.user = v,
            Edit::Password(v) => state.password = v,
            Edit::ApiKey(v) => state.api_key = v,
            Edit::ClientId(v) => state.client_id = v,
            Edit::Token(v) => state.token = v,
            Edit::AutoSync(v) => state.auto_sync = v,
        },
        Msg::LoginJellyfin => {
            if state.url.trim().is_empty() || state.user.trim().is_empty() {
                state.error("Server URL and username are required.");
                return Task::none();
            }
            state.info("Logging in…");
            let credentials = JellyfinLogin {
                url: state.url.trim().to_owned(),
                user: state.user.trim().to_owned(),
                password: std::mem::take(&mut state.password),
                api_key: std::mem::take(&mut state.api_key),
            };
            return ui::perform(credentials.run(core), logged_in);
        }
        Msg::OpenDeveloperPage => {
            if open::that_detached(DEVELOPER_URL).is_err() {
                state.error(format!("Could not open a browser, visit {DEVELOPER_URL}"));
            }
        }
        Msg::LoginAniListBrowser => {
            let id = state.client_id.trim().to_owned();
            if id.is_empty() {
                state.error("Enter the client ID of your AniList app.");
                return Task::none();
            }
            state.info("Waiting for the browser…");
            return ui::perform(login_anilist_browser(core, id), logged_in);
        }
        Msg::LoginAniListToken => {
            state.info("Checking the token…");
            let token = std::mem::take(&mut state.token);
            return ui::perform(
                async move { Ok(core.login_anilist_pin(&token).await?.name) },
                logged_in,
            );
        }
        Msg::LoggedIn(Err(error)) => state.error(error),
        Msg::Back => {
            state.message = None;
            state.step = match state.step {
                Step::Sync => Step::AniList,
                _ => Step::Jellyfin,
            };
        }
        Msg::Next => {
            state.message = None;
            state.step = match state.step {
                Step::Jellyfin => Step::AniList,
                _ => Step::Sync,
            };
        }
        Msg::Close | Msg::LoggedIn(Ok(_)) => {}
    }
    Task::none()
}

fn status<'a>(value: String) -> Element<'a, Msg> {
    row![
        text("Status").size(14).width(70).color(theme::MUTED),
        text(value).size(14)
    ]
    .into()
}

fn jellyfin_step<'a>(state: &'a State, auth: &AuthStatus) -> Column<'a, Msg> {
    let edit = |wrap: fn(String) -> Edit| move |v: String| Msg::Edit(wrap(v));
    column![
        ui::muted("Enter the server address with its port, for example 192.168.1.10:8096."),
        status(jellyfin_status(auth)),
        ui::field("Server URL", &state.url, edit(Edit::Url)),
        ui::field("Username", &state.user, edit(Edit::User)),
        ui::secret_field(
            "Password (never stored)",
            &state.password,
            edit(Edit::Password),
            Some(Msg::LoginJellyfin)
        ),
        ui::secret_field("or API key", &state.api_key, edit(Edit::ApiKey), None),
        ui::primary("Log in", Msg::LoginJellyfin),
    ]
    .spacing(12)
}

fn anilist_step<'a>(state: &'a State, auth: &AuthStatus) -> Column<'a, Msg> {
    let edit = |wrap: fn(String) -> Edit| move |v: String| Msg::Edit(wrap(v));
    column![
        ui::muted("Musubi has no built-in AniList app. Create your own once, the client secret is not needed."),
        status(anilist_status(auth)),
        ui::secondary("Open AniList developer page", Msg::OpenDeveloperPage),
        text(format!("Redirect URL: {}", state.redirect)).size(14),
        ui::field("Client ID", &state.client_id, edit(Edit::ClientId)),
        ui::primary("Log in with browser", Msg::LoginAniListBrowser),
        ui::secret_field(
            "Token or redirect URL (without browser login)",
            &state.token,
            edit(Edit::Token),
            Some(Msg::LoginAniListToken)
        ),
        ui::secondary("Use pasted token", Msg::LoginAniListToken),
    ]
    .spacing(12)
}

fn sync_step<'a>(state: &'a State, auth: &AuthStatus) -> Column<'a, Msg> {
    column![
        ui::muted("You can preview and apply changes on the Sync page at any time."),
        row![
            text("Jellyfin").size(14).width(70).color(theme::MUTED),
            text(jellyfin_status(auth)).size(14)
        ],
        row![
            text("AniList").size(14).width(70).color(theme::MUTED),
            text(anilist_status(auth)).size(14)
        ],
        toggler(state.auto_sync)
            .label("Update AniList automatically while watching")
            .text_size(14)
            .on_toggle(|v| Msg::Edit(Edit::AutoSync(v))),
        ui::muted("When off, AniList only changes when you apply a sync."),
    ]
    .spacing(12)
}

/// Legt die Einrichtung über die App.
pub fn with_modal<'a>(
    app: &'a App,
    state: &'a State,
    base: Element<'a, Message>,
) -> Element<'a, Message> {
    let auth = &app.auth;
    let (name, mut body) = match state.step {
        Step::Jellyfin => ("Jellyfin", jellyfin_step(state, auth)),
        Step::AniList => ("AniList", anilist_step(state, auth)),
        Step::Sync => ("Sync", sync_step(state, auth)),
    };
    if let Some((message, error)) = &state.message {
        body = body.push(if *error {
            ui::error_line(message.as_str())
        } else {
            ui::muted(message.as_str())
        });
    }
    let logged_in = match state.step {
        Step::Jellyfin => auth.jellyfin_user.is_some(),
        Step::AniList => auth.anilist_logged_in,
        Step::Sync => true,
    };
    let next_label = match (state.step, logged_in) {
        (Step::Sync, _) => "Finish",
        (_, true) => "Continue",
        (_, false) => "Skip",
    };
    let next = if logged_in {
        ui::primary(next_label, Msg::Next)
    } else {
        ui::secondary(next_label, Msg::Next)
    };
    let mut actions = row![ui::secondary("Close", Msg::Close), ui::spacer()]
        .spacing(8)
        .align_y(Center);
    if state.step != Step::Jellyfin {
        actions = actions.push(ui::secondary("Back", Msg::Back));
    }
    let body = column![
        ui::muted(format!("Step {} of 3: {name}", state.step.number())),
        body,
        actions.push(next),
    ]
    .spacing(16);
    ui::modal(
        base,
        "Set up Musubi",
        600.0,
        Element::from(body).map(Message::Setup),
    )
}
