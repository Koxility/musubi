//! Abgleich zwischen Jellyfin und AniList mit Vorschau.

use std::fmt;

use iced::widget::{column, container, pick_list, row, text};
use iced::{Center, Element, Fill, Task};
use musubi_core::{OutboxItem, SyncDirection, SyncLogEntry, SyncReport, SyncRow};

use crate::app::{App, Message};
use crate::icons::Icon;
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

/// Stand eines Dienstes in der Farbe seiner Kordel.
fn side<'a>(name: &'a str, value: String, cord: iced::Color) -> Element<'a, Msg> {
    row![
        container(iced::widget::Space::new())
            .width(6)
            .height(6)
            .style(theme::solid(cord, 3.0)),
        ui::muted(name),
        text(value).size(theme::T_SMALL).font(theme::SEMIBOLD),
    ]
    .spacing(6)
    .align_y(Center)
    .into()
}

fn diff_row(index: usize, row_data: &SyncRow) -> Element<'_, Msg> {
    row![
        column![
            text(ui::ellipsize(&row_data.media_title, 70))
                .size(theme::T_BODY)
                .font(theme::MEDIUM),
            row![
                side(
                    "Jellyfin",
                    row_data.jellyfin_progress.to_string(),
                    theme::WATCH
                ),
                side(
                    "AniList",
                    format!(
                        "{} of {}",
                        row_data.anilist_progress,
                        row_data.total.map_or("?".to_owned(), |t| t.to_string())
                    ),
                    theme::TRACK
                ),
            ]
            .spacing(theme::S4),
        ]
        .spacing(theme::S1)
        .width(Fill),
        pick_list(
            DIRECTIONS,
            Some(DirectionOption(row_data.direction)),
            move |d| Msg::Direction(index, d)
        )
        .text_size(theme::T_SMALL)
        .padding([7, 12])
        .style(theme::pick)
        .menu_style(theme::pick_menu),
    ]
    .spacing(theme::S3)
    .padding([theme::S2, theme::S2])
    .align_y(Center)
    .into()
}

fn when(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|t| t.with_timezone(&chrono::Local).format("%F %R").to_string())
        .unwrap_or_default()
}

fn group<'a>(rows: Vec<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(ui::divided(rows))
        .padding([theme::S1, theme::S2])
        .width(Fill)
        .style(theme::surface)
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let state = &app.sync;
    let diff: Vec<Element<'_, Msg>> = state
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| diff_row(i, r))
        .collect();
    let pending: Vec<Element<'_, Msg>> = state
        .pending
        .iter()
        .map(|item| {
            let details = match (item.attempts, item.last_error.as_deref()) {
                (0, _) => "Waiting to be sent".to_owned(),
                (n, Some(error)) => format!("{n} attempts, last error: {error}"),
                (n, None) => format!("{n} attempts"),
            };
            row![
                ui::icon(Icon::Clock, 16.0, theme::WARN),
                column![
                    text(format!("Change for AniList entry {}", item.media_id))
                        .size(theme::T_SMALL)
                        .font(theme::MEDIUM),
                    ui::muted(details),
                ]
                .spacing(2),
            ]
            .spacing(theme::S3)
            .padding([theme::S2, theme::S2])
            .align_y(Center)
            .into()
        })
        .collect();
    let log: Vec<Element<'_, Msg>> = state
        .log
        .iter()
        .map(|entry| {
            let (glyph, color) = if entry.ok {
                (Icon::Check, theme::OK)
            } else {
                (Icon::Close, theme::BAD)
            };
            row![
                ui::icon(glyph, 14.0, color),
                text(ui::ellipsize(&entry.message, 110))
                    .size(theme::T_SMALL)
                    .color(if entry.ok { theme::PAPER } else { theme::BAD })
                    .width(Fill),
                ui::muted(when(entry.at)),
            ]
            .spacing(theme::S3)
            .padding([7.0, theme::S2])
            .align_y(Center)
            .into()
        })
        .collect();
    let changes = diff.len();
    let summary: Element<'_, Msg> = if !diff.is_empty() {
        group(diff)
    } else if state.busy {
        container(ui::muted("Comparing Jellyfin and AniList…"))
            .padding(theme::S4)
            .width(Fill)
            .style(theme::surface)
            .into()
    } else {
        container(
            row![
                ui::icon(Icon::Knot, 20.0, theme::TRACK),
                text("Jellyfin and AniList agree.").size(theme::T_BODY),
            ]
            .spacing(theme::S3)
            .align_y(Center),
        )
        .padding(theme::S4)
        .width(Fill)
        .style(theme::surface)
        .into()
    };
    let mut body = column![
        ui::page_header(
            "Sync",
            Some(
                "Compare what you watched in Jellyfin with your AniList progress and choose which side wins for each title."
                    .to_owned()
            ),
            vec![
                ui::tool(
                    Icon::Sync,
                    "Compare again",
                    Some(Msg::Reload),
                    theme::secondary,
                    theme::PAPER
                ),
                ui::tool(
                    Icon::Check,
                    "Apply changes",
                    (changes > 0 && !state.busy).then_some(Msg::Apply),
                    theme::track,
                    theme::INK
                ),
            ],
        ),
        ui::section(ui::heading_count("Differences", changes), summary),
    ]
    .spacing(theme::S5)
    .padding(iced::Padding {
        top: theme::S5,
        right: ui::PAGE_PADDING,
        bottom: theme::S6,
        left: ui::PAGE_PADDING,
    });
    if !pending.is_empty() {
        let count = pending.len();
        body = body.push(ui::section(
            row![
                ui::heading_count("Waiting to be sent", count),
                ui::spacer(),
                ui::quiet("Retry now", Msg::Flush)
            ]
            .align_y(Center)
            .into(),
            group(pending),
        ));
    }
    let history: Element<'_, Msg> = if log.is_empty() {
        ui::muted("Nothing synced yet.")
    } else {
        group(log)
    };
    body = body.push(ui::section(ui::heading("History"), history));
    let page: Element<'_, Msg> = ui::scroll(body);
    page.map(Message::Sync)
}
