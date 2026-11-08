//! Durchstich gegen Mock-Server für Jellyfin und AniList und das echte mpv.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use musubi_core::domain::{
    EntryPatch, ItemKind, LibraryItem, ListStatus, Mapping, MappingSource, MediaKind,
};
use musubi_core::{
    Config, Core, CoreEvent, CoreOptions, Paths, PlayerCommand, Service, SyncDirection,
};
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio::time::timeout;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const JELLYFIN_TOKEN: &str = "jf-secret-token";

fn have(binary: &str) -> bool {
    Command::new(binary).arg("-version").output().is_ok()
        || Command::new(binary).arg("--version").output().is_ok()
}

fn test_video(dir: &Path) -> Vec<u8> {
    let path = dir.join("video.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=4:size=64x64:rate=10",
        ])
        .args(["-c:v", "mpeg4", "-y"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::read(path).unwrap()
}

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

/// Beantwortet Range-Anfragen wie der echte Server, sonst kann mpv nicht zur Resume-Position.
fn ranged(request: &Request, video: &[u8]) -> ResponseTemplate {
    let base = |status| {
        ResponseTemplate::new(status)
            .insert_header("Content-Type", "video/x-matroska")
            .insert_header("Accept-Ranges", "bytes")
    };
    let start = request
        .headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.split('-').next())
        .and_then(|v| v.parse::<usize>().ok());
    match start {
        Some(start) if start < video.len() => base(206)
            .insert_header(
                "Content-Range",
                format!("bytes {start}-{}/{}", video.len() - 1, video.len()),
            )
            .set_body_bytes(video[start..].to_vec()),
        Some(_) => base(416),
        None => base(200).set_body_bytes(video.to_vec()),
    }
}

async fn mock_jellyfin(video: Vec<u8>) -> MockServer {
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
        .and(path("/Videos/e1/stream"))
        .respond_with(move |request: &Request| ranged(request, &video))
        .mount(&server)
        .await;
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
    let paths = Paths::under(root).unwrap();
    let mut config = Config::default();
    config.player.hwdec = "no".into();
    config.player.extra_args = ["--no-config", "--vo=null", "--ao=null", "--force-window=no"]
        .map(String::from)
        .to_vec();
    config.save(&paths.config_file()).unwrap();
    let options = CoreOptions {
        anilist_endpoint: Some(anilist.uri()),
        player: None,
    };
    Core::open(paths, options).await.unwrap()
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

/// Prüft die Kommandozeilen aller Prozesse, die den Socket dieses Tests benutzen.
fn cmdlines_mentioning(needle: &str) -> Vec<String> {
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|entry| std::fs::read(entry.path().join("cmdline")).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).replace('\0', " "))
        .filter(|line| line.contains(needle))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn playback_reports_to_jellyfin_and_syncs_to_anilist() {
    if !have("mpv") || !have("ffmpeg") {
        eprintln!("mpv or ffmpeg missing, skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let jellyfin = mock_jellyfin(test_video(dir.path())).await;
    let anilist = mock_anilist(json!([])).await;
    let core = core(dir.path(), &anilist).await;
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

    let socket_dir = core.paths().runtime_dir.display().to_string();
    let lines = cmdlines_mentioning(&format!("--input-ipc-server={socket_dir}"));
    assert!(!lines.is_empty(), "mpv process not found");
    for line in &lines {
        assert!(
            !line.contains(JELLYFIN_TOKEN),
            "token leaked into argv: {line}"
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

    let stream = requests
        .iter()
        .find(|r| r.url.path() == "/Videos/e1/stream")
        .unwrap();
    assert_eq!(
        stream
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("MediaBrowser Token=\"{JELLYFIN_TOKEN}\"")
    );
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
