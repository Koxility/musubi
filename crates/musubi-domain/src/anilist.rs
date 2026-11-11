//! AniList-Fachtypen: Medien, Listeneinträge, Status, Scores und Teil-Updates.

use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Medientyp einer AniList.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MediaKind {
    #[default]
    Anime,
    Manga,
}

impl MediaKind {
    pub fn as_api(self) -> &'static str {
        match self {
            Self::Anime => "ANIME",
            Self::Manga => "MANGA",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Anime => "Anime",
            Self::Manga => "Manga",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Anime => Self::Manga,
            Self::Manga => Self::Anime,
        }
    }
}

/// Status eines Listeneintrags. Die serde-Namen entsprechen dem AniList-Enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ListStatus {
    Current,
    Planning,
    Completed,
    Dropped,
    Paused,
    Repeating,
}

impl ListStatus {
    pub const ALL: [Self; 6] = [
        Self::Current,
        Self::Repeating,
        Self::Completed,
        Self::Paused,
        Self::Dropped,
        Self::Planning,
    ];

    pub fn as_api(self) -> &'static str {
        match self {
            Self::Current => "CURRENT",
            Self::Planning => "PLANNING",
            Self::Completed => "COMPLETED",
            Self::Dropped => "DROPPED",
            Self::Paused => "PAUSED",
            Self::Repeating => "REPEATING",
        }
    }

    pub fn label(self, kind: MediaKind) -> &'static str {
        match (self, kind) {
            (Self::Current, MediaKind::Anime) => "Watching",
            (Self::Current, MediaKind::Manga) => "Reading",
            (Self::Repeating, MediaKind::Anime) => "Rewatching",
            (Self::Repeating, MediaKind::Manga) => "Rereading",
            (Self::Planning, _) => "Planning",
            (Self::Completed, _) => "Completed",
            (Self::Dropped, _) => "Dropped",
            (Self::Paused, _) => "Paused",
        }
    }
}

/// Bewertungsformat des Nutzers. Intern arbeitet Musubi immer mit `scoreRaw` (0..=100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScoreFormat {
    #[serde(rename = "POINT_100")]
    Point100,
    #[default]
    #[serde(rename = "POINT_10_DECIMAL")]
    Point10Decimal,
    #[serde(rename = "POINT_10")]
    Point10,
    #[serde(rename = "POINT_5")]
    Point5,
    #[serde(rename = "POINT_3")]
    Point3,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid score `{input}` (expected {expected})")]
pub struct ParseScoreError {
    pub input: String,
    pub expected: &'static str,
}

impl ScoreFormat {
    /// Beschreibung des erlaubten Wertebereichs für Eingabemasken.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Point100 => "0-100",
            Self::Point10Decimal => "0.0-10.0",
            Self::Point10 => "0-10",
            Self::Point5 => "0-5",
            Self::Point3 => "0-3 or :( :| :)",
        }
    }

    /// Stellt einen Rohwert im Format des Nutzers dar. `0` bedeutet „nicht bewertet“.
    pub fn display(self, raw: u32) -> String {
        if raw == 0 {
            return "-".to_owned();
        }
        match self {
            Self::Point100 => raw.to_string(),
            Self::Point10Decimal => format!("{:.1}", f64::from(raw) / 10.0),
            Self::Point10 => ((raw + 5) / 10).max(1).to_string(),
            Self::Point5 => format!("{}/5", ((raw + 10) / 20).max(1)),
            Self::Point3 => match raw {
                0..=40 => ":(",
                41..=60 => ":|",
                _ => ":)",
            }
            .to_owned(),
        }
    }

    /// Converts a whole score in the user's format, e.g. `7` of 10 or `4` stars, to
    /// `scoreRaw`.
    pub fn points_to_raw(self, points: u32) -> u32 {
        match self {
            Self::Point100 => points.min(100),
            Self::Point10Decimal | Self::Point10 => (points * 10).min(100),
            Self::Point5 => (points * 20).min(100),
            Self::Point3 => match points {
                0 => 0,
                1 => 35,
                2 => 60,
                _ => 85,
            },
        }
    }

    /// The highest whole score of this format.
    pub fn max_points(self) -> u32 {
        match self {
            Self::Point100 => 100,
            Self::Point10Decimal | Self::Point10 => 10,
            Self::Point5 => 5,
            Self::Point3 => 3,
        }
    }

    /// Converts user input in the user's format to `scoreRaw`.
    pub fn parse(self, input: &str) -> Result<u32, ParseScoreError> {
        let trimmed = input.trim();
        let err = || ParseScoreError {
            input: input.to_owned(),
            expected: self.hint(),
        };
        if trimmed.is_empty() || trimmed == "-" {
            return Ok(0);
        }
        match self {
            Self::Point100 => {
                let value: u32 = trimmed.parse().map_err(|_| err())?;
                (value <= 100).then_some(value).ok_or_else(err)
            }
            Self::Point10Decimal => {
                let value: f64 = trimmed.parse().map_err(|_| err())?;
                if (0.0..=10.0).contains(&value) {
                    Ok((value * 10.0).round() as u32)
                } else {
                    Err(err())
                }
            }
            Self::Point10 => {
                let value: u32 = trimmed.parse().map_err(|_| err())?;
                (value <= 10).then_some(value * 10).ok_or_else(err)
            }
            Self::Point5 => {
                let value: u32 = trimmed.parse().map_err(|_| err())?;
                (value <= 5).then_some(value * 20).ok_or_else(err)
            }
            Self::Point3 => match trimmed {
                "0" => Ok(0),
                "1" | ":(" => Ok(35),
                "2" | ":|" => Ok(60),
                "3" | ":)" => Ok(85),
                _ => Err(err()),
            },
        }
    }
}

/// Datum mit optionalen Teilen, wie AniList es führt. Ein leeres Datum löscht den Wert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FuzzyDate {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

impl FuzzyDate {
    pub fn is_empty(&self) -> bool {
        self.year.is_none() && self.month.is_none() && self.day.is_none()
    }
}

impl From<NaiveDate> for FuzzyDate {
    fn from(date: NaiveDate) -> Self {
        Self {
            year: Some(date.year()),
            month: Some(date.month()),
            day: Some(date.day()),
        }
    }
}

impl fmt::Display for FuzzyDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.year, self.month, self.day) {
            (Some(y), Some(m), Some(d)) => write!(f, "{y:04}-{m:02}-{d:02}"),
            (Some(y), Some(m), None) => write!(f, "{y:04}-{m:02}"),
            (Some(y), None, _) => write!(f, "{y:04}"),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid date `{0}` (expected YYYY, YYYY-MM or YYYY-MM-DD)")]
pub struct ParseDateError(pub String);

impl FromStr for FuzzyDate {
    type Err = ParseDateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseDateError(s.to_owned());
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Ok(Self::default());
        }
        let mut parts = trimmed.split('-');
        let year: i32 = parts.next().ok_or_else(err)?.parse().map_err(|_| err())?;
        let month = parts
            .next()
            .map(|m| m.parse::<u32>().map_err(|_| err()))
            .transpose()?;
        let day = parts
            .next()
            .map(|d| d.parse::<u32>().map_err(|_| err()))
            .transpose()?;
        if parts.next().is_some() {
            return Err(err());
        }
        let valid = match (month, day) {
            (None, _) => true,
            (Some(m), None) => (1..=12).contains(&m),
            (Some(m), Some(d)) => NaiveDate::from_ymd_opt(year, m, d).is_some(),
        };
        valid
            .then_some(Self {
                year: Some(year),
                month,
                day,
            })
            .ok_or_else(err)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MediaTitle {
    pub romaji: Option<String>,
    pub english: Option<String>,
    pub native: Option<String>,
}

impl MediaTitle {
    /// Bevorzugter Anzeigetitel: Romaji, dann Englisch, dann Originaltitel.
    pub fn preferred(&self) -> &str {
        self.romaji
            .as_deref()
            .or(self.english.as_deref())
            .or(self.native.as_deref())
            .unwrap_or("(untitled)")
    }

    pub fn all(&self) -> impl Iterator<Item = &str> {
        [&self.romaji, &self.english, &self.native]
            .into_iter()
            .filter_map(|t| t.as_deref())
    }
}

/// Ein Werk auf AniList.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Media {
    pub id: i64,
    pub id_mal: Option<i64>,
    pub kind: MediaKind,
    pub title: MediaTitle,
    pub synonyms: Vec<String>,
    /// z. B. `TV`, `MOVIE`, `OVA`, `ONA`, `MANGA`.
    pub format: Option<String>,
    /// z. B. `FINISHED`, `RELEASING`.
    pub status: Option<String>,
    pub episodes: Option<u32>,
    pub chapters: Option<u32>,
    pub volumes: Option<u32>,
    pub season_year: Option<i32>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub average_score: Option<u32>,
    /// Fehlt in Zwischenspeichern aus älteren Versionen.
    #[serde(default)]
    pub next_airing: Option<NextAiring>,
}

/// Die nächste Ausstrahlung eines laufenden Anime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextAiring {
    /// Unix-Zeit in Sekunden.
    pub airing_at: i64,
    pub episode: u32,
}

impl Media {
    /// Gesamtzahl der Einheiten, die `progress` zählt (Episoden oder Kapitel).
    pub fn total_units(&self) -> Option<u32> {
        match self.kind {
            MediaKind::Anime => self.episodes,
            MediaKind::Manga => self.chapters,
        }
    }
}

/// Ein Eintrag auf der Liste des Nutzers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListEntry {
    pub id: i64,
    pub media: Media,
    pub status: ListStatus,
    pub score_raw: u32,
    pub progress: u32,
    pub progress_volumes: u32,
    pub repeat: u32,
    pub priority: u32,
    pub private: bool,
    pub hidden_from_status_lists: bool,
    pub notes: String,
    /// Namen der aktivierten Custom Lists.
    pub custom_lists: Vec<String>,
    pub advanced_scores: Vec<(String, f64)>,
    pub started_at: FuzzyDate,
    pub completed_at: FuzzyDate,
    pub updated_at: i64,
}

impl ListEntry {
    pub fn is_watching(&self) -> bool {
        matches!(self.status, ListStatus::Current | ListStatus::Repeating)
    }

    /// Bereits ausgestrahlte Episoden, die noch nicht gesehen sind.
    pub fn episodes_behind(&self) -> u32 {
        self.media.next_airing.map_or(0, |next| {
            next.episode.saturating_sub(1).saturating_sub(self.progress)
        })
    }
}

/// Aktuell geschaute Titel mit kommender Ausstrahlung, die nächste Folge zuerst.
pub fn airing_entries(entries: &[ListEntry]) -> Vec<&ListEntry> {
    let mut airing: Vec<&ListEntry> = entries
        .iter()
        .filter(|e| e.is_watching() && e.media.next_airing.is_some())
        .collect();
    airing.sort_by_key(|e| e.media.next_airing.map(|next| next.airing_at));
    airing
}

/// Aktuell geschaute Titel ohne kommende Ausstrahlung, zuletzt bearbeitete zuerst.
pub fn in_progress_entries(entries: &[ListEntry]) -> Vec<&ListEntry> {
    let mut progress: Vec<&ListEntry> = entries
        .iter()
        .filter(|e| e.is_watching() && e.media.next_airing.is_none())
        .collect();
    progress.sort_by_key(|e| std::cmp::Reverse(e.updated_at));
    progress
}

/// Teil-Update eines Listeneintrags. `None` lässt ein Feld unverändert.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EntryPatch {
    pub media_id: i64,
    pub status: Option<ListStatus>,
    pub score_raw: Option<u32>,
    pub progress: Option<u32>,
    pub progress_volumes: Option<u32>,
    pub repeat: Option<u32>,
    pub priority: Option<u32>,
    pub private: Option<bool>,
    pub hidden_from_status_lists: Option<bool>,
    pub notes: Option<String>,
    pub custom_lists: Option<Vec<String>>,
    pub advanced_scores: Option<Vec<f64>>,
    pub started_at: Option<FuzzyDate>,
    pub completed_at: Option<FuzzyDate>,
}

impl EntryPatch {
    pub fn new(media_id: i64) -> Self {
        Self {
            media_id,
            ..Self::default()
        }
    }

    /// Führt ein neueres Update in dieses ein. Felder von `newer` gewinnen.
    pub fn merge(&mut self, newer: EntryPatch) {
        macro_rules! take {
            ($($field:ident),+) => { $(if newer.$field.is_some() { self.$field = newer.$field; })+ };
        }
        take!(
            status,
            score_raw,
            progress,
            progress_volumes,
            repeat,
            priority,
            private,
            hidden_from_status_lists,
            notes,
            custom_lists,
            advanced_scores,
            started_at,
            completed_at
        );
    }

    /// Überträgt das Update auf einen lokalen Eintrag (optimistische Anzeige).
    pub fn apply_to(&self, entry: &mut ListEntry) {
        macro_rules! set {
            ($($field:ident),+) => { $(if let Some(v) = &self.$field { entry.$field = v.clone(); })+ };
        }
        set!(
            status,
            score_raw,
            progress,
            progress_volumes,
            repeat,
            priority,
            private,
            hidden_from_status_lists,
            notes,
            custom_lists,
            started_at,
            completed_at
        );
    }
}

/// Der angemeldete AniList-Nutzer.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Viewer {
    pub id: i64,
    pub name: String,
    pub score_format: ScoreFormat,
    pub anime_custom_lists: Vec<String>,
    pub manga_custom_lists: Vec<String>,
    pub anime_advanced_scoring: Vec<String>,
    pub manga_advanced_scoring: Vec<String>,
}

impl Viewer {
    pub fn custom_lists(&self, kind: MediaKind) -> &[String] {
        match kind {
            MediaKind::Anime => &self.anime_custom_lists,
            MediaKind::Manga => &self.manga_custom_lists,
        }
    }

    /// Namen der Kategorien für Advanced Scoring, in der von AniList erwarteten Reihenfolge.
    pub fn advanced_scoring(&self, kind: MediaKind) -> &[String] {
        match kind {
            MediaKind::Anime => &self.anime_advanced_scoring,
            MediaKind::Manga => &self.manga_advanced_scoring,
        }
    }
}

/// Eine Gruppe der Statistik, z. B. ein Genre, ein Format oder ein Erscheinungsjahr.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StatBucket {
    /// Name der Gruppe, wie AniList ihn liefert, z. B. `Drama`, `TV_SHORT`, `2019`.
    pub key: String,
    pub count: u32,
    /// Mittlere Bewertung von 0 bis 100, 0 ohne Bewertungen.
    pub mean_score: f32,
    /// Gesehene Minuten bei Anime, gelesene Kapitel bei Manga.
    pub amount: u64,
}

/// Anzahl der Einträge mit einem Status.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatusCount {
    pub status: ListStatus,
    pub count: u32,
}

/// Statistik über alle Anime oder alle Manga eines Nutzers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KindStats {
    pub count: u32,
    /// Mittlere Bewertung von 0 bis 100.
    pub mean_score: f32,
    pub standard_deviation: f32,
    pub minutes_watched: u64,
    pub episodes_watched: u32,
    pub chapters_read: u32,
    pub volumes_read: u32,
    pub statuses: Vec<StatusCount>,
    /// Entries per score. The keys are `scoreRaw` values from 1 to 100, whatever the
    /// user's score format is.
    pub scores: Vec<StatBucket>,
    pub formats: Vec<StatBucket>,
    pub genres: Vec<StatBucket>,
    pub release_years: Vec<StatBucket>,
}

impl KindStats {
    /// Gesehene Zeit in Tagen.
    pub fn days_watched(&self) -> f32 {
        self.minutes_watched as f32 / 1_440.0
    }

    /// Anzahl der Einträge mit diesem Status.
    pub fn status_count(&self, status: ListStatus) -> u32 {
        self.statuses
            .iter()
            .filter(|s| s.status == status)
            .map(|s| s.count)
            .sum()
    }

    /// Entries per score step from 10 to 100. Steps without entries have 0, so the chart
    /// always shows all ten columns.
    pub fn score_counts(&self) -> [(u32, u32); 10] {
        let mut counts = [(0, 0); 10];
        for (index, slot) in counts.iter_mut().enumerate() {
            slot.0 = (index as u32 + 1) * 10;
        }
        for bucket in &self.scores {
            let Ok(score) = bucket.key.parse::<u32>() else {
                continue;
            };
            // Finer scores such as 85 count towards the step below and 1 to 9 towards the
            // lowest step; 0 means unscored.
            if score == 0 {
                continue;
            }
            let step = (score / 10).clamp(1, 10) as usize;
            if let Some(slot) = counts.get_mut(step - 1) {
                slot.1 += bucket.count;
            }
        }
        counts
    }
}

/// Anteil von `value` am größten Wert der Reihe, für die Länge eines Balkens.
pub fn share_of_max(value: u32, max: u32) -> f32 {
    if max == 0 {
        0.0
    } else {
        (value as f32 / max as f32).min(1.0)
    }
}

/// Öffentliches Profil des angemeldeten Nutzers mit seiner Statistik.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub site_url: Option<String>,
    pub anime: KindStats,
    pub manga: KindStats,
    /// Unix-Zeit des Abrufs, damit die GUI das Alter einer Kopie aus dem Cache zeigen kann.
    pub fetched_at: i64,
}

impl Profile {
    pub fn stats(&self, kind: MediaKind) -> &KindStats {
        match kind {
            MediaKind::Anime => &self.anime,
            MediaKind::Manga => &self.manga,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_counts_always_cover_ten_steps() {
        let bucket = |key: &str, count| StatBucket {
            key: key.to_owned(),
            count,
            ..StatBucket::default()
        };
        let stats = KindStats {
            scores: vec![
                bucket("70", 4),
                bucket("85", 2),
                bucket("80", 1),
                bucket("0", 9),
                bucket("5", 1),
            ],
            ..KindStats::default()
        };
        let counts = stats.score_counts();
        assert_eq!(counts[0], (10, 1));
        assert_eq!(counts[6], (70, 4));
        assert_eq!(counts[7], (80, 3));
        assert_eq!(counts[9], (100, 0));
    }

    #[test]
    fn bar_share_is_relative_to_the_largest_value() {
        assert_eq!(share_of_max(5, 10), 0.5);
        assert_eq!(share_of_max(12, 10), 1.0);
        assert_eq!(share_of_max(3, 0), 0.0);
    }

    #[test]
    fn score_round_trips_in_each_format() {
        let cases = [
            (ScoreFormat::Point100, "85", 85, "85"),
            (ScoreFormat::Point10Decimal, "8.5", 85, "8.5"),
            (ScoreFormat::Point10, "8", 80, "8"),
            (ScoreFormat::Point5, "4", 80, "4/5"),
            (ScoreFormat::Point3, ":)", 85, ":)"),
            (ScoreFormat::Point3, "2", 60, ":|"),
            (ScoreFormat::Point3, "1", 35, ":("),
        ];
        for (format, input, raw, shown) in cases {
            assert_eq!(format.parse(input), Ok(raw), "{format:?} {input}");
            assert_eq!(format.display(raw), shown, "{format:?} {raw}");
        }
    }

    #[test]
    fn score_rejects_out_of_range_and_treats_empty_as_unscored() {
        assert!(ScoreFormat::Point100.parse("101").is_err());
        assert!(ScoreFormat::Point10Decimal.parse("10.5").is_err());
        assert!(ScoreFormat::Point5.parse("6").is_err());
        assert!(ScoreFormat::Point3.parse("4").is_err());
        assert_eq!(ScoreFormat::Point10.parse(""), Ok(0));
        assert_eq!(ScoreFormat::Point10.display(0), "-");
    }

    #[test]
    fn score_format_uses_api_names() {
        let parsed: ScoreFormat = serde_json::from_str("\"POINT_10_DECIMAL\"").unwrap();
        assert_eq!(parsed, ScoreFormat::Point10Decimal);
        let parsed: ScoreFormat = serde_json::from_str("\"POINT_3\"").unwrap();
        assert_eq!(parsed, ScoreFormat::Point3);
    }

    #[test]
    fn fuzzy_date_parses_partial_values() {
        let full: FuzzyDate = "2024-02-29".parse().unwrap();
        assert_eq!(full.to_string(), "2024-02-29");
        let month: FuzzyDate = "2024-02".parse().unwrap();
        assert_eq!((month.month, month.day), (Some(2), None));
        assert!("".parse::<FuzzyDate>().unwrap().is_empty());
        assert!("2023-02-29".parse::<FuzzyDate>().is_err());
        assert!("2023-13".parse::<FuzzyDate>().is_err());
        assert!("2023-01-01-01".parse::<FuzzyDate>().is_err());
    }

    #[test]
    fn newer_patch_fields_win_and_others_stay() {
        let mut old = EntryPatch {
            progress: Some(3),
            notes: Some("a".into()),
            ..EntryPatch::new(1)
        };
        old.merge(EntryPatch {
            progress: Some(4),
            ..EntryPatch::new(1)
        });
        assert_eq!(old.progress, Some(4));
        assert_eq!(old.notes.as_deref(), Some("a"));
    }

    fn watching(
        media_id: i64,
        progress: u32,
        next: Option<(i64, u32)>,
        updated_at: i64,
    ) -> ListEntry {
        ListEntry {
            id: media_id,
            media: Media {
                id: media_id,
                next_airing: next.map(|(airing_at, episode)| NextAiring { airing_at, episode }),
                ..Media::default()
            },
            status: ListStatus::Current,
            score_raw: 0,
            progress,
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
            updated_at,
        }
    }

    #[test]
    fn airing_and_in_progress_split_the_watching_list() {
        let mut planned = watching(5, 0, Some((10, 2)), 0);
        planned.status = ListStatus::Planning;
        let entries = [
            watching(1, 0, Some((500, 2)), 0),
            watching(2, 0, Some((100, 2)), 0),
            watching(3, 0, None, 10),
            watching(4, 0, None, 20),
            planned,
        ];
        let ids = |list: Vec<&ListEntry>| list.iter().map(|e| e.media.id).collect::<Vec<_>>();
        assert_eq!(ids(airing_entries(&entries)), vec![2, 1]);
        assert_eq!(ids(in_progress_entries(&entries)), vec![4, 3]);
    }

    #[test]
    fn behind_counts_aired_but_unwatched_episodes() {
        assert_eq!(watching(1, 2, Some((0, 8)), 0).episodes_behind(), 5);
        assert_eq!(watching(1, 7, Some((0, 8)), 0).episodes_behind(), 0);
        assert_eq!(watching(1, 9, Some((0, 8)), 0).episodes_behind(), 0);
        assert_eq!(watching(1, 2, None, 0).episodes_behind(), 0);
    }

    #[test]
    fn cached_media_without_airing_data_still_loads() {
        let mut value = serde_json::to_value(Media::default()).unwrap();
        value.as_object_mut().unwrap().remove("next_airing");
        let media: Media = serde_json::from_value(value).unwrap();
        assert_eq!(media.next_airing, None);
    }
}
