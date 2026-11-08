//! Abgleich zwischen Jellyfin und AniList mit Vorschau.

use std::fmt;

use iced::widget::{column, container, pick_list, row, scrollable, text};
use iced::{Center, Element, Fill, Task};
use musubi_core::{OutboxItem, SyncDirection, SyncLogEntry, SyncReport, SyncRow};

use crate::app::{App, Message};
use crate::{theme, ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectionOption(SyncDirection);

const DIRECTIONS: [DirectionOption; 3] = [
    DirectionOption(SyncDirection::Push),
    DirectionOption(SyncDirection::Pull),
    DirectionOption(SyncDirection::Skip),
];

impl fmt::Display for DirectionOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            SyncDirection::Push => "To AniList",
            SyncDirection::Pull => "To Jellyfin",
            SyncDirection::Skip => "Skip",
        })
    }
}

#[derive(Default)]
pub struct State {
    rows: Vec<SyncRow>,
    pending: Vec<OutboxItem>,
    log: Vec<SyncLogEntry>,
    busy: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Diff(Result<Vec<SyncRow>, String>),
    Status(Result<(Vec<OutboxItem>, Vec<SyncLogEntry>), String>),
    Direction(usize, DirectionOption),
    Reload,
    Apply,
    Applied(Result<SyncReport, String>),
    Flush,
}

pub fn load_status(app: &App) -> Task<Message> {
    let core = app.core.clone();
    ui::perform(
        async move { Ok((core.pending_writes().await?, core.sync_log(50).await?)) },
        |r| Message::Sync(Msg::Status(r)),
    )
}

pub fn load(app: &mut App) -> Task<Message> {
    let status = load_status(app);
    if app.auth.jellyfin_user.is_none() || !app.auth.anilist_logged_in {
        return status;
    }
    app.sync.busy = true;
    let core = app.core.clone();
    let diff = ui::perform(async move { core.sync_diff().await }, |r| {
        Message::Sync(Msg::Diff(r))
    });
    Task::batch([diff, status])
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    match msg {
        Msg::Diff(result) => {
            app.sync.busy = false;
            match result {
                Ok(rows) => app.sync.rows = rows,
                Err(error) => app.toast(error, false),
            }
        }
        Msg::Status(Ok((pending, log))) => {
            app.sync.pending = pending;
            app.sync.log = log;
        }
        Msg::Status(Err(error)) => app.toast(error, false),
        Msg::Direction(index, DirectionOption(direction)) => {
            if let Some(row) = app.sync.rows.get_mut(index) {
                row.direction = direction;
            }
        }
        Msg::Reload => return load(app),
        Msg::Apply => {
            let chosen: Vec<SyncRow> = app
                .sync
                .rows
                .iter()
                .filter(|r| r.direction != SyncDirection::Skip)
                .cloned()
                .collect();
            if chosen.is_empty() {
                return Task::none();
            }
            app.sync.busy = true;
            let core = app.core.clone();
            return ui::perform(async move { core.apply_sync(&chosen).await }, |r| {
                Message::Sync(Msg::Applied(r))
            });
        }
        Msg::Applied(Ok(report)) => {
            app.toast(
                format!(
                    "{} pushed, {} episodes marked, {} conflicts, {} failed",
                    report.pushed, report.pulled, report.conflicts, report.failed
                ),
                report.failed == 0,
            );
            return load(app);
        }
        Msg::Applied(Err(error)) => {
            app.sync.busy = false;
            app.toast(error, false);
        }
        Msg::Flush => {
            let core = app.core.clone();
            return ui::perform(async move { core.flush_outbox().await }, Message::Notify);
        }
    }
    Task::none()
}

fn diff_row(index: usize, row_data: &SyncRow) -> Element<'_, Msg> {
    row![
        column![
            text(ui::ellipsize(&row_data.media_title, 70)).size(15),
            ui::muted(format!(
                "Jellyfin {}  ·  AniList {}/{}",
                row_data.jellyfin_progress,
                row_data.anilist_progress,
                row_data.total.map_or("?".to_owned(), |t| t.to_string())
            )),
        ]
        .spacing(2)
        .width(Fill),
        pick_list(
            DIRECTIONS,
            Some(DirectionOption(row_data.direction)),
            move |d| Msg::Direction(index, d)
        )
        .text_size(13)
        .padding([6, 10]),
    ]
    .align_y(Center)
    .into()
}

fn when(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|t| t.with_timezone(&chrono::Local).format("%F %R").to_string())
        .unwrap_or_default()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let state = &app.sync;
    let diff: Vec<Element<'_, Msg>> = state
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| diff_row(i, r))
        .collect();
    let pending = state.pending.iter().map(|item| {
        let details = format!(
            "{} attempts{}",
            item.attempts,
            item.last_error
                .as_deref()
                .map(|e| format!(": {e}"))
                .unwrap_or_default()
        );
        column![
            text(format!("Pending write for media {}", item.media_id)).size(14),
            ui::muted(details),
        ]
        .spacing(2)
        .into()
    });
    let log = state.log.iter().map(|entry| {
        column![
            text(ui::ellipsize(&entry.message, 110))
                .size(14)
                .color(if entry.ok { theme::TEXT } else { theme::RED }),
            ui::muted(when(entry.at)),
        ]
        .spacing(2)
        .into()
    });
    let summary: Element<'_, Msg> = if diff.is_empty() {
        ui::muted(if state.busy {
            "Comparing…"
        } else {
            "Jellyfin and AniList agree."
        })
    } else {
        column(diff).spacing(10).into()
    };
    let body = column![
        row![
            ui::heading("Sync"),
            ui::spacer(),
            ui::secondary("Retry queue", Msg::Flush),
            ui::secondary("Compare again", Msg::Reload),
            ui::primary("Apply", Msg::Apply),
        ]
        .spacing(8)
        .align_y(Center),
        ui::card(summary),
        text("Queue and history").size(16),
        ui::card(column(pending.chain(log)).spacing(10)),
    ]
    .spacing(18)
    .padding(iced::padding::right(14));
    let page: Element<'_, Msg> = container(scrollable(body).width(Fill).height(Fill))
        .padding(24)
        .into();
    page.map(Message::Sync)
}
