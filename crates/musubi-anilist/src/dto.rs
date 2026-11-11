//! Antwort-Strukturen von AniList und ihre Umwandlung in Fachtypen.

use musubi_domain::{
    FuzzyDate, KindStats, ListEntry, ListStatus, Media, MediaKind, MediaTitle, NextAiring, Profile,
    ScoreFormat, StatBucket, StatusCount, Viewer,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Envelope<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GraphQlError>>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQlError {
    pub message: String,
    pub status: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TitleDto {
    pub romaji: Option<String>,
    pub english: Option<String>,
    pub native: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DateDto {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

impl From<DateDto> for FuzzyDate {
    fn from(d: DateDto) -> Self {
        Self {
            year: d.year,
            month: d.month,
            day: d.day,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CoverDto {
    pub large: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDto {
    pub id: i64,
    pub id_mal: Option<i64>,
    #[serde(rename = "type")]
    pub kind: Option<MediaKind>,
    #[serde(default)]
    pub title: TitleDto,
    pub synonyms: Option<Vec<Option<String>>>,
    pub format: Option<String>,
    pub status: Option<String>,
    pub episodes: Option<u32>,
    pub chapters: Option<u32>,
    pub volumes: Option<u32>,
    pub season_year: Option<i32>,
    pub start_date: Option<DateDto>,
    pub cover_image: Option<CoverDto>,
    pub description: Option<String>,
    pub genres: Option<Vec<Option<String>>>,
    pub average_score: Option<u32>,
    pub next_airing_episode: Option<NextAiringDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextAiringDto {
    pub airing_at: i64,
    pub episode: u32,
}

/// Entfernt die wenigen HTML-Tags, die AniList auch bei `asHtml: false` mitliefert.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for c in text.chars() {
        match c {
            '<' => inside = true,
            '>' if inside => inside = false,
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out.trim().to_owned()
}

impl From<MediaDto> for Media {
    fn from(d: MediaDto) -> Self {
        Self {
            id: d.id,
            id_mal: d.id_mal,
            kind: d.kind.unwrap_or_default(),
            title: MediaTitle {
                romaji: d.title.romaji,
                english: d.title.english,
                native: d.title.native,
            },
            synonyms: d
                .synonyms
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .collect(),
            format: d.format,
            status: d.status,
            episodes: d.episodes,
            chapters: d.chapters,
            volumes: d.volumes,
            season_year: d.season_year.or(d.start_date.and_then(|s| s.year)),
            cover_url: d.cover_image.and_then(|c| c.large),
            description: d.description.as_deref().map(strip_tags),
            genres: d.genres.unwrap_or_default().into_iter().flatten().collect(),
            average_score: d.average_score,
            next_airing: d.next_airing_episode.map(|next| NextAiring {
                airing_at: next.airing_at,
                episode: next.episode,
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDto {
    pub id: i64,
    pub status: Option<ListStatus>,
    pub score: Option<f64>,
    pub progress: Option<u32>,
    pub progress_volumes: Option<u32>,
    pub repeat: Option<u32>,
    pub priority: Option<u32>,
    pub private: Option<bool>,
    pub hidden_from_status_lists: Option<bool>,
    pub notes: Option<String>,
    pub custom_lists: Option<Value>,
    pub advanced_scores: Option<Value>,
    pub updated_at: Option<i64>,
    pub started_at: Option<DateDto>,
    pub completed_at: Option<DateDto>,
    pub media: MediaDto,
}

impl From<EntryDto> for ListEntry {
    fn from(d: EntryDto) -> Self {
        let custom_lists = d
            .custom_lists
            .as_ref()
            .and_then(Value::as_object)
            .map(|lists| {
                lists
                    .iter()
                    .filter(|(_, enabled)| enabled.as_bool().unwrap_or(false))
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let advanced_scores = d
            .advanced_scores
            .as_ref()
            .and_then(Value::as_object)
            .map(|scores| {
                scores
                    .iter()
                    .map(|(name, value)| (name.clone(), value.as_f64().unwrap_or(0.0)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            id: d.id,
            media: d.media.into(),
            status: d.status.unwrap_or(ListStatus::Planning),
            score_raw: d.score.unwrap_or(0.0).round() as u32,
            progress: d.progress.unwrap_or(0),
            progress_volumes: d.progress_volumes.unwrap_or(0),
            repeat: d.repeat.unwrap_or(0),
            priority: d.priority.unwrap_or(0),
            private: d.private.unwrap_or(false),
            hidden_from_status_lists: d.hidden_from_status_lists.unwrap_or(false),
            notes: d.notes.unwrap_or_default(),
            custom_lists,
            advanced_scores,
            started_at: d.started_at.map(Into::into).unwrap_or_default(),
            completed_at: d.completed_at.map(Into::into).unwrap_or_default(),
            updated_at: d.updated_at.unwrap_or(0),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListTypeOptionsDto {
    pub custom_lists: Option<Vec<Option<String>>>,
    pub advanced_scoring: Option<Vec<Option<String>>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListOptionsDto {
    pub score_format: Option<ScoreFormat>,
    pub anime_list: Option<ListTypeOptionsDto>,
    pub manga_list: Option<ListTypeOptionsDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerDto {
    pub id: i64,
    pub name: String,
    pub media_list_options: Option<ListOptionsDto>,
}

fn flat(names: Option<Vec<Option<String>>>) -> Vec<String> {
    names.unwrap_or_default().into_iter().flatten().collect()
}

impl From<ViewerDto> for Viewer {
    fn from(d: ViewerDto) -> Self {
        let options = d.media_list_options.unwrap_or_default();
        let anime = options.anime_list.unwrap_or_default();
        let manga = options.manga_list.unwrap_or_default();
        Self {
            id: d.id,
            name: d.name,
            score_format: options.score_format.unwrap_or_default(),
            anime_custom_lists: flat(anime.custom_lists),
            manga_custom_lists: flat(manga.custom_lists),
            anime_advanced_scoring: flat(anime.advanced_scoring),
            manga_advanced_scoring: flat(manga.advanced_scoring),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename = "Viewer")]
pub struct ViewerData {
    #[serde(rename = "Viewer")]
    pub viewer: ViewerDto,
}

/// One group of the statistics. Depending on the query, the key sits in a different field.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatDto {
    pub status: Option<ListStatus>,
    /// A float, so that a fractional score never fails the whole profile.
    pub score: Option<f64>,
    pub format: Option<String>,
    pub genre: Option<String>,
    pub release_year: Option<i32>,
    pub count: Option<u32>,
    pub mean_score: Option<f32>,
    pub minutes_watched: Option<u64>,
    pub chapters_read: Option<u64>,
}

impl From<StatDto> for StatBucket {
    fn from(d: StatDto) -> Self {
        let key = d
            .genre
            .or(d.format)
            .or(d.release_year.map(|y| y.to_string()))
            .or(d.score.map(|s| (s.round() as u32).to_string()))
            .unwrap_or_default();
        Self {
            key,
            count: d.count.unwrap_or(0),
            mean_score: d.mean_score.unwrap_or(0.0),
            amount: d.minutes_watched.or(d.chapters_read).unwrap_or(0),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KindStatsDto {
    pub count: Option<u32>,
    pub mean_score: Option<f32>,
    pub standard_deviation: Option<f32>,
    pub minutes_watched: Option<u64>,
    pub episodes_watched: Option<u32>,
    pub chapters_read: Option<u32>,
    pub volumes_read: Option<u32>,
    pub statuses: Option<Vec<Option<StatDto>>>,
    pub scores: Option<Vec<Option<StatDto>>>,
    pub formats: Option<Vec<Option<StatDto>>>,
    pub genres: Option<Vec<Option<StatDto>>>,
    pub release_years: Option<Vec<Option<StatDto>>>,
}

/// Converts the score buckets to `scoreRaw` keys.
///
/// AniList reports them in the user's own scale: 1 to 10 for `POINT_10`, 1 to 5 for stars
/// and so on. Read as 0 to 100, everything but a 10 would vanish from the chart.
fn score_buckets(list: Option<Vec<Option<StatDto>>>, format: ScoreFormat) -> Vec<StatBucket> {
    let list: Vec<StatDto> = list.unwrap_or_default().into_iter().flatten().collect();
    let highest = list.iter().filter_map(|s| s.score).fold(0.0, f64::max);
    // A value above the format's range can only be a raw score already.
    let already_raw = highest > f64::from(format.max_points());
    list.into_iter()
        .map(|stat| {
            let score = stat.score.unwrap_or(0.0).max(0.0);
            let raw = if already_raw || format == ScoreFormat::Point100 {
                score.round() as u32
            } else if format == ScoreFormat::Point10Decimal {
                (score * 10.0).round() as u32
            } else {
                format.points_to_raw(score.round() as u32)
            };
            StatBucket {
                key: raw.min(100).to_string(),
                ..StatBucket::from(stat)
            }
        })
        .collect()
}

impl KindStatsDto {
    fn into_stats(self, format: ScoreFormat) -> KindStats {
        let d = self;
        let buckets = |list: Option<Vec<Option<StatDto>>>| -> Vec<StatBucket> {
            list.unwrap_or_default()
                .into_iter()
                .flatten()
                .map(StatBucket::from)
                .collect()
        };
        let statuses = d
            .statuses
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter_map(|s| {
                Some(StatusCount {
                    status: s.status?,
                    count: s.count.unwrap_or(0),
                })
            })
            .collect();
        KindStats {
            count: d.count.unwrap_or(0),
            mean_score: d.mean_score.unwrap_or(0.0),
            standard_deviation: d.standard_deviation.unwrap_or(0.0),
            minutes_watched: d.minutes_watched.unwrap_or(0),
            episodes_watched: d.episodes_watched.unwrap_or(0),
            chapters_read: d.chapters_read.unwrap_or(0),
            volumes_read: d.volumes_read.unwrap_or(0),
            statuses,
            scores: score_buckets(d.scores, format),
            formats: buckets(d.formats),
            genres: buckets(d.genres),
            release_years: buckets(d.release_years),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StatisticsDto {
    pub anime: Option<KindStatsDto>,
    pub manga: Option<KindStatsDto>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AvatarDto {
    pub large: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDto {
    pub name: String,
    pub site_url: Option<String>,
    pub banner_image: Option<String>,
    pub avatar: Option<AvatarDto>,
    pub media_list_options: Option<ListOptionsDto>,
    pub statistics: Option<StatisticsDto>,
}

impl From<ProfileDto> for Profile {
    fn from(d: ProfileDto) -> Self {
        let statistics = d.statistics.unwrap_or_default();
        let format = d
            .media_list_options
            .and_then(|o| o.score_format)
            .unwrap_or_default();
        Self {
            name: d.name,
            avatar_url: d.avatar.and_then(|a| a.large),
            banner_url: d.banner_image,
            site_url: d.site_url,
            anime: statistics.anime.unwrap_or_default().into_stats(format),
            manga: statistics.manga.unwrap_or_default().into_stats(format),
            fetched_at: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProfileData {
    #[serde(rename = "Viewer")]
    pub viewer: ProfileDto,
}

#[derive(Debug, Deserialize)]
pub struct ListGroupDto {
    pub entries: Option<Vec<EntryDto>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub has_next_chunk: Option<bool>,
    pub lists: Option<Vec<ListGroupDto>>,
}

#[derive(Debug, Deserialize)]
pub struct CollectionData {
    #[serde(rename = "MediaListCollection")]
    pub collection: CollectionDto,
}

#[derive(Debug, Deserialize)]
pub struct PageMedia {
    pub media: Vec<MediaDto>,
}

#[derive(Debug, Deserialize)]
pub struct PageData {
    #[serde(rename = "Page")]
    pub page: PageMedia,
}

#[derive(Debug, Deserialize)]
pub struct MediaData {
    #[serde(rename = "Media")]
    pub media: MediaDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationEdgeDto {
    pub relation_type: Option<String>,
    pub node: Option<MediaDto>,
}

#[derive(Debug, Deserialize)]
pub struct RelationsDto {
    pub edges: Option<Vec<RelationEdgeDto>>,
}

#[derive(Debug, Deserialize)]
pub struct MediaWithRelationsDto {
    #[serde(flatten)]
    pub media: MediaDto,
    pub relations: Option<RelationsDto>,
}

#[derive(Debug, Deserialize)]
pub struct RelationsData {
    #[serde(rename = "Media")]
    pub media: MediaWithRelationsDto,
}

#[derive(Debug, Deserialize)]
pub struct SaveData {
    #[serde(rename = "SaveMediaListEntry")]
    pub entry: EntryDto,
}

#[derive(Debug, Deserialize)]
pub struct Deleted {
    pub deleted: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteData {
    #[serde(rename = "DeleteMediaListEntry")]
    pub result: Deleted,
}

#[derive(Debug, Deserialize)]
pub struct Ignored {}

#[derive(Debug, Deserialize)]
pub struct UpdateUserData {
    #[serde(rename = "UpdateUser")]
    pub _user: Ignored,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(format: ScoreFormat, scores: &[(f64, u32)]) -> KindStats {
        let scores = scores
            .iter()
            .map(|(score, count)| {
                Some(StatDto {
                    score: Some(*score),
                    count: Some(*count),
                    ..StatDto::default()
                })
            })
            .collect();
        KindStatsDto {
            scores: Some(scores),
            ..KindStatsDto::default()
        }
        .into_stats(format)
    }

    #[test]
    fn score_buckets_are_read_in_the_users_scale() {
        let ten = stats(ScoreFormat::Point10, &[(10.0, 33), (7.0, 12), (1.0, 2)]).score_counts();
        assert_eq!(ten[9], (100, 33));
        assert_eq!(ten[6], (70, 12));
        assert_eq!(ten[0], (10, 2));

        let stars = stats(ScoreFormat::Point5, &[(5.0, 4), (3.0, 6)]).score_counts();
        assert_eq!(stars[9], (100, 4));
        assert_eq!(stars[5], (60, 6));

        let decimal = stats(ScoreFormat::Point10Decimal, &[(8.5, 3), (9.0, 1)]).score_counts();
        assert_eq!(decimal[7], (80, 3));
        assert_eq!(decimal[8], (90, 1));
    }

    #[test]
    fn raw_score_buckets_stay_as_they_are() {
        let raw = stats(ScoreFormat::Point100, &[(85.0, 2), (70.0, 5)]).score_counts();
        assert_eq!(raw[7], (80, 2));
        assert_eq!(raw[6], (70, 5));
        // A scale above the format's range can only be raw already.
        let wide = stats(ScoreFormat::Point10, &[(80.0, 2), (10.0, 1)]).score_counts();
        assert_eq!(wide[7], (80, 2));
        assert_eq!(wide[0], (10, 1));
    }
}
