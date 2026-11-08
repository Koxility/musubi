//! Keyboard-first Terminal-Oberfläche.
//!
//! Aufbau nach dem Elm-Muster: Tasten und Ergebnisse ändern den Zustand in [`app::App`] und
//! liefern `Effect`-Werte. Nur [`effects`] spricht mit dem Kern, nur [`view`] zeichnet.

pub mod app;
mod effects;
pub mod form;
pub mod fuzzy;
pub mod keys;
pub mod view;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use musubi_core::{Core, PlayerCommand, oauth};
use ratatui_image::picker::Picker;
use tokio::sync::{broadcast, mpsc};

use app::{App, Msg, SetupInfo};

/// Startet die TUI und kehrt beim Beenden zurück.
pub async fn run(core: Core) -> Result<()> {
    let mut terminal = ratatui::init();
    let picker = core.config().tui.posters.then(choose_picker);
    let result = event_loop(&mut terminal, core, picker).await;
    ratatui::restore();
    result
}

/// Wählt das Grafikprotokoll für Poster.
///
/// Die Abfrage von `ratatui-image` lässt bei Terminals ohne Antwort einen Thread in `stdin`
/// zurück, der die nächste Eingabe verschluckt und danach den Raw-Modus abschaltet. Deshalb
/// wird nur gefragt, wo eine Antwort sicher kommt. Sonst dienen Halbblöcke als Darstellung.
fn choose_picker() -> Picker {
    if terminal_answers_queries(
        |name| std::env::var_os(name).is_some(),
        |name| std::env::var(name).ok(),
    ) {
        Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks())
    } else {
        Picker::halfblocks()
    }
}

fn terminal_answers_queries(
    is_set: impl Fn(&str) -> bool,
    value: impl Fn(&str) -> Option<String>,
) -> bool {
    const MARKERS: [&str; 4] = [
        "KITTY_WINDOW_ID",
        "GHOSTTY_RESOURCES_DIR",
        "WEZTERM_PANE",
        "KONSOLE_VERSION",
    ];
    const TERMS: [&str; 3] = ["xterm-kitty", "xterm-ghostty", "foot"];
    const PROGRAMS: [&str; 3] = ["WezTerm", "ghostty", "iTerm.app"];
    MARKERS.iter().any(|name| is_set(name))
        || value("TERM").is_some_and(|t| TERMS.iter().any(|known| t.starts_with(known)))
        || value("TERM_PROGRAM").is_some_and(|p| PROGRAMS.contains(&p.as_str()))
}

async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    core: Core,
    picker: Option<Picker>,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let mut core_events = core.subscribe();
    let mut keys = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(200));

    let config = core.config();
    let mut app = App::new(keys::Keymap::with_overrides(&config.tui.keys), picker);
    app.setup_info = SetupInfo {
        jellyfin_url: config.jellyfin.url.clone(),
        client_id: config.anilist.client_id.clone().unwrap_or_default(),
        redirect: oauth::loopback_redirect(config.anilist.loopback_port),
        auto_sync: config.sync.auto_sync,
    };
    let initial = app.startup(&core.auth_status());
    if core.setup_needed() {
        app.begin_setup();
    }
    effects::spawn_all(&core, &tx, initial);

    loop {
        terminal.draw(|frame| view::draw(frame, &mut app))?;
        let effects = tokio::select! {
            event = keys.next() => match event {
                Some(Ok(Event::Key(key))) if key.kind != KeyEventKind::Release => app.on_key(key),
                Some(Err(error)) => return Err(error.into()),
                None => break,
                _ => vec![],
            },
            Some(message) = rx.recv() => app.on_msg(message),
            event = core_events.recv() => match event {
                Ok(event) => app.on_core_event(event),
                Err(broadcast::error::RecvError::Lagged(_)) => vec![],
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = tick.tick() => app.on_tick(),
        };
        effects::spawn_all(&core, &tx, effects);
        if app.should_quit {
            break;
        }
    }
    let _ = core.player_command(PlayerCommand::Stop).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::terminal_answers_queries;

    fn check(vars: &[(&str, &str)]) -> bool {
        terminal_answers_queries(
            |name| vars.iter().any(|(k, _)| *k == name),
            |name| {
                vars.iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            },
        )
    }

    #[test]
    fn only_known_terminals_are_queried() {
        assert!(check(&[("KONSOLE_VERSION", "250800")]));
        assert!(check(&[("TERM", "xterm-kitty")]));
        assert!(check(&[("TERM", "foot-extra")]));
        assert!(check(&[("TERM_PROGRAM", "WezTerm")]));
        assert!(!check(&[("TERM", "xterm-256color")]));
        assert!(!check(&[]));
    }
}
