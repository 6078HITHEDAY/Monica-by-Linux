# Monica by Linux

<p align="center">
  <img src="assets/Logo.png"
       alt="Monica application logo"
       width="128" />
</p>

<p align="center">
  <strong>Monica 的本地优先桌面密码库：以 Android 主应用为功能与安全基准，
  以 GTK4 + libadwaita 作为桌面交互设计基准。</strong>
</p>

<p align="center">
  <a href="https://github.com/6078HITHEDAY/Monica-by-Linux/actions/workflows/check-gtk.yml">
    <img src="https://github.com/6078HITHEDAY/Monica-by-Linux/actions/workflows/check-gtk.yml/badge.svg?branch=main"
         alt="GTK4 checks" />
  </a>
  <img src="https://img.shields.io/badge/Rust-1.86%2B-000000?style=flat-square&logo=rust&logoColor=white"
       alt="Rust 1.86+" />
  <img src="https://img.shields.io/badge/GTK-4-4A86CF?style=flat-square&logo=gtk&logoColor=white"
       alt="GTK 4" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue?style=flat-square"
       alt="GPL-3.0 license" />
</p>

* 这是一个个人维护的，给自己使用的 Linux 版本

## 分支布局

| 分支 | 技术栈 | 状态 |
| --- | --- | --- |
| **`main`（本分支）** | Rust + gtk4-rs / libadwaita-rs | **活跃开发中**，Phase 0 / Phase 1 / Phase 2 已完成 |
| `avalonia-frozen` | .NET 10 + Avalonia 12 + FluentAvalonia | **冻结**：只收安全修复 |

> ⚠️ **本分支目前不产出可分发安装包。** deb / rpm / AppImage / Flatpak 仍由
> `avalonia-frozen` 产出，直到本分支完成 Phase 5。在此之前存在一段**两个前端都不完整**
> 的窗口期。冻结线的维护流程见 [FROZEN.md](FROZEN.md)。

## 产品定位

Monica by Linux 是 Monica 的 **Linux 桌面**实现，只维护 Linux 目标。

- **产品与安全基线来自 Monica Android。** 数据格式、核心能力、安全边界和兼容路线以主
  应用为准。
- **桌面交互以 GTK4 + libadwaita 为基准。** 深浅色与 accent 跟随系统，字体走 fontconfig
  家族名，平台能力走 portal / D-Bus —— 而不是把 WinUI / Fluent 的假设搬到 Linux 上。
- **Vault 业务数据以 canonical MDBX 为准。** 直接使用上游 `Monica-Pass/Mdbx` 的
  `mdbx-storage`，不再经过 UniFFI 包装层。

本仓库不会生成 Android、iOS、Windows 或 macOS 包。Monica Android 仍由
[Monica 主仓库](https://github.com/Monica-Pass/Monica)独立维护和发布。

## 为什么重写

本仓库在 Linux 上收集到的问题（[issue #7](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/7)）
有一个共同根因：把 Windows 的假设当成了世界本身。

| 债务 | 旧实现的表现 | GTK4 + libadwaita 下 |
| --- | --- | --- |
| 主题 | 自定义 token 落在非主题条目 → 深色下白块、白字白底不可见 | 主题由 toolkit 提供，跟随系统 |
| 字体 | 字体栈首选 Segoe UI / 微软雅黑 → Linux 落进等宽回退 | fontconfig 家族名 + 系统 CJK 回退 |
| 图标 | 窗口/托盘用 `.ico`，打包另起一份 PNG | hicolor 多尺寸，`.desktop` 直接 `Icon=` |
| 依赖 | 发布包曾携带 Windows PE 的 `mdbx_ffi.dll` | 原生库随包，Flatpak 走 GNOME runtime |
| 能力 | 能力状态硬编码，全局快捷键被标"平台受限" | portal / D-Bus 直接对接 |
| 文案 | 英文串硬编码，中文缺 90 个 key | gettext，键集可校验 |

这些不该是一个个打补丁的 bug，而应该是"用了对的 toolkit 之后自动成立"的事。完整动机见
[RFC #8](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8)。

## 阶段计划

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| Phase 0 | `AdwApplicationWindow` + `AdwNavigationSplitView` 骨架；跟随系统主题、CJK 字体、中文解锁界面 | **已完成** |
| Phase 1 | 解锁 / 建库 → 密码列表 / 详情 / 编辑（含剪贴板清除、自动锁） | **已完成** |
| Phase 2 | 生成器、笔记、钱包、TOTP、时间线、回收站与归档 | **已完成** |
| Phase 3 | 同步与备份、导入 / 导出、设置、MDBX 工作台 | 未开始 |
| Phase 4 | portal 能力（全局快捷键、通知、文件选择器）、托盘（StatusNotifierItem） | 未开始 |
| Phase 5 | 打包与 CI（Flatpak + RPM/deb），键集校验进 CI | 未开始 |

### D1 决策：Rust + gtk4-rs，不采用 C# + Gir.Core

RFC #8 的 D1 曾建议先按 **A（C# + Gir.Core）** 做 Phase 0，理由是能**进程内复用**
`Monica.Core` 与 `Monica.Data`。**该建议已作废，方向定为 B（Rust + gtk4-rs）**：

- 落地的 Phase 0 是 Rust，绑定锁定 `gtk4 0.10.3`（`v4_14`）/ `libadwaita 0.8.1`
  （`v1_5`）。Fedora 44 的 GTK 4.22 / libadwaita 1.9 足以支持 RFC 里写的
  `gtk4 0.11` / `libadwaita 0.9`，但那条路需要 rustc 1.92+，且 Ubuntu 24.04 编不过。
- **代价已经兑现：核心层在 Rust 侧重写，不进程内复用 .NET 的 Core/Data。**
  `monica-vault` crate 直接 git 依赖上游 `Monica-Pass/Mdbx` 的 `mdbx-storage`
  （rev `d1d3cc4`），**不使用**冻结线那份 UniFFI `libmdbx_ffi.so`。
- 这么选的收益是不必让主密码或明文跨进程传递 —— 对密码管理器而言，跨进程调用
  .NET 后端是安全减分项。
- 两条线**不做 ABI 对齐**。同一种 `local.mdbx` 两边都能读，但本分支对既有库只做
  **只读**检查；一旦发现需要从 `MDBX-1` 升到 `MDBX-2`，会拒绝就地打开并要求先复制或备份。

决议与实测证据记录在
[issue #8 的 D1 评论](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8#issuecomment-5707570676)。

## 架构

```mermaid
flowchart TB
    Android["Monica Android\n功能与安全基线"] --> Contract["共享产品契约"]
    MdbxUp["Monica-Pass/Mdbx\ncanonical vault 格式与兼容路线"]

    subgraph Gtk["GTK4 线（本分支）"]
        direction TB
        Adw["GTK4 + libadwaita\n系统主题 / fontconfig / portal"] --> GtkApp["monica-gtk\nAdwApplicationWindow / SplitView"]
        Contract --> GtkApp
        GtkApp --> VaultCrate["monica-vault\nVaultRuntime 会话 / 条目 CRUD / TOTP / 生成器"]
        VaultCrate --> Storage["上游 mdbx-storage\nrev d1d3cc4"]
    end

    Frozen["avalonia-frozen\n.NET / Avalonia（冻结）"] --> UniFfi["UniFFI libmdbx_ffi.so\n冻结于 fdf3382"]
    Contract --> Frozen
    UniFfi --> MdbxUp
    Storage --> MdbxUp
```

| 路径 | 职责 |
| --- | --- |
| `monica-gtk/crates/monica-gtk` | GTK4 / libadwaita 外壳：解锁 / 建库、密码 / 笔记 / 钱包 / TOTP、生成器、时间线、回收站与归档、剪贴板超时清除、空闲自动锁定 |
| `monica-gtk/crates/monica-vault` | Rust 侧 vault 封装：`VaultRuntime` 会话、各类型条目 CRUD、TOTP 码、生成器、只读 `inspect_migration`，以及 `secrecy` + `zeroize` |

`monica-gtk/README.md` 记录了 Phase 0–2 的实测边界（哪些做到、哪些被上游 API 挡住），是了解当前进度最
准确的一份材料。

## 构建与运行

完整依赖、验证环境与限制见 [`monica-gtk/README.md`](monica-gtk/README.md)。Ubuntu 24.04：

```bash
sudo apt-get install -y build-essential pkg-config clang \
  libgtk-4-dev libadwaita-1-dev libglib2.0-dev fonts-noto-cjk
```

Fedora 用：

```bash
sudo dnf install -y rust cargo gcc clang \
  gtk4-devel libadwaita-devel glib2-devel \
  google-noto-sans-cjk-fonts
```

两者都需要 **Rust 1.86+**。在 `monica-gtk/` 下：

```bash
cargo build --workspace
cargo test --workspace
cargo run -p monica-gtk -- --self-test   # 无 GUI：创建/解锁/登录·笔记·钱包·TOTP/锁定
cargo run -p monica-gtk                  # 解锁与密码库界面
```

CJK 渲染依赖 fontconfig 能解析到中文字形；不要在应用里指定 Segoe UI / 微软雅黑。

## 测试与 CI

```bash
cd monica-gtk
cargo test --workspace
```

CI 是 [`.github/workflows/check-gtk.yml`](.github/workflows/check-gtk.yml)：在
`ubuntu-24.04` 上装 GTK4 / libadwaita 开发包，跑 `cargo build --workspace --locked` 与
`cargo test --workspace --locked`。选 24.04 而不是 `ubuntu-latest` 是因为绑定锁定
`v4_14` / `v1_5`，正好对应 24.04 的 GTK 4.14.5 与 libadwaita 1.5.0。

尚未接入的部分：打包产物、gettext 键集校验、lint 与 MSRV（1.86）自动验证 —— 按 Phase 5
计划补齐。当前**没有任何自动化测试覆盖真实窗口渲染**，Phase 0 / Phase 1 的窗口验证是人工完成的。

## 冻结线

`avalonia-frozen` 分支保留 `.NET 10 + Avalonia 12 + FluentAvalonia` 实现，**只接受安全
修复**，并且仍是当前唯一能产出可分发包的路径。冻结点 tag：`avalonia-final`。

两条线的边界、安全修复流程、以及「**修复不会在两条线之间自动传播**」这项维护成本，写在
[FROZEN.md](FROZEN.md) 里，改动冻结线前请先读它。

## 浏览器扩展

[`browser-extension/`](browser-extension/README.md) 包含用于本地开发和配对验证的
Chrome/Edge Manifest V3 扩展，通过仅回环地址的令牌桥接与应用通信。协议见
[浏览器桥接协议](docs/browser-bridge-protocol.md)。

扩展本身与 toolkit 无关，两条线共用；但**桥接服务端目前只在冻结线实现**，本分支要等
Phase 3 / Phase 4 才有对应实现。

## 项目关系

- [Monica](https://github.com/Monica-Pass/Monica)：Android 主应用、产品与安全基线。
- [MDBX](https://github.com/Monica-Pass/Mdbx)：Monica 的本地优先 vault 格式与长期兼容路线。
- **Monica by Linux（本仓库）**：仅维护 Linux 桌面的 Monica 密码库实现。分支划分见上文
  「分支布局」。上游原仓库为
  [Monica-Pass/Monica-by-Avalonia](https://github.com/Monica-Pass/Monica-by-Avalonia)。

## 致谢

本项目使用或参考 GTK4、libadwaita、gtk4-rs、libadwaita-rs、rusqlite、secrecy、zeroize，
以及冻结线上的 Avalonia、FluentAvalonia、Bitwarden、KeePass、QRCoder、ZXing、Otp.NET、
Bouncy Castle、Dapper 和 Microsoft Graph 等开源生态。具体许可与声明见
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。

## 许可证

本项目基于 [GNU General Public License v3.0](LICENSE) 开源发布。
