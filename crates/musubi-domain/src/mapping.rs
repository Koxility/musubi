//! Zuordnung zwischen Jellyfin-Staffeln und AniList-Einträgen.
//!
//! AniList zählt pro Werk (oft pro Cour), Jellyfin pro Staffel. Deshalb gilt eine Zuordnung
//! immer für einen Episodenbereich einer Staffel und trägt einen Offset.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MappingSource {
    Manual,
    ProviderId,
    Title,
}

impl MappingSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::ProviderId => "provider",
            Self::Title => "title",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "provider" => Self::ProviderId,
            "title" => Self::Title,
            _ => Self::Manual,
        }
    }
}

/// Filme werden als Staffel 1, Episode 1 geführt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    pub jellyfin_id: String,
    pub season: u32,
    pub ep_start: u32,
    /// `None` bedeutet offenes Ende (laufende Serien).
    pub ep_end: Option<u32>,
    pub media_id: i64,
    /// `anilist_episode = jellyfin_episode + offset`.
    pub offset: i32,
    pub source: MappingSource,
    pub confidence: f64,
    pub title: String,
}

impl Mapping {
    pub fn contains(&self, season: u32, episode: u32) -> bool {
        self.season == season
            && episode >= self.ep_start
            && self.ep_end.is_none_or(|end| episode <= end)
    }

    /// Übersetzt eine Jellyfin-Episode in die AniList-Zählung.
    pub fn anilist_episode(&self, episode: u32) -> Option<u32> {
        let mapped = i64::from(episode) + i64::from(self.offset);
        u32::try_from(mapped).ok().filter(|ep| *ep > 0)
    }
}

/// Findet Werk und AniList-Episode zu einer Jellyfin-Episode.
pub fn resolve_episode(mappings: &[Mapping], season: u32, episode: u32) -> Option<(i64, u32)> {
    mappings
        .iter()
        .filter(|m| m.contains(season, episode))
        .find_map(|m| m.anilist_episode(episode).map(|ep| (m.media_id, ep)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(season: u32, start: u32, end: Option<u32>, media: i64, offset: i32) -> Mapping {
        Mapping {
            jellyfin_id: "x".into(),
            season,
            ep_start: start,
            ep_end: end,
            media_id: media,
            offset,
            source: MappingSource::Manual,
            confidence: 1.0,
            title: String::new(),
        }
    }

    #[test]
    fn splits_one_season_over_two_cours() {
        let maps = [
            mapping(1, 1, Some(12), 10, 0),
            mapping(1, 13, Some(24), 20, -12),
        ];
        assert_eq!(resolve_episode(&maps, 1, 12), Some((10, 12)));
        assert_eq!(resolve_episode(&maps, 1, 13), Some((20, 1)));
        assert_eq!(resolve_episode(&maps, 1, 25), None);
    }

    #[test]
    fn open_end_and_absolute_numbering() {
        let maps = [mapping(2, 25, None, 30, -24)];
        assert_eq!(resolve_episode(&maps, 2, 26), Some((30, 2)));
        assert_eq!(resolve_episode(&maps, 1, 26), None);
    }

    #[test]
    fn negative_results_are_rejected() {
        let maps = [mapping(1, 1, None, 1, -5)];
        assert_eq!(resolve_episode(&maps, 1, 3), None);
    }
}
