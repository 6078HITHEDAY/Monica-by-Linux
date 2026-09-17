# 发布就绪与证据矩阵（冻结的 Avalonia 树）

文档修订日期：2026-09-14
验证快照产出：2026-07-26（见文末「当前验证快照」）

本文件区分五种状态，避免把“代码已存在”“自动化测试通过”“已冻结待重写”和
“可以公开分发”混为一谈：

- **已验证**：当前实现存在，并有源代码、自动化测试或工作流证据。
- **平台受限**：能力边界已明确，不能宣称与 Android 系统集成完全等价。
- **实验性**：可以构建或测试，但不是默认受支持的发布路径。
- **外部待完成**：需要证书、商店、真实设备或 GitHub 管理员权限，仓库代码不能替代。
- **已冻结 / 待重写**：现状可运行，但已被 GTK4 重构（issue #8）取代，只接受安全修复。
  下文标为「已验证」的行大量属于此类 —— 它们描述的是当前可发布的那条线，
  **不是长期产品方向**。

## 适用范围

本矩阵覆盖**冻结的 Avalonia 树**（`monica by avalonia/`），也就是当前唯一能产出可分发
Linux 包的实现线。GTK4 树（`monica-gtk/`）**不在本矩阵范围内**：它只完成 Phase 0，没有
打包产物，也未接入任何 CI。重构方向、Phase 0–5 阶段表与 D1（Rust）决策记录见仓库
`README.md`。

本仓库仅维护 **Linux 桌面**发布面；不再构建或声明 Windows / macOS 产物。

## 产品与平台基线

| 要求 | 状态 | 实现证据 | 测试或决策证据 |
| --- | --- | --- | --- |
| Android 是功能与安全基线，桌面按 FluentAvalonia 任务布局 | **已冻结 / 待重写** | `README.md`、`src/Monica.App/Features/` | `UiArchitectureTests.cs` 及各工作区 Headless 测试；GTK4 重构（issue #8）已把交互基准改为 libadwaita |
| 密码与笔记支持嵌套分类 | 已验证 | `LocalCategoryPath`、密码/笔记目录投影与管理命令 | `LocalCategoryPathTests.cs`、`SecureNoteTests.cs` |
| Bitwarden 在线账户双向同步 | 已验证 | `Core/Bitwarden`、`Data/Bitwarden`、`Platform/Bitwarden`、同步工作区 | Bitwarden protocol、authentication、transport、merge、queue、conflict 和 UI 测试 |
| 浏览器本地配对与站点凭据查询 | 已验证 | `LoopbackBrowserBridgeService`、Manifest V3 扩展 | `BrowserBridgeServiceTests.cs`、`DesktopIntegrationUiTests.cs`、协议文档 |
| Linux 托盘与 Secret Service 设置加密 | 已验证 | `AvaloniaTrayService`、`LinuxSecretProtector`、`libsecret` | `PlatformServiceTests.cs`；GNOME 托盘可能需要 AppIndicator 扩展 |
| Android 钱包类型的桌面等价实现 | 已验证 | `ExtendedWalletItemData.cs`、钱包编辑器和详情投影 | `WalletParityTests.cs`、`WalletWorkflowUiTests.cs` |
| Linux 原生 passkey | 平台受限 / Unsupported | `NativePasskeyService.cs` 仅能力目录 stub | `PlatformServiceTests.cs`、`native-passkey-boundary.md`；不充当系统 Credential Provider |
| 全局快捷键 | 平台受限 | `CapabilityOnlyGlobalHotkeyService`；设置页可见但不可用 | `DesktopIntegrationUiTests.cs`（按 `CanUseGlobalHotkeyIntegration` 分支） |
| 截图保护 | 平台受限 | `DisabledWindowPrivacyService`；设置开关按 `window-security` 禁用 | `AppSettingsTests.WindowCapture.cs` |

## 数据与安全边界

| 控制 | 状态 | 实现证据 | 测试证据 |
| --- | --- | --- | --- |
| canonical vault 真源 | 已验证 | `MdbxBackedMonicaRepository`、`MdbxVaultStore`、`CanonicalVaultBootstrapService` | `MdbxRepositoryTests.cs`、`MdbxUniffiBindingTests.cs`、`SmokeVaultSeedTests.cs` |
| 解锁期 native handle 复用与锁定释放 | 已验证 | `MdbxVaultStore.Session.cs`、`VaultSessionService` | MDBX session/lease 测试及 dispatcher responsiveness 测试 |
| 主密码、短期密钥和账户秘密生命周期 | 已验证 | vault credential、Bitwarden secret container、lock-aware session manager | `VaultCredentialTests.cs`、Bitwarden account/session 测试 |
| 剪贴板最小暴露 | 已验证 | `SecureClipboardService` 的所有权检查与定时清除 | `SecurityBaselineTests.cs` 和 clipboard lifecycle 测试 |
| 后台敏感状态释放 | 已验证 | 最小化时释放工作区、详情、预热编辑器和可重建缓存 | `BackgroundMemoryUiTests.cs`、`BackgroundSensitiveDetailUiTests.cs`、`BackgroundTransientSecretUiTests.cs` |
| 浏览器桥接隔离 | 已验证 | IPv4 loopback、256 位会话令牌、HTTPS origin/extension caller 校验 | `BrowserBridgeServiceTests.cs`、`browser-bridge-protocol.md` |
| Bitwarden 网络与密码学限制 | 已验证 | HTTPS endpoint policy、KDF 上限、Type 2 authenticated CipherString、固定时间 MAC 校验 | `BitwardenProtocolTests.cs`、network authentication 和 transport 测试 |
| 导入、同步和设置失败时不泄露秘密 | 已验证 | 错误净化、临时状态清理、原子设置持久化 | `*FailureSecurity.cs`、`AppSettingsTests.AtomicPersistence.cs` |

## 桌面体验、性能与可维护性

| 维度 | 状态 | 当前证据 | 剩余边界 |
| --- | --- | --- | --- |
| FluentAvalonia 风格任务布局 | **已冻结 / 待重写** | 密码、笔记、动态口令、钱包、安全分析、同步、设置等拆分工作区及真实截图 | 随 GTK4 重写整体作废；GTK4 侧只完成 Phase 0 骨架，信息层级与视觉审查待重建 |
| 键盘与基础辅助功能 | 已验证（自动化范围） | focusable command、AutomationProperties、live region 和焦点释放测试 | 屏幕阅读器、高对比度和系统缩放仍需真实 Linux 人工验收 |
| 本地化 | 已验证（自动化范围） | 中英文 localization service、语言持久化和界面绑定 | 仍需逐页人工校对截断、术语和复数规则 |
| 冷启动与首次导航 | 已验证（当前预算） | `ColdStartupPerformanceTests.cs`、延迟工作区物化和编辑器预热 | 必须在发布硬件上继续记录真实启动、解锁和大 vault 指标 |
| MDBX UI 响应性 | 已验证 | blocking UniFFI 工作移出 Avalonia dispatcher | `MdbxUiResponsivenessTests.cs` |
| 后台内存 | 已验证（行为） | 最小化释放可重建视觉树、投影和图片缓存 | 自动化验证对象可回收，不替代多小时进程 RSS soak test |
| 功能拆分 | 已验证 | 商业质量门限制重点功能源文件不超过 300 行 | 跨功能共享规则必须继续下沉到 Core/Data/Platform |

真实 AppHost 截图烟雾测试使用临时 canonical MDBX vault，验证了 26 个密码、
14 个笔记、1 个 TOTP、2 个钱包项目和 12 个页面截图。一次审计中的 vault 加载为
约 2.97 秒；该数据只用于诊断，不是跨硬件发布 SLO。

## 构建、发布与供应链

| 项目 | 状态 | 证据或边界 |
| --- | --- | --- |
| 统一商业质量门 | 已验证 | `eng/ci/verify-commercial-release.ps1` 在 Ubuntu 上执行卫生、文件体积、格式、漏洞、零警告构建、核心和 Headless UI 测试 |
| JIT 桌面包 | 已验证（默认） | Build/Release 工作流仅覆盖 `ubuntu-latest` + `linux-x64`；Release 默认 `jit` |
| Linux `.deb` / `.rpm` / AppImage / Flatpak | 已验证（工作流） | `package-linux-deb.sh`、`package-linux-rpm.sh`、`package-linux-appimage.sh`、`package-linux-flatpak.sh`；Release Linux job 上传四类产物 |
| Portable `tar.gz` | 已验证（工作流） | `pack-portable.ps1` 仅产出 `.tar.gz` |
| NativeAOT 包 | 实验性 | CI 保留 AOT 构建信号，但 Release 输入明确标为 experimental，且不再默认选择 |
| Action 供应链固定 | 已验证 | 所有第三方 Action 固定完整 commit SHA，checkout 不保留凭据 |
| 依赖更新 | 已验证（配置） | `.github/dependabot.yml` 每周检查 GitHub Actions 与 NuGet |
| 产物校验 | 已验证（工作流） | Draft Release 生成 `SHA256SUMS` 并执行 GitHub build provenance attestation |
| Release 可见性 | 已验证（限制） | 工作流移除非草稿输入并硬编码 `draft: true` |
| Linux 仓库签名 | 外部待完成 | 当前生成 `.deb` / `.rpm` / AppImage / Flatpak，没有发行仓库元数据和仓库签名 |
| Linux 人工验收 | 外部待完成 | 安装、升级、卸载、窗口管理、辅助技术和真实硬件性能需在目标发行版执行 |

## 远端 GitHub 安全设置

正式公开发布前，仓库管理员应明确批准并配置分支保护、必需状态检查、漏洞警报、秘密扫描、
推送保护、Dependabot security updates，以及组织允许的 Action 策略。这些是 GitHub 管理员
设置，不属于普通代码提交。

## 当前验证快照

以下数字来自 **2026-09-17 的一次真实 CI 运行**（本分支提交 `93a075bb`，run
[#35176040915](https://github.com/6078HITHEDAY/Monica-by-Linux/actions/runs/35176040915)），
可以直接点开核对，不再是手抄的历史值。

- `624/624` 个核心与集成测试通过（`Monica.Tests`）。
- 全部 Headless UI 套件通过。
- Release 构建 `0 warning / 0 error`。

### 与旧数字的差异

本文件此前写的 `630/630` 产出于 2026-07-26，而且 2026-09-14 那次提交只改了上面的「文档
修订日期」标签、**没有重跑质量门**。6 处的差值来自 Linux-only 改造（`f9fa194`）移除的
Windows 专属测试。

### 已知不稳定用例

`Monica.UiTests.BackgroundMemoryUiTests` 断言 GC 最终回收，曾以约 100 ms 的预算在共享
runner 上偶发失败 —— 例如 `main` 上 `74d4948` 的 run
[#35173906375](https://github.com/6078HITHEDAY/Monica-by-Linux/actions/runs/35173906375)。
**同一份代码在 `avalonia-frozen` 上通过**，说明是时序 flake 而不是回归。

已提交修复（加强 GC + 扩充预算 + 独立进程）。在修复合并并复跑多次之前，仍应把该用例的失败
视为可能的 flake 而非确定回归；若它在修复后仍重现，需要重新评估而不是直接重跑过关。

该快照只证明特定提交的自动化证据，不替代发布前在目标 Linux 发行版上的安装与人工验收。
