#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$script_dir/common.sh"

root="$(repo_root)"
arch="$(deb_arch)"
dist="$root/dist"
workdir="$(mktemp -d "${TMPDIR:-/tmp}/monica-gtk-deb.XXXXXX")"
trap 'rm -rf "$workdir"' EXIT

mkdir -p "$dist" "$workdir/pkg"
ensure_release_binary "$root" "$workdir/monica-gtk"
install_binary "$workdir/monica-gtk" "$workdir/pkg"
install_metadata "$root" "$workdir/pkg"

mkdir -p "$workdir/pkg/DEBIAN"
installed_size="$(du -sk "$workdir/pkg/usr" | cut -f1)"
cat > "$workdir/pkg/DEBIAN/control" <<EOF
Package: ${PACKAGE_NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${arch}
Maintainer: ${MAINTAINER}
Installed-Size: ${installed_size}
Depends: libc6, libgtk-4-1, libadwaita-1-0, libglib2.0-0
Homepage: https://github.com/6078HITHEDAY/Monica-by-Linux
Description: Monica GTK4 local-first password vault
 Monica by Linux (GTK4 / libadwaita). Local MDBX vault, portal file
 dialogs, notifications, optional StatusNotifierItem tray.
EOF

deb_path="$dist/${PACKAGE_NAME}_${VERSION}_${arch}.deb"
if command -v fakeroot >/dev/null; then
  fakeroot dpkg-deb --build "$workdir/pkg" "$deb_path"
else
  dpkg-deb --build "$workdir/pkg" "$deb_path"
fi
echo "Created $deb_path"
