//! Lädt Poster, Hintergrundbilder und Cover erst, wenn sie sichtbar werden.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use iced::Task;
use iced::widget::image::Handle;
use musubi_core::Core;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImageKey {
    Poster(String),
    Backdrop(String),
    Cover(String),
}

#[derive(Default)]
pub struct ImageCache {
    handles: HashMap<ImageKey, Handle>,
    requested: HashSet<ImageKey>,
}

impl ImageCache {
    pub fn get(&self, key: &ImageKey) -> Option<&Handle> {
        self.handles.get(key)
    }

    /// Fordert ein Bild einmalig an. Fehlgeschlagene Bilder bleiben Platzhalter.
    pub fn request(
        &mut self,
        core: &Core,
        key: ImageKey,
    ) -> Option<Task<(ImageKey, Result<PathBuf, String>)>> {
        if !self.requested.insert(key.clone()) {
            return None;
        }
        let core = core.clone();
        Some(Task::future(async move {
            let path = match &key {
                ImageKey::Poster(id) => core.poster(id).await,
                ImageKey::Backdrop(id) => core.backdrop(id).await,
                ImageKey::Cover(url) => core.cover(url).await,
            };
            (key, path.map_err(|e| e.to_string()))
        }))
    }

    pub fn loaded(&mut self, key: ImageKey, path: Result<PathBuf, String>) {
        match path {
            Ok(path) => {
                self.handles.insert(key, Handle::from_path(path));
            }
            Err(error) => tracing::debug!(%error, ?key, "image not available"),
        }
    }
}
