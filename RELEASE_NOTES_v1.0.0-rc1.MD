# navtui v1.0.0-rc1 Release Notes

Release date: 2026-03-08

## AI Transparency (Important)

This project was created with substantial generative AI assistance.

- A generative AI coding agent produced major portions of code and docs.
- Human maintainers directed requirements, reviewed output, and validated behavior.
- Final release responsibility remains with human maintainers.

Detailed disclosure: `AI_TRANSPARENCY.MD`.

## Highlights

- Fast terminal UI player for Navidrome/Subsonic
- Artists / Albums / Songs browsing with drill-down navigation
- Queue controls, queue focus mode, reorder, play-next
- Live search/filter and keyboard-first interaction model
- Timeline scrubber + seek hotkeys + mouse seek
- Live Linux volume control with `pactl` sink-input path
- Library + DNS caching for lower startup latency
- Keyring-backed secret storage (no plaintext password in config)

## Project Rename

This RC uses the new project identity `navtui` (renamed from `subsonic-tui`).

- Binary: `navtui` / `navtui.exe`
- Config dir: `~/.config/navtui/` (Linux), `%APPDATA%\\navtui\\` (Windows)
- Cache dir: `~/.cache/navtui/` (Linux), `%LOCALAPPDATA%\\navtui\\` (Windows)
- Env vars: `NAVTUI_RESOLVE_IP`, `NAVTUI_FAST_START`

Legacy compatibility remains in place:

- Legacy config/cache paths are read as fallback
- Legacy keyring service entries are read as fallback
- Legacy env vars remain accepted (`SUBSONIC_TUI_RESOLVE_IP`, `SUBSONIC_TUI_FAST_START`)

## Artifacts

- `linux_navtui_1.0.0-rc1.7z`
- `windows_navtui_1.0.0-rc1.7z`
- `SHA256SUMS`

Current checksums:

```text
b990fa5c864337787252fa37c0df35b1bca9089ede8b1315e6698a74d454d848  linux_navtui_1.0.0-rc1.7z
51e764e2b23c27b41b39ef977562beccad625c31bfac05431ceaf9df6c684a7f  windows_navtui_1.0.0-rc1.7z
```

## Known Status

- Linux support: stable target
- Windows support: experimental target

## Known Performance Behavior

After a hard refresh (or any cold cache state), the first load of each tab
(`Artists`, `Albums`, `Songs`) can be noticeably slow while data is fetched and cached.
Subsequent visits are much faster once the cache is warm.

## Upgrade Note

If you used earlier `subsonic-tui` builds, you can move to `navtui` directly. Existing credentials/config/cache should continue to work through compatibility fallbacks.
