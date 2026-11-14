//! Logins and settings.

use iced::widget::{column, container, row, slider, text};
use iced::{Center, Element, Fill, Task};
use musubi_core::Config;

use crate::anilist_login::{self, Outcome};
use crate::app::{App, COMMIT, Message, VERSION};
use crate::{logo, setup, theme, ui, updates};

#[derive(Default)]
pub struct State {
    url: String,
    user: String,
    password: String,
    api_key: String,
    anilist: anilist_login::State,
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

    /// Takes over the config. Passwords that were not saved are not kept.
    pub fn reload(&mut self, config: &Config) {
        self.url = config.jellyfin.url.clone().unwrap_or_default();
        self.password.clear();
        self.api_key.clear();
        self.anilist.reload(config);
        self.auto_sync = config.sync.auto_sync;
        self.threshold = config.sync.watched_threshold * 100.0;
        self.autoplay = config.player.autoplay_next;
    }

    fn apply(&self, config: &mut Config) {
        config.sync.auto_sync = self.auto_sync;
        config.sync.watched_threshold = self.threshold / 100.0;
        config.player.autoplay_next = self.autoplay;
        config.anilist.client_id = self.anilist.own_client_id();
    }
}

#[derive(Debug, Clone)]
pub enum Edit {
    Url(String),
    User(String),
    Password(String),
    ApiKey(String),
    AutoSync(bool),
    Threshold(f64),
    Autoplay(bool),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Edit(Edit),
    LoginJellyfin,
    AniList(anilist_login::Msg),
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
        Msg::AniList(msg) => {
            let (task, outcome) = anilist_login::update(&mut app.settings.anilist, &core, msg);
            let task = task.map(|m| Message::Settings(Msg::AniList(m)));
            return match outcome {
                Outcome::Nothing => task,
                Outcome::LoggedIn(name) => Task::batch([task, Task::done(logged_in(Ok(name)))]),
                Outcome::Failed(error) => {
                    app.toast(error, false);
                    task
                }
            };
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

/// Head of a section: the name on the left, the connection state on the right in the
/// cord colour.
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
            if connected { cord } else { theme::TEXT_2 }
        ),
    ]
    .spacing(theme::S3)
    .align_y(Center)
    .into()
}

fn half<'a>(field: Element<'a, Msg>) -> Element<'a, Msg> {
    container(field).width(Fill).into()
}

/// Version and git commit of this build, so bug reports are unambiguous, and the update
/// button next to them.
fn about(app: &App) -> Element<'_, Message> {
    let fact = |name: &'static str, value: &'static str| {
        row![
            ui::muted(name),
            text(value)
                .size(theme::T_SMALL)
                .font(theme::MONO)
                .color(theme::TEXT),
        ]
        .spacing(6)
        .align_y(Center)
    };
    let line = row![
        logo::mark(36.0),
        column![
            text("Musubi").size(theme::T_BODY).font(theme::DISPLAY),
            ui::muted("Watch on Jellyfin, track on AniList."),
        ]
        .spacing(1),
        ui::spacer(),
        fact("Version", VERSION),
        fact("Commit", COMMIT),
        updates::action(&app.updates),
    ]
    .spacing(theme::S4)
    .align_y(Center);
    match updates::hint(&app.updates) {
        Some(hint) => column![line, row![ui::spacer(), hint]]
            .spacing(theme::S2)
            .into(),
        None => line.into(),
    }
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
            ui::primary("Log in to Jellyfin", Msg::LoginJellyfin),
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
        anilist_login::view(&s.anilist, app.auth.anilist_logged_in).map(Msg::AniList),
        row![ui::quiet("Log out", Msg::LogoutAniList)],
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
                    .color(theme::TEXT),
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
    let header: Element<'_, Msg> = ui::page_header(
        "Settings",
        None,
        vec![ui::secondary("Run setup again", Msg::RunSetup)],
    );
    let cards = [
        header,
        ui::card(jellyfin),
        ui::card(anilist),
        ui::card(preferences),
        ui::divider(),
    ]
    .into_iter()
    .map(|part| part.map(Message::Settings));
    let body = column(cards)
        .push(about(app))
        .spacing(theme::S4)
        .max_width(820)
        .padding(iced::Padding {
            top: theme::S5,
            right: ui::PAGE_PADDING,
            bottom: theme::S6,
            left: ui::PAGE_PADDING,
        });
    ui::scroll(body)
}
