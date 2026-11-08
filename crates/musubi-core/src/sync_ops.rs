//! Automatischer Sync beim Abspielen und manueller Abgleich.

use std::collections::{BTreeMap, HashMap};

use chrono::Local;
use musubi_domain::{
    FuzzyDate, ItemKind, LibraryItem, LibraryQuery, ListEntry, Mapping, Media, resolve_episode,
};

use crate::anilist_ops::placeholder_entry;
use crate::inner::now;
use crate::sync::{SyncDecision, SyncDirection, SyncRow, decide_watched, suggest_direction};
use crate::{Core, CoreEvent, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct SyncReport {
    pub pushed: u32,
    pub pulled: u32,
    pub conflicts: u32,
    pub failed: u32,
}

fn today() -> FuzzyDate {
    Local::now().date_naive().into()
}

impl Core {
    async fn media_for_decision(&self, media_id: i64) -> Result<Media> {
        if let Some(entry) = self.inner.entry_for(media_id) {
            return Ok(entry.media);
        }
        let session = self.require_anilist()?;
        Ok(session.client.media(media_id).await?)
    }

    async fn notice(&self, ok: bool, message: String) -> Result<()> {
        self.inner.db.log_sync(now(), ok, &message).await?;
        self.inner.emit(CoreEvent::Notice { ok, message });
        Ok(())
    }

    /// Wendet die Sync-Regeln für eine gesehene AniList-Episode an.
    /// Gibt zurück, ob etwas eingereiht wurde. Konflikte landen im Protokoll.
    async fn apply_watched(&self, media_id: i64, episode: u32) -> Result<(bool, bool)> {
        let media = self.media_for_decision(media_id).await?;
        let entry = self.inner.entry_for(media_id);
        let decision = decide_watched(media_id, media.episodes, entry.as_ref(), episode, today());
        match decision {
            SyncDecision::NoChange => Ok((false, false)),
            SyncDecision::Conflict(message) => {
                self.notice(false, message).await?;
                Ok((false, true))
            }
            SyncDecision::Patch(patch) => {
                if entry.is_none() {
                    let status = patch.status.unwrap_or(musubi_domain::ListStatus::Current);
                    self.inner.upsert_entry(placeholder_entry(media, status));
                }
                self.inner.queue_patch(patch).await?;
                Ok((true, false))
            }
        }
    }

    /// Wird beim Erreichen der Gesehen-Schwelle aufgerufen.
    pub(crate) async fn on_episode_watched(
        &self,
        item: &LibraryItem,
        season: u32,
        episode: u32,
    ) -> Result<()> {
        if !self.inner.config().sync.auto_sync || self.inner.anilist().is_none() {
            return Ok(());
        }
        self.ensure_anime_list().await?;
        let mut mappings = self.inner.db.mappings_for(&item.id).await?;
        if mappings.is_empty() {
            self.auto_match(item).await?;
            mappings = self.inner.db.mappings_for(&item.id).await?;
        }
        match resolve_episode(&mappings, season, episode) {
            Some((media_id, anilist_episode)) => {
                self.apply_watched(media_id, anilist_episode).await?;
            }
            None => {
                self.notice(false, format!("{} is not mapped to AniList yet", item.name))
                    .await?;
            }
        }
        Ok(())
    }

    /// Fortschritt pro Werk aus Sicht von Jellyfin.
    async fn jellyfin_progress(
        &self,
        item: &LibraryItem,
        mappings: &[Mapping],
    ) -> Result<(BTreeMap<i64, u32>, Vec<(String, i64, u32, bool)>)> {
        let mut progress: BTreeMap<i64, u32> = BTreeMap::new();
        let mut episodes = Vec::new();
        for mapping in mappings {
            progress.entry(mapping.media_id).or_insert(0);
        }
        match item.kind {
            ItemKind::Movie => {
                if let Some(mapping) = mappings.first() {
                    if item.played {
                        progress.insert(mapping.media_id, 1);
                    }
                    episodes.push((item.id.clone(), mapping.media_id, 1, item.played));
                }
            }
            ItemKind::Series => {
                let session = self.require_jellyfin()?;
                for ep in session.client.episodes(&session.user_id, &item.id).await? {
                    let Some((media, number)) =
                        resolve_episode(mappings, ep.season_number, ep.number)
                    else {
                        continue;
                    };
                    if ep.played {
                        let best = progress.entry(media).or_insert(0);
                        *best = (*best).max(number);
                    }
                    episodes.push((ep.id, media, number, ep.played));
                }
            }
        }
        Ok((progress, episodes))
    }

    /// Stellt Jellyfin und AniList gegenüber, ohne etwas zu ändern.
    pub async fn sync_diff(&self) -> Result<Vec<SyncRow>> {
        self.ensure_anime_list().await?;
        let items = self.library(&LibraryQuery::default()).await?;
        let mut rows = Vec::new();
        for row in items.into_iter().filter(|r| !r.mappings.is_empty()) {
            let (progress, _) = self.jellyfin_progress(&row.item, &row.mappings).await?;
            for (media_id, jellyfin) in progress {
                let entry: Option<ListEntry> = self.inner.entry_for(media_id);
                let anilist = entry.as_ref().map_or(0, |e| e.progress);
                let Some(direction) = suggest_direction(jellyfin, anilist) else {
                    continue;
                };
                let title = row
                    .mappings
                    .iter()
                    .find(|m| m.media_id == media_id)
                    .map(|m| m.title.clone())
                    .unwrap_or_default();
                rows.push(SyncRow {
                    series_id: row.item.id.clone(),
                    series_title: row.item.name.clone(),
                    media_id,
                    media_title: title,
                    jellyfin_progress: jellyfin,
                    anilist_progress: anilist,
                    total: entry.as_ref().and_then(|e| e.media.episodes),
                    status: entry.as_ref().map(|e| e.status),
                    direction,
                });
            }
        }
        Ok(rows)
    }

    /// Führt den Abgleich mit den pro Zeile gewählten Richtungen aus.
    pub async fn apply_sync(&self, rows: &[SyncRow]) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        let items: HashMap<String, LibraryItem> = self
            .library(&LibraryQuery::default())
            .await?
            .into_iter()
            .map(|r| (r.item.id.clone(), r.item))
            .collect();

        for row in rows {
            match row.direction {
                SyncDirection::Skip => {}
                SyncDirection::Push => match self
                    .apply_watched(row.media_id, row.jellyfin_progress)
                    .await
                {
                    Ok((true, _)) => report.pushed += 1,
                    Ok((false, true)) => report.conflicts += 1,
                    Ok((false, false)) => {}
                    Err(error) => {
                        report.failed += 1;
                        self.notice(false, format!("{}: {error}", row.media_title))
                            .await?;
                    }
                },
                SyncDirection::Pull => {
                    let Some(item) = items.get(&row.series_id) else {
                        continue;
                    };
                    match self.pull_progress(item, row).await {
                        Ok(count) => report.pulled += count,
                        Err(error) => {
                            report.failed += 1;
                            self.notice(false, format!("{}: {error}", row.series_title))
                                .await?;
                        }
                    }
                }
            }
        }
        self.inner.emit(CoreEvent::LibraryChanged);
        Ok(report)
    }

    async fn pull_progress(&self, item: &LibraryItem, row: &SyncRow) -> Result<u32> {
        let session = self.require_jellyfin()?;
        let mappings = self.inner.db.mappings_for(&item.id).await?;
        let (_, episodes) = self.jellyfin_progress(item, &mappings).await?;
        let mut count = 0;
        for (id, media, number, played) in episodes {
            if media == row.media_id && number <= row.anilist_progress && !played {
                session
                    .client
                    .set_played(&session.user_id, &id, true)
                    .await?;
                count += 1;
            }
        }
        Ok(count)
    }
}
