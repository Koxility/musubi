//! AniList-Profil mit Statistik, angelehnt an die Profilseite von AniList.

use iced::widget::{Row, Space, button, column, container, responsive, row, stack, text};
use iced::{Bottom, Center, Color, Element, Fill, FillPortion, Task};
use musubi_core::domain::{KindStats, ListStatus, MediaKind, Profile, StatBucket, share_of_max};

use crate::app::{App, Message, Page};
use crate::icons::Icon;
use crate::images::ImageKey;
use crate::{anilist, theme, ui};

const BANNER_HEIGHT: f32 = 190.0;
const AVATAR: f32 = 96.0;
/// Höhe der Säulen in den Diagrammen.
const CHART_HEIGHT: f32 = 120.0;

pub struct State {
    profile: Option<Profile>,
    kind: MediaKind,
    loading: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            profile: None,
            kind: MediaKind::Anime,
            loading: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    WantImage(ImageKey),
    /// Die Kopie aus dem Cache, sofort verfügbar.
    Cached(Result<Option<Profile>, String>),
    /// Der frische Stand von AniList.
    Loaded(Result<Profile, String>),
    Kind(MediaKind),
    /// Öffnet die Liste mit diesem Status, dort lassen sich die Einträge bearbeiten.
    OpenStatus(ListStatus),
    OpenSite,
    Refresh,
}

/// Zeigt zuerst die Kopie aus dem Cache und holt dann den aktuellen Stand.
pub fn load(app: &mut App) -> Task<Message> {
    if !app.auth.anilist_logged_in {
        app.profile.profile = None;
        return Task::none();
    }
    app.profile.loading = true;
    let (cached, fresh) = (app.core.clone(), app.core.clone());
    Task::batch([
        ui::perform(async move { cached.cached_profile().await }, |r| {
            Message::Profile(Msg::Cached(r))
        }),
        ui::perform(async move { fresh.refresh_profile().await }, |r| {
            Message::Profile(Msg::Loaded(r))
        }),
    ])
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    match msg {
        Msg::WantImage(key) => return app.want_image(key),
        // Kommt der frische Stand zuerst an, bleibt er stehen.
        Msg::Cached(Ok(Some(profile))) if app.profile.profile.is_none() => {
            app.profile.profile = Some(profile);
        }
        Msg::Cached(_) => {}
        Msg::Loaded(Ok(profile)) => {
            app.profile.loading = false;
            app.profile.profile = Some(profile);
        }
        Msg::Loaded(Err(error)) => {
            app.profile.loading = false;
            app.toast(error, false);
        }
        Msg::Kind(kind) => app.profile.kind = kind,
        Msg::OpenStatus(status) => {
            anilist::show_status(app, app.profile.kind, status);
            return Task::done(Message::Navigate(Page::AniList));
        }
        Msg::OpenSite => {
            let url = app
                .profile
                .profile
                .as_ref()
                .and_then(|p| p.site_url.clone());
            if let Some(url) = url
                && open::that_detached(&url).is_err()
            {
                app.toast(format!("Could not open a browser, visit {url}"), false);
            }
        }
        Msg::Refresh => return load(app),
    }
    Task::none()
}

fn want(key: ImageKey) -> Msg {
    Msg::WantImage(key)
}

/// Alter des Abrufs in Worten, z. B. `Updated 5 minutes ago`.
fn age(now: i64, fetched_at: i64) -> String {
    let minutes = (now - fetched_at).max(0) / 60;
    match minutes {
        0 => "Updated just now".to_owned(),
        1 => "Updated a minute ago".to_owned(),
        2..60 => format!("Updated {minutes} minutes ago"),
        60..120 => "Updated an hour ago".to_owned(),
        120..2_880 => format!("Updated {} hours ago", minutes / 60),
        _ => format!("Updated {} days ago", minutes / 1_440),
    }
}

/// Macht aus AniList-Schlüsseln wie `TV_SHORT` lesbare Namen.
fn readable(key: &str) -> String {
    // Genres kommen schon lesbar an, nur Formate sind Konstanten in Großbuchstaben.
    if key.chars().any(char::is_lowercase) {
        return key.to_owned();
    }
    key.split('_')
        .map(|word| match word {
            "TV" | "OVA" | "ONA" => word.to_owned(),
            _ => {
                let mut chars = word.chars();
                chars.next().map_or(String::new(), |first| {
                    first
                        .to_uppercase()
                        .chain(chars.flat_map(char::to_lowercase))
                        .collect()
                })
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// Große Zahlen mit Tausendertrennung, z. B. `2,741`.
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn header<'a>(app: &'a App, profile: &'a Profile) -> Element<'a, Msg> {
    let banner = ui::picture(
        &app.images,
        profile.banner_url.clone().map(ImageKey::Cover),
        Fill,
        BANNER_HEIGHT,
        want,
    );
    let avatar = container(ui::picture(
        &app.images,
        profile.avatar_url.clone().map(ImageKey::Cover),
        AVATAR,
        AVATAR,
        want,
    ))
    .padding(3)
    .style(theme::solid(theme::BG, theme::RADIUS));
    let status = if app.profile.loading {
        "Updating…".to_owned()
    } else {
        age(app.now, profile.fetched_at)
    };
    let identity = row![
        avatar,
        column![
            text(&profile.name).size(theme::T_PAGE).font(theme::DISPLAY),
            ui::muted(status),
        ]
        .spacing(2),
        ui::spacer(),
        ui::tool(
            Icon::Sync,
            "Refresh",
            (!app.profile.loading).then_some(Msg::Refresh),
            theme::secondary,
            theme::TEXT
        ),
        ui::tool(
            Icon::External,
            "Open on AniList",
            profile.site_url.is_some().then_some(Msg::OpenSite),
            theme::secondary,
            theme::TEXT
        ),
    ]
    .spacing(theme::S3)
    .align_y(Bottom);
    stack![
        banner,
        container(Space::new())
            .width(Fill)
            .height(BANNER_HEIGHT)
            .style(theme::hero_fade),
        container(identity)
            .height(BANNER_HEIGHT)
            .align_y(Bottom)
            .padding(iced::Padding {
                top: 0.0,
                right: ui::PAGE_PADDING,
                bottom: theme::S3,
                left: ui::PAGE_PADDING,
            }),
    ]
    .height(BANNER_HEIGHT)
    .into()
}

fn numbers<'a>(stats: &KindStats, kind: MediaKind) -> Element<'a, Msg> {
    let mean = if stats.mean_score > 0.0 {
        format!("{:.1}", stats.mean_score)
    } else {
        "–".to_owned()
    };
    let deviation = format!("{:.1}", stats.standard_deviation);
    let tiles: Vec<Element<'a, Msg>> = match kind {
        MediaKind::Anime => vec![
            ui::stat(grouped(u64::from(stats.count)), "Total anime", theme::TRACK),
            ui::stat(
                format!("{:.1}", stats.days_watched()),
                "Days watched",
                theme::TEXT,
            ),
            ui::stat(
                grouped(u64::from(stats.episodes_watched)),
                "Episodes watched",
                theme::TEXT,
            ),
            ui::stat(mean, "Mean score", theme::TEXT),
            ui::stat(deviation, "Standard deviation", theme::TEXT),
        ],
        MediaKind::Manga => vec![
            ui::stat(grouped(u64::from(stats.count)), "Total manga", theme::TRACK),
            ui::stat(
                grouped(u64::from(stats.chapters_read)),
                "Chapters read",
                theme::TEXT,
            ),
            ui::stat(
                grouped(u64::from(stats.volumes_read)),
                "Volumes read",
                theme::TEXT,
            ),
            ui::stat(mean, "Mean score", theme::TEXT),
            ui::stat(deviation, "Standard deviation", theme::TEXT),
        ],
    };
    row(tiles).spacing(theme::S3).into()
}

fn panel<'a>(title: &'a str, content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(column![ui::heading(title), content.into()].spacing(theme::S4))
        .padding(theme::S4)
        .width(Fill)
        .style(theme::surface)
        .into()
}

/// Verteilung über die Status als ein Balken, darunter die Legende zum Anklicken.
fn statuses<'a>(stats: &KindStats, kind: MediaKind) -> Element<'a, Msg> {
    let counted: Vec<(ListStatus, u32)> = ListStatus::ALL
        .iter()
        .map(|status| (*status, stats.status_count(*status)))
        .collect();
    let total: u32 = counted.iter().map(|(_, count)| count).sum();
    if total == 0 {
        return panel("By status", ui::muted("No entries yet."));
    }
    let mut bar = Row::new().height(10).spacing(2);
    for (status, count) in counted.iter().filter(|(_, count)| *count > 0) {
        bar = bar.push(
            container(Space::new())
                .width(FillPortion((*count).min(u32::from(u16::MAX)) as u16))
                .height(10)
                .style(theme::solid(theme::status_color(*status), 2.0)),
        );
    }
    let legend = counted.into_iter().map(|(status, count)| {
        button(
            row![
                container(Space::new())
                    .width(8)
                    .height(8)
                    .style(theme::solid(theme::status_color(status), 4.0)),
                column![
                    text(grouped(u64::from(count)))
                        .size(theme::T_BODY)
                        .font(theme::SEMIBOLD),
                    ui::muted(status.label(kind)),
                ]
                .spacing(1),
            ]
            .spacing(theme::S2)
            .align_y(Center),
        )
        .padding([6.0, theme::S3])
        .style(theme::row_button)
        .on_press(Msg::OpenStatus(status))
        .into()
    });
    panel(
        "By status",
        column![
            bar,
            row(legend)
                .spacing(theme::S2)
                .wrap()
                .vertical_spacing(theme::S1),
            ui::muted("Pick a status to open that list and edit its entries."),
        ]
        .spacing(theme::S3),
    )
}

/// Säulendiagramm: Wert über der Säule, Beschriftung darunter.
fn columns<'a>(data: Vec<(String, u32)>, color: Color, labels: bool) -> Element<'a, Msg> {
    let max = data.iter().map(|(_, value)| *value).max().unwrap_or(0);
    let bars = data.into_iter().map(|(label, value)| {
        let height =
            (share_of_max(value, max) * CHART_HEIGHT).max(if value > 0 { 2.0 } else { 0.0 });
        let mut column = column![].align_x(Center).spacing(4).width(Fill);
        if labels {
            column = column.push(
                text(value.to_string())
                    .size(theme::T_CAPTION)
                    .color(theme::TEXT_2),
            );
        }
        column = column.push(
            container(Space::new())
                .width(Fill)
                .height(height)
                .style(theme::solid(color, 2.0)),
        );
        if labels {
            column = column.push(text(label).size(theme::T_CAPTION).color(theme::TEXT_3));
        }
        column.into()
    });
    container(
        row(bars)
            .spacing(if labels { theme::S2 } else { 3.0 })
            .align_y(Bottom),
    )
    .height(CHART_HEIGHT + if labels { 40.0 } else { 0.0 })
    .align_y(Bottom)
    .into()
}

fn scores<'a>(stats: &KindStats) -> Element<'a, Msg> {
    let counts = stats.score_counts();
    if counts.iter().all(|(_, count)| *count == 0) {
        return panel("Score distribution", ui::muted("Nothing scored yet."));
    }
    let data = counts
        .iter()
        .map(|(score, count)| (score.to_string(), *count))
        .collect();
    panel("Score distribution", columns(data, theme::TRACK, true))
}

fn years<'a>(stats: &KindStats) -> Element<'a, Msg> {
    let mut years: Vec<&StatBucket> = stats.release_years.iter().collect();
    years.sort_by(|a, b| a.key.cmp(&b.key));
    let (Some(first), Some(last)) = (years.first(), years.last()) else {
        return panel("Release years", ui::muted("No entries yet."));
    };
    let range = row![
        ui::muted(first.key.clone()),
        ui::spacer(),
        ui::muted(last.key.clone())
    ];
    let data = years
        .iter()
        .map(|bucket| (bucket.key.clone(), bucket.count))
        .collect();
    panel(
        "Release years",
        column![
            columns(data, theme::alpha(theme::TRACK, 0.75), false),
            range
        ]
        .spacing(6),
    )
}

/// Liste mit Balken: Name, Anteil, Anzahl und mittlere Bewertung.
fn ranked<'a>(title: &'a str, buckets: &[StatBucket], limit: usize) -> Element<'a, Msg> {
    let mut sorted: Vec<&StatBucket> = buckets.iter().collect();
    sorted.sort_by(|a, b| b.count.cmp(&a.count).then(a.key.cmp(&b.key)));
    let max = sorted.first().map_or(0, |b| b.count);
    if max == 0 {
        return panel(title, ui::muted("No entries yet."));
    }
    let rows = sorted.into_iter().take(limit).map(|bucket| {
        let mean = if bucket.mean_score > 0.0 {
            format!("{:.1}", bucket.mean_score)
        } else {
            "–".to_owned()
        };
        row![
            text(readable(&bucket.key))
                .size(theme::T_SMALL)
                .font(theme::MEDIUM)
                .width(120),
            ui::bar(share_of_max(bucket.count, max), theme::TRACK, 6.0),
            text(bucket.count.to_string())
                .size(theme::T_SMALL)
                .width(40)
                .align_x(iced::Alignment::End),
            text(mean)
                .size(theme::T_SMALL)
                .color(theme::TEXT_2)
                .width(40)
                .align_x(iced::Alignment::End),
        ]
        .spacing(theme::S3)
        .align_y(Center)
        .into()
    });
    let head = row![
        ui::spacer(),
        text("Titles").size(theme::T_CAPTION).color(theme::TEXT_3),
        text("Mean")
            .size(theme::T_CAPTION)
            .color(theme::TEXT_3)
            .width(40)
            .align_x(iced::Alignment::End),
    ]
    .spacing(theme::S3);
    panel(
        title,
        column![head, column(rows).spacing(theme::S2)].spacing(theme::S2),
    )
}

fn body<'a>(app: &'a App, profile: &'a Profile) -> Element<'a, Msg> {
    let kind = app.profile.kind;
    let stats = profile.stats(kind);
    let tab = |label: &str, value: MediaKind| (label.to_owned(), kind == value, Msg::Kind(value));
    let tabs = column![
        ui::tabs(
            [
                tab("Anime stats", MediaKind::Anime),
                tab("Manga stats", MediaKind::Manga)
            ],
            theme::TRACK,
        ),
        ui::divider()
    ];
    let charts = responsive(move |size| {
        // Breit stehen je zwei Diagramme nebeneinander, schmal alle untereinander.
        if size.width >= 820.0 {
            column![
                row![scores(stats), years(stats)].spacing(theme::S4),
                row![
                    ranked("Genres", &stats.genres, 10),
                    ranked("Formats", &stats.formats, 10)
                ]
                .spacing(theme::S4),
            ]
            .spacing(theme::S4)
            .into()
        } else {
            column![
                scores(stats),
                years(stats),
                ranked("Genres", &stats.genres, 10),
                ranked("Formats", &stats.formats, 10),
            ]
            .spacing(theme::S4)
            .into()
        }
    })
    .height(iced::Shrink);
    column![tabs, numbers(stats, kind), statuses(stats, kind), charts]
        .spacing(theme::S4)
        .padding(iced::Padding {
            top: theme::S2,
            right: ui::PAGE_PADDING,
            bottom: theme::S6,
            left: ui::PAGE_PADDING,
        })
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let page: Element<'_, Msg> = if !app.auth.anilist_logged_in {
        ui::empty_state(
            Icon::Profile,
            "Not connected to AniList",
            "Log in to AniList in Settings to see your profile and statistics.",
        )
    } else {
        match &app.profile.profile {
            Some(profile) => ui::scroll(column![header(app, profile), body(app, profile)]),
            None if app.profile.loading => {
                iced::widget::center(ui::muted("Loading your profile…")).into()
            }
            None => ui::empty_state(
                Icon::Profile,
                "No statistics yet",
                "Press Ctrl+R to fetch your profile from AniList.",
            ),
        }
    };
    page.map(Message::Profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_numbers_become_readable() {
        assert_eq!(readable("TV_SHORT"), "TV Short");
        assert_eq!(readable("ONE_SHOT"), "One Shot");
        assert_eq!(readable("TV"), "TV");
        assert_eq!(readable("Slice of Life"), "Slice of Life");
        assert_eq!(grouped(2_741), "2,741");
        assert_eq!(grouped(66), "66");
        assert_eq!(grouped(1_000_000), "1,000,000");
    }

    #[test]
    fn age_names_the_largest_unit() {
        assert_eq!(age(1_000, 990), "Updated just now");
        assert_eq!(age(1_000 + 5 * 60, 1_000), "Updated 5 minutes ago");
        assert_eq!(age(1_000 + 3 * 3_600, 1_000), "Updated 3 hours ago");
        assert_eq!(age(1_000 + 3 * 86_400, 1_000), "Updated 3 days ago");
    }
}
