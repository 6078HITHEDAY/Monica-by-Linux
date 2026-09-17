# 冻结线维护策略

`avalonia-frozen` 分支是 `.NET 10 + Avalonia 12 + FluentAvalonia` 实现线的最终冻结状态。
本文件定义这条线的边界、维护流程与检查清单。

| 项 | 值 |
| --- | --- |
| 分支 | `avalonia-frozen` |
| 冻结点 tag | `avalonia-final` |
| 分叉基线提交 | `74d4948`（两条分支的共同祖先） |
| 活跃开发线 | `main`（Rust + GTK4 / libadwaita） |

## 边界

| 允许 | 不允许 |
| --- | --- |
| 安全修复（漏洞、密钥处理、认证、解析器崩溃） | 新功能、新工作区、新导入格式 |
| 修复数据损坏或迁移错误 | UI / 交互重设计、主题与字体改动 |
| 依赖的安全升级与 CVE 修补 | 依赖的功能性升级、框架大版本跃迁 |
| 修好构建与打包，使其在当前形态下继续可用 | 把 `monica by avalonia/` 迁进 `main` |

判断标准很简单：**如果改动会改变用户可见的行为或界面，它就不属于这条线。** 拿不准就开
issue 讨论，不要直接提交。

## 为什么这条线保留完整 CI

`.github/workflows/` 里的三个 workflow 全部硬绑 `PROJECT_DIR: monica by avalonia`，它们
**只存在于 `avalonia-frozen`**。`main` 上的同名 workflow 已移除，取而代之的是
`.github/workflows/check-gtk.yml`。

保留的理由很直接：**安全修复必须还能出包。** 在 GTK4 线到达 Phase 5（能打 Flatpak / RPM /
deb）之前，这条线是唯一能产出可分发 Linux 包的路径。

| workflow | 触发器 | 说明 |
| --- | --- | --- |
| `check.yml` | `pull_request` / `push` → **`avalonia-frozen`** | 商业质量门。触发器已从 `main` 改为本分支，否则在冻结线上永远不会跑 |
| `build.yml` | `pull_request` / `push` → **`avalonia-frozen`** | 三平台构建（已裁剪为 Linux 面） |
| `release.yml` | 仅 `workflow_dispatch` | 手动触发，不受分支限制；从本分支派发即可 |

不要为了「统一」而把 GTK4 的 cargo 步骤塞进这三个 workflow —— 两条线的工具链与门禁不同。

### 依赖更新自动化

Dependabot **只读取默认分支（`main`）上的 `.github/dependabot.yml`**，非默认分支上的同名
文件会被忽略。所以本分支上的 `.github/dependabot.yml`（继承自拆分之前）**是无效配置**，
保留它只是为了记录历史；真正生效的冻结线条目是 `main` 上那两个带
`target-branch: avalonia-frozen` 的 entry。

维护时注意：**不要在 `main` 的配置里给冻结线加 `ignore` 规则**。Dependabot 的 `ignore`
与安全更新之间的交互在不同生态下不一致，加规则有可能把本该出的安全更新一起挡掉。冻结策略
由人工 review 把关，不依赖 bot 配置。

## 安全修复流程

1. 从本分支切出 `security/<slug>`。
2. 改动 + 针对该缺陷的回归测试（`Monica.Tests` 或 `Monica.UiTests`）。
3. 跑质量门：

   ```powershell
   cd ".\monica by avalonia"
   .\eng\ci\verify-commercial-release.ps1 -Configuration Release
   ```

4. PR 到 **`avalonia-frozen`**（不是 `main`）。PR 描述里写清受影响的版本与是否需要出包。
5. 合并后按需用 `release.yml` 发草稿 Release；tag 建议 `v0.1.x-avalonia`。

### 修复不会自动进入 `main`

`main` 上的 Rust 侧是**重写**而非移植，没有共享代码路径。所以：

- 同一条安全修复**不会**通过合并传播到 `main`。
- 如果该缺陷在 GTK4 线同样存在，需要在 `main` 上单独修一次，并在两边 PR 里互相引用。
- 反过来也一样：`main` 上的修复不会回流到本分支。

这是接受这笔重构代价时一并接受的维护成本，不是疏漏。

## 已知且接受的边界

- **两条线不做 ABI 对齐。** 本分支使用冻结的 UniFFI `libmdbx_ffi.so`（绑定提交
  `fdf3382`，uniffi 锁定 `=0.29.4`）；`main` 直接 git 依赖上游 `mdbx-storage`。
  同一种 `local.mdbx` 两边都能读，但 GTK4 线对既有库只做只读检查，遇到需要
  `MDBX-1` → `MDBX-2` 升级时会拒绝就地打开并要求先复制备份。
- **`docs/release-readiness.md` 里的测试计数产出于 2026-07-26**，早于 Linux-only 改造
  （`f9fa194`）与 GTK4 Phase 0（`4c88009`）。文档已如实标注来源，但发布前仍必须重跑质量门
  刷新计数，不要把历史数字当作当前状态。
- **`monica-gtk/` 在本分支内只是冻结当时的快照**，不再更新。

## 分支保护建议（远端管理员设置，仓库文件无法代劳）

仓库文件不能开关分支保护，以下是需要人工在 GitHub 上落实的项：

- `avalonia-frozen`：禁止直接 push；要求 PR + `check.yml` 通过。
- `main`：禁止直接 push；要求 PR + `check-gtk.yml` 通过。
- **不要删除 `avalonia-frozen`**：它承载当前唯一可出包的实现线。
- `avalonia-final` tag 不要移动；将来的新冻结点另打 tag。
- 安全公告（Security advisory）与私密漏洞上报路径应同时覆盖两条分支。

## 相关

- [RFC #8：Linux 端重构起点：改用 GTK4 + libadwaita](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8)
- [D1 决议评论（采用 Rust，含实测证据）](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/8#issuecomment-5707570676)
- [Issue #7：Linux 桌面版功能性问题汇总](https://github.com/6078HITHEDAY/Monica-by-Linux/issues/7)
