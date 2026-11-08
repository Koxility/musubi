//! Zeichnet den Zustand. Enthält keine Logik außer Darstellung.

use musubi_core::domain::{ItemKind, ListStatus, MediaKind};
use musubi_core::{Service, SyncDirection};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Gauge, List, ListItem, ListState, Paragraph, Row, Table,
    TableState, Tabs, Wrap,
};
use ratatui_image::StatefulImage;

use crate::app::{App, InputMode, MappingView, Modal, Tab};
use crate::form::Form;
use crate::keys::Action;

const ACCENT: Color = Color::Cyan;

fn highlight() -> Style {
    Style::new()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD)
}

fn status_color(status: ListStatus) -> Color {
    match status {
        ListStatus::Current => Color::Green,
        ListStatus::Repeating => Color::LightGreen,
        ListStatus::Completed => Color::Blue,
        ListStatus::Paused => Color::Yellow,
        ListStatus::Dropped => Color::Red,
        ListStatus::Planning => Color::Magenta,
    }
}

fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn progress_text(progress: u32, total: Option<u32>) -> String {
    format!(
        "{progress}/{}",
        total.map_or("?".to_owned(), |t| t.to_string())
    )
}

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let bar = if app.playback.is_some() { 2 } else { 0 };
    let [header, body, player, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(bar),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    draw_header(frame, app, header);
    match app.tab {
        Tab::Library if app.series.is_some() => draw_series(frame, app, body),
        Tab::Library => draw_library(frame, app, body),
        Tab::AniList => draw_list(frame, app, body),
        Tab::Mapping => draw_mapping(frame, app, body),
        Tab::Sync => draw_sync(frame, app, body),
    }
    if bar > 0 {
        draw_player(frame, app, player);
    }
    draw_status(frame, app, status);
    draw_modal(frame, app);
}

fn draw_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let right = format!(
        "JF: {}  AL: {} ",
        app.auth_label(Service::Jellyfin),
        app.auth_label(Service::AniList)
    );
    let [tabs_area, auth_area] = Layout::horizontal([
        Constraint::Min(30),
        Constraint::Length(right.chars().count() as u16),
    ])
    .areas(area);
    let titles = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{} {}", i + 1, t.title()));
    let selected = Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .highlight_style(Style::new().fg(ACCENT).bold()),
        tabs_area,
    );
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right).dim(),
        auth_area,
    );
}

/// Teilt die Fläche in Liste und Detailspalte, wenn das Terminal breit genug ist.
fn split_detail(area: Rect) -> (Rect, Option<Rect>) {
    if area.width >= 100 {
        let [list, detail] =
            Layout::horizontal([Constraint::Min(50), Constraint::Length(42)]).areas(area);
        (list, Some(detail))
    } else {
        (area, None)
    }
}

fn draw_detail(
    frame: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    title: String,
    lines: Vec<Line<'static>>,
) {
    let block = Block::new().borders(Borders::LEFT).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text_area = if let Some(poster) = app.poster.as_mut() {
        let [image, text] =
            Layout::vertical([Constraint::Length(16), Constraint::Min(0)]).areas(inner);
        frame.render_stateful_widget(StatefulImage::default(), image, poster);
        text
    } else {
        inner
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text_area);
}

fn draw_library(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let (list_area, detail) = split_detail(area);
    let format = app.score_format();
    let rows: Vec<Row<'_>> = app
        .library_visible
        .iter()
        .map(|i| {
            let item = &app.rows[*i].item;
            let entry = app.entry_for_item(&item.id);
            let status = match entry {
                Some(e) => Cell::from(e.status.label(MediaKind::Anime)).fg(status_color(e.status)),
                None if app.needs_review(&item.id) => Cell::from("review").yellow(),
                None if app.mappings.contains_key(&item.id) => Cell::from("not listed").dim(),
                None => Cell::from("unmapped").dim(),
            };
            Row::new(vec![
                Cell::from(item.name.clone()),
                Cell::from(item.year.map(|y| y.to_string()).unwrap_or_default()),
                Cell::from(if item.kind == ItemKind::Movie {
                    "Movie"
                } else {
                    "TV"
                }),
                status,
                Cell::from(
                    entry
                        .map(|e| progress_text(e.progress, e.media.episodes))
                        .unwrap_or_default(),
                ),
                Cell::from(
                    entry
                        .map(|e| format.display(e.score_raw))
                        .unwrap_or_default(),
                ),
                Cell::from(
                    item.unplayed_count
                        .filter(|c| *c > 0)
                        .map(|c| c.to_string())
                        .unwrap_or_default(),
                ),
            ])
        })
        .collect();
    let title = format!(
        " Library ({}/{}){} ",
        app.library_visible.len(),
        app.rows.len(),
        if app.library_filter.is_empty() {
            String::new()
        } else {
            format!(" /{}", app.library_filter)
        }
    );
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(11),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(4),
        ],
    )
    .header(Row::new(["Title", "Year", "Type", "AniList", "Prog", "Score", "New"]).dim())
    .row_highlight_style(highlight())
    .block(Block::new().title(title));
    let mut state = TableState::new().with_selected(Some(app.library_selected));
    frame.render_stateful_widget(table, list_area, &mut state);

    if let (Some(detail), Some(row)) = (detail, app.selected_row().cloned()) {
        let item = row.item;
        let mut lines = vec![Line::from(item.name.clone()).bold()];
        let mut meta = vec![];
        if let Some(year) = item.year {
            meta.push(year.to_string());
        }
        if let Some(status) = &item.status {
            meta.push(status.clone());
        }
        if let Some(rating) = item.community_rating {
            meta.push(format!("★ {rating:.1}"));
        }
        lines.push(Line::from(meta.join("  ")).dim());
        if !item.genres.is_empty() {
            lines.push(Line::from(item.genres.join(", ")).fg(ACCENT));
        }
        if let Some(mappings) = app.mappings.get(&item.id) {
            for mapping in mappings {
                lines.push(
                    Line::from(format!(
                        "S{} E{}-{} → {}",
                        mapping.season,
                        mapping.ep_start,
                        mapping.ep_end.map_or("…".to_owned(), |e| e.to_string()),
                        mapping.title
                    ))
                    .dim(),
                );
            }
        }
        lines.push(Line::default());
        lines.push(Line::from(item.overview.clone().unwrap_or_default()));
        draw_detail(frame, app, detail, " Details ".to_owned(), lines);
    }
}

fn draw_series(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(series) = &app.series else { return };
    let rows: Vec<Row<'_>> = series
        .episodes
        .iter()
        .map(|row| {
            let e = &row.episode;
            let state = if e.played {
                Cell::from("✓").green()
            } else if e.position_secs > 0 {
                Cell::from("◐").yellow()
            } else {
                Cell::from(" ")
            };
            let anilist = match (row.anilist, row.counted_on_anilist) {
                (Some(_), true) => Cell::from("✓ list").green(),
                (Some((_, ep)), false) => Cell::from(format!("→ {ep}")).dim(),
                (None, _) => Cell::from("-").dim(),
            };
            Row::new(vec![
                state,
                Cell::from(format!("S{:02}E{:02}", e.season_number, e.number)),
                Cell::from(e.name.clone()),
                Cell::from(
                    e.runtime_secs
                        .map(|s| format_time(s as f64))
                        .unwrap_or_default(),
                ),
                anilist,
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(7),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(Row::new(["", "No.", "Title", "Length", "AniList"]).dim())
    .row_highlight_style(highlight())
    .block(Block::new().title(format!(" {} ", series.item.name)));
    let mut state = TableState::new().with_selected(Some(series.selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn draw_list(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let (list_area, detail) = split_detail(area);
    let format = app.score_format();
    let kind = app.kind;
    let entries = app.entries();
    let rows: Vec<Row<'_>> = app
        .list_visible
        .iter()
        .map(|i| {
            let e = &entries[*i];
            let mut title = e.media.title.preferred().to_owned();
            if e.private {
                title.push_str(" 🔒");
            }
            Row::new(vec![
                Cell::from(title),
                Cell::from(e.status.label(kind)).fg(status_color(e.status)),
                Cell::from(progress_text(e.progress, e.media.total_units())),
                Cell::from(format.display(e.score_raw)),
                Cell::from(e.media.format.clone().unwrap_or_default()),
            ])
        })
        .collect();
    let filter = app.status_filter.map_or("All", |s| s.label(kind));
    let title = format!(
        " {} list: {filter} ({}){} ",
        kind.label(),
        app.list_visible.len(),
        if app.list_filter.is_empty() {
            String::new()
        } else {
            format!(" /{}", app.list_filter)
        }
    );
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(11),
            Constraint::Length(9),
            Constraint::Length(6),
            Constraint::Length(8),
        ],
    )
    .header(Row::new(["Title", "Status", "Progress", "Score", "Format"]).dim())
    .row_highlight_style(highlight())
    .block(Block::new().title(title));
    let mut state = TableState::new().with_selected(Some(app.list_selected));
    frame.render_stateful_widget(table, list_area, &mut state);

    if let (Some(detail), Some(e)) = (detail, app.selected_entry().cloned()) {
        let mut lines = vec![Line::from(e.media.title.preferred().to_owned()).bold()];
        if let Some(native) = &e.media.title.native {
            lines.push(Line::from(native.clone()).dim());
        }
        lines.push(
            Line::from(format!(
                "Started {}  Finished {}",
                dash_if_empty(e.started_at.to_string()),
                dash_if_empty(e.completed_at.to_string())
            ))
            .dim(),
        );
        if e.repeat > 0 {
            lines.push(Line::from(format!("Repeated {} times", e.repeat)).dim());
        }
        if !e.custom_lists.is_empty() {
            lines.push(Line::from(e.custom_lists.join(", ")).fg(ACCENT));
        }
        if !e.notes.is_empty() {
            lines.push(Line::from(format!("Notes: {}", e.notes)).italic());
        }
        lines.push(Line::default());
        lines.push(Line::from(e.media.description.clone().unwrap_or_default()));
        draw_detail(frame, app, detail, " Details ".to_owned(), lines);
    }
}

fn dash_if_empty(text: String) -> String {
    if text.is_empty() {
        "-".to_owned()
    } else {
        text
    }
}

fn draw_mapping(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if let Some((done, total)) = app.match_progress {
        let [gauge, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let ratio = if total == 0 {
            0.0
        } else {
            done as f64 / total as f64
        };
        frame.render_widget(
            Gauge::default()
                .ratio(ratio)
                .gauge_style(Style::new().fg(ACCENT))
                .label(format!("Matching {done}/{total}")),
            gauge,
        );
        draw_mapping_lists(frame, app, rest);
    } else {
        draw_mapping_lists(frame, app, area);
    }
}

fn draw_mapping_lists(frame: &mut Frame<'_>, app: &App, area: Rect) {
    match app.mapping_view {
        MappingView::Review => {
            let [list_area, candidates] =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(area);
            let items: Vec<ListItem<'_>> = app
                .reviews
                .iter()
                .map(|r| ListItem::new(r.title.clone()))
                .collect();
            let list = List::new(items)
                .highlight_style(highlight())
                .block(Block::new().title(format!(" Needs review ({}) ", app.reviews.len())));
            let mut state = ListState::default().with_selected(Some(app.mapping_selected));
            frame.render_stateful_widget(list, list_area, &mut state);

            let lines: Vec<Line<'_>> = app
                .reviews
                .get(app.mapping_selected)
                .map(|r| {
                    r.candidates
                        .iter()
                        .map(|c| {
                            Line::from(format!(
                                "{:>7}  {:.2}  {} ({}, {}, {} eps)",
                                c.media_id,
                                c.score,
                                c.title,
                                c.format.as_deref().unwrap_or("?"),
                                c.year.map_or("?".to_owned(), |y| y.to_string()),
                                c.episodes.map_or("?".to_owned(), |e| e.to_string()),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            frame.render_widget(
                Paragraph::new(lines)
                    .block(Block::new().borders(Borders::LEFT).title(" Candidates ")),
                candidates,
            );
        }
        MappingView::Mapped => {
            let rows: Vec<Row<'_>> = app
                .mapping_rows
                .iter()
                .map(|m| {
                    let name = app
                        .rows
                        .iter()
                        .find(|r| r.item.id == m.jellyfin_id)
                        .map_or(m.jellyfin_id.clone(), |r| r.item.name.clone());
                    Row::new(vec![
                        name,
                        format!("S{}", m.season),
                        format!(
                            "{}-{}",
                            m.ep_start,
                            m.ep_end.map_or("…".to_owned(), |e| e.to_string())
                        ),
                        format!("{:+}", m.offset),
                        m.title.clone(),
                        m.source.as_str().to_owned(),
                    ])
                })
                .collect();
            let table = Table::new(
                rows,
                [
                    Constraint::Percentage(35),
                    Constraint::Length(4),
                    Constraint::Length(8),
                    Constraint::Length(5),
                    Constraint::Percentage(40),
                    Constraint::Length(9),
                ],
            )
            .header(Row::new(["Jellyfin", "Sea", "Episodes", "Off", "AniList", "Source"]).dim())
            .row_highlight_style(highlight())
            .block(Block::new().title(format!(" Mappings ({}) ", app.mapping_rows.len())));
            let mut state = TableState::new().with_selected(Some(app.mapping_selected));
            frame.render_stateful_widget(table, area, &mut state);
        }
    }
}

fn draw_sync(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let [diff_area, log_area] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);
    let rows: Vec<Row<'_>> = app
        .diff
        .iter()
        .map(|r| {
            let direction = match r.direction {
                SyncDirection::Push => Cell::from("→ AniList").green(),
                SyncDirection::Pull => Cell::from("→ Jellyfin").blue(),
                SyncDirection::Skip => Cell::from("skip").dim(),
            };
            Row::new(vec![
                Cell::from(r.media_title.clone()),
                Cell::from(r.jellyfin_progress.to_string()),
                Cell::from(progress_text(r.anilist_progress, r.total)),
                direction,
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(11),
        ],
    )
    .header(Row::new(["Title", "Jellyfin", "AniList", "Direction"]).dim())
    .row_highlight_style(highlight())
    .block(Block::new().title(format!(" Differences ({}) ", app.diff.len())));
    let mut state = TableState::new().with_selected(Some(app.diff_selected));
    frame.render_stateful_widget(table, diff_area, &mut state);

    let mut lines: Vec<Line<'_>> = Vec::new();
    for item in &app.pending {
        let text = format!(
            "pending media {}: {} attempts{}",
            item.media_id,
            item.attempts,
            item.last_error
                .as_deref()
                .map(|e| format!(" ({e})"))
                .unwrap_or_default()
        );
        lines.push(Line::from(text).fg(if item.failed {
            Color::Red
        } else {
            Color::Yellow
        }));
    }
    for entry in &app.log {
        let when = chrono_time(entry.at);
        lines.push(
            Line::from(format!("{when}  {}", entry.message)).fg(if entry.ok {
                Color::Gray
            } else {
                Color::Red
            }),
        );
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::new().borders(Borders::TOP).title(" Queue and log ")),
        log_area,
    );
}

fn chrono_time(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn draw_player(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(state) = &app.playback else { return };
    let [title, gauge] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let icon = if state.paused { "⏸" } else { "▶" };
    frame.render_widget(
        Paragraph::new(format!("{icon} {}", state.title)).fg(ACCENT),
        title,
    );
    let ratio = state
        .duration
        .filter(|d| *d > 0.0)
        .map_or(0.0, |d| (state.position / d).clamp(0.0, 1.0));
    let label = format!(
        "{} / {}",
        format_time(state.position),
        state.duration.map_or("?".to_owned(), format_time)
    );
    frame.render_widget(
        Gauge::default()
            .ratio(ratio)
            .label(label)
            .gauge_style(Style::new().fg(ACCENT).bg(Color::Black)),
        gauge,
    );
}

fn hints(app: &App) -> Vec<(Action, &'static str)> {
    use Action::*;
    let mut list = match app.tab {
        Tab::Library if app.series.is_some() => {
            vec![
                (Open, "play"),
                (Continue, "next new"),
                (TogglePlayed, "seen"),
                (Map, "map"),
                (Back, "back"),
            ]
        }
        Tab::Library => vec![
            (Open, "open"),
            (Continue, "continue"),
            (Filter, "filter"),
            (ServerFilter, "filters"),
            (Increment, "+1"),
            (Status, "status"),
            (Edit, "edit"),
            (Map, "map"),
        ],
        Tab::AniList => vec![
            (Filter, "filter"),
            (Toggle, "anime/manga"),
            (NextStatus, "status filter"),
            (Increment, "+1"),
            (Score, "score"),
            (Edit, "edit"),
            (Add, "add"),
            (Delete, "remove"),
        ],
        Tab::Mapping => match app.mapping_view {
            MappingView::Review => {
                vec![(Open, "map"), (MatchAll, "match all"), (Toggle, "mappings")]
            }
            MappingView::Mapped => vec![(Edit, "edit"), (Delete, "unmap"), (Toggle, "review")],
        },
        Tab::Sync => vec![
            (Delete, "direction"),
            (Apply, "apply"),
            (Refresh, "refresh"),
            (Add, "retry queue"),
        ],
    };
    list.extend([(Command, "cmd"), (Help, "help"), (Quit, "quit")]);
    list
}

fn draw_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let [line, keys] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let top = match app.mode {
        InputMode::Filter => Line::from(format!("/{}▏", app.input)),
        InputMode::Command => Line::from(format!(":{}▏", app.input)),
        InputMode::Normal => match (&app.rate_limited, &app.notice) {
            (Some(wait), _) => Line::from(format!(
                "AniList rate limit reached, waiting {} s",
                wait.as_secs()
            ))
            .yellow(),
            (None, Some(notice)) => Line::from(notice.text.clone()).fg(if notice.ok {
                Color::Green
            } else {
                Color::Red
            }),
            _ => Line::default(),
        },
    };
    frame.render_widget(Paragraph::new(top), line);
    let spans: Vec<Span<'_>> = hints(app)
        .into_iter()
        .flat_map(|(action, label)| {
            [
                Span::styled(app.keymap.label(action).to_owned(), Style::new().fg(ACCENT)),
                Span::raw(format!(" {label}  ")),
            ]
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(spans)).dim(), keys);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn draw_form(frame: &mut Frame<'_>, form: &Form) {
    let hint_height = u16::from(form.hint.as_ref().is_some_and(|h| !h.is_empty())) * 2;
    let height = form.fields.len() as u16 + 4 + hint_height + u16::from(form.error.is_some());
    let area = centered(frame.area(), 72, height);
    frame.render_widget(Clear, area);
    let label_width = form
        .fields
        .iter()
        .map(|f| f.label.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line<'_>> = form
        .fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let focused = i == form.focus;
            let marker = if focused { "▸ " } else { "  " };
            let mut value = field.display();
            if focused
                && matches!(
                    field.kind,
                    crate::form::FieldKind::Text | crate::form::FieldKind::Secret
                )
            {
                value.push('▏');
            }
            let style = if focused {
                Style::new().fg(ACCENT)
            } else {
                Style::new()
            };
            Line::from(vec![
                Span::styled(format!("{marker}{:<label_width$}  ", field.label), style),
                Span::raw(value),
            ])
        })
        .collect();
    if let Some(error) = &form.error {
        lines.push(Line::from(error.clone()).red());
    }
    if let Some(hint) = form.hint.as_ref().filter(|h| !h.is_empty()) {
        lines.push(Line::default());
        lines.push(Line::from(hint.clone()).dim());
    }
    let block = Block::bordered()
        .title(format!(" {} ", form.title))
        .title_bottom(" Enter save  Tab next  Esc cancel ");
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

const HELP: &[(&str, &str)] = &[
    ("1-4, Tab", "switch tab"),
    ("j/k, g/G, Ctrl-d/u", "move, jump, page"),
    ("/", "fuzzy filter the list"),
    ("F", "server filters: genre, year, status, type"),
    ("Enter", "open / play / edit"),
    ("c, p", "continue with next unwatched episode"),
    ("w", "toggle watched in Jellyfin"),
    ("+ -", "change AniList progress"),
    ("s S e", "status, score, full editor"),
    ("a d", "add to list, remove from list"),
    ("t, [ ]", "anime/manga, status filter"),
    ("m M", "map title, match everything"),
    ("A", "apply sync (Sync tab, d changes direction)"),
    ("Space, ←/→", "pause, seek 10 s"),
    ("n N X", "next, previous, stop"),
    ("v b", "cycle subtitles, audio"),
    (":status :score :progress", "edit selected entry"),
    (":notes :private on|off", "edit selected entry"),
    (":list-create NAME", "new custom list"),
    (":list-delete NAME", "remove custom list"),
    (":list-rename A -> B", "rename custom list"),
    (":login [anilist]", "log in"),
    (":logout jellyfin|anilist", "log out"),
    (":sync :flush :match", "compare, retry queue, match"),
];

fn draw_modal(frame: &mut Frame<'_>, app: &App) {
    match &app.modal {
        None => {}
        Some(Modal::Help) => {
            let area = centered(frame.area(), 80, HELP.len() as u16 + 3);
            frame.render_widget(Clear, area);
            let lines: Vec<Line<'_>> = HELP
                .iter()
                .map(|(keys, text)| {
                    Line::from(vec![
                        Span::styled(format!("{keys:<28}"), Style::new().fg(ACCENT)),
                        Span::raw(*text),
                    ])
                })
                .collect();
            frame.render_widget(
                Paragraph::new(lines).block(Block::bordered().title(" Help (any key closes) ")),
                area,
            );
        }
        Some(Modal::Confirm { text, .. }) => {
            let area = centered(frame.area(), 60, 5);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(text.clone()),
                    Line::default(),
                    Line::from("y confirm   n cancel").dim(),
                ])
                .wrap(Wrap { trim: true })
                .block(Block::bordered().title(" Confirm ")),
                area,
            );
        }
        Some(Modal::Form(form, _)) => draw_form(frame, form),
        Some(Modal::SearchResults { results, selected }) => {
            let area = centered(frame.area(), 80, 22);
            frame.render_widget(Clear, area);
            let items: Vec<ListItem<'_>> = results
                .iter()
                .map(|m| {
                    ListItem::new(format!(
                        "{}  ({}, {})",
                        m.title.preferred(),
                        m.format.as_deref().unwrap_or("?"),
                        m.season_year.map_or("?".to_owned(), |y| y.to_string())
                    ))
                })
                .collect();
            let list = List::new(items)
                .highlight_style(highlight())
                .block(Block::bordered().title(" Results (Enter adds) "));
            let mut state = ListState::default().with_selected(Some(*selected));
            frame.render_stateful_widget(list, area, &mut state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Msg;
    use crate::keys::Keymap;
    use musubi_core::LibraryRow;
    use musubi_core::domain::{FuzzyDate, LibraryItem, ListEntry, Media, MediaTitle};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::collections::BTreeMap;

    fn render(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content
            .chunks(width as usize)
            .map(|line| line.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app() -> App {
        App::new(Keymap::with_overrides(&BTreeMap::new()), None)
    }

    fn row(id: &str, name: &str) -> LibraryRow {
        LibraryRow {
            item: LibraryItem {
                id: id.into(),
                kind: ItemKind::Series,
                name: name.into(),
                overview: Some("An overview".into()),
                year: Some(2023),
                status: Some("Ended".into()),
                genres: vec!["Fantasy".into()],
                community_rating: Some(8.5),
                runtime_secs: None,
                provider_ids: Default::default(),
                has_poster: false,
                played: false,
                unplayed_count: Some(3),
                position_secs: 0,
            },
            mappings: vec![],
            entry: None,
            needs_review: false,
        }
    }

    #[test]
    fn library_shows_rows_detail_and_hints() {
        let mut app = app();
        app.on_msg(Msg::Library(vec![row("a", "Frieren"), row("b", "Bleach")]));
        let screen = render(&mut app, 120, 24);
        assert!(screen.contains("Library (2/2)"), "{screen}");
        assert!(screen.contains("Frieren"));
        assert!(screen.contains("unmapped"));
        assert!(screen.contains("An overview"));
        assert!(screen.contains("Fantasy"));
        assert!(screen.contains("continue"));
    }

    #[test]
    fn list_shows_status_score_and_progress() {
        let mut app = app();
        app.tab = Tab::AniList;
        app.on_msg(Msg::List(
            MediaKind::Anime,
            vec![ListEntry {
                id: 1,
                media: Media {
                    id: 1,
                    title: MediaTitle {
                        romaji: Some("Frieren".into()),
                        ..MediaTitle::default()
                    },
                    episodes: Some(28),
                    ..Media::default()
                },
                status: ListStatus::Current,
                score_raw: 85,
                progress: 3,
                progress_volumes: 0,
                repeat: 0,
                priority: 0,
                private: false,
                hidden_from_status_lists: false,
                notes: String::new(),
                custom_lists: vec![],
                advanced_scores: vec![],
                started_at: FuzzyDate::default(),
                completed_at: FuzzyDate::default(),
                updated_at: 0,
            }],
        ));
        let screen = render(&mut app, 120, 24);
        assert!(screen.contains("Watching"), "{screen}");
        assert!(screen.contains("3/28"));
        assert!(screen.contains("8.5"));
    }

    #[test]
    fn narrow_terminals_and_modals_render_without_panicking() {
        let mut app = app();
        app.on_msg(Msg::Library(vec![row("a", "Frieren")]));
        for (w, h) in [(20, 5), (40, 10), (80, 24), (200, 60)] {
            app.modal = None;
            render(&mut app, w, h);
            app.open_jellyfin_login();
            render(&mut app, w, h);
            app.modal = Some(Modal::Help);
            render(&mut app, w, h);
        }
    }

    #[test]
    fn secrets_are_masked_in_forms() {
        let mut app = app();
        app.open_jellyfin_login();
        if let Some(Modal::Form(form, _)) = &mut app.modal {
            form.fields[2].value = "hunter2".into();
        }
        let screen = render(&mut app, 100, 30);
        assert!(!screen.contains("hunter2"), "{screen}");
        assert!(screen.contains("•••••••"));
    }

    #[test]
    fn time_formatting() {
        assert_eq!(format_time(65.0), "1:05");
        assert_eq!(format_time(3725.0), "1:02:05");
        assert_eq!(format_time(-5.0), "0:00");
    }
}
