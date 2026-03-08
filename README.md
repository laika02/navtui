# navtui

Fast, minimal TUI music player for Subsonic-compatible servers (tested against Navidrome).

- UI: `ratatui` + `crossterm`
- Audio backend: external `ffplay` process
- Auth: Subsonic token auth (`s` + `t`), password stored in keyring (not plaintext config)
- Scope: Linux stable, Windows experimental

## AI Transparency

This project was created with substantial generative AI assistance.

- See `AI_TRANSPARENCY.MD` for the full disclosure.
- See `RELEASE_NOTES_v1.0.0-rc1.MD` for release-specific disclosure.

## Features

- Artists / Albums / Songs tabbed browser
- Optional header identity label (`username@server-host`) in the bottom-left of the Queue pane
- Drill-down navigation (artist -> albums -> songs)
- Context-aware shuffle
- Queue controls (enqueue, clear, back, forward)
- Live in-tab search/filter (`/`)
- DNS fallback logic + DNS cache
- Library snapshot cache to reduce startup latency

## Platform Support

- Linux: supported
- Windows: experimental
  - Core navigation/playback controls match Linux behavior
  - `Space` pause/resume uses ffplay stdin control fallback

## Requirements

- Rust toolchain (stable)
- `ffplay` available on `PATH` (usually via an FFmpeg package)
- `pactl` available on `PATH` for live in-playback volume changes on Linux
- Working OS credential storage (keyring)

Quick dependency check:

```bash
ffplay -version
pactl --version
cargo --version
```

## Run

```bash
cargo run
```

First run prompts for:

1. Navidrome/Subsonic server URL
2. Username
3. Password

On later runs, server + username come from config, and password is read from keyring.

## Data and Secrets

- Config file:
  - Linux: `~/.config/navtui/config.toml`
  - Windows: `%APPDATA%\\navtui\\config.toml` (resolved via `dirs`)
  - Legacy fallback is supported from `subsonic-tui` paths if new paths are absent
  - Stores only non-secret settings:
    - `server_url`
    - `username`
    - `always_hard_refresh_on_launch` (default `false`)
    - `expand_on_search_collapse` (default `false`)
    - `show_identity_label` (default `true`)
    - `keybinds` (optional overrides table; defaults shown below)
  - File mode is set to `0600` on Unix
- Password storage: keyring service `navtui`
  - Legacy fallback reads `subsonic-tui` keyring service entries
  - Entry key format: `<username>@<md5(server_url)>`
- Cache dir:
  - Linux: `~/.cache/navtui/`
  - Windows: `%LOCALAPPDATA%\\navtui\\`
  - Legacy fallback reads from `subsonic-tui` cache dir when needed
  - `dns.json` (hostname -> IP, TTL 1 hour)
  - `library-<hash>.json` (library snapshot)

Example `config.toml`:

```toml
server_url = "https://music.example.com"
username = "your_user"
always_hard_refresh_on_launch = false
expand_on_search_collapse = false
show_identity_label = true

[keybinds]
# Optional: only set keys you want to override.
# Example:
# quit = ["ctrl+q"]
```

## Default Keybindings

Global:

- `q`: quit
- `Tab`: cycle tabs (`Artists -> Albums -> Songs`) and reset tab context/filter
- `1` / `2` / `3`: jump to Artists / Albums / Songs (same reset behavior)
- `Shift` (or `Shift+Tab` fallback): toggle Queue Focus mode
- `Up` / `Down`: move selection (wraps at top/bottom)
- `Left`: navigate back
- `Right` or `Enter`: activate current selection
- Holding arrow keys repeats movement/navigation in that direction.
- Holding `[` / `]` repeats volume down/up while held.
- `Space`: play/pause
- `[` / `]`: volume down / up
- `;` / `'`: rewind / fast-forward 10 seconds
- `s`: shuffle current context
- `a` or `A`: enqueue selected item (artist in Artists tab, album in Albums tab, song in Songs tab)
- `n`: play selected item next (artist/album/song by active tab; selected queue song in Queue Focus mode)
- `c`: clear queue and stop playback
- `r`: hard refresh library (clears on-disk library/DNS cache first), resets browser state, and rewrites snapshot
- `Shift+Esc`: global reset to main Artists tab (clears search/filters/scopes)
- `Esc`: clear search/filter first; if already clear, acts as back
- `,` / `.`: queue back / queue forward

Queue Focus mode:

- `Up` / `Down`: move queue selection cursor (wraps at queue boundaries)
- `Enter` or `Right`: play the selected queue item
- `Backspace`: remove selected queue item
- `Space`: toggle queue reorder pick/drop on selected item
  - While picked, `Up` / `Down` moves that queue item up/down in the queue
  - `Esc` cancels the active move and restores queue order to the pre-move state
- Top browser keys are disabled in this mode (`Tab`, `1/2/3`, browser `Left/Right`, search/filter actions)

Mouse:

- Click or drag on the Now Playing volume slider to set volume.
- Click or drag on the Now Playing timeline scrubber to seek playback position.
- Scroll wheel over the main browser pane (Artists/Albums/Songs) to move selection.
- Scroll wheel over the Queue pane to scroll queued items when they exceed visible rows.
- Live volume updates while a track is playing use Linux PulseAudio-compatible control (`pactl`) and do not restart playback.
  - Slider display follows the last confirmed applied playback volume; if sink input is not ready yet, updates are deferred until it appears.

Search mode:

- `/`: start search in active tab
- Typing: live case-insensitive filtering
- `Backspace`: delete character
- `Backspace` on an empty search field: exits search mode
- `Up` / `Down` / `Left`: still navigates while searching
- `Right` / `Enter`: activate current selection
- `Esc`: clear search buffer/filter; if already clear, exit search and navigate back
- `Tab`: cancel search, then switch tab (resetting tab context/filter)
- `1` / `2` / `3`: treated as normal search text while search is active
- If the current tab already has an active filter, `Backspace` continues editing it even outside explicit search mode
- When search narrows to exactly one item, that item is auto-selected
  - If `expand_on_search_collapse = true`, single-match auto-select in Artists/Albums auto-expands that item and clears the search field

### Customize Keybindings

All keyboard actions are configurable in `config.toml` under `[keybinds]`.

- Each action accepts an array of key specs.
- Set an action to `[]` to disable it.
- Key spec formats:
  - Single chars: `"q"`, `"/"`, `","`, `"."`, `"["`, `"]"`, `"A"`
  - Named keys: `"tab"`, `"backtab"`, `"enter"`, `"esc"`, `"backspace"`, `"up"`, `"down"`, `"left"`, `"right"`, `"space"`
  - Modifiers: `"shift+esc"`, `"ctrl+q"`, `"alt+enter"`
  - Standalone modifier-key event: `"shift"` (useful for queue mode toggle)

Default keybind table:

```toml
[keybinds]
queue_mode_toggle = ["shift", "backtab"]
quit = ["q"]
escape = ["esc"]
global_reset = ["shift+esc"]
volume_down = ["["]
volume_up = ["]"]
seek_back = [";"]
seek_forward = ["'"]
search = ["/"]
tab_artists = ["1"]
tab_albums = ["2"]
tab_songs = ["3"]
tab_cycle = ["tab"]
nav_up = ["up"]
nav_down = ["down"]
nav_left = ["left"]
activate = ["right", "enter"]
enqueue = ["a", "A"]
play_next = ["n"]
play_pause = ["space"]
clear_queue = ["c"]
hard_refresh = ["r"]
shuffle = ["s"]
queue_back = [","]
queue_forward = ["."]
queue_remove = ["backspace"]
queue_reorder_toggle = ["space"]
search_backspace = ["backspace"]
```

## Behavior Notes

- In Artists tab, `Right/Enter` scopes Albums to that artist.
- In Albums tab, `Right/Enter` scopes Songs to that album.
- In Songs tab, `Right/Enter` plays selected song.
- In Artists tab, `a`/`A` enqueues the selected artist's songs.
- When queue is idle, first `a`/`A` enqueue also starts playback from the newly queued item.
- `n` inserts the selected item immediately after the currently playing queue entry.
- In Queue Focus mode, `n` moves the selected queue song to play next.
- Now Playing shows a timeline scrubber line under the volume slider using `POS elapsed |■■■□□□| total` style.
- Queue Focus mode routes `Up`/`Down` to queue cursor navigation; playback changes only on `Enter`/`Right`.
- Queue viewport auto-scrolls to keep the current queue item visible when queue position changes (`,`, `.`, auto-advance, first enqueue start).
- On Windows, `Space` pause/resume is driven through ffplay control input.
- Artist-scoped song views include songs where that artist appears as a primary or featured artist (`artist_id` plus parsed `artists[]` IDs).
- Queue is in-memory only and is not persisted across restarts.

## Environment Variables

- `NAVTUI_RESOLVE_IP=<ip>`
  - Force hostname resolution to a specific IP (useful when system DNS fails).
- `NAVTUI_FAST_START=1`
  - Enables aggressive `ffplay` startup flags for lower latency.
  - If this causes startup failures on your media/host, unset it and use compatibility behavior.
  - Legacy aliases still supported: `SUBSONIC_TUI_RESOLVE_IP`, `SUBSONIC_TUI_FAST_START`.

## DNS and Connectivity Strategy

When connecting to a hostname server URL, the client resolution order is:

1. `NAVTUI_RESOLVE_IP` override
2. Cached DNS override (`dns.json` in the platform cache dir)
3. DNS-over-HTTPS fallback (Cloudflare, then Google)

Resolved IPs are cached for 1 hour.

## Troubleshooting

- `DNS lookup failed ...`
  - Verify URL and connectivity.
  - Try: `NAVTUI_RESOLVE_IP=<your-server-ip> cargo run`
  - If this works, the issue is name resolution in your environment.

- `failed to store password in keyring ... DBus ... not activatable` (Linux)
  - The app uses Linux keyutils backend directly; ensure kernel keyring support is available.
  - Confirm your environment allows keyring operations.

- Playback starts slowly
  - Ensure server responsiveness first.
  - Test `NAVTUI_FAST_START=1 cargo run`.
  - If first-track failures occur with fast start, run without it.

- `unable to spawn ffplay process`
  - Install `ffplay` and ensure it is in `PATH`.

- Volume keys/slider say `live backend unavailable`
  - Install `pactl` (`pulseaudio-utils` or equivalent).
  - Ensure your audio stack exposes PulseAudio-compatible sink inputs (e.g. PipeWire Pulse server).

## Development

Run tests:

```bash
cargo test
```

Validate release build profile:

```bash
cargo build --release --locked
```

## Release

Use the project checklist:

- `RELEASE_CHECKLIST.md`

Build versioned archives + checksums:

```bash
./scripts/package_release.sh vX.Y.Z
```

Optional (Linux-only artifact build):

```bash
./scripts/package_release.sh vX.Y.Z --skip-windows
```

Project layout:

- `src/main.rs`: bootstrap flow
- `src/auth.rs`: prompt + keyring + login validation
- `src/subsonic.rs`: API client, mapping, DNS fallback
- `src/library.rs`: lazy library cache
- `src/cache.rs`: disk caches (DNS + snapshot)
- `src/state.rs`: browser/tab state machine
- `src/app.rs`: TUI loop, input handling, queue logic
- `src/playback.rs`: `ffplay` process management
- `src/model.rs`: shared data models
- `scripts/package_release.sh`: release archive/checksum builder
- `RELEASE_CHECKLIST.md`: release quality gates
- `CHANGELOG.md`: release notes source

## License

MIT. See `LICENSE`.
