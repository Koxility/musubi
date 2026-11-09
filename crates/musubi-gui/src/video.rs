//! Videowiedergabe im App-Fenster über GStreamer.
//!
//! [`EmbeddedPlayer`] erfüllt denselben Vertrag wie das mpv-Backend, damit Fortschritt, Gesehen-
//! Schwelle und Sync im Kern unverändert laufen. GStreamer-Objekte leben auf dem UI-Thread, der
//! Kern spricht über einen Kanal mit ihnen.

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use iced::mouse::Interaction;
use iced::widget::{
    Space, button, center, column, container, mouse_area, row, slider, stack, text,
};
use iced::{Center, ContentFit, Element, Fill, Subscription, Task, window};
use iced_video_player::{Video, VideoPlayer};
use musubi_core::{PlayerBackend, PlayerCommand};
use musubi_player::{
    EndReason, OpenRequest, PlayItem, PlayerEvent, PlayerSession, Result as PlayerResult,
    SessionHandle,
};
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::{Mutex, mpsc};

use crate::app::{App, Message};
use crate::{theme, ui};

const REQUIRED_ELEMENTS: [&str; 5] = [
    "playbin",
    "appsink",
    "videoconvert",
    "souphttpsrc",
    "matroskademux",
];
const VIDEO_SINK: &str = "videoconvert ! videoscale ! appsink name=iced_video drop=true \
                          caps=video/x-raw,format=NV12,pixel-aspect-ratio=1/1";
const SEEK_STEP: f64 = 10.0;
const PREROLL_TIMEOUT: u64 = 60;
const CONTROLS_VISIBLE: Duration = Duration::from_secs(3);
const POSITION_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_BUFFER: usize = 256;

#[derive(Debug, Clone)]
pub enum Command {
    Open(Box<OpenRequest>, mpsc::Sender<PlayerEvent>),
    Append(Box<PlayItem>),
    Control(PlayerCommand),
}

/// Empfängerseite für die Befehle des Kerns.
#[derive(Clone)]
pub struct VideoLink(Arc<Mutex<mpsc::UnboundedReceiver<Command>>>);

impl std::fmt::Debug for VideoLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VideoLink")
    }
}

impl Hash for VideoLink {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "musubi-video".hash(state);
    }
}

struct EmbeddedPlayer {
    commands: mpsc::UnboundedSender<Command>,
}

struct EmbeddedSession {
    commands: mpsc::UnboundedSender<Command>,
}

fn send(commands: &mpsc::UnboundedSender<Command>, command: Command) -> PlayerResult<()> {
    commands
        .send(command)
        .map_err(|_| musubi_player::PlayerError::Disconnected)
}

#[async_trait]
impl PlayerSession for EmbeddedSession {
    async fn append(&self, item: PlayItem) -> PlayerResult<()> {
        send(&self.commands, Command::Append(Box::new(item)))
    }

    async fn command(&self, command: PlayerCommand) -> PlayerResult<()> {
        send(&self.commands, Command::Control(command))
    }
}

#[async_trait]
impl PlayerBackend for EmbeddedPlayer {
    async fn open(
        &self,
        request: OpenRequest,
    ) -> PlayerResult<(SessionHandle, mpsc::Receiver<PlayerEvent>)> {
        let (events, receiver) = mpsc::channel(EVENT_BUFFER);
        send(&self.commands, Command::Open(Box::new(request), events))?;
        let session = EmbeddedSession {
            commands: self.commands.clone(),
        };
        Ok((Arc::new(session), receiver))
    }
}

/// Der eingebettete Player, sofern GStreamer und alle nötigen Plugins vorhanden sind.
pub fn embedded_player() -> Option<(Arc<dyn PlayerBackend>, VideoLink)> {
    if let Err(error) = gst::init() {
        tracing::warn!(%error, "GStreamer could not be started");
        return None;
    }
    let missing: Vec<&str> = REQUIRED_ELEMENTS
        .into_iter()
        .filter(|name| gst::ElementFactory::find(name).is_none())
        .collect();
    if !missing.is_empty() {
        tracing::warn!(?missing, "GStreamer plugins are missing");
        return None;
    }
    let (commands, receiver) = mpsc::unbounded_channel();
    let player: Arc<dyn PlayerBackend> = Arc::new(EmbeddedPlayer { commands });
    Some((player, VideoLink(Arc::new(Mutex::new(receiver)))))
}

/// Wiedergabeliste. Der Kern hängt Folgeepisoden an, sobald eine Episode läuft.
#[derive(Debug, Default)]
struct Playlist {
    items: Vec<PlayItem>,
    index: usize,
}

impl Playlist {
    fn next_index(&self) -> Option<usize> {
        (self.index + 1 < self.items.len()).then_some(self.index + 1)
    }

    fn previous_index(&self) -> Option<usize> {
        self.index.checked_sub(1)
    }
}

/// Nächster Untertitelzustand: alle Spuren der Reihe nach, danach aus.
fn next_subtitle(current: i32, count: i32, enabled: bool) -> (i32, bool) {
    if count == 0 {
        return (current, false);
    }
    if !enabled {
        return (0, true);
    }
    if current + 1 >= count {
        (current, false)
    } else {
        (current + 1, true)
    }
}

/// Zerlegt `Name: Wert`. Der Wert darf selbst Doppelpunkte enthalten.
fn split_header(header: &str) -> Option<(&str, &str)> {
    let (name, value) = header.split_once(':')?;
    Some((name.trim(), value.trim()))
}

/// GStreamer-Objekte eines fertig geladenen Streams.
#[derive(Debug, Clone)]
pub struct Prepared {
    pipeline: gst::Pipeline,
    sink: gst_app::AppSink,
}

fn pipeline_error(pipeline: &gst::Pipeline) -> String {
    pipeline
        .bus()
        .and_then(|bus| bus.timed_pop_filtered(gst::ClockTime::ZERO, &[gst::MessageType::Error]))
        .and_then(|message| match message.view() {
            // Nur der Fehlertext: Die Debug-Zeile kann die Stream-URL samt Zugangsdaten enthalten.
            gst::MessageView::Error(error) => Some(error.error().to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "the video could not be opened".to_owned())
}

fn wait_for_preroll(pipeline: &gst::Pipeline) -> Result<(), String> {
    let (result, _, _) = pipeline.state(gst::ClockTime::from_seconds(PREROLL_TIMEOUT));
    result.map(|_| ()).map_err(|_| pipeline_error(pipeline))
}

/// Öffnet einen Stream bis zum ersten Bild. Blockiert und läuft deshalb abseits des UI-Threads.
fn prepare(item: &PlayItem, headers: &[String]) -> Result<Prepared, String> {
    let description = format!("playbin video-sink=\"{VIDEO_SINK}\"");
    let element = gst::parse::launch(&description).map_err(|e| e.to_string())?;
    element.set_property("uri", item.url.expose_secret());
    if let Some(subtitle) = item.subtitles.first() {
        element.set_property("suburi", subtitle.url.expose_secret());
    }
    let extra: Vec<(String, String)> = headers
        .iter()
        .filter_map(|h| split_header(h).map(|(n, v)| (n.to_owned(), v.to_owned())))
        .collect();
    element.connect("source-setup", false, move |args| {
        let source = args.get(1).and_then(|v| v.get::<gst::Element>().ok())?;
        if source.has_property("extra-headers", None) {
            let mut structure = gst::Structure::new_empty("extra-headers");
            for (name, value) in &extra {
                structure.set(name.as_str(), value.as_str());
            }
            source.set_property("extra-headers", &structure);
        }
        None
    });
    let pipeline = element
        .downcast::<gst::Pipeline>()
        .map_err(|_| "unexpected GStreamer pipeline".to_owned())?;
    let opened = (|| {
        pipeline
            .set_state(gst::State::Paused)
            .map_err(|_| pipeline_error(&pipeline))?;
        wait_for_preroll(&pipeline)?;
        if let Some(start) = item.start_secs.filter(|s| *s > 0.0) {
            pipeline
                .seek_simple(
                    gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                    gst::ClockTime::from_seconds_f64(start),
                )
                .map_err(|e| e.to_string())?;
            wait_for_preroll(&pipeline)?;
        }
        let bin_sink: gst::Element = pipeline.property("video-sink");
        let sink = bin_sink
            .downcast::<gst::Bin>()
            .ok()
            .and_then(|bin| bin.by_name("iced_video"))
            .and_then(|e| e.downcast::<gst_app::AppSink>().ok())
            .ok_or_else(|| "the video sink is missing".to_owned())?;
        Ok(sink)
    })();
    match opened {
        Ok(sink) => Ok(Prepared { pipeline, sink }),
        Err(error) => {
            let _ = pipeline.set_state(gst::State::Null);
            Err(error)
        }
    }
}

struct Session {
    events: mpsc::Sender<PlayerEvent>,
    headers: Vec<String>,
    playlist: Playlist,
    video: Option<Video>,
    subtitles_on: bool,
    last_position: Instant,
}

impl Session {
    fn emit(&self, event: PlayerEvent) {
        if self.events.try_send(event).is_err() {
            tracing::debug!("playback event dropped");
        }
    }

    fn title(&self) -> &str {
        self.playlist
            .items
            .get(self.playlist.index)
            .map_or("", |i| i.title.as_str())
    }

    fn report_position(&self, video: &Video) {
        self.emit(PlayerEvent::Position {
            position: video.position().as_secs_f64(),
            duration: Some(video.duration().as_secs_f64()).filter(|d| *d > 0.0),
            paused: video.paused(),
        });
    }
}

pub struct State {
    session: Option<Session>,
    generation: u64,
    controls_until: Instant,
    fullscreen: bool,
    seek_preview: Option<f64>,
    volume: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            session: None,
            generation: 0,
            controls_until: Instant::now(),
            fullscreen: false,
            seek_preview: None,
            volume: 1.0,
        }
    }
}

impl State {
    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    fn show_controls(&mut self) {
        self.controls_until = Instant::now() + CONTROLS_VISIBLE;
    }

    fn controls_visible(&self) -> bool {
        let paused = self
            .session
            .as_ref()
            .and_then(|s| s.video.as_ref())
            .is_none_or(Video::paused);
        paused || Instant::now() < self.controls_until || self.seek_preview.is_some()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Key {
    Pause,
    Back,
    Forward,
    Fullscreen,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Command(Command),
    Prepared(u64, Result<Prepared, String>),
    EndOfStream,
    Tick,
    Activity,
    Control(PlayerCommand),
    Key(Key),
    Stop,
    ToggleFullscreen,
    SeekPreview(f64),
    SeekCommit,
    Volume(f64),
}

fn command_stream(link: &VideoLink) -> impl iced::futures::Stream<Item = Message> + use<> {
    let link = link.clone();
    iced::stream::channel(32, async move |mut output| {
        use iced::futures::SinkExt;
        let mut receiver = link.0.lock().await;
        while let Some(command) = receiver.recv().await {
            if output
                .send(Message::Video(Msg::Command(command)))
                .await
                .is_err()
            {
                return;
            }
        }
    })
}

pub fn subscription(state: &State, link: Option<&VideoLink>) -> Subscription<Message> {
    let commands = link.map_or_else(Subscription::none, |link| {
        Subscription::run_with(link.clone(), command_stream)
    });
    let ticks = if state.is_active() {
        iced::time::every(Duration::from_millis(250)).map(|_| Message::Video(Msg::Tick))
    } else {
        Subscription::none()
    };
    Subscription::batch([commands, ticks])
}

/// Lädt den Eintrag `index` im Hintergrund. Das Ergebnis kommt als [`Msg::Prepared`].
fn load(app: &mut App, index: usize) -> Task<Message> {
    let Some(session) = &mut app.video.session else {
        return Task::none();
    };
    let Some(item) = session.playlist.items.get(index).cloned() else {
        return Task::none();
    };
    session.playlist.index = index;
    session.video = None;
    let headers = session.headers.clone();
    app.video.generation += 1;
    let generation = app.video.generation;
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || prepare(&item, &headers))
                .await
                .map_err(|e| e.to_string())?
        },
        move |result| Message::Video(Msg::Prepared(generation, result)),
    )
}

fn set_fullscreen(on: bool) -> Task<Message> {
    let mode = if on {
        window::Mode::Fullscreen
    } else {
        window::Mode::Windowed
    };
    window::latest().then(move |id| match id {
        Some(id) => window::set_mode(id, mode),
        None => Task::none(),
    })
}

/// Schließt die Sitzung, nachdem das Ende des letzten Eintrags gemeldet wurde.
fn close(app: &mut App) -> Task<Message> {
    let Some(session) = app.video.session.take() else {
        return Task::none();
    };
    session.emit(PlayerEvent::Exited);
    app.video.generation += 1;
    app.video.seek_preview = None;
    if app.video.fullscreen {
        app.video.fullscreen = false;
        return set_fullscreen(false);
    }
    Task::none()
}

/// Bricht die Wiedergabe ab und meldet das Ende an den Kern.
fn finish(app: &mut App, reason: EndReason) -> Task<Message> {
    if let Some(session) = &app.video.session {
        session.emit(PlayerEvent::Ended {
            index: session.playlist.index,
            reason,
        });
    }
    close(app)
}

fn jump(app: &mut App, index: Option<usize>) -> Task<Message> {
    let Some(index) = index else {
        return Task::none();
    };
    if let Some(session) = &app.video.session {
        session.emit(PlayerEvent::Ended {
            index: session.playlist.index,
            reason: EndReason::Stopped,
        });
    }
    load(app, index)
}

fn seek_to(session: &mut Session, target: f64) {
    let Some(video) = &mut session.video else {
        return;
    };
    let duration = video.duration().as_secs_f64();
    let upper = if duration > 0.0 { duration } else { f64::MAX };
    let position = Duration::from_secs_f64(target.clamp(0.0, upper));
    if let Err(error) = video.seek(position, false) {
        tracing::warn!(%error, "seeking failed");
        return;
    }
    session.emit(PlayerEvent::Seeked);
}

fn cycle_subtitle(session: &mut Session) {
    let Some(video) = &session.video else {
        return;
    };
    let pipeline = video.pipeline();
    let count: i32 = pipeline.property("n-text");
    let current: i32 = pipeline.property("current-text");
    let (next, enabled) = next_subtitle(current, count, session.subtitles_on);
    let flags = pipeline.property_value("flags");
    let Some(class) = gst::glib::FlagsClass::with_type(flags.type_()) else {
        return;
    };
    let Some(builder) = class.builder_with_value(flags) else {
        return;
    };
    let builder = if enabled {
        builder.set_by_nick("text")
    } else {
        builder.unset_by_nick("text")
    };
    if let Some(value) = builder.build() {
        pipeline.set_property_from_value("flags", &value);
    }
    if enabled {
        pipeline.set_property("current-text", next);
    }
    session.subtitles_on = enabled;
}

fn cycle_audio(session: &Session) {
    let Some(video) = &session.video else {
        return;
    };
    let pipeline = video.pipeline();
    let count: i32 = pipeline.property("n-audio");
    if count > 1 {
        let current: i32 = pipeline.property("current-audio");
        pipeline.set_property("current-audio", (current + 1) % count);
    }
}

fn apply(app: &mut App, command: PlayerCommand) -> Task<Message> {
    app.video.show_controls();
    if matches!(command, PlayerCommand::Stop) {
        return finish(app, EndReason::Quit);
    }
    let Some(session) = &mut app.video.session else {
        return Task::none();
    };
    match command {
        PlayerCommand::TogglePause => {
            if let Some(video) = &mut session.video {
                let paused = !video.paused();
                video.set_paused(paused);
                session.emit(PlayerEvent::Paused(paused));
            }
        }
        PlayerCommand::SeekRelative(delta) => {
            let position = session
                .video
                .as_ref()
                .map_or(0.0, |v| v.position().as_secs_f64());
            seek_to(session, position + delta);
        }
        PlayerCommand::SeekAbsolute(position) => seek_to(session, position),
        PlayerCommand::CycleSubtitle => cycle_subtitle(session),
        PlayerCommand::CycleAudio => cycle_audio(session),
        PlayerCommand::Next => {
            let index = session.playlist.next_index();
            return jump(app, index);
        }
        PlayerCommand::Previous => {
            let index = session.playlist.previous_index();
            return jump(app, index);
        }
        PlayerCommand::Stop => {}
    }
    Task::none()
}

fn on_command(app: &mut App, command: Command) -> Task<Message> {
    match command {
        Command::Open(request, events) => {
            let request = *request;
            let headers = request
                .http_headers
                .iter()
                .map(|h: &SecretString| h.expose_secret().to_owned())
                .collect();
            app.video.session = Some(Session {
                events,
                headers,
                playlist: Playlist {
                    items: vec![request.first],
                    index: 0,
                },
                video: None,
                subtitles_on: true,
                last_position: Instant::now(),
            });
            app.video.show_controls();
            load(app, 0)
        }
        Command::Append(item) => {
            if let Some(session) = &mut app.video.session {
                session.playlist.items.push(*item);
            }
            Task::none()
        }
        Command::Control(command) => apply(app, command),
    }
}

fn on_prepared(app: &mut App, generation: u64, result: Result<Prepared, String>) -> Task<Message> {
    if generation != app.video.generation {
        if let Ok(stale) = result {
            let _ = stale.pipeline.set_state(gst::State::Null);
        }
        return Task::none();
    }
    let opened = result.and_then(|prepared| {
        Video::from_gst_pipeline(prepared.pipeline, prepared.sink, None).map_err(|e| e.to_string())
    });
    match opened {
        Ok(mut video) => {
            video.set_volume(app.video.volume);
            let Some(session) = &mut app.video.session else {
                return Task::none();
            };
            session.emit(PlayerEvent::Started {
                index: session.playlist.index,
            });
            session.report_position(&video);
            session.last_position = Instant::now();
            session.video = Some(video);
            Task::none()
        }
        Err(error) => {
            app.toast(format!("Playback failed: {error}"), false);
            finish(app, EndReason::Error(error))
        }
    }
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    match msg {
        Msg::Command(command) => return on_command(app, command),
        Msg::Prepared(generation, result) => return on_prepared(app, generation, result),
        Msg::EndOfStream => {
            let Some(session) = &app.video.session else {
                return Task::none();
            };
            session.emit(PlayerEvent::Ended {
                index: session.playlist.index,
                reason: EndReason::Eof,
            });
            return match session.playlist.next_index() {
                Some(index) => load(app, index),
                None => close(app),
            };
        }
        Msg::Tick => {
            if let Some(session) = &mut app.video.session
                && session.last_position.elapsed() >= POSITION_INTERVAL
                && let Some(video) = &session.video
            {
                session.report_position(video);
                session.last_position = Instant::now();
            }
        }
        Msg::Activity => app.video.show_controls(),
        Msg::Control(command) => return apply(app, command),
        Msg::Key(key) if app.video.is_active() => {
            return match key {
                Key::Pause => apply(app, PlayerCommand::TogglePause),
                Key::Back => apply(app, PlayerCommand::SeekRelative(-SEEK_STEP)),
                Key::Forward => apply(app, PlayerCommand::SeekRelative(SEEK_STEP)),
                Key::Fullscreen => update(app, Msg::ToggleFullscreen),
            };
        }
        Msg::Key(_) => {}
        Msg::Stop => return apply(app, PlayerCommand::Stop),
        Msg::ToggleFullscreen => {
            app.video.fullscreen = !app.video.fullscreen;
            return set_fullscreen(app.video.fullscreen);
        }
        Msg::SeekPreview(position) => {
            app.video.show_controls();
            app.video.seek_preview = Some(position);
        }
        Msg::SeekCommit => {
            if let Some(position) = app.video.seek_preview.take() {
                return apply(app, PlayerCommand::SeekAbsolute(position));
            }
        }
        Msg::Volume(volume) => {
            app.video.show_controls();
            app.video.volume = volume;
            if let Some(video) = app.video.session.as_mut().and_then(|s| s.video.as_mut()) {
                video.set_volume(volume);
            }
        }
    }
    Task::none()
}

fn control<'a>(label: &'a str, message: Msg) -> Element<'a, Msg> {
    button(text(label).size(15))
        .padding([8, 12])
        .style(theme::secondary)
        .on_press(message)
        .into()
}

fn top_bar<'a>(title: &'a str) -> Element<'a, Msg> {
    container(
        row![
            control("← Back", Msg::Stop),
            text(ui::ellipsize(title, 110)).size(17),
        ]
        .spacing(14)
        .align_y(Center),
    )
    .padding([12, 16])
    .width(Fill)
    .style(theme::dim)
    .into()
}

fn bottom_bar<'a>(state: &'a State, video: &'a Video) -> Element<'a, Msg> {
    let duration = video.duration().as_secs_f64().max(1.0);
    let position = state
        .seek_preview
        .unwrap_or_else(|| video.position().as_secs_f64())
        .min(duration);
    let time = format!(
        "{} / {}",
        ui::format_time(position),
        ui::format_time(duration)
    );
    let command = Msg::Control;
    container(
        column![
            slider(0.0..=duration, position, Msg::SeekPreview).on_release(Msg::SeekCommit),
            row![
                control("⏮", command(PlayerCommand::Previous)),
                control("-10", command(PlayerCommand::SeekRelative(-SEEK_STEP))),
                control(
                    if video.paused() { "▶" } else { "⏸" },
                    command(PlayerCommand::TogglePause)
                ),
                control("+10", command(PlayerCommand::SeekRelative(SEEK_STEP))),
                control("⏭", command(PlayerCommand::Next)),
                text(time).size(14),
                ui::spacer(),
                control("Subs", command(PlayerCommand::CycleSubtitle)),
                control("Audio", command(PlayerCommand::CycleAudio)),
                container(slider(0.0..=1.0, state.volume, Msg::Volume).step(0.01)).width(110),
                control(
                    if state.fullscreen {
                        "Window"
                    } else {
                        "Fullscreen"
                    },
                    Msg::ToggleFullscreen
                ),
            ]
            .spacing(8)
            .align_y(Center),
        ]
        .spacing(8),
    )
    .padding([12, 16])
    .width(Fill)
    .style(theme::dim)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let state = &app.video;
    let Some(session) = &state.session else {
        return Space::new().into();
    };
    let picture: Element<'_, Msg> = match &session.video {
        Some(video) => VideoPlayer::new(video)
            .width(Fill)
            .height(Fill)
            .content_fit(ContentFit::Contain)
            .on_end_of_stream(Msg::EndOfStream)
            .into(),
        None => center(text("Loading…").size(18).color(theme::MUTED)).into(),
    };
    let visible = state.controls_visible();
    let surface = mouse_area(
        container(picture)
            .width(Fill)
            .height(Fill)
            .style(theme::video),
    )
    .on_move(|_| Msg::Activity)
    .on_press(Msg::Control(PlayerCommand::TogglePause))
    .on_double_click(Msg::ToggleFullscreen)
    .interaction(if visible {
        Interaction::Idle
    } else {
        Interaction::Hidden
    });
    let mut layers = stack![surface].width(Fill).height(Fill);
    if visible {
        let bottom: Element<'_, Msg> = match &session.video {
            Some(video) => bottom_bar(state, video),
            None => Space::new().into(),
        };
        layers = layers.push(column![
            top_bar(session.title()),
            Space::new().height(Fill),
            bottom
        ]);
    }
    Element::from(layers).map(Message::Video)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str) -> PlayItem {
        PlayItem {
            url: SecretString::from("http://server/video"),
            title: title.to_owned(),
            start_secs: None,
            subtitles: vec![],
        }
    }

    #[test]
    fn playlist_only_moves_within_its_items() {
        let mut list = Playlist {
            items: vec![item("a")],
            index: 0,
        };
        assert_eq!(list.next_index(), None);
        assert_eq!(list.previous_index(), None);
        list.items.push(item("b"));
        assert_eq!(list.next_index(), Some(1));
        list.index = 1;
        assert_eq!(list.next_index(), None);
        assert_eq!(list.previous_index(), Some(0));
    }

    #[test]
    fn subtitles_cycle_through_tracks_then_off_then_first() {
        assert_eq!(next_subtitle(0, 2, true), (1, true));
        assert_eq!(next_subtitle(1, 2, true), (1, false));
        assert_eq!(next_subtitle(1, 2, false), (0, true));
        assert_eq!(next_subtitle(-1, 0, true), (-1, false));
    }

    #[test]
    fn auth_headers_keep_colons_in_their_value() {
        assert_eq!(
            split_header("Authorization: MediaBrowser Token=\"a:b\""),
            Some(("Authorization", "MediaBrowser Token=\"a:b\""))
        );
        assert_eq!(split_header("broken"), None);
    }

    #[tokio::test]
    async fn the_backend_hands_commands_to_the_interface() {
        let (commands, mut receiver) = mpsc::unbounded_channel();
        let player = EmbeddedPlayer { commands };
        let request = OpenRequest {
            http_headers: vec![],
            first: item("a"),
        };

        let (session, _events) = player.open(request).await.unwrap();
        session.command(PlayerCommand::TogglePause).await.unwrap();
        session.append(item("b")).await.unwrap();

        assert!(matches!(receiver.recv().await, Some(Command::Open(..))));
        assert!(matches!(
            receiver.recv().await,
            Some(Command::Control(PlayerCommand::TogglePause))
        ));
        assert!(matches!(receiver.recv().await, Some(Command::Append(..))));
    }
}
