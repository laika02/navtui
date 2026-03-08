# AGENTS Guide

This file is the handoff and maintenance guide for future humans and AI agents working in this repository.

## Mission

Keep `navtui` fast, minimal, and behavior-stable.

Priorities:

1. Correct playback and navigation behavior
2. Low perceived latency
3. Zero plaintext password storage
4. Minimal dependencies and minimal runtime overhead

## Current Product Contract

- Platform: Linux (stable), Windows (experimental)
- Server: Subsonic-compatible (Navidrome target)
- UI: single-screen TUI with three tabs (Artists, Albums, Songs), Now Playing, Queue, and an optional bottom-left identity label (`username@server-host`) on the Queue pane
- Playback backend: `ffplay` child process
- Queue persistence: none (in-memory only)

## Core Runtime Flow

1. `main` calls `auth::bootstrap()`
2. Config and keyring credentials are loaded/validated (or prompted)
3. `SubsonicClient` is created
4. Library snapshot is loaded from cache if present, else artists are loaded from server
5. TUI loop runs (`app::run`)
6. Final library snapshot is saved on exit

## Module Ownership Map

- `src/main.rs`
  - Bootstrap and cache load/save orchestration
- `src/auth.rs`
  - First-run prompts
  - Keyring read/write
  - Login validation and human-readable auth/network errors
- `src/config.rs`
  - Platform-specific config path via `dirs` (`~/.config/...` on Linux, `%APPDATA%` on Windows)
  - Stores non-secret settings only (including keybinding overrides)
- `src/subsonic.rs`
  - Subsonic REST client
  - Response mapping into local models
  - Stream URL generation
  - DNS override and DoH fallback
- `src/library.rs`
  - Lazy artists/albums/songs cache
  - Flattened all-albums/all-songs derivation
- `src/cache.rs`
  - Disk cache files under platform cache dir (`~/.cache/navtui/` on Linux, `%LOCALAPPDATA%\\navtui\\` on Windows)
  - Legacy cache fallback from `subsonic-tui` paths for migration safety
  - DNS cache + library snapshot
- `src/state.rs`
  - Browser state machine for tabs/scopes/filters/cursor
  - Navigation and drill-down/back rules
- `src/app.rs`
  - Event loop
  - Keybindings and search mode behavior
  - Queue operations and shuffle routing
  - Drawing UI panes
- `src/playback.rs`
  - `ffplay` process lifecycle
  - live volume on Linux via `pactl` sink-input control (no playback restart)
  - pause/resume via `SIGSTOP`/`SIGCONT` on Unix
  - pause/resume on non-Unix targets via ffplay stdin control command

## Behavior Invariants (Do Not Break)

- All keyboard actions are configurable via `config.keybinds`; defaults below describe baseline behavior.
- `Tab` cycles tabs and resets context/filter state.
- `1/2/3` tab hotkeys mirror tab-switch reset behavior.
- `Shift` (or `Shift+Tab` fallback) toggles Queue Focus mode.
- `Up` and `Down` wrap around list boundaries.
- Arrow keys should auto-repeat on hold (`Up/Down/Left/Right`) wherever arrow navigation/actions apply.
- Volume keys should auto-repeat on hold (`[`/`]`) for continuous level changes.
- `;` and `'` should seek current playback by `-10s` and `+10s`.
- `r` hard refresh should clear on-disk library/DNS cache, reload library from server, and reset browser state.
- `Right/Enter` semantics:
  - Artist -> Albums filtered by artist
  - Album -> Songs filtered by album
  - Song -> play song
- `[`/`]` should adjust volume, and mouse click/drag on the Now Playing slider should set volume.
- Mouse click/drag on the Now Playing timeline scrubber should seek playback position.
- On Linux, in-playback volume adjustments should be live (no restart/cutout) when `pactl` backend is available.
- Mouse wheel over Queue pane should scroll queue viewport when overflowed.
- Mouse wheel over main browser pane should map to list cursor up/down behavior.
- Queue viewport should auto-follow current queue item when queue position changes (`,`, `.`, auto-advance, initial enqueue start).
- On tall windows, layout should keep Now Playing compact and use a proportional main/queue split that still gives queue meaningful extra space.
- In Queue Focus mode, `Up/Down` should move queue selection cursor (wrapping), and `Enter/Right` should commit playback.
- In Queue Focus mode, `Backspace` should remove the selected queue item.
- In Queue Focus mode, `Space` should toggle queue item reorder pick/drop; while picked, `Up/Down` should move that item within queue order.
- In Queue Focus mode, `Esc` while reordering should cancel the move and restore queue order to its pre-move state.
- In Queue Focus mode, browser-pane keybindings (`Tab`, `1/2/3`, browser navigation/search actions) should not affect top tabs.
- `a/A` should enqueue selected context item (artist/album/song based on active tab).
- First `a/A` enqueue while queue is idle should bootstrap playback from the queued item.
- `n` should queue selected context item as play-next; in Queue Focus mode it should move selected queue song to play-next.
- `Left` returns to prior drill-down context if available.
- Search:
  - Starts with `/`
  - Filters live, case-insensitive
  - `Esc` clears search/filter first; if already clear, it navigates back
  - `Shift+Esc` is the global reset to main Artists tab and clears filters/scopes
  - `Tab` cancels search before switching tabs
  - `1/2/3` are text input while in search mode
  - If current tab has an active filter, `Backspace` keeps editing it even outside explicit search mode
  - In search mode, `Backspace` exits search once the field is empty
  - If `expand_on_search_collapse` is enabled, single-match auto-select in Artists/Albums auto-expands and clears search
  - Cursor navigation still works while searching
- Artist-scoped song lists must include featured tracks (not just primary artist fields).
- Password must never be stored in plaintext config/log output.
- Windows build must stay compilable even if some behavior is degraded behind explicit errors.

## Env Flags and Operational Modes

- `NAVTUI_RESOLVE_IP=<ip>`
  - Hard DNS override for server host
- `NAVTUI_FAST_START=1`
  - Aggressive low-latency `ffplay` flags
  - May fail on some media/hosts; compatibility fallback must remain reliable
  - Legacy aliases are still accepted: `SUBSONIC_TUI_RESOLVE_IP`, `SUBSONIC_TUI_FAST_START`

## Performance Constraints

- Keep startup cheap:
  - Reuse library snapshot cache when available
  - Avoid unnecessary eager full-library refetches
- Keep interaction cheap:
  - Search is live; avoid expensive recomputation beyond active tab set
- Keep network calls bounded:
  - Use lazy loading where possible

## Security Constraints

- Config file contains only non-secrets (`server_url`, `username`).
- Password is stored in keyring only.
- Avoid printing auth tokens/passwords in errors or logs.

## Safe Change Checklist

Before finishing a change, verify:

1. `cargo test` passes.
2. Keybindings still match README contract.
3. Search cancel/tab-switch semantics still hold.
4. Queue flow still works (`a/A`, `n`, `c`, `,`, `.`).
5. Playback fallback still prevents rapid queue-skip regressions.
6. No plaintext secret was introduced into config/cache/logging.

## Manual Smoke Test

1. `cargo run`
2. Authenticate successfully (with existing saved password)
3. Navigate tabs with `Tab` and `1/2/3`
4. Drill in/out (`Right/Enter`, `Left`)
5. Run search (`/`, type, arrow navigate, `Esc`, `Tab`)
6. Start playback from Songs
7. Pause/resume (`Space`) on Unix
8. Shuffle from each tab context (`s`)
9. Enqueue song and album (`a`/`A`), play-next (`n`), clear queue (`c`), queue back/forward (`,`/`.`)
10. Seek current playback (`;` back 10s, `'` forward 10s) and confirm scrubber updates

## Known Risk Areas

- Playback startup timing differs by codec/container/server; avoid assuming a single ffplay profile works everywhere.
- `all_songs()` can be expensive on large libraries when snapshot is missing/stale.
- DNS behavior depends on local networking and DoH reachability.

## Documentation Policy

When behavior changes, update these files in the same PR/commit:

- `README.md` for user-facing behavior
- `plan.MD` for roadmap/status
- `TUI.MD` if layout or interaction contract changes
- `AGENTS.md` for maintenance guidance
- `CHANGELOG.md` for release notes history
