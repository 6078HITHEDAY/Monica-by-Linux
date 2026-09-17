# monica-gtk

Monica Linux 的 **GTK4 + libadwaita** 客户端（[issue #8](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8)）。Phase 1 覆盖解锁 / 建库 → 密码列表 / 详情 / 编辑，以及剪贴板超时清除和自动锁定。

现有 Avalonia 实现在 `avalonia-frozen` 分支上冻结，维护流程见 [`FROZEN.md`](../FROZEN.md)。本目录是 `main` 分支上的独立 Rust workspace，不原地重写。

## 本阶段完成了什么

| 清单 | 状态 |
| --- | --- |
| Phase 0 骨架：跟随系统主题、CJK 解锁文案、只读 inspect、拒绝原地升级 | 完成（沿用） |
| 创建保险库 / 解锁保险库，会话留在 `VaultRuntime` 上，I/O 走 `gio::spawn_blocking` | 完成 |
| 解锁后密码列表（标题 / 用户名 / 网址） | 完成 |
| 详情：密码默认隐藏，按需显示；离开或锁定时从控件清掉 | 完成 |
| 新建 / 编辑 / 保存登录项；软删除（MDBX tombstone） | 完成（回收站 UI 留到 Phase 2） |
| 复制密码后超时清除剪贴板 | 完成，默认 **30 秒** |
| 空闲自动锁定 + 锁定按钮 | 完成，默认 **300 秒（5 分钟）** |
| Adwaita 系统 symbolic 图标（工具栏 / 侧栏 / 动作按钮） | 完成（无自定义图标包） |

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
cargo run -p monica-gtk -- --self-test   # 无 GUI：创建/解锁/登录 CRUD/锁定
cargo run -p monica-gtk                  # GTK 界面
```

GUI：

1. 「创建保险库」或「解锁」需要主密码（不再静默使用演示密码）。
2. 成功后进入「密码库」：左侧列表，右侧详情。
3. 「+」新建；详情里显示/复制/编辑/删除。密码默认显示为 `••••••••`。
4. 「仅打开（不解锁）」仍是只读检查，不会原地升级 MDBX-1。
5. 标题栏锁定按钮（`system-lock-screen-symbolic`）会清掉会话与敏感控件，回到解锁页。

动作按钮使用 libadwaita `ButtonContent` + Adwaita **symbolic** 图标名（`list-add-symbolic`、`edit-copy-symbolic`、`view-reveal-symbolic` / `view-conceal-symbolic`、`document-save-symbolic`、`user-trash-symbolic`、`folder-open-symbolic`、`dialog-password-symbolic` 等），跟随系统浅色/深色。窗口暂用 `dialog-password-symbolic` 作为 `icon-name`；品牌应用图标（hicolor 多尺寸 PNG/SVG）留到打包阶段，见下方「图标」。

### 超时（可环境变量覆盖）

| 行为 | 默认 | 环境变量 |
| --- | --- | --- |
| 复制密码后清除剪贴板 | **30 秒** | `MONICA_GTK_CLIPBOARD_CLEAR_SECS`（1–600） |
| 空闲自动锁定 | **300 秒（5 分钟）** | `MONICA_GTK_AUTO_LOCK_SECS`（3–7200） |

剪贴板使用 GTK4 `GdkClipboard`（`WidgetExt::clipboard()`），在 Wayland 上走系统剪贴板 / xdg-desktop-portal，而不是 X11 选择所有者 API。超时到期时会再读一次剪贴板，**只有内容仍是 Monica 写入的那份秘密才清除**，避免覆盖用户后来复制的文本。锁定时会额外 `set_content(None)`，避免锁定后秘密仍留在系统剪贴板。

自动锁定根据窗口上的指针移动、按键和点击重置空闲计时器。

主密码与条目密码在 Rust 侧包进 `secrecy::SecretString`；锁定会调用上游 `VaultConnection::clear_session()` 丢掉 keyring。

## 上游 MDBX

- **依赖：** `mdbx-storage` + `mdbx-core` git rev `d1d3cc4fdff4e33fcb70099b3e7df36eeae43ba4`。登录项走 `EntryRepo` / `ProjectRepo` / `CommitContext`（commit + tombstone），不是平行存储层。
- **载荷：** 写入 `kind=password` JSON（`username` / `website` / `password` / `password_plain` / `notes`），读取时同时兼容上游测试用的 `{username,password}` 和 Android/Avalonia 的 `password_plain`。
- **现有 Avalonia `local.mdbx`：** inspect 只读；需要升级时拒绝解锁，不原地把 MDBX-1 升成 MDBX-2。
- **删除：** `EntryRepo::soft_delete`（tombstone）。回收站 / 恢复 UI 属于 Phase 2。

## 图标

Phase 1 **不**自带图标包。按钮和侧栏走 Adwaita 主题里已有的 `*-symbolic` 名称，由 `gtk::IconTheme` 解析（Ubuntu / GNOME 的 `adwaita-icon-theme`）。

- 窗口 / `.desktop` 的 `Icon=` 目前也指向 `dialog-password-symbolic`（见 `data/com.monicapass.MonicaGtk.desktop`）。这只是占位，**不是** Monica 品牌标。
- 完整应用图标（hicolor 48/128/256/scalable + 安装进 prefix）属于 Phase 5 打包，不要在本阶段画一套自定义 SVG。

## 打包

Phase 1 **不做** Flatpak / RPM / deb（D4 仍延后）。
