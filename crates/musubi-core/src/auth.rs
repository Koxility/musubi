//! Anmeldung und Abmeldung bei Jellyfin und AniList.

use std::time::Duration;

use musubi_anilist::oauth;
use musubi_domain::Viewer;
use musubi_jellyfin::Client as JellyfinClient;
use musubi_store::SecretKey;
use secrecy::{ExposeSecret, SecretString};

use crate::inner::{AniListSession, CACHE_VIEWER, Inner, JellyfinSession, now};
use crate::{Core, CoreError, CoreEvent, Result, Service};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const EXPIRY_WARNING_DAYS: i64 = 14;

impl Core {
    async fn finish_jellyfin_login(
        &self,
        client: JellyfinClient,
        token: SecretString,
        user_id: String,
        user_name: String,
    ) -> Result<String> {
        self.inner.secrets.set(SecretKey::JellyfinToken, &token)?;
        self.inner.set_jellyfin_meta(&user_id, &user_name).await?;
        let url = client.base_url().to_owned();
        self.update_config(|c| c.jellyfin.url = Some(url))?;
        let session = JellyfinSession {
            client: client.with_token(token),
            user_id,
            user_name: user_name.clone(),
        };
        *self
            .inner
            .jellyfin
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(session);
        self.inner.emit(CoreEvent::LibraryChanged);
        Ok(user_name)
    }

    /// Das Passwort wird nur für diesen Aufruf verwendet und nie gespeichert.
    pub async fn login_jellyfin(
        &self,
        url: &str,
        user: &str,
        password: &SecretString,
    ) -> Result<String> {
        let client = JellyfinClient::new(url, Inner::identity(&self.inner.device_id))?;
        client.check_server().await?;
        let auth = client.authenticate(user, password).await?;
        self.finish_jellyfin_login(client, auth.token, auth.user_id, auth.user_name)
            .await
    }

    pub async fn login_jellyfin_api_key(
        &self,
        url: &str,
        api_key: SecretString,
        user: &str,
    ) -> Result<String> {
        let client = JellyfinClient::new(url, Inner::identity(&self.inner.device_id))?;
        client.check_server().await?;
        let authed = client.clone().with_token(api_key.clone());
        let (user_id, user_name) = authed.find_user(user).await?;
        self.finish_jellyfin_login(client, api_key, user_id, user_name)
            .await
    }

    pub async fn logout_jellyfin(&self) -> Result<()> {
        let session = self
            .inner
            .jellyfin
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(session) = session
            && let Err(error) = session.client.logout().await
        {
            tracing::warn!(%error, "server-side Jellyfin logout failed");
        }
        self.inner.secrets.delete(SecretKey::JellyfinToken)?;
        self.inner.clear_jellyfin_meta().await?;
        self.inner.db.clear_prefix("library.").await?;
        self.inner.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    /// Client ID and loopback port for the login: the user's own AniList app when one is
    /// configured, otherwise the app built into Musubi.
    fn anilist_app(&self) -> Result<(String, u16)> {
        let config = self.inner.config().anilist;
        let own = config
            .client_id
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty());
        match (own, oauth::builtin_client_id()) {
            (Some(id), _) => Ok((id, config.loopback_port)),
            // The built-in app is registered with one fixed redirect URL.
            (None, Some(id)) => Ok((id.to_owned(), oauth::BUILTIN_LOOPBACK_PORT)),
            (None, None) => Err(CoreError::Invalid(
                "this build has no AniList app; create one at \
                 https://anilist.co/settings/developer and enter its client ID"
                    .to_owned(),
            )),
        }
    }

    /// Whether the browser login works without any setup by the user.
    pub fn anilist_login_ready(&self) -> bool {
        self.anilist_app().is_ok()
    }

    /// URL where the user approves Musubi on AniList.
    pub fn anilist_authorize_url(&self) -> Result<String> {
        Ok(oauth::authorize_url(&self.anilist_app()?.0))
    }

    /// Opens the browser and waits for the redirect to the local server. The token AniList
    /// issues there is personal to the user who approved.
    pub async fn login_anilist_loopback(&self) -> Result<Viewer> {
        let (client_id, port) = self.anilist_app()?;
        let url = oauth::authorize_url(&client_id);
        let listener = oauth::bind_loopback(port).await?;
        if let Err(error) = open::that_detached(&url) {
            tracing::warn!(%error, "could not open a browser");
            return Err(CoreError::Invalid(format!(
                "could not open a browser, visit {url}"
            )));
        }
        let token = oauth::wait_for_token(listener, LOGIN_TIMEOUT).await?;
        self.finish_anilist_login(token).await
    }

    /// Takes a token copied by hand or the complete redirect URL.
    pub async fn login_anilist_pin(&self, input: &str) -> Result<Viewer> {
        let token = oauth::parse_pin_input(input).ok_or_else(|| {
            CoreError::Invalid("that does not look like an access token".to_owned())
        })?;
        self.finish_anilist_login(token).await
    }

    async fn finish_anilist_login(&self, token: SecretString) -> Result<Viewer> {
        let client = self.inner.anilist_client(token.clone())?;
        // Erst prüfen, dann speichern: ein ungültiges Token landet nie auf der Platte.
        let viewer = client.viewer().await?;
        self.inner.secrets.set(SecretKey::AniListToken, &token)?;
        self.inner
            .db
            .cache_put(CACHE_VIEWER, &viewer, now())
            .await?;
        self.inner.watch_rate_limit(&client);
        *self
            .inner
            .anilist
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(AniListSession { client });
        self.inner.set_viewer(Some(viewer.clone()));
        self.warn_if_expiring()?;
        self.inner.outbox_wake.notify_one();
        Ok(viewer)
    }

    pub async fn logout_anilist(&self) -> Result<()> {
        *self
            .inner
            .anilist
            .write()
            .unwrap_or_else(|e| e.into_inner()) = None;
        self.inner.set_viewer(None);
        self.inner
            .lists
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.inner.secrets.delete(SecretKey::AniListToken)?;
        self.inner.db.clear_prefix("anilist.").await?;
        Ok(())
    }

    /// Warnt vor dem Ablauf des Tokens, das AniList nicht verlängern kann.
    pub fn warn_if_expiring(&self) -> Result<()> {
        let Some(token) = self.inner.secrets.get(SecretKey::AniListToken)? else {
            return Ok(());
        };
        if let Some(expiry) = oauth::token_expiry(token.expose_secret()) {
            let days = (expiry - chrono::Utc::now()).num_days();
            if days <= EXPIRY_WARNING_DAYS {
                self.inner.emit(CoreEvent::TokenExpiring { days });
            }
        }
        Ok(())
    }

    pub(crate) fn require_jellyfin(&self) -> Result<JellyfinSession> {
        self.inner
            .jellyfin()
            .ok_or(CoreError::NotLoggedIn(Service::Jellyfin))
    }

    pub(crate) fn require_anilist(&self) -> Result<AniListSession> {
        self.inner
            .anilist()
            .ok_or(CoreError::NotLoggedIn(Service::AniList))
    }
}
