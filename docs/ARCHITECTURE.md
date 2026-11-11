# Architecture

## Crates

```
                 musubi (binary: CLI, logging, startup)
                              │
                         musubi-gui
                     (Iced, Elm architecture)
                              │
                         musubi-core
        facade · matching · sync rules · outbox · playback
          │             │              │             │
   musubi-jellyfin  musubi-anilist  musubi-player  musubi-store
    REST client     GraphQL, OAuth,  PlayerBackend  config, secrets,
                    rate limiter     commands,      SQLite
                                     events
          └─────────────┴──────┬───────┴─────────────┘
                         musubi-domain
                     plain types, no I/O
```

Dependencies only point downwards, and the crate boundaries let the compiler enforce that. The GUI depends only on the core and knows nothing about HTTP. The workspace sets `unsafe_code = "forbid"`.

### Core–GUI contract

`Core` is cheap to clone and offers typed async methods. `subscribe()` streams events such as playback progress, changed lists or rate limits. The GUI talks to nothing else, so every feature is implemented once, in the core.

## Technology choices

### GUI: Iced

The GUI is pure Rust without system toolkits like GTK and looks the same on every desktop.

- Each page owns its state, messages and view, and `app.rs` wires them to the core.
- The design system lives in `theme/`: `tokens.rs` names every colour, spacing step, radius and font size, `fonts.rs` embeds Sora (titles) and Hanken Grotesk (text), and `styles.rs` builds all widget styles from those tokens. Pages use the tokens and the components in `ui.rs` instead of literals.
- Two accent colours carry meaning. Red (`WATCH`) marks watching in Jellyfin: play, resume, playback progress. Teal (`TRACK`) marks tracking on AniList: status, list progress, sync. The knot mark shows where a title is connected to both.
- The logo has one source, `packaging/musubi.svg`. `logo.rs` draws it in the sidebar and the About line and renders it with resvg for the window icon; the launcher entry installs the same file.
- Icons are hand-made SVGs in `icons.rs`, rendered by resvg. A test parses every icon with usvg so a broken one cannot silently disappear.
- Library and AniList offer a grid and a list in three sizes. The choice is stored per page in `config.toml` (`[ui.library]`, `[ui.anilist]`) through `Core::update_config`.
- Posters load only when they scroll into view. The cache files are named `*.img`, so the GUI reads them as bytes; when given a path, Iced picks the decoder by file extension.
- Iced has no virtualized grids, so the library builds widgets for every title.
- Iced keeps widget state such as scroll position as long as the same widget type sits at the same place in the tree. On every view change (page switch, detail page opened or closed), the nesting depth of the page alternates so that the new view starts fresh. `keyed_column` is not an option here: in Iced 0.14 it diffs equally long child lists without a type check and panics when the page type changes.
- Motion runs on one frame clock: `Message::Frame` from `window::frames()`, subscribed only while something moves (`App::animating`: the sidebar, a gliding shelf, a cover or a page fading in). Every animated value is read at `App::frame`. Whoever starts a motion sets `frame` to now. The page fade is a veil over the page that is always in the tree; inserting it only while fading would reset the page's widget state.
- Shelves on Home are dragged through raw pointer events (`home::subscription`), because the cards are buttons and capture the press before a wrapping widget sees it. A press that became a drag suppresses the click that ends it.

### Player: GStreamer inside the window

Videos play through `iced_video_player` on GStreamer.

- The picture renders natively inside the Iced window, the controls sit on top, and the project needs no `unsafe` code of its own. `assrender` draws subtitles into the picture.
- `musubi-player` defines the `PlayerBackend` trait with commands and events. The GUI's `EmbeddedPlayer` implements it, so progress tracking and sync stay in the core and are tested there with a scripted player.
- Streams open off the UI thread; afterwards the GStreamer objects live on the UI thread.
- If GStreamer or a required plugin is missing, the app starts anyway. It shows a notice, and playing a title fails with a hint to install the plugins. Everything else, including AniList management, keeps working.

### Persistence: SQLite on its own thread

Matches and the write queue must survive crashes. rusqlite is synchronous, so the connection lives on a dedicated thread and async code sends it jobs.

### GraphQL: hand-written queries

About a dozen operations do not justify a code generation step with a checked-in schema.

### Build info

`crates/musubi-gui/build.rs` embeds the short git commit. The sidebar and the bottom of the Settings page show it next to the version, which makes it obvious whether a launcher starts an outdated binary.

## Flows

### Playback

1. `PlaybackInfo` returns a direct stream or a transcoding URL. The device profile allows everything, because GStreamer with `gst-libav` plays almost any format.
2. The Jellyfin token travels as an HTTP header set on the GStreamer source, never in the stream URL.
3. Progress goes to Jellyfin every 10 s and on pause and seek. The next episode is queued while the current one plays, so next and previous work.
4. Once the watched threshold (85 % by default) is reached, or at the end of the file, the sync rules apply.

### Matching

AniList usually has one entry per season, often per cour, while Jellyfin counts per season. So every Jellyfin season is matched on its own, in this order:

1. **Manual match.** A manual match for the season always wins and serves as the starting point for the next season.
2. **A long entry goes on.** If the previous season's entry still has room for this whole season (one 366-episode AniList entry behind sixteen Jellyfin seasons), the season continues it, whatever ID the season carries.
3. **Season provider IDs.** The season's own `ProviderIds`: AniList directly, or MyAnimeList via `Media(idMal:)`. An ID already used by an earlier season, or written on more than one season, is ignored: plugins copy IDs around, and an ID names one entry only.
4. **First season.** The series' `ProviderIds`, otherwise a title search.
   - Titles are normalized and compared with Jaro–Winkler across all titles and synonyms.
   - Year, format and episode count add a bonus or a penalty.
   - A match is accepted automatically from 0.92 with a margin of at least 0.05 over the runner-up.
5. **Later seasons.** The rest of the previous season's work if it spans both seasons, otherwise the next `SEQUEL`. Movies, OVAs and specials in between are skipped, but their sequels are followed.
6. **Fallback.** A search for the season itself, e.g. "Title Season 2" or "Title <season name>". Candidates without a season number, or with the wrong one, get a penalty.
7. **Guesses.** Otherwise the best candidate above a minimum score becomes an unsure match (`Guess`). If nothing qualifies, only that season stays unmatched.

"Unsure" applies per AniList entry. A guessed season does not write to AniList on its own and is proposed as `Skip` in the sync. The other seasons sync normally.

`plan_season` anchors every season afresh:

- A season spanning several cours gets several entries.
- A work spanning several seasons is continued.
- Absolute episode numbering is handled through the offset.
- If the count differs by at most two episodes (a recap or bonus episode), the next season still starts at episode 1 of its own entry instead of drifting.
- If a season split over several entries has one or two episodes fewer than they hold, the library has that many specials and a later season follows, the surplus is skipped at the start of the first entry (`leading_surplus`): AniList counts an episode 0 that Jellyfin keeps under Specials. The newest season is left alone, because it is usually just missing its latest episodes. This is a heuristic; the exact editor overrides it.
- A season's length is its highest episode number minus its first plus one, not the number of files. An entry that spans several seasons is counted on by these lengths (season 14 episode 1 of a show whose first thirteen seasons hold 265 episodes is episode 266), so an episode missing inside a season does not shift the seasons after it. Episodes missing at the end of a season cannot be seen and do.

Every match covers an episode range and carries an offset.

Matching runs at startup, after login and on *Refresh* for every title not tried before. When the matching rules change (`matching_version`, currently 4), automatic matches of multi-season series are re-planned once, and titles left without a match are searched again once. The first season keeps its entry, and manual seasons are never overwritten. The change dialog matches a single season or all seasons at once, and *Match automatically* hands a title back to the automatic rules, replacing its manual matches.

### Sync rules

`sync::decide_watched` is a pure function with table-driven tests.

| AniList status | Watched episode | Result |
|---|---|---|
| no entry, `PLANNING` | any | `CURRENT` with `startedAt` |
| no entry, `PLANNING`, `CURRENT` | last | `COMPLETED` with `completedAt` |
| `CURRENT` | above progress | progress increases |
| `CURRENT`, `REPEATING` | not above | nothing |
| `REPEATING` | last | `COMPLETED`, `repeat` + 1 |
| `PAUSED`, `DROPPED` | any | nothing, reported as a conflict |
| `COMPLETED` | any | nothing |

Automatic sync only goes from Jellyfin to AniList, so no loops can occur. The manual sync shows a direction per difference that you can change.

### Outbox

Every AniList write first lands in the SQLite table `outbox`, and the UI updates immediately.

- There is one job per entry, and newer changes are merged into it.
- A revision number keeps a job that is being sent from deleting a change that arrived meanwhile.
- Temporary errors are retried with backoff, up to one hour apart.
- Permanent errors stay visible as failed and can be retried.

The rate limiter starts at the measured limit of 30 requests per minute, adopts `X-RateLimit-Limit` from every response and honors `Retry-After`.

## Security

- **Credentials.** By default they live in `credentials.toml` with mode 0600 in a 0700 directory, written atomically. More permissive modes abort startup. Secret Service can be chosen in the config.
- **Passwords and tokens.** Passwords are never stored. Tokens are held as `SecretString` and never appear in logs.
- **AniList login.** It uses the implicit grant, so no client secret is needed. The client ID comes from the user's config or, failing that, is the one of Musubi's own AniList client (`oauth::BUILTIN_CLIENT_ID`); either way the token AniList issues is personal to the user who approved. The loopback server listens on 127.0.0.1 only, checks the Host header, and uses a random path per login. It accepts the token only after verifying it with `Viewer`.

## Known limitations

### Playback
- Of several external subtitle files, only the first is loaded. Embedded tracks are not affected.
- A subtitle that starts at exactly 0 s may be missing at startup.
- The server decides whether to transcode. There are no custom profiles, only `max_bitrate`.

### Sync and AniList
- There are no offline downloads. Without a server, only AniList management works, from the cache.
- AniList to Jellyfin sync is manual only.
- AniList tokens must be renewed yearly.
- AniDB and TVDB provider IDs are not used, because AniList offers no lookup for them.
- Specials (season 0) are matched manually only.
- With the current limit of 30 requests per minute, the first matching run of a large library takes a while: each title needs one or more requests.
- Renaming a custom list is not atomic on AniList. If it is interrupted, both lists exist.

### Platform
- Custom CA certificates are taken from the system store only.
- The interface is English only.
- Release builds are x86_64 Linux only.

## Possible extensions

- Quick Connect as a third Jellyfin login method.
- Offline mode with downloads.
- More trackers such as MyAnimeList or Kitsu behind a `Tracker` trait.
- Notifications for new episodes from AniList airing data.
