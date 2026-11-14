//! Lokale Attrappen von Jellyfin und AniList, um die GUI ohne echte Server zu starten.
//!
//! ```text
//! cargo run -p musubi-core --example demo_backend
//! ```
//! Gibt die URLs aus und läuft, bis es beendet wird. Jedes Passwort wird akzeptiert.
//!
//! Mit `--login` meldet es zusätzlich das Profil unter den üblichen XDG-Pfaden an beiden Attrappen
//! an. Zusammen mit eigenen `XDG_*`-Verzeichnissen startet die GUI so direkt mit Inhalt, etwa für
//! Bildschirmfotos.
//!
//! With `--update` it also stands in for GitHub and announces a newer release. Point
//! `MUSUBI_DEV_UPDATE_ENDPOINT` at the printed `UPDATE` URL to see the update button.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
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

/// Adresse der Jellyfin-Attrappe, die auch die Cover für AniList ausliefert.
static IMAGE_BASE: OnceLock<String> = OnceLock::new();

/// Unkomprimiertes PNG, damit die Attrappe ohne Bildbibliothek auskommt.
fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & (!(crc & 1)).wrapping_add(1));
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity((width * height * 3 + height) as usize);
    for y in 0..height {
        raw.push(0);
        for x in 0..width {
            raw.extend(pixel(x, y));
        }
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(u8::from(blocks.peek().is_none()));
        zlib.extend((block.len() as u16).to_le_bytes());
        zlib.extend((!(block.len() as u16)).to_le_bytes());
        zlib.extend(block);
    }
    zlib.extend(((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut header = Vec::new();
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Erfundenes Titelbild: Verlauf, Lichtkreis und Horizont, je Titel in eigenen Farben.
fn artwork(seed: &str, width: u32, height: u32) -> Vec<u8> {
    let hash = seed.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let hue = |shift: u32| -> [f32; 3] {
        let h = ((hash >> shift) % 360) as f32 / 60.0;
        let x = 1.0 - (h % 2.0 - 1.0).abs();
        match h as u32 {
            0 => [1.0, x, 0.0],
            1 => [x, 1.0, 0.0],
            2 => [0.0, 1.0, x],
            3 => [0.0, x, 1.0],
            4 => [x, 0.0, 1.0],
            _ => [1.0, 0.0, x],
        }
    };
    let (top, bottom) = (hue(0), hue(9));
    let (cx, cy) = (
        0.25 + (hash % 50) as f32 / 100.0,
        0.2 + ((hash >> 5) % 30) as f32 / 100.0,
    );
    let horizon = 0.58 + ((hash >> 12) % 20) as f32 / 100.0;
    png(width, height, |x, y| {
        let (u, v) = (x as f32 / width as f32, y as f32 / height as f32);
        let aspect = width as f32 / height as f32;
        let distance = (((u - cx) * aspect).powi(2) + (v - cy).powi(2)).sqrt();
        let glow = (1.0 - distance / 0.42).clamp(0.0, 1.0).powi(2) * 0.55;
        let ground = if v > horizon + (u * 9.0).sin() * 0.03 {
            0.45
        } else {
            1.0
        };
        let mut rgb = [0u8; 3];
        for i in 0..3 {
            let sky = top[i] * (1.0 - v) * 0.55 + bottom[i] * v * 0.5 + 0.10;
            rgb[i] = (((sky + glow) * ground).clamp(0.0, 1.0) * 255.0) as u8;
        }
        rgb
    })
}

fn image_response(request: &Request) -> ResponseTemplate {
    let path = request.url.path();
    let seed = path.split('/').nth(2).unwrap_or("x");
    let (width, height) = if path.ends_with("Primary") && !seed.contains("-e") {
        (240, 360)
    } else if path.ends_with("Square") {
        (160, 160)
    } else {
        (640, 360)
    };
    ResponseTemplate::new(200)
        .insert_header("Content-Type", "image/png")
        .set_body_bytes(artwork(&format!("{seed}{}", path.len()), width, height))
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn series_json((id, name, year, episodes, status): (&str, &str, i32, u32, &str)) -> Value {
    json!({
        "Id": id, "Name": name, "Type": "Series", "ProductionYear": year, "Status": status,
        "Overview": format!("Demo overview of {name}."),
        "Genres": ["Fantasy", "Adventure"], "CommunityRating": 8.4,
        "ImageTags": {"Primary": "demo"}, "BackdropImageTags": ["demo"],
        "UserData": {"Played": id == "s-mob", "UnplayedItemCount": if id == "s-mob" { 0 } else { episodes }},
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
        "ImageTags": {"Primary": "demo"},
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
        "seasonYear": year, "genres": ["Fantasy"], "averageScore": 80 + id % 15,
        "coverImage": {"large": format!(
            "{}/Items/al-{id}/Images/Primary",
            IMAGE_BASE.get().map_or("", String::as_str)
        )},
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

/// Profil mit Statistik, wie AniList es für `Viewer { statistics }` liefert.
fn profile_json() -> Value {
    let base = IMAGE_BASE.get().map_or("", String::as_str);
    let statuses = |amount: &str| -> Value {
        [("COMPLETED", 212, 78.4, 61_300), ("CURRENT", 14, 81.0, 1_900), ("PLANNING", 96, 0.0, 0),
         ("PAUSED", 9, 74.5, 1_450), ("DROPPED", 17, 52.3, 1_100), ("REPEATING", 2, 90.0, 600)]
            .iter()
            .map(|(status, count, mean, value)| json!({"status": status, "count": count, "meanScore": mean, amount: value}))
            .collect()
    };
    // AniList reports score buckets in the user's own scale, here 1 to 10.
    let scores: Vec<Value> = [
        (1, 1),
        (2, 2),
        (3, 4),
        (4, 9),
        (5, 17),
        (6, 34),
        (7, 71),
        (8, 68),
        (9, 31),
        (10, 9),
    ]
    .iter()
    .map(|(score, count)| json!({"score": score, "count": count}))
    .collect();
    let genres = |amount: &str| -> Value {
        [("Drama", 148, 78.9), ("Comedy", 131, 76.2), ("Slice of Life", 96, 79.4), ("Fantasy", 88, 77.0),
         ("Adventure", 74, 78.1), ("Sci-Fi", 59, 76.6), ("Mystery", 41, 80.2), ("Psychological", 37, 81.3),
         ("Romance", 35, 73.8), ("Sports", 22, 79.0), ("Music", 14, 82.5), ("Horror", 9, 70.4)]
            .iter()
            .map(|(genre, count, mean)| json!({"genre": genre, "count": count, "meanScore": mean, amount: count * 290}))
            .collect()
    };
    let years: Vec<Value> = (1998..=2025)
        .map(|year| json!({"releaseYear": year, "count": 3 + (year * 7 % 19), "meanScore": 70.0 + f64::from(year % 9)}))
        .collect();
    let formats = |names: &[(&str, u32)], amount: &str| -> Value {
        names
            .iter()
            .map(|(format, count)| json!({"format": format, "count": count, "meanScore": 77.0, amount: count * 260}))
            .collect()
    };
    json!({
        "id": 1, "name": "demo-user",
        "avatar": {"large": format!("{base}/Items/avatar/Images/Square")},
        "bannerImage": format!("{base}/Items/banner/Images/Backdrop"),
        "siteUrl": "https://anilist.co/user/demo-user",
        "mediaListOptions": {"scoreFormat": "POINT_10_DECIMAL"},
        "statistics": {
            "anime": {
                "count": 350, "meanScore": 76.8, "standardDeviation": 13.2,
                "minutesWatched": 66_350, "episodesWatched": 2_741,
                "statuses": statuses("minutesWatched"), "scores": scores,
                "formats": formats(&[("TV", 231), ("MOVIE", 58), ("OVA", 27), ("ONA", 21), ("SPECIAL", 9), ("TV_SHORT", 4)], "minutesWatched"),
                "genres": genres("minutesWatched"), "releaseYears": years,
            },
            "manga": {
                "count": 64, "meanScore": 81.3, "standardDeviation": 9.7,
                "chaptersRead": 4_812, "volumesRead": 391,
                "statuses": statuses("chaptersRead"), "scores": scores,
                "formats": formats(&[("MANGA", 55), ("ONE_SHOT", 6), ("NOVEL", 3)], "chaptersRead"),
                "genres": genres("chaptersRead"), "releaseYears": years,
            },
        },
    })
}

async fn jellyfin(video: Option<PathBuf>) -> MockServer {
    let server = MockServer::start().await;
    let _ = IMAGE_BASE.set(server.uri());
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
                "ParentIndexNumber": if n > 8 { 2 } else { 1 }, "IndexNumber": n,
                "RunTimeTicks": 14_400_000_000u64, "ImageTags": {"Primary": "demo"},
                "Overview": format!("Demo overview of episode {n}: the party travels on and meets someone from the past."),
                "UserData": {"Played": n <= 2, "PlaybackPositionTicks": if n == 3 { 6_000_000_000u64 } else { 0 }}
            })).collect();
            ok(json!({"Items": items}))
        })
        .mount(&server).await;
    Mock::given(method("GET"))
        .and(path_regex("^/Items/[^/]+/Images/[A-Za-z]+$"))
        .respond_with(image_response)
        .mount(&server)
        .await;
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
        entry_json(5, 105, "Bocchi the Rock!", 2022, 12, "CURRENT", 8),
        entry_json(6, 106, "Hunter x Hunter (2011)", 2011, 148, "PAUSED", 61),
        entry_json(7, 107, "Dungeon Meshi", 2024, 24, "CURRENT", 20),
        entry_json(8, 108, "Vinland Saga", 2019, 24, "COMPLETED", 24),
        entry_json(9, 109, "Odd Taxi", 2021, 13, "COMPLETED", 13),
        entry_json(
            10,
            110,
            "Ping Pong the Animation",
            2014,
            11,
            "COMPLETED",
            11,
        ),
        entry_json(11, 111, "Sonny Boy", 2021, 12, "DROPPED", 4),
        entry_json(12, 112, "Mushishi", 2005, 26, "PLANNING", 0),
        entry_json(13, 113, "Girls Band Cry", 2024, 13, "REPEATING", 5),
    ]));
    let manga: Vec<Value> = [
        (21, 501, "Berserk", "CURRENT", 214),
        (22, 502, "Blue Period", "CURRENT", 48),
        (23, 503, "Yotsuba&!", "COMPLETED", 110),
        (24, 504, "Witch Hat Atelier", "PLANNING", 0),
    ]
    .into_iter()
    .map(|(id, media_id, title, status, progress)| {
        let mut entry = entry_json(id, media_id, title, 2018, 0, status, progress);
        entry["media"]["type"] = json!("MANGA");
        entry["media"]["format"] = json!("MANGA");
        entry["media"]["episodes"] = Value::Null;
        entry["media"]["nextAiringEpisode"] = Value::Null;
        entry
    })
    .collect();
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
            } else if query.contains("statistics") {
                json!({"Viewer": profile_json()})
            } else if query.contains("Viewer") {
                json!({"Viewer": {"id": 1, "name": "demo-user", "mediaListOptions": {
                    "scoreFormat": "POINT_10_DECIMAL",
                    "animeList": {"customLists": ["Favourites"], "advancedScoring": []},
                    "mangaList": {"customLists": []}
                }}})
            } else if query.contains("MediaListCollection") {
                let list = if vars["type"] == "ANIME" { entries.lock().unwrap().clone() } else { manga.clone() };
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

/// Stands in for GitHub in the update check and announces a release far ahead of any
/// real one, so the update button and badge can be seen.
async fn github() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": "v99.0.0",
            "html_url": "https://github.com/Koxility/musubi/releases/latest",
            "assets": [],
        })))
        .mount(&server)
        .await;
    server
}

/// Meldet das Profil unter den XDG-Pfaden an beiden Attrappen an.
async fn login(jellyfin: &str, anilist: &str) {
    use musubi_core::{Core, CoreOptions, Paths};
    let paths = Paths::resolve().expect("XDG paths resolve");
    let options = CoreOptions {
        anilist_endpoint: Some(anilist.to_owned()),
        ..CoreOptions::default()
    };
    let core = Core::open(paths, options).await.expect("core opens");
    core.login_jellyfin(jellyfin, "demo", &"demo".to_owned().into())
        .await
        .expect("Jellyfin login");
    core.login_anilist_pin("demo-token")
        .await
        .expect("AniList login");
    core.update_config(|c| c.ui.setup_completed = true)
        .expect("config saves");
    println!("LOGGED_IN=1");
}

#[tokio::main]
async fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let with_login = args.iter().any(|a| a == "--login");
    let with_update = args.iter().any(|a| a == "--update");
    args.retain(|a| a != "--login" && a != "--update");
    let video = args.first().map(PathBuf::from);
    let (jf, al) = (jellyfin(video).await, anilist().await);
    println!("JELLYFIN={}", jf.uri());
    println!("ANILIST={}", al.uri());
    // Only with --update, so the app otherwise shows itself as up to date.
    let _github = if with_update {
        let github = github().await;
        println!("UPDATE={}", github.uri());
        Some(github)
    } else {
        None
    };
    if with_login {
        login(&jf.uri(), &al.uri()).await;
    }
    std::future::pending::<()>().await;
}
