#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$script_dir/common.sh"

root="$(repo_root)"
manifest="$root/packaging/flatpak/${APP_ID}.yml"
dist="$root/dist"
workdir="${TMPDIR:-/tmp}/monica-gtk-flatpak-$$"

if ! command -v flatpak >/dev/null || ! command -v flatpak-builder >/dev/null; then
  echo "flatpak and flatpak-builder are required." >&2
  exit 1
fi

mkdir -p "$dist"
cleanup() { rm -rf "$workdir"; }
trap cleanup EXIT

repo="$workdir/repo"
build="$workdir/build"
state="$workdir/state"
mkdir -p "$repo" "$build" "$state"

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install -y --user flathub \
  org.gnome.Platform//48 \
  org.gnome.Sdk//48 \
  org.freedesktop.Sdk.Extension.rust-stable//24.08 || true

flatpak-builder \
  --user \
  --force-clean \
  --install-deps-from=flathub \
  --state-dir="$state" \
  --repo="$repo" \
  "$build" \
  "$manifest"

bundle="$dist/${APP_ID}.flatpak"
flatpak build-bundle "$repo" "$bundle" "$APP_ID"
echo "Created $bundle"
