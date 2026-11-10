# Changelog

All notable changes to Musubi are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [0.1.1] - 2026-11-10

### Fixed

- The application launcher entry from `install.sh` now points at the installed binary, so it also works when `~/.local/bin` is not on the desktop session's `PATH`.
- `install.sh` prints the exact command to add `~/.local/bin` to the `PATH` for fish, bash and zsh.

## [0.1.0] - 2026-11-10

First release.

### Highlights

- **Streaming from Jellyfin.**
  - Log in with a password or an API key.
  - Browse and filter the library by genre, status, year and type.
  - Episode lists with metadata.
  - Direct streaming with transcoding as a fallback; progress is reported back to the server.
- **Built-in player.**
  - Plays inside the app window via GStreamer, with hardware decoding and ASS subtitles.
  - Controls in the style of common streaming apps: icon buttons, skip ±10 s, previous and next episode, volume and mute, subtitle and audio track switching, fullscreen.
- **AniList management.**
  - Add, edit and delete entries for anime and manga: status, progress, score in your format, advanced scoring, notes, custom lists, dates and privacy.
  - Works without Jellyfin as well.
- **Per-season matching.**
  - Every Jellyfin season gets its own AniList entry, found via provider IDs, sequels (also across movies and OVAs) and title search.
  - Recaps and bonus episodes do not shift later seasons.
  - Unsure matches are marked and only sync after you confirm them.
  - Any season can be changed on its own.
- **Sync.**
  - Progress is written to AniList automatically while you watch, or manually with a preview.
  - Writes are queued in a local outbox and survive being offline.
- **Home screen** with a hero banner and shelves for *Continue Watching*, *Next Up* and *Recently Added*.
- **Setup wizard** for Jellyfin, AniList and sync. There is also a command line for `login`, `logout` and `sync`.

### Notes

- Linux only. Release archives are built for x86_64.
- Playback needs GStreamer with `gst-plugins-base`, `-good`, `-bad` and `gst-libav`.
- You need your own AniList client ID; see the README.

[0.1.1]: https://github.com/Koxility/musubi/releases/tag/v0.1.1
[0.1.0]: https://github.com/Koxility/musubi/releases/tag/v0.1.0
