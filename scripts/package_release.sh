#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/package_release.sh <tag> [--skip-windows]

Examples:
  ./scripts/package_release.sh vX.Y.Z
  ./scripts/package_release.sh vX.Y.Z --skip-windows
USAGE
}

if [[ $# -lt 1 ]]; then
  usage
  exit 1
fi

TAG=""
SKIP_WINDOWS=0

for arg in "$@"; do
  case "$arg" in
    --skip-windows)
      SKIP_WINDOWS=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    v*|[0-9]*)
      if [[ -n "$TAG" ]]; then
        echo "error: multiple tag/version arguments provided" >&2
        exit 1
      fi
      TAG="$arg"
      ;;
    *)
      echo "error: unknown argument: $arg" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "$TAG" ]]; then
  echo "error: missing tag/version argument" >&2
  usage
  exit 1
fi

if [[ "$TAG" != v* ]]; then
  TAG="v${TAG}"
fi

VERSION="${TAG#v}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/dist/releases/$TAG"
LINUX_STAGE="$OUT_DIR/linux_navtui_${VERSION}"
WINDOWS_STAGE="$OUT_DIR/windows_navtui_${VERSION}"

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "error: required command not found: $cmd" >&2
    exit 1
  fi
}

archive_dir() {
  local source_dir="$1"
  local out_archive="$2"
  if command -v 7z >/dev/null 2>&1; then
    (
      cd "$(dirname "$source_dir")"
      rm -f "$out_archive"
      7z a -t7z "$out_archive" "$(basename "$source_dir")" >/dev/null
    )
  else
    echo "error: 7z is required to create .7z archives" >&2
    exit 1
  fi
}

require_cmd cargo
require_cmd sha256sum

mkdir -p "$OUT_DIR"
rm -rf "$LINUX_STAGE" "$WINDOWS_STAGE"
mkdir -p "$LINUX_STAGE"

echo "==> Building Linux release binary"
cargo build --release --locked
cp "$ROOT_DIR/target/release/navtui" "$LINUX_STAGE/navtui"

if command -v ffplay >/dev/null 2>&1; then
  cp "$(command -v ffplay)" "$LINUX_STAGE/ffplay"
else
  echo "warning: ffplay not found on PATH; Linux package will only contain navtui" >&2
fi

if [[ "$SKIP_WINDOWS" -eq 0 ]]; then
  echo "==> Building Windows release binary"
  cargo build --release --locked --target x86_64-pc-windows-gnu
  mkdir -p "$WINDOWS_STAGE"
  cp "$ROOT_DIR/target/x86_64-pc-windows-gnu/release/navtui.exe" \
    "$WINDOWS_STAGE/navtui.exe"
fi

echo "==> Creating archives"
LINUX_ARCHIVE="$OUT_DIR/linux_navtui_${VERSION}.7z"
archive_dir "$LINUX_STAGE" "$LINUX_ARCHIVE"

WINDOWS_ARCHIVE=""
if [[ "$SKIP_WINDOWS" -eq 0 ]]; then
  WINDOWS_ARCHIVE="$OUT_DIR/windows_navtui_${VERSION}.7z"
  archive_dir "$WINDOWS_STAGE" "$WINDOWS_ARCHIVE"
fi

echo "==> Writing checksums"
(
  cd "$OUT_DIR"
  if [[ "$SKIP_WINDOWS" -eq 0 ]]; then
    sha256sum "$(basename "$LINUX_ARCHIVE")" "$(basename "$WINDOWS_ARCHIVE")" > SHA256SUMS
  else
    sha256sum "$(basename "$LINUX_ARCHIVE")" > SHA256SUMS
  fi
)

echo "==> Done"
echo "Output directory: $OUT_DIR"
