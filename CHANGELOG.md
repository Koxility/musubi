# Changelog

All notable changes to Musubi are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.4.0] - 2026-11-11

### Added

- **Several AniList entries per season, by hand.** The change dialog for a season lists each of its entries with its own *Edit…* and has *Add entry…* for another one. A title's page shows every entry of the selected season, not only the first.
- **Collapsible sidebar.** The menu button next to the logo, or `Ctrl+B`, hides the navigation and gives the page the full width; the button stays in the corner to bring it back. Musubi remembers the choice.
- **Jump from AniList to the show.** Clicking a cover on the AniList overview (Airing and in progress) opens the matched title from your Jellyfin library, on the season the entry belongs to. Editing the entry moved to the ⋯ button in the top-left corner of the cover; titles that are not in your library still open the editor.

### Changed

- **Home feels more fluid.** The rows scroll by dragging the cards with the mouse, and a flick carries them on; the arrows glide instead of jumping. Covers fade in as they load and pages fade in when you switch.
- **Smarter matching of long and split series.** One AniList entry now continues across as many Jellyfin seasons as it has episodes for, even when the metadata plugin wrote other IDs on those seasons, so a classic run split into many seasons maps to its single entry and the sequels follow after it. A season split over several AniList entries skips an episode 0 that Jellyfin keeps under Specials instead of counting every episode one too low. Automatic matches of multi-season series are re-planned once on the next start; manual ones stay.

## [0.3.0] - 2026-11-11

### Added

- **View options** in Library and AniList: grid or list, each in three sizes, switched at the top right and remembered per page. The Library gains a list view with year, genres, AniList status and episodes left.
- **One-click AniList login.** Musubi ships with its own AniList client, so *Log in with AniList* is all it takes: approve Musubi in the browser and AniList issues your personal token. A waiting login can be cancelled. Your own API client and a pasted token remain available under the advanced options.
- **Add to AniList from a title's page.** A matched title that is not on your lists yet shows a list picker and *Add to AniList* right in its AniList strip.

### Changed

- **English titles.** AniList titles show in English where AniList has one, otherwise in Romaji, then in the native script. Titles stored with existing matches are updated once on the next start.
- One logo everywhere: the sidebar, the About line and the window icon use the same mark as the launcher entry.
- **Home is larger.** The hero for what to continue follows the window and shows the whole backdrop instead of a cropped strip; posters, cards and text in the rows are bigger. Rows also scroll by dragging the thin bar below the cards.
- **Airing on the AniList overview** uses the same cover cards as "Anime in progress", with the countdown next to the episode count.
- The `+1` button is a solid, labelled pill that is always visible in the top-right corner of a cover and in list rows.

### Fixed

- The Profile score distribution showed every score in the column for 10 and nothing else for users who score out of 10, 5 or 3. AniList reports these scores on the user's own scale; they are now converted.
- Backdrops no longer spill over the content below them with the software renderer.

## [0.2.0] - 2026-11-11

### Added

- **Profile page** (`Ctrl+7`) with your AniList statistics for anime and manga: totals, time watched, mean score, the split by status, score and release-year charts, and genre and format rankings. Picking a status opens that list for editing. The last copy stays available offline.
- The Settings page shows the app version and git commit at the bottom.
- Scroll arrows on the Home shelves.

### Changed

- **New look.** A design system with bundled fonts (Sora and Hanken Grotesk) replaces the default styling. Red marks watching on Jellyfin, teal marks tracking on AniList.
- The sidebar groups pages into Watch, Track and Connect and shows who is logged in.
- Home has a larger hero for what to continue; the library has library tabs, poster marks for watched and matched titles, and a detail page with an AniList strip.
- The AniList overview shows key numbers and an airing schedule; lists are tables grouped by status with counts.
- New app icon: two cords tied into a knot.
- The window now reports the application ID `io.github.musubi.Musubi`, so desktops group it with its launcher entry.

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

[0.4.0]: https://github.com/Koxility/musubi/releases/tag/v0.4.0
[0.3.0]: https://github.com/Koxility/musubi/releases/tag/v0.3.0
[0.2.0]: https://github.com/Koxility/musubi/releases/tag/v0.2.0
[0.1.1]: https://github.com/Koxility/musubi/releases/tag/v0.1.1
[0.1.0]: https://github.com/Koxility/musubi/releases/tag/v0.1.0
