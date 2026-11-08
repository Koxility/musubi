//! Leiste für die laufende Wiedergabe in mpv.

use iced::widget::{Space, button, column, container, row, slider, text};
use iced::{Center, Element, Fill};
use musubi_core::PlayerCommand;

use crate::app::{App, Message};
use crate::{theme, ui};

fn control<'a>(label: &'a str, command: PlayerCommand) -> Element<'a, Message> {
    button(text(label).size(14))
        .padding([6, 10])
        .style(theme::secondary)
        .on_press(Message::Player(command))
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let Some(state) = &app.playback else {
        return Space::new().height(0).into();
    };
    let duration = state.duration.unwrap_or(0.0).max(1.0);
    let position = app.seek_preview.unwrap_or(state.position).min(duration);
    let time = format!(
        "{} / {}",
        ui::format_time(position),
        state.duration.map_or("?".to_owned(), ui::format_time)
    );
    container(
        row![
            control("⏮", PlayerCommand::Previous),
            control("-10", PlayerCommand::SeekRelative(-10.0)),
            control(
                if state.paused { "▶" } else { "⏸" },
                PlayerCommand::TogglePause
            ),
            control("+10", PlayerCommand::SeekRelative(10.0)),
            control("⏭", PlayerCommand::Next),
            control("■", PlayerCommand::Stop),
            column![
                text(ui::ellipsize(&state.title, 90)).size(14),
                slider(0.0..=duration, position, Message::SeekPreview)
                    .on_release(Message::SeekCommit),
            ]
            .spacing(4)
            .width(Fill),
            text(time).size(13).color(theme::MUTED),
            control("Subs", PlayerCommand::CycleSubtitle),
            control("Audio", PlayerCommand::CycleAudio),
        ]
        .spacing(8)
        .align_y(Center),
    )
    .padding([10, 16])
    .width(Fill)
    .style(theme::rail)
    .into()
}
