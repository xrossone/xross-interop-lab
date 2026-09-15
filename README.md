# xross-interop-lab

私有研究仓库（private lab）。存放互操作协议的来源清单、研究档案、实验证据与决策记录，
以及实现 workspace（`impl/`）。**本仓库不公开、不发布任何产物**（见 AGENTS.md）。

## 仓库边界

| 仓库 | 可见性 | 职责 |
|---|---|---|
| `xross-interop-lab`（本仓库） | private | 来源清单、研究笔记/dossier、设备记录、脱敏抓包、决策与证据，**以及实现 workspace（`impl/`）** |
| `xross-dev` | 产品仓库 | XROSS 身份、设备、Shelf、传输/媒体产品与集成 adapter |

设计包原规划的独立实现仓 `xross-interop` **暂不创建**：实现代码先在本仓库 `impl/` 内演进；
未来是否开源由用户决定，届时把许可清理过的 `impl/` 子集复制成新仓库。
私有仓库与新 Git 历史都不是版权豁免：所有第三方源码使用必须保留真实 provenance，规则见 [AGENTS.md](AGENTS.md)。

## 目录结构

```
impl/                           # 实现 workspace（plans/01 代码路径映射到此处）
  crates/interop-contract/      # 领域契约：ids/capability/offer/media/error + golden + JSON Schema
  crates/interop-policy/        # scoped grant（默认拒绝跨 scope）
  crates/interop-file/          # 受限流式存储：路径形状/预算/原子 commit/hash
  crates/interop-runtime/       # HostPorts + fake host（T07）；
                                # 会话注册表/事件/取消（T10）
  crates/interop-ipc/           # 本地控制面：长度前缀 JSON-RPC + hello 认证
  crates/interop-testkit/       # fake clock/确定性分片/fixture 登记/run manifest
  adapters/standalone-host/     # standalone 显式 policy + 最小权限 host
  apps/interopd/                # headless daemon 骨架（UDS 0600）
  schemas/interop-api.schema.json
references/repositories.json    # 来源清单（64 项，均未放行）
references/sources.lock.json    # 本机实际 HEAD + 许可文件 hash（2026-09-15）
references/external/<slug>      # → /Volumes/Portable2TB/ExtDev/others/<slug> 的 symlink（不入库）
references/research/xross-interop-plan-2026-09-15/  # 设计输入包（勿修改）
docs/goal-phase-1.md            # 基础阶段 goal prompt（已完成）
docs/goal-phase-2.md            # 阶段 2 goal prompt（T11–T13 收尾 + 执行边界 ADR + seam 模拟实证）
docs/research/airplay-research.md  # AirPlay 真机 POC 记录（UxPlay/shairplay-rust）
research/                       # 来源 intake + 逐 profile dossier（T02/T04 产出）
specs-reviewed/                 # 审查后的 wire spec（F01/M01 实质；其余 review-pending）
decisions/                      # source-allowlist / xross-contract-baseline / provider-adoption / ADR（见 decisions/README.md 台账）
provenance/                     # approved-inputs / review-log
evidence/                       # run manifest（run.schema.json 校验）+ intake 扫描
tools/                          # 仓库检查脚本与测试（python3 -m unittest discover -s tools）
```

参考源码放在仓库外 `/Volumes/Portable2TB/ExtDev/others/`，以 slug 名浅克隆 default branch，
共 63 项约 1.6 GB；R41（AOSP frameworks/av）因入口未核验且为巨仓而跳过。本仓库不 vendor 第三方源码树。

## 证据等级

`catalogued → source-reviewed → build-verified → simulated → device-verified → release-qualified`；
受阻任务标 `blocked` 并写明障碍与重评条件。README 声明、模拟自通、未固定 commit 一律不升级证据等级。

## 当前状态（2026-09-15，基础阶段 T01–T14 完成）

**研究与决策（T02–T05，全部完成）**

- **T02 来源 intake**：file-core+airplay 19 项只读扫描（[research/source-intake.md](research/source-intake.md)、
  [decisions/source-allowlist.json](decisions/source-allowlist.json)）；restricted：R10/R13/R16（许可不明）；
  7 个 foreign AGENTS/CLAUDE/.kiro 全部登记为 `untrusted-material`；全部 `production_approved=false`。
- **T03 Xross 集成映射**：[research/xross-integration-map.md](research/xross-integration-map.md) 锚定
  xross-dev `099b6c72`（该仓为活跃工作区，测试按"基线是 HEAD 祖先"校验）；唯一 seam =
  XrossHostAdapter（gRPC/UDS）；**媒体接口 absent → mock host 先行**；LocalSend reuse-first。
- **T04 dossier**：F01/F02/M01/M05/M07/M08 六份（机器可读证据等级/方向/probe，由
  [tools/test_dossiers.py](tools/test_dossiers.py) 强制）；M01 折算 airplay-research.md：
  transient 链路 device-verified 但 `capability_status=blocked`（画质/音频/PIN 三障碍）。
- **T05 许可路线**：[decisions/provider-adoption.json](decisions/provider-adoption.json)——
  UxPlay→worker(外部二进制 oracle/fallback)、shairplay-rust→仅参考（用户实测质量不达标，不链接）、
  LocalSend→reuse（主仓 adapter；用户自有 localsend-rs 属另一项目不引入）、
  QuickShare 参考→independent、GStreamer→worker、WinRT→independent、**FairPlay→永久 vendor-gated**；
  全部 `production_approved=false`（放行权保留给用户）。
- **执行与信任边界（ADR-003，accepted 2026-09-15 用户采纳）**：产品集成 = xrossd 内第一方 interop
  bridge（协议名到此为止）；协议执行 = 隔离低权限 worker（supervisor 管理，只经 scoped 能力面，
  禁用 `control.v1` 全权）；provider 部署类 A/B/C（LocalSend=A grandfathered）。见
  [decisions/adr-003-execution-and-trust-boundary.md](decisions/adr-003-execution-and-trust-boundary.md)
  与 [decisions/README.md](decisions/README.md) 台账。
- **T01/T06–T10/T14 实现**：`impl/` 7 个 crate + 1 adapter + 1 app；
  `cargo test --manifest-path impl/Cargo.toml --workspace` **46 tests 全绿**；
  clippy 0 warnings；覆盖 T01（卫生）、T06（schema/大整数/能力协商）、T07（scoped grant）、
  T08（路径/预算/symlink/原子发布/hash）、T09（分片粘包/超长/hello/版本）、T10（幂等终态/
  bounded replay/worker 隔离）、T14（fixture hash/分级 manifest/脱敏）。
- **AirPlay 接收**：[docs/research/airplay-research.md](docs/research/airplay-research.md) —— Gate 1
  供应链审计已过；Gate 2 真机进行中（transient 视频链路已通，画质/音频/PIN 待解）；Gate 3/4 未开始。

**未开始**：T11（多发现源 endpoint registry）、T12（platform probe/radio lease）、T13（worker
supervisor 真实隔离）、T15+ 协议 provider 实现。阶段 2 的 goal prompt 已就绪：
[docs/goal-phase-2.md](docs/goal-phase-2.md)——T0 执行与信任边界 ADR-003（建议形态为 first-party
bridge + 隔离低权限 worker，**待用户采纳**，见 [decisions/README.md](decisions/README.md) 待裁决清单）、
T1–T3 即上列三项、T4 seam 模拟实证、T5 首个真机 vertical gate。

Agent 工作规则、禁止事项与汇报格式见 [AGENTS.md](AGENTS.md)；基础阶段执行提示词见
[docs/goal-phase-1.md](docs/goal-phase-1.md)（已执行完毕，期末报告见
[evidence/2026-09-15-phase-1-final-report.md](evidence/2026-09-15-phase-1-final-report.md)）。
