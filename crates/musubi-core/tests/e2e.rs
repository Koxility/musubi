//! Durchstich gegen Mock-Server für Jellyfin und AniList und einen Player nach Drehbuch.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use musubi_core::domain::{
    EntryPatch, ItemKind, LibraryItem, ListStatus, Mapping, MappingSource, MediaKind,
    resolve_episode,
};
use musubi_core::{
    Config, Core, CoreError, CoreEvent, CoreOptions, EndReason, MatchOutcome, OpenRequest, Paths,
    PlayItem, PlayerBackend, PlayerCommand, PlayerError, PlayerEvent, PlayerSession, Service,
    SessionHandle, SyncDirection,
};
use secrecy::ExposeSecret;
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};
use tokio::time::timeout;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const JELLYFIN_TOKEN: &str = "jf-secret-token";

fn episode(id: &str, number: u32, played: bool, position_ticks: u64) -> Value {
    json!({
        "Id": id, "Name": format!("Ep {number}"), "SeriesName": "Show",
        "ParentIndexNumber": 1, "IndexNumber": number, "RunTimeTicks": 40_000_000u64,
        "UserData": {"Played": played, "PlaybackPositionTicks": position_ticks}
    })
}

fn media(id: i64) -> Value {
    json!({
        "id": id, "type": "ANIME", "title": {"romaji": "Show"}, "format": "TV",
        "episodes": 12, "status": "FINISHED"
    })
}

fn saved_entry(media_id: i64, progress: u32) -> Value {
    json!({
        "id": 900, "status": "CURRENT", "score": 0, "progress": progress,
        "media": media(media_id)
    })
}

async fn mock_jellyfin() -> MockServer {
    let server = MockServer::start().await;
    let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
    let routes = [
        (
            "GET",
            "/System/Info/Public",
            ok(json!({"ServerName": "Mock"})),
        ),
        (
            "POST",
            "/Users/AuthenticateByName",
            ok(json!({"AccessToken": JELLYFIN_TOKEN, "User": {"Id": "u1", "Name": "eden"}})),
        ),
        (
            "POST",
            "/Items/e1/PlaybackInfo",
            ok(
                json!({"PlaySessionId": "ps1", "MediaSources": [{"Id": "ms1", "SupportsDirectStream": true}]}),
            ),
        ),
        (
            "POST",
            "/Items/e2/PlaybackInfo",
            ok(
                json!({"PlaySessionId": "ps2", "MediaSources": [{"Id": "ms2", "SupportsDirectStream": true}]}),
            ),
        ),
    ];
    for (verb, route, response) in routes {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    for route in [
        "/Sessions/Playing",
        "/Sessions/Playing/Progress",
        "/Sessions/Playing/Stopped",
    ] {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/Shows/s1/Episodes"))
        .respond_with(ok(json!({"Items": [episode("e1", 1, false, 10_000_000)]})))
        .mount(&server)
        .await;
    server
}

/// AniList-Mock, der anhand des Query-Texts antwortet. `progress` ist der Stand auf der Liste.
async fn mock_anilist(entries: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let query = body["query"].as_str().unwrap();
            let data = if query.contains("SaveMediaListEntry") {
                let vars = &body["variables"];
                json!({"SaveMediaListEntry": saved_entry(
                    vars["mediaId"].as_i64().unwrap(),
                    vars["progress"].as_u64().unwrap_or(0) as u32,
                )})
            } else if query.contains("Viewer") {
                json!({"Viewer": {"id": 7, "name": "eden",
                    "mediaListOptions": {"scoreFormat": "POINT_100"}}})
            } else if query.contains("MediaListCollection") {
                json!({"MediaListCollection": {"hasNextChunk": false,
                    "lists": [{"entries": entries.clone()}]}})
            } else {
                json!({"Media": media(body["variables"]["id"].as_i64().unwrap_or(101))})
            };
            ResponseTemplate::new(200)
                .insert_header("X-RateLimit-Limit", "90")
                .set_body_json(json!({"data": data}))
        })
        .mount(&server)
        .await;
    server
}

async fn core(root: &Path, anilist: &MockServer) -> Core {
    core_with_player(root, anilist, None).await
}

async fn core_with_player(
    root: &Path,
    anilist: &MockServer,
    player: Option<Arc<dyn PlayerBackend>>,
) -> Core {
    let paths = Paths::under(root).unwrap();
    Config::default().save(&paths.config_file()).unwrap();
    let options = CoreOptions {
        anilist_endpoint: Some(anilist.uri()),
        player,
    };
    Core::open(paths, options).await.unwrap()
}

/// Spielt eine Folge nach festem Ablauf ab, statt ein Video zu öffnen: Start an der
/// Resume-Position, Pause beim ersten Umschalten, Ende beim zweiten.
#[derive(Default)]
struct ScriptedPlayer {
    opened: Mutex<Vec<OpenRequest>>,
}

struct ScriptedSession {
    events: mpsc::Sender<PlayerEvent>,
    toggles: Mutex<u32>,
}

#[async_trait]
impl PlayerBackend for ScriptedPlayer {
    async fn open(
        &self,
        request: OpenRequest,
    ) -> Result<(SessionHandle, mpsc::Receiver<PlayerEvent>), PlayerError> {
        let start = request.first.start_secs.unwrap_or(0.0);
        self.opened.lock().unwrap().push(request);
        let (events, receiver) = mpsc::channel(32);
        for event in [
            PlayerEvent::Started { index: 0 },
            PlayerEvent::Position {
                position: start,
                duration: Some(4.0),
                paused: false,
            },
        ] {
            events.send(event).await.unwrap();
        }
        let session = ScriptedSession {
            events,
            toggles: Mutex::new(0),
        };
        Ok((Arc::new(session), receiver))
    }
}

#[async_trait]
impl PlayerSession for ScriptedSession {
    async fn append(&self, _item: PlayItem) -> Result<(), PlayerError> {
        Ok(())
    }

    async fn command(&self, command: PlayerCommand) -> Result<(), PlayerError> {
        let script = match command {
            PlayerCommand::TogglePause => {
                let toggles = {
                    let mut count = self.toggles.lock().unwrap();
                    *count += 1;
                    *count
                };
                if toggles == 1 {
                    vec![PlayerEvent::Paused(true)]
                } else {
                    vec![
                        PlayerEvent::Paused(false),
                        PlayerEvent::Position {
                            position: 4.0,
                            duration: Some(4.0),
                            paused: false,
                        },
                        PlayerEvent::Ended {
                            index: 0,
                            reason: EndReason::Eof,
                        },
                        PlayerEvent::Exited,
                    ]
                }
            }
            PlayerCommand::Stop => vec![
                PlayerEvent::Ended {
                    index: 0,
                    reason: EndReason::Quit,
                },
                PlayerEvent::Exited,
            ],
            _ => Vec::new(),
        };
        for event in script {
            self.events
                .send(event)
                .await
                .map_err(|_| PlayerError::Disconnected)?;
        }
        Ok(())
    }
}

fn series() -> LibraryItem {
    LibraryItem {
        id: "s1".into(),
        kind: ItemKind::Series,
        name: "Show".into(),
        overview: None,
        year: Some(2020),
        status: None,
        genres: vec![],
        community_rating: None,
        runtime_secs: None,
        provider_ids: Default::default(),
        has_poster: false,
        has_backdrop: false,
        played: false,
        unplayed_count: None,
        position_secs: 1,
    }
}

fn mapping() -> Mapping {
    Mapping {
        jellyfin_id: "s1".into(),
        season: 1,
        ep_start: 1,
        ep_end: Some(12),
        media_id: 101,
        offset: 0,
        source: MappingSource::Manual,
        confidence: 1.0,
        title: "Show".into(),
    }
}

async fn wait_event(
    events: &mut broadcast::Receiver<CoreEvent>,
    mut matches: impl FnMut(&CoreEvent) -> bool,
) -> CoreEvent {
    timeout(Duration::from_secs(30), async {
        loop {
            match events.recv().await {
                Ok(event) if matches(&event) => return event,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => panic!("event stream closed: {e}"),
            }
        }
    })
    .await
    .expect("timed out waiting for core event")
}

fn saves(requests: &[Request]) -> Vec<Value> {
    requests
        .iter()
        .filter_map(|r| serde_json::from_slice::<Value>(&r.body).ok())
        .filter(|b| {
            b["query"]
                .as_str()
                .is_some_and(|q| q.contains("SaveMediaListEntry"))
        })
        .map(|b| b["variables"].clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn playback_reports_to_jellyfin_and_syncs_to_anilist() {
    let dir = tempfile::tempdir().unwrap();
    let jellyfin = mock_jellyfin().await;
    let anilist = mock_anilist(json!([])).await;
    let player = Arc::new(ScriptedPlayer::default());
    let core = core_with_player(dir.path(), &anilist, Some(player.clone())).await;
    let mut events = core.subscribe();

    core.login_jellyfin(&jellyfin.uri(), "eden", &SecretString::from("pw"))
        .await
        .unwrap();
    core.login_anilist_pin("anilist-token").await.unwrap();
    core.set_mappings("s1", vec![mapping()]).await.unwrap();

    let credentials = std::fs::read_to_string(core.paths().credentials_file()).unwrap();
    assert!(credentials.contains(JELLYFIN_TOKEN));
    assert!(!credentials.contains("pw\""), "password must not be stored");

    core.play(&series(), None).await.unwrap();

    let first = wait_event(&mut events, |e| matches!(e, CoreEvent::Playback(_))).await;
    let CoreEvent::Playback(state) = first else {
        unreachable!()
    };
    assert!(
        state.position >= 0.9,
        "resume position ignored: {}",
        state.position
    );
    assert_eq!(state.item_id, "e1");

    {
        let opened = player.opened.lock().unwrap();
        let request = &opened[0];
        assert_eq!(request.first.start_secs, Some(1.0), "resume position");
        assert!(
            request
                .http_headers
                .iter()
                .any(|h| h.expose_secret().contains(JELLYFIN_TOKEN)),
            "stream requests carry the Jellyfin login"
        );
        assert!(
            !request.first.url.expose_secret().contains(JELLYFIN_TOKEN),
            "the token stays out of the stream URL"
        );
    }

    core.player_command(PlayerCommand::TogglePause)
        .await
        .unwrap();
    wait_event(
        &mut events,
        |e| matches!(e, CoreEvent::Playback(s) if s.paused),
    )
    .await;
    core.player_command(PlayerCommand::TogglePause)
        .await
        .unwrap();

    wait_event(&mut events, |e| matches!(e, CoreEvent::PlaybackEnded)).await;
    assert!(!core.is_playing());

    let requests = jellyfin.received_requests().await.unwrap();
    let count = |p: &str| requests.iter().filter(|r| r.url.path() == p).count();
    assert_eq!(count("/Sessions/Playing"), 1);
    assert!(count("/Sessions/Playing/Progress") >= 1);
    assert_eq!(count("/Sessions/Playing/Stopped"), 1);

    let started = requests
        .iter()
        .find(|r| r.url.path() == "/Sessions/Playing")
        .unwrap();
    let body: Value = serde_json::from_slice(&started.body).unwrap();
    assert_eq!(body["PositionTicks"], 10_000_000);
    assert_eq!(body["PlaySessionId"], "ps1");

    timeout(Duration::from_secs(10), async {
        loop {
            if core.pending_writes().await.unwrap().is_empty()
                && !saves(&anilist.received_requests().await.unwrap()).is_empty()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("AniList write never completed");

    let saved = saves(&anilist.received_requests().await.unwrap());
    assert_eq!(saved.len(), 1, "{saved:?}");
    assert_eq!(saved[0]["mediaId"], 101);
    assert_eq!(saved[0]["progress"], 1);
    assert_eq!(saved[0]["status"], "CURRENT");
    assert!(saved[0]["startedAt"]["year"].is_number());

    let entry = core.entry_for(101).unwrap();
    assert_eq!(
        (entry.id, entry.progress),
        (900, 1),
        "placeholder must be replaced"
    );
}

#[tokio::test]
async fn without_a_player_playback_fails_with_a_hint() {
    let dir = tempfile::tempdir().unwrap();
    let jellyfin = mock_jellyfin().await;
    let anilist = mock_anilist(json!([])).await;
    let core = core(dir.path(), &anilist).await;
    core.login_jellyfin(&jellyfin.uri(), "eden", &SecretString::from("pw"))
        .await
        .unwrap();

    let error = core.play(&series(), None).await.unwrap_err();

    assert!(
        matches!(error, CoreError::Player(PlayerError::Unavailable(_))),
        "{error}"
    );
    assert!(error.to_string().contains("GStreamer"), "{error}");
    assert!(!core.is_playing());
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_writes_stay_queued_and_expired_tokens_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let anilist = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            if body["query"].as_str().unwrap().contains("Viewer") {
                return ResponseTemplate::new(200).set_body_json(json!({"data": {"Viewer": {
                    "id": 7, "name": "eden"
                }}}));
            }
            let media_id = body["variables"]["mediaId"].as_i64();
            if media_id == Some(1) {
                ResponseTemplate::new(503).set_body_string("down")
            } else {
                ResponseTemplate::new(400).set_body_json(json!({
                    "data": null, "errors": [{"message": "Invalid token", "status": 400}]
                }))
            }
        })
        .mount(&anilist)
        .await;
    let core = core(dir.path(), &anilist).await;
    let mut events = core.subscribe();
    core.login_anilist_pin("tok").await.unwrap();

    core.save_entry(EntryPatch {
        progress: Some(3),
        ..EntryPatch::new(1)
    })
    .await
    .unwrap();
    timeout(Duration::from_secs(10), async {
        loop {
            let pending = core.pending_writes().await.unwrap();
            if pending.first().is_some_and(|p| p.attempts == 1) {
                assert!(pending[0].last_error.as_deref().unwrap().contains("503"));
                assert!(!pending[0].failed, "a server error must be retried");
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("write was not attempted");

    core.save_entry(EntryPatch {
        progress: Some(1),
        ..EntryPatch::new(2)
    })
    .await
    .unwrap();
    let event = wait_event(&mut events, |e| matches!(e, CoreEvent::AuthRequired(_))).await;
    assert_eq!(event, CoreEvent::AuthRequired(Service::AniList));
    assert_eq!(
        core.pending_writes().await.unwrap().len(),
        2,
        "nothing may be lost"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let anilist = mock_anilist(json!([])).await;
    {
        let core = core(dir.path(), &anilist).await;
        // Ohne Login nimmt der Kern keine Änderung an.
        assert!(core.save_entry(EntryPatch::new(1)).await.is_err());
        core.login_anilist_pin("tok").await.unwrap();
    }
    let core = core(dir.path(), &anilist).await;
    assert!(
        core.auth_status().anilist_logged_in,
        "token must be restored"
    );
    assert_eq!(core.auth_status().anilist_user.as_deref(), Some("eden"));
}

#[tokio::test(flavor = "multi_thread")]
async fn manual_sync_pushes_and_pulls() {
    let dir = tempfile::tempdir().unwrap();
    let jellyfin = MockServer::start().await;
    let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
    Mock::given(method("GET"))
        .and(path("/System/Info/Public"))
        .respond_with(ok(json!({})))
        .mount(&jellyfin)
        .await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateByName"))
        .respond_with(ok(
            json!({"AccessToken": "t", "User": {"Id": "u1", "Name": "eden"}}),
        ))
        .mount(&jellyfin)
        .await;
    Mock::given(method("GET"))
        .and(path("/Items"))
        .respond_with(ok(json!({"TotalRecordCount": 2, "Items": [
            {"Id": "ahead", "Name": "Ahead", "Type": "Series"},
            {"Id": "behind", "Name": "Behind", "Type": "Series"}
        ]})))
        .mount(&jellyfin)
        .await;
    Mock::given(method("GET"))
        .and(path("/Shows/ahead/Episodes"))
        .respond_with(ok(json!({"Items": [
            episode("a1", 1, true, 0), episode("a2", 2, true, 0), episode("a3", 3, false, 0)
        ]})))
        .mount(&jellyfin)
        .await;
    Mock::given(method("GET"))
        .and(path("/Shows/behind/Episodes"))
        .respond_with(ok(json!({"Items": [
            episode("b1", 1, true, 0), episode("b2", 2, false, 0), episode("b3", 3, false, 0)
        ]})))
        .mount(&jellyfin)
        .await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::path_regex("^/UserPlayedItems/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&jellyfin)
        .await;

    let entry = |id: i64, media_id: i64, progress: u32| json!({"id": id, "status": "CURRENT", "progress": progress, "media": media(media_id)});
    let anilist = mock_anilist(json!([entry(1, 501, 1), entry(2, 502, 2)])).await;
    let core = core(dir.path(), &anilist).await;
    core.login_jellyfin(&jellyfin.uri(), "eden", &SecretString::from("pw"))
        .await
        .unwrap();
    core.login_anilist_pin("tok").await.unwrap();
    core.refresh_list(MediaKind::Anime).await.unwrap();

    let map = |series: &str, media_id: i64| Mapping {
        jellyfin_id: series.into(),
        media_id,
        ..mapping()
    };
    core.set_mappings("ahead", vec![map("ahead", 501)])
        .await
        .unwrap();
    core.set_mappings("behind", vec![map("behind", 502)])
        .await
        .unwrap();

    let rows = core.sync_diff().await.unwrap();
    assert_eq!(rows.len(), 2, "{rows:?}");
    let ahead = rows.iter().find(|r| r.series_id == "ahead").unwrap();
    assert_eq!((ahead.jellyfin_progress, ahead.anilist_progress), (2, 1));
    assert_eq!(ahead.direction, SyncDirection::Push);
    assert_eq!(ahead.status, Some(ListStatus::Current));
    let behind = rows.iter().find(|r| r.series_id == "behind").unwrap();
    assert_eq!(behind.direction, SyncDirection::Pull);

    let report = core.apply_sync(&rows).await.unwrap();
    assert_eq!((report.pushed, report.pulled, report.failed), (1, 1, 0));

    let played: Vec<String> = jellyfin
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().starts_with("/UserPlayedItems/"))
        .map(|r| r.url.path().to_owned())
        .collect();
    assert_eq!(
        played,
        ["/UserPlayedItems/b2"],
        "only the missing episode is marked"
    );

    timeout(Duration::from_secs(10), async {
        loop {
            if !saves(&anilist.received_requests().await.unwrap()).is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("push never reached AniList");
    let saved = saves(&anilist.received_requests().await.unwrap());
    assert_eq!(
        (saved[0]["mediaId"].as_i64(), saved[0]["progress"].as_u64()),
        (Some(501), Some(2))
    );
}

async fn mock_search(candidate_title: &'static str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let query = body["query"].as_str().unwrap();
            let candidate = json!({
                "id": 500, "type": "ANIME", "format": "MOVIE", "episodes": 1,
                "title": {"romaji": candidate_title}, "seasonYear": 2020, "genres": []
            });
            let data = if query.contains("Viewer") {
                json!({"Viewer": {"id": 7, "name": "eden",
                    "mediaListOptions": {"scoreFormat": "POINT_100"}}})
            } else if query.contains("Page") {
                json!({"Page": {"media": [candidate]}})
            } else {
                json!({"Media": candidate})
            };
            ResponseTemplate::new(200)
                .insert_header("X-RateLimit-Limit", "90")
                .set_body_json(json!({"data": data}))
        })
        .mount(&server)
        .await;
    server
}

fn movie() -> LibraryItem {
    LibraryItem {
        id: "m1".into(),
        kind: ItemKind::Movie,
        name: "Garden of Words".into(),
        ..series()
    }
}

#[tokio::test]
async fn weak_matches_are_guessed_kept_changeable_and_confirmable() {
    let dir = tempfile::tempdir().unwrap();
    let anilist = mock_search("The Gardens of Wonder").await;
    let core = core(dir.path(), &anilist).await;
    core.login_anilist_pin("token").await.unwrap();

    let outcome = core.auto_match(&movie()).await.unwrap();

    assert_eq!(outcome, MatchOutcome::Unsure);
    let guessed = core.mappings_for("m1").await.unwrap();
    assert_eq!(guessed[0].source, MappingSource::Guess);
    assert_eq!(guessed[0].media_id, 500);
    let reviews = core.reviews().await.unwrap();
    assert_eq!(reviews[0].candidates[0].media_id, 500);

    core.confirm_mapping("m1").await.unwrap();

    let confirmed = core.mappings_for("m1").await.unwrap();
    assert_eq!(confirmed[0].source, MappingSource::Manual);
    assert_eq!(confirmed[0].confidence, 1.0);
}

#[tokio::test]
async fn hopeless_candidates_stay_unmapped_but_count_as_tried() {
    let dir = tempfile::tempdir().unwrap();
    let anilist = mock_search("Zzyzx").await;
    let core = core(dir.path(), &anilist).await;
    core.login_anilist_pin("token").await.unwrap();

    let outcome = core.auto_match(&movie()).await.unwrap();

    assert_eq!(outcome, MatchOutcome::NoMatch);
    assert!(core.mappings_for("m1").await.unwrap().is_empty());
    assert_eq!(core.reviews().await.unwrap().len(), 1);
}

#[tokio::test]
async fn removing_a_mapping_keeps_the_title_from_being_matched_again() {
    let dir = tempfile::tempdir().unwrap();
    let anilist = mock_search("Garden of Words").await;
    let core = core(dir.path(), &anilist).await;
    core.login_anilist_pin("token").await.unwrap();
    core.auto_match(&movie()).await.unwrap();

    core.unmap("m1").await.unwrap();

    assert!(core.mappings_for("m1").await.unwrap().is_empty());
    assert_eq!(core.reviews().await.unwrap()[0].jellyfin_id, "m1");
}

fn anime(id: i64, title: &str, format: &str, episodes: u32, year: i32) -> Value {
    json!({
        "id": id, "type": "ANIME", "format": format, "episodes": episodes,
        "title": {"romaji": title}, "seasonYear": year, "genres": [], "status": "FINISHED"
    })
}

/// AniList mit Staffel 1 (1), einem Film dazwischen (2) und Staffel 2 (3).
async fn mock_franchise() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let query = body["query"].as_str().unwrap();
            let id = body["variables"]["id"].as_i64().unwrap_or(0);
            let by_id = |id: i64| match id {
                1 => anime(1, "Show", "TV", 12, 2020),
                2 => anime(2, "Show the Movie", "MOVIE", 1, 2021),
                3 => anime(3, "Show 2nd Season", "TV", 12, 2022),
                other => anime(other, "Something Else", "TV", 12, 2010),
            };
            let edge =
                |relation: &str, node: Value| json!({"relationType": relation, "node": node});
            let data = if query.contains("Viewer") {
                json!({"Viewer": {"id": 7, "name": "eden",
                    "mediaListOptions": {"scoreFormat": "POINT_100"}}})
            } else if query.contains("MediaListCollection") {
                json!({"MediaListCollection": {"hasNextChunk": false, "lists": []}})
            } else if query.contains("Page") {
                json!({"Page": {"media": [by_id(1), by_id(3)]}})
            } else if query.contains("relations") {
                let edges = match id {
                    1 => vec![edge("SEQUEL", by_id(2))],
                    2 => vec![edge("PREQUEL", by_id(1)), edge("SEQUEL", by_id(3))],
                    _ => vec![],
                };
                let mut media = by_id(id);
                media["relations"] = json!({"edges": edges});
                json!({"Media": media})
            } else {
                json!({"Media": by_id(id)})
            };
            ResponseTemplate::new(200)
                .insert_header("X-RateLimit-Limit", "90")
                .set_body_json(json!({"data": data}))
        })
        .mount(&server)
        .await;
    server
}

/// Jellyfin mit einer Serie aus zwei Staffeln zu je zwölf Folgen.
async fn mock_two_seasons() -> MockServer {
    let server = MockServer::start().await;
    let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
    Mock::given(method("GET"))
        .and(path("/System/Info/Public"))
        .respond_with(ok(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateByName"))
        .respond_with(ok(
            json!({"AccessToken": "t", "User": {"Id": "u1", "Name": "eden"}}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/Items"))
        .respond_with(ok(json!({"TotalRecordCount": 1, "Items": [
            {"Id": "s1", "Name": "Show", "Type": "Series", "ProductionYear": 2020}
        ]})))
        .mount(&server)
        .await;
    let episodes: Vec<Value> = (1..=2)
        .flat_map(|season| {
            (1..=12).map(move |number| {
                json!({"Id": format!("e{season}x{number}"), "Name": "Ep", "SeriesName": "Show",
                    "ParentIndexNumber": season, "IndexNumber": number})
            })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/Shows/s1/Episodes"))
        .respond_with(ok(json!({ "Items": episodes })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/Shows/s1/Seasons"))
        .respond_with(ok(json!({"Items": [
            {"Id": "x1", "Name": "Season 1", "IndexNumber": 1},
            {"Id": "x2", "Name": "Season 2", "IndexNumber": 2}
        ]})))
        .mount(&server)
        .await;
    server
}

fn season_media(mappings: &[Mapping], season: u32) -> Vec<(i64, MappingSource)> {
    mappings
        .iter()
        .filter(|m| m.season == season)
        .map(|m| (m.media_id, m.source))
        .collect()
}

async fn franchise_core(dir: &Path) -> (Core, MockServer, MockServer) {
    let jellyfin = mock_two_seasons().await;
    let anilist = mock_franchise().await;
    let core = core(dir, &anilist).await;
    core.login_jellyfin(&jellyfin.uri(), "eden", &SecretString::from("pw"))
        .await
        .unwrap();
    core.login_anilist_pin("token").await.unwrap();
    (core, jellyfin, anilist)
}

#[tokio::test]
async fn every_season_gets_its_own_entry_even_across_a_movie() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _jellyfin, _anilist) = franchise_core(dir.path()).await;

    let outcome = core.auto_match(&series()).await.unwrap();

    assert_eq!(outcome, MatchOutcome::Mapped);
    let mappings = core.mappings_for("s1").await.unwrap();
    assert_eq!(season_media(&mappings, 1), vec![(1, MappingSource::Title)]);
    assert_eq!(season_media(&mappings, 2), vec![(3, MappingSource::Title)]);
    let s2 = mappings.iter().find(|m| m.season == 2).unwrap();
    assert_eq!((s2.ep_start, s2.offset), (1, 0));
}

#[tokio::test]
async fn one_season_can_be_changed_without_touching_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _jellyfin, _anilist) = franchise_core(dir.path()).await;
    core.auto_match(&series()).await.unwrap();

    core.map_season_manually(&series(), Some(2), 77, false)
        .await
        .unwrap();
    core.unmap_season("s1", 1).await.unwrap();

    let mappings = core.mappings_for("s1").await.unwrap();
    assert!(season_media(&mappings, 1).is_empty());
    assert_eq!(
        season_media(&mappings, 2),
        vec![(77, MappingSource::Manual)]
    );
}

#[tokio::test]
async fn old_mappings_are_replanned_once_and_manual_seasons_stay() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _jellyfin, _anilist) = franchise_core(dir.path()).await;
    // Stand einer älteren Version: Staffel 1 sicher, Staffel 2 landete auf dem Film.
    let old = |season: u32, media_id: i64, source: MappingSource| Mapping {
        season,
        media_id,
        source,
        confidence: 0.9,
        ..mapping()
    };
    core.set_mappings(
        "s1",
        vec![
            old(1, 1, MappingSource::Title),
            old(2, 2, MappingSource::Guess),
        ],
    )
    .await
    .unwrap();

    core.match_all(false).await.unwrap();

    let mappings = core.mappings_for("s1").await.unwrap();
    assert_eq!(season_media(&mappings, 1), vec![(1, MappingSource::Title)]);
    assert_eq!(season_media(&mappings, 2), vec![(3, MappingSource::Title)]);

    // Ein zweiter Lauf plant nicht noch einmal neu.
    core.set_mappings(
        "s1",
        vec![
            old(1, 50, MappingSource::Manual),
            old(2, 2, MappingSource::Guess),
        ],
    )
    .await
    .unwrap();
    core.match_all(false).await.unwrap();
    let mappings = core.mappings_for("s1").await.unwrap();
    assert_eq!(season_media(&mappings, 2), vec![(2, MappingSource::Guess)]);
}

#[tokio::test]
async fn a_manual_season_survives_the_upgrade_and_the_next_season_searches_itself() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _jellyfin, _anilist) = franchise_core(dir.path()).await;
    let manual = Mapping {
        media_id: 50,
        ..mapping()
    };
    let wrong = Mapping {
        season: 2,
        media_id: 2,
        source: MappingSource::Title,
        ..mapping()
    };
    core.set_mappings("s1", vec![manual, wrong]).await.unwrap();

    core.match_all(false).await.unwrap();

    let mappings = core.mappings_for("s1").await.unwrap();
    assert_eq!(
        season_media(&mappings, 1),
        vec![(50, MappingSource::Manual)]
    );
    assert_eq!(season_media(&mappings, 2), vec![(3, MappingSource::Title)]);
}

#[tokio::test]
async fn profile_statistics_are_fetched_cached_and_cleared_on_logout() {
    let dir = tempfile::tempdir().unwrap();
    let anilist = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let data = if body["query"].as_str().unwrap().contains("statistics") {
                json!({"Viewer": {
                    "name": "eden", "siteUrl": "https://anilist.co/user/eden",
                    "bannerImage": null, "avatar": {"large": "https://img.example/a.png"},
                    "mediaListOptions": {"scoreFormat": "POINT_10"},
                    "statistics": {
                        "anime": {
                            "count": 3, "meanScore": 81.5, "standardDeviation": 6.0,
                            "minutesWatched": 2880, "episodesWatched": 120,
                            "statuses": [{"status": "COMPLETED", "count": 2},
                                {"status": "CURRENT", "count": 1}],
                            "scores": [{"score": 8, "count": 3}, {"score": 10, "count": 1},
                                {"score": 3, "count": 2}],
                            "formats": [{"format": "TV", "count": 3, "meanScore": 81.5,
                                "minutesWatched": 2880}],
                            "genres": [{"genre": "Drama", "count": 2, "meanScore": 84.0,
                                "minutesWatched": 1900}, null],
                            "releaseYears": [{"releaseYear": 2019, "count": 3, "meanScore": 81.5,
                                "minutesWatched": 2880}]
                        },
                        "manga": {"count": 1, "chaptersRead": 40, "volumesRead": 4,
                            "genres": [{"genre": "Fantasy", "count": 1, "chaptersRead": 40}]}
                    }
                }})
            } else {
                json!({"Viewer": {"id": 7, "name": "eden",
                    "mediaListOptions": {"scoreFormat": "POINT_100"}}})
            };
            ResponseTemplate::new(200)
                .insert_header("X-RateLimit-Limit", "90")
                .set_body_json(json!({"data": data}))
        })
        .mount(&anilist)
        .await;
    let core = core(dir.path(), &anilist).await;
    core.login_anilist_pin("token").await.unwrap();
    assert!(core.cached_profile().await.unwrap().is_none());

    let profile = core.refresh_profile().await.unwrap();

    assert_eq!(profile.name, "eden");
    assert_eq!(
        profile.avatar_url.as_deref(),
        Some("https://img.example/a.png")
    );
    assert_eq!(profile.anime.count, 3);
    assert_eq!(profile.anime.days_watched(), 2.0);
    assert_eq!(profile.anime.status_count(ListStatus::Completed), 2);
    // AniList reports the buckets on the user's 1 to 10 scale.
    assert_eq!(profile.anime.score_counts()[7], (80, 3));
    assert_eq!(profile.anime.score_counts()[9], (100, 1));
    assert_eq!(profile.anime.score_counts()[2], (30, 2));
    assert_eq!(profile.anime.genres[0].key, "Drama");
    assert_eq!(profile.anime.genres.len(), 1);
    assert_eq!(profile.anime.release_years[0].key, "2019");
    assert_eq!(profile.manga.genres[0].amount, 40);
    assert!(profile.fetched_at > 0);

    // Die letzte Kopie liegt im Cache und ist ohne weiteren Abruf lesbar.
    assert_eq!(core.cached_profile().await.unwrap(), Some(profile));

    // Beim Abmelden verschwindet sie mit den übrigen AniList-Daten.
    core.logout_anilist().await.unwrap();
    assert!(core.cached_profile().await.unwrap().is_none());
}

#[tokio::test]
async fn matches_show_english_titles_and_can_be_added_to_a_list() {
    let dir = tempfile::tempdir().unwrap();
    let anilist = MockServer::start().await;
    let frieren = json!({
        "id": 101, "type": "ANIME", "format": "TV", "episodes": 28, "status": "FINISHED",
        "title": {"romaji": "Sousou no Frieren", "english": "Frieren: Beyond Journey's End",
            "native": "葬送のフリーレン"}
    });
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let query = body["query"].as_str().unwrap();
            let data = if query.contains("SaveMediaListEntry") {
                json!({"SaveMediaListEntry": {"id": 900, "status": body["variables"]["status"],
                    "score": 0, "progress": 0, "media": frieren.clone()}})
            } else if query.contains("Viewer") {
                json!({"Viewer": {"id": 7, "name": "eden",
                    "mediaListOptions": {"scoreFormat": "POINT_100"}}})
            } else if query.contains("id_in") {
                json!({"Page": {"media": [frieren.clone()]}})
            } else {
                json!({"Media": frieren.clone()})
            };
            ResponseTemplate::new(200)
                .insert_header("X-RateLimit-Limit", "90")
                .set_body_json(json!({"data": data}))
        })
        .mount(&anilist)
        .await;
    let core = core(dir.path(), &anilist).await;
    core.login_anilist_pin("tok").await.unwrap();

    // A match stored by an older version carries the Romaji title.
    let old = Mapping {
        title: "Sousou no Frieren".into(),
        ..mapping()
    };
    core.set_mappings("s1", vec![old]).await.unwrap();
    core.refresh_mapping_titles().await.unwrap();
    let stored = core.mappings_for("s1").await.unwrap();
    assert_eq!(stored[0].title, "Frieren: Beyond Journey's End");

    // The matched work is not on the list yet and gets added by its ID.
    assert!(core.entry_for(101).is_none());
    let title = core
        .add_media_to_list(101, ListStatus::Planning)
        .await
        .unwrap();
    assert_eq!(title, "Frieren: Beyond Journey's End");
    let entry = core.entry_for(101).expect("the entry shows at once");
    assert_eq!(entry.status, ListStatus::Planning);
    assert_eq!(
        entry.media.title.preferred(),
        "Frieren: Beyond Journey's End"
    );
}

/// Jellyfin with one series. Each season is `(number, episodes, AniList ID on the season)`.
async fn mock_seasons(seasons: &[(u32, u32, Option<&str>)]) -> MockServer {
    let server = MockServer::start().await;
    let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
    Mock::given(method("GET"))
        .and(path("/System/Info/Public"))
        .respond_with(ok(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateByName"))
        .respond_with(ok(
            json!({"AccessToken": "t", "User": {"Id": "u1", "Name": "eden"}}),
        ))
        .mount(&server)
        .await;
    let episodes: Vec<Value> = seasons
        .iter()
        .flat_map(|&(season, count, _)| {
            (1..=count).map(move |number| {
                json!({"Id": format!("e{season}x{number}"), "Name": "Ep", "SeriesName": "Show",
                    "ParentIndexNumber": season, "IndexNumber": number})
            })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/Shows/s1/Episodes"))
        .respond_with(ok(json!({ "Items": episodes })))
        .mount(&server)
        .await;
    let metas: Vec<Value> = seasons
        .iter()
        .map(|&(season, _, id)| {
            json!({"Id": format!("x{season}"), "Name": format!("Season {season}"),
                "IndexNumber": season, "ProviderIds": {"AniList": id}})
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/Shows/s1/Seasons"))
        .respond_with(ok(json!({ "Items": metas })))
        .mount(&server)
        .await;
    server
}

/// AniList with a chain of TV entries `(id, title, episodes)`, each the sequel of the one
/// before. The search finds the first.
async fn mock_chain(works: &'static [(i64, &'static str, u32)]) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let query = body["query"].as_str().unwrap();
            let id = body["variables"]["id"].as_i64().unwrap_or(0);
            let position = works.iter().position(|w| w.0 == id);
            let node =
                |&(id, title, episodes): &(i64, &str, u32)| anime(id, title, "TV", episodes, 2020);
            let by_id = || match position {
                Some(index) => node(&works[index]),
                None => anime(id, "Something Else", "TV", 12, 2010),
            };
            let data = if query.contains("Viewer") {
                json!({"Viewer": {"id": 7, "name": "eden",
                    "mediaListOptions": {"scoreFormat": "POINT_100"}}})
            } else if query.contains("MediaListCollection") {
                json!({"MediaListCollection": {"hasNextChunk": false, "lists": []}})
            } else if query.contains("Page") {
                json!({"Page": {"media": [node(&works[0])]}})
            } else if query.contains("relations") {
                let edges: Vec<Value> = position
                    .and_then(|index| works.get(index + 1))
                    .map(|next| json!({"relationType": "SEQUEL", "node": node(next)}))
                    .into_iter()
                    .collect();
                let mut media = by_id();
                media["relations"] = json!({"edges": edges});
                json!({"Media": media})
            } else {
                json!({"Media": by_id()})
            };
            ResponseTemplate::new(200)
                .insert_header("X-RateLimit-Limit", "90")
                .set_body_json(json!({"data": data}))
        })
        .mount(&server)
        .await;
    server
}

async fn chain_core(dir: &Path, jellyfin: &MockServer, anilist: &MockServer) -> Core {
    let core = core(dir, anilist).await;
    core.login_jellyfin(&jellyfin.uri(), "eden", &SecretString::from("pw"))
        .await
        .unwrap();
    core.login_anilist_pin("token").await.unwrap();
    core
}

#[tokio::test]
async fn one_long_entry_spans_many_seasons_despite_wrong_season_ids() {
    let dir = tempfile::tempdir().unwrap();
    // The plugin put the sequel's ID on season 2 and one unrelated ID on seasons 3 and 4.
    let jellyfin = mock_seasons(&[
        (1, 10, Some("100")),
        (2, 10, Some("200")),
        (3, 10, Some("300")),
        (4, 12, Some("300")),
    ])
    .await;
    let anilist = mock_chain(&[(100, "Show", 30), (200, "Show Final Arc", 12)]).await;
    let core = chain_core(dir.path(), &jellyfin, &anilist).await;

    let outcome = core.auto_match(&series()).await.unwrap();

    assert_eq!(outcome, MatchOutcome::Mapped);
    let mappings = core.mappings_for("s1").await.unwrap();
    assert_eq!(resolve_episode(&mappings, 1, 1), Some((100, 1)));
    assert_eq!(resolve_episode(&mappings, 2, 1), Some((100, 11)));
    assert_eq!(resolve_episode(&mappings, 3, 10), Some((100, 30)));
    assert_eq!(resolve_episode(&mappings, 4, 1), Some((200, 1)));
    assert_eq!(resolve_episode(&mappings, 4, 12), Some((200, 12)));
}

#[tokio::test]
async fn a_two_part_season_skips_the_episode_zero_kept_in_specials() {
    let dir = tempfile::tempdir().unwrap();
    let jellyfin = mock_seasons(&[(0, 1, None), (1, 24, None)]).await;
    let anilist = mock_chain(&[(1, "Show", 13), (2, "Show Part 2", 12)]).await;
    let core = chain_core(dir.path(), &jellyfin, &anilist).await;

    let outcome = core.auto_match(&series()).await.unwrap();

    assert_eq!(outcome, MatchOutcome::Mapped);
    let mappings = core.mappings_for("s1").await.unwrap();
    assert_eq!(resolve_episode(&mappings, 1, 1), Some((1, 2)));
    assert_eq!(resolve_episode(&mappings, 1, 12), Some((1, 13)));
    assert_eq!(resolve_episode(&mappings, 1, 13), Some((2, 1)));
    assert_eq!(resolve_episode(&mappings, 1, 24), Some((2, 12)));
}
