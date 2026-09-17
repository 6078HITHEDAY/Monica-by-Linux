# monica-gtk

Monica Linux 的 **GTK4 + libadwaita** 客户端（[issue #8](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8)）。Phase 4 覆盖 portal 能力（文件选择、通知、全局快捷键）与 StatusNotifierItem 托盘，并沿用 Phase 0–3 的解锁、条目、备份与导入导出。

现有 Avalonia 实现在 `avalonia-frozen` 分支上冻结，维护流程见 [`FROZEN.md`](../FROZEN.md)。本目录是 `main` 分支上的独立 Rust workspace，不原地重写。

## 本阶段完成了什么

| 清单 | 状态 |
| --- | --- |
| Phase 0–3（解锁、条目、备份、离线 MDBXSYNC、JSON 导入导出、设置、工作台） | 完成（沿用） |
| 文件选择：`GtkFileDialog`（xdg-desktop-portal FileChooser） | 完成；解锁 / 备份 / 导入导出共用 `dialogs.rs`，不用 `GtkFileChooserDialog` |
| 桌面通知：剪贴板清除、自动锁定、备份完成 | 完成；`Gio Notification`（Wayland 上走 portal）。无 `.desktop` 时 GNOME 可能不显示 |
| 全局快捷键：`ashpd` GlobalShortcuts，显示/隐藏（首选 `<Control><Alt>M`） | **部分**：运行时探测；无 portal 则设置页写明「未提供」。绑定需系统对话框，不自动弹窗 |
| 托盘：StatusNotifierItem（`ksni`），显示/隐藏、锁定、退出 | **部分**：有 watcher 才启动。GNOME 需 AppIndicator 扩展 |
| 能力探测：session bus / portal 接口 / SNI watcher | 完成；设置页展示，`--self-test` 打印，不硬编码「平台受限」 |
| 在线同步 / 二进制 `.kdbx` / CSV | **不做**（Phase 3 起阻塞，本阶段不接） |
| Flatpak 权限（`--talk-name` 等） | **延后 Phase 5** |
| 永久删除 | **阻塞**（TIGA / 墓碑保留期） |

## 构建依赖

在 **Ubuntu 24.04**（GTK 4.14.5 / libadwaita 1.5.0）上验证，需要 **Rust 1.86+**。

### Debian / Ubuntu

```bash
sudo apt-get install -y \
  build-essential pkg-config clang \
  libgtk-4-dev libadwaita-1-dev libglib2.0-dev \
  fonts-noto-cjk \
  xdg-desktop-portal xdg-desktop-portal-gtk
# GNOME 托盘（可选）：
# sudo apt-get install -y gnome-shell-extension-appindicator xdg-desktop-portal-gnome
```

### Fedora

```bash
sudo dnf install -y rust cargo gcc clang \
  gtk4-devel libadwaita-devel glib2-devel \
  google-noto-sans-cjk-fonts \
  xdg-desktop-portal xdg-desktop-portal-gtk
# GNOME 托盘（可选）：
# sudo dnf install -y gnome-shell-extension-appindicator xdg-desktop-portal-gnome
```

绑定版本（与 Phase 0 相同，以便 Ubuntu 24.04 能编过）：

| crate | 版本 | 用途 |
| --- | --- | --- |
| `gtk4` | 0.10.3 (`v4_14`) | 界面 / `GtkFileDialog` / `Gio Notification` |
| `libadwaita` | 0.8.1 (`v1_5`) | 壳与设置页 |
| `ashpd` | 0.11.1（`tokio`，**无** `gtk4` feature） | GlobalShortcuts portal |
| `ksni` | 0.3.1（blocking + tokio） | StatusNotifierItem |
| `zbus` | 5.5.0 | 运行时能力探测 |

`ashpd` 未开 `gtk4` feature，避免拉进另一套 gtk4-rs。

## 构建与运行

在 `monica-gtk/` 下：

```bash
cargo build --workspace
cargo test --workspace
cargo run -p monica-gtk -- --self-test   # 无 GUI：vault + 打印 portal/托盘探测
cargo run -p monica-gtk                  # GTK 界面
```

GUI：

1. 「创建保险库」或「解锁」需要主密码。路径旁两个按钮分别是打开 / 新建，都走 `GtkFileDialog`。
2. 侧栏：密码库 / 生成器 / 动态口令 / 笔记 / 钱包 / 时间线 / 回收站 / 归档 / 备份同步 / 导入导出 / 工作台 / 设置。
3. 设置页「运行时能力」显示文件选择、通知、全局快捷键、托盘的探测结果。「注册全局快捷键」会弹出系统确认框。
4. 有托盘时，点窗口关闭会隐藏到托盘（可在设置关掉）；无托盘则锁定并退出。
5. 窗口内：`Ctrl+L` 锁定，`Ctrl+Q` 退出。
6. 「仅打开（不解锁）」仍是只读检查，不会原地升级 MDBX-1。

### portal / 托盘（手动验证）

CI 和 `--self-test` **不会**点真实的 portal 对话框或点托盘图标。有桌面会话时请人工确认：

| 项 | 怎么测 | 预期 |
| --- | --- | --- |
| 文件选择 | 解锁页打开 / 新建；备份；导入导出 | 系统文件选择器（Wayland 为 portal），不是 GTK3 式 `FileChooserDialog` |
| 通知 | 复制秘密等超时；空闲锁定；备份完成 | 桌面通知 + 应用内 toast。GNOME 未装 `.desktop` 时可能只有 toast |
| 全局快捷键 | 设置 → 注册全局快捷键 | 有 GlobalShortcuts 则系统对话框；没有则副标题写明未提供 |
| 托盘 | 显示 / 隐藏 / 锁定 / 退出 | 有 `StatusNotifierWatcher` 才出现图标。GNOME 需扩展 |

**GNOME 托盘：** Shell 默认不画 StatusNotifierItem。请安装并启用
[AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/)
（Debian/Ubuntu 包 `gnome-shell-extension-appindicator`，Fedora 同名）。
KDE Plasma 与多数 wlroots 栏（Waybar 等）自带 watcher。

**通知与 `.desktop`：** Gio 用应用 id `com.monicapass.MonicaGtk`。未打包运行时，把
[`data/com.monicapass.MonicaGtk.desktop`](data/com.monicapass.MonicaGtk.desktop)
拷到 `~/.local/share/applications/`，否则 GNOME 可能丢掉通知。Phase 5 安装包会带上它。
Flatpak 权限（`--talk-name=org.freedesktop.portal.*` 等）也留到 Phase 5。

**GlobalShortcuts：** 需要足够新的 `xdg-desktop-portal` 与后端（GNOME 46+ / 对应 KDE portal）。
旧会话会在设置里显示「portal 无 GlobalShortcuts」，应用照常可用。首选触发 `<Control><Alt>M`，实际以系统对话框为准。

### 备份与交换格式

**便携备份（`.mdbx`）** — 上游 `BackupService`（与 `mdbx-cli backup` 相同）：

- SQLite 在线备份，发布时 `journal_mode=DELETE`，不带 WAL/SHM。
- 只复制密文，不解锁、不改格式。`MDBX-1` 备份仍是 `MDBX-1`。
- 禁止覆盖已有目标。

**离线同步包（`.mdbx-sync`）** — 上游 `mdbx-sync` 完整包：

- 文件头魔数 `MDBXSYNC`，默认写出 v3，尾部 SHA-256。
- 含 commit 图与同步状态。应用到另一份**同一 vault_id** 的已解锁保险库。
- 增量包（v4+，需 checkpoint）被拒绝，请改用完整包或 `mdbx-cli`。

**Monica JSON（`monica-gtk-export-v1`）** — 本客户端往返格式：

```json
{
  "format": "monica-gtk-export-v1",
  "exported_at": "…",
  "vault_id": "…",
  "logins": [{ "title": "", "username": "", "url": "", "notes": "", "password": "", "totp_secret": "" }],
  "notes": [{ "title": "", "content": "", "tags": "", "markdown": false }],
  "wallet": [{ "kind": "card|document", "title": "", "holder": "", "number": "", "extra": "", "expiry": "", "cvv": "", "notes": "" }],
  "totp": [{ "title": "", "issuer": "", "account": "", "secret": "", "period": 30, "digits": 6 }]
}
```

导入总是新建条目（不按 `entry_id` 合并）。独立 TOTP 写入 `TotpSource::Standalone`；登录绑定口令在 `logins[].totp_secret`。

**KDBX JSON** — `Vec<KdbxEntry>`，与 `mdbx-cli import-kdbx-json` 相同。GTK 把多条登录放在一个默认 project 里，所以**导出按登录条目写出**，而不是调用 `KdbxExporter::export_all`（那会把整个 project 折成一条）。导入走 `KdbxImporter::import_entries_atomic`，每条 KDBX 记录会新建一个 project。

### 超时（设置页 + 环境变量）

| 行为 | 默认 | 设置页 | 环境变量（启动时覆盖文件） |
| --- | --- | --- | --- |
| 复制后清除剪贴板 | **30 秒** | 「剪贴板清除」1–600 秒 | `MONICA_GTK_CLIPBOARD_CLEAR_SECS` |
| 空闲自动锁定 | **300 秒（5 分钟）** | 「自动锁定」1–120 分钟 | `MONICA_GTK_AUTO_LOCK_SECS` |

配置文件：`$XDG_CONFIG_HOME/monica-gtk/settings.json`（否则 `~/.config/monica-gtk/settings.json`），另含 `desktop_notifications` 与 `close_to_tray`（默认均为 true）。

剪贴板使用 GTK4 `GdkClipboard`（`WidgetExt::clipboard()`），在 Wayland 上走系统剪贴板 / xdg-desktop-portal。超时到期时会再读一次剪贴板，**只有内容仍是 Monica 写入的那份秘密才清除**。锁定时会额外 `set_content(None)`。

主密码、条目密码、TOTP 密钥、卡号 / CVV 在 Rust 侧包进 `secrecy::SecretString`；锁定会调用上游 `VaultConnection::clear_session()` 丢掉 keyring。备份 / 导入 / 导出 / 同步包 I/O 走 `gio::spawn_blocking`。

## 上游 MDBX

- **依赖：** `mdbx-storage`（`core` + `kdbx-import` + `kdbx-export`）+ `mdbx-core` + `mdbx-sync`，git rev `d1d3cc4fdff4e33fcb70099b3e7df36eeae43ba4`。
- **备份：** `BackupService::create_portable_copy` / `create_portable_copy_path`。
- **同步：** `PeerSyncService::export_complete_bundle` + `SyncApplyRepo::apply_batch_mut`。未接 `SyncClient` 线协议。
- **现有 Avalonia `local.mdbx`：** inspect / 工作台只读；需要升级时拒绝解锁，不原地把 MDBX-1 升成 MDBX-2。备份可在升级前保留 MDBX-1。
- **删除：** `EntryRepo::soft_delete`；恢复走 `EntryRepo::restore`。永久清理被上游 TIGA 门闩挡住。

## 打包

Phase 4 **不做** Flatpak / RPM / deb（D4 仍延后到 Phase 5）。开发用 `.desktop` 见 `data/com.monicapass.MonicaGtk.desktop`。
