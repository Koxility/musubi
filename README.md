# Musubi

**Musubi** (結び, „Verbindung, Knoten“) streamt Anime von deinem Jellyfin-Server und verwaltet deine AniList vollständig. Die Oberfläche ist eine GUI mit Iced.

- **Startseite:** großes Titelbild und Regale für Weiterschauen, Nächste Episoden und Zuletzt hinzugefügt, wie in einem Mediacenter.
- **AniList-Übersicht:** Airing mit Countdown und Rückstand, Anime und Manga in Progress, `+` für die nächste Episode.
- **Jellyfin:** Login per Passwort oder API-Key, Bibliothek durchsuchen und filtern (Genre, Status, Jahr, Typ), Episoden mit Metadaten, Direct Stream mit Transcoding-Fallback, Fortschritt wird zurückgemeldet.
- **AniList:** Einträge anlegen, bearbeiten und löschen, Status, Fortschritt (Episoden, Kapitel, Bände), Score in deinem Format, Advanced Scoring, Notizen, Custom Lists, Start- und Enddatum, Private und Sichtbarkeit. Funktioniert auch ganz ohne Jellyfin.
- **Sync:** automatisch beim Schauen oder manuell mit Vorschau. Schreibzugriffe landen in einer Warteschlange und gehen offline nicht verloren.
- **Matching:** Provider-IDs, Titelvergleich, Fortsetzungen und Cours, Prüfliste für unklare Fälle, exakte manuelle Zuordnung.
- **Player:** mpv mit Hardware-Decoding, Untertiteln und deiner eigenen `mpv.conf`.

## Voraussetzungen

Linux mit Wayland oder X11 und:

| Paket | Zweck |
|---|---|
| Rust ab 1.95 | Build |
| `mpv` | Wiedergabe (zur Laufzeit) |
| C-Compiler, `pkgconf` | Build (SQLite wird mitgebaut) |

Auf Arch, CachyOS und Verwandten:

```bash
sudo pacman -S --needed base-devel mpv
```

Rust installierst du über [rustup](https://rustup.rs). Liegt es wie hier unter `~/.cargo/bin` und du nutzt fish:

```bash
fish_add_path ~/.cargo/bin
```

## Build und Installation

```bash
cargo build --release
```

```bash
cargo install --path crates/musubi
```

Optional für den Anwendungsstarter:

```bash
install -Dm644 packaging/musubi.desktop ~/.local/share/applications/musubi.desktop
```

```bash
install -Dm644 packaging/musubi.svg ~/.local/share/icons/hicolor/scalable/apps/musubi.svg
```

## Einrichtung

Beim ersten Start führt dich Musubi Schritt für Schritt durch Jellyfin, AniList und die Sync-Einstellung. Jeder Schritt lässt sich überspringen. Solange die Einrichtung nicht abgeschlossen ist und ein Login fehlt, erscheint sie beim nächsten Start wieder. Später startest du sie unter *Settings* mit *Run setup* erneut.

### Jellyfin

Im Setup, unter *Settings* oder per Kommandozeile:

```bash
musubi login jellyfin
```

Mit `--api-key` meldest du dich per API-Key an. Das Passwort wird nur für die Anmeldung benutzt und nie gespeichert.

### AniList

Musubi enthält bewusst keine fest eingebaute App-Kennung.

1. Lege unter <https://anilist.co/settings/developer> eine Anwendung an.
2. Trage als Redirect-URL `http://127.0.0.1:47823/callback` ein.
3. Gib die Client-ID im Setup ein oder melde dich per Kommandozeile an. Das Client-Secret braucht Musubi nicht:

```bash
musubi login anilist
```

Auf einem Rechner ohne Browser, etwa per SSH, nutzt du als Redirect-URL `https://anilist.co/api/v2/oauth/pin` und meldest dich so an:

```bash
musubi login anilist --pin
```

AniList-Tokens gelten ein Jahr und lassen sich nicht verlängern. Musubi warnt 14 Tage vor dem Ablauf.

### Konfiguration

Alles ist optional. Die Datei `~/.config/musubi/config.toml` entsteht beim ersten Speichern:

```toml
[jellyfin]
url = "https://jellyfin.example.org"
library_ids = []            # leer = alle Bibliotheken
# max_bitrate = 8000000     # ohne Wert entscheidet der Server

[anilist]
client_id = "12345"
login_flow = "loopback"     # oder "pin"
loopback_port = 47823

[sync]
auto_sync = true
watched_threshold = 0.85    # ab 85 % gilt eine Episode als gesehen

[player]
mpv_path = "mpv"
hwdec = "auto-safe"         # leer = Einstellung aus deiner mpv.conf
extra_args = []
autoplay_next = true

[secrets]
backend = "file"            # oder "secret-service" für KWallet/GNOME Keyring
```

| Pfad | Inhalt |
|---|---|
| `~/.config/musubi/config.toml` | Einstellungen |
| `~/.config/musubi/credentials.toml` | Tokens, Rechte 0600, wird bei offeneren Rechten abgelehnt |
| `~/.local/share/musubi/musubi.db` | Zuordnungen, Warteschlange, Cache |
| `~/.cache/musubi/images/` | Poster |
| `~/.local/state/musubi/` | Logs, Detailgrad über `MUSUBI_LOG=debug` |

## Benutzung

```bash
musubi
```

startet die GUI. Ohne Oberfläche abgleichen:

```bash
musubi sync --dry-run
```

```bash
musubi sync
```

`F1` zeigt alle Kürzel. Die wichtigsten sind `Ctrl+1` bis `Ctrl+6` für die Seiten, `Ctrl+R` zum Aktualisieren, `Ctrl+P` für Pause und `Alt+←/→` zum Spulen. `Esc` schließt Dialoge und Detailseiten. Im mpv-Fenster gelten die üblichen mpv-Tasten.

## Entwicklung

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Die End-to-End-Tests nutzen Mock-Server und, falls installiert, echtes `mpv` und `ffmpeg`.

Ohne echte Server kannst du mit dem Demo-Backend starten. Es gibt zwei URLs aus und akzeptiert jedes Passwort:

```bash
cargo run -p musubi-core --example demo_backend
```

Debug-Builds schicken AniList-Anfragen an die Adresse aus `MUSUBI_DEV_ANILIST_ENDPOINT`. Release-Builds ignorieren diese Variable.

Architektur, Entscheidungen und Grenzen stehen in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
