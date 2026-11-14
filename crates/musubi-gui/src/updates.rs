//! The update check and where it shows: a small pill under the version in the sidebar, a
//! button in Settings, and a dialog when the update has to be done by hand.

use iced::widget::{button, column, row, text, text_input};
use iced::{Center, Element, Task};
use musubi_core::{InstallKind, Update, UpdateOutcome, Version};

use crate::app::{App, Message, Page};
use crate::{theme, ui};

#[derive(Default)]
pub struct State {
    status: Status,
    manual: Option<Manual>,
}

#[derive(Default)]
enum Status {
    /// Not known yet, or the check failed. Shown like "up to date": nothing to offer.
    #[default]
    Unknown,
    UpToDate,
    Available(Update),
    Installing(Update),
    /// In place, running after a restart.
    Installed,
}

/// What the dialog for an update by hand shows.
struct Manual {
    version: Version,
    command: Option<String>,
    page: String,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Checked(Result<Option<Update>, String>),
    Install,
    Finished(Result<UpdateOutcome, String>),
    OpenPage,
    CopyCommand,
    CloseDialog,
}

impl State {
    /// Closes the dialog; `false` if none was open.
    pub fn close_modal(&mut self) -> bool {
        self.manual.take().is_some()
    }
}

/// Asks for a newer release, once, at startup.
pub fn check(app: &App) -> Task<Message> {
    let core = app.core.clone();
    ui::perform(async move { core.check_for_update().await }, |result| {
        Message::Update(Msg::Checked(result))
    })
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    match msg {
        Msg::Checked(Ok(Some(update))) => app.updates.status = Status::Available(update),
        Msg::Checked(Ok(None)) => app.updates.status = Status::UpToDate,
        // Being offline is no reason for a notice; the next start asks again.
        Msg::Checked(Err(error)) => tracing::debug!(%error, "the update check failed"),
        Msg::Install => {
            let Status::Available(update) = std::mem::take(&mut app.updates.status) else {
                return Task::none();
            };
            let (core, job) = (app.core.clone(), update.clone());
            app.updates.status = Status::Installing(update);
            return ui::perform(async move { core.install_update(&job).await }, |result| {
                Message::Update(Msg::Finished(result))
            });
        }
        Msg::Finished(result) => {
            let Status::Installing(update) = std::mem::take(&mut app.updates.status) else {
                return Task::none();
            };
            let version = update.version;
            app.updates.status = Status::Available(update);
            match result {
                Ok(UpdateOutcome::Installed) => {
                    app.updates.status = Status::Installed;
                    app.toast(
                        format!("Musubi {version} is installed. Restart Musubi to use it."),
                        true,
                    );
                }
                Ok(UpdateOutcome::Started) => app.toast(
                    "The update continues in the terminal window that just opened.",
                    true,
                ),
                Ok(UpdateOutcome::Manual { command, page }) => {
                    app.updates.manual = Some(Manual {
                        version,
                        command,
                        page,
                    });
                }
                Err(error) => app.toast(error, false),
            }
        }
        Msg::OpenPage => {
            if let Some(manual) = &app.updates.manual
                && let Err(error) = open::that_detached(&manual.page)
            {
                app.toast(format!("Could not open the browser: {error}"), false);
            }
        }
        Msg::CopyCommand => {
            if let Some(command) = app.updates.manual.as_ref().and_then(|m| m.command.clone()) {
                app.toast("Command copied", true);
                return iced::clipboard::write(command);
            }
        }
        Msg::CloseDialog => app.updates.manual = None,
    }
    Task::none()
}

/// The pill for the sidebar, only while an update waits to be installed.
pub fn badge(state: &State) -> Option<Element<'_, Message>> {
    let (Status::Available(update) | Status::Installing(update)) = &state.status else {
        return None;
    };
    let pill = button(
        text("Update")
            .size(theme::T_CAPTION)
            .font(theme::SEMIBOLD)
            .line_height(1.45),
    )
    .padding([1.0, 7.0])
    .style(theme::update_badge)
    .on_press(Message::Navigate(Page::Settings));
    Some(ui::with_tip(
        pill,
        format!("Musubi {} is available", update.version),
    ))
}

/// The button for Settings: greyed out unless there is an update to install.
pub fn action(state: &State) -> Element<'_, Message> {
    let (label, message) = match &state.status {
        Status::Unknown | Status::UpToDate => ("Up to date".to_owned(), None),
        Status::Available(update) => (
            format!("Update to {}", update.version),
            Some(Message::Update(Msg::Install)),
        ),
        Status::Installing(_) => ("Updating…".to_owned(), None),
        Status::Installed => ("Restart to finish".to_owned(), None),
    };
    button(text(label).size(theme::T_SMALL).font(theme::MEDIUM))
        .padding([theme::S2, 14.0])
        .style(theme::info)
        .on_press_maybe(message)
        .into()
}

/// One line under the button's row that says how this copy gets its update.
pub fn hint(state: &State) -> Option<Element<'_, Message>> {
    let Status::Available(update) = &state.status else {
        return None;
    };
    Some(ui::muted(match &update.install {
        InstallKind::Script { .. } => {
            "Downloads the release, checks it and replaces this installation.".to_owned()
        }
        InstallKind::Pacman { package } => {
            format!("Opens a terminal and rebuilds the {package} package with makepkg.")
        }
        InstallKind::Other => "Shows how to update this copy by hand.".to_owned(),
    }))
}

pub fn with_modal<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let Some(manual) = &app.updates.manual else {
        return base;
    };
    let mut body = column![].spacing(theme::S4);
    body = match &manual.command {
        Some(command) => body
            .push(ui::muted(
                "No terminal could be opened. The package is prepared; run this command to \
                 build and install it:",
            ))
            .push(
                row![
                    text_input("", command)
                        .font(theme::MONO)
                        .size(theme::T_SMALL)
                        .padding([10, 12])
                        .style(theme::input),
                    ui::secondary("Copy", Message::Update(Msg::CopyCommand)),
                ]
                .spacing(theme::S2)
                .align_y(Center),
            ),
        None => body.push(ui::muted(
            "This copy was not installed by pacman or by install.sh, so Musubi cannot replace \
             it itself. The release page has the download and the steps for every way to \
             install.",
        )),
    };
    body = body.push(
        row![
            ui::primary("Open release page", Message::Update(Msg::OpenPage)),
            ui::secondary("Close", Message::Update(Msg::CloseDialog)),
        ]
        .spacing(theme::S2),
    );
    ui::modal(
        base,
        format!("Update to Musubi {}", manual.version),
        520.0,
        body.into(),
    )
}
