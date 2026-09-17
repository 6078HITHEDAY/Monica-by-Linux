# monica-gtk

Monica Linux 的 **GTK4 + libadwaita** Phase 0 spike（[issue #8](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8)）。

现有 Avalonia 实现在 `avalonia-frozen` 分支上冻结，维护流程见 [`FROZEN.md`](../FROZEN.md)。本目录是 `main` 分支上的独立 Rust workspace，不原地重写。

## 本阶段完成了什么

| 清单 | 状态 |
| --- | --- |
| Rust crate 依赖 gtk4-rs + libadwaita-rs，能在装了 GTK4 / libadwaita 开发包的 Linux 上构建 | 完成 |
| `AdwApplicationWindow` + `AdwNavigationSplitView` + `AdwToolbarView` 骨架 | 完成 |
| 跟随系统主题（深浅色 / accent），无自定义浅色画刷 | 完成 |
| 系统 fontconfig 字体 + 中文解锁文案（「解锁」/「主密码」） | 完成 |
| 直接 git 依赖上游 `Monica-Pass/Mdbx` 的 `mdbx-storage`（不是 UniFFI `.so`） | 完成（自建 `local.mdbx` 可打开/解锁；Avalonia 旧格式只读检查，解锁前需复制） |

## 构建依赖

本 spike 在 **Ubuntu 24.04**（GTK 4.14.5 / libadwaita 1.5.0 / rustc 1.86）上验证。Ubuntu 仓库里的 rustc 1.83 太旧，传递依赖需要 **Rust 1.86+**（与上游 Mdbx CI 相同）。

### Debian / Ubuntu

```bash
sudo apt-get install -y \
  build-essential pkg-config clang \
  libgtk-4-dev libadwaita-1-dev libglib2.0-dev \
  fonts-noto-cjk
```

CJK 渲染依赖 fontconfig 能解析到中文字形。Ubuntu 上 `fonts-noto-cjk` 即可；不要在应用里指定 Segoe UI / 微软雅黑。

### Fedora（维护者环境：Fedora 44）

```bash
sudo dnf install -y rust cargo gcc clang \
  gtk4-devel libadwaita-devel glib2-devel \
  google-noto-sans-cjk-fonts
```

Fedora 44 的 GTK 4.22 / libadwaita 1.9 比 Ubuntu 24.04 新。绑定版本可以以后再升到 RFC 里写的 `gtk4 0.11` / `libadwaita 0.9`；那些 crate 需要 **rustc 1.92** 和更新的系统库。Phase 0 为了在 Ubuntu 24.04 上编过，锁定：

| crate | 版本 | 系统库 feature |
| --- | --- | --- |
| `gtk4` | 0.10.3 | `v4_14` |
| `libadwaita` | 0.8.1 | `v1_5` |

## 构建与运行

在 `monica-gtk/` 下：

```bash
cargo build --workspace
cargo test --workspace
cargo run -p monica-gtk -- --self-test   # 无 GUI：创建/打开/解锁 temp local.mdbx
cargo run -p monica-gtk                  # 解锁界面
```

GUI 启动后：

1. 左侧 `AdwNavigationSplitView` 是工作区列表（解锁 / 密码库 / …）。
2. 右侧 `AdwToolbarView` 是中文解锁表单：「主密码」+「解锁」。
3. 「创建演示保险库」会在路径处写一个新的 `local.mdbx`（默认 `Tiga Sky`，便于 spike）。创建/解锁/检查在工作线程执行，按钮在完成前禁用。
4. 「仅打开（不解锁）」走只读 SQLite + 上游 `inspect_migration`，读取 `vault_meta`，**不会**原地升级。
5. 状态行会显示当前是浅色还是深色（`AdwStyleManager`，跟随系统）。

## 上游 MDBX（清单第 5 项）

- **依赖方式：** `mdbx-storage` + `mdbx-core` 的 git 依赖，rev `d1d3cc4fdff4e33fcb70099b3e7df36eeae43ba4`（Monica-Pass/Mdbx `master`，2026-09-03）。内部仍用 bundled `rusqlite`，**没有** `libmdbx_ffi.so` / UniFFI。
- **自建库：** `monica-vault::self_test` 会创建 `local.mdbx`、再打开、用正确主密码解锁、拒绝错误密码。这证明 Phase 0 能直接调用 storage/repo API。
- **现有 Avalonia `local.mdbx`：** 上游没有 `VaultConnection::open_readonly`。`inspect_vault` 用只读 SQLite（`mode=ro&immutable=1`）调用官方 `inspect_migration`，不会把 `MDBX-1` / `MDBX-1-DRAFT` 升级成 `MDBX-2`。「解锁」在只读检查发现需要升级时会拒绝原地打开，并要求先复制/备份。当前格式的解锁仍走可写 `VaultConnection::open`（会设 WAL / `secure_delete`），但不会自动升格式或 schema。
- **未做：** 条目列表、commit/tombstone、与 Avalonia UniFFI 0.29.4 绑定（`fdf3382`）的 ABI 对齐。Avalonia 仍继续用那份冻结的 `.so`。

## 密钥材料（D3 脚手架）

`monica-vault` 已依赖 `secrecy` + `zeroize`。主密码从 GTK 控件取出后立刻包进 `SecretString`，只在调用 `UnlockService` 时 `expose_secret()`。GTK `GString` 本身的擦除、剪贴板超时清除留给后续阶段。

## 打包

Phase 0 **不做** Flatpak / RPM / deb（D4 延后）。
