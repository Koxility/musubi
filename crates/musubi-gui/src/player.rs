//! Leiste für die Wiedergabe im externen mpv-Fenster.

use iced::widget::{Space, column, container, row, slider, text};
use iced::{Center, Element, Fill};
use musubi_core::PlayerCommand;

use crate::app::{App, Message};
use crate::icons::Icon;
use crate::{theme, ui};

const SEEK_STEP: f64 = 10.0;

fn control<'a>(
    icon: Icon,
    size: f32,
    command: PlayerCommand,
    label: &'a str,
) -> Element<'a, Message> {
    ui::icon_button(
        icon,
        size,
        theme::TEXT,
        Some(Message::Player(command)),
        label,
        theme::icon_button,
    )
}

pub fn view(app: &App) -> Element<'_, Message> {
    // Der eingebettete Player bringt seine eigene Steuerung mit.
    let Some(state) = app.playback.as_ref().filter(|_| !app.video.is_active()) else {
        return Space::new().height(0).into();
    };
    let duration = state.duration.unwrap_or(0.0).max(1.0);
    let position = app.seek_preview.unwrap_or(state.position).min(duration);
    let time = format!(
        "{} / {}",
        ui::format_time(position),
        state.duration.map_or("–:––".to_owned(), ui::format_time)
    );
    let (series, episode) = ui::split_title(&state.title);
    let mut now = column![
        text(ui::ellipsize(series, 48))
            .size(14)
            .font(ui::SEMIBOLD)
            .color(theme::TEXT)
    ]
    .spacing(2);
    now = now.push(ui::muted(match episode {
        Some(episode) => format!("{}  ·  {time}", ui::ellipsize(&episode, 44)),
        None => time,
    }));
    let play = if state.paused {
        (Icon::Play, "Play (Ctrl+P)")
    } else {
        (Icon::Pause, "Pause (Ctrl+P)")
    };
    let transport = row![
        control(
            Icon::SkipPrevious,
            18.0,
            PlayerCommand::Previous,
            "Previous episode (Ctrl+,)"
        ),
        control(
            Icon::Replay10,
            20.0,
            PlayerCommand::SeekRelative(-SEEK_STEP),
            "Back 10 s (Alt+Left)"
        ),
        ui::icon_button(
            play.0,
            22.0,
            theme::BG,
            Some(Message::Player(PlayerCommand::TogglePause)),
            play.1,
            theme::primary_round,
        ),
        control(
            Icon::Forward10,
            20.0,
            PlayerCommand::SeekRelative(SEEK_STEP),
            "Forward 10 s (Alt+Right)"
        ),
        control(
            Icon::SkipNext,
            18.0,
            PlayerCommand::Next,
            "Next episode (Ctrl+.)"
        ),
    ]
    .spacing(6)
    .align_y(Center);
    let extras = row![
        control(
            Icon::Subtitles,
            19.0,
            PlayerCommand::CycleSubtitle,
            "Subtitles (Ctrl+J)"
        ),
        control(
            Icon::Audio,
            19.0,
            PlayerCommand::CycleAudio,
            "Audio track (Ctrl+K)"
        ),
        control(Icon::Stop, 17.0, PlayerCommand::Stop, "Stop (Ctrl+Shift+S)"),
    ]
    .spacing(4)
    .align_y(Center);
    container(
        column![
            slider(0.0..=duration, position, Message::SeekPreview)
                .on_release(Message::SeekCommit)
                .height(14)
                .style(theme::seek_slider),
            row![
                container(now).width(Fill),
                transport,
                container(extras).width(Fill).align_right(Fill),
            ]
            .spacing(16)
            .align_y(Center),
        ]
        .spacing(4),
    )
    .padding(iced::padding::top(2).left(18).right(18).bottom(10))
    .width(Fill)
    .style(theme::now_playing)
    .into()
}
