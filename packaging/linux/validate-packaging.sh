#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$script_dir/common.sh"

root="$(repo_root)"
data="$root/monica-gtk/data"
manifest="$root/packaging/flatpak/com.monicapass.MonicaGtk.yml"
status=0

require() {
  if ! command -v "$1" >/dev/null; then
    echo "missing tool: $1" >&2
    status=1
    return 1
  fi
}

require desktop-file-validate
require python3

if command -v appstreamcli >/dev/null; then
  echo "==> appstreamcli validate"
  appstreamcli validate --no-net "$data/${APP_ID}.metainfo.xml" || status=1
elif command -v appstream-util >/dev/null; then
  echo "==> appstream-util validate"
  appstream-util validate-relax "$data/${APP_ID}.metainfo.xml" || status=1
else
  echo "warning: no appstreamcli / appstream-util; skipping metainfo schema" >&2
fi

echo "==> desktop-file-validate"
desktop-file-validate "$data/${APP_ID}.desktop" || status=1

echo "==> hicolor icons"
for size in 16 24 32 48 64 128 256 512; do
  icon="$data/icons/hicolor/${size}x${size}/apps/${APP_ID}.png"
  if [[ ! -f "$icon" ]]; then
    echo "missing icon: $icon" >&2
    status=1
  fi
done

echo "==> Flatpak manifest"
python3 - "$manifest" <<'PY' || status=1
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
required = [
    "id: com.monicapass.MonicaGtk",
    "runtime: org.gnome.Platform",
    "runtime-version:",
    "sdk: org.gnome.Sdk",
    "command: monica-gtk",
    "--socket=wayland",
    "--socket=fallback-x11",
    "--device=dri",
    "--talk-name=org.freedesktop.Notifications",
    "--talk-name=org.kde.StatusNotifierWatcher",
    "--talk-name=org.freedesktop.StatusNotifierWatcher",
    "--own-name=org.kde.StatusNotifierItem-2-1",
]
missing = [item for item in required if item not in text]
if "org.freedesktop.Platform" in text.split("runtime:", 1)[-1].splitlines()[0]:
    missing.append("must use org.gnome.Platform, not org.freedesktop.Platform")
if "--share=network" in text.split("finish-args:", 1)[-1].split("build-options:", 1)[0]:
    missing.append("finish-args must not share network (online sync is out of scope)")
if "--filesystem=home" in text:
    missing.append("do not grant --filesystem=home; FileChooser portal is enough")
if missing:
    print("Flatpak manifest checks failed:")
    for item in missing:
        print(f"  - {item}")
    sys.exit(1)
print("manifest keys and portal/tray finish-args ok")
PY

if [[ "$status" -ne 0 ]]; then
  echo "validate-packaging: FAIL" >&2
  exit "$status"
fi
echo "validate-packaging: ok"
