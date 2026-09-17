# monica-gtk

Monica Linux 的 **GTK4 + libadwaita** 客户端（[issue #8](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8)）。Phase 3 覆盖备份、离线同步包、导入 / 导出、设置和 MDBX 工作台，并沿用 Phase 0–2 的解锁与条目功能。

现有 Avalonia 实现在 `avalonia-frozen` 分支上冻结，维护流程见 [`FROZEN.md`](../FROZEN.md)。本目录是 `main` 分支上的独立 Rust workspace，不原地重写。

## 本阶段完成了什么

| 清单 | 状态 |
| --- | --- |
| Phase 0–2（解锁、密码 / 笔记 / 钱包 / TOTP、生成器、时间线、回收站、归档） | 完成（沿用） |
| 便携备份：用户选路径（`GtkFileDialog` / portal），`BackupService::create_portable_copy` | 完成 |
| 离线同步：导出 / 应用完整 `MDBXSYNC` 包（`PeerSyncService` + `mdbx-sync::write_bundle`） | 完成 |
| 在线同步（WebDAV / OneDrive / Bitwarden / 实时对端） | **阻塞**：无传输层，界面写明，不假装已同步 |
| 增量同步包 | **阻塞**：需要持久化 checkpoint，当前只接受完整包 |
| Monica JSON 导入 / 导出（`monica-gtk-export-v1`） | 完成；明文密钥，走 `gio::spawn_blocking` |
| KDBX JSON 导入 / 导出（与 `mdbx-cli import-kdbx-json` 同形） | 完成；导入走 `KdbxImporter::import_entries_atomic` |
| 二进制 `.kdbx`、Bitwarden JSON、CSV | **阻塞**：未启用 `kdbx-binary-*` / 无对应 Rust 解析器 |
| 设置：自动锁定、剪贴板清除（短中文标签，写入 `~/.config/monica-gtk/settings.json`） | 完成 |
| MDBX 工作台：格式 / schema / 计数 / 迁移检查 | 完成 |
| 原地 MDBX-1 → MDBX-2 升级 | **不做**：工作台只提示先备份 |
| 永久删除 | **阻塞**（TIGA / 墓碑保留期，Phase 2 起） |

## 构建依赖

在 **Ubuntu 24.04**（GTK 4.14.5 / libadwaita 1.5.0）上验证，需要 **Rust 1.86+**。

### Debian / Ubuntu

```bash
sudo apt-get install -y \
  build-essential pkg-config clang \
  libgtk-4-dev libadwaita-1-dev libglib2.0-dev \
  fonts-noto-cjk
```

### Fedora

```bash
sudo dnf install -y rust cargo gcc clang \
  gtk4-devel libadwaita-devel glib2-devel \
  google-noto-sans-cjk-fonts
```

绑定版本（与 Phase 0 相同，以便 Ubuntu 24.04 能编过）：

| crate | 版本 | 系统库 feature |
| --- | --- | --- |
| `gtk4` | 0.10.3 | `v4_14` |
| `libadwaita` | 0.8.1 | `v1_5` |

## 构建与运行

在 `monica-gtk/` 下：

```bash
cargo build --workspace
cargo test --workspace
cargo run -p monica-gtk -- --self-test   # 无 GUI：CRUD + 备份 + 导入导出 + 同步包 + 工作台
cargo run -p monica-gtk                  # GTK 界面
```

GUI：

1. 「创建保险库」或「解锁」需要主密码。
2. 侧栏增加「备份同步」「导入导出」「工作台」「设置」。标题栏齿轮也可进设置。
3. 「备份到文件」写出便携 `.mdbx`（未解锁也可按路径备份）。目标必须是新文件。
4. 「导出同步包 / 应用同步包」处理完整 `MDBXSYNC` 文件，且必须是同一 `vault_id`。
5. Monica JSON 含明文密码；KDBX JSON 导入会按上游规则为每条建 project。
6. 「工作台」显示格式、schema、条目计数和迁移计划，没有升级按钮。
7. 「仅打开（不解锁）」仍是只读检查，不会原地升级 MDBX-1。
8. 标题栏「锁定」会清掉会话与敏感控件。

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

配置文件：`$XDG_CONFIG_HOME/monica-gtk/settings.json`（否则 `~/.config/monica-gtk/settings.json`）。

剪贴板使用 GTK4 `GdkClipboard`（`WidgetExt::clipboard()`），在 Wayland 上走系统剪贴板 / xdg-desktop-portal。超时到期时会再读一次剪贴板，**只有内容仍是 Monica 写入的那份秘密才清除**。锁定时会额外 `set_content(None)`。

主密码、条目密码、TOTP 密钥、卡号 / CVV 在 Rust 侧包进 `secrecy::SecretString`；锁定会调用上游 `VaultConnection::clear_session()` 丢掉 keyring。备份 / 导入 / 导出 / 同步包 I/O 走 `gio::spawn_blocking`。

## 上游 MDBX

- **依赖：** `mdbx-storage`（`core` + `kdbx-import` + `kdbx-export`）+ `mdbx-core` + `mdbx-sync`，git rev `d1d3cc4fdff4e33fcb70099b3e7df36eeae43ba4`。
- **备份：** `BackupService::create_portable_copy` / `create_portable_copy_path`。
- **同步：** `PeerSyncService::export_complete_bundle` + `SyncApplyRepo::apply_batch_mut`。未接 `SyncClient` 线协议。
- **现有 Avalonia `local.mdbx`：** inspect / 工作台只读；需要升级时拒绝解锁，不原地把 MDBX-1 升成 MDBX-2。备份可在升级前保留 MDBX-1。
- **删除：** `EntryRepo::soft_delete`；恢复走 `EntryRepo::restore`。永久清理被上游 TIGA 门闩挡住。

## 打包

Phase 3 **不做** Flatpak / RPM / deb（D4 仍延后）。
