<div align="center">

<img src="packaging/musubi.svg" width="104" alt="Musubi logo: a red and a teal cord tied into a knot">

# Musubi

**Watch anime from your Jellyfin server. Your AniList keeps itself up to date.**

[![Latest release](https://img.shields.io/github/v/release/Koxility/musubi?label=release&color=3b82f6)](https://github.com/Koxility/musubi/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)
![Platform: Linux x86_64](https://img.shields.io/badge/platform-Linux%20x86__green)
![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-green)

[Install](#installation) · [Getting started](#getting-started) · [Using Musubi](#using-musubi) · [Configuration](#configuration) · [Documentation](#documentation)

</div>

Musubi (結び, "knot, connection") is a desktop app for Linux that ties two things together: the anime on your [Jellyfin](https://jellyfin.org) server and your lists on [AniList](https://anilist.co). You watch in the built-in player, and every episode you finish is counted on AniList without you doing anything.

## Features

**Watch** — everything around Jellyfin, marked red in the app.

- A home screen with what to continue, what is next up and what was added recently.
- Your library as a grid or a list, with search, filters for genre, year, status and type, and a page per title with seasons and episodes.
- A built-in player inside the window: resume, next and previous episode, autoplay, audio and subtitle track switching, styled ASS subtitles.

**Track** — everything around AniList, marked teal.

- Finished episodes update your AniList automatically. Changes made offline are queued and sent later.
- Airing titles with a countdown to the next episode, and a `+1` button on everything in progress.
- Full list editing for anime and manga: status, progress, score, notes, dates, rewatches and custom lists.
- A profile page with your statistics: time watched, scores, genres, formats and release years.

**Connect** — the knot between the two.

- Every Jellyfin season is matched to its own AniList entry. Matches Musubi is unsure about wait for your confirmation and never write to AniList on their own.
- A sync page that compares both sides and fixes differences in either direction.
- A title that is matched but not on your lists yet can be added from its page with one click.

## Requirements

- Linux on x86_64
- A Jellyfin server you can log in to
- An AniList account
- GStreamer with its plugin sets, for playback

## Installation

### 1. Install GStreamer

Arch, CachyOS and derivatives:

```bash
sudo pacman -S --needed gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav
```

Debian and Ubuntu:

```bash
sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav
```

### 2. Install Musubi

Download `musubi-<version>-x86_64-linux.tar.gz` from the [latest release](https://github.com/Koxility/musubi/releases/latest), then unpack and install it:

```bash
tar xzf musubi-*-x86_64-linux.tar.gz
```

```bash
cd musubi-*-x86_64-linux && ./install.sh
```

This puts `musubi` into `~/.local/bin` and adds Musubi to your application launcher. To install somewhere else, set `PREFIX`, for example `PREFIX=/usr/local sudo ./install.sh`.

To start `musubi` from a terminal as well, `~/.local/bin` has to be on your `PATH`. `install.sh` prints the command for your shell if it is not.

<details>
<summary>Building from source</summary>

You need Rust 1.95 or newer, a C compiler, `pkgconf` and the GStreamer development files. [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) lists the packages per distribution.

```bash
git clone https://github.com/Koxility/musubi.git
```

```bash
cd musubi && cargo install --locked --path crates/musubi
```

</details>

## Getting started

Start Musubi from your application launcher or with `musubi`. A short setup wizard walks you through three steps. You can skip any of them and run the wizard again under *Settings → Run setup again*.

1. **Jellyfin.** Enter your server address, user name and password. The password is only used for this login and is never stored. An API key works too.
2. **AniList.** Click *Log in with AniList*. Your browser opens AniList; approve Musubi there and you are logged in. AniList issues a token that is personal to your account, and there is nothing to copy or paste.
3. **Sync.** Choose whether finished episodes update AniList automatically.

Musubi then matches your library to AniList in the background. Matches it is unsure about show up on the *Mapping* page for you to confirm or change.

<details>
<summary>Using your own AniList API client, or logging in without a browser</summary>

Musubi logs in through its own AniList API client. Under *Use my own API client or a token* you can enter the client ID of yours instead:

1. Open <https://anilist.co/settings/developer> and create a new client.
2. Use `http://127.0.0.1:47823/callback` as the redirect URL.
3. Paste the client ID into Musubi and log in with your browser.

On a machine without a browser, run `musubi login anilist --pin`. It prints a link to open on another device and takes the token, or the address of the page you land on, in return.

</details>

AniList tokens are valid for one year. Musubi reminds you 14 days before yours expires.

## Using Musubi

| Page | What it is for |
|---|---|
| **Home** | The episode to continue, then rows for continue watching, next up and recently added |
| **Library** | Browse, search and filter your Jellyfin library; open a title for its seasons, episodes and AniList entry |
| **AniList** | Airing titles, everything in progress, and your anime and manga lists with editing and `+1` |
| **Profile** | Your AniList statistics for anime and manga |
| **Mapping** | Check and change which AniList entry each season belongs to |
| **Sync** | Compare Jellyfin and AniList and apply differences in either direction |
| **Settings** | Accounts, sync and playback options |

Library and AniList have a view switch at the top right: grid or list, each in three sizes. Musubi remembers the choice per page. The rows on Home scroll sideways by dragging the cards with the mouse, with a touchpad swipe, `Shift` + mouse wheel, the arrows in the row header or the thin bar below the cards.

### Keyboard shortcuts

| Keys | Action |
|---|---|
| `Ctrl+1` … `Ctrl+7` | Switch page: Home, Library, AniList, Mapping, Sync, Settings, Profile |
| `Ctrl+R` | Refresh |
| `Ctrl+B` | Show or hide the sidebar |
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
| `musubi login jellyfin` | Log in to Jellyfin; `--api-key` uses an API key instead of the password |
| `musubi login anilist` | Log in to AniList; `--pin` takes a pasted token instead of the browser redirect |
| `musubi logout jellyfin` / `musubi logout anilist` | Log out |

## Configuration

Everything can be changed in the app. The settings are stored in `~/.config/musubi/config.toml`:

```toml
[jellyfin]
url = "https://jellyfin.example.org"
library_ids = []            # empty = all libraries
# max_bitrate = 8000000     # unset = the server decides

[anilist]
# client_id = "12345"       # only for your own AniList API client
login_flow = "loopback"     # or "pin", for `musubi login anilist`
loopback_port = 47823       # only used with your own client

[sync]
auto_sync = true
watched_threshold = 0.85    # an episode counts as watched at 85 %

[player]
autoplay_next = true

[secrets]
backend = "file"            # or "secret-service" for KWallet / GNOME Keyring

[ui.library]
mode = "grid"               # or "list"
size = "medium"             # "small", "medium" or "large"

[ui.anilist]
mode = "list"
size = "medium"
```

<details>
<summary>Where Musubi keeps its files</summary>

| Path | Contents |
|---|---|
| `~/.config/musubi/config.toml` | Settings |
| `~/.config/musubi/credentials.toml` | Login tokens, readable only by you (mode 0600) |
| `~/.local/share/musubi/musubi.db` | Matches, pending AniList writes, cache |
| `~/.cache/musubi/images/` | Posters and backdrops |
| `~/.local/state/musubi/` | Logs |

</details>

## Troubleshooting

- **"GStreamer is not available" at startup, or a title does not play.** Install the plugin sets from [step 1](#1-install-gstreamer). Everything except playback works without them.
- **The launcher starts an old version.** The bottom of the sidebar shows the version and git commit of the running build. If it is not the one you installed, the launcher entry points at another binary.
- **AniList changes do not arrive.** They are queued while AniList is unreachable or rate-limited and sent later. The *Sync* page lists what is waiting.
- **Something else.** Start Musubi with `MUSUBI_LOG=debug musubi` and look at the logs in `~/.local/state/musubi/`, then [open an issue](https://github.com/Koxility/musubi/issues).

Sync from AniList to Jellyfin is manual, specials (season 0) are matched by hand, and the interface is English only. The full list of known limitations is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#known-limitations).

## Documentation

- [Changelog](CHANGELOG.md): what changed in each release
- [Architecture](docs/ARCHITECTURE.md): how Musubi works inside, the matching and sync rules, design decisions and known limitations
- [Development](docs/DEVELOPMENT.md): building from source, the checks, the demo backend and the release process

## Contributing

Bug reports and ideas are welcome in the [issue tracker](https://github.com/Koxility/musubi/issues). For code changes, [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) explains how to build Musubi and how to run it against the demo backend, which needs neither a Jellyfin server nor an AniList account. These three have to pass before a pull request:

```bash
cargo fmt --all --check
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

## License

Musubi is free software, licensed under the [GPL-3.0-or-later](LICENSE).

## Legal disclaimer

Musubi is a client-side tool. It plays media from a Jellyfin server that you configure and have access to, and it updates the AniList account that you log in to. It does not host, provide, index or distribute any media, and it does not connect to any content source other than the services you set up yourself.

You are responsible for how you use it, for the content on the server you connect to, and for following the laws and terms of service that apply where you live. The project is provided "as is", without warranty of any kind. Its maintainers are not responsible for third-party content, external links, or the availability, accuracy, legality or reliability of outside services.

Musubi is an independent project. It is not affiliated with, endorsed by or sponsored by Jellyfin or AniList; their names and trademarks belong to their respective owners. Questions about content hosted by another service should be directed to that service.
