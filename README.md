# Musubi

**Musubi** (結び, "knot, connection") streams anime from your Jellyfin server and keeps your AniList in sync. It is a native Linux desktop app written in Rust.

- **Home** – a hero banner plus shelves for *Continue Watching*, *Next Up* and *Recently Added*, like a media center.
- **Built-in player** – plays inside the app window via GStreamer, with hardware decoding and ASS subtitles. mpv in its own window is available as an alternative.
- **Automatic matching** – every Jellyfin season is matched to its own AniList entry. Matching uses provider IDs, sequels (even across movies and OVAs), and title search. You can change, confirm or remove any match per season.
- **Sync** – progress goes to AniList automatically as you watch, or manually with a preview. Writes are queued and survive being offline.
- **AniList management** – add, edit and delete entries for anime and manga: status, progress, score in your format, advanced scoring, notes, custom lists, dates, privacy. Works without Jellyfin too.
- **Jellyfin library** – browse and filter by genre, status, year and type. Uses direct streaming with transcoding as a fallback and reports playback progress back to the server.

## Install

### Release binary

Download `musubi-<version>-x86_64-linux.tar.gz` from the [releases page](https://github.com/Koxility/musubi/releases) and unpack it:

```bash
tar xzf musubi-*-x86_64-linux.tar.gz
```

```bash
cd musubi-*-x86_64-linux && ./install.sh
```

`install.sh` copies the binary to `~/.local/bin` and the launcher and icon to `~/.local/share`. Make sure `~/.local/bin` is on your `PATH`.

The binary needs GStreamer at runtime. See [Requirements](#requirements).

### From source

```bash
cargo install --locked --path crates/musubi
```

```bash
install -Dm644 packaging/musubi.desktop ~/.local/share/applications/musubi.desktop
```

```bash
install -Dm644 packaging/musubi.svg ~/.local/share/icons/hicolor/scalable/apps/musubi.svg
```

The two `install` lines are optional; they add Musubi to your application launcher.

The bottom of the sidebar shows the version and commit you are running, e.g. `v0.1.0 · 218b921`.

## Requirements

Linux with Wayland or X11, plus:

| Package | Needed for |
|---|---|
| GStreamer with `gst-plugins-base`, `-good`, `-bad` and `gst-libav` | the built-in player |
| `mpv` | optional, playback in a separate window |
| Rust 1.95 or newer, a C compiler, `pkgconf`, GStreamer development files | building from source only |

Arch, CachyOS and derivatives:

```bash
sudo pacman -S --needed base-devel gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav mpv
```

Debian and Ubuntu:

```bash
sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav mpv
```

Building on Debian or Ubuntu also needs `build-essential pkg-config libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev`. Install Rust with [rustup](https://rustup.rs).

## Setup

On first launch, a short wizard walks you through Jellyfin, AniList and sync. You can skip any step and rerun the wizard later from *Settings → Run setup*.

### Jellyfin

Log in from the wizard, from *Settings*, or from the command line:

```bash
musubi login jellyfin
```

Add `--api-key` to log in with an API key instead. Your password is only used to log in and is never stored.

### AniList

Musubi does not ship a built-in AniList client ID, so you create your own once:

1. Create an app at <https://anilist.co/settings/developer>.
2. Set the redirect URL to `http://127.0.0.1:47823/callback`.
3. Enter the client ID in the wizard or log in from the command line. The client secret is not needed.

```bash
musubi login anilist
```

On a machine without a browser, e.g. over SSH, use `https://anilist.co/api/v2/oauth/pin` as the redirect URL and log in with:

```bash
musubi login anilist --pin
```

AniList tokens are valid for one year and cannot be refreshed. Musubi warns you 14 days before yours expires.

## Usage

Start the app with:

```bash
musubi
```

Sync without opening the window, first as a preview:

```bash
musubi sync --dry-run
```

```bash
musubi sync
```

### Keyboard shortcuts

| Keys | Action |
|---|---|
| `Ctrl+1` … `Ctrl+6` | Switch page |
| `Ctrl+R` | Refresh |
| `Space`, `←` / `→`, `F`, `M` | Player: pause, seek 10 s, fullscreen, mute |
| `Ctrl+P` | Play or pause |
| `Alt+←` / `Alt+→` | Seek 10 s |
| `Ctrl+.` / `Ctrl+,` | Next or previous episode |
| `Ctrl+J` / `Ctrl+K` | Cycle subtitles / audio track |
| `Ctrl+Shift+S` | Stop playback |
| `Esc` | Close a dialog, the detail page or the player |
| `F1` | Show all shortcuts |

In the mpv window, the usual mpv keys apply.

## Configuration

Everything is optional and can be changed in *Settings*. The file `~/.config/musubi/config.toml` is created the first time you save:

```toml
[jellyfin]
url = "https://jellyfin.example.org"
library_ids = []            # empty = all libraries
# max_bitrate = 8000000     # unset = the server decides

[anilist]
client_id = "12345"
login_flow = "loopback"     # or "pin"
loopback_port = 47823

[sync]
auto_sync = true
watched_threshold = 0.85    # an episode counts as watched at 85 %

[player]
backend = "embedded"        # or "mpv" for a separate window
mpv_path = "mpv"
hwdec = "auto-safe"         # empty = use your mpv.conf
extra_args = []
autoplay_next = true

[secrets]
backend = "file"            # or "secret-service" for KWallet / GNOME Keyring
```

| Path | Contents |
|---|---|
| `~/.config/musubi/config.toml` | settings |
| `~/.config/musubi/credentials.toml` | tokens, mode 0600 (refused if more permissive) |
| `~/.local/share/musubi/musubi.db` | matches, write queue, cache |
| `~/.cache/musubi/images/` | posters and backdrops |
| `~/.local/state/musubi/` | logs; set `MUSUBI_LOG=debug` for more detail |

## Development

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

The end-to-end tests run against mock servers and use real `mpv` and `ffmpeg` when installed.

To try the app without real servers, start the demo backend. It prints two URLs and accepts any password:

```bash
cargo run -p musubi-core --example demo_backend
```

Debug builds send AniList requests to `MUSUBI_DEV_ANILIST_ENDPOINT` if set; release builds ignore it.

Pushing a tag like `v0.1.0` builds the release archive and publishes it, using the matching section of [CHANGELOG.md](CHANGELOG.md) as release notes.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design, decisions and known limitations.

## License

[GPL-3.0-or-later](LICENSE)
