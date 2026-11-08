use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};
use tokio::time::{Instant, sleep, timeout};

use crate::{
    EndReason, OpenRequest, PlayItem, PlayerBackend, PlayerCommand, PlayerError, PlayerEvent,
    PlayerSession, Result, SessionHandle, SubtitleTrack,
};

const OBSERVED: [&str; 4] = ["time-pos", "duration", "pause", "idle-active"];
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const POSITION_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct MpvOptions {
    pub binary: String,
    /// Wert für `--hwdec`. Leer lässt die Einstellung der eigenen `mpv.conf` gelten.
    pub hwdec: String,
    pub extra_args: Vec<String>,
    /// Verzeichnis für den IPC-Socket, nur für den Eigentümer zugänglich.
    pub socket_dir: PathBuf,
}

pub struct MpvBackend {
    options: MpvOptions,
}

impl MpvBackend {
    pub fn new(options: MpvOptions) -> Self {
        Self { options }
    }

    fn spawn(&self, socket: &std::path::Path) -> Result<Child> {
        let mut command = Command::new(&self.options.binary);
        command
            .arg("--idle=yes")
            .arg("--force-window=yes")
            .arg("--terminal=no")
            .arg("--ytdl=no")
            .arg(format!("--input-ipc-server={}", socket.display()));
        if !self.options.hwdec.is_empty() {
            command.arg(format!("--hwdec={}", self.options.hwdec));
        }
        command
            .args(&self.options.extra_args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        command.spawn().map_err(|source| PlayerError::Spawn {
            path: self.options.binary.clone(),
            source,
        })
    }
}

async fn connect(socket: &std::path::Path, child: &mut Child) -> Result<UnixStream> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match UnixStream::connect(socket).await {
            Ok(stream) => return Ok(stream),
            Err(_) if Instant::now() < deadline => {
                if let Ok(Some(_)) = child.try_wait() {
                    return Err(PlayerError::Disconnected);
                }
                sleep(Duration::from_millis(50)).await;
            }
            Err(_) => return Err(PlayerError::ConnectTimeout),
        }
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<std::result::Result<Value, String>>>>>;
type SubtitleLists = Arc<Mutex<Vec<Vec<SubtitleTrack>>>>;

struct Link {
    writer: AsyncMutex<OwnedWriteHalf>,
    pending: Pending,
    next_id: AtomicU64,
}

impl Link {
    async fn send(&self, command: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, response) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, reply);
        let mut line = serde_json::to_vec(&json!({ "command": command, "request_id": id }))
            .map_err(|e| PlayerError::Command(e.to_string()))?;
        line.push(b'\n');
        {
            let mut writer = self.writer.lock().await;
            if writer.write_all(&line).await.is_err() {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                return Err(PlayerError::Disconnected);
            }
        }
        match timeout(REPLY_TIMEOUT, response).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(message))) => Err(PlayerError::Command(message)),
            Ok(Err(_)) | Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                Err(PlayerError::Disconnected)
            }
        }
    }
}

struct MpvSession {
    link: Arc<Link>,
    subtitles: SubtitleLists,
}

impl MpvSession {
    async fn load(&self, item: PlayItem, flags: &str) -> Result<()> {
        self.subtitles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(item.subtitles.clone());
        let mut options = serde_json::Map::new();
        options.insert("force-media-title".to_owned(), json!(item.title));
        if let Some(start) = item.start_secs.filter(|s| *s > 0.0) {
            options.insert("start".to_owned(), json!(format!("{start:.3}")));
        }
        // Benannte Argumente: Positionsargumente von `loadfile` haben sich in mpv 0.38 geändert.
        self.link
            .send(json!({
                "name": "loadfile",
                "url": item.url.expose_secret(),
                "flags": flags,
                "options": options,
            }))
            .await?;
        Ok(())
    }
}

#[async_trait]
impl PlayerSession for MpvSession {
    async fn append(&self, item: PlayItem) -> Result<()> {
        self.load(item, "append").await
    }

    async fn command(&self, command: PlayerCommand) -> Result<()> {
        let value = match command {
            PlayerCommand::TogglePause => json!(["cycle", "pause"]),
            PlayerCommand::SeekRelative(secs) => json!(["seek", secs, "relative"]),
            PlayerCommand::SeekAbsolute(secs) => json!(["seek", secs, "absolute"]),
            PlayerCommand::CycleSubtitle => json!(["cycle", "sub"]),
            PlayerCommand::CycleAudio => json!(["cycle", "audio"]),
            PlayerCommand::Next => json!(["playlist-next", "weak"]),
            PlayerCommand::Previous => json!(["playlist-prev", "weak"]),
            PlayerCommand::Stop => json!(["quit"]),
        };
        self.link.send(value).await?;
        Ok(())
    }
}

/// Verarbeitet die Ereignisse von mpv zu Player-Ereignissen.
#[derive(Default)]
struct EventState {
    /// `playlist_entry_id` des aktuellen Eintrags (mpv zählt ab 1 in Einfüge-Reihenfolge).
    current_entry: Option<u64>,
    position: Option<f64>,
    duration: Option<f64>,
    paused: bool,
    has_played: bool,
    last_position_sent: Option<Instant>,
}

enum Effect {
    Emit(PlayerEvent),
    AddSubtitles(usize),
    Quit,
}

impl EventState {
    fn index(entry: u64) -> usize {
        entry.saturating_sub(1) as usize
    }

    fn handle(&mut self, message: &Value, now: Instant) -> Vec<Effect> {
        match message["event"].as_str() {
            Some("property-change") => self.property(message, now),
            Some("start-file") => {
                self.current_entry = message["playlist_entry_id"].as_u64();
                self.position = None;
                self.duration = None;
                vec![]
            }
            Some("file-loaded") => {
                self.has_played = true;
                match self.current_entry.map(Self::index) {
                    Some(index) => vec![
                        Effect::Emit(PlayerEvent::Started { index }),
                        Effect::AddSubtitles(index),
                    ],
                    None => vec![],
                }
            }
            Some("seek") => vec![Effect::Emit(PlayerEvent::Seeked)],
            Some("end-file") => {
                let Some(entry) = message["playlist_entry_id"].as_u64() else {
                    return vec![];
                };
                let reason = match message["reason"].as_str() {
                    Some("eof") => EndReason::Eof,
                    Some("quit") => EndReason::Quit,
                    Some("error") => EndReason::Error(
                        message["file_error"]
                            .as_str()
                            .unwrap_or("unknown error")
                            .to_owned(),
                    ),
                    _ => EndReason::Stopped,
                };
                vec![Effect::Emit(PlayerEvent::Ended {
                    index: Self::index(entry),
                    reason,
                })]
            }
            _ => vec![],
        }
    }

    fn property(&mut self, message: &Value, now: Instant) -> Vec<Effect> {
        let data = &message["data"];
        match message["name"].as_str() {
            Some("time-pos") => {
                self.position = data.as_f64();
                let due = self
                    .last_position_sent
                    .is_none_or(|last| now.duration_since(last) >= POSITION_INTERVAL);
                match self.position {
                    Some(position) if due => {
                        self.last_position_sent = Some(now);
                        vec![Effect::Emit(PlayerEvent::Position {
                            position,
                            duration: self.duration,
                            paused: self.paused,
                        })]
                    }
                    _ => vec![],
                }
            }
            Some("duration") => {
                self.duration = data.as_f64();
                vec![]
            }
            Some("pause") => {
                self.paused = data.as_bool().unwrap_or(false);
                vec![Effect::Emit(PlayerEvent::Paused(self.paused))]
            }
            // Liste abgearbeitet: Fenster schließen, statt leer stehen zu bleiben.
            Some("idle-active") if data.as_bool() == Some(true) && self.has_played => {
                vec![Effect::Quit]
            }
            _ => vec![],
        }
    }
}

async fn read_loop(
    read: OwnedReadHalf,
    link: Arc<Link>,
    subtitles: SubtitleLists,
    events: mpsc::Sender<PlayerEvent>,
) {
    let mut lines = BufReader::new(read).lines();
    let mut state = EventState::default();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if let Some(id) = message["request_id"]
            .as_u64()
            .filter(|_| message["error"].is_string())
        {
            let reply = link
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            if let Some(reply) = reply {
                let result = match message["error"].as_str() {
                    Some("success") => Ok(message["data"].clone()),
                    other => Err(other.unwrap_or("unknown").to_owned()),
                };
                let _ = reply.send(result);
            }
            continue;
        }

        for effect in state.handle(&message, Instant::now()) {
            match effect {
                Effect::Emit(event) => {
                    let _ = events.send(event).await;
                }
                Effect::AddSubtitles(index) => {
                    let tracks = subtitles
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    add_subtitles(&link, tracks);
                }
                Effect::Quit => {
                    let link = Arc::clone(&link);
                    tokio::spawn(async move {
                        let _ = link.send(json!(["quit"])).await;
                    });
                }
            }
        }
    }
}

/// Eigener Task, weil die Antworten über dieselbe Leseschleife kommen.
fn add_subtitles(link: &Arc<Link>, tracks: Vec<SubtitleTrack>) {
    if tracks.is_empty() {
        return;
    }
    let link = Arc::clone(link);
    tokio::spawn(async move {
        for track in tracks {
            let command = json!({
                "name": "sub-add",
                "url": track.url.expose_secret(),
                "flags": "auto",
                "title": track.title.unwrap_or_default(),
                "lang": track.language.unwrap_or_default(),
            });
            if let Err(error) = link.send(command).await {
                tracing::warn!(%error, "could not add subtitle track");
            }
        }
    });
}

#[async_trait]
impl PlayerBackend for MpvBackend {
    async fn open(
        &self,
        request: OpenRequest,
    ) -> Result<(SessionHandle, mpsc::Receiver<PlayerEvent>)> {
        let socket = self
            .options
            .socket_dir
            .join(format!("mpv-{}.sock", uuid::Uuid::new_v4()));
        let mut child = self.spawn(&socket)?;
        let stream = connect(&socket, &mut child).await?;
        let (read, write) = stream.into_split();

        let link = Arc::new(Link {
            writer: AsyncMutex::new(write),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        });
        let subtitles: SubtitleLists = Arc::new(Mutex::new(Vec::new()));
        let (events_tx, events_rx) = mpsc::channel(256);

        {
            let (link, subtitles, events) =
                (Arc::clone(&link), Arc::clone(&subtitles), events_tx.clone());
            tokio::spawn(async move {
                read_loop(read, link, subtitles, events.clone()).await;
                let _ = child.wait().await;
                let _ = std::fs::remove_file(&socket);
                let _ = events.send(PlayerEvent::Exited).await;
            });
        }

        let headers: Vec<&str> = request
            .http_headers
            .iter()
            .map(|h| h.expose_secret())
            .collect();
        link.send(json!(["set_property", "http-header-fields", headers]))
            .await?;
        for (id, name) in OBSERVED.iter().enumerate() {
            link.send(json!(["observe_property", id + 1, name])).await?;
        }

        let session = MpvSession { link, subtitles };
        session.load(request.first, "replace").await?;
        Ok((Arc::new(session), events_rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitted(effects: Vec<Effect>) -> Vec<PlayerEvent> {
        effects
            .into_iter()
            .filter_map(|e| {
                if let Effect::Emit(ev) = e {
                    Some(ev)
                } else {
                    None
                }
            })
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn maps_entry_ids_to_indices_and_throttles_positions() {
        let mut state = EventState::default();
        let t0 = Instant::now();
        state.handle(&json!({"event": "start-file", "playlist_entry_id": 2}), t0);
        assert_eq!(
            emitted(state.handle(&json!({"event": "file-loaded"}), t0)),
            [PlayerEvent::Started { index: 1 }]
        );
        let tick = json!({"event": "property-change", "name": "time-pos", "data": 1.5});
        assert_eq!(emitted(state.handle(&tick, t0)).len(), 1);
        assert_eq!(
            emitted(state.handle(&tick, t0 + Duration::from_millis(100))).len(),
            0
        );
        assert_eq!(
            emitted(state.handle(&tick, t0 + POSITION_INTERVAL)).len(),
            1
        );
    }

    #[tokio::test(start_paused = true)]
    async fn idle_only_quits_after_playback_began() {
        let mut state = EventState::default();
        let t0 = Instant::now();
        let idle = json!({"event": "property-change", "name": "idle-active", "data": true});
        assert!(state.handle(&idle, t0).is_empty());
        state.handle(&json!({"event": "start-file", "playlist_entry_id": 1}), t0);
        state.handle(&json!({"event": "file-loaded"}), t0);
        assert!(matches!(state.handle(&idle, t0).as_slice(), [Effect::Quit]));
    }

    #[tokio::test(start_paused = true)]
    async fn end_reasons_are_translated() {
        let mut state = EventState::default();
        let t0 = Instant::now();
        let end = json!({"event": "end-file", "reason": "error", "file_error": "boom",
                         "playlist_entry_id": 1});
        assert_eq!(
            emitted(state.handle(&end, t0)),
            [PlayerEvent::Ended {
                index: 0,
                reason: EndReason::Error("boom".into())
            }]
        );
    }
}
