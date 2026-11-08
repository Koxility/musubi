//! Zuordnung von Jellyfin-Titeln zu AniList: automatisch, zur Prüfung und von Hand.

use std::sync::atomic::{AtomicBool, Ordering};

use musubi_anilist::Error as AniListError;
use musubi_domain::{
    ItemKind, LibraryItem, LibraryQuery, Mapping, MappingSource, Media, MediaKind,
};
use musubi_store::{ReviewCandidate, ReviewItem};

use crate::inner::now;
use crate::matching::{
    ChainLink, MatchQuery, SeasonSpan, normalize_title, plan_chain, rank, should_auto_accept,
};
use crate::{Core, CoreEvent, Result};

const SERIES_FORMATS: [&str; 5] = ["TV", "TV_SHORT", "ONA", "OVA", "SPECIAL"];
const MOVIE_FORMATS: [&str; 1] = ["MOVIE"];
const CHAIN_FORMATS: [&str; 3] = ["TV", "TV_SHORT", "ONA"];
const MAX_CHAIN: usize = 10;
const MAX_REVIEW_CANDIDATES: usize = 8;

static MATCH_RUNNING: AtomicBool = AtomicBool::new(false);

struct MatchRunGuard;

impl Drop for MatchRunGuard {
    fn drop(&mut self) {
        MATCH_RUNNING.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchOutcome {
    Mapped,
    NeedsReview,
}

impl Core {
    async fn seasons_of(&self, item: &LibraryItem) -> Result<Vec<SeasonSpan>> {
        if item.kind == ItemKind::Movie {
            return Ok(vec![SeasonSpan {
                season: 1,
                first_episode: 1,
                count: 1,
            }]);
        }
        let session = self.require_jellyfin()?;
        let episodes = session.client.episodes(&session.user_id, &item.id).await?;
        let mut spans: Vec<SeasonSpan> = Vec::new();
        for episode in episodes
            .iter()
            .filter(|e| e.season_number > 0 && e.number > 0)
        {
            match spans
                .last_mut()
                .filter(|s| s.season == episode.season_number)
            {
                Some(span) => span.count += 1,
                None => spans.push(SeasonSpan {
                    season: episode.season_number,
                    first_episode: episode.number,
                    count: 1,
                }),
            }
        }
        Ok(spans)
    }

    /// Folgt den Fortsetzungen, bis genug Episoden abgedeckt sind.
    async fn build_chain(&self, first: &Media, needed: u32) -> Result<Vec<ChainLink>> {
        let session = self.require_anilist()?;
        let mut chain = vec![link_of(first)];
        let mut covered = first.episodes.unwrap_or(needed);
        let mut current = first.id;
        while covered < needed && chain.len() < MAX_CHAIN {
            let (_, relations) = session.client.relations(current).await?;
            let next = relations.into_iter().find(|r| {
                r.relation == "SEQUEL"
                    && r.media
                        .format
                        .as_deref()
                        .is_some_and(|f| CHAIN_FORMATS.contains(&f))
            });
            let Some(next) = next else { break };
            covered += next.media.episodes.unwrap_or(needed);
            current = next.media.id;
            chain.push(link_of(&next.media));
        }
        Ok(chain)
    }

    async fn apply_mapping(
        &self,
        item: &LibraryItem,
        media: &Media,
        source: MappingSource,
        confidence: f64,
    ) -> Result<()> {
        let spans = self.seasons_of(item).await?;
        let needed = spans.iter().map(|s| s.count).sum();
        let chain = self.build_chain(media, needed).await?;
        let mappings = plan_chain(&item.id, &spans, &chain, source, confidence);
        self.inner.db.replace_mappings(&item.id, mappings).await?;
        self.inner.emit(CoreEvent::MappingsChanged);
        Ok(())
    }

    async fn media_from_provider_ids(&self, item: &LibraryItem) -> Result<Option<Media>> {
        let session = self.require_anilist()?;
        if let Some(id) = item.provider_id(&["AniList"]).and_then(|v| v.parse().ok()) {
            match session.client.media(id).await {
                Ok(media) => return Ok(Some(media)),
                Err(AniListError::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        if let Some(id) = item
            .provider_id(&["MyAnimeList", "Mal"])
            .and_then(|v| v.parse().ok())
        {
            match session.client.media_by_mal(id, MediaKind::Anime).await {
                Ok(media) => return Ok(Some(media)),
                Err(AniListError::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(None)
    }

    /// Ordnet einen Titel zu oder legt ihn mit Kandidaten auf die Prüfliste.
    pub async fn auto_match(&self, item: &LibraryItem) -> Result<MatchOutcome> {
        let session = self.require_anilist()?;
        if let Some(media) = self.media_from_provider_ids(item).await? {
            self.apply_mapping(item, &media, MappingSource::ProviderId, 1.0)
                .await?;
            return Ok(MatchOutcome::Mapped);
        }

        let formats: &[&str] = if item.kind == ItemKind::Movie {
            &MOVIE_FORMATS
        } else {
            &SERIES_FORMATS
        };
        let search = normalize_title(&item.name);
        let candidates = session
            .client
            .search(&search, MediaKind::Anime, Some(formats), 15)
            .await?;
        let episodes = match item.kind {
            ItemKind::Movie => None,
            ItemKind::Series => self.seasons_of(item).await?.first().map(|s| s.count),
        };
        let query = MatchQuery {
            title: &item.name,
            year: item.year,
            kind: item.kind,
            episodes,
        };
        let ranked = rank(&query, &candidates);

        if should_auto_accept(&ranked) {
            let (media, score) = ranked[0];
            self.apply_mapping(item, media, MappingSource::Title, score)
                .await?;
            return Ok(MatchOutcome::Mapped);
        }

        let review = ReviewItem {
            jellyfin_id: item.id.clone(),
            title: item.name.clone(),
            candidates: ranked
                .iter()
                .take(MAX_REVIEW_CANDIDATES)
                .map(|(media, score)| candidate_of(media, *score))
                .collect(),
        };
        self.inner.db.set_review(review, now()).await?;
        self.inner.emit(CoreEvent::MappingsChanged);
        Ok(MatchOutcome::NeedsReview)
    }

    /// Ordnet alle noch offenen Titel zu. Läuft pro Prozess nur einmal gleichzeitig.
    pub async fn match_all(&self) -> Result<()> {
        if MATCH_RUNNING.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let _guard = MatchRunGuard;
        let rows = self.library(&LibraryQuery::default()).await?;
        let pending: Vec<LibraryItem> = rows
            .into_iter()
            .filter(|r| r.mappings.is_empty() && !r.needs_review)
            .map(|r| r.item)
            .collect();
        let total = pending.len();
        self.inner.emit(CoreEvent::MatchProgress { done: 0, total });
        let mut mapped = 0;
        let mut review = 0;
        for (index, item) in pending.iter().enumerate() {
            match self.auto_match(item).await {
                Ok(MatchOutcome::Mapped) => mapped += 1,
                Ok(MatchOutcome::NeedsReview) => review += 1,
                Err(error) => tracing::warn!(item = %item.name, %error, "matching failed"),
            }
            self.inner.emit(CoreEvent::MatchProgress {
                done: index + 1,
                total,
            });
        }
        self.inner.emit(CoreEvent::LibraryChanged);
        self.inner.emit(CoreEvent::Notice {
            ok: true,
            message: format!("Matching finished: {mapped} mapped, {review} need review"),
        });
        Ok(())
    }

    pub async fn reviews(&self) -> Result<Vec<ReviewItem>> {
        Ok(self.inner.db.reviews().await?)
    }

    pub async fn all_mappings(&self) -> Result<Vec<Mapping>> {
        Ok(self.inner.db.all_mappings().await?)
    }

    pub async fn mappings_for(&self, jellyfin_id: &str) -> Result<Vec<Mapping>> {
        Ok(self.inner.db.mappings_for(jellyfin_id).await?)
    }

    /// Manuelle Zuordnung. Fortsetzungen werden auf Wunsch automatisch mit verteilt.
    pub async fn map_manually(
        &self,
        item: &LibraryItem,
        media_id: i64,
        follow_sequels: bool,
    ) -> Result<()> {
        let session = self.require_anilist()?;
        let media = session.client.media(media_id).await?;
        if follow_sequels {
            return self
                .apply_mapping(item, &media, MappingSource::Manual, 1.0)
                .await;
        }
        let spans = self.seasons_of(item).await?;
        let chain = [ChainLink {
            media_id: media.id,
            episodes: None,
            title: media.title.preferred().to_owned(),
        }];
        let mappings = plan_chain(&item.id, &spans, &chain, MappingSource::Manual, 1.0);
        self.inner.db.replace_mappings(&item.id, mappings).await?;
        self.inner.emit(CoreEvent::MappingsChanged);
        Ok(())
    }

    /// Setzt die Zuordnungen einer Serie exakt (Staffel, Bereich, Offset).
    pub async fn set_mappings(&self, jellyfin_id: &str, mappings: Vec<Mapping>) -> Result<()> {
        self.inner
            .db
            .replace_mappings(jellyfin_id, mappings)
            .await?;
        self.inner.emit(CoreEvent::MappingsChanged);
        Ok(())
    }

    pub async fn unmap(&self, jellyfin_id: &str) -> Result<()> {
        self.set_mappings(jellyfin_id, Vec::new()).await
    }
}

fn link_of(media: &Media) -> ChainLink {
    ChainLink {
        media_id: media.id,
        episodes: media.episodes,
        title: media.title.preferred().to_owned(),
    }
}

fn candidate_of(media: &Media, score: f64) -> ReviewCandidate {
    ReviewCandidate {
        media_id: media.id,
        title: media.title.preferred().to_owned(),
        format: media.format.clone(),
        year: media.season_year,
        episodes: media.episodes,
        score: score.min(1.0),
    }
}
