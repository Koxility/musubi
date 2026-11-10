//! Steuert eine Wiedergabe: Warteschlange, Meldungen an Jellyfin, Auslösen des Sync.

use std::time::Duration;

use musubi_domain::{ItemKind, LibraryItem};
use musubi_jellyfin::{PlaybackTarget, ProgressReport};
use musubi_player::{
    EndReason, OpenRequest, PlayItem, PlayerCommand, PlayerEvent, SessionHandle, SubtitleTrack,
};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::inner::{ActivePlayback, JellyfinSession};
use crate::{Core, CoreError, CoreEvent, PlaybackState, Result};

const REPORT_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
struct QueueEntry {
    id: String,
    season: u32,
    number: u32,
    title: String,
    resume_secs: Option<f64>,
}

struct CurrentFile {
    index: usize,
    target: PlaybackTarget,
    position: f64,
    duration: Option<f64>,
    paused: bool,
    last_report: Instant,
    synced: bool,
}

struct Run {
    core: Core,
    jellyfin: JellyfinSession,
    session: SessionHandle,
    item: LibraryItem,
    queue: Vec<QueueEntry>,
    /// Bereits an den Player übergebene Ziele, Index entspricht der Warteschlange.
    targets: Vec<PlaybackTarget>,
    current: Option<CurrentFile>,
    threshold: f64,
    max_bitrate: Option<u64>,
    generation: u64,
}

fn play_item(entry: &QueueEntry, target: &PlaybackTarget) -> PlayItem {
    PlayItem {
        url: target.url.clone(),
        title: entry.title.clone(),
        start_secs: entry.resume_secs,
        subtitles: target
            .subtitles
            .iter()
            .map(|s| SubtitleTrack {
                url: s.url.clone(),
                title: s.title.clone(),
                language: s.language.clone(),
            })
            .collect(),
    }
}

impl Run {
    fn report(current: &CurrentFile) -> ProgressReport<'_> {
        ProgressReport {
            target: &current.target,
            position_secs: current.position,
            paused: current.paused,
        }
    }

    async fn report_progress(&mut self) {
        let Some(current) = &mut self.current else {
            return;
        };
        current.last_report = Instant::now();
        let current = &*current;
        if let Err(error) = self
            .jellyfin
            .client
            .report_progress(Self::report(current))
            .await
        {
            tracing::warn!(%error, "progress report failed");
        }
    }

    async fn finish_current(&mut self) {
        if let Some(current) = self.current.take()
            && let Err(error) = self
                .jellyfin
                .client
                .report_stopped(Self::report(&current))
                .await
        {
            tracing::warn!(%error, "stop report failed");
        }
    }

    /// Reiht die Folgeepisode ein, sobald die aktuelle läuft.
    async fn append_next(&mut self, index: usize) -> Result<()> {
        let next = index + 1;
        if next != self.targets.len() {
            return Ok(());
        }
        let Some(entry) = self.queue.get(next).cloned() else {
            return Ok(());
        };
        let target = self
            .jellyfin
            .client
            .playback_target(&self.jellyfin.user_id, &entry.id, self.max_bitrate)
            .await?;
        self.session.append(play_item(&entry, &target)).await?;
        self.targets.push(target);
        Ok(())
    }

    async fn on_started(&mut self, index: usize) {
        self.finish_current().await;
        let Some(target) = self.targets.get(index).cloned() else {
            return;
        };
        let position = self.queue[index].resume_secs.unwrap_or(0.0);
        let current = CurrentFile {
            index,
            target,
            position,
            duration: None,
            paused: false,
            last_report: Instant::now(),
            synced: false,
        };
        if let Err(error) = self
            .jellyfin
            .client
            .report_started(Self::report(&current))
            .await
        {
            tracing::warn!(%error, "start report failed");
        }
        self.current = Some(current);
        if let Err(error) = self.append_next(index).await {
            tracing::warn!(%error, "could not queue the next episode");
        }
    }

    fn publish(&self) {
        let Some(current) = &self.current else { return };
        let entry = &self.queue[current.index];
        self.core.inner.emit(CoreEvent::Playback(PlaybackState {
            item_id: entry.id.clone(),
            title: entry.title.clone(),
            position: current.position,
            duration: current.duration,
            paused: current.paused,
        }));
    }

    async fn mark_watched(&mut self) {
        let Some(current) = &mut self.current else {
            return;
        };
        if current.synced {
            return;
        }
        current.synced = true;
        let entry = self.queue[current.index].clone();
        let result = self
            .core
            .on_episode_watched(&self.item, entry.season, entry.number)
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "sync after playback failed");
            self.core.inner.emit(CoreEvent::Notice {
                ok: false,
                message: format!("Sync failed: {error}"),
            });
        }
    }

    async fn on_position(&mut self, position: f64, duration: Option<f64>, paused: bool) {
        let Some(current) = &mut self.current else {
            return;
        };
        current.position = position;
        current.duration = duration.or(current.duration);
        current.paused = paused;
        let reached = current
            .duration
            .is_some_and(|d| d > 0.0 && position / d >= self.threshold);
        let report_due = current.last_report.elapsed() >= REPORT_INTERVAL;
        self.publish();
        if report_due {
            self.report_progress().await;
        }
        if reached {
            self.mark_watched().await;
        }
    }

    async fn run(mut self, mut events: mpsc::Receiver<PlayerEvent>) {
        while let Some(event) = events.recv().await {
            match event {
                PlayerEvent::Started { index } => self.on_started(index).await,
                PlayerEvent::Position {
                    position,
                    duration,
                    paused,
                } => {
                    self.on_position(position, duration, paused).await;
                }
                PlayerEvent::Paused(paused) => {
                    if let Some(current) = &mut self.current {
                        current.paused = paused;
                    }
                    self.publish();
                    self.report_progress().await;
                }
                PlayerEvent::Seeked => self.report_progress().await,
                PlayerEvent::Ended { index, reason } => {
                    let is_current = self.current.as_ref().is_some_and(|c| c.index == index);
                    if is_current {
                        if reason == EndReason::Eof {
                            self.mark_watched().await;
                        }
                        self.finish_current().await;
                    }
                    if let EndReason::Error(message) = reason {
                        self.core.inner.emit(CoreEvent::Notice {
                            ok: false,
                            message: format!("Playback failed: {message}"),
                        });
                    }
                }
                PlayerEvent::Exited => break,
            }
        }
        self.finish_current().await;
        {
            let mut active = self
                .core
                .inner
                .active
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if active
                .as_ref()
                .is_some_and(|a| a.generation == self.generation)
            {
                *active = None;
            }
        }
        self.core.inner.emit(CoreEvent::PlaybackEnded);
        self.core.inner.emit(CoreEvent::LibraryChanged);
    }
}

impl Core {
    fn active_session(&self) -> Option<SessionHandle> {
        let active = self.inner.active.lock().unwrap_or_else(|e| e.into_inner());
        active.as_ref().map(|a| a.session.clone())
    }

    pub fn is_playing(&self) -> bool {
        self.active_session().is_some()
    }

    /// Spielt einen Titel ab. Ohne `episode_id` startet die erste ungesehene Episode.
    pub async fn play(&self, item: &LibraryItem, episode_id: Option<&str>) -> Result<()> {
        // Ohne Player gar nicht erst bei Jellyfin anfragen.
        let player = self.inner.player_backend()?;
        let jellyfin = self.require_jellyfin()?;
        let config = self.inner.config();
        let queue = self
            .build_queue(&jellyfin, item, episode_id, config.player.autoplay_next)
            .await?;
        let first = queue
            .first()
            .ok_or_else(|| CoreError::Invalid(format!("{} has no episodes", item.name)))?;
        let max_bitrate = config.jellyfin.max_bitrate;
        let target = jellyfin
            .client
            .playback_target(&jellyfin.user_id, &first.id, max_bitrate)
            .await?;

        if let Some(previous) = self.active_session() {
            let _ = previous.command(PlayerCommand::Stop).await;
        }

        let request = OpenRequest {
            http_headers: jellyfin.client.stream_auth_header().into_iter().collect(),
            first: play_item(first, &target),
        };
        let (session, events) = player.open(request).await?;

        let generation = {
            let mut counter = self
                .inner
                .generation
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *counter += 1;
            *counter
        };
        *self.inner.active.lock().unwrap_or_else(|e| e.into_inner()) = Some(ActivePlayback {
            generation,
            session: session.clone(),
        });

        let run = Run {
            core: self.clone(),
            jellyfin,
            session,
            item: item.clone(),
            queue,
            targets: vec![target],
            current: None,
            threshold: config.sync.watched_threshold,
            max_bitrate,
            generation,
        };
        tokio::spawn(run.run(events));
        Ok(())
    }

    async fn build_queue(
        &self,
        jellyfin: &JellyfinSession,
        item: &LibraryItem,
        episode_id: Option<&str>,
        autoplay: bool,
    ) -> Result<Vec<QueueEntry>> {
        if item.kind == ItemKind::Movie {
            return Ok(vec![QueueEntry {
                id: item.id.clone(),
                season: 1,
                number: 1,
                title: item.name.clone(),
                resume_secs: (!item.played && item.position_secs > 0)
                    .then_some(item.position_secs as f64),
            }]);
        }
        let episodes = jellyfin
            .client
            .episodes(&jellyfin.user_id, &item.id)
            .await?;
        let start = match episode_id {
            Some(id) => episodes
                .iter()
                .position(|e| e.id == id)
                .ok_or_else(|| CoreError::Invalid("episode not found".to_owned()))?,
            None => episodes
                .iter()
                .position(|e| !e.played && e.season_number > 0)
                .unwrap_or(0),
        };
        let in_specials = episodes.get(start).is_some_and(|e| e.season_number == 0);
        Ok(episodes
            .iter()
            .enumerate()
            .skip(start)
            .filter(|(i, e)| *i == start || (autoplay && (in_specials || e.season_number > 0)))
            .take(if autoplay { usize::MAX } else { 1 })
            .map(|(_, e)| QueueEntry {
                id: e.id.clone(),
                season: e.season_number,
                number: e.number,
                title: format!(
                    "{} S{:02}E{:02} - {}",
                    item.name, e.season_number, e.number, e.name
                ),
                resume_secs: (!e.played && e.position_secs > 0).then_some(e.position_secs as f64),
            })
            .collect())
    }

    /// Steuert die laufende Wiedergabe.
    pub async fn player_command(&self, command: PlayerCommand) -> Result<()> {
        if let Some(session) = self.active_session() {
            session.command(command).await?;
        }
        Ok(())
    }
}
