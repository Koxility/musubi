//! Grafische Oberfläche mit Iced.
//!
//! Aufbau nach dem Elm-Muster: Jede Seite bringt ihren Zustand, ihre Nachrichten und ihre
//! Ansicht mit, [`app`] verbindet sie mit dem Kern.

mod anilist;
mod app;
mod home;
mod icons;
mod images;
mod library;
mod mapping;
mod settings;
mod setup;
mod sync;
mod theme;
mod ui;
mod video;

use anyhow::Context;
use musubi_core::Core;

use app::App;
pub use video::{VideoLink, embedded_player};

/// Startet die GUI und kehrt beim Schließen des Fensters zurück.
pub fn run(core: Core, video: Option<VideoLink>) -> anyhow::Result<()> {
    iced::application(
        move || App::boot(core.clone(), video.clone()),
        App::update,
        App::view,
    )
    .title("Musubi")
    .theme(App::theme)
    .subscription(App::subscription)
    .window_size((1360.0, 880.0))
    .run()
    .context("could not start the GUI")
}
