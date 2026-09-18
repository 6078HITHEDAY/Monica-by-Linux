#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$script_dir/common.sh"

if ! command -v rpmbuild >/dev/null; then
  echo "rpmbuild is required. Install 'rpm' (Debian/Ubuntu) or 'rpm-build' (Fedora)." >&2
  exit 1
fi

root="$(repo_root)"
arch="$(host_arch)"
dist="$root/dist"
workdir="$(mktemp -d "${TMPDIR:-/tmp}/monica-gtk-rpm.XXXXXX")"
trap 'rm -rf "$workdir"' EXIT

rpmbuild_top="$workdir/rpmbuild"
staging="$workdir/staging"
spec="$workdir/${PACKAGE_NAME}.spec"

mkdir -p "$dist" \
  "$rpmbuild_top"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS} \
  "$staging"

ensure_release_binary "$root" "$workdir/monica-gtk"
install_binary "$workdir/monica-gtk" "$staging"
install_metadata "$root" "$staging"

cat > "$spec" <<EOF
Name:           ${PACKAGE_NAME}
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Monica GTK4 local-first password vault
License:        GPL-3.0-or-later
URL:            https://github.com/6078HITHEDAY/Monica-by-Linux
BuildArch:      ${arch}

Requires:       gtk4
Requires:       libadwaita
Requires:       glib2

%description
Monica by Linux (GTK4 / libadwaita). Local MDBX vault, portal file
dialogs, notifications, optional StatusNotifierItem tray.

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a ${staging}/. %{buildroot}/

%files
%defattr(-,root,root,-)
/usr/bin/${BINARY_NAME}
/usr/share/applications/${APP_ID}.desktop
/usr/share/metainfo/${APP_ID}.metainfo.xml
/usr/share/icons/hicolor/*/apps/${APP_ID}.png

%changelog
* $(date -u '+%a %b %d %Y') Monica Linux <maintainers@example.com> - ${VERSION}-1
- monica-gtk ${VERSION}
EOF

rpmbuild \
  --define "_topdir ${rpmbuild_top}" \
  --define "_rpmdir ${rpmbuild_top}/RPMS" \
  --define "_srcrpmdir ${rpmbuild_top}/SRPMS" \
  --define "_builddir ${rpmbuild_top}/BUILD" \
  --define "_sourcedir ${rpmbuild_top}/SOURCES" \
  --define "_specdir ${rpmbuild_top}/SPECS" \
  --target "${arch}-linux" \
  -bb "$spec"

built="$(find "$rpmbuild_top/RPMS" -type f -name '*.rpm' | head -n 1)"
if [[ -z "$built" ]]; then
  echo "rpmbuild did not produce an RPM." >&2
  exit 1
fi

rpm_path="$dist/${PACKAGE_NAME}-${VERSION}-1.${arch}.rpm"
cp "$built" "$rpm_path"
echo "Created $rpm_path"
