//! Client für die Jellyfin-REST-API (geprüft gegen OpenAPI 12.4.0).

mod dto;

use std::collections::BTreeMap;

use musubi_domain::{
    Episode, ItemKind, Library, LibraryFilters, LibraryItem, LibraryQuery, LibrarySort, Season,
    ShelfItem,
};
use reqwest::{Method, RequestBuilder, Response, StatusCode, header};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use thiserror::Error;

use dto::*;

/// 100-Nanosekunden-Ticks pro Sekunde.
pub const TICKS_PER_SECOND: u64 = 10_000_000;

pub fn secs_to_ticks(secs: f64) -> u64 {
    (secs.max(0.0) * TICKS_PER_SECOND as f64) as u64
}

pub(crate) fn ticks_to_secs(ticks: u64) -> u64 {
    ticks / TICKS_PER_SECOND
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Jellyfin rejected the credentials")]
    Unauthorized,
    #[error("Jellyfin returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("could not reach Jellyfin: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("invalid server URL: {0}")]
    InvalidUrl(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// So meldet sich die App beim Server an.
#[derive(Debug, Clone)]
pub struct Identity {
    pub client: String,
    pub device: String,
    pub device_id: String,
    pub version: String,
}

#[derive(Debug)]
pub struct AuthResult {
    pub token: SecretString,
    pub user_id: String,
    pub user_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMethod {
    DirectPlay,
    DirectStream,
    Transcode,
}

impl PlayMethod {
    fn as_api(self) -> &'static str {
        match self {
            Self::DirectPlay => "DirectPlay",
            Self::DirectStream => "DirectStream",
            Self::Transcode => "Transcode",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExternalSubtitle {
    pub url: SecretString,
    pub title: Option<String>,
    pub language: Option<String>,
}

/// Alles, was zum Abspielen und Melden einer Episode nötig ist.
#[derive(Debug, Clone)]
pub struct PlaybackTarget {
    pub item_id: String,
    pub media_source_id: String,
    pub play_session_id: Option<String>,
    pub play_method: PlayMethod,
    /// Die URL kann bei Transcoding einen API-Key enthalten und wird nie geloggt.
    pub url: SecretString,
    pub subtitles: Vec<ExternalSubtitle>,
}

#[derive(Debug, Clone, Copy)]
pub struct ProgressReport<'a> {
    pub target: &'a PlaybackTarget,
    pub position_secs: f64,
    pub paused: bool,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    identity: Identity,
    token: Option<SecretString>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

fn sanitize(value: &str) -> String {
    value.replace(['"', '\r', '\n'], "")
}

/// Ergänzt ein fehlendes Schema. Im lokalen Netz ist das `http`, weil Jellyfin dort
/// standardmäßig unverschlüsselt läuft. Alles andere bekommt `https`, damit das Passwort
/// nicht versehentlich im Klartext durchs Internet geht.
fn with_scheme(address: &str) -> String {
    let address = address.trim();
    if address.contains("://") {
        return address.to_owned();
    }
    let scheme = match url::Url::parse(&format!("http://{address}")) {
        Ok(url) if is_local_host(&url) => "http",
        _ => "https",
    };
    format!("{scheme}://{address}")
}

fn is_local_host(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        Some(url::Host::Ipv6(ip)) => {
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
        }
        Some(url::Host::Domain(name)) => {
            !name.contains('.')
                || [".local", ".lan", ".home.arpa"]
                    .iter()
                    .any(|suffix| name.ends_with(suffix))
        }
        None => false,
    }
}

impl Client {
    pub fn new(base_url: &str, identity: Identity) -> Result<Self> {
        let parsed = url::Url::parse(&with_scheme(base_url))
            .map_err(|e| Error::InvalidUrl(format!("{base_url}: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(Error::InvalidUrl(format!(
                "{base_url}: expected http or https"
            )));
        }
        let http = reqwest::Client::builder()
            .user_agent(format!("Musubi/{}", identity.version))
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            base: parsed.as_str().trim_end_matches('/').to_owned(),
            identity,
            token: None,
        })
    }

    pub fn with_token(mut self, token: SecretString) -> Self {
        self.token = Some(token);
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// Anmelde-Header für Streams im Format `Name: Wert`, den der Player jeder Anfrage mitgibt.
    pub fn stream_auth_header(&self) -> Option<SecretString> {
        self.token.as_ref().map(|t| {
            SecretString::from(format!(
                "Authorization: MediaBrowser Token=\"{}\"",
                sanitize(t.expose_secret())
            ))
        })
    }

    fn auth_value(&self) -> String {
        let id = &self.identity;
        let mut value = format!(
            "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\"",
            sanitize(&id.client),
            sanitize(&id.device),
            sanitize(&id.device_id),
            sanitize(&id.version),
        );
        if let Some(token) = &self.token {
            value.push_str(&format!(", Token=\"{}\"", sanitize(token.expose_secret())));
        }
        value
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, self.url(path))
            .header(header::AUTHORIZATION, self.auth_value())
    }

    async fn send(request: RequestBuilder) -> Result<Response> {
        let response = request.send().await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(Error::Unauthorized);
        }
        let message = response.text().await.unwrap_or_default();
        Err(Error::Http {
            status: status.as_u16(),
            message: truncate(&message),
        })
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let response = Self::send(self.request(Method::GET, path).query(query)).await?;
        Ok(response.json().await?)
    }

    async fn post_empty<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        Self::send(self.request(Method::POST, path).json(body)).await?;
        Ok(())
    }

    // Anmeldung --------------------------------------------------------------------------

    pub async fn check_server(&self) -> Result<String> {
        let info: PublicInfoDto = self.get_json("System/Info/Public", &[]).await?;
        Ok(info.server_name.unwrap_or_else(|| "Jellyfin".to_owned()))
    }

    pub async fn authenticate(&self, user: &str, password: &SecretString) -> Result<AuthResult> {
        let body = json!({ "Username": user, "Pw": password.expose_secret() });
        let response = Self::send(
            self.request(Method::POST, "Users/AuthenticateByName")
                .json(&body),
        )
        .await?;
        let auth: AuthDto = response.json().await?;
        Ok(AuthResult {
            token: SecretString::from(auth.access_token),
            user_id: auth.user.id,
            user_name: auth.user.name,
        })
    }

    /// Sucht zum API-Key (der keinem Nutzer gehört) die Nutzer-ID über den Namen.
    pub async fn find_user(&self, name: &str) -> Result<(String, String)> {
        let users: Vec<UserDto> = self.get_json("Users", &[]).await?;
        users
            .into_iter()
            .find(|u| u.name.eq_ignore_ascii_case(name))
            .map(|u| (u.id, u.name))
            .ok_or_else(|| Error::Other(format!("no Jellyfin user named `{name}`")))
    }

    pub async fn logout(&self) -> Result<()> {
        Self::send(self.request(Method::POST, "Sessions/Logout")).await?;
        Ok(())
    }

    // Bibliothek -------------------------------------------------------------------------

    pub async fn libraries(&self, user_id: &str) -> Result<Vec<Library>> {
        let views: PagedDto<ItemDto> = self
            .get_json("UserViews", &[("userId", user_id.to_owned())])
            .await?;
        Ok(views
            .items
            .into_iter()
            .filter(|v| {
                matches!(
                    v.collection_type.as_deref(),
                    Some("tvshows" | "movies") | None
                )
            })
            .map(|v| Library {
                id: v.id,
                name: v.name,
                collection_type: v.collection_type,
            })
            .collect())
    }

    fn items_params(user_id: &str, query: &LibraryQuery) -> Vec<(&'static str, String)> {
        let types = match query.kind {
            Some(ItemKind::Series) => "Series",
            Some(ItemKind::Movie) => "Movie",
            None => "Series,Movie",
        };
        let (sort_by, order) = match query.sort {
            LibrarySort::Name => ("SortName", "Ascending"),
            LibrarySort::Year => ("ProductionYear,SortName", "Descending"),
            LibrarySort::DateAdded => ("DateCreated,SortName", "Descending"),
            LibrarySort::Rating => ("CommunityRating,SortName", "Descending"),
        };
        let mut params = vec![
            ("userId", user_id.to_owned()),
            ("recursive", "true".to_owned()),
            ("includeItemTypes", types.to_owned()),
            ("fields", "Overview,Genres,ProviderIds".to_owned()),
            ("enableUserData", "true".to_owned()),
            ("sortBy", sort_by.to_owned()),
            ("sortOrder", order.to_owned()),
            ("startIndex", query.start.to_string()),
        ];
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(id) = &query.library_id {
            params.push(("parentId", id.clone()));
        }
        if let Some(term) = query.search.as_deref().filter(|t| !t.is_empty()) {
            params.push(("searchTerm", term.to_owned()));
        }
        if !query.genres.is_empty() {
            // Genres können Kommas enthalten, Jellyfin erwartet hier Pipes.
            params.push(("genres", query.genres.join("|")));
        }
        if !query.years.is_empty() {
            let years: Vec<String> = query.years.iter().map(i32::to_string).collect();
            params.push(("years", years.join(",")));
        }
        if let Some(status) = &query.status {
            params.push(("seriesStatus", status.clone()));
        }
        if query.unplayed_only {
            params.push(("filters", "IsUnplayed".to_owned()));
        }
        params
    }

    /// Eine Seite der Bibliothek samt Gesamtzahl.
    pub async fn items(
        &self,
        user_id: &str,
        query: &LibraryQuery,
    ) -> Result<(Vec<LibraryItem>, u32)> {
        let page: PagedDto<ItemDto> = self
            .get_json("Items", &Self::items_params(user_id, query))
            .await?;
        let total = page.total_record_count.unwrap_or(page.items.len() as u32);
        Ok((
            page.items
                .into_iter()
                .filter_map(ItemDto::into_library_item)
                .collect(),
            total,
        ))
    }

    /// Alle Seiten einer Abfrage.
    pub async fn all_items(&self, user_id: &str, query: &LibraryQuery) -> Result<Vec<LibraryItem>> {
        const PAGE: u32 = 200;
        let mut out = Vec::new();
        let mut current = query.clone();
        current.limit = Some(PAGE);
        loop {
            let (items, total) = self.items(user_id, &current).await?;
            let received = items.len() as u32;
            out.extend(items);
            current.start += received;
            if received == 0 || current.start >= total {
                return Ok(out);
            }
        }
    }

    pub async fn filters(&self, user_id: &str, library_id: Option<&str>) -> Result<LibraryFilters> {
        let mut params = vec![
            ("userId", user_id.to_owned()),
            ("includeItemTypes", "Series,Movie".to_owned()),
        ];
        if let Some(id) = library_id {
            params.push(("parentId", id.to_owned()));
        }
        let dto: FiltersDto = self.get_json("Items/Filters", &params).await?;
        Ok(LibraryFilters {
            genres: dto.genres.unwrap_or_default(),
            years: dto.years.unwrap_or_default(),
        })
    }

    pub async fn seasons(&self, user_id: &str, series_id: &str) -> Result<Vec<Season>> {
        let page: PagedDto<ItemDto> = self
            .get_json(
                &format!("Shows/{series_id}/Seasons"),
                &[
                    ("userId", user_id.to_owned()),
                    ("enableUserData", "true".to_owned()),
                    ("fields", "ProviderIds,ProductionYear".to_owned()),
                ],
            )
            .await?;
        Ok(page
            .items
            .into_iter()
            .map(|s| Season {
                number: s.index_number.unwrap_or(0),
                id: s.id,
                name: s.name,
                provider_ids: provider_map(s.provider_ids),
                year: s.production_year,
            })
            .collect())
    }

    pub async fn episodes(&self, user_id: &str, series_id: &str) -> Result<Vec<Episode>> {
        let page: PagedDto<ItemDto> = self
            .get_json(
                &format!("Shows/{series_id}/Episodes"),
                &[
                    ("userId", user_id.to_owned()),
                    ("fields", "Overview".to_owned()),
                    ("enableUserData", "true".to_owned()),
                    ("isMissing", "false".to_owned()),
                ],
            )
            .await?;
        let mut episodes: Vec<Episode> = page
            .items
            .into_iter()
            .map(|e| e.into_episode(series_id))
            .collect();
        episodes.sort_by_key(|e| (e.season_number == 0, e.season_number, e.number));
        Ok(episodes)
    }

    pub async fn set_played(&self, user_id: &str, item_id: &str, played: bool) -> Result<()> {
        let method = if played { Method::POST } else { Method::DELETE };
        Self::send(
            self.request(method, &format!("UserPlayedItems/{item_id}"))
                .query(&[("userId", user_id)]),
        )
        .await?;
        Ok(())
    }

    fn shelf_params(user_id: &str, limit: u32) -> Vec<(&'static str, String)> {
        vec![
            ("userId", user_id.to_owned()),
            ("limit", limit.to_string()),
            ("fields", "Overview".to_owned()),
            ("enableUserData", "true".to_owned()),
        ]
    }

    fn shelf(items: Vec<ItemDto>) -> Vec<ShelfItem> {
        items
            .into_iter()
            .filter_map(ItemDto::into_shelf_item)
            .collect()
    }

    /// Angefangene Episoden und Filme.
    pub async fn resume(&self, user_id: &str, limit: u32) -> Result<Vec<ShelfItem>> {
        let mut params = Self::shelf_params(user_id, limit);
        params.push(("mediaTypes", "Video".to_owned()));
        let page: PagedDto<ItemDto> = self.get_json("UserItems/Resume", &params).await?;
        Ok(Self::shelf(page.items))
    }

    /// Die jeweils nächste ungesehene Episode laufender Serien.
    pub async fn next_up(&self, user_id: &str, limit: u32) -> Result<Vec<ShelfItem>> {
        let params = Self::shelf_params(user_id, limit);
        let page: PagedDto<ItemDto> = self.get_json("Shows/NextUp", &params).await?;
        Ok(Self::shelf(page.items))
    }

    /// Zuletzt hinzugefügte Titel. Mehrere neue Episoden fasst der Server zur Serie zusammen.
    pub async fn latest(&self, user_id: &str, limit: u32) -> Result<Vec<ShelfItem>> {
        let mut params = Self::shelf_params(user_id, limit);
        params.push(("includeItemTypes", "Episode,Movie".to_owned()));
        params.push(("groupItems", "true".to_owned()));
        let items: Vec<ItemDto> = self.get_json("Items/Latest", &params).await?;
        Ok(Self::shelf(items))
    }

    /// Ein einzelnes Item, `None` wenn es keine Serie und kein Film ist.
    pub async fn item(&self, user_id: &str, item_id: &str) -> Result<Option<LibraryItem>> {
        let dto: ItemDto = self
            .get_json(
                &format!("Items/{item_id}"),
                &[("userId", user_id.to_owned())],
            )
            .await?;
        Ok(dto.into_library_item())
    }

    async fn image(&self, item_id: &str, kind: &str, max_width: u32) -> Result<Vec<u8>> {
        let response = Self::send(
            self.request(Method::GET, &format!("Items/{item_id}/Images/{kind}"))
                .query(&[
                    ("maxWidth", max_width.to_string()),
                    ("quality", "90".to_owned()),
                ]),
        )
        .await?;
        Ok(response.bytes().await?.to_vec())
    }

    pub async fn poster(&self, item_id: &str, max_width: u32) -> Result<Vec<u8>> {
        self.image(item_id, "Primary", max_width).await
    }

    pub async fn backdrop(&self, item_id: &str, max_width: u32) -> Result<Vec<u8>> {
        self.image(item_id, "Backdrop", max_width).await
    }

    // Wiedergabe -------------------------------------------------------------------------

    /// Fragt den Server, wie das Item abgespielt werden soll. GStreamer mit `gst-libav` spielt
    /// praktisch jedes Format, das Geräteprofil erlaubt deshalb alles und Transcoding bleibt
    /// reiner Fallback.
    pub async fn playback_target(
        &self,
        user_id: &str,
        item_id: &str,
        max_bitrate: Option<u64>,
    ) -> Result<PlaybackTarget> {
        let mut profile = json!({
            "Name": "Musubi",
            "DirectPlayProfiles": [{ "Type": "Video" }, { "Type": "Audio" }],
            "TranscodingProfiles": [{
                "Type": "Video",
                "Container": "ts",
                "Protocol": "hls",
                "Context": "Streaming",
                "VideoCodec": "h264,hevc",
                "AudioCodec": "aac,opus,flac,mp3",
                "MaxAudioChannels": "6",
                "BreakOnNonKeyFrames": true
            }],
            "SubtitleProfiles": [
                { "Format": "ass", "Method": "External" },
                { "Format": "ssa", "Method": "External" },
                { "Format": "srt", "Method": "External" },
                { "Format": "vtt", "Method": "External" },
                { "Format": "pgssub", "Method": "Embed" },
                { "Format": "dvdsub", "Method": "Embed" },
                { "Format": "ass", "Method": "Embed" },
                { "Format": "srt", "Method": "Embed" }
            ]
        });
        let mut body = json!({
            "UserId": user_id,
            "EnableDirectPlay": true,
            "EnableDirectStream": true,
            "EnableTranscoding": true,
            "AutoOpenLiveStream": false,
        });
        if let Some(bitrate) = max_bitrate {
            body["MaxStreamingBitrate"] = json!(bitrate);
            profile["MaxStreamingBitrate"] = json!(bitrate);
        }
        body["DeviceProfile"] = profile;

        let response = Self::send(
            self.request(Method::POST, &format!("Items/{item_id}/PlaybackInfo"))
                .query(&[("userId", user_id)])
                .json(&body),
        )
        .await?;
        let info: PlaybackInfoDto = response.json().await?;
        if let Some(code) = info.error_code {
            return Err(Error::Other(format!("Jellyfin refused playback: {code}")));
        }
        let source = info
            .media_sources
            .into_iter()
            .next()
            .ok_or_else(|| Error::Other("item has no playable media source".to_owned()))?;
        self.target_from_source(item_id, info.play_session_id, source)
    }

    fn target_from_source(
        &self,
        item_id: &str,
        play_session_id: Option<String>,
        source: MediaSourceDto,
    ) -> Result<PlaybackTarget> {
        let (url, play_method) = if source.supports_direct_play || source.supports_direct_stream {
            let mut url = url::Url::parse(&self.url(&format!("Videos/{item_id}/stream")))
                .map_err(|e| Error::InvalidUrl(e.to_string()))?;
            {
                let mut query = url.query_pairs_mut();
                query
                    .append_pair("static", "true")
                    .append_pair("mediaSourceId", &source.id);
                if let Some(tag) = &source.e_tag {
                    query.append_pair("Tag", tag);
                }
                if let Some(session) = &play_session_id {
                    query.append_pair("PlaySessionId", session);
                }
            }
            (url.to_string(), PlayMethod::DirectStream)
        } else if let Some(path) = &source.transcoding_url {
            (self.url(path), PlayMethod::Transcode)
        } else {
            return Err(Error::Other(
                "server offers no way to stream this item".to_owned(),
            ));
        };

        let subtitles = source
            .media_streams
            .iter()
            .filter(|s| s.stream_type.as_deref() == Some("Subtitle"))
            .filter(|s| s.is_external || s.delivery_method.as_deref() == Some("External"))
            .filter_map(|s| {
                let url = match &s.delivery_url {
                    Some(path) if path.starts_with("http") => path.clone(),
                    Some(path) => self.url(path),
                    None => {
                        let format = match s.codec.as_deref()? {
                            "subrip" => "srt",
                            other => other,
                        };
                        self.url(&format!(
                            "Videos/{item_id}/{}/Subtitles/{}/Stream.{format}",
                            source.id, s.index
                        ))
                    }
                };
                Some(ExternalSubtitle {
                    url: SecretString::from(url),
                    title: s.display_title.clone().or_else(|| s.title.clone()),
                    language: s.language.clone(),
                })
            })
            .collect();

        Ok(PlaybackTarget {
            item_id: item_id.to_owned(),
            media_source_id: source.id,
            play_session_id,
            play_method,
            url: SecretString::from(url),
            subtitles,
        })
    }

    fn report_body(report: ProgressReport<'_>) -> serde_json::Value {
        json!({
            "ItemId": report.target.item_id,
            "MediaSourceId": report.target.media_source_id,
            "PlaySessionId": report.target.play_session_id,
            "PlayMethod": report.target.play_method.as_api(),
            "PositionTicks": secs_to_ticks(report.position_secs),
            "IsPaused": report.paused,
            "IsMuted": false,
            "CanSeek": true,
        })
    }

    pub async fn report_started(&self, report: ProgressReport<'_>) -> Result<()> {
        self.post_empty("Sessions/Playing", &Self::report_body(report))
            .await
    }

    pub async fn report_progress(&self, report: ProgressReport<'_>) -> Result<()> {
        self.post_empty("Sessions/Playing/Progress", &Self::report_body(report))
            .await
    }

    pub async fn report_stopped(&self, report: ProgressReport<'_>) -> Result<()> {
        self.post_empty("Sessions/Playing/Stopped", &Self::report_body(report))
            .await
    }
}

fn truncate(text: &str) -> String {
    text.chars().take(300).collect()
}

pub(crate) fn provider_map(
    ids: Option<BTreeMap<String, Option<String>>>,
) -> BTreeMap<String, String> {
    ids.unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::with_scheme;

    #[test]
    fn local_addresses_get_http() {
        let cases = [
            ("192.168.0.130", "http://192.168.0.130"),
            ("192.168.0.130:8096", "http://192.168.0.130:8096"),
            (" 10.0.0.5:8096 ", "http://10.0.0.5:8096"),
            ("localhost:8096", "http://localhost:8096"),
            ("nas:8096", "http://nas:8096"),
            ("jellyfin.local", "http://jellyfin.local"),
            ("[::1]:8096", "http://[::1]:8096"),
        ];
        for (input, expected) in cases {
            assert_eq!(with_scheme(input), expected);
        }
    }

    #[test]
    fn public_addresses_get_https() {
        assert_eq!(
            with_scheme("jellyfin.example.org"),
            "https://jellyfin.example.org"
        );
        assert_eq!(with_scheme("8.8.8.8:8096"), "https://8.8.8.8:8096");
    }

    #[test]
    fn explicit_schemes_are_kept() {
        assert_eq!(with_scheme("http://example.org"), "http://example.org");
        assert_eq!(with_scheme("https://10.0.0.5"), "https://10.0.0.5");
    }
}
