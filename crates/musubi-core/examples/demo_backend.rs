//! Lokale Attrappen von Jellyfin und AniList, um die GUI ohne echte Server zu starten.
//!
//! ```text
//! cargo run -p musubi-core --example demo_backend
//! ```
//! Gibt die URLs aus und läuft, bis es beendet wird. Jedes Passwort wird akzeptiert.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const SHOWS: [(&str, &str, i32, u32, &str); 6] = [
    (
        "s-frieren",
        "Frieren: Beyond Journey's End",
        2023,
        28,
        "Ended",
    ),
    ("s-mob", "Mob Psycho 100", 2016, 12, "Ended"),
    ("s-apoth", "The Apothecary Diaries", 2023, 24, "Continuing"),
    ("s-hxh", "Hunter x Hunter", 2011, 148, "Ended"),
    ("s-bocchi", "Bocchi the Rock!", 2022, 12, "Ended"),
    (
        "s-unknown",
        "A Very Obscure Local Recording",
        2001,
        3,
        "Ended",
    ),
];

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn series_json((id, name, year, episodes, status): (&str, &str, i32, u32, &str)) -> Value {
    json!({
        "Id": id, "Name": name, "Type": "Series", "ProductionYear": year, "Status": status,
        "Overview": format!("Demo overview of {name}."),
        "Genres": ["Fantasy", "Adventure"], "CommunityRating": 8.4,
        "UserData": {"Played": false, "UnplayedItemCount": episodes},
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn episode_json(series: &str, title: &str, number: u32, position_ticks: u64) -> Value {
    json!({
        "Id": format!("{series}-e{number}"), "Name": format!("Episode {number}"), "Type": "Episode",
        "SeriesId": series, "SeriesName": title, "ParentIndexNumber": 1, "IndexNumber": number,
        "Overview": format!("Demo overview of {title}, episode {number}."),
        "RunTimeTicks": 14_400_000_000u64,
        "UserData": {"Played": false, "PlaybackPositionTicks": position_ticks}
    })
}

fn media_json(id: i64, title: &str, year: i32, episodes: u32) -> Value {
    // Jeder zweite Titel läuft noch und zeigt so die Airing-Ansicht.
    let airing = id % 2 == 1;
    json!({
        "id": id, "type": "ANIME", "title": {"romaji": title, "native": null}, "format": "TV",
        "status": if airing { "RELEASING" } else { "FINISHED" }, "episodes": episodes,
        "seasonYear": year, "genres": ["Fantasy"],
        "description": format!("AniList description of {title}."),
        "nextAiringEpisode": airing.then(|| json!({
            "airingAt": unix_now() + 90_000 * id, "episode": episodes.min(8)
        })),
    })
}

fn entry_json(
    id: i64,
    media_id: i64,
    title: &str,
    year: i32,
    eps: u32,
    status: &str,
    progress: u32,
) -> Value {
    json!({
        "id": id, "status": status, "score": 85, "progress": progress, "repeat": 0, "priority": 0,
        "private": false, "hiddenFromStatusLists": false, "notes": "", "customLists": {"Favourites": true},
        "advancedScores": {}, "updatedAt": 0, "startedAt": {"year": 2024, "month": 3, "day": 1},
        "completedAt": {}, "media": media_json(media_id, title, year, eps)
    })
}

async fn jellyfin(video: Option<PathBuf>) -> MockServer {
    let server = MockServer::start().await;
    let items: Vec<Value> = SHOWS.iter().copied().map(series_json).collect();
    let total = items.len();
    Mock::given(method("GET"))
        .and(path("/System/Info/Public"))
        .respond_with(ok(json!({"ServerName": "Demo"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Users/AuthenticateByName"))
        .respond_with(ok(
            json!({"AccessToken": "demo-token", "User": {"Id": "demo", "Name": "demo"}}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/UserViews"))
        .respond_with(ok(
            json!({"Items": [{"Id": "lib", "Name": "Anime", "CollectionType": "tvshows"}]}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/Items/Filters"))
        .respond_with(ok(
            json!({"Genres": ["Adventure", "Fantasy"], "Years": [2016, 2023]}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/UserItems/Resume"))
        .respond_with(ok(json!({"Items": [
            episode_json("s-frieren", SHOWS[0].1, 3, 7_200_000_000),
            episode_json("s-mob", SHOWS[1].1, 5, 3_000_000_000),
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/Shows/NextUp"))
        .respond_with(ok(json!({"Items": [
            episode_json("s-apoth", SHOWS[2].1, 4, 0),
            episode_json("s-bocchi", SHOWS[4].1, 9, 0),
        ]})))
        .mount(&server)
        .await;
    let latest: Vec<Value> = SHOWS.iter().map(|s| series_json(*s)).collect();
    Mock::given(method("GET"))
        .and(path("/Items/Latest"))
        .respond_with(ok(json!(latest)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/Items"))
        .respond_with(ok(json!({"Items": items, "TotalRecordCount": total})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/Items/s-[a-z]+$"))
        .respond_with(|request: &Request| {
            let id = request.url.path().rsplit('/').next().unwrap_or_default();
            match SHOWS.iter().find(|s| s.0 == id) {
                Some(show) => ok(series_json(*show)),
                None => ResponseTemplate::new(404),
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path_regex("^/Shows/[^/]+/Episodes$"))
        .respond_with(|request: &Request| {
            let series = request.url.path().split('/').nth(2).unwrap_or("x").to_owned();
            let count = SHOWS.iter().find(|s| s.0 == series).map_or(3, |s| s.3.min(12));
            let items: Vec<Value> = (1..=count).map(|n| json!({
                "Id": format!("{series}-e{n}"), "Name": format!("Episode {n}"), "SeriesName": "Demo",
                "ParentIndexNumber": 1, "IndexNumber": n, "RunTimeTicks": 14_400_000_000u64,
                "UserData": {"Played": n <= 2, "PlaybackPositionTicks": 0}
            })).collect();
            ok(json!({"Items": items}))
        })
        .mount(&server).await;
    Mock::given(method("POST")).and(path_regex("^/Items/[^/]+/PlaybackInfo$"))
        .respond_with(ok(json!({"PlaySessionId": "ps", "MediaSources": [{"Id": "ms", "SupportsDirectStream": true}]})))
        .mount(&server).await;
    Mock::given(method("POST"))
        .and(path_regex("^/(Sessions/|UserPlayedItems/)"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    if let Some(video) = video {
        let bytes = std::fs::read(video).expect("video file is readable");
        Mock::given(method("GET"))
            .and(path_regex("^/Videos/[^/]+/stream$"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "video/x-matroska")
                    .set_body_bytes(bytes),
            )
            .mount(&server)
            .await;
    }
    server
}

async fn anilist() -> MockServer {
    let server = MockServer::start().await;
    let entries = Arc::new(Mutex::new(vec![
        entry_json(1, 101, "Sousou no Frieren", 2023, 28, "CURRENT", 2),
        entry_json(2, 102, "Mob Psycho 100", 2016, 12, "COMPLETED", 12),
        entry_json(3, 103, "Kusuriya no Hitorigoto", 2023, 24, "PLANNING", 0),
        entry_json(4, 104, "Mob Psycho 100 II", 2019, 13, "CURRENT", 6),
    ]));
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
            let query = body["query"].as_str().unwrap_or_default();
            let vars = &body["variables"];
            let data = if query.contains("SaveMediaListEntry") {
                let id = vars["mediaId"].as_i64().unwrap_or(0);
                let mut entries = entries.lock().unwrap();
                let entry = match entries.iter_mut().find(|e| e["media"]["id"] == id) {
                    Some(entry) => entry,
                    None => {
                        entries.push(entry_json(1000 + id, id, "New title", 2024, 12, "PLANNING", 0));
                        entries.last_mut().unwrap()
                    }
                };
                if let Some(v) = vars.get("progress") { entry["progress"] = v.clone(); }
                if let Some(v) = vars.get("status") { entry["status"] = v.clone(); }
                if let Some(v) = vars.get("scoreRaw") { entry["score"] = v.clone(); }
                if let Some(v) = vars.get("notes") { entry["notes"] = v.clone(); }
                json!({"SaveMediaListEntry": entry.clone()})
            } else if query.contains("DeleteMediaListEntry") {
                let id = vars["id"].as_i64().unwrap_or(0);
                entries.lock().unwrap().retain(|e| e["id"] != id);
                json!({"DeleteMediaListEntry": {"deleted": true}})
            } else if query.contains("UpdateUser") {
                json!({"UpdateUser": {"id": 1}})
            } else if query.contains("Viewer") {
                json!({"Viewer": {"id": 1, "name": "demo-user", "mediaListOptions": {
                    "scoreFormat": "POINT_10_DECIMAL",
                    "animeList": {"customLists": ["Favourites"], "advancedScoring": []},
                    "mangaList": {"customLists": []}
                }}})
            } else if query.contains("MediaListCollection") {
                let list = if vars["type"] == "ANIME" { entries.lock().unwrap().clone() } else { vec![] };
                json!({"MediaListCollection": {"hasNextChunk": false, "lists": [{"entries": list}]}})
            } else if query.contains("Page") {
                let text = vars["search"].as_str().unwrap_or_default().to_lowercase();
                let media: Vec<Value> = if text.contains("frieren") {
                    vec![media_json(101, "Frieren: Beyond Journey's End", 2023, 28)]
                } else if text.contains("mob") {
                    vec![media_json(102, "Mob Psycho 100", 2016, 12)]
                } else if text.contains("hunter") {
                    vec![media_json(201, "Hunter x Hunter", 1999, 62), media_json(202, "Hunter x Hunter", 2011, 148)]
                } else {
                    vec![media_json(301, "Something Unrelated", 2010, 26)]
                };
                json!({"Page": {"media": media}})
            } else {
                let id = vars["id"].as_i64().unwrap_or(101);
                let mut media = media_json(id, "Demo work", 2023, 28);
                media["relations"] = json!({"edges": []});
                json!({"Media": media})
            };
            ok(json!({"data": data})).insert_header("X-RateLimit-Limit", "90")
        })
        .mount(&server)
        .await;
    server
}

#[tokio::main]
async fn main() {
    let video = std::env::args().nth(1).map(PathBuf::from);
    let (jf, al) = (jellyfin(video).await, anilist().await);
    println!("JELLYFIN={}", jf.uri());
    println!("ANILIST={}", al.uri());
    std::future::pending::<()>().await;
}
