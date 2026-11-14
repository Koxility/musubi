//! The AniList login, shared by the setup wizard and the Settings page.
//!
//! The usual way is one button: the browser opens, the user approves Musubi and AniList
//! issues a token that is personal to them. A user's own API client and a pasted token are
//! still there, behind the advanced options.

use iced::widget::{column, container, row, text};
use iced::{Center, Element, Fill, Task};
use musubi_core::{Config, Core, oauth};

use crate::icons::Icon;
use crate::{theme, ui};

const DEVELOPER_URL: &str = "https://anilist.co/settings/developer";

#[derive(Default)]
pub struct State {
    client_id: String,
    token: String,
    redirect: String,
    advanced: bool,
    /// The running browser login. Dropping the handle ends it and frees the loopback port.
    waiting: Option<iced::task::Handle>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Start,
    Cancel,
    Advanced(bool),
    ClientId(String),
    Token(String),
    UseToken,
    OpenDeveloperPage,
    Finished(Result<String, String>),
}

/// What the page around the form has to react to.
pub enum Outcome {
    Nothing,
    /// Logged in as this user.
    LoggedIn(String),
    Failed(String),
}

impl State {
    pub fn new(config: &Config) -> Self {
        let mut state = Self::default();
        state.reload(config);
        state
    }

    /// Takes over the config. A pasted token is never kept.
    pub fn reload(&mut self, config: &Config) {
        self.client_id = config.anilist.client_id.clone().unwrap_or_default();
        self.redirect = oauth::loopback_redirect(config.anilist.loopback_port);
        self.token.clear();
    }

    /// The client ID of the user's own app, if they entered one.
    pub fn own_client_id(&self) -> Option<String> {
        let id = self.client_id.trim();
        (!id.is_empty()).then(|| id.to_owned())
    }
}

pub fn update(state: &mut State, core: &Core, msg: Msg) -> (Task<Msg>, Outcome) {
    match msg {
        Msg::Start => {
            let own = state.own_client_id();
            if let Err(error) = core.update_config(|c| c.anilist.client_id = own) {
                return (Task::none(), Outcome::Failed(error.to_string()));
            }
            let core = core.clone();
            let (task, handle) = Task::perform(
                async move {
                    core.login_anilist_loopback()
                        .await
                        .map(|viewer| viewer.name)
                        .map_err(|e| e.to_string())
                },
                Msg::Finished,
            )
            .abortable();
            // Starting again replaces the handle, which ends the earlier attempt first.
            state.waiting = Some(handle.abort_on_drop());
            return (task, Outcome::Nothing);
        }
        Msg::Cancel => state.waiting = None,
        Msg::Advanced(open) => state.advanced = open,
        Msg::ClientId(value) => state.client_id = value,
        Msg::Token(value) => state.token = value,
        Msg::UseToken => {
            let token = std::mem::take(&mut state.token);
            let core = core.clone();
            let task = ui::perform(
                async move { Ok(core.login_anilist_pin(&token).await?.name) },
                Msg::Finished,
            );
            return (task, Outcome::Nothing);
        }
        Msg::OpenDeveloperPage => {
            if open::that_detached(DEVELOPER_URL).is_err() {
                return (
                    Task::none(),
                    Outcome::Failed(format!("Could not open a browser, visit {DEVELOPER_URL}")),
                );
            }
        }
        Msg::Finished(result) => {
            state.waiting = None;
            let outcome = match result {
                Ok(name) => Outcome::LoggedIn(name),
                Err(error) => Outcome::Failed(error),
            };
            return (Task::none(), outcome);
        }
    }
    (Task::none(), Outcome::Nothing)
}

fn advanced(state: &State) -> Element<'_, Msg> {
    column![
        ui::muted(
            "Musubi logs in through its own AniList API client. To use yours instead, create \
             one on the developer page with the redirect URL below and enter its client ID. \
             The client secret is not needed. Leave the field empty to go back to Musubi's."
        ),
        row![ui::secondary(
            "Open the AniList developer page",
            Msg::OpenDeveloperPage
        )],
        column![
            ui::label("Redirect URL for that client"),
            container(
                text(&state.redirect)
                    .size(theme::T_SMALL)
                    .color(theme::TEXT_2)
            )
            .padding([9, 10])
            .width(Fill)
            .style(theme::inset),
        ]
        .spacing(6),
        ui::field("Client ID", &state.client_id, Msg::ClientId),
        ui::secret_field(
            "Token or redirect URL, if the browser cannot reach this computer",
            &state.token,
            Msg::Token,
            Some(Msg::UseToken)
        ),
        row![ui::secondary("Use pasted token", Msg::UseToken)],
    ]
    .spacing(theme::S3)
    .into()
}

/// The login form: one button, and the advanced options folded away below it.
pub fn view(state: &State, logged_in: bool) -> Element<'_, Msg> {
    let hint = "Musubi opens AniList in your browser. Approve it there and you are logged in \
                with your own personal token. There is nothing to copy or paste.";
    let action: Element<'_, Msg> = if state.waiting.is_some() {
        row![
            ui::icon(Icon::Clock, 16.0, theme::TRACK),
            text("Waiting for you to approve Musubi in the browser…")
                .size(theme::T_SMALL)
                .color(theme::TEXT),
            ui::secondary("Cancel", Msg::Cancel),
        ]
        .spacing(theme::S2)
        .align_y(Center)
        .into()
    } else {
        let label = if logged_in {
            "Log in again with AniList"
        } else {
            "Log in with AniList"
        };
        row![ui::tool(
            Icon::External,
            label,
            Some(Msg::Start),
            theme::primary,
            theme::BG
        )]
        .into()
    };
    let open = state.advanced;
    let mut form = column![
        ui::muted(hint),
        action,
        row![ui::quiet(
            if open {
                "Hide advanced options"
            } else {
                "Use my own API client or a token"
            },
            Msg::Advanced(!open)
        )],
    ]
    .spacing(theme::S3);
    if open {
        form = form.push(advanced(state));
    }
    form.into()
}
