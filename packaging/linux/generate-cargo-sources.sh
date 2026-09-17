#!/usr/bin/env bash
# Generate Flathub-compatible offline cargo sources for monica-gtk.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$script_dir/common.sh"

root="$(repo_root)"
lock="$root/monica-gtk/Cargo.lock"
generator="$root/packaging/flatpak/flatpak-cargo-generator.py"
output="$root/packaging/flatpak/cargo-sources.json"

if [[ ! -f "$lock" ]]; then
  echo "missing Cargo.lock: $lock" >&2
  exit 1
fi
if [[ ! -f "$generator" ]]; then
  echo "missing generator: $generator" >&2
  exit 1
fi

echo "Generating $output from $lock …"
python3 "$generator" "$lock" -o "$output"
echo "Wrote $output"

cat <<'EOF'

Rebuild this file whenever monica-gtk/Cargo.lock (or the pinned Mdbx git rev)
changes:

  python3 -m pip install --user aiohttp tomlkit PyYAML
  ./packaging/linux/generate-cargo-sources.sh

Needs network (crates.io + git clone of Monica-Pass/Mdbx). Commit the updated
packaging/flatpak/cargo-sources.json. The Flatpak module already builds with
cargo --offline and must not gain --share=network.

  ./packaging/linux/check-cargo-sources.py
EOF
