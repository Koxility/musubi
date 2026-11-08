//! Antwort-Strukturen der Jellyfin-API und ihre Umwandlung in Fachtypen.

use std::collections::BTreeMap;

use musubi_domain::{Episode, ItemKind, LibraryItem, ShelfItem};
use serde::Deserialize;

use crate::{provider_map, ticks_to_secs};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PublicInfoDto {
    pub server_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthDto {
    pub access_token: String,
    pub user: UserDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PagedDto<T> {
    pub items: Vec<T>,
    pub total_record_count: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct UserDataDto {
    pub played: bool,
    pub unplayed_item_count: Option<u32>,
    pub playback_position_ticks: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ItemDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "Type")]
    pub item_type: Option<String>,
    pub collection_type: Option<String>,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub status: Option<String>,
    pub genres: Option<Vec<String>>,
    pub community_rating: Option<f32>,
    pub run_time_ticks: Option<u64>,
    pub provider_ids: Option<BTreeMap<String, Option<String>>>,
    pub image_tags: Option<BTreeMap<String, Option<String>>>,
    pub backdrop_image_tags: Option<Vec<String>>,
    pub user_data: Option<UserDataDto>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub index_number: Option<u32>,
    pub parent_index_number: Option<u32>,
}

impl ItemDto {
    fn has_primary(&self) -> bool {
        self.image_tags
            .as_ref()
            .is_some_and(|tags| tags.contains_key("Primary"))
    }

    pub fn into_library_item(self) -> Option<LibraryItem> {
        let kind = match self.item_type.as_deref() {
            Some("Series") => ItemKind::Series,
            Some("Movie") => ItemKind::Movie,
            _ => return None,
        };
        let has_poster = self.has_primary();
        let has_backdrop = self
            .backdrop_image_tags
            .as_ref()
            .is_some_and(|tags| !tags.is_empty());
        let user = self.user_data.unwrap_or_default();
        Some(LibraryItem {
            id: self.id,
            kind,
            name: self.name,
            overview: self.overview,
            year: self.production_year,
            status: self.status,
            genres: self.genres.unwrap_or_default(),
            community_rating: self.community_rating,
            runtime_secs: self.run_time_ticks.map(ticks_to_secs),
            provider_ids: provider_map(self.provider_ids),
            has_poster,
            has_backdrop,
            played: user.played,
            unplayed_count: user.unplayed_item_count,
            position_secs: ticks_to_secs(user.playback_position_ticks),
        })
    }

    pub fn into_episode(self, series_id: &str) -> Episode {
        let has_image = self.has_primary();
        let user = self.user_data.unwrap_or_default();
        Episode {
            id: self.id,
            series_id: series_id.to_owned(),
            series_name: self.series_name.unwrap_or_default(),
            season_number: self.parent_index_number.unwrap_or(1),
            number: self.index_number.unwrap_or(0),
            name: self.name,
            overview: self.overview,
            runtime_secs: self.run_time_ticks.map(ticks_to_secs),
            played: user.played,
            position_secs: ticks_to_secs(user.playback_position_ticks),
            has_image,
        }
    }

    /// Episoden, Filme und Serien können auf der Startseite stehen, alles andere nicht.
    pub fn into_shelf_item(self) -> Option<ShelfItem> {
        let is_episode = match self.item_type.as_deref() {
            Some("Episode") => true,
            Some("Movie" | "Series") => false,
            _ => return None,
        };
        let library_id = if is_episode {
            self.series_id.clone()?
        } else {
            self.id.clone()
        };
        let has_image = self.has_primary();
        let user = self.user_data.unwrap_or_default();
        let (title, subtitle) = if is_episode {
            let subtitle = format!(
                "S{}:E{} · {}",
                self.parent_index_number.unwrap_or(1),
                self.index_number.unwrap_or(0),
                self.name
            );
            (self.series_name.unwrap_or(self.name), Some(subtitle))
        } else {
            (self.name, None)
        };
        Some(ShelfItem {
            id: self.id,
            library_id,
            is_episode,
            title,
            subtitle,
            overview: self.overview,
            year: self.production_year,
            runtime_secs: self.run_time_ticks.map(ticks_to_secs),
            position_secs: ticks_to_secs(user.playback_position_ticks),
            has_image,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct FiltersDto {
    pub genres: Option<Vec<String>>,
    pub years: Option<Vec<i32>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct PlaybackInfoDto {
    pub media_sources: Vec<MediaSourceDto>,
    pub play_session_id: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct MediaSourceDto {
    pub id: String,
    #[serde(rename = "ETag")]
    pub e_tag: Option<String>,
    pub supports_direct_play: bool,
    pub supports_direct_stream: bool,
    pub transcoding_url: Option<String>,
    pub media_streams: Vec<MediaStreamDto>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct MediaStreamDto {
    #[serde(rename = "Type")]
    pub stream_type: Option<String>,
    pub index: u32,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub display_title: Option<String>,
    pub is_external: bool,
    pub delivery_method: Option<String>,
    pub delivery_url: Option<String>,
}
