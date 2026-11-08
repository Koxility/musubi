//! Reine Sync-Regeln. Sie machen kein IO und sind deshalb vollständig tabellarisch getestet.

use musubi_domain::{EntryPatch, FuzzyDate, ListEntry, ListStatus};

#[derive(Debug, Clone, PartialEq)]
pub enum SyncDecision {
    /// AniList ist bereits aktuell.
    NoChange,
    Patch(EntryPatch),
    /// Automatisch nichts ändern, sondern den Nutzer informieren.
    Conflict(String),
}

/// Entscheidet, was auf AniList passiert, wenn Episode `episode` gesehen wurde.
pub fn decide_watched(
    media_id: i64,
    total: Option<u32>,
    entry: Option<&ListEntry>,
    episode: u32,
    today: FuzzyDate,
) -> SyncDecision {
    let is_final = total.is_some_and(|t| t > 0 && episode >= t);
    let mut patch = EntryPatch::new(media_id);
    patch.progress = Some(episode);

    let Some(entry) = entry else {
        patch.status = Some(if is_final {
            ListStatus::Completed
        } else {
            ListStatus::Current
        });
        patch.started_at = Some(today);
        if is_final {
            patch.completed_at = Some(today);
        }
        return SyncDecision::Patch(patch);
    };

    let started_if_empty = entry.started_at.is_empty().then_some(today);
    match entry.status {
        ListStatus::Paused | ListStatus::Dropped => SyncDecision::Conflict(format!(
            "{} is marked {:?} on AniList, progress not changed",
            entry.media.title.preferred(),
            entry.status
        )),
        ListStatus::Completed => SyncDecision::NoChange,
        ListStatus::Planning => {
            patch.status = Some(if is_final {
                ListStatus::Completed
            } else {
                ListStatus::Current
            });
            patch.started_at = started_if_empty;
            if is_final {
                patch.completed_at = Some(today);
            }
            SyncDecision::Patch(patch)
        }
        ListStatus::Current if episode <= entry.progress => SyncDecision::NoChange,
        ListStatus::Current => {
            patch.started_at = started_if_empty;
            if is_final {
                patch.status = Some(ListStatus::Completed);
                patch.completed_at = Some(today);
            }
            SyncDecision::Patch(patch)
        }
        ListStatus::Repeating if episode <= entry.progress => SyncDecision::NoChange,
        ListStatus::Repeating => {
            if is_final {
                patch.status = Some(ListStatus::Completed);
                patch.repeat = Some(entry.repeat + 1);
                patch.completed_at = Some(today);
            }
            SyncDecision::Patch(patch)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SyncDirection {
    /// Jellyfin-Fortschritt nach AniList übertragen.
    Push,
    /// AniList-Fortschritt nach Jellyfin übertragen.
    Pull,
    Skip,
}

/// Eine Abweichung zwischen Jellyfin und AniList.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncRow {
    pub series_id: String,
    pub series_title: String,
    pub media_id: i64,
    pub media_title: String,
    pub jellyfin_progress: u32,
    pub anilist_progress: u32,
    pub total: Option<u32>,
    pub status: Option<ListStatus>,
    pub direction: SyncDirection,
}

/// Vorgeschlagene Richtung. `None`, wenn beide Seiten übereinstimmen.
pub fn suggest_direction(jellyfin: u32, anilist: u32) -> Option<SyncDirection> {
    match jellyfin.cmp(&anilist) {
        std::cmp::Ordering::Greater => Some(SyncDirection::Push),
        std::cmp::Ordering::Less => Some(SyncDirection::Pull),
        std::cmp::Ordering::Equal => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use musubi_domain::Media;

    const TODAY: FuzzyDate = FuzzyDate {
        year: Some(2026),
        month: Some(11),
        day: Some(8),
    };
    const OLD: FuzzyDate = FuzzyDate {
        year: Some(2020),
        month: Some(1),
        day: Some(1),
    };

    fn entry(status: ListStatus, progress: u32) -> ListEntry {
        ListEntry {
            id: 1,
            media: Media {
                id: 5,
                ..Media::default()
            },
            status,
            score_raw: 0,
            progress,
            progress_volumes: 0,
            repeat: 2,
            priority: 0,
            private: false,
            hidden_from_status_lists: false,
            notes: String::new(),
            custom_lists: vec![],
            advanced_scores: vec![],
            started_at: OLD,
            completed_at: FuzzyDate::default(),
            updated_at: 0,
        }
    }

    fn patch(decision: SyncDecision) -> EntryPatch {
        match decision {
            SyncDecision::Patch(p) => p,
            other => panic!("expected patch, got {other:?}"),
        }
    }

    #[test]
    fn new_entry_starts_watching() {
        let p = patch(decide_watched(5, Some(12), None, 1, TODAY));
        assert_eq!(p.status, Some(ListStatus::Current));
        assert_eq!(p.progress, Some(1));
        assert_eq!(p.started_at, Some(TODAY));
        assert_eq!(p.completed_at, None);
    }

    #[test]
    fn new_entry_on_single_episode_movie_completes() {
        let p = patch(decide_watched(5, Some(1), None, 1, TODAY));
        assert_eq!(p.status, Some(ListStatus::Completed));
        assert_eq!(p.completed_at, Some(TODAY));
    }

    #[test]
    fn planning_becomes_current_and_keeps_existing_start_date() {
        let e = entry(ListStatus::Planning, 0);
        let p = patch(decide_watched(5, Some(12), Some(&e), 1, TODAY));
        assert_eq!(p.status, Some(ListStatus::Current));
        assert_eq!(p.started_at, None);
    }

    #[test]
    fn planning_sets_start_date_when_empty() {
        let mut e = entry(ListStatus::Planning, 0);
        e.started_at = FuzzyDate::default();
        let p = patch(decide_watched(5, Some(12), Some(&e), 1, TODAY));
        assert_eq!(p.started_at, Some(TODAY));
    }

    #[test]
    fn current_only_moves_forward() {
        let e = entry(ListStatus::Current, 5);
        assert_eq!(
            decide_watched(5, Some(12), Some(&e), 3, TODAY),
            SyncDecision::NoChange
        );
        assert_eq!(
            decide_watched(5, Some(12), Some(&e), 5, TODAY),
            SyncDecision::NoChange
        );
        let p = patch(decide_watched(5, Some(12), Some(&e), 6, TODAY));
        assert_eq!(p.progress, Some(6));
        assert_eq!(p.status, None);
    }

    #[test]
    fn last_episode_completes() {
        let e = entry(ListStatus::Current, 11);
        let p = patch(decide_watched(5, Some(12), Some(&e), 12, TODAY));
        assert_eq!(p.status, Some(ListStatus::Completed));
        assert_eq!(p.completed_at, Some(TODAY));
        assert_eq!(p.repeat, None);
    }

    #[test]
    fn unknown_total_never_completes() {
        let e = entry(ListStatus::Current, 11);
        assert_eq!(
            patch(decide_watched(5, None, Some(&e), 500, TODAY)).status,
            None
        );
    }

    #[test]
    fn repeating_increments_repeat_at_the_end() {
        let e = entry(ListStatus::Repeating, 11);
        let p = patch(decide_watched(5, Some(12), Some(&e), 12, TODAY));
        assert_eq!(p.status, Some(ListStatus::Completed));
        assert_eq!(p.repeat, Some(3));

        let e = entry(ListStatus::Repeating, 2);
        let p = patch(decide_watched(5, Some(12), Some(&e), 3, TODAY));
        assert_eq!((p.status, p.repeat), (None, None));
        assert_eq!(
            decide_watched(5, Some(12), Some(&e), 1, TODAY),
            SyncDecision::NoChange
        );
    }

    #[test]
    fn paused_and_dropped_are_conflicts() {
        for status in [ListStatus::Paused, ListStatus::Dropped] {
            let e = entry(status, 3);
            assert!(matches!(
                decide_watched(5, Some(12), Some(&e), 4, TODAY),
                SyncDecision::Conflict(_)
            ));
        }
    }

    #[test]
    fn completed_is_left_alone() {
        let e = entry(ListStatus::Completed, 12);
        assert_eq!(
            decide_watched(5, Some(12), Some(&e), 12, TODAY),
            SyncDecision::NoChange
        );
    }

    #[test]
    fn direction_follows_the_side_that_is_ahead() {
        assert_eq!(suggest_direction(5, 3), Some(SyncDirection::Push));
        assert_eq!(suggest_direction(3, 5), Some(SyncDirection::Pull));
        assert_eq!(suggest_direction(3, 3), None);
    }
}
