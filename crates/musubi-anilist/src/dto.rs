//! Antwort-Strukturen von AniList und ihre Umwandlung in Fachtypen.

use musubi_domain::{
    FuzzyDate, ListEntry, ListStatus, Media, MediaKind, MediaTitle, NextAiring, ScoreFormat, Viewer,
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
