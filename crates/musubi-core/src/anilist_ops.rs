//! AniList-Verwaltung, unabhängig von Jellyfin.

use musubi_domain::{
    EntryPatch, FuzzyDate, ListEntry, ListStatus, Media, MediaKind, Profile, Viewer,
};
use musubi_store::OutboxOp;

use crate::inner::{CACHE_PROFILE, CACHE_VIEWER, Inner, list_cache_key, now};
use crate::{Core, CoreError, CoreEvent, Result};

/// Lokaler Platzhalter, bis AniList die echte Eintrags-ID liefert.
pub(crate) fn placeholder_entry(media: Media, status: ListStatus) -> ListEntry {
    ListEntry {
        id: 0,
        media,
        status,
        score_raw: 0,
        progress: 0,
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
        updated_at: now(),
    }
}

impl Inner {
    pub async fn persist_list(&self, kind: MediaKind) -> Result<()> {
        let list = self.list(kind);
        self.db
            .cache_put(&list_cache_key(kind), &list, now())
            .await?;
        Ok(())
    }

    /// Fügt einen Eintrag ein oder ersetzt ihn (Schlüssel ist das Werk).
    pub fn upsert_entry(&self, entry: ListEntry) -> MediaKind {
        let kind = entry.media.kind;
        let mut lists = self.lists.write().unwrap_or_else(|e| e.into_inner());
        let list = lists.entry(kind).or_default();
        match list.iter_mut().find(|e| e.media.id == entry.media.id) {
            Some(slot) => *slot = entry,
            None => list.push(entry),
        }
        kind
    }

    pub fn remove_entry(&self, media_id: i64) -> Option<MediaKind> {
        let mut lists = self.lists.write().unwrap_or_else(|e| e.into_inner());
        for (kind, list) in lists.iter_mut() {
            if let Some(pos) = list.iter().position(|e| e.media.id == media_id) {
                list.remove(pos);
                return Some(*kind);
            }
        }
        None
    }

    /// Zeigt eine Änderung sofort lokal an und reiht sie zum Senden ein.
    pub async fn queue_patch(&self, patch: EntryPatch) -> Result<()> {
        let media_id = patch.media_id;
        if let Some(mut entry) = self.entry_for(media_id) {
            patch.apply_to(&mut entry);
            let kind = self.upsert_entry(entry);
            self.persist_list(kind).await?;
            self.emit(CoreEvent::ListChanged(kind));
        }
        self.db
            .outbox_enqueue(media_id, OutboxOp::Save(patch))
            .await?;
        self.outbox_wake.notify_one();
        Ok(())
    }
}

impl Core {
    /// Lokal gehaltene Liste (sofort verfügbar, auch offline).
    pub fn list(&self, kind: MediaKind) -> Vec<ListEntry> {
        self.inner.list(kind)
    }

    pub fn entry_for(&self, media_id: i64) -> Option<ListEntry> {
        self.inner.entry_for(media_id)
    }

    pub async fn refresh_viewer(&self) -> Result<Viewer> {
        let session = self.require_anilist()?;
        let viewer = session.client.viewer().await?;
        self.inner
            .db
            .cache_put(CACHE_VIEWER, &viewer, now())
            .await?;
        self.inner.set_viewer(Some(viewer.clone()));
        Ok(viewer)
    }

    /// Das zuletzt abgerufene Profil mit Statistik, auch offline verfügbar.
    pub async fn cached_profile(&self) -> Result<Option<Profile>> {
        Ok(self.inner.db.cache_get::<Profile>(CACHE_PROFILE).await?)
    }

    /// Holt Profil und Statistik neu von AniList und merkt sie sich für später.
    pub async fn refresh_profile(&self) -> Result<Profile> {
        let session = self.require_anilist()?;
        let mut profile = session.client.profile().await?;
        profile.fetched_at = now();
        self.inner
            .db
            .cache_put(CACHE_PROFILE, &profile, now())
            .await?;
        Ok(profile)
    }

    async fn viewer_or_fetch(&self) -> Result<Viewer> {
        match self.inner.viewer() {
            Some(viewer) => Ok(viewer),
            None => self.refresh_viewer().await,
        }
    }

    /// Holt die komplette Liste neu von AniList.
    pub async fn refresh_list(&self, kind: MediaKind) -> Result<Vec<ListEntry>> {
        let session = self.require_anilist()?;
        let viewer = self.viewer_or_fetch().await?;
        let mut entries = session.client.list(viewer.id, kind).await?;
        entries.sort_by(|a, b| a.media.title.preferred().cmp(b.media.title.preferred()));
        self.inner
            .lists
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(kind, entries.clone());
        self.inner.persist_list(kind).await?;
        self.inner.emit(CoreEvent::ListChanged(kind));
        Ok(entries)
    }

    /// Stellt sicher, dass die Anime-Liste für Sync-Entscheidungen geladen ist.
    pub(crate) async fn ensure_anime_list(&self) -> Result<()> {
        if self.inner.list(MediaKind::Anime).is_empty() {
            self.refresh_list(MediaKind::Anime).await?;
        }
        Ok(())
    }

    pub async fn search_anilist(&self, text: &str, kind: MediaKind) -> Result<Vec<Media>> {
        let session = self.require_anilist()?;
        Ok(session.client.search(text, kind, None, 25).await?)
    }

    /// Speichert Änderungen an einem Eintrag. Offline bleiben sie in der Outbox.
    pub async fn save_entry(&self, patch: EntryPatch) -> Result<()> {
        self.require_anilist()?;
        self.inner.queue_patch(patch).await
    }

    /// Nimmt ein Werk neu auf die Liste.
    pub async fn add_to_list(&self, media: Media, status: ListStatus) -> Result<()> {
        self.require_anilist()?;
        let patch = EntryPatch {
            status: Some(status),
            ..EntryPatch::new(media.id)
        };
        if self.inner.entry_for(media.id).is_none() {
            self.inner.upsert_entry(placeholder_entry(media, status));
        }
        self.inner.queue_patch(patch).await
    }

    /// Puts a work the user only knows by its ID on their list, e.g. the match of a
    /// Jellyfin title that is not tracked yet. Returns the title it was added under.
    pub async fn add_media_to_list(&self, media_id: i64, status: ListStatus) -> Result<String> {
        let session = self.require_anilist()?;
        let media = session.client.media(media_id).await?;
        let title = media.title.preferred().to_owned();
        self.add_to_list(media, status).await?;
        Ok(title)
    }

    pub async fn delete_entry(&self, entry: &ListEntry) -> Result<()> {
        self.require_anilist()?;
        if entry.id == 0 {
            return Err(CoreError::Invalid(
                "this entry has not reached AniList yet, try again in a moment".to_owned(),
            ));
        }
        self.inner
            .db
            .outbox_enqueue(entry.media.id, OutboxOp::Delete { entry_id: entry.id })
            .await?;
        if let Some(kind) = self.inner.remove_entry(entry.media.id) {
            self.inner.persist_list(kind).await?;
            self.inner.emit(CoreEvent::ListChanged(kind));
        }
        self.inner.outbox_wake.notify_one();
        Ok(())
    }

    pub async fn create_custom_list(&self, kind: MediaKind, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::Invalid("list name must not be empty".to_owned()));
        }
        let session = self.require_anilist()?;
        let viewer = self.viewer_or_fetch().await?;
        let mut names = viewer.custom_lists(kind).to_vec();
        if names.iter().any(|n| n == name) {
            return Err(CoreError::Invalid(format!(
                "a list named `{name}` already exists"
            )));
        }
        names.push(name.to_owned());
        session.client.set_custom_lists(kind, &names).await?;
        self.refresh_viewer().await?;
        Ok(())
    }

    pub async fn delete_custom_list(&self, kind: MediaKind, name: &str) -> Result<()> {
        let session = self.require_anilist()?;
        let viewer = self.viewer_or_fetch().await?;
        let names: Vec<String> = viewer
            .custom_lists(kind)
            .iter()
            .filter(|n| *n != name)
            .cloned()
            .collect();
        session.client.set_custom_lists(kind, &names).await?;
        self.refresh_viewer().await?;
        self.refresh_list(kind).await?;
        Ok(())
    }

    /// AniList kennt kein Umbenennen: neue Liste anlegen, Einträge umziehen, alte entfernen.
    pub async fn rename_custom_list(&self, kind: MediaKind, old: &str, new: &str) -> Result<()> {
        let new = new.trim();
        if new.is_empty() {
            return Err(CoreError::Invalid("list name must not be empty".to_owned()));
        }
        let session = self.require_anilist()?;
        let viewer = self.viewer_or_fetch().await?;
        let mut names = viewer.custom_lists(kind).to_vec();
        if !names.iter().any(|n| n == old) {
            return Err(CoreError::Invalid(format!("no list named `{old}`")));
        }
        if names.iter().any(|n| n == new) {
            return Err(CoreError::Invalid(format!(
                "a list named `{new}` already exists"
            )));
        }
        names.push(new.to_owned());
        session.client.set_custom_lists(kind, &names).await?;

        for entry in self.refresh_list(kind).await? {
            if entry.custom_lists.iter().any(|l| l == old) {
                let lists: Vec<String> = entry
                    .custom_lists
                    .iter()
                    .map(|l| if l == old { new.to_owned() } else { l.clone() })
                    .collect();
                let patch = EntryPatch {
                    custom_lists: Some(lists),
                    ..EntryPatch::new(entry.media.id)
                };
                session.client.save_entry(&patch).await?;
            }
        }

        names.retain(|n| n != old);
        session.client.set_custom_lists(kind, &names).await?;
        self.refresh_viewer().await?;
        self.refresh_list(kind).await?;
        Ok(())
    }

    /// Offene und fehlgeschlagene Schreibzugriffe.
    pub async fn pending_writes(&self) -> Result<Vec<musubi_store::OutboxItem>> {
        Ok(self.inner.db.outbox_all().await?)
    }

    /// Stößt das Senden an und gibt fehlgeschlagene Einträge wieder frei.
    pub async fn flush_outbox(&self) -> Result<()> {
        self.inner.db.outbox_reset_failed().await?;
        self.inner.outbox_wake.notify_one();
        Ok(())
    }
}
