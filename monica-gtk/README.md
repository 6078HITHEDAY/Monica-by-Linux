# monica-gtk

Monica Linux 的 **GTK4 + libadwaita** 客户端（[issue #8](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8)）。Phase 2 覆盖生成器、笔记、钱包、TOTP、时间线、回收站与归档，并沿用 Phase 1 的解锁 / 密码库 / 剪贴板清除 / 自动锁定。

现有 Avalonia 实现在 `avalonia-frozen` 分支上冻结，维护流程见 [`FROZEN.md`](../FROZEN.md)。本目录是 `main` 分支上的独立 Rust workspace，不原地重写。

## 本阶段完成了什么

| 清单 | 状态 |
| --- | --- |
| Phase 0 / Phase 1（解锁、密码 CRUD、剪贴板超时、自动锁定） | 完成（沿用） |
| 密码生成器（长度 / 大小写 / 数字 / 符号）；登录编辑可「生成」填入 | 完成 |
| 安全笔记：列表 / 详情 / 新建 / 编辑 / 软删除 | 完成 |
| 钱包：银行卡（`card`）与证件（`document-ref`）CRUD | 完成 |
| 动态口令：独立 TOTP 条目 + 登录项 `authenticator_key`；显示当前码与剩余秒数；复制走同一剪贴板清除 | 完成 |
| 时间线：上游 `CommitHistoryRepo` 最近提交 | 完成 |
| 回收站：列出软删除项并恢复 | 完成 |
| 永久删除 | **阻塞**：`TombstoneRepo::purge` 已禁用，`purge_authorized` 需要 TIGA 授权与墓碑保留期 |
| 归档 / 取消归档 | 完成（载荷字段 `archived`；MDBX 无一等归档位） |

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
cargo run -p monica-gtk -- --self-test   # 无 GUI：创建/解锁/登录·笔记·钱包·TOTP·归档·回收站·时间线
cargo run -p monica-gtk                  # GTK 界面
```

GUI：

1. 「创建保险库」或「解锁」需要主密码。
2. 侧栏：密码库 / 生成器 / 动态口令 / 安全笔记 / 钱包 / 时间线 / 回收站 / 归档。
3. 登录编辑里点刷新图标「生成」会按默认选项填入密码；生成器页可改字符集后再复制。
4. 动态口令支持 `otpauth://` 或 Base32 密钥，口令每秒刷新，复制后超时清除剪贴板。
5. 删除进回收站，可恢复。永久删除当前不可用（TIGA / 保留期）。
6. 「归档」把条目藏出列表；归档页可取消归档。归档写在 JSON 载荷的 `archived` 字段。
7. 「仅打开（不解锁）」仍是只读检查，不会原地升级 MDBX-1。
8. 标题栏「锁定」会清掉会话与敏感控件。

### 超时（可环境变量覆盖）

| 行为 | 默认 | 环境变量 |
| --- | --- | --- |
| 复制密码 / 口令 / 卡号后清除剪贴板 | **30 秒** | `MONICA_GTK_CLIPBOARD_CLEAR_SECS`（1–600） |
| 空闲自动锁定 | **300 秒（5 分钟）** | `MONICA_GTK_AUTO_LOCK_SECS`（3–7200） |

剪贴板使用 GTK4 `GdkClipboard`（`WidgetExt::clipboard()`），在 Wayland 上走系统剪贴板 / xdg-desktop-portal。超时到期时会再读一次剪贴板，**只有内容仍是 Monica 写入的那份秘密才清除**。锁定时会额外 `set_content(None)`。

主密码、条目密码、TOTP 密钥、卡号 / CVV 在 Rust 侧包进 `secrecy::SecretString`；锁定会调用上游 `VaultConnection::clear_session()` 丢掉 keyring。

## 上游 MDBX

- **依赖：** `mdbx-storage` + `mdbx-core` git rev `d1d3cc4fdff4e33fcb70099b3e7df36eeae43ba4`。条目走 `EntryRepo` / `ProjectRepo` / `CommitContext` / `CommitHistoryRepo`，不是平行存储层。
- **类型：** `login` / `note` / `card` / `totp` / `document-ref`。笔记与钱包载荷兼容 Android/Avalonia 的 `kind` + `item_data` JSON。
- **登录载荷：** `username` / `website` / `password_plain` / `authenticator_key` / `archived`。
- **现有 Avalonia `local.mdbx`：** inspect 只读；需要升级时拒绝解锁，不原地把 MDBX-1 升成 MDBX-2。
- **删除：** `EntryRepo::soft_delete`；恢复走 `EntryRepo::restore`。永久清理被上游 TIGA 门闩挡住。
- **归档：** 上游无归档 API。本客户端在载荷写入 `archived` / `archived_at`。其他客户端若整份重写载荷可能丢掉该字段。

## 打包

Phase 2 **不做** Flatpak / RPM / deb（D4 仍延后）。
