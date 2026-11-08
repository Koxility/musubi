//! Poster-Cache auf der Platte.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::{Core, Result};

fn cache_name(prefix: &str, key: &str) -> String {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{prefix}-{:016x}.img", hasher.finish())
}

impl Core {
    async fn store_image(&self, path: &PathBuf, bytes: &[u8]) -> Result<()> {
        let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, path).await?;
        Ok(())
    }

    /// Pfad zum Poster eines Jellyfin-Items, bei Bedarf wird es geladen.
    pub async fn poster(&self, item_id: &str) -> Result<PathBuf> {
        let path = self
            .inner
            .paths
            .images_dir()
            .join(cache_name("jf", item_id));
        if !path.exists() {
            let session = self.require_jellyfin()?;
            let bytes = session.client.poster(item_id, 400).await?;
            self.store_image(&path, &bytes).await?;
        }
        Ok(path)
    }

    /// Pfad zum breiten Hintergrundbild eines Jellyfin-Items.
    pub async fn backdrop(&self, item_id: &str) -> Result<PathBuf> {
        let path = self
            .inner
            .paths
            .images_dir()
            .join(cache_name("jf-backdrop", item_id));
        if !path.exists() {
            let session = self.require_jellyfin()?;
            let bytes = session.client.backdrop(item_id, 1280).await?;
            self.store_image(&path, &bytes).await?;
        }
        Ok(path)
    }

    /// Pfad zum AniList-Cover.
    pub async fn cover(&self, url: &str) -> Result<PathBuf> {
        let path = self.inner.paths.images_dir().join(cache_name("al", url));
        if !path.exists() {
            let bytes = self
                .inner
                .http
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            self.store_image(&path, &bytes).await?;
        }
        Ok(path)
    }
}
