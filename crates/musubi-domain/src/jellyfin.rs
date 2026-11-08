//! Jellyfin-Fachtypen: Bibliotheken, Serien, Filme, Staffeln, Episoden.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub collection_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    Series,
    Movie,
}

/// Serie oder Film aus der Bibliothek.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryItem {
    pub id: String,
    pub kind: ItemKind,
    pub name: String,
    pub overview: Option<String>,
    pub year: Option<i32>,
    /// z. B. `Continuing` oder `Ended`.
    pub status: Option<String>,
    pub genres: Vec<String>,
    pub community_rating: Option<f32>,
    pub runtime_secs: Option<u64>,
    pub provider_ids: BTreeMap<String, String>,
    pub has_poster: bool,
    /// Fehlt in Zwischenspeichern aus älteren Versionen.
    #[serde(default)]
    pub has_backdrop: bool,
    pub played: bool,
    pub unplayed_count: Option<u32>,
    pub position_secs: u64,
}

/// Eintrag eines Regals auf der Startseite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShelfItem {
    pub id: String,
    /// Serie einer Episode, sonst das Item selbst.
    pub library_id: String,
    pub is_episode: bool,
    /// Serientitel bei Episoden, sonst der Name des Films oder der Serie.
    pub title: String,
    /// Bei Episoden z. B. `S1:E3 · Name`.
    pub subtitle: Option<String>,
    pub overview: Option<String>,
    pub year: Option<i32>,
    pub runtime_secs: Option<u64>,
    pub position_secs: u64,
    pub has_image: bool,
}

impl ShelfItem {
    /// Gesehener Anteil von 0 bis 1, falls die Laufzeit bekannt ist.
    pub fn progress(&self) -> Option<f32> {
        let runtime = self.runtime_secs.filter(|r| *r > 0)?;
        (self.position_secs > 0).then(|| (self.position_secs as f32 / runtime as f32).min(1.0))
    }
}

impl LibraryItem {
    /// Sucht eine Provider-ID ohne Rücksicht auf Groß-/Kleinschreibung.
    pub fn provider_id(&self, names: &[&str]) -> Option<&str> {
        self.provider_ids
            .iter()
            .find(|(key, _)| names.iter().any(|n| key.eq_ignore_ascii_case(n)))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Season {
    pub id: String,
    pub name: String,
    pub number: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub series_id: String,
    pub series_name: String,
    pub season_number: u32,
    pub number: u32,
    pub name: String,
    pub overview: Option<String>,
    pub runtime_secs: Option<u64>,
    pub played: bool,
    pub position_secs: u64,
    pub has_image: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LibrarySort {
    #[default]
    Name,
    Year,
    DateAdded,
    Rating,
}

/// Such- und Filterkriterien für die Bibliothek.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryQuery {
    pub library_id: Option<String>,
    pub search: Option<String>,
    pub genres: Vec<String>,
    pub years: Vec<i32>,
    /// `Continuing` oder `Ended`.
    pub status: Option<String>,
    pub kind: Option<ItemKind>,
    pub unplayed_only: bool,
    pub sort: LibrarySort,
    pub start: u32,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LibraryFilters {
    pub genres: Vec<String>,
    pub years: Vec<i32>,
}
