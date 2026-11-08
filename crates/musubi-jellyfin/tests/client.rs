use musubi_domain::{ItemKind, LibraryQuery};
use musubi_jellyfin::{Client, Error, Identity, PlayMethod, ProgressReport};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use wiremock::matchers::{body_partial_json, header_regex, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn identity() -> Identity {
    Identity {
        client: "Musubi".into(),
        device: "test".into(),
        device_id: "dev-1".into(),
        version: "0.1.0".into(),
    }
}

fn client(server: &MockServer) -> Client {
    Client::new(&server.uri(), identity())
        .unwrap()
        .with_token(SecretString::from("tok"))
}

#[tokio::test]
async fn authenticates_with_identity_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateByName"))
        .and(header_regex(
            "authorization",
            r#"^MediaBrowser Client="Musubi".*DeviceId="dev-1""#,
        ))
        .and(body_partial_json(json!({"Username": "eden", "Pw": "pw"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "AccessToken": "abc", "User": {"Id": "u1", "Name": "eden"}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), identity()).unwrap();
    let auth = client
        .authenticate("eden", &SecretString::from("pw"))
        .await
        .unwrap();
    assert_eq!(auth.token.expose_secret(), "abc");
    assert_eq!(auth.user_id, "u1");
}

#[tokio::test]
async fn wrong_token_maps_to_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/UserViews"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server).libraries("u").await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn items_send_filters_and_pages_until_complete() {
    let server = MockServer::start().await;
    let item = |id: &str| {
        json!({
            "Id": id, "Name": id, "Type": "Series", "ProductionYear": 2020,
            "ProviderIds": {"AniList": "123", "Nope": null},
            "ImageTags": {"Primary": "x"},
            "UserData": {"Played": false, "UnplayedItemCount": 4}
        })
    };
    Mock::given(method("GET"))
        .and(path("/Items"))
        .and(query_param("userId", "u"))
        .and(query_param("genres", "Action|Drama, Tragic"))
        .and(query_param("years", "2019,2020"))
        .and(query_param("seriesStatus", "Ended"))
        .and(query_param("startIndex", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Items": [item("a")], "TotalRecordCount": 2
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/Items"))
        .and(query_param("startIndex", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Items": [item("b")], "TotalRecordCount": 2
        })))
        .expect(1)
        .mount(&server)
        .await;

    let query = LibraryQuery {
        genres: vec!["Action".into(), "Drama, Tragic".into()],
        years: vec![2019, 2020],
        status: Some("Ended".into()),
        ..LibraryQuery::default()
    };
    let items = client(&server).all_items("u", &query).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].kind, ItemKind::Series);
    assert_eq!(items[0].provider_id(&["anilist"]), Some("123"));
    assert_eq!(items[0].unplayed_count, Some(4));
    assert!(items[0].has_poster);
}

#[tokio::test]
async fn episodes_are_sorted_with_specials_last() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/Shows/s/Episodes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "Items": [
            {"Id": "sp", "Name": "Special", "ParentIndexNumber": 0, "IndexNumber": 1},
            {"Id": "e2", "Name": "Two", "ParentIndexNumber": 1, "IndexNumber": 2,
             "RunTimeTicks": 14_400_000_000u64,
             "UserData": {"Played": true, "PlaybackPositionTicks": 50_000_000}},
            {"Id": "e1", "Name": "One", "ParentIndexNumber": 1, "IndexNumber": 1},
        ]})))
        .mount(&server)
        .await;
    let episodes = client(&server).episodes("u", "s").await.unwrap();
    let ids: Vec<_> = episodes.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["e1", "e2", "sp"]);
    assert_eq!(episodes[1].runtime_secs, Some(1440));
    assert_eq!(episodes[1].position_secs, 5);
    assert!(episodes[1].played);
}

#[tokio::test]
async fn direct_stream_is_preferred_and_token_stays_out_of_the_url() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/Items/i/PlaybackInfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "PlaySessionId": "ps",
            "MediaSources": [{
                "Id": "ms", "ETag": "tag", "SupportsDirectStream": true,
                "MediaStreams": [
                    {"Type": "Subtitle", "Index": 3, "Codec": "subrip", "IsExternal": true,
                     "Language": "ger", "DisplayTitle": "German"},
                    {"Type": "Subtitle", "Index": 4, "Codec": "ass", "IsExternal": false}
                ]
            }]
        })))
        .mount(&server)
        .await;
    let client = client(&server);
    let target = client.playback_target("u", "i", None).await.unwrap();
    assert_eq!(target.play_method, PlayMethod::DirectStream);
    let url = target.url.expose_secret();
    assert!(
        url.contains("/Videos/i/stream?static=true&mediaSourceId=ms"),
        "{url}"
    );
    assert!(!url.contains("tok"));
    assert_eq!(target.subtitles.len(), 1);
    assert!(
        target.subtitles[0]
            .url
            .expose_secret()
            .ends_with("/Videos/i/ms/Subtitles/3/Stream.srt")
    );
    assert_eq!(
        client.stream_auth_header().unwrap().expose_secret(),
        "Authorization: MediaBrowser Token=\"tok\""
    );
}

#[tokio::test]
async fn transcode_is_the_fallback() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/Items/i/PlaybackInfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "MediaSources": [{"Id": "ms", "TranscodingUrl": "/videos/i/master.m3u8?x=1"}]
        })))
        .mount(&server)
        .await;
    let target = client(&server)
        .playback_target("u", "i", Some(4_000_000))
        .await
        .unwrap();
    assert_eq!(target.play_method, PlayMethod::Transcode);
    assert!(
        target
            .url
            .expose_secret()
            .ends_with("/videos/i/master.m3u8?x=1")
    );
}

#[tokio::test]
async fn progress_uses_ticks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/Items/i/PlaybackInfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "PlaySessionId": "ps", "MediaSources": [{"Id": "ms", "SupportsDirectPlay": true}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Sessions/Playing/Progress"))
        .and(body_partial_json(json!({
            "ItemId": "i", "MediaSourceId": "ms", "PlaySessionId": "ps",
            "PositionTicks": 125_000_000u64, "IsPaused": true
        })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server);
    let target = client.playback_target("u", "i", None).await.unwrap();
    client
        .report_progress(ProgressReport {
            target: &target,
            position_secs: 12.5,
            paused: true,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn set_played_uses_post_and_delete() {
    let server = MockServer::start().await;
    for verb in ["POST", "DELETE"] {
        Mock::given(method(verb))
            .and(path("/UserPlayedItems/i"))
            .and(query_param("userId", "u"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }
    let client = client(&server);
    client.set_played("u", "i", true).await.unwrap();
    client.set_played("u", "i", false).await.unwrap();
}

#[test]
fn rejects_non_http_urls() {
    assert!(matches!(
        Client::new("ftp://x", identity()),
        Err(Error::InvalidUrl(_))
    ));
    assert!(matches!(
        Client::new("not a url", identity()),
        Err(Error::InvalidUrl(_))
    ));
}
