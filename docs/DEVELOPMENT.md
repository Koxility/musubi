# Development

## Building from source

You need Rust 1.95 or newer ([rustup](https://rustup.rs)), a C compiler, `pkgconf` and the GStreamer development files. SQLite is compiled in.

Arch, CachyOS and derivatives:

```bash
sudo pacman -S --needed base-devel gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav
```

Debian and Ubuntu:

```bash
sudo apt install build-essential pkg-config libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav
```

Build and install into `~/.local`, with the launcher entry and icon. `--prefix` picks another place, `--uninstall` removes it again:

```bash
cargo build --release --locked -p musubi && packaging/install.sh
```

For a quick binary-only install into `~/.cargo/bin`, `cargo install --locked --path crates/musubi` works too. The launcher entry then has to be added by hand from `packaging/musubi.desktop` and `packaging/musubi.svg`, and starts `musubi` from your `PATH`.

The bottom of the sidebar and of the Settings page show the version and git commit of the running build, e.g. `v0.1.0 · 218b921`. `crates/musubi-gui/build.rs` embeds the commit. If the commit doesn't match your checkout, your launcher starts an older binary.

## Checks

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo fmt --all --check
```

The end-to-end tests in `crates/musubi-core/tests/e2e.rs` run against mock servers for Jellyfin and AniList and a scripted player, so they need no network and no video files.

## Running without real servers

The demo backend starts mock Jellyfin and AniList servers and prints their URLs. Jellyfin accepts any user name and password. An optional video file gets streamed for every episode:

```bash
cargo run -p musubi-core --example demo_backend -- path/to/video.mkv
```

Debug builds send AniList requests to `MUSUBI_DEV_ANILIST_ENDPOINT` when it is set; point it at the printed AniList URL. Release builds ignore the variable.

The demo serves generated artwork, anime and manga lists and profile statistics, so every page has content. With `--login` it also logs the profile under the usual XDG paths in to both mock servers. Point `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` and `XDG_STATE_HOME` at a scratch directory for both the demo and the app to try the GUI without touching your real profile:

```bash
cargo run -p musubi-core --example demo_backend -- --login
```

## Code layout

The workspace is split into crates whose dependencies only point downwards. The GUI talks to the core and nothing else. See [ARCHITECTURE.md](ARCHITECTURE.md) for the crate diagram, the matching and sync rules, and known limitations.

Everything is written in English: code comments, UI, docs and commit messages. Older code still has German comments; translate them when you change that code.

## Releasing

1. Set the version in `Cargo.toml` (`[workspace.package]`) and in both PKGBUILDs under `packaging/aur/`, and run `cargo build` once, so `Cargo.lock` picks up the new version. The release workflow builds with `--locked` and fails otherwise.
2. Add a `## [x.y.z] - YYYY-MM-DD` section to `CHANGELOG.md`.
3. Merge to `master`, then fetch it, tag and push. Without the fetch, `origin/master` can be stale and the tag lands on an older commit; the release then fails because the version doesn't match.

```bash
git fetch origin master
```

```bash
git tag -a vx.y.z -m "Musubi x.y.z" origin/master
```

```bash
git push origin vx.y.z
```

The tag starts `.github/workflows/release.yml`. It runs clippy and the tests, then builds the release binary. Next, `packaging/package.sh` packs it with `install.sh`, the launcher entry, the icon, README, CHANGELOG and the licences. Finally, the workflow publishes a GitHub release with the CHANGELOG section as its notes. The archive is attached twice: as `musubi-x.y.z-x86_64-linux.tar.gz`, and as `musubi-x86_64-linux.tar.gz` for the version-less `releases/latest/download/` link in the README.

`packaging/package.sh` refuses to run when the tag doesn't match `Cargo.toml` or the CHANGELOG has no section for the version. You can try it locally after `cargo build --release --locked -p musubi`:

```bash
packaging/package.sh x.y.z
```

### Arch packages

`packaging/aur/musubi` builds from the GitHub source tarball of the tag, `packaging/aur/musubi-bin` repackages the release archive. Bump `pkgver` in both together with `Cargo.toml` and reset `pkgrel` to 1. Their checksums can only be filled in once the release exists, so the tagged commit carries `sha256sums=('SKIP')`. After the workflow has published the release, run on an Arch machine (needs `pacman-contrib`):

```bash
packaging/aur/update.sh
```

It fills in the checksums and regenerates `.SRCINFO`. Build both once with `makepkg -si` in a clean chroot or at least a clean directory, commit the result to `master`, then copy `PKGBUILD` and `.SRCINFO` of each into its AUR repository and push:

```bash
git clone ssh://aur@aur.archlinux.org/musubi.git aur-musubi
```

```bash
cp packaging/aur/musubi/{PKGBUILD,.SRCINFO} aur-musubi/ && cd aur-musubi && git add PKGBUILD .SRCINFO && git commit -m "Update to x.y.z" && git push
```

The same for `musubi-bin`. A fix to the packaging alone, without a new release, raises `pkgrel` instead of `pkgver`.
