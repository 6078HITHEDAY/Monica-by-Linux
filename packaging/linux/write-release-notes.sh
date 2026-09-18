#!/usr/bin/env bash
# Write bilingual GitHub Release notes for a monica-gtk native-package tag.
# Usage: write-release-notes.sh X.Y.Z [git-log-range]
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

raw="${1:-}"
if [[ -z "$raw" ]]; then
  echo "usage: write-release-notes.sh X.Y.Z [git-log-range]" >&2
  exit 1
fi
export MONICA_GTK_VERSION="$raw"
# shellcheck source=common.sh
source "$script_dir/common.sh"
version="$VERSION"
tag="v${version}"
root="$(repo_root)"
range="${2:-}"

changelog=""
if [[ -n "$range" ]]; then
  changelog="$(git -C "$root" log --pretty=format:'- %s (%h)' --no-merges "$range")"
else
  prev="$(
    git -C "$root" tag --list 'v*.*.*' --sort=-version:refname \
      | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' \
      | grep -vx "$tag" \
      | head -n1 \
      || true
  )"
  if [[ -n "$prev" ]]; then
    changelog="$(git -C "$root" log --pretty=format:'- %s (%h)' --no-merges "${prev}..HEAD")"
  else
    changelog="$(git -C "$root" log --pretty=format:'- %s (%h)' --no-merges -30)"
  fi
fi
if [[ -z "$changelog" ]]; then
  changelog="- (no conventional commits in range)"
fi

deb="monica-gtk_${version}_amd64.deb"
rpm="monica-gtk-${version}-1.x86_64.rpm"

cat <<EOF
## Monica GTK ${version}

GTK4 / libadwaita 本地优先密码库。Local-first GTK4 / libadwaita password vault.

CI 在 \`ubuntu-24.04\`（x86_64）上打 **deb** 与 **RPM**，Version 来自 git tag \`${tag}\`（去掉前导 \`v\`）。

This GitHub Release **does not** attach a Flatpak bundle. A full GNOME SDK build is too heavy for this workflow; build locally with \`flatpak-builder\` (\`./packaging/linux/build-flatpak.sh\`). Flathub upload and online sync are out of scope.

### 安装 / Install

**Debian / Ubuntu (amd64):**

\`\`\`bash
sudo apt install ./${deb}
\`\`\`

**Fedora / RPM (x86_64):**

\`\`\`bash
sudo dnf install ./${rpm}
# or: sudo rpm -Uvh ./${rpm}
\`\`\`

运行时依赖：GTK 4、libadwaita、glib（deb：\`libgtk-4-1\` \`libadwaita-1-0\`；RPM：\`gtk4\` \`libadwaita\` \`glib2\`）。

### 产物 / Artifacts

| File | Notes |
| --- | --- |
| \`${deb}\` | Package name \`monica-gtk\`, app id \`com.monicapass.MonicaGtk\` |
| \`${rpm}\` | Same payload; Release \`1\` |
| \`SHA256SUMS\` | SHA-256 of the packages above |

aarch64 / other arches: build the scripts locally (\`./packaging/linux/package-deb.sh\`, \`package-rpm.sh\`). Set \`MONICA_GTK_VERSION=${version}\` if you are not on tag \`${tag}\`.

### 变更 / Changes

${changelog}
EOF
