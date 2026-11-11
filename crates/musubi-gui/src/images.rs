//! Lädt Poster, Hintergrundbilder und Cover erst, wenn sie sichtbar werden.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use iced::Task;
use iced::widget::image::{self, Allocation, Handle};
use musubi_core::Core;

use crate::theme;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImageKey {
    Poster(String),
    Backdrop(String),
    Cover(String),
}

pub struct ImageCache {
    /// Each picture with the moment it arrived, from which it fades in.
    handles: HashMap<ImageKey, (Handle, Instant)>,
    requested: HashSet<ImageKey>,
    /// The app's frame clock; fades are read at this instant.
    now: Instant,
    newest: Option<Instant>,
}

impl Default for ImageCache {
    fn default() -> Self {
        Self {
            handles: HashMap::new(),
            requested: HashSet::new(),
            now: Instant::now(),
            newest: None,
        }
    }
}

/// Liest ein Bild aus dem Cache in den Speicher.
///
/// Cache-Dateien heißen `*.img`. Bei einem Pfad wählt Iced den Decoder nach der Dateiendung und
/// scheitert daran, bei Bytes richtet er sich nach dem Inhalt.
async fn read_cached(path: &Path) -> Result<Handle, String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    Ok(Handle::from_bytes(bytes))
}

async fn fetch(core: &Core, key: &ImageKey) -> Result<(PathBuf, Handle), String> {
    let path = match key {
        ImageKey::Poster(id) => core.poster(id).await,
        ImageKey::Backdrop(id) => core.backdrop(id).await,
        ImageKey::Cover(url) => core.cover(url).await,
    }
    .map_err(|e| e.to_string())?;
    let handle = read_cached(&path).await?;
    Ok((path, handle))
}

/// Wertet das Dekodieren aus. Unlesbare Dateien fliegen aus dem Cache, damit der nächste Start
/// sie neu lädt. Die `Allocation` fällt hier weg, so kann der Renderer unsichtbare Bilder freigeben.
fn decoded(
    path: &Path,
    handle: Handle,
    allocation: Result<Allocation, image::Error>,
) -> Result<Handle, String> {
    match allocation {
        Ok(_) => Ok(handle),
        Err(error) => {
            if matches!(error, image::Error::Invalid(_) | image::Error::Empty) {
                let _ = std::fs::remove_file(path);
            }
            Err(format!("could not decode {}: {error}", path.display()))
        }
    }
}

impl ImageCache {
    pub fn get(&self, key: &ImageKey) -> Option<&Handle> {
        self.handles.get(key).map(|(handle, _)| handle)
    }

    /// How visible a picture is while it fades in after loading.
    pub fn opacity(&self, key: &ImageKey) -> f32 {
        self.handles.get(key).map_or(1.0, |(_, since)| {
            theme::fade_in(self.now.saturating_duration_since(*since))
        })
    }

    /// Moves the fades on to the frame at `at`.
    pub fn tick(&mut self, at: Instant) {
        self.now = at;
    }

    /// Whether a picture is still fading in.
    pub fn fading(&self) -> bool {
        self.newest
            .is_some_and(|since| self.now.saturating_duration_since(since) < theme::FADE)
    }

    /// Fordert ein Bild einmalig an. Fehlgeschlagene Bilder bleiben Platzhalter.
    pub fn request(
        &mut self,
        core: &Core,
        key: ImageKey,
    ) -> Option<Task<(ImageKey, Result<Handle, String>)>> {
        if !self.requested.insert(key.clone()) {
            return None;
        }
        let core = core.clone();
        let task = Task::future(async move {
            let fetched = fetch(&core, &key).await;
            (key, fetched)
        })
        .then(|(key, fetched)| match fetched {
            Err(error) => Task::done((key, Err(error))),
            Ok((path, handle)) => image::allocate(handle.clone())
                .map(move |allocation| (key.clone(), decoded(&path, handle.clone(), allocation))),
        });
        Some(task)
    }

    pub fn loaded(&mut self, key: ImageKey, result: Result<Handle, String>) {
        match result {
            Ok(handle) => {
                let now = Instant::now();
                self.now = now;
                self.newest = Some(now);
                self.handles.insert(key, (handle, now));
            }
            Err(error) => tracing::warn!(%error, ?key, "image could not be loaded"),
        }
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::graphics::image::load;

    use super::*;

    /// Ein Pixel als PNG.
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0,
        3, 1, 1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    #[tokio::test]
    async fn cache_files_decode_despite_their_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jf-0123.img");
        std::fs::write(&path, PNG).unwrap();

        let handle = read_cached(&path).await.unwrap();

        assert_eq!(load(&handle).unwrap().dimensions(), (1, 1));
    }

    #[tokio::test]
    async fn a_missing_file_names_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.img");

        let error = read_cached(&path).await.unwrap_err();

        assert!(error.contains("missing.img"), "{error}");
    }

    #[test]
    fn undecodable_files_leave_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.img");
        std::fs::write(&path, b"<html>").unwrap();

        let result = decoded(&path, Handle::from_bytes(vec![]), Err(image::Error::Empty));

        assert!(result.unwrap_err().contains("broken.img"));
        assert!(!path.exists());
    }

    #[test]
    fn failed_images_stay_placeholders() {
        let mut cache = ImageCache::default();
        let key = ImageKey::Poster("a".into());
        cache.loaded(key.clone(), Err("nope".into()));
        assert!(cache.get(&key).is_none());
    }
}
