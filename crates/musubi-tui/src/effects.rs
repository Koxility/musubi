//! Führt Effekte asynchron aus und schickt die Ergebnisse als Nachrichten zurück.

use musubi_core::{Core, Result};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::{Effect, Msg, PosterSource};

pub fn spawn_all(core: &Core, tx: &UnboundedSender<Msg>, effects: Vec<Effect>) {
    for effect in effects {
        let (core, tx) = (core.clone(), tx.clone());
        tokio::spawn(async move {
            if let Err(error) = perform(&core, &tx, effect).await {
                tracing::warn!(%error, "action failed");
                let _ = tx.send(Msg::Error(error.to_string()));
            }
        });
    }
}

async fn perform(core: &Core, tx: &UnboundedSender<Msg>, effect: Effect) -> Result<()> {
    let send = |msg: Msg| {
        let _ = tx.send(msg);
    };
    match effect {
        Effect::LoadCachedLibrary => send(Msg::Library(core.cached_library().await?)),
        Effect::LoadLibrary(query) => send(Msg::Library(core.library(&query).await?)),
        Effect::LoadChoices => {
            let filters = core.filters(None).await?;
            let libraries = core.libraries().await?;
            send(Msg::Choices(filters, libraries));
        }
        Effect::LoadEpisodes(series_id) => {
            send(Msg::Episodes(
                series_id.clone(),
                core.episodes(&series_id).await?,
            ));
        }
        Effect::ReadList(kind) => send(Msg::List(kind, core.list(kind))),
        Effect::RefreshList(kind) => {
            send(Msg::List(kind, core.refresh_list(kind).await?));
            send(Msg::Viewer(core.viewer()));
        }
        Effect::RefreshViewer => send(Msg::Viewer(Some(core.refresh_viewer().await?))),
        Effect::ReadViewer => send(Msg::Viewer(core.viewer())),
        Effect::LoadMappingData => {
            send(Msg::MappingData(
                core.reviews().await?,
                core.all_mappings().await?,
            ));
        }
        Effect::Play(item, episode) => core.play(&item, episode.as_deref()).await?,
        Effect::Player(command) => core.player_command(command).await?,
        Effect::SetPlayed(id, played) => core.set_played(&id, played).await?,
        Effect::Save(patch) => core.save_entry(patch).await?,
        Effect::Add(media, status) => core.add_to_list(media, status).await?,
        Effect::Delete(entry) => core.delete_entry(&entry).await?,
        Effect::Search(text, kind) => {
            send(Msg::SearchResults(core.search_anilist(&text, kind).await?));
        }
        Effect::MatchAll => core.match_all().await?,
        Effect::MapManual(item, media, follow) => core.map_manually(&item, media, follow).await?,
        Effect::SetMappings(id, mappings) => core.set_mappings(&id, mappings).await?,
        Effect::Unmap(id) => core.unmap(&id).await?,
        Effect::LoadDiff => send(Msg::Diff(core.sync_diff().await?)),
        Effect::ApplySync(rows) => {
            send(Msg::SyncDone(core.apply_sync(&rows).await?));
            send(Msg::Diff(core.sync_diff().await?));
        }
        Effect::LoadSyncStatus => {
            send(Msg::SyncStatus(
                core.pending_writes().await?,
                core.sync_log(50).await?,
            ));
        }
        Effect::FlushOutbox => core.flush_outbox().await?,
        Effect::LoginJellyfinPassword {
            url,
            user,
            password,
        } => {
            let name = core.login_jellyfin(&url, &user, &password).await?;
            send(Msg::LoggedIn(
                format!("Logged in to Jellyfin as {name}"),
                core.auth_status(),
            ));
        }
        Effect::LoginJellyfinKey { url, user, key } => {
            let name = core.login_jellyfin_api_key(&url, key, &user).await?;
            send(Msg::LoggedIn(
                format!("Logged in to Jellyfin as {name}"),
                core.auth_status(),
            ));
        }
        Effect::LoginAniListBrowser(client_id) => {
            core.update_config(|c| c.anilist.client_id = Some(client_id))?;
            let viewer = core.login_anilist_loopback().await?;
            send(Msg::LoggedIn(
                format!("Logged in to AniList as {}", viewer.name),
                core.auth_status(),
            ));
        }
        Effect::LoginAniListPin(input) => {
            let viewer = core.login_anilist_pin(&input).await?;
            send(Msg::LoggedIn(
                format!("Logged in to AniList as {}", viewer.name),
                core.auth_status(),
            ));
        }
        Effect::LogoutJellyfin => {
            core.logout_jellyfin().await?;
            send(Msg::LoggedIn(
                "Logged out of Jellyfin".to_owned(),
                core.auth_status(),
            ));
        }
        Effect::LogoutAniList => {
            core.logout_anilist().await?;
            send(Msg::LoggedIn(
                "Logged out of AniList".to_owned(),
                core.auth_status(),
            ));
        }
        Effect::CreateList(kind, name) => {
            core.create_custom_list(kind, &name).await?;
            send(Msg::Viewer(core.viewer()));
        }
        Effect::DeleteList(kind, name) => {
            core.delete_custom_list(kind, &name).await?;
            send(Msg::Viewer(core.viewer()));
        }
        Effect::RenameList(kind, old, new) => {
            core.rename_custom_list(kind, &old, &new).await?;
            send(Msg::Viewer(core.viewer()));
        }
        Effect::LoadPoster(key, source) => {
            let path = match source {
                PosterSource::Jellyfin(id) => core.poster(&id).await?,
                PosterSource::Url(url) => core.cover(&url).await?,
            };
            let decoded = tokio::task::spawn_blocking(move || {
                image::ImageReader::open(path)
                    .ok()?
                    .with_guessed_format()
                    .ok()?
                    .decode()
                    .ok()
            })
            .await
            .ok()
            .flatten();
            if let Some(image) = decoded {
                send(Msg::Poster(key, image));
            }
        }
    }
    Ok(())
}
