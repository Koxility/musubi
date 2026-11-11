//! Grafische Oberfläche mit Iced.
//!
//! Aufbau nach dem Elm-Muster: Jede Seite bringt ihren Zustand, ihre Nachrichten und ihre
//! Ansicht mit, [`app`] verbindet sie mit dem Kern.

mod anilist;
mod anilist_login;
mod app;
mod home;
mod icons;
mod images;
mod library;
mod logo;
mod mapping;
mod profile;
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
    let mut app = iced::application(
        move || App::boot(core.clone(), video.clone()),
        App::update,
        App::view,
    )
    .title("Musubi")
    .theme(App::theme)
    .subscription(App::subscription)
    .default_font(theme::BODY)
    .window(iced::window::Settings {
        size: iced::Size::new(1360.0, 880.0),
        min_size: Some(iced::Size::new(960.0, 620.0)),
        icon: logo::window_icon(),
        platform_specific: iced::window::settings::PlatformSpecific {
            // Muss zu `StartupWMClass` in `packaging/musubi.desktop` passen.
            application_id: "io.github.musubi.Musubi".to_owned(),
            ..Default::default()
        },
        ..Default::default()
    });
    for font in theme::FONT_FILES {
        app = app.font(font);
    }
    app.run().context("could not start the GUI")
}
