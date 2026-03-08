# navtui Release Plan

Last updated: 2026-03-08

## Target

Ship `navtui` `v1.0.0` as a fast, keyboard-first Navidrome/Subsonic TUI with Linux stable support and Windows experimental support.

## Release Gates

Must pass before `v1.0.0-rc1`:

- `cargo fmt --check`
- `cargo check --locked`
- `cargo test --locked`
- `cargo check --locked --target x86_64-pc-windows-gnu`
- `cargo build --release --locked`
- Manual smoke test pass on Linux and Windows
- Docs synced (`README.md`, `TUI.MD`, `AGENTS.md`, `plan.MD`, `CHANGELOG.md`)
- AI transparency docs synced (`AI_TRANSPARENCY.MD` and release-notes doc for the target tag)
- `LICENSE` present

## Current Status

Completed:

- Core Artists/Albums/Songs navigation and drill/back model
- Queue operations and queue focus mode (move/remove/commit)
- Search model and tab-reset semantics
- Playback fallback path and timeline/seek controls
- Live volume sync on Linux (`pactl` sink-input path)
- Library and DNS cache layers
- Keyring-only secret storage
- Configurable keybinds
- Packaging script (`scripts/package_release.sh`)
- CI (fmt/check/test/windows target check + Linux release build)
- App-level regression tests for queue wrap/reorder cancel, search hotkey behavior, and track-end retry/advance decision logic

Remaining before final `v1.0.0`:

- Add coverage for queue auto-advance integration path and playback failure pause behavior using a playback abstraction seam
- Run an RC soak pass against a large Navidrome library and capture any perf regressions
- Cut and validate `v1.0.0-rc1` artifacts on a clean machine

## Cut Sequence

1. Freeze feature work.
2. Update `CHANGELOG.md` for the RC.
3. Update AI transparency disclosure docs:
   - `AI_TRANSPARENCY.MD`
   - `RELEASE_NOTES_vX.Y.Z.MD`
4. Build artifacts:
   - `./scripts/package_release.sh v1.0.0-rc1`
5. Soak/manual test from archives.
6. Fix blockers only.
7. Repeat quality gates.
8. Tag and publish `v1.0.0`.

## Non-Goals for v1.0

- Queue persistence
- MPD backend
- Additional protocol surfaces beyond current playback/browse flow
