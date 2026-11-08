//! Grafische Oberfläche mit GTK4 und libadwaita.

mod app;
mod library;
mod list;
mod mapping;
mod player;
mod settings;
mod sync;
mod util;

use anyhow::Context;
use musubi_core::Core;
use relm4::RelmApp;

const APP_ID: &str = "io.github.musubi.Musubi";

const CSS: &str = "
.musubi-poster { border-radius: 8px; background-color: alpha(currentColor, 0.08); }
.musubi-badge {
    border-radius: 6px; padding: 1px 6px; margin: 6px; font-size: 0.8em; font-weight: bold;
    background-color: alpha(black, 0.72); color: white;
}
.musubi-badge.watching { background-color: #26a269; }
.musubi-badge.completed { background-color: #1c71d8; }
.musubi-badge.paused { background-color: #c88800; }
.musubi-badge.dropped { background-color: #c01c28; }
.musubi-badge.planning { background-color: #813d9c; }
.musubi-badge.review { background-color: #e5a50a; color: black; }
";

/// Startet die GUI und blockiert, bis das Fenster geschlossen wird.
/// Der Aufrufer muss einen Tokio-Runtime am Leben halten.
pub fn run(core: Core) -> anyhow::Result<()> {
    // relm4 bricht sonst mit einem Panic ab, wenn kein Display erreichbar ist.
    relm4::gtk::init().context("could not connect to a display, try `musubi tui`")?;
    let app = RelmApp::new(APP_ID).with_args(Vec::new());
    relm4::set_global_css(CSS);
    app.run::<app::App>(core);
    Ok(())
}
