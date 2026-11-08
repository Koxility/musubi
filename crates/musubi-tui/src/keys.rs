//! Tastenbelegung. Standardwerte lassen sich in `config.toml` unter `[tui.keys]` überschreiben.

use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,
    Help,
    NextTab,
    PrevTab,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    Down,
    Up,
    Top,
    Bottom,
    PageDown,
    PageUp,
    Open,
    Back,
    Filter,
    Command,
    Refresh,
    ServerFilter,
    Play,
    Continue,
    TogglePlayed,
    Increment,
    Decrement,
    Status,
    Score,
    Edit,
    /// Löschen im AniList-Tab, Zuordnung lösen im Mapping-Tab, Richtung wechseln im Sync-Tab.
    Delete,
    Add,
    /// Anime/Manga bzw. Prüfliste/Zuordnungen umschalten.
    Toggle,
    NextStatus,
    PrevStatus,
    Map,
    MatchAll,
    Apply,
    PlayerPause,
    PlayerBack,
    PlayerForward,
    PlayerNext,
    PlayerPrevious,
    PlayerStop,
    PlayerSubtitle,
    PlayerAudio,
}

const DEFAULTS: &[(&str, Action, &str)] = &[
    ("quit", Action::Quit, "q"),
    ("help", Action::Help, "?"),
    ("next_tab", Action::NextTab, "tab"),
    ("prev_tab", Action::PrevTab, "backtab"),
    ("tab_library", Action::Tab1, "1"),
    ("tab_anilist", Action::Tab2, "2"),
    ("tab_mapping", Action::Tab3, "3"),
    ("tab_sync", Action::Tab4, "4"),
    ("down", Action::Down, "j"),
    ("up", Action::Up, "k"),
    ("top", Action::Top, "g"),
    ("bottom", Action::Bottom, "G"),
    ("page_down", Action::PageDown, "ctrl-d"),
    ("page_up", Action::PageUp, "ctrl-u"),
    ("open", Action::Open, "enter"),
    ("back", Action::Back, "esc"),
    ("filter", Action::Filter, "/"),
    ("command", Action::Command, ":"),
    ("refresh", Action::Refresh, "r"),
    ("server_filter", Action::ServerFilter, "F"),
    ("play", Action::Play, "p"),
    ("continue", Action::Continue, "c"),
    ("toggle_played", Action::TogglePlayed, "w"),
    ("increment", Action::Increment, "+"),
    ("decrement", Action::Decrement, "-"),
    ("status", Action::Status, "s"),
    ("score", Action::Score, "S"),
    ("edit", Action::Edit, "e"),
    ("delete", Action::Delete, "d"),
    ("add", Action::Add, "a"),
    ("toggle", Action::Toggle, "t"),
    ("next_status", Action::NextStatus, "]"),
    ("prev_status", Action::PrevStatus, "["),
    ("map", Action::Map, "m"),
    ("match_all", Action::MatchAll, "M"),
    ("apply", Action::Apply, "A"),
    ("player_pause", Action::PlayerPause, "space"),
    ("player_back", Action::PlayerBack, "left"),
    ("player_forward", Action::PlayerForward, "right"),
    ("player_next", Action::PlayerNext, "n"),
    ("player_previous", Action::PlayerPrevious, "N"),
    ("player_stop", Action::PlayerStop, "X"),
    ("player_subtitle", Action::PlayerSubtitle, "v"),
    ("player_audio", Action::PlayerAudio, "b"),
];

/// Pfeiltasten funktionieren immer, auch wenn die Vim-Tasten umgelegt wurden.
const ALIASES: &[(&str, Action)] = &[
    ("down", Action::Down),
    ("up", Action::Up),
    ("home", Action::Top),
    ("end", Action::Bottom),
    ("pgdn", Action::PageDown),
    ("pgup", Action::PageUp),
    ("=", Action::Increment),
];

/// Wandelt `ctrl-d`, `enter`, `G` usw. in eine Taste um.
pub fn parse_key(text: &str) -> Option<KeyEvent> {
    let (modifiers, name) = match text.strip_prefix("ctrl-") {
        Some(rest) => (KeyModifiers::CONTROL, rest),
        None => match text.strip_prefix("alt-") {
            Some(rest) => (KeyModifiers::ALT, rest),
            None => (KeyModifiers::NONE, text),
        },
    };
    let code = match name {
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "space" => KeyCode::Char(' '),
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pgup" => KeyCode::PageUp,
        "pgdn" => KeyCode::PageDown,
        single => {
            let mut chars = single.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            KeyCode::Char(c)
        }
    };
    Some(KeyEvent::new(code, modifiers))
}

/// Terminals melden Großbuchstaben teils mit, teils ohne SHIFT. Vereinheitlicht wird ohne.
fn normalize(key: KeyEvent) -> (KeyCode, KeyModifiers) {
    let mut modifiers = key.modifiers;
    if matches!(key.code, KeyCode::Char(_) | KeyCode::BackTab) {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    (key.code, modifiers)
}

#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: HashMap<(KeyCode, KeyModifiers), Action>,
    labels: HashMap<Action, String>,
}

impl Keymap {
    pub fn with_overrides(overrides: &BTreeMap<String, String>) -> Self {
        let mut bindings = HashMap::new();
        let mut labels = HashMap::new();
        for (name, action) in ALIASES {
            if let Some(key) = parse_key(name) {
                bindings.insert(normalize(key), *action);
            }
        }
        for (name, action, default) in DEFAULTS {
            let text = overrides.get(*name).map(String::as_str).unwrap_or(default);
            match parse_key(text) {
                Some(key) => {
                    bindings.insert(normalize(key), *action);
                    labels.insert(*action, text.to_owned());
                }
                None => tracing::warn!(key = text, action = name, "ignoring invalid key binding"),
            }
        }
        Self { bindings, labels }
    }

    pub fn action(&self, key: KeyEvent) -> Option<Action> {
        self.bindings.get(&normalize(key)).copied()
    }

    pub fn label(&self, action: Action) -> &str {
        self.labels.get(&action).map(String::as_str).unwrap_or("?")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_uppercase_with_shift() {
        let map = Keymap::with_overrides(&BTreeMap::new());
        let upper = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(map.action(upper), Some(Action::Bottom));
        let ctrl = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(map.action(ctrl), Some(Action::PageDown));
        let plain = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        assert_eq!(map.action(plain), Some(Action::Delete));
    }

    #[test]
    fn override_replaces_binding_and_label() {
        let overrides = BTreeMap::from([("play".to_owned(), "ctrl-p".to_owned())]);
        let map = Keymap::with_overrides(&overrides);
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert_eq!(map.action(key), Some(Action::Play));
        assert_eq!(map.label(Action::Play), "ctrl-p");
    }

    #[test]
    fn invalid_names_are_rejected() {
        assert!(parse_key("nonsense").is_none());
        assert!(parse_key("").is_none());
    }
}
