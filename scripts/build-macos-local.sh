#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"

VERSION=$(node -p "require('./package.json').version")
STAGING_DEFAULT="/tmp/aite-macos-$VERSION"
STAGING_DIR="${STAGING_DIR:-$STAGING_DEFAULT}"
TAURI_EXTRA_ARGS=()
TAURI_BUILD_CONFIG_ARGS=()

if [ "$#" -gt 0 ]; then
  TAURI_EXTRA_ARGS=("$@")
fi

log() {
  printf '\n[%s] %s\n' "$1" "$2"
}

warn() {
  printf 'WARN: %s\n' "$1"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: missing command '$1'"
    exit 1
  }
}

require_target() {
  rustup target list --installed | grep -qx "$1" || {
    echo "ERROR: missing Rust target '$1'"
    echo "Run: rustup target add $1"
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

  warn "No Developer ID Application certificate found. The build can still succeed, but Gatekeeper will keep showing 'Apple 无法验证' when the app is downloaded from the Internet."
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
  warn "No TAURI_SIGNING_PRIVATE_KEY detected. Local packaging will disable updater artifacts so DMG bundles can still be produced."
}

copy_if_exists() {
  local source_dir="$1"
  local pattern="$2"
  local label="$3"

  shopt -s nullglob
  local files=("$source_dir"/$pattern)
  shopt -u nullglob

  if [ ${#files[@]} -eq 0 ]; then
    echo "WARN: no $label found in $source_dir"
    return 0
  fi

  cp "${files[@]}" "$STAGING_DIR/"
  echo "Copied $label"
}

log 1 "Preflight checks"
require_cmd npm
require_cmd rustup
require_cmd cargo
require_cmd npx
require_cmd security
require_target aarch64-apple-darwin
require_target x86_64-apple-darwin
detect_signing_identity || true
detect_notarization || true
configure_updater_artifacts

log 2 "Building shared frontend and CLI assets"
npm run build:cli

log 3 "Building aarch64-apple-darwin bundle"
if [ ${#TAURI_EXTRA_ARGS[@]} -gt 0 ]; then
  npx tauri build --target aarch64-apple-darwin "${TAURI_BUILD_CONFIG_ARGS[@]}" "${TAURI_EXTRA_ARGS[@]}"
else
  npx tauri build --target aarch64-apple-darwin "${TAURI_BUILD_CONFIG_ARGS[@]}"
fi

log 4 "Building x86_64-apple-darwin bundle"
if [ ${#TAURI_EXTRA_ARGS[@]} -gt 0 ]; then
  npx tauri build --target x86_64-apple-darwin "${TAURI_BUILD_CONFIG_ARGS[@]}" "${TAURI_EXTRA_ARGS[@]}"
else
  npx tauri build --target x86_64-apple-darwin "${TAURI_BUILD_CONFIG_ARGS[@]}"
fi

log 5 "Collecting bundle artifacts"
AARCH64_BUNDLE="src-tauri/target/aarch64-apple-darwin/release/bundle"
X86_64_BUNDLE="src-tauri/target/x86_64-apple-darwin/release/bundle"

rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"

copy_if_exists "$AARCH64_BUNDLE/dmg" '*.dmg' 'aarch64 DMG'
copy_if_exists "$X86_64_BUNDLE/dmg" '*.dmg' 'x86_64 DMG'
copy_if_exists "$AARCH64_BUNDLE/macos" '*.app.tar.gz' 'aarch64 app tarball'
copy_if_exists "$X86_64_BUNDLE/macos" '*.app.tar.gz' 'x86_64 app tarball'
copy_if_exists "$AARCH64_BUNDLE/macos" '*.sig' 'aarch64 signatures'
copy_if_exists "$X86_64_BUNDLE/macos" '*.sig' 'x86_64 signatures'

log 6 "Done"
echo "Artifacts collected in: $STAGING_DIR"
ls -lh "$STAGING_DIR"
