# navtui Plan

Last updated: 2026-03-08

## Goal

Maintain a fast, minimal Subsonic/Navidrome TUI player with stable keyboard-driven browsing and playback.

## Current Status

Completed:

- [x] Rust implementation with `ratatui` + `crossterm`
- [x] First-run auth prompts (server URL, username, password)
- [x] Password storage in keyring (no plaintext password in config)
- [x] Artists/Albums/Songs tabs with drill-down navigation
- [x] Context-aware shuffle (`s`)
- [x] Queue controls (`a/A`, `c`, `,`, `.`)
- [x] Live case-insensitive search (`/`) with cancel/tab-switch behavior
- [x] Wrap-around cursor navigation (`Up`/`Down`)
- [x] DNS fallback (override env var + DoH + DNS cache)
- [x] Library snapshot cache for faster startup
- [x] Playback compatibility fallback to reduce queue-skip failures
- [x] Live no-restart volume control on Linux (slider + `[`/`]`) via `pactl`
- [x] Mouse-wheel queue viewport scrolling with auto-follow on queue index changes
- [x] Queue Focus mode toggle (`Shift`/`Shift+Tab`) with queue cursor nav (`Up/Down`), `Enter` to play, and `Space` pick/drop reordering
- [x] Mouse-wheel scroll on main browser pane mapped to tab selection navigation
- [x] Responsive vertical pane sizing that keeps top pane compact and gives extra height to Queue on tall windows
- [x] Windows build port (experimental)
- [x] Windows pause/resume parity for `Space` via ffplay control input
- [x] Config toggle for Queue-pane identity label (`show_identity_label`)
- [x] Fully configurable keyboard bindings via `config.toml` `[keybinds]`
- [x] `n` play-next action for browser selection and queue selection
- [x] Now Playing timeline scrubber + `;`/`'` seek hotkeys (`-10s` / `+10s`) + mouse scrub seek
- [x] Hard refresh key switched to `r` and now clears disk cache before reload
- [x] Release checklist + changelog scaffolding
- [x] Versioned packaging script with checksum output (`scripts/package_release.sh`)
- [x] Basic CI workflow for format/check/test (Linux + Windows target check)
- [x] App-level regression tests for queue wrap/reorder cancel, search-mode key handling, and track-end decision logic
- [x] Explicit project license file (`LICENSE`)

In scope next:

- [ ] Expand automated tests for queue advancement and playback retry policy integration path
- [ ] Add integration-friendly playback abstraction (for easier mocking/tests)
- [ ] Add lightweight benchmarks/profiling notes for large libraries

Backlog (nice-to-have):

- [ ] Optional MPD backend (only if required)
- [ ] Optional persistence for queue position
- [ ] Additional Subsonic endpoints (`search3`, scrobble, starring)

## Non-Goals (Current)

- Heavy UI framework migration
- Overly complex background services

## Architecture Snapshot

- `main`: bootstrap, load cache snapshot, run app, save snapshot
- `auth`: prompt + keyring + login validation
- `subsonic`: REST client + mapping + stream URL + DNS fallback
- `library`: lazy loading and in-memory normalized cache
- `cache`: disk cache (DNS + library snapshot)
- `state`: browser/tab/filter/navigation state machine
- `app`: event loop, input handling, queue/playback coordination
- `playback`: `ffplay` process control

## Operational Constraints

- Keep startup and interaction fast (avoid unnecessary full-library refetch)
- Keep docs synced with behavior (`README.md`, `TUI.MD`, `AGENTS.md`)
- Preserve security boundary: password only in keyring

## Definition of Done (for future features)

1. `cargo test` passes.
2. Keybinding contract remains intact.
3. No secret leakage in config/cache/logging.
4. Docs updated in same change.
