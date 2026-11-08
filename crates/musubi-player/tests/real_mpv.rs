//! Spielt ein generiertes Testvideo mit dem echten mpv ab.

use std::time::Duration;

use musubi_player::{
    EndReason, MpvBackend, MpvOptions, OpenRequest, PlayItem, PlayerBackend, PlayerCommand,
    PlayerEvent,
};
use secrecy::SecretString;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

fn mpv_available() -> bool {
    std::process::Command::new("mpv")
        .arg("--version")
        .output()
        .is_ok()
}

fn backend(dir: &std::path::Path) -> MpvBackend {
    MpvBackend::new(MpvOptions {
        binary: "mpv".into(),
        hwdec: "no".into(),
        extra_args: vec![
            "--no-config".into(),
            "--vo=null".into(),
            "--ao=null".into(),
            "--force-window=no".into(),
        ],
        socket_dir: dir.to_path_buf(),
    })
}

fn item(title: &str, seconds: u32, start: Option<f64>) -> PlayItem {
    PlayItem {
        url: SecretString::from(format!(
            "av://lavfi:testsrc=duration={seconds}:size=64x64:rate=10"
        )),
        title: title.into(),
        start_secs: start,
        subtitles: vec![],
    }
}

async fn wait_for(
    events: &mut Receiver<PlayerEvent>,
    mut matches: impl FnMut(&PlayerEvent) -> bool,
) -> PlayerEvent {
    timeout(Duration::from_secs(20), async {
        loop {
            let event = events.recv().await.expect("event stream ended early");
            if matches(&event) {
                return event;
            }
        }
    })
    .await
    .expect("timed out waiting for mpv event")
}

#[tokio::test]
async fn plays_queue_seeks_and_exits_on_its_own() {
    if !mpv_available() {
        eprintln!("mpv not installed, skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let request = OpenRequest {
        http_headers: vec![SecretString::from(
            "Authorization: MediaBrowser Token=\"t\"",
        )],
        first: item("first", 2, Some(0.5)),
    };
    let (session, mut events) = backend(dir.path()).open(request).await.unwrap();

    assert_eq!(
        wait_for(&mut events, |e| matches!(e, PlayerEvent::Started { .. })).await,
        PlayerEvent::Started { index: 0 }
    );
    session.append(item("second", 1, None)).await.unwrap();

    let PlayerEvent::Position { position, .. } =
        wait_for(&mut events, |e| matches!(e, PlayerEvent::Position { .. })).await
    else {
        unreachable!()
    };
    assert!(position >= 0.4, "resume offset was ignored: {position}");

    session.command(PlayerCommand::TogglePause).await.unwrap();
    assert_eq!(
        wait_for(&mut events, |e| matches!(e, PlayerEvent::Paused(true))).await,
        PlayerEvent::Paused(true)
    );
    session.command(PlayerCommand::TogglePause).await.unwrap();

    assert_eq!(
        wait_for(&mut events, |e| matches!(
            e,
            PlayerEvent::Ended { index: 0, .. }
        ))
        .await,
        PlayerEvent::Ended {
            index: 0,
            reason: EndReason::Eof
        }
    );
    assert_eq!(
        wait_for(&mut events, |e| matches!(e, PlayerEvent::Started { .. })).await,
        PlayerEvent::Started { index: 1 }
    );
    wait_for(&mut events, |e| matches!(e, PlayerEvent::Exited)).await;
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "socket not cleaned up"
    );
}

#[tokio::test]
async fn stop_ends_the_process_and_bad_binary_is_reported() {
    if !mpv_available() {
        eprintln!("mpv not installed, skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let request = OpenRequest {
        http_headers: vec![],
        first: item("long", 60, None),
    };
    let (session, mut events) = backend(dir.path()).open(request).await.unwrap();
    wait_for(&mut events, |e| matches!(e, PlayerEvent::Started { .. })).await;
    let _ = session.command(PlayerCommand::Stop).await;
    wait_for(&mut events, |e| matches!(e, PlayerEvent::Exited)).await;

    let missing = MpvBackend::new(MpvOptions {
        binary: "/nonexistent/mpv".into(),
        hwdec: String::new(),
        extra_args: vec![],
        socket_dir: dir.path().to_path_buf(),
    });
    let request = OpenRequest {
        http_headers: vec![],
        first: item("x", 1, None),
    };
    assert!(missing.open(request).await.is_err());
}
