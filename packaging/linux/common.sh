#!/usr/bin/env bash
# Shared layout for native Linux packages (deb / RPM).
# shellcheck disable=SC2034
set -euo pipefail

repo_root() {
  local here
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  printf '%s\n' "$here"
}

APP_ID="com.monicapass.MonicaGtk"
PACKAGE_NAME="monica-gtk"
BINARY_NAME="monica-gtk"
VERSION="${MONICA_GTK_VERSION:-0.1.0}"
MAINTAINER="${MONICA_GTK_MAINTAINER:-Monica Linux <maintainers@example.com>}"

host_arch() {
  case "$(uname -m)" in
    x86_64|amd64) printf 'x86_64\n' ;;
    aarch64|arm64) printf 'aarch64\n' ;;
    *)
      echo "Unsupported architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac
}

deb_arch() {
  case "$(host_arch)" in
    x86_64) printf 'amd64\n' ;;
    aarch64) printf 'arm64\n' ;;
  esac
}

ensure_release_binary() {
  local root="$1"
  local out="$2"
  if [[ -n "${MONICA_GTK_BIN:-}" ]]; then
    cp -f "$MONICA_GTK_BIN" "$out"
    chmod 0755 "$out"
    return
  fi
  if [[ ! -x "$root/monica-gtk/target/release/monica-gtk" ]]; then
    echo "Building monica-gtk (release)…" >&2
    (cd "$root/monica-gtk" && cargo build --release --locked -p monica-gtk)
  fi
  cp -f "$root/monica-gtk/target/release/monica-gtk" "$out"
  chmod 0755 "$out"
}

install_metadata() {
  local root="$1"
  local dest="$2"
  local data="$root/monica-gtk/data"
  install -Dm644 "$data/${APP_ID}.desktop" \
    "$dest/usr/share/applications/${APP_ID}.desktop"
  install -Dm644 "$data/${APP_ID}.metainfo.xml" \
    "$dest/usr/share/metainfo/${APP_ID}.metainfo.xml"
  local size
  for size in 16 24 32 48 64 128 256 512; do
    install -Dm644 \
      "$data/icons/hicolor/${size}x${size}/apps/${APP_ID}.png" \
      "$dest/usr/share/icons/hicolor/${size}x${size}/apps/${APP_ID}.png"
  done
}

install_binary() {
  local binary="$1"
  local dest="$2"
  install -Dm755 "$binary" "$dest/usr/bin/${BINARY_NAME}"
}
