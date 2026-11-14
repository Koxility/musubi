# Musubi

Linux desktop app: streams anime from Jellyfin and keeps AniList progress in sync. Rust workspace, Iced GUI.
Read `docs/ARCHITECTURE.md` before changing how crates fit together, and `docs/DEVELOPMENT.md` for building, the demo backend and releasing.

## Language

- Everything written in the repo is English: code comments, doc comments, UI text, docs, commit messages, PR titles and descriptions.
- Older code still has German comments. When you change code, translate the comments of the items you touch; leave untouched files alone.

## Architecture rules

- Crate dependencies point downwards only (diagram in `docs/ARCHITECTURE.md`). `musubi-domain` stays plain types without I/O.
- The GUI talks to `Core` and nothing else. A feature the GUI needs gets a typed method or `CoreEvent` in `musubi-core` first; the GUI never does HTTP, SQL or file I/O itself.
- In `musubi-gui`, each page owns its `State`, `Msg`, `update` and `view`; `app.rs` wires them. Styling comes from the tokens and styles in `theme/` and the components in `ui.rs`, never from literal colours or sizes. The neutral tokens, state colours, radius and type sizes mirror Kura's `tokens.css`; keep them in step. Red (`WATCH`) means Jellyfin/watching, teal (`TRACK`) means AniList/tracking; they mark things and never colour buttons or large areas. Blue (`INFO`) is for updates only.
- Icons are SVG strings in `icons.rs`; each SVG attribute may appear only once or resvg drops the icon.
- Keep the `view_epoch` nesting trick in `app.rs` and use `column`, not `keyed_column` (Iced 0.14 panics on page changes).

## Rust style

- Libraries return typed errors (`thiserror`); only the binary and the GUI use `anyhow`. Propagate with `?` and add context where the cause is not obvious.
- `unwrap`/`expect` only in tests, or with a message that states why it cannot fail.
- `unsafe` is forbidden workspace-wide.
- Prefer borrowing (`&str`, slices) in signatures; clone at the boundary where ownership is needed.
- Comments explain why, not what. Public items get a `///` line when the name alone does not tell the contract.
- Name things in the domain's words (`ListEntry`, `Mapping`, `ShelfItem`), not in the API's.
- Tests sit next to the code in `#[cfg(test)] mod tests`; core behaviour across services is tested in `crates/musubi-core/tests/e2e.rs` against mock servers.

## Before every commit

All three must pass:

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

For GUI changes, also look at the result: run the demo backend (`docs/DEVELOPMENT.md`, "Running without real servers").

## Commits and pull requests

- Commit subject: imperative, capitalised, no trailing period, up to ~60 characters ("Add the Profile page"). Body says what changed and why, wrapped at 72.
- One topic per commit. Keep the default branch `master` releasable; work on a branch and merge through a pull request.
- A user-visible change adds a line to the `[Unreleased]` section of `CHANGELOG.md`.
- PR descriptions end with their last line of content; leave out the "Generated with Claude Code" footer and its session link.
- Update `README.md` (shortcuts) and `docs/` in the same change when behaviour they describe changes.

## Releasing

Follow `docs/DEVELOPMENT.md`. Tag only after the version bump is merged, and fetch first so the tag lands on the real `master`:

    git fetch origin master && git tag -a vX.Y.Z -m "Musubi X.Y.Z" origin/master

## Agent skills

### Issue tracker

Issues and specs live in GitHub Issues, accessed with the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context: one `CONTEXT.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
