# Musubi

**Musubi** (結び, "knot, connection") is a Linux desktop app that streams anime from your Jellyfin server and keeps your AniList up to date.

- **Watch** – your Jellyfin library with a built-in player: hardware decoding, ASS subtitles, skip and episode controls.
- **Track** – episodes you finish are written to AniList automatically, even when you are offline for a while.
- **Match** – every Jellyfin season is linked to its own AniList entry; unsure matches wait for your confirmation.
- **Manage** – edit your AniList for anime and manga: status, progress, score, notes, custom lists, dates.

## Contents

- [Installation](#installation)
- [Getting started](#getting-started)
- [Usage](#usage)
- [Configuration](#configuration)
- [More](#more)

## Installation

### 1. Install GStreamer

The player needs GStreamer and its plugins.

Arch, CachyOS and derivatives:

```bash
sudo pacman -S --needed gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav
```

Debian and Ubuntu:

```bash
sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav
```

### 2. Install Musubi

Download `musubi-<version>-x86_64-linux.tar.gz` from the [releases page](https://github.com/Koxility/musubi/releases), then:

```bash
tar xzf musubi-*-x86_64-linux.tar.gz
```

```bash
cd musubi-*-x86_64-linux && ./install.sh
```

This puts `musubi` into `~/.local/bin` and adds it to your application launcher. The launcher works right away.

To also start `musubi` from a terminal, `~/.local/bin` has to be on your `PATH`. `install.sh` prints the command for your shell if it isn't. For fish (the default on CachyOS):

```bash
fish_add_path ~/.local/bin
```

To build from source instead, see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Getting started

Launch Musubi from your application menu or with `musubi`. A short setup wizard walks you through the steps below. You can skip any step and run it again later under *Settings → Run setup*.

### Jellyfin

Enter your server address, user name and password. The password is only used to log in and is never stored. An API key works too.

### AniList

Musubi does not ship a shared AniList client ID, so you create your own once:

1. Open <https://anilist.co/settings/developer> and create a new client.
2. Use `http://127.0.0.1:47823/callback` as the redirect URL.
3. Paste the client ID into Musubi and log in with your browser.

> **No browser on this machine?** Use `https://anilist.co/api/v2/oauth/pin` as the redirect URL instead and run `musubi login anilist --pin`.

AniList tokens are valid for one year. Musubi reminds you 14 days before yours expires.

## Usage

| Page | What it does |
|---|---|
| **Home** | Continue watching, next up, recently added |
| **Library** | Browse and filter your Jellyfin library; open a title to see its seasons and episodes |
| **AniList** | Your lists, airing schedule and quick `+1` progress |
| **Mapping** | Check and change which AniList entry each season belongs to |
| **Sync** | Compare Jellyfin and AniList and fix differences in either direction |
| **Settings** | Accounts, sync and playback options |

### Keyboard shortcuts

| Keys | Action |
|---|---|
| `Ctrl+1` … `Ctrl+7` | Switch page (Home, Library, AniList, Mapping, Sync, Settings, Profile) |
| `Ctrl+R` | Refresh |
| `Esc` | Close a dialog, the detail page or the player |
| `Space` / `Ctrl+P` | Play / pause |
| `←` / `→` | Seek 10 s (in the player) |
| `F` / `M` | Fullscreen / mute (in the player) |
| `Ctrl+.` / `Ctrl+,` | Next / previous episode |
| `Ctrl+J` / `Ctrl+K` | Cycle subtitles / audio track |
| `F1` | Show all shortcuts |

### Command line

| Command | Purpose |
|---|---|
| `musubi` | Start the app |
| `musubi sync --dry-run` | Show what a sync would change |
| `musubi sync` | Sync without opening the window |
| `musubi login jellyfin` / `musubi login anilist` | Log in (`--api-key`, `--pin` for the variants above) |
| `musubi logout jellyfin` / `musubi logout anilist` | Log out |

## Configuration

Everything can be changed under *Settings*. The settings are stored in `~/.config/musubi/config.toml`:

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
autoplay_next = true

[secrets]
backend = "file"            # or "secret-service" for KWallet / GNOME Keyring
```

<details>
<summary>Where Musubi keeps its files</summary>

| Path | Contents |
|---|---|
| `~/.config/musubi/config.toml` | Settings |
| `~/.config/musubi/credentials.toml` | Login tokens, readable only by you (mode 0600) |
| `~/.local/share/musubi/musubi.db` | Matches, pending AniList writes, cache |
| `~/.cache/musubi/images/` | Posters and backdrops |
| `~/.local/state/musubi/` | Logs; start with `MUSUBI_LOG=debug` for more detail |

</details>

## More

- [Changelog](CHANGELOG.md)
- [Architecture](docs/ARCHITECTURE.md): how Musubi works inside, design decisions and known limitations
- [Development](docs/DEVELOPMENT.md): building from source, tests, demo backend, releases

Musubi is licensed under the [GPL-3.0-or-later](LICENSE).
