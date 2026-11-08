use std::time::{Duration, Instant};

use musubi_anilist::{Client, Error};
use musubi_domain::{EntryPatch, FuzzyDate, ListStatus, MediaKind, ScoreFormat};
use secrecy::SecretString;
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, header, method};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn client(server: &MockServer) -> Client {
    Client::with_endpoint(&server.uri(), Some(SecretString::from("tok"))).unwrap()
}

fn media(id: i64) -> Value {
    json!({
        "id": id, "idMal": 5, "type": "ANIME",
        "title": {"romaji": "Foo", "english": null, "native": "フー"},
        "synonyms": ["Bar", null], "format": "TV", "status": "FINISHED", "episodes": 12,
        "seasonYear": null, "startDate": {"year": 2019},
        "coverImage": {"large": "http://x/a.jpg"},
        "description": "Line<br>two", "genres": ["Action"], "averageScore": 80
    })
}

fn entry(id: i64, media_id: i64) -> Value {
    json!({
        "id": id, "status": "CURRENT", "score": 85.0, "progress": 3, "progressVolumes": 0,
        "repeat": 1, "priority": 2, "private": true, "hiddenFromStatusLists": false,
        "notes": "n", "customLists": {"Fav": true, "Other": false},
        "advancedScores": {"Story": 7.0}, "updatedAt": 99,
        "startedAt": {"year": 2024, "month": 1, "day": null},
        "completedAt": {"year": null, "month": null, "day": null},
        "media": media(media_id)
    })
}

#[tokio::test]
async fn viewer_is_read_with_bearer_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data": {"Viewer": {
                "id": 7, "name": "eden",
                "mediaListOptions": {
                    "scoreFormat": "POINT_5",
                    "animeList": {"customLists": ["Fav"], "advancedScoring": ["Story"]},
                    "mangaList": {"customLists": []}
                }
            }}})),
        )
        .mount(&server)
        .await;
    let viewer = client(&server).viewer().await.unwrap();
    assert_eq!(viewer.name, "eden");
    assert_eq!(viewer.score_format, ScoreFormat::Point5);
    assert_eq!(viewer.custom_lists(MediaKind::Anime), ["Fav"]);
}

#[tokio::test]
async fn list_walks_chunks_and_removes_duplicates() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let chunk = body["variables"]["chunk"].as_i64().unwrap();
            let (entries, more) = if chunk == 1 {
                (json!([entry(1, 10), entry(2, 20)]), true)
            } else {
                (json!([entry(2, 20), entry(3, 30)]), false)
            };
            ResponseTemplate::new(200).set_body_json(json!({"data": {"MediaListCollection": {
                "hasNextChunk": more, "lists": [{"entries": entries}]
            }}}))
        })
        .expect(2)
        .mount(&server)
        .await;

    let entries = client(&server).list(7, MediaKind::Anime).await.unwrap();
    assert_eq!(entries.len(), 3);
    let first = &entries[0];
    assert_eq!(first.score_raw, 85);
    assert_eq!(first.status, ListStatus::Current);
    assert_eq!(first.custom_lists, ["Fav"]);
    assert_eq!(first.advanced_scores, [("Story".to_owned(), 7.0)]);
    assert_eq!(first.media.season_year, Some(2019));
    assert_eq!(first.media.synonyms, ["Bar"]);
    assert_eq!(first.media.description.as_deref(), Some("Linetwo"));
    assert_eq!(
        first.started_at,
        FuzzyDate {
            year: Some(2024),
            month: Some(1),
            day: None
        }
    );
}

#[tokio::test]
async fn save_sends_only_set_fields() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("SaveMediaListEntry"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": {"SaveMediaListEntry": entry(1, 10)}})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let patch = EntryPatch {
        status: Some(ListStatus::Completed),
        progress: Some(12),
        completed_at: Some(FuzzyDate::default()),
        custom_lists: Some(vec!["Fav".into()]),
        ..EntryPatch::new(10)
    };
    client.save_entry(&patch).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["variables"],
        json!({
            "mediaId": 10, "status": "COMPLETED", "progress": 12, "customLists": ["Fav"],
            "completedAt": {"year": null, "month": null, "day": null}
        })
    );
}

#[tokio::test]
async fn honours_retry_after_on_429_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .insert_header("X-RateLimit-Limit", "90"),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": {"DeleteMediaListEntry": {"deleted": true}}})),
        )
        .with_priority(2)
        .mount(&server)
        .await;

    let start = Instant::now();
    assert!(client(&server).delete_entry(1).await.unwrap());
    assert!(start.elapsed() >= Duration::from_secs(1));
}

#[tokio::test]
async fn invalid_token_and_not_found_are_classified() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("idMal"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "data": {"Media": null}, "errors": [{"message": "Not Found.", "status": 404}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "data": null, "errors": [{"message": "Invalid token", "status": 400}]
        })))
        .mount(&server)
        .await;

    let client = client(&server);
    assert!(matches!(
        client.media_by_mal(1, MediaKind::Anime).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(client.viewer().await, Err(Error::Unauthorized)));
}

#[tokio::test]
async fn relations_are_returned() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data": {"Media": {
                "id": 1, "title": {"romaji": "A"},
                "relations": {"edges": [
                    {"relationType": "SEQUEL", "node": media(2)},
                    {"relationType": "PREQUEL", "node": null}
                ]}
            }}})),
        )
        .mount(&server)
        .await;
    let (base, relations) = client(&server).relations(1).await.unwrap();
    assert_eq!(base.id, 1);
    assert_eq!(relations.len(), 1);
    assert_eq!(
        (relations[0].relation.as_str(), relations[0].media.id),
        ("SEQUEL", 2)
    );
}

#[tokio::test]
async fn deleting_an_entry_that_is_gone_reports_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "data": {"DeleteMediaListEntry": null},
            "errors": [{"message": "validation", "status": 400,
                        "validation": {"id": ["The selected id is invalid."]}}]
        })))
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server).delete_entry(1).await,
        Err(Error::NotFound)
    ));
}
