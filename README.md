# xross-interop-lab

私有研究仓库（private lab）。存放互操作协议的来源清单、研究档案、实验证据与决策记录。
本仓库**不放**可发布的产品实现——实现属于独立的 `xross-interop` 仓库，产品集成属于 `xross-dev`（见下）。

## 仓库边界

| 仓库 | 可见性 | 职责 |
|---|---|---|
| `xross-interop-lab`（本仓库） | private | 来源清单、研究笔记/dossier、设备记录、脱敏抓包、实验 POC、决策与证据，**以及实现 workspace（`impl/`）** |
| `xross-dev` | 产品仓库 | XROSS 身份、设备、Shelf、传输/媒体产品与集成 adapter |

设计包原规划的独立实现仓 `xross-interop` **暂不创建**：实现代码先在本仓库 `impl/` 内演进；
未来是否开源由用户决定，届时把许可清理过的 `impl/` 子集复制成新仓库。
私有仓库与新 Git 历史都不是版权豁免：所有第三方源码使用必须保留真实 provenance，规则见 [AGENTS.md](AGENTS.md)。

## 目录结构

```
impl/                           # 实现 workspace（基础阶段创建；plans/01 的代码路径映射到此处）
references/repositories.json    # 来源清单（T01；复制自设计包 manifests/，64 项，均未放行）
references/sources.lock.json    # 本机实际 HEAD + 许可文件 hash（resolve_lock 生成，2026-09-15）
references/clone-plan-2026-09-15.sh  # 实际执行的克隆脚本（shallow、default branch、63 项）
references/external/<slug>      # → /Volumes/Portable2TB/ExtDev/others/<slug> 的 symlink（不入库）
references/research/
  xross-interop-plan-2026-09-15/  # 设计输入包（MASTER-PLAN / docs / plans / manifests / tools）
docs/goal-phase-1.md            # 基础阶段（T01–T14）goal prompt
docs/research/                  # 自由格式研究笔记（如 airplay-research.md）
research/                       # 逐 profile dossier 与来源 intake（T02/T04 产出）
evidence/                       # run manifest、probe 记录、脱敏证据
decisions/                      # 来源 allowlist、provider 采纳决议、ADR
provenance/                     # approved-inputs、review log
specs-reviewed/                 # 审查后的 wire spec
```

参考源码放在仓库外 `/Volumes/Portable2TB/ExtDev/others/`（= `~/Dev/others`），以 slug 名
（如 `r01-uxplay`）浅克隆 default branch，共 63 项约 1.6 GB；R41（AOSP frameworks/av）因入口
未核验且为巨仓而跳过。本仓库不 vendor 第三方源码树。

## 设计输入包

[`references/research/xross-interop-plan-2026-09-15/`](references/research/xross-interop-plan-2026-09-15/README.md)
是本仓库的规划/研究工作包：14 篇规格文档、6 卷实施计划（T01–T80）、38 个互操作 profile、
278 条验收 case 与来源管理工具。开工顺序按包内 [README](references/research/xross-interop-plan-2026-09-15/README.md)
的"先读什么"，第一阶段任务见 [docs/12 交接约定](references/research/xross-interop-plan-2026-09-15/docs/12-ai-handoff-and-working-agreement.md)。

## 证据等级

`catalogued → source-reviewed → build-verified → simulated → device-verified → release-qualified`；
受阻任务标 `blocked` 并写明障碍与重评条件。README 声明、模拟自通、未固定 commit 一律不升级证据等级。

## 当前状态

- **AirPlay 接收**：[docs/research/airplay-research.md](docs/research/airplay-research.md) —— Gate 1 供应链审计已过；Gate 2 真机兼容进行中（transient 视频链路已通，画质/音频待解决）；Gate 3/4 未开始。
- **基础任务 T01（lab 侧）**：仓库边界、目录骨架、.gitignore、来源清单已就位。
- **参考源码（T02 输入）**：63 项已浅克隆到 `~/Dev/others/<slug>` 并 symlink 至 `references/external/`；HEAD 与许可文件 hash 已锁入 `references/sources.lock.json`（R41 missing 为预期）。尚未做逐仓库许可/build 入口审查。
- **T02–T14 及全部协议实现**：未开始。基础阶段执行提示词见 [docs/goal-phase-1.md](docs/goal-phase-1.md)。
- 实现 workspace 在本仓库 `impl/` 下，由基础阶段 T0 创建；独立实现仓 `xross-interop` 暂不建。

AI agent 的工作规则、禁止事项与汇报格式见 [AGENTS.md](AGENTS.md)。
