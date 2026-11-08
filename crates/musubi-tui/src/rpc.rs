//! Zeilenweises JSON zwischen Kern und Frontend.
//!
//! Anfragen: `{"id":1,"method":"home","params":{}}`. Antworten: `{"id":1,"result":…}` oder
//! `{"id":1,"error":"…"}`. Ereignisse des Kerns kommen als `{"event":…}`. Parameter werden
//! nie protokolliert, weil sie Passwörter enthalten können.

use std::collections::BTreeMap;

use musubi_core::domain::{
    EntryPatch, FuzzyDate, LibraryItem, LibraryQuery, ListEntry, ListStatus, Mapping, Media,
    MediaKind, ParseDateError, ScoreFormat, Viewer, airing_entries, in_progress_entries,
};
use musubi_core::{AuthStatus, Core, PlayerCommand, SyncRow, oauth};
use secrecy::SecretString;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc};

#[derive(Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

/// Bedient eine Verbindung, bis das Frontend sie schließt.
pub async fn serve<S>(core: Core, stream: S) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (read, mut write) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            write.write_all(line.as_bytes()).await?;
            write.write_all(b"\n").await?;
            write.flush().await?;
        }
        Ok::<(), std::io::Error>(())
    });
    let mut events = core.subscribe();
    let event_tx = tx.clone();
    let event_task = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if event_tx
                        .send(json!({ "event": event }).to_string())
                        .is_err()
                    {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    });

    let mut lines = BufReader::new(read).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(request) = serde_json::from_str::<Request>(&line) else {
            tracing::warn!("ignoring a malformed request from the terminal interface");
            continue;
        };
        let (core, tx) = (core.clone(), tx.clone());
        tokio::spawn(async move {
            let response = match call(&core, &request.method, request.params).await {
                Ok(result) => json!({ "id": request.id, "result": result }),
                Err(error) => json!({ "id": request.id, "error": error }),
            };
            let _ = tx.send(response.to_string());
        });
    }
    event_task.abort();
    drop(tx);
    writer.await.map_err(std::io::Error::other)?
}

#[derive(Serialize)]
struct Session {
    auth: AuthStatus,
    viewer: Option<Viewer>,
    setup_needed: bool,
    jellyfin_url: Option<String>,
    client_id: Option<String>,
    redirect: String,
    auto_sync: bool,
    score_hint: &'static str,
    keys: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct ListRow {
    #[serde(flatten)]
    entry: ListEntry,
    score_text: String,
    behind: u32,
}

#[derive(Serialize)]
struct Overview {
    airing: Vec<ListRow>,
    anime_in_progress: Vec<ListRow>,
    manga_in_progress: Vec<ListRow>,
}

fn list_row(entry: &ListEntry, format: ScoreFormat) -> ListRow {
    ListRow {
        score_text: if entry.score_raw == 0 {
            String::new()
        } else {
            format.display(entry.score_raw)
        },
        behind: entry.episodes_behind(),
        entry: entry.clone(),
    }
}

/// Felder eines Formulars. Punkte und Daten kommen als Text und werden hier geprüft.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct EntryForm {
    media_id: i64,
    status: Option<ListStatus>,
    score: Option<String>,
    progress: Option<u32>,
    progress_volumes: Option<u32>,
    repeat: Option<u32>,
    priority: Option<u32>,
    private: Option<bool>,
    hidden_from_status_lists: Option<bool>,
    notes: Option<String>,
    custom_lists: Option<Vec<String>>,
    advanced_scores: Option<Vec<f64>>,
    started_at: Option<String>,
    completed_at: Option<String>,
}

fn date(value: Option<String>) -> Result<Option<FuzzyDate>, String> {
    value
        .map(|v| v.parse().map_err(|e: ParseDateError| e.to_string()))
        .transpose()
}

impl EntryForm {
    fn into_patch(self, format: ScoreFormat) -> Result<EntryPatch, String> {
        Ok(EntryPatch {
            media_id: self.media_id,
            status: self.status,
            score_raw: self
                .score
                .map(|s| format.parse(&s).map_err(|e| e.to_string()))
                .transpose()?,
            progress: self.progress,
            progress_volumes: self.progress_volumes,
            repeat: self.repeat,
            priority: self.priority,
            private: self.private,
            hidden_from_status_lists: self.hidden_from_status_lists,
            notes: self.notes,
            custom_lists: self.custom_lists,
            advanced_scores: self.advanced_scores,
            started_at: date(self.started_at)?,
            completed_at: date(self.completed_at)?,
        })
    }
}

fn args<T: DeserializeOwned>(params: Value) -> Result<T, String> {
    serde_json::from_value(params).map_err(|e| format!("invalid parameters: {e}"))
}

fn reply<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn score_format(core: &Core) -> ScoreFormat {
    core.viewer().map(|v| v.score_format).unwrap_or_default()
}

fn session(core: &Core) -> Session {
    let config = core.config();
    Session {
        auth: core.auth_status(),
        viewer: core.viewer(),
        setup_needed: core.setup_needed(),
        redirect: oauth::loopback_redirect(config.anilist.loopback_port),
        jellyfin_url: config.jellyfin.url,
        client_id: config.anilist.client_id,
        auto_sync: config.sync.auto_sync,
        score_hint: score_format(core).hint(),
        keys: config.tui.keys,
    }
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
struct Kind {
    kind: MediaKind,
}

#[derive(Deserialize)]
struct ListName {
    kind: MediaKind,
    name: String,
}

async fn call(core: &Core, method: &str, params: Value) -> Result<Value, String> {
    let error = |e: musubi_core::CoreError| e.to_string();
    match method {
        "session" => reply(session(core)),
        "finish_setup" => {
            #[derive(Deserialize)]
            struct P {
                auto_sync: Option<bool>,
            }
            let P { auto_sync } = args(params)?;
            core.update_config(|c| {
                c.ui.setup_completed = true;
                if let Some(enabled) = auto_sync {
                    c.sync.auto_sync = enabled;
                }
            })
            .map_err(error)?;
            reply(())
        }
        "login_jellyfin" => {
            #[derive(Deserialize)]
            struct P {
                url: String,
                user: String,
                #[serde(default)]
                password: String,
                #[serde(default)]
                api_key: String,
            }
            let P {
                url,
                user,
                password,
                api_key,
            } = args(params)?;
            let name = if api_key.is_empty() {
                core.login_jellyfin(&url, &user, &SecretString::from(password))
                    .await
            } else {
                core.login_jellyfin_api_key(&url, SecretString::from(api_key), &user)
                    .await
            }
            .map_err(error)?;
            reply(name)
        }
        "login_anilist_browser" => {
            #[derive(Deserialize)]
            struct P {
                client_id: String,
            }
            let P { client_id } = args(params)?;
            core.update_config(|c| c.anilist.client_id = Some(client_id))
                .map_err(error)?;
            reply(core.login_anilist_loopback().await.map_err(error)?.name)
        }
        "login_anilist_pin" => {
            #[derive(Deserialize)]
            struct P {
                input: String,
            }
            let P { input } = args(params)?;
            reply(core.login_anilist_pin(&input).await.map_err(error)?.name)
        }
        "logout" => {
            #[derive(Deserialize)]
            struct P {
                service: String,
            }
            let P { service } = args(params)?;
            match service.as_str() {
                "jellyfin" => core.logout_jellyfin().await.map_err(error)?,
                "anilist" => core.logout_anilist().await.map_err(error)?,
                other => return Err(format!("unknown service {other}")),
            }
            reply(())
        }
        "home" => reply(core.home().await.map_err(error)?),
        "library_item" => {
            let Id { id } = args(params)?;
            reply(core.library_item(&id).await.map_err(error)?)
        }
        "library" => {
            let query: LibraryQuery = args(params)?;
            reply(core.library(&query).await.map_err(error)?)
        }
        "cached_library" => reply(core.cached_library().await.map_err(error)?),
        "library_choices" => {
            let filters = core.filters(None).await.map_err(error)?;
            let libraries = core.libraries().await.map_err(error)?;
            reply(json!({ "filters": filters, "libraries": libraries }))
        }
        "episodes" => {
            let Id { id } = args(params)?;
            reply(core.episodes(&id).await.map_err(error)?)
        }
        "set_played" => {
            #[derive(Deserialize)]
            struct P {
                id: String,
                played: bool,
            }
            let P { id, played } = args(params)?;
            core.set_played(&id, played).await.map_err(error)?;
            reply(())
        }
        "play" => {
            #[derive(Deserialize)]
            struct P {
                library_id: String,
                episode_id: Option<String>,
            }
            let P {
                library_id,
                episode_id,
            } = args(params)?;
            let item = core.library_item(&library_id).await.map_err(error)?;
            core.play(&item, episode_id.as_deref())
                .await
                .map_err(error)?;
            reply(())
        }
        "player" => {
            let command: PlayerCommand = args(params)?;
            core.player_command(command).await.map_err(error)?;
            reply(())
        }
        "list" => {
            let Kind { kind } = args(params)?;
            let format = score_format(core);
            let rows: Vec<ListRow> = core
                .list(kind)
                .iter()
                .map(|e| list_row(e, format))
                .collect();
            reply(rows)
        }
        "overview" => {
            let format = score_format(core);
            let (anime, manga) = (core.list(MediaKind::Anime), core.list(MediaKind::Manga));
            let rows = |entries: Vec<&ListEntry>| -> Vec<ListRow> {
                entries.into_iter().map(|e| list_row(e, format)).collect()
            };
            reply(Overview {
                airing: rows(airing_entries(&anime)),
                anime_in_progress: rows(in_progress_entries(&anime)),
                manga_in_progress: rows(in_progress_entries(&manga)),
            })
        }
        "refresh_lists" => {
            core.refresh_viewer().await.map_err(error)?;
            core.refresh_list(MediaKind::Anime).await.map_err(error)?;
            core.refresh_list(MediaKind::Manga).await.map_err(error)?;
            reply(session(core))
        }
        "save_entry" => {
            let form: EntryForm = args(params)?;
            let patch = form.into_patch(score_format(core))?;
            core.save_entry(patch).await.map_err(error)?;
            reply(())
        }
        "add_to_list" => {
            #[derive(Deserialize)]
            struct P {
                media: Box<Media>,
                status: ListStatus,
            }
            let P { media, status } = args(params)?;
            core.add_to_list(*media, status).await.map_err(error)?;
            reply(())
        }
        "delete_entry" => {
            #[derive(Deserialize)]
            struct P {
                media_id: i64,
                kind: MediaKind,
            }
            let P { media_id, kind } = args(params)?;
            let entry = core
                .list(kind)
                .into_iter()
                .find(|e| e.media.id == media_id)
                .ok_or_else(|| "this title is no longer on your list".to_owned())?;
            core.delete_entry(&entry).await.map_err(error)?;
            reply(())
        }
        "search" => {
            #[derive(Deserialize)]
            struct P {
                text: String,
                kind: MediaKind,
            }
            let P { text, kind } = args(params)?;
            reply(core.search_anilist(&text, kind).await.map_err(error)?)
        }
        "create_list" => {
            let ListName { kind, name } = args(params)?;
            core.create_custom_list(kind, &name).await.map_err(error)?;
            reply(session(core))
        }
        "delete_list" => {
            let ListName { kind, name } = args(params)?;
            core.delete_custom_list(kind, &name).await.map_err(error)?;
            reply(session(core))
        }
        "rename_list" => {
            #[derive(Deserialize)]
            struct P {
                kind: MediaKind,
                old: String,
                new: String,
            }
            let P { kind, old, new } = args(params)?;
            core.rename_custom_list(kind, &old, &new)
                .await
                .map_err(error)?;
            reply(session(core))
        }
        "match_all" => {
            core.match_all().await.map_err(error)?;
            reply(())
        }
        "map_manually" => {
            #[derive(Deserialize)]
            struct P {
                item_id: String,
                media_id: i64,
                follow: bool,
            }
            let P {
                item_id,
                media_id,
                follow,
            } = args(params)?;
            let item: LibraryItem = core.library_item(&item_id).await.map_err(error)?;
            core.map_manually(&item, media_id, follow)
                .await
                .map_err(error)?;
            reply(())
        }
        "mapping_data" => {
            let reviews = core.reviews().await.map_err(error)?;
            let mappings = core.all_mappings().await.map_err(error)?;
            reply(json!({ "reviews": reviews, "mappings": mappings }))
        }
        "set_mappings" => {
            #[derive(Deserialize)]
            struct P {
                id: String,
                mappings: Vec<Mapping>,
            }
            let P { id, mappings } = args(params)?;
            core.set_mappings(&id, mappings).await.map_err(error)?;
            reply(())
        }
        "unmap" => {
            let Id { id } = args(params)?;
            core.unmap(&id).await.map_err(error)?;
            reply(())
        }
        "sync_diff" => reply(core.sync_diff().await.map_err(error)?),
        "apply_sync" => {
            #[derive(Deserialize)]
            struct P {
                rows: Vec<SyncRow>,
            }
            let P { rows } = args(params)?;
            reply(core.apply_sync(&rows).await.map_err(error)?)
        }
        "sync_status" => {
            let pending = core.pending_writes().await.map_err(error)?;
            let log = core.sync_log(50).await.map_err(error)?;
            reply(json!({ "pending": pending, "log": log }))
        }
        "flush_outbox" => {
            core.flush_outbox().await.map_err(error)?;
            reply(())
        }
        other => Err(format!("unknown method {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use musubi_core::{CoreOptions, Paths};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[test]
    fn form_parses_score_and_dates_and_keeps_unset_fields_empty() {
        let form = EntryForm {
            media_id: 7,
            score: Some("85".into()),
            started_at: Some("2024-03-01".into()),
            ..EntryForm::default()
        };
        let patch = form.into_patch(ScoreFormat::Point100).unwrap();
        assert_eq!(patch.media_id, 7);
        assert_eq!(patch.score_raw, Some(85));
        assert_eq!(patch.started_at.and_then(|d| d.day), Some(1));
        assert_eq!(patch.progress, None);
    }

    #[test]
    fn invalid_form_values_are_reported() {
        let score = EntryForm {
            score: Some("great".into()),
            ..EntryForm::default()
        };
        assert!(score.into_patch(ScoreFormat::Point100).is_err());
        let date = EntryForm {
            completed_at: Some("2023-13".into()),
            ..EntryForm::default()
        };
        assert!(date.into_patch(ScoreFormat::Point100).is_err());
    }

    #[tokio::test]
    async fn answers_requests_and_reports_unknown_methods() {
        let dir = tempfile::tempdir().unwrap();
        let core = Core::open(Paths::under(dir.path()).unwrap(), CoreOptions::default())
            .await
            .unwrap();
        let (client, server) = tokio::io::duplex(16 * 1024);
        let task = tokio::spawn(serve(core, server));
        let (read, mut write) = tokio::io::split(client);
        write
            .write_all(
                b"not json\n{\"id\":1,\"method\":\"session\"}\n{\"id\":2,\"method\":\"nope\"}\n",
            )
            .await
            .unwrap();
        let mut lines = BufReader::new(read).lines();
        let mut replies = BTreeMap::new();
        for _ in 0..2 {
            let line = lines.next_line().await.unwrap().unwrap();
            let value: Value = serde_json::from_str(&line).unwrap();
            replies.insert(value["id"].as_u64().unwrap(), value);
        }
        assert_eq!(replies[&1]["result"]["setup_needed"], true);
        assert_eq!(replies[&1]["result"]["auth"]["anilist_logged_in"], false);
        assert_eq!(replies[&2]["error"], "unknown method nope");
        drop(write);
        drop(lines);
        task.await.unwrap().unwrap();
    }
}
