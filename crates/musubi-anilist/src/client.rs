use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use musubi_domain::{EntryPatch, ListEntry, Media, MediaKind, Viewer};
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use crate::dto::*;
use crate::queries::*;
use crate::{Error, RateLimiter, Result};

const ENDPOINT: &str = "https://graphql.anilist.co";
const MAX_ATTEMPTS: u32 = 3;

/// Ein mit einem anderen Werk verknüpftes Werk.
#[derive(Debug, Clone, PartialEq)]
pub struct Relation {
    /// z. B. `SEQUEL` oder `PREQUEL`.
    pub relation: String,
    pub media: Media,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    endpoint: String,
    token: Option<SecretString>,
    limiter: Arc<RateLimiter>,
}

impl Client {
    pub fn new(token: Option<SecretString>) -> Result<Self> {
        Self::with_endpoint(ENDPOINT, token)
    }

    pub fn with_endpoint(endpoint: &str, token: Option<SecretString>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Musubi/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            endpoint: endpoint.to_owned(),
            token,
            limiter: Arc::new(RateLimiter::new()),
        })
    }

    pub fn limiter(&self) -> Arc<RateLimiter> {
        Arc::clone(&self.limiter)
    }

    fn observe_headers(&self, headers: &HeaderMap) {
        if let Some(limit) = header_number(headers, "x-ratelimit-limit") {
            self.limiter.observe_limit(limit as u32);
        }
    }

    async fn graphql<T: DeserializeOwned>(&self, query: String, variables: Value) -> Result<T> {
        let body = json!({ "query": query, "variables": variables });
        let mut attempt = 0;
        loop {
            attempt += 1;
            self.limiter.acquire().await;
            let mut request = self
                .http
                .post(&self.endpoint)
                .header(ACCEPT, "application/json");
            if let Some(token) = &self.token {
                request =
                    request.header(AUTHORIZATION, format!("Bearer {}", token.expose_secret()));
            }
            let response = request.json(&body).send().await?;
            let status = response.status();
            self.observe_headers(response.headers());

            if status == StatusCode::TOO_MANY_REQUESTS {
                let wait = Duration::from_secs(
                    header_number(response.headers(), "retry-after").unwrap_or(60),
                );
                self.limiter.block_for(wait);
                if attempt >= MAX_ATTEMPTS {
                    return Err(Error::RateLimited { retry_after: wait });
                }
                continue;
            }
            self.limiter.clear_notice();

            let text = response.text().await?;
            let envelope: Envelope<Value> = match serde_json::from_str(&text) {
                Ok(envelope) => envelope,
                Err(_) if !status.is_success() => {
                    return Err(Error::Api {
                        status: status.as_u16(),
                        message: text.chars().take(300).collect(),
                    });
                }
                Err(e) => return Err(e.into()),
            };
            if let Some(error) = envelope.errors.as_ref().and_then(|e| e.first()) {
                return Err(classify(error, status));
            }
            let data = envelope.data.ok_or_else(|| Error::Api {
                status: status.as_u16(),
                message: "response contains no data".to_owned(),
            })?;
            return Ok(serde_json::from_value(data)?);
        }
    }

    pub async fn viewer(&self) -> Result<Viewer> {
        let data: ViewerData = self.graphql(VIEWER.to_owned(), json!({})).await?;
        Ok(data.viewer.into())
    }

    /// Die vollständige Liste. Einträge in Custom Lists kommen mehrfach und werden entdoppelt.
    pub async fn list(&self, user_id: i64, kind: MediaKind) -> Result<Vec<ListEntry>> {
        let query = format!("{LIST_COLLECTION}{ENTRY_FIELDS}{MEDIA_FIELDS}");
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        let mut chunk = 1;
        loop {
            let data: CollectionData = self
                .graphql(
                    query.clone(),
                    json!({ "userId": user_id, "type": kind.as_api(), "chunk": chunk }),
                )
                .await?;
            for group in data.collection.lists.unwrap_or_default() {
                for entry in group.entries.unwrap_or_default() {
                    if seen.insert(entry.id) {
                        entries.push(ListEntry::from(entry));
                    }
                }
            }
            if !data.collection.has_next_chunk.unwrap_or(false) {
                return Ok(entries);
            }
            chunk += 1;
        }
    }

    pub async fn search(
        &self,
        text: &str,
        kind: MediaKind,
        formats: Option<&[&str]>,
        per_page: u32,
    ) -> Result<Vec<Media>> {
        let mut variables = json!({ "search": text, "type": kind.as_api(), "perPage": per_page });
        if let Some(formats) = formats {
            variables["formats"] = json!(formats);
        }
        let data: PageData = self
            .graphql(format!("{SEARCH}{MEDIA_FIELDS}"), variables)
            .await?;
        Ok(data.page.media.into_iter().map(Into::into).collect())
    }

    pub async fn media(&self, id: i64) -> Result<Media> {
        let data: MediaData = self
            .graphql(format!("{MEDIA_BY_ID}{MEDIA_FIELDS}"), json!({ "id": id }))
            .await?;
        Ok(data.media.into())
    }

    pub async fn media_by_mal(&self, id_mal: i64, kind: MediaKind) -> Result<Media> {
        let data: MediaData = self
            .graphql(
                format!("{MEDIA_BY_MAL}{MEDIA_FIELDS}"),
                json!({ "idMal": id_mal, "type": kind.as_api() }),
            )
            .await?;
        Ok(data.media.into())
    }

    pub async fn relations(&self, id: i64) -> Result<(Media, Vec<Relation>)> {
        let data: RelationsData = self
            .graphql(format!("{RELATIONS}{MEDIA_FIELDS}"), json!({ "id": id }))
            .await?;
        let relations = data
            .media
            .relations
            .and_then(|r| r.edges)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|edge| {
                Some(Relation {
                    relation: edge.relation_type?,
                    media: edge.node?.into(),
                })
            })
            .collect();
        Ok((data.media.media.into(), relations))
    }

    pub async fn save_entry(&self, patch: &EntryPatch) -> Result<ListEntry> {
        let data: SaveData = self
            .graphql(
                format!("{SAVE_ENTRY}{ENTRY_FIELDS}{MEDIA_FIELDS}"),
                patch_variables(patch),
            )
            .await?;
        Ok(data.entry.into())
    }

    pub async fn delete_entry(&self, entry_id: i64) -> Result<bool> {
        let result: Result<DeleteData> = self
            .graphql(DELETE_ENTRY.to_owned(), json!({ "id": entry_id }))
            .await;
        match result {
            Ok(data) => Ok(data.result.deleted.unwrap_or(false)),
            // AniList meldet einen bereits entfernten Eintrag als Validierungsfehler der ID.
            Err(Error::Api {
                status: 400,
                message,
            }) if message == "validation" => Err(Error::NotFound),
            Err(error) => Err(error),
        }
    }

    /// Ersetzt die Namen der Custom Lists.
    pub async fn set_custom_lists(&self, kind: MediaKind, names: &[String]) -> Result<()> {
        let query = match kind {
            MediaKind::Anime => UPDATE_ANIME_LISTS,
            MediaKind::Manga => UPDATE_MANGA_LISTS,
        };
        let _: UpdateUserData = self
            .graphql(query.to_owned(), json!({ "lists": names }))
            .await?;
        Ok(())
    }
}

fn header_number(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

fn classify(error: &GraphQlError, http: StatusCode) -> Error {
    let status = error.status.unwrap_or(http.as_u16());
    let lower = error.message.to_ascii_lowercase();
    if status == 401 || lower.contains("invalid token") || lower.contains("unauthorized") {
        Error::Unauthorized
    } else if status == 404 {
        Error::NotFound
    } else if status == 429 {
        Error::RateLimited {
            retry_after: Duration::from_secs(60),
        }
    } else {
        Error::Api {
            status,
            message: error.message.clone(),
        }
    }
}

/// Nur gesetzte Felder werden gesendet, fehlende Variablen ändert AniList nicht.
fn patch_variables(patch: &EntryPatch) -> Value {
    let mut vars = Map::new();
    vars.insert("mediaId".to_owned(), json!(patch.media_id));
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            vars.insert(key.to_owned(), value);
        }
    };
    put("status", patch.status.map(|s| json!(s.as_api())));
    put("scoreRaw", patch.score_raw.map(|v| json!(v)));
    put("progress", patch.progress.map(|v| json!(v)));
    put("progressVolumes", patch.progress_volumes.map(|v| json!(v)));
    put("repeat", patch.repeat.map(|v| json!(v)));
    put("priority", patch.priority.map(|v| json!(v)));
    put("private", patch.private.map(|v| json!(v)));
    put(
        "hiddenFromStatusLists",
        patch.hidden_from_status_lists.map(|v| json!(v)),
    );
    put("notes", patch.notes.as_ref().map(|v| json!(v)));
    put("customLists", patch.custom_lists.as_ref().map(|v| json!(v)));
    put(
        "advancedScores",
        patch.advanced_scores.as_ref().map(|v| json!(v)),
    );
    let date =
        |d: &musubi_domain::FuzzyDate| json!({"year": d.year, "month": d.month, "day": d.day});
    put("startedAt", patch.started_at.as_ref().map(date));
    put("completedAt", patch.completed_at.as_ref().map(date));
    Value::Object(vars)
}
