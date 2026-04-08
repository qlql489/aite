#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"

VERSION=$(node -p "require('./package.json').version")
PLATFORM="$(uname -s)"
STAGING_DEFAULT="/tmp/aite-bundles-$VERSION"
STAGING_DIR="${STAGING_DIR:-$STAGING_DEFAULT}"
TAURI_EXTRA_ARGS=()
TAURI_BUILD_CONFIG_ARGS=()
TARGET=""
BUNDLE_DIR=""

if [ "$#" -gt 0 ]; then
  TAURI_EXTRA_ARGS=("$@")
fi

log() {
  printf '\n[%s] %s\n' "$1" "$2"
}

warn() {
  printf 'WARN: %s\n' "$1"
}

fail() {
  printf 'ERROR: %s\n' "$1" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing command '$1'"
}

require_target() {
  rustup target list --installed | grep -qx "$1" || {
    echo "ERROR: missing Rust target '$1'" >&2
    echo "Run: rustup target add $1" >&2
    exit 1
  }
}

detect_signing_identity() {
  if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    log INFO "Using APPLE_SIGNING_IDENTITY from environment: $APPLE_SIGNING_IDENTITY"
    return 0
  fi

  local detected
  detected=$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/Developer ID Application/ { print $2; exit }')
  if [ -n "$detected" ]; then
    export APPLE_SIGNING_IDENTITY="$detected"
    log INFO "Detected Developer ID certificate: $APPLE_SIGNING_IDENTITY"
    return 0
  fi

  warn "No Developer ID Application certificate found. The build can still succeed, but Gatekeeper will keep showing 'Apple 无法验证'."
  return 1
}

detect_notarization() {
  if [ -n "${APPLE_API_KEY:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ] && [ -n "${APPLE_API_KEY_PATH:-}" ]; then
    log INFO "Notarization enabled via App Store Connect API key"
    return 0
  fi

  if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
    log INFO "Notarization enabled via Apple ID credentials"
    return 0
  fi

  warn "No notarization credentials detected. Even with a signing certificate, macOS will still warn unless the app is notarized."
  return 1
}

configure_updater_artifacts() {
  if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    log INFO "Updater artifact signing enabled via TAURI_SIGNING_PRIVATE_KEY"
    return 0
  fi

  TAURI_BUILD_CONFIG_ARGS=(--config src-tauri/tauri.private-ci.conf.json)
  warn "No TAURI_SIGNING_PRIVATE_KEY detected. Updater artifacts will be disabled for local packaging."
}

copy_first_match() {
  local source_dir="$1"
  local pattern="$2"
  local destination="$3"

  shopt -s nullglob
  local files=("$source_dir"/$pattern)
  shopt -u nullglob

  if [ ${#files[@]} -eq 0 ]; then
    warn "No files matched $pattern in $source_dir"
    return 1
  fi

  cp "${files[0]}" "$destination"
  return 0
}

copy_all_matches() {
  local source_dir="$1"
  local pattern="$2"

  shopt -s nullglob
  local files=("$source_dir"/$pattern)
  shopt -u nullglob

  if [ ${#files[@]} -eq 0 ]; then
    return 0
  fi

  cp "${files[@]}" "$STAGING_DIR/"
}

run_tauri_build() {
  local cmd=(tauri build)

  if [ -n "$TARGET" ]; then
    cmd+=(--target "$TARGET")
  fi

  if [ ${#TAURI_BUILD_CONFIG_ARGS[@]} -gt 0 ]; then
    cmd+=("${TAURI_BUILD_CONFIG_ARGS[@]}")
  fi

  if [ ${#TAURI_EXTRA_ARGS[@]} -gt 0 ]; then
    cmd+=("${TAURI_EXTRA_ARGS[@]}")
  fi

  npx "${cmd[@]}"
}

resolve_build_target() {
  case "$PLATFORM" in
    Darwin)
      TARGET="universal-apple-darwin"
      BUNDLE_DIR="src-tauri/target/$TARGET/release/bundle"
      ;;
    Linux)
      TARGET=""
      BUNDLE_DIR="src-tauri/target/release/bundle"
      ;;
    MINGW*|MSYS*|CYGWIN*)
      TARGET=""
      BUNDLE_DIR="src-tauri/target/release/bundle"
      ;;
    *)
      fail "Unsupported platform: $PLATFORM"
      ;;
  esac
}

prepare_macos_artifacts() {
  local dmg_dir="$BUNDLE_DIR/dmg"
  local macos_dir="$BUNDLE_DIR/macos"

  copy_first_match "$dmg_dir" '*.dmg' "$STAGING_DIR/Aite-v${VERSION}-macOS.dmg" \
    || fail "未找到 macOS dmg 安装包"

  local app_path
  app_path=$(find "$macos_dir" -maxdepth 1 -name '*.app' -type d | head -1 || true)
  if [ -n "$app_path" ]; then
    local app_dir app_name
    app_dir="$(dirname "$app_path")"
    app_name="$(basename "$app_path")"
    (
      cd "$app_dir"
      ditto -c -k --sequesterRsrc --keepParent "$app_name" "$STAGING_DIR/Aite-v${VERSION}-macOS.zip"
    )
  else
    warn "未找到 macOS .app，跳过 zip 产物生成"
  fi

  copy_all_matches "$macos_dir" '*.app.tar.gz'
  copy_all_matches "$macos_dir" '*.sig'
}

prepare_windows_artifacts() {
  copy_first_match "$BUNDLE_DIR/msi" '*.msi' "$STAGING_DIR/Aite-v${VERSION}-Windows.msi" \
    || fail "未找到 Windows MSI 安装包"
  copy_all_matches "$BUNDLE_DIR/nsis" '*.exe'
}

prepare_linux_artifacts() {
  copy_all_matches "$BUNDLE_DIR/appimage" '*.AppImage'
  copy_all_matches "$BUNDLE_DIR/deb" '*.deb'
  copy_all_matches "$BUNDLE_DIR/rpm" '*.rpm'

  if [ -z "$(find "$STAGING_DIR" -maxdepth 1 -type f | head -1)" ]; then
    fail "未找到 Linux 安装包产物"
  fi
}

log 1 "Preflight checks"
require_cmd node
require_cmd npm
require_cmd rustup
require_cmd cargo
require_cmd npx
resolve_build_target

if [ "$PLATFORM" = "Darwin" ]; then
  require_cmd security
  require_cmd ditto
  require_target aarch64-apple-darwin
  require_target x86_64-apple-darwin
  detect_signing_identity || true
  detect_notarization || true
fi

configure_updater_artifacts

log 2 "Building shared CLI assets"
npm run build:cli

log 3 "Building installer bundles"
run_tauri_build

log 4 "Collecting artifacts"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"

case "$PLATFORM" in
  Darwin)
    prepare_macos_artifacts
    ;;
  Linux)
    prepare_linux_artifacts
    ;;
  MINGW*|MSYS*|CYGWIN*)
    prepare_windows_artifacts
    ;;
esac

log 5 "Done"
echo "Artifacts collected in: $STAGING_DIR"
ls -lh "$STAGING_DIR"
