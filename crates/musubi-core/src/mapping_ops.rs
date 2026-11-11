//! Zuordnung von Jellyfin-Titeln zu AniList: automatisch und nachträglich von Hand.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};

use musubi_anilist::Error as AniListError;
use musubi_domain::{
    ItemKind, LibraryItem, LibraryQuery, Mapping, MappingSource, Media, MediaKind, Season,
    is_unsure,
};
use musubi_store::{ReviewCandidate, ReviewItem};

use crate::inner::now;
use crate::library::LibraryRow;
use crate::matching::{
    Carry, ChainLink, MIN_GUESS, MatchQuery, SMALL_DRIFT, SeasonSpan, Verdict, leading_surplus,
    needs_more, plan_chain, plan_season, rank, search_terms, season_match_title,
    season_search_terms, verdict,
};
use crate::{Core, CoreEvent, Result};

const SERIES_FORMATS: [&str; 5] = ["TV", "TV_SHORT", "ONA", "OVA", "SPECIAL"];
const MOVIE_FORMATS: [&str; 1] = ["MOVIE"];
const CHAIN_FORMATS: [&str; 3] = ["TV", "TV_SHORT", "ONA"];
const MAX_CHAIN: usize = 10;
const MAX_REVIEW_CANDIDATES: usize = 8;
/// So viele Fortsetzungen tief wird über Filme oder OVAs hinweg nach der nächsten Staffel gesucht.
const MAX_SEQUEL_DEPTH: usize = 3;
/// Obergrenze für Relations-Abfragen pro Suche, wegen des Rate-Limits.
const MAX_RELATION_LOOKUPS: usize = 6;
const META_MATCHING_VERSION: &str = "matching_version";
const META_TITLES_VERSION: &str = "mapping_titles_version";
/// Version 1 stores the English title of a match where AniList has one, not the Romaji one.
const TITLES_VERSION: u32 = 1;
/// Version 2 matches every season on its own. Version 3 lets one entry span seasons despite
/// wrong season IDs and skips an episode 0 kept in Specials. Version 4 no longer mistakes a
/// newest season that is missing episodes for that case. Older automatic mappings are
/// re-planned once, and titles left without a match are searched again once.
const MATCHING_VERSION: u32 = 4;

struct MatchRunGuard<'a>(&'a AtomicBool);

impl Drop for MatchRunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchOutcome {
    Mapped,
    Unsure,
    NoMatch,
}

/// Eine Jellyfin-Staffel: Episodenbereich und, falls bekannt, ihre Metadaten.
#[derive(Debug, Clone)]
struct SeasonInfo {
    span: SeasonSpan,
    meta: Option<Season>,
}

/// Das Werk, mit dem eine Staffel beginnt.
#[derive(Debug, Clone)]
struct Anchor {
    link: ChainLink,
    /// Episoden des Werks, die schon frühere Staffeln belegen.
    used: u32,
    source: MappingSource,
    confidence: f64,
}

impl Anchor {
    fn new(media: &Media, source: MappingSource, confidence: f64) -> Self {
        Self {
            link: link_of(media),
            used: 0,
            source,
            confidence,
        }
    }
}

/// Quelle einer Staffel, deren Werk über die Fortsetzungen der Vorstaffel gefunden wurde.
fn inherited(source: MappingSource, manual_run: bool) -> MappingSource {
    match source {
        MappingSource::Guess => MappingSource::Guess,
        MappingSource::Manual if manual_run => MappingSource::Manual,
        _ => MappingSource::Title,
    }
}

/// The AniList or MyAnimeList ID a metadata plugin wrote on the season itself.
fn season_provider_id(info: &SeasonInfo) -> Option<&str> {
    let meta = info.meta.as_ref()?;
    meta.provider_id(&["AniList"])
        .or_else(|| meta.provider_id(&["MyAnimeList", "Mal"]))
}

/// Season IDs that occur on more than one season. An ID names one entry, so a repeated one
/// was copied around by the plugin and says nothing about any of its seasons.
fn repeated_season_ids(seasons: &[SeasonInfo]) -> HashSet<&str> {
    let mut seen = HashSet::new();
    seasons
        .iter()
        .filter_map(season_provider_id)
        .filter(|id| !seen.insert(*id))
        .collect()
}

fn spans(seasons: &[SeasonInfo]) -> Vec<SeasonSpan> {
    seasons.iter().map(|s| s.span).collect()
}

/// Gesamturteil über alle regulären Staffeln eines Titels.
fn outcome_of(seasons: &[SeasonInfo], mappings: &[Mapping]) -> MatchOutcome {
    if mappings.is_empty() {
        return MatchOutcome::NoMatch;
    }
    let all_covered = seasons
        .iter()
        .filter(|s| s.span.season > 0)
        .all(|s| mappings.iter().any(|m| m.season == s.span.season));
    if all_covered && !is_unsure(mappings) {
        MatchOutcome::Mapped
    } else {
        MatchOutcome::Unsure
    }
}

impl Core {
    /// The seasons of a title including specials (season 0). A movie counts as season 1 with
    /// one episode.
    async fn seasons_of(&self, item: &LibraryItem) -> Result<Vec<SeasonInfo>> {
        if item.kind == ItemKind::Movie {
            return Ok(vec![SeasonInfo {
                span: SeasonSpan {
                    season: 1,
                    first_episode: 1,
                    count: 1,
                },
                meta: None,
            }]);
        }
        let session = self.require_jellyfin()?;
        let episodes = session.client.episodes(&session.user_id, &item.id).await?;
        let mut spans: Vec<SeasonSpan> = Vec::new();
        for episode in episodes.iter().filter(|e| e.number > 0) {
            match spans
                .last_mut()
                .filter(|s| s.season == episode.season_number)
            {
                // The length runs to the highest number, not over the files present: an
                // episode missing in between must not shift the seasons after it.
                Some(span) => {
                    span.count = span
                        .count
                        .max((episode.number + 1).saturating_sub(span.first_episode));
                }
                None => spans.push(SeasonSpan {
                    season: episode.season_number,
                    first_episode: episode.number,
                    count: 1,
                }),
            }
        }
        // The metadata helps, but matching works without it.
        let metas = match session.client.seasons(&session.user_id, &item.id).await {
            Ok(metas) => metas,
            Err(error) => {
                tracing::debug!(item = %item.name, %error, "season details unavailable");
                Vec::new()
            }
        };
        Ok(spans
            .into_iter()
            .map(|span| SeasonInfo {
                meta: metas.iter().find(|m| m.number == span.season).cloned(),
                span,
            })
            .collect())
    }

    async fn media_by_ids(
        &self,
        anilist: Option<&str>,
        mal: Option<&str>,
    ) -> Result<Option<Media>> {
        let session = self.require_anilist()?;
        if let Some(id) = anilist.and_then(|v| v.trim().parse().ok()) {
            match session.client.media(id).await {
                Ok(media) => return Ok(Some(media)),
                Err(AniListError::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        if let Some(id) = mal.and_then(|v| v.trim().parse().ok()) {
            match session.client.media_by_mal(id, MediaKind::Anime).await {
                Ok(media) => return Ok(Some(media)),
                Err(AniListError::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(None)
    }

    async fn media_from_provider_ids(&self, item: &LibraryItem) -> Result<Option<Media>> {
        self.media_by_ids(
            item.provider_id(&["AniList"]),
            item.provider_id(&["MyAnimeList", "Mal"]),
        )
        .await
    }

    /// The entry a metadata plugin named on the season itself, unless the ID is `repeated`
    /// on other seasons or already taken.
    async fn season_provider_anchor(
        &self,
        info: &SeasonInfo,
        used: &HashSet<i64>,
        repeated: &HashSet<&str>,
    ) -> Result<Option<Anchor>> {
        let Some(meta) = &info.meta else {
            return Ok(None);
        };
        if season_provider_id(info).is_some_and(|id| repeated.contains(id)) {
            return Ok(None);
        }
        let media = self
            .media_by_ids(
                meta.provider_id(&["AniList"]),
                meta.provider_id(&["MyAnimeList", "Mal"]),
            )
            .await?;
        // Some plugins copy the series ID to every season. That one does not count here.
        Ok(media
            .filter(|m| !used.contains(&m.id))
            .map(|m| Anchor::new(&m, MappingSource::ProviderId, 1.0)))
    }

    /// Die nächste noch nicht vergebene TV-Fortsetzung. Filme, OVAs und Specials dazwischen
    /// werden übersprungen, ihre Fortsetzungen aber weiter verfolgt.
    async fn next_sequel(&self, media_id: i64, used: &HashSet<i64>) -> Result<Option<Media>> {
        let session = self.require_anilist()?;
        let mut queue = VecDeque::from([(media_id, 0usize)]);
        let mut seen = HashSet::from([media_id]);
        let mut lookups = 0;
        while let Some((id, depth)) = queue.pop_front() {
            if lookups == MAX_RELATION_LOOKUPS {
                break;
            }
            lookups += 1;
            let (_, relations) = session.client.relations(id).await?;
            let sequels: Vec<Media> = relations
                .into_iter()
                .filter(|r| r.relation == "SEQUEL" && r.media.kind == MediaKind::Anime)
                .map(|r| r.media)
                .collect();
            if let Some(next) = sequels.iter().find(|m| {
                !used.contains(&m.id)
                    && m.format
                        .as_deref()
                        .is_some_and(|f| CHAIN_FORMATS.contains(&f))
            }) {
                return Ok(Some(next.clone()));
            }
            if depth + 1 < MAX_SEQUEL_DEPTH {
                for media in sequels {
                    if seen.insert(media.id) {
                        queue.push_back((media.id, depth + 1));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Werke für eine Staffel ab ihrem Anker. Weitere Cours kommen nur dazu, wenn mehr als ein
    /// paar Episoden fehlen.
    async fn season_chain(
        &self,
        anchor: &Anchor,
        count: u32,
        used: &mut HashSet<i64>,
    ) -> Result<Vec<ChainLink>> {
        let mut chain = vec![anchor.link.clone()];
        let mut covered = anchor.link.episodes.map(|e| e.saturating_sub(anchor.used));
        while needs_more(covered, count) && chain.len() < MAX_CHAIN {
            let last = chain.last().map_or(anchor.link.media_id, |l| l.media_id);
            let Some(next) = self.next_sequel(last, used).await? else {
                break;
            };
            used.insert(next.id);
            covered = covered.zip(next.episodes).map(|(c, e)| c + e);
            chain.push(link_of(&next));
        }
        Ok(chain)
    }

    /// Sucht eine einzelne Staffel auf AniList, z. B. „Titel Season 2“.
    async fn search_season(
        &self,
        item: &LibraryItem,
        info: &SeasonInfo,
        used: &HashSet<i64>,
    ) -> Result<Option<Anchor>> {
        let session = self.require_anilist()?;
        let name = info.meta.as_ref().map(|m| m.name.as_str());
        let mut candidates = Vec::new();
        for term in season_search_terms(&item.name, info.span.season, name) {
            candidates = session
                .client
                .search(&term, MediaKind::Anime, Some(&SERIES_FORMATS), 15)
                .await?;
            candidates.retain(|m| !used.contains(&m.id));
            if !candidates.is_empty() {
                break;
            }
        }
        let title = season_match_title(&item.name, info.span.season);
        let query = MatchQuery {
            title: &title,
            year: info.meta.as_ref().and_then(|m| m.year),
            kind: ItemKind::Series,
            episodes: Some(info.span.count),
        };
        let ranked = rank(&query, &candidates);
        let source = match verdict(&ranked) {
            Verdict::Sure => MappingSource::Title,
            Verdict::Unsure => MappingSource::Guess,
            Verdict::NoMatch => return Ok(None),
        };
        let (media, score) = ranked[0];
        Ok(Some(Anchor::new(media, source, score.min(1.0))))
    }

    /// Matches every season on its own. Seasons in `keep` stay as they are and give the next
    /// season its starting point. `start` applies to the first season.
    async fn plan_series(
        &self,
        item: &LibraryItem,
        seasons: &[SeasonInfo],
        mut start: Option<Anchor>,
        keep: &[Mapping],
        manual_run: bool,
    ) -> Result<Vec<Mapping>> {
        let mut out: Vec<Mapping> = Vec::new();
        let mut used: HashSet<i64> = keep.iter().map(|m| m.media_id).collect();
        let mut carry: Option<Carry> = None;
        // Last entry of the previous season with its source and confidence.
        let mut previous: Option<(i64, MappingSource, f64)> = None;
        let repeated = repeated_season_ids(seasons);
        let specials = seasons
            .iter()
            .find(|s| s.span.season == 0)
            .map_or(0, |s| s.span.count);
        let regular = seasons.iter().filter(|s| s.span.season > 0);
        let last_season = seasons.iter().map(|s| s.span.season).max();
        for (index, info) in regular.enumerate() {
            let span = info.span;
            let kept: Vec<Mapping> = keep
                .iter()
                .filter(|m| m.season == span.season)
                .cloned()
                .collect();
            if let Some(last) = kept.iter().max_by_key(|m| m.ep_start) {
                previous = Some((last.media_id, last.source, last.confidence));
                carry = None;
                out.extend(kept);
                continue;
            }

            let mut anchor = None;
            if index == 0
                && start
                    .as_ref()
                    .is_some_and(|a| a.source == MappingSource::Manual)
            {
                anchor = start.take();
            }
            let carried = |c: &Carry| {
                previous.map(|(_, source, confidence)| Anchor {
                    link: c.link.clone(),
                    used: c.used,
                    source: inherited(source, manual_run),
                    confidence,
                })
            };
            // An entry with room for the whole season goes on, whatever ID the season carries.
            if anchor.is_none() {
                anchor = carry.as_ref().filter(|c| c.fills(&span)).and_then(carried);
            }
            if anchor.is_none() {
                anchor = self.season_provider_anchor(info, &used, &repeated).await?;
            }
            if anchor.is_none() && index == 0 {
                anchor = start.take();
            }
            if anchor.is_none() {
                anchor = carry
                    .as_ref()
                    .filter(|c| c.continues_into(&span))
                    .and_then(carried);
            }
            if anchor.is_none()
                && let Some((media_id, source, confidence)) = previous
                && let Some(next) = self.next_sequel(media_id, &used).await?
            {
                anchor = Some(Anchor::new(
                    &next,
                    inherited(source, manual_run),
                    confidence,
                ));
            }
            if anchor.is_none() {
                anchor = self.search_season(item, info, &used).await?;
            }
            let Some(anchor) = anchor else {
                // Only this season stays open. The next one searches for itself instead of
                // continuing from a wrong predecessor.
                carry = None;
                previous = None;
                continue;
            };

            used.insert(anchor.link.media_id);
            let chain = self.season_chain(&anchor, span.count, &mut used).await?;
            let followed = last_season.is_some_and(|last| last > span.season);
            let skipped = leading_surplus(&chain, anchor.used, span.count, specials, followed);
            let plan = plan_season(
                &item.id,
                &span,
                &chain,
                anchor.used + skipped,
                anchor.source,
                anchor.confidence,
            );
            let mut source = anchor.source;
            let mut mappings = plan.mappings;
            // Many episodes left without an entry mean that something does not fit.
            if plan.unmapped > SMALL_DRIFT && source != MappingSource::Manual {
                source = MappingSource::Guess;
                for mapping in &mut mappings {
                    mapping.source = source;
                }
            }
            previous = mappings
                .last()
                .map(|m| (m.media_id, source, anchor.confidence));
            carry = plan.carry;
            out.extend(mappings);
        }
        Ok(out)
    }

    /// Plant alle Zuordnungen eines Titels ab einem Startwerk.
    async fn plan_from(
        &self,
        item: &LibraryItem,
        seasons: &[SeasonInfo],
        start: Option<Anchor>,
        keep: &[Mapping],
        manual_run: bool,
    ) -> Result<Vec<Mapping>> {
        if item.kind == ItemKind::Movie {
            let Some(start) = start else {
                return Ok(Vec::new());
            };
            return Ok(plan_chain(
                &item.id,
                &spans(seasons),
                &[start.link],
                start.source,
                start.confidence,
            ));
        }
        self.plan_series(item, seasons, start, keep, manual_run)
            .await
    }

    /// Sucht den Titel selbst auf AniList und bewertet die Kandidaten.
    async fn search_title(
        &self,
        item: &LibraryItem,
        seasons: &[SeasonInfo],
    ) -> Result<(Option<Anchor>, ReviewItem)> {
        let session = self.require_anilist()?;
        let formats: &[&str] = if item.kind == ItemKind::Movie {
            &MOVIE_FORMATS
        } else {
            &SERIES_FORMATS
        };
        let mut candidates = Vec::new();
        for term in search_terms(&item.name) {
            candidates = session
                .client
                .search(&term, MediaKind::Anime, Some(formats), 15)
                .await?;
            if !candidates.is_empty() {
                break;
            }
        }
        let episodes = match item.kind {
            ItemKind::Movie => None,
            ItemKind::Series => seasons
                .iter()
                .find(|s| s.span.season > 0)
                .map(|s| s.span.count),
        };
        let query = MatchQuery {
            title: &item.name,
            year: item.year,
            kind: item.kind,
            episodes,
        };
        let ranked = rank(&query, &candidates);
        let source = match verdict(&ranked) {
            Verdict::Sure => Some(MappingSource::Title),
            Verdict::Unsure => Some(MappingSource::Guess),
            Verdict::NoMatch => None,
        };
        let anchor = source.map(|source| {
            let (media, score) = ranked[0];
            Anchor::new(media, source, score.min(1.0))
        });
        let review = ReviewItem {
            jellyfin_id: item.id.clone(),
            title: item.name.clone(),
            candidates: ranked
                .iter()
                .take(MAX_REVIEW_CANDIDATES)
                .map(|(media, score)| candidate_of(media, *score))
                .collect(),
        };
        Ok((anchor, review))
    }

    /// Ordnet einen Titel zu, bei Serien jede Staffel für sich. `keep` sind Zuordnungen, die
    /// bleiben (manuelle Staffeln), `start` ein bereits bekanntes Werk für die erste Staffel.
    async fn match_title(
        &self,
        item: &LibraryItem,
        seasons: &[SeasonInfo],
        keep: &[Mapping],
        start: Option<Anchor>,
    ) -> Result<MatchOutcome> {
        let first_regular = seasons.iter().find(|s| s.span.season > 0);
        // Fängt die Bibliothek erst mit einer späteren Staffel an, passt die Suche nach dem
        // Serientitel nicht. Die Staffel sucht dann selbst.
        let title_fits_first = first_regular.is_none_or(|s| s.span.season == 1)
            && !first_regular.is_some_and(|s| keep.iter().any(|m| m.season == s.span.season));
        let mut review = None;
        let start = match start {
            Some(start) => Some(start),
            None if !title_fits_first => None,
            None => match self.media_from_provider_ids(item).await? {
                Some(media) => Some(Anchor::new(&media, MappingSource::ProviderId, 1.0)),
                None => {
                    let (anchor, found) = self.search_title(item, seasons).await?;
                    review = Some(found);
                    anchor
                }
            },
        };
        let mappings = self.plan_from(item, seasons, start, keep, false).await?;
        let outcome = outcome_of(seasons, &mappings);
        self.inner.db.replace_mappings(&item.id, mappings).await?;
        // Erst nach `replace_mappings`, das gespeicherte Kandidaten entfernt. Ohne Treffer merkt
        // sich ein leerer Eintrag, dass der Titel schon versucht wurde.
        let review = review.or_else(|| {
            (outcome == MatchOutcome::NoMatch).then(|| ReviewItem {
                jellyfin_id: item.id.clone(),
                title: item.name.clone(),
                candidates: Vec::new(),
            })
        });
        if let Some(review) = review {
            self.inner.db.set_review(review, now()).await?;
        }
        self.inner.emit(CoreEvent::MappingsChanged);
        Ok(outcome)
    }

    /// Ordnet einen Titel seinem besten Treffer zu, bei Serien jede Staffel einzeln. Die
    /// Kandidaten bleiben für spätere Änderungen gespeichert und merken zugleich, dass der Titel
    /// schon versucht wurde.
    pub async fn auto_match(&self, item: &LibraryItem) -> Result<MatchOutcome> {
        let seasons = self.seasons_of(item).await?;
        self.match_title(item, &seasons, &[], None).await
    }

    /// Plant eine Serie aus älteren Versionen pro Staffel neu. Manuelle Staffeln bleiben, die
    /// erste Staffel behält ihr Werk. `None`, wenn es nur eine Staffel gibt.
    async fn rematch_by_season(
        &self,
        item: &LibraryItem,
        current: &[Mapping],
    ) -> Result<Option<MatchOutcome>> {
        let seasons = self.seasons_of(item).await?;
        let regular: Vec<&SeasonInfo> = seasons.iter().filter(|s| s.span.season > 0).collect();
        let Some(first) = regular.first().filter(|_| regular.len() >= 2) else {
            return Ok(None);
        };
        let keep: Vec<Mapping> = current
            .iter()
            .filter(|m| m.source == MappingSource::Manual)
            .cloned()
            .collect();
        let previous = current
            .iter()
            .filter(|m| m.season == first.span.season && m.source != MappingSource::Manual)
            .min_by_key(|m| m.ep_start);
        let start = match previous {
            Some(mapping) => {
                let session = self.require_anilist()?;
                let media = session.client.media(mapping.media_id).await?;
                let used = (i64::from(mapping.ep_start) + i64::from(mapping.offset) - 1).max(0);
                Some(Anchor {
                    used: u32::try_from(used).unwrap_or(0),
                    ..Anchor::new(&media, mapping.source, mapping.confidence)
                })
            }
            None => None,
        };
        self.match_title(item, &seasons, &keep, start)
            .await
            .map(Some)
    }

    /// Übernimmt Titel, die in früheren Versionen auf der Prüfliste gelandet sind.
    async fn adopt_open_reviews(&self, rows: &[LibraryRow]) -> Result<HashSet<String>> {
        let session = self.require_anilist()?;
        let mut adopted = HashSet::new();
        for review in self.inner.db.reviews().await? {
            let Some(row) = rows
                .iter()
                .find(|r| r.item.id == review.jellyfin_id && r.mappings.is_empty())
            else {
                continue;
            };
            let Some((media_id, score)) = review
                .candidates
                .first()
                .map(|c| (c.media_id, c.score))
                .filter(|(_, score)| *score >= MIN_GUESS)
            else {
                continue;
            };
            let applied = async {
                let media = session.client.media(media_id).await?;
                let seasons = self.seasons_of(&row.item).await?;
                let start = Anchor::new(&media, MappingSource::Guess, score);
                let mappings = self
                    .plan_from(&row.item, &seasons, Some(start), &[], false)
                    .await?;
                self.inner
                    .db
                    .replace_mappings(&row.item.id, mappings)
                    .await?;
                Result::Ok(())
            };
            match applied.await {
                Ok(()) => {
                    adopted.insert(row.item.id.clone());
                    self.inner.db.set_review(review, now()).await?;
                }
                Err(error) => tracing::warn!(item = %row.item.name, %error, "matching failed"),
            }
        }
        if !adopted.is_empty() {
            self.inner.emit(CoreEvent::MappingsChanged);
        }
        Ok(adopted)
    }

    async fn matching_version(&self) -> Result<u32> {
        Ok(self
            .inner
            .db
            .meta_get(META_MATCHING_VERSION)
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    /// Matches all titles still open. Only one run at a time per core.
    ///
    /// Titles without a match are only searched again with `retry_unmatched`, or once after
    /// an update that changed the matching rules. Without a change
    /// there is no `LibraryChanged`, because the interface starts this run itself. After an
    /// update, automatic mappings of series with several seasons are re-planned once, season
    /// by season.
    pub async fn match_all(&self, retry_unmatched: bool) -> Result<()> {
        // Not worth failing the run for: offline, the titles are refreshed next time.
        if let Err(error) = self.refresh_mapping_titles().await {
            tracing::warn!(%error, "could not refresh the titles of matches");
        }
        if self.inner.matching.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let _guard = MatchRunGuard(&self.inner.matching);
        let rows = self.library(&LibraryQuery::default()).await?;
        let adopted = self.adopt_open_reviews(&rows).await?;
        let reviews = self.inner.db.reviews().await?;
        let outdated = self.matching_version().await? < MATCHING_VERSION;
        // Newer rules may find what the old ones did not, so they get one go at every title
        // that was left without a match.
        let tried: HashSet<String> = if retry_unmatched || outdated {
            HashSet::new()
        } else {
            reviews.iter().map(|r| r.jellyfin_id.clone()).collect()
        };
        let upgrade: Vec<&LibraryRow> = if outdated {
            rows.iter()
                .filter(|r| {
                    r.item.kind == ItemKind::Series
                        && !adopted.contains(&r.item.id)
                        && r.mappings.iter().any(|m| m.source != MappingSource::Manual)
                })
                .collect()
        } else {
            Vec::new()
        };
        let pending: Vec<&LibraryItem> = rows
            .iter()
            .filter(|r| {
                r.mappings.is_empty()
                    && !adopted.contains(&r.item.id)
                    && !tried.contains(&r.item.id)
            })
            .map(|r| &r.item)
            .collect();
        if pending.is_empty() && adopted.is_empty() && upgrade.is_empty() {
            self.inner
                .db
                .meta_set(META_MATCHING_VERSION, &MATCHING_VERSION.to_string())
                .await?;
            return Ok(());
        }
        let total = pending.len() + upgrade.len();
        self.inner.emit(CoreEvent::MatchProgress { done: 0, total });
        let mut mapped = 0;
        let mut unsure = adopted.len();
        let mut unmatched = 0;
        let mut replanned = 0;
        let mut count = |outcome: &MatchOutcome| match outcome {
            MatchOutcome::Mapped => mapped += 1,
            MatchOutcome::Unsure => unsure += 1,
            MatchOutcome::NoMatch => unmatched += 1,
        };
        for (index, item) in pending.iter().enumerate() {
            match self.auto_match(item).await {
                Ok(outcome) => count(&outcome),
                Err(error) => tracing::warn!(item = %item.name, %error, "matching failed"),
            }
            self.inner.emit(CoreEvent::MatchProgress {
                done: index + 1,
                total,
            });
        }
        for (index, row) in upgrade.iter().enumerate() {
            match self.rematch_by_season(&row.item, &row.mappings).await {
                Ok(Some(_)) => replanned += 1,
                Ok(None) => {}
                Err(error) => tracing::warn!(item = %row.item.name, %error, "re-matching failed"),
            }
            // Gespeicherte Vorschläge bleiben für den Ändern-Dialog erhalten.
            if let Some(review) = reviews.iter().find(|r| r.jellyfin_id == row.item.id) {
                self.inner.db.set_review(review.clone(), now()).await?;
            }
            self.inner.emit(CoreEvent::MatchProgress {
                done: pending.len() + index + 1,
                total,
            });
        }
        self.inner
            .db
            .meta_set(META_MATCHING_VERSION, &MATCHING_VERSION.to_string())
            .await?;
        if mapped + unsure + replanned > 0 {
            self.inner.emit(CoreEvent::LibraryChanged);
        }
        let mut message = format!(
            "Matching finished: {mapped} matched, {unsure} unsure, {unmatched} without a match"
        );
        if replanned > 0 {
            message.push_str(&format!(", {replanned} re-matched season by season"));
        }
        self.inner.emit(CoreEvent::Notice { ok: true, message });
        Ok(())
    }

    /// Brings the titles stored with matches in line with [`MediaTitle::preferred`], once
    /// after an update that changed which title is shown. Titles on the user's lists are
    /// known already; the rest is fetched in batches.
    ///
    /// [`MediaTitle::preferred`]: musubi_domain::MediaTitle::preferred
    pub async fn refresh_mapping_titles(&self) -> Result<()> {
        let stored = self.inner.db.meta_get(META_TITLES_VERSION).await?;
        if stored.and_then(|v| v.parse::<u32>().ok()).unwrap_or(0) >= TITLES_VERSION {
            return Ok(());
        }
        let Some(session) = self.inner.anilist() else {
            return Ok(());
        };
        let mappings = self.inner.db.all_mappings().await?;
        let ids: HashSet<i64> = mappings.iter().map(|m| m.media_id).collect();
        let mut titles: Vec<(i64, String)> = Vec::new();
        let mut unknown: Vec<i64> = Vec::new();
        for id in ids {
            match self.inner.entry_for(id) {
                Some(entry) => titles.push((id, entry.media.title.preferred().to_owned())),
                None => unknown.push(id),
            }
        }
        for chunk in unknown.chunks(musubi_anilist::MEDIA_PAGE) {
            let fetched = session.client.media_many(chunk).await?;
            titles.extend(
                fetched
                    .iter()
                    .map(|media| (media.id, media.title.preferred().to_owned())),
            );
        }
        let mut changed = false;
        for (id, title) in titles {
            let outdated = mappings
                .iter()
                .any(|m| m.media_id == id && m.title != title);
            if outdated {
                self.inner.db.set_mapping_title(id, &title).await?;
                changed = true;
            }
        }
        self.inner
            .db
            .meta_set(META_TITLES_VERSION, &TITLES_VERSION.to_string())
            .await?;
        if changed {
            self.inner.emit(CoreEvent::MappingsChanged);
        }
        Ok(())
    }

    pub async fn reviews(&self) -> Result<Vec<ReviewItem>> {
        Ok(self.inner.db.reviews().await?)
    }

    pub async fn all_mappings(&self) -> Result<Vec<Mapping>> {
        Ok(self.inner.db.all_mappings().await?)
    }

    /// The library title matched to an AniList entry, with the season if the entry belongs
    /// to a single one. `None` when nothing in the library is matched to it.
    pub async fn library_item_for(
        &self,
        media_id: i64,
    ) -> Result<Option<(LibraryItem, Option<u32>)>> {
        let Some((id, season)) = self.inner.db.jellyfin_for_media(media_id).await? else {
            return Ok(None);
        };
        Ok(Some((self.library_item(&id).await?, season)))
    }

    pub async fn mappings_for(&self, jellyfin_id: &str) -> Result<Vec<Mapping>> {
        Ok(self.inner.db.mappings_for(jellyfin_id).await?)
    }

    /// Manuelle Zuordnung für alle Staffeln. Fortsetzungen werden auf Wunsch mit verteilt.
    pub async fn map_manually(
        &self,
        item: &LibraryItem,
        media_id: i64,
        follow_sequels: bool,
    ) -> Result<()> {
        self.map_season_manually(item, None, media_id, follow_sequels)
            .await
    }

    /// Manuelle Zuordnung eines Werks. Mit `season` ändert sich nur diese Staffel, die übrigen
    /// bleiben, wie sie sind. `follow` verteilt bei allen Staffeln die Fortsetzungen auf spätere
    /// Staffeln, bei einer Staffel weitere Cours innerhalb dieser Staffel.
    pub async fn map_season_manually(
        &self,
        item: &LibraryItem,
        season: Option<u32>,
        media_id: i64,
        follow: bool,
    ) -> Result<()> {
        let session = self.require_anilist()?;
        let media = session.client.media(media_id).await?;
        let seasons = self.seasons_of(item).await?;
        let anchor = Anchor::new(&media, MappingSource::Manual, 1.0);
        let open = ChainLink {
            episodes: None,
            ..link_of(&media)
        };
        let mappings = match season {
            None if follow && item.kind == ItemKind::Series => {
                self.plan_series(item, &seasons, Some(anchor), &[], true)
                    .await?
            }
            None => {
                let regular: Vec<SeasonSpan> = spans(&seasons)
                    .into_iter()
                    .filter(|s| s.season > 0)
                    .collect();
                plan_chain(&item.id, &regular, &[open], MappingSource::Manual, 1.0)
            }
            Some(number) => {
                let span = seasons
                    .iter()
                    .find(|s| s.span.season == number)
                    .map(|s| s.span)
                    .unwrap_or(SeasonSpan {
                        season: number,
                        first_episode: 1,
                        count: media.episodes.unwrap_or(1),
                    });
                let mut kept: Vec<Mapping> = self
                    .inner
                    .db
                    .mappings_for(&item.id)
                    .await?
                    .into_iter()
                    .filter(|m| m.season != number)
                    .collect();
                let chain = if follow {
                    let mut used: HashSet<i64> = kept.iter().map(|m| m.media_id).collect();
                    used.insert(media.id);
                    self.season_chain(&anchor, span.count, &mut used).await?
                } else {
                    vec![open]
                };
                let plan = plan_season(&item.id, &span, &chain, 0, MappingSource::Manual, 1.0);
                kept.extend(plan.mappings);
                kept
            }
        };
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

    /// Macht aus unsicheren Treffern feste Zuordnungen, mit `season` nur für diese Staffel.
    pub async fn confirm_season(&self, jellyfin_id: &str, season: Option<u32>) -> Result<()> {
        let mut mappings = self.inner.db.mappings_for(jellyfin_id).await?;
        for mapping in mappings
            .iter_mut()
            .filter(|m| season.is_none_or(|s| m.season == s))
        {
            mapping.source = MappingSource::Manual;
            mapping.confidence = 1.0;
        }
        self.set_mappings(jellyfin_id, mappings).await
    }

    /// Macht aus einem unsicheren Treffer eine feste Zuordnung.
    pub async fn confirm_mapping(&self, jellyfin_id: &str) -> Result<()> {
        self.confirm_season(jellyfin_id, None).await
    }

    /// Entfernt die Zuordnung einer Staffel. Bleibt keine übrig, gilt das wie `unmap`.
    pub async fn unmap_season(&self, jellyfin_id: &str, season: u32) -> Result<()> {
        let rest: Vec<Mapping> = self
            .inner
            .db
            .mappings_for(jellyfin_id)
            .await?
            .into_iter()
            .filter(|m| m.season != season)
            .collect();
        if rest.is_empty() {
            return self.unmap(jellyfin_id).await;
        }
        self.set_mappings(jellyfin_id, rest).await
    }

    /// Entfernt die Zuordnung. Der Titel gilt als versucht, damit der nächste automatische Lauf
    /// die Entscheidung nicht rückgängig macht.
    pub async fn unmap(&self, jellyfin_id: &str) -> Result<()> {
        let title = self
            .inner
            .db
            .mappings_for(jellyfin_id)
            .await?
            .first()
            .map(|m| m.title.clone())
            .unwrap_or_default();
        self.inner
            .db
            .replace_mappings(jellyfin_id, Vec::new())
            .await?;
        let tried = ReviewItem {
            jellyfin_id: jellyfin_id.to_owned(),
            title,
            candidates: Vec::new(),
        };
        self.inner.db.set_review(tried, now()).await?;
        self.inner.emit(CoreEvent::MappingsChanged);
        Ok(())
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
