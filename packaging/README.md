# Packaging (GTK4 line)

Phase 5 packaging for `monica-gtk`. Flatpak is **offline at build time**:
`packaging/flatpak/cargo-sources.json` is committed and the module uses
`cargo --offline` (no `--share=network`). Flathub submission is still a
separate upload step.

App id: **`com.monicapass.MonicaGtk`**
Binary: **`monica-gtk`**
Metadata lives in [`monica-gtk/data/`](../monica-gtk/data/).

| Artifact | Path | Notes |
| --- | --- | --- |
| `.desktop` | `monica-gtk/data/com.monicapass.MonicaGtk.desktop` | `Icon=com.monicapass.MonicaGtk`, `X-GNOME-UsesNotifications=true` |
| AppStream | `monica-gtk/data/com.monicapass.MonicaGtk.metainfo.xml` | Flathub-style metainfo, GPL-3.0-or-later |
| Icons | `monica-gtk/data/icons/hicolor/{16,24,32,48,64,128,256,512}x…/apps/` | Generated from `assets/Logo.png` |
| Flatpak | `packaging/flatpak/com.monicapass.MonicaGtk.yml` | **GNOME** runtime; offline `cargo-sources.json` |
| cargo-sources | `packaging/flatpak/cargo-sources.json` | Flathub vendor list from `Cargo.lock` |
| gettext | `monica-gtk/po/{zh_CN,en}.po` | Stable msgid 键集; CI via `check-i18n.py` |
| deb | `packaging/linux/package-deb.sh` | Host `cargo build --release` → `.deb` |
| RPM | `packaging/linux/package-rpm.sh` | Host `cargo build --release` → `.rpm` |

Native packages are named **`monica-gtk`** so they do not collide with the
frozen Avalonia `monica` packages.

Package **Version** comes from `MONICA_GTK_VERSION` (leading `v` stripped), or
from an exact `vX.Y.Z` / `X.Y.Z` tag on `HEAD`. Otherwise it stays `0.1.0`.
Release CI always sets the env from the git tag so artifacts are not stuck at
the fallback.

## GitHub Releases (CI)

GTK installers are published by [`.github/workflows/release-gtk.yml`](../.github/workflows/release-gtk.yml)
on `main`. This is **not** Avalonia’s `release.yml` (that file lives only on
`avalonia-frozen`).

### Cut a release

Gate CI (`check-gtk.yml`) should be green on `main` first.

```bash
git checkout main
git pull
git tag v0.1.0          # leading v; X.Y.Z only
git push origin v0.1.0
```

That tag glob is `v*.*.*` (example: `v0.1.0`). The workflow strips the `v` and
builds packages as Version `0.1.0`.

Or run **Actions → Release (GTK4) → Run workflow** on **`main`**, with version
`0.1.0` or `v0.1.0`. Manual runs from other branches are rejected.

### What CI publishes

| Artifact | Name |
| --- | --- |
| deb | `monica-gtk_<version>_amd64.deb` |
| RPM | `monica-gtk-<version>-1.x86_64.rpm` |
| checksums | `SHA256SUMS` |

Attached to a GitHub Release titled like `Monica GTK 0.1.0` (tag `v0.1.0`).
Release notes are bilingual and include Debian/Ubuntu install one-liners plus
the commit list since the previous `vX.Y.Z` tag.

Runner: `ubuntu-24.04` / x86_64. Other architectures: run the package scripts
locally with `MONICA_GTK_VERSION` set.

### Flatpak is not a release asset

The release workflow **does not** build or upload `.flatpak`. A full GNOME 48
SDK compile downloads ~1G and is the same cost `check-gtk.yml` already skips.
Use the local offline path below (`flatpak-builder` + `cargo-sources.json`).
Flathub submission is still a separate step; this workflow does not enable
`--share=network` or online sync.

## Flatpak permissions

Portals matching `org.freedesktop.portal.*` are **always allowed** by Flatpak.
The Phase 4 client uses them through GTK / Gio / ashpd; they are listed here so
reviewers can see *why* we still mention them, even without `--talk-name`.

| Permission | Why |
| --- | --- |
| `--share=ipc` | Required for Wayland / X11 toolkit shared memory |
| `--socket=wayland` | Display (preferred) |
| `--socket=fallback-x11` | X11 only if no Wayland; not `--socket=x11` |
| `--device=dri` | GTK4 GPU rendering |
| `org.freedesktop.portal.FileChooser` | `GtkFileDialog` (unlock / backup / import / export). Always allowed. |
| `org.freedesktop.portal.Notification` | `Gio Notification` on Wayland. Always allowed. |
| `org.freedesktop.portal.GlobalShortcuts` | ashpd bind for `<Control><Alt>M`. Always allowed. |
| `--talk-name=org.freedesktop.Notifications` | Fallback host notifications when the Notification portal is missing (X11 / some WMs) |
| `--talk-name=org.kde.StatusNotifierWatcher` | Register StatusNotifierItem tray (`ksni`) |
| `--talk-name=org.freedesktop.StatusNotifierWatcher` | Ayatana / older SNI watcher name (Phase 4 probes both) |
| `--own-name=org.kde.StatusNotifierItem-2-1` | `ksni` owns `org.kde.StatusNotifierItem-{pid}-{id}`. In the sandbox PID is typically 2. Broader `org.kde.*` is intentionally not granted. |

Not granted on purpose:

- `--share=network` — online sync is out of scope
- `--filesystem=home` — vault / import paths go through FileChooser portal
- `--socket=session-bus` unfiltered — only the talk/own names above

### Build / install Flatpak (local, offline cargo)

Needs `flatpak`, `flatpak-builder`, and Flathub. Source downloads (GNOME SDK,
`cargo-sources.json` crates, Mdbx git) use the network; the **module build**
does not (`cargo --offline`).

```bash
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install -y --user flathub \
  org.gnome.Platform//48 \
  org.gnome.Sdk//48 \
  org.freedesktop.Sdk.Extension.rust-stable//24.08

# From the repository root:
./packaging/linux/build-flatpak.sh
flatpak --user install --reinstall dist/com.monicapass.MonicaGtk.flatpak
flatpak run com.monicapass.MonicaGtk
```

### Rebuild `cargo-sources.json` when Rust deps change

Commit a new file whenever `monica-gtk/Cargo.lock` or the pinned
`Monica-Pass/Mdbx` git rev changes. The generator is vendored at
`packaging/flatpak/flatpak-cargo-generator.py` (MIT, from
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools)).

```bash
python3 -m pip install --user aiohttp tomlkit PyYAML
./packaging/linux/generate-cargo-sources.sh
./packaging/linux/check-cargo-sources.py
git add packaging/flatpak/cargo-sources.json
```

Then keep `cargo --offline` in `packaging/flatpak/com.monicapass.MonicaGtk.yml`
and do **not** add module `build-args: [--share=network]`. CI runs
`check-cargo-sources.py` to require every crates.io / git package from
`Cargo.lock` to appear in the JSON.

Runtime **48** is the Flathub GNOME line that matches gtk4-rs `v4_14` /
libadwaita `v1_5` without pulling Fedora 44's GNOME 49/50. Bump
`runtime-version` in the YAML when you are ready; rust-stable's branch must
match the Freedesktop SDK under that GNOME runtime (`24.08` for GNOME 48,
`25.08` for GNOME 49+).

## deb

```bash
# optional: MONICA_GTK_VERSION=0.1.0  (or v0.1.0)
./packaging/linux/package-deb.sh
# → dist/monica-gtk_<version>_amd64.deb
sudo apt install ./dist/monica-gtk_0.1.0_amd64.deb
```

Depends on `libgtk-4-1`, `libadwaita-1-0`, and glib. The script builds a
release binary with the host rustc (1.86+). GitHub Releases attach the same
file for the tagged version.

## RPM

```bash
./packaging/linux/package-rpm.sh
# → dist/monica-gtk-<version>-1.x86_64.rpm
sudo dnf install ./dist/monica-gtk-0.1.0-1.x86_64.rpm   # Fedora
# or: sudo rpm -Uvh ./dist/monica-gtk-0.1.0-1.x86_64.rpm
```

Requires `gtk4`, `libadwaita`, `glib2`. `rpmbuild` is needed (`rpm` on Debian,
`rpm-build` on Fedora).

## Validate without installing

```bash
./packaging/linux/validate-packaging.sh
```

Checks the desktop file, AppStream metainfo, Flatpak manifest keys / finish-args
(including **offline cargo**), hicolor icon sizes, `cargo-sources.json` vs
`Cargo.lock`, gettext 键集 (`check-i18n.py`), and `MONICA_GTK_VERSION`
normalization (`v2.3.4` → `2.3.4`). Gate CI (`check-gtk.yml`) runs this plus
actually building the `.deb` / `.rpm`. Release CI runs it again before
attaching tagged packages.

## gettext 键集

GTK UI copy lives in [`monica-gtk/po/`](../monica-gtk/po/) (`zh_CN.po`, `en.po`).
`msgid` is a stable key, not English-as-msgid. Catalogs are parsed at compile
time (`monica-gtk/crates/monica-gtk/src/i18n.rs`); there is no `gettext-sys`.

```bash
# After adding a t("new.key") call, add msgstr to both .po files (or the table
# in monica-gtk/po/catalog.py and re-run it), then:
python3 packaging/linux/check-i18n.py
```

## Out of scope / blocked

| Item | Status |
| --- | --- |
| Flathub upload | Offline `cargo-sources.json` is committed; store listing is a separate step |
| gettext 键集校验 | **Done** — `check-i18n.py` in CI |
| Online sync (WebDAV / OneDrive / Bitwarden) | **Blocked** — see monica-gtk README; offline MDBXSYNC only |
| AppImage | Not requested; use Flatpak or native packages |
| Avalonia `monica` packages | Stay on `avalonia-frozen` |
