//! Zentraler Zustand und Aufbau von [`Core`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use musubi_domain::{ListEntry, MediaKind, Viewer};
use musubi_player::{PlayerBackend, PlayerError, SessionHandle};
use musubi_store::{Config, Db, Paths, SecretKey, SecretStore, open_secret_store};
use tokio::sync::{Notify, broadcast};

use crate::{CoreEvent, Result};

const META_USER_ID: &str = "jellyfin.user_id";
const META_USER_NAME: &str = "jellyfin.user_name";
const META_DEVICE_ID: &str = "jellyfin.device_id";
pub(crate) const CACHE_VIEWER: &str = "anilist.viewer";

pub(crate) fn list_cache_key(kind: MediaKind) -> String {
    format!("anilist.list.{}", kind.as_api())
}

#[derive(Clone)]
pub(crate) struct JellyfinSession {
    pub client: musubi_jellyfin::Client,
    pub user_id: String,
    pub user_name: String,
}

#[derive(Clone)]
pub(crate) struct AniListSession {
    pub client: musubi_anilist::Client,
}

pub(crate) struct ActivePlayback {
    pub generation: u64,
    pub session: SessionHandle,
}

pub(crate) struct Inner {
    pub paths: Paths,
    pub config: RwLock<Config>,
    pub secrets: Box<dyn SecretStore>,
    pub db: Db,
    pub device_id: String,
    pub jellyfin: RwLock<Option<JellyfinSession>>,
    pub anilist: RwLock<Option<AniListSession>>,
    pub viewer: RwLock<Option<Viewer>>,
    pub lists: RwLock<HashMap<MediaKind, Vec<ListEntry>>>,
    pub events: broadcast::Sender<CoreEvent>,
    pub outbox_wake: Notify,
    /// Der Player der GUI. Fehlt er, etwa ohne GStreamer, gibt es keine Wiedergabe.
    pub player: Option<Arc<dyn PlayerBackend>>,
    pub active: Mutex<Option<ActivePlayback>>,
    pub generation: Mutex<u64>,
    pub anilist_endpoint: Option<String>,
    pub http: reqwest::Client,
}

/// Schnittstelle für die GUI. Klonen ist billig.
#[derive(Clone)]
pub struct Core {
    pub(crate) inner: Arc<Inner>,
}

/// Abweichungen vom Normalbetrieb, vor allem für Tests.
#[derive(Default)]
pub struct CoreOptions {
    pub anilist_endpoint: Option<String>,
    pub player: Option<Arc<dyn PlayerBackend>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthStatus {
    pub jellyfin_user: Option<String>,
    pub jellyfin_url: Option<String>,
    pub anilist_user: Option<String>,
    pub anilist_logged_in: bool,
}

pub(crate) fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Inner {
    pub fn jellyfin(&self) -> Option<JellyfinSession> {
        self.jellyfin
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn anilist(&self) -> Option<AniListSession> {
        self.anilist
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn viewer(&self) -> Option<Viewer> {
        self.viewer
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_viewer(&self, viewer: Option<Viewer>) {
        *self.viewer.write().unwrap_or_else(|e| e.into_inner()) = viewer;
    }

    pub fn config(&self) -> Config {
        self.config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn emit(&self, event: CoreEvent) {
        let _ = self.events.send(event);
    }

    pub fn list(&self, kind: MediaKind) -> Vec<ListEntry> {
        self.lists
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&kind)
            .cloned()
            .unwrap_or_default()
    }

    pub fn entry_for(&self, media_id: i64) -> Option<ListEntry> {
        let lists = self.lists.read().unwrap_or_else(|e| e.into_inner());
        lists
            .values()
            .flatten()
            .find(|e| e.media.id == media_id)
            .cloned()
    }

    pub fn player_backend(&self) -> Result<Arc<dyn PlayerBackend>> {
        self.player.clone().ok_or_else(|| {
            PlayerError::Unavailable(
                "Video playback is not available: install GStreamer with gst-plugins-base, \
                 gst-plugins-good, gst-plugins-bad and gst-libav"
                    .to_owned(),
            )
            .into()
        })
    }

    pub fn identity(device_id: &str) -> musubi_jellyfin::Identity {
        let device = std::env::var("HOSTNAME")
            .ok()
            .or_else(|| {
                std::fs::read_to_string("/etc/hostname")
                    .ok()
                    .map(|h| h.trim().to_owned())
            })
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "Linux".to_owned());
        musubi_jellyfin::Identity {
            client: "Musubi".to_owned(),
            device,
            device_id: device_id.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    pub fn anilist_client(
        &self,
        token: secrecy::SecretString,
    ) -> std::result::Result<musubi_anilist::Client, musubi_anilist::Error> {
        match &self.anilist_endpoint {
            Some(endpoint) => musubi_anilist::Client::with_endpoint(endpoint, Some(token)),
            None => musubi_anilist::Client::new(Some(token)),
        }
    }

    /// Meldet Drosselungen des Rate-Limiters an die GUI.
    pub fn watch_rate_limit(self: &Arc<Self>, client: &musubi_anilist::Client) {
        let mut notices = client.limiter().subscribe();
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            while notices.changed().await.is_ok() {
                let Some(inner) = weak.upgrade() else { return };
                inner.emit(CoreEvent::RateLimited(*notices.borrow_and_update()));
            }
        });
    }

    pub async fn set_jellyfin_meta(&self, user_id: &str, user_name: &str) -> Result<()> {
        self.db.meta_set(META_USER_ID, user_id).await?;
        self.db.meta_set(META_USER_NAME, user_name).await?;
        Ok(())
    }

    pub async fn clear_jellyfin_meta(&self) -> Result<()> {
        self.db.meta_delete(META_USER_ID).await?;
        self.db.meta_delete(META_USER_NAME).await?;
        Ok(())
    }
}

impl Core {
    /// Öffnet Konfiguration, Datenbank und gespeicherte Sitzungen, ohne das Netz zu nutzen.
    /// Muss innerhalb eines Tokio-Runtimes aufgerufen werden.
    pub async fn open(paths: Paths, options: CoreOptions) -> Result<Self> {
        let config = Config::load(&paths.config_file())?;
        let secrets = open_secret_store(config.secrets.backend, &paths)?;
        let db = Db::open(&paths.database_file())?;

        let device_id = match db.meta_get(META_DEVICE_ID).await? {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().simple().to_string();
                db.meta_set(META_DEVICE_ID, &id).await?;
                id
            }
        };

        let http = reqwest::Client::builder()
            .user_agent(concat!("Musubi/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let (events, _) = broadcast::channel(512);
        let inner = Arc::new(Inner {
            paths,
            config: RwLock::new(config),
            secrets,
            db,
            device_id,
            jellyfin: RwLock::new(None),
            anilist: RwLock::new(None),
            viewer: RwLock::new(None),
            lists: RwLock::new(HashMap::new()),
            events,
            outbox_wake: Notify::new(),
            player: options.player,
            active: Mutex::new(None),
            generation: Mutex::new(0),
            anilist_endpoint: options.anilist_endpoint,
            http,
        });

        Self::restore_sessions(&inner).await?;
        crate::outbox::spawn_worker(Arc::clone(&inner));
        Ok(Self { inner })
    }

    async fn restore_sessions(inner: &Arc<Inner>) -> Result<()> {
        let url = inner.config().jellyfin.url;
        let token = inner.secrets.get(SecretKey::JellyfinToken)?;
        let user_id = inner.db.meta_get(META_USER_ID).await?;
        let user_name = inner.db.meta_get(META_USER_NAME).await?;
        if let (Some(url), Some(token), Some(user_id)) = (url, token, user_id) {
            let client = musubi_jellyfin::Client::new(&url, Inner::identity(&inner.device_id))?
                .with_token(token);
            let session = JellyfinSession {
                client,
                user_id,
                user_name: user_name.unwrap_or_default(),
            };
            *inner.jellyfin.write().unwrap_or_else(|e| e.into_inner()) = Some(session);
        }

        if let Some(token) = inner.secrets.get(SecretKey::AniListToken)? {
            let client = inner.anilist_client(token)?;
            inner.watch_rate_limit(&client);
            *inner.anilist.write().unwrap_or_else(|e| e.into_inner()) =
                Some(AniListSession { client });
            inner.set_viewer(inner.db.cache_get::<Viewer>(CACHE_VIEWER).await?);
            for kind in [MediaKind::Anime, MediaKind::Manga] {
                if let Some(list) = inner
                    .db
                    .cache_get::<Vec<ListEntry>>(&list_cache_key(kind))
                    .await?
                {
                    inner
                        .lists
                        .write()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(kind, list);
                }
            }
        }
        Ok(())
    }

    /// Abonniert die Ereignisse des Kerns.
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.inner.events.subscribe()
    }

    pub fn config(&self) -> Config {
        self.inner.config()
    }

    pub fn update_config(&self, change: impl FnOnce(&mut Config)) -> Result<()> {
        let mut config = self.inner.config.write().unwrap_or_else(|e| e.into_inner());
        let mut updated = config.clone();
        change(&mut updated);
        updated.save(&self.inner.paths.config_file())?;
        *config = updated;
        Ok(())
    }

    pub fn paths(&self) -> &Paths {
        &self.inner.paths
    }

    pub fn auth_status(&self) -> AuthStatus {
        let jellyfin = self.inner.jellyfin();
        AuthStatus {
            jellyfin_user: jellyfin.as_ref().map(|s| s.user_name.clone()),
            jellyfin_url: jellyfin.as_ref().map(|s| s.client.base_url().to_owned()),
            anilist_user: self.inner.viewer().map(|v| v.name),
            anilist_logged_in: self.inner.anilist().is_some(),
        }
    }

    /// Ob die geführte Einrichtung beim Start von selbst erscheinen soll.
    pub fn setup_needed(&self) -> bool {
        setup_needed(&self.config(), &self.auth_status())
    }

    pub fn viewer(&self) -> Option<Viewer> {
        self.inner.viewer()
    }

    pub async fn sync_log(&self, limit: u32) -> Result<Vec<musubi_store::SyncLogEntry>> {
        Ok(self.inner.db.sync_log(limit).await?)
    }
}

fn setup_needed(config: &Config, auth: &AuthStatus) -> bool {
    !config.ui.setup_completed && (auth.jellyfin_user.is_none() || !auth.anilist_logged_in)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(jellyfin: bool, anilist: bool) -> AuthStatus {
        AuthStatus {
            jellyfin_user: jellyfin.then(|| "user".to_owned()),
            anilist_logged_in: anilist,
            ..AuthStatus::default()
        }
    }

    #[test]
    fn setup_appears_while_a_login_is_missing() {
        let config = Config::default();
        assert!(setup_needed(&config, &auth(false, false)));
        assert!(setup_needed(&config, &auth(true, false)));
        assert!(setup_needed(&config, &auth(false, true)));
        assert!(!setup_needed(&config, &auth(true, true)));
    }

    #[test]
    fn finished_setup_never_returns_by_itself() {
        let mut config = Config::default();
        config.ui.setup_completed = true;
        assert!(!setup_needed(&config, &auth(false, false)));
    }
}
