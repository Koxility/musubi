//! Bibliotheksansichten: Jellyfin-Daten, verknüpft mit Zuordnung und AniList-Status.

use std::collections::{HashMap, HashSet};

use musubi_domain::{
    Episode, Library, LibraryFilters, LibraryItem, LibraryQuery, ListEntry, Mapping, Season,
    ShelfItem, resolve_episode,
};

use serde::Serialize;

use crate::inner::now;
use crate::{Core, CoreError, Result};

const CACHE_LIBRARY: &str = "library.default";
const SHELF_LIMIT: u32 = 24;

/// Die Regale der Startseite.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct HomeShelves {
    pub continue_watching: Vec<ShelfItem>,
    pub next_up: Vec<ShelfItem>,
    pub latest: Vec<ShelfItem>,
}

/// Eine Zeile der Bibliothek, wie beide Oberflächen sie anzeigen.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LibraryRow {
    pub item: LibraryItem,
    pub mappings: Vec<Mapping>,
    /// AniList-Eintrag des ersten zugeordneten Werks.
    pub entry: Option<ListEntry>,
    pub needs_review: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EpisodeRow {
    pub episode: Episode,
    pub anilist: Option<(i64, u32)>,
    /// Die Episode ist auf AniList bereits als gesehen gezählt.
    pub counted_on_anilist: bool,
}

impl Core {
    /// Lädt Weiterschauen, Nächste Episoden und Zuletzt hinzugefügt.
    pub async fn home(&self) -> Result<HomeShelves> {
        let session = self.require_jellyfin()?;
        let (continue_watching, next_up, latest) = tokio::try_join!(
            session.client.resume(&session.user_id, SHELF_LIMIT),
            session.client.next_up(&session.user_id, SHELF_LIMIT),
            session.client.latest(&session.user_id, SHELF_LIMIT),
        )?;
        // Eine angefangene Episode steht nur unter Weiterschauen.
        let resumed: HashSet<&str> = continue_watching.iter().map(|i| i.id.as_str()).collect();
        let next_up = next_up
            .into_iter()
            .filter(|i| !resumed.contains(i.id.as_str()))
            .collect();
        Ok(HomeShelves {
            continue_watching,
            next_up,
            latest,
        })
    }

    /// Serie oder Film zu einem Regaleintrag.
    pub async fn library_item(&self, item_id: &str) -> Result<LibraryItem> {
        let session = self.require_jellyfin()?;
        session
            .client
            .item(&session.user_id, item_id)
            .await?
            .ok_or_else(|| CoreError::Invalid(format!("{item_id} is not a series or movie")))
    }

    pub async fn libraries(&self) -> Result<Vec<Library>> {
        let session = self.require_jellyfin()?;
        Ok(session.client.libraries(&session.user_id).await?)
    }

    pub async fn filters(&self, library_id: Option<&str>) -> Result<LibraryFilters> {
        let session = self.require_jellyfin()?;
        Ok(session.client.filters(&session.user_id, library_id).await?)
    }

    async fn rows(&self, items: Vec<LibraryItem>) -> Result<Vec<LibraryRow>> {
        let mut mappings: HashMap<String, Vec<Mapping>> = HashMap::new();
        for mapping in self.inner.db.all_mappings().await? {
            mappings
                .entry(mapping.jellyfin_id.clone())
                .or_default()
                .push(mapping);
        }
        let reviews: HashSet<String> = self
            .inner
            .db
            .reviews()
            .await?
            .into_iter()
            .map(|r| r.jellyfin_id)
            .collect();
        Ok(items
            .into_iter()
            .map(|item| {
                let mappings = mappings.remove(&item.id).unwrap_or_default();
                let entry = mappings
                    .first()
                    .and_then(|m| self.inner.entry_for(m.media_id));
                let needs_review = reviews.contains(&item.id);
                LibraryRow {
                    item,
                    mappings,
                    entry,
                    needs_review,
                }
            })
            .collect())
    }

    /// Lädt die Bibliothek vom Server. Eine ungefilterte Abfrage wird zwischengespeichert.
    pub async fn library(&self, query: &LibraryQuery) -> Result<Vec<LibraryRow>> {
        let session = self.require_jellyfin()?;
        let configured = self.inner.config().jellyfin.library_ids;
        let mut items = Vec::new();
        if query.library_id.is_none() && !configured.is_empty() {
            for id in configured {
                let mut scoped = query.clone();
                scoped.library_id = Some(id);
                items.extend(session.client.all_items(&session.user_id, &scoped).await?);
            }
        } else {
            items = session.client.all_items(&session.user_id, query).await?;
        }
        if *query == LibraryQuery::default() {
            self.inner
                .db
                .cache_put(CACHE_LIBRARY, &items, now())
                .await?;
        }
        self.rows(items).await
    }

    /// Letzter bekannter Stand für sofortige Anzeige beim Start und im Offline-Fall.
    pub async fn cached_library(&self) -> Result<Vec<LibraryRow>> {
        let items: Vec<LibraryItem> = self
            .inner
            .db
            .cache_get(CACHE_LIBRARY)
            .await?
            .unwrap_or_default();
        self.rows(items).await
    }

    pub async fn seasons(&self, series_id: &str) -> Result<Vec<Season>> {
        let session = self.require_jellyfin()?;
        Ok(session.client.seasons(&session.user_id, series_id).await?)
    }

    pub async fn episodes(&self, series_id: &str) -> Result<Vec<EpisodeRow>> {
        let session = self.require_jellyfin()?;
        let episodes = session.client.episodes(&session.user_id, series_id).await?;
        let mappings = self.inner.db.mappings_for(series_id).await?;
        Ok(episodes
            .into_iter()
            .map(|episode| {
                let anilist = resolve_episode(&mappings, episode.season_number, episode.number);
                let counted_on_anilist = anilist.is_some_and(|(media, ep)| {
                    self.inner
                        .entry_for(media)
                        .is_some_and(|entry| entry.progress >= ep)
                });
                EpisodeRow {
                    episode,
                    anilist,
                    counted_on_anilist,
                }
            })
            .collect())
    }

    pub async fn set_played(&self, item_id: &str, played: bool) -> Result<()> {
        let session = self.require_jellyfin()?;
        session
            .client
            .set_played(&session.user_id, item_id, played)
            .await?;
        self.inner.emit(crate::CoreEvent::LibraryChanged);
        Ok(())
    }
}
