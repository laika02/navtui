# Release Checklist

Use this checklist before cutting a release tag.

## 1. Scope Lock

- Freeze feature changes for the release branch/window.
- Confirm current target version (`Cargo.toml`) and release tag format (`vX.Y.Z`).
- Update `CHANGELOG.md`:
  - Move finalized items from `Unreleased` into a new version section with date.

## 2. Quality Gates

- Run formatting and static checks:
  - `cargo fmt --check`
  - `cargo check --locked`
  - `cargo build --release --locked`
- Run tests:
  - `cargo test --locked`
- Confirm Windows compile remains healthy:
  - `cargo check --locked --target x86_64-pc-windows-gnu`

## 3. Manual Smoke Test

- Run: `navtui` (preferred) or `cargo run` in a source checkout
- Verify login and keyring credential reuse.
- Verify tab navigation and drill-down/back.
- Verify search (`/`, typing, `Esc`, tab switch behavior).
- Verify playback start, queue add/remove/reorder, queue back/forward.
- Verify volume controls (`[`, `]`, mouse slider) remain synced.
- Verify `r` hard refresh behavior.

## 4. Build Artifacts

- Build and package archives/checksums:
  - `./scripts/package_release.sh vX.Y.Z`
- Verify outputs exist:
  - `dist/releases/vX.Y.Z/linux_navtui_X.Y.Z.7z`
  - `dist/releases/vX.Y.Z/windows_navtui_X.Y.Z.7z` (unless skipped)
  - `dist/releases/vX.Y.Z/SHA256SUMS`

## 5. Documentation Pass

- Confirm these files reflect release behavior:
  - `README.md`
  - `TUI.MD`
  - `AGENTS.md`
  - `plan.MD`
  - `AI_TRANSPARENCY.MD`
  - release notes file for the target tag (for example `RELEASE_NOTES_v1.0.0-rc1.MD`)
- Confirm repository licensing is explicitly set (`LICENSE` file) before public release.

## 6. Tag and Publish

- Commit release prep changes.
- Create signed/annotated tag:
  - `git tag -a vX.Y.Z -m "navtui vX.Y.Z"`
- Push branch + tag.
- Confirm GitHub Release workflow succeeds and attaches:
  - `linux_navtui_X.Y.Z.7z`
  - `windows_navtui_X.Y.Z.7z` (unless intentionally skipped)
  - `SHA256SUMS`
- Publish release notes from `CHANGELOG.md`.
- Include explicit generative-AI disclosure in release notes (`AI_TRANSPARENCY.MD` + release notes file).
- If workflow publishing is disabled or fails, attach archives and `SHA256SUMS` manually.

## 7. Post-Release

- Re-open `Unreleased` section in `CHANGELOG.md`.
- Bump to next development version only when needed.
