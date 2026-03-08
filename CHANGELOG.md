# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog and this project follows SemVer.

## [Unreleased]

### Added

- Dedicated AI disclosure docs: `AI_TRANSPARENCY.MD` and `RELEASE_NOTES_v1.0.0-rc1.MD`.
- Local install helper script (`scripts/install_local.sh`) so the app can be launched directly as `navtui`.
- GitHub tag-triggered release workflow (`.github/workflows/release.yml`) that builds and publishes prebuilt Linux/Windows archives.
- Full keybinding customization via `config.toml` `[keybinds]`.
- Config option `show_identity_label` to toggle `username@server-host` display.
- Queue-pane identity label placement at bottom-left.
- Queue Focus `Backspace` delete action.
- Queue reorder cancel behavior on `Esc` (restores pre-move order).
- `[`/`]` key-hold repeat behavior for continuous volume adjustments.
- Release checklist and packaging script scaffolding.
- Windows `Space` pause/resume parity via ffplay control input fallback.
- `n` play-next action for selected browser item and selected queue item.
- Timeline scrubber line in Now Playing with mouse seek support and seek hotkeys `;`/`'` (`-10s`/`+10s`).
- MIT `LICENSE` file.
- App-level regression tests for queue wrap/reorder cancel, search-mode hotkey/backspace behavior, and track-end retry/advance decision logic.

### Changed

- Project identity renamed to `navtui`:
  - package/binary name now `navtui`
  - config/cache dirs now `navtui` with fallback to legacy `subsonic-tui` paths
  - keyring service now `navtui` with fallback to legacy `subsonic-tui` entries
  - release archive naming now `linux_navtui_*` / `windows_navtui_*`
  - new env flags `NAVTUI_RESOLVE_IP` and `NAVTUI_FAST_START` with legacy alias support
- Live volume synchronization now tracks confirmed backend-applied level.
- Linux live-volume detection aligned to `pactl` sink-input control path.
- Volume behavior is now consistent across actions/track changes and honors the user-set level.
- Hard refresh now uses `r` by default (instead of `Shift+R`) and clears disk cache before reloading.
- Documentation expanded for keybind override syntax and defaults.
- Live search and library sort paths now avoid repeated case-folding work in inner loops.
- Release checklist now includes `cargo build --release --locked` and updated hard-refresh smoke key (`r`).
- Release profile tuned for slimmer/faster production binaries (`thin` LTO, single codegen unit, stripped symbols, aborting panics).
- CI now validates a Linux release build (`cargo build --release --locked`) in addition to check/test gates.
- README and release checklist now treat `navtui` as the primary run command (`cargo run` as development fallback).
- Default global reset key changed from `Shift+Esc` to `Ctrl+Esc`.
- README now documents no-Rust usage via prebuilt GitHub Release binaries.
- Release workflow now installs MinGW on GitHub runners so Windows release artifacts can be built and attached.
- Release workflow now supports manual `workflow_dispatch` with explicit tag input for recovery when tag-trigger runs are skipped.
- CI/release workflows now install `pkg-config` and `libdbus-1-dev` on Ubuntu runners so `libdbus-sys` builds reliably.

## [0.1.0] - 2026-03-04

### Added

- Initial Rust TUI player for Subsonic/Navidrome (`ratatui` + `crossterm`).
- Artists/Albums/Songs browsing with drill-down navigation.
- Search, queue, shuffle, and playback controls.
- `ffplay` playback engine with compatibility fallback.
- Keyring-backed password storage (no plaintext secrets in config).
- DNS fallback/cache and library snapshot cache.
