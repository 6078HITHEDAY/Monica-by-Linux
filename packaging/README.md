# Packaging (GTK4 line)

Phase 5 packaging for `monica-gtk`. This is **Flathub-ready in structure**, not a
day-one Flathub submission: the source Flatpak still needs a generated
`cargo-sources.json` before it can build fully offline.

App id: **`com.monicapass.MonicaGtk`**
Binary: **`monica-gtk`**
Metadata lives in [`monica-gtk/data/`](../monica-gtk/data/).

| Artifact | Path | Notes |
| --- | --- | --- |
| `.desktop` | `monica-gtk/data/com.monicapass.MonicaGtk.desktop` | `Icon=com.monicapass.MonicaGtk`, `X-GNOME-UsesNotifications=true` |
| AppStream | `monica-gtk/data/com.monicapass.MonicaGtk.metainfo.xml` | Flathub-style metainfo, GPL-3.0-or-later |
| Icons | `monica-gtk/data/icons/hicolor/{16,24,32,48,64,128,256,512}x…/apps/` | Generated from `assets/Logo.png` |
| Flatpak | `packaging/flatpak/com.monicapass.MonicaGtk.yml` | **GNOME** runtime (not Freedesktop) |
| deb | `packaging/linux/package-deb.sh` | Host `cargo build --release` → `.deb` |
| RPM | `packaging/linux/package-rpm.sh` | Host `cargo build --release` → `.rpm` |

Native packages are named **`monica-gtk`** so they do not collide with the
frozen Avalonia `monica` packages.

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

### Build / install Flatpak (local, network during cargo)

Needs `flatpak`, `flatpak-builder`, and Flathub:

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

`build-flatpak.sh` uses `--share=network` **at build time** so cargo can fetch the
pinned `Monica-Pass/Mdbx` git dependency. Flathub rejects that. To go offline:

```bash
# requires https://github.com/flatpak/flatpak-builder-tools
python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
  monica-gtk/Cargo.lock \
  -o packaging/flatpak/cargo-sources.json
```

Then replace the module `build-args: [--share=network]` with cargo `--offline`
and add `cargo-sources.json` under `sources` (see comments in the manifest).

Runtime **48** is the Flathub GNOME line that matches gtk4-rs `v4_14` /
libadwaita `v1_5` without pulling Fedora 44's GNOME 49/50. Bump
`runtime-version` in the YAML when you are ready; rust-stable's branch must
match the Freedesktop SDK under that GNOME runtime (`24.08` for GNOME 48,
`25.08` for GNOME 49+).

## deb

```bash
./packaging/linux/package-deb.sh
# → dist/monica-gtk_0.1.0_amd64.deb
sudo apt install ./dist/monica-gtk_0.1.0_amd64.deb
```

Depends on `libgtk-4-1`, `libadwaita-1-0`, and glib. The script builds a
release binary with the host rustc (1.86+).

## RPM

```bash
./packaging/linux/package-rpm.sh
# → dist/monica-gtk-0.1.0-1.x86_64.rpm
sudo dnf install ./dist/monica-gtk-0.1.0-1.x86_64.rpm   # Fedora
# or: sudo rpm -Uvh ./dist/monica-gtk-0.1.0-1.x86_64.rpm
```

Requires `gtk4`, `libadwaita`, `glib2`. `rpmbuild` is needed (`rpm` on Debian,
`rpm-build` on Fedora).

## Validate without installing

```bash
./packaging/linux/validate-packaging.sh
```

Checks the desktop file, AppStream metainfo, Flatpak manifest keys / finish-args,
and hicolor icon sizes. CI runs this plus actually building the `.deb` / `.rpm`.

## Out of scope / blocked

| Item | Status |
| --- | --- |
| Flathub upload + offline `cargo-sources.json` | Structure ready; generator output not committed |
| gettext 键集校验 | GTK 文案仍是源码内中文，还没有 `.po` |
| AppImage | Not requested; use Flatpak or native packages |
| Avalonia `monica` packages | Stay on `avalonia-frozen` |
