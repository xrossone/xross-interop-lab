# 阶段 2 期末报告（T0 + T11–T13 + T4 + T5）

**日期**：2026-09-15　**范围**：`docs/goal-phase-2.md` 全部任务　**格式**：docs/12 六项汇报格式
**结论**：阶段 2 完成定义 5 项全部满足；无未记录的阻塞项。

---

## 1. 当前 task 与 commit，改动文件列表

各 task 单独提交，全部已 push origin（github.com/xrossone/xross-interop-lab）：

| task | commit | 规模 | 主要内容 |
|---|---|---|---|
| 阶段 2 goal | `6c06cf8` | 4 files, +168 | `docs/goal-phase-2.md`（T0–T5）+ decisions 台账/顶层设计简报同步 |
| T0 执行与信任边界 | `aecc2af` | 9 files, +381 | ADR-003（accepted）+ `deployment_class`/`source_role` 字段 + ADR 纪律测试恢复 + review-log |
| T11 endpoint registry | `efdcecb` | 13 files, +1245 | `interop-platform` 新 crate（发现观察）+ registry/路由 + 7 tests |
| T12 platform probe | `adcdddd` | 13 files, +1197 | probe/radio + `xinterop doctor` CLI + `research/platform-probes.md` + 7 tests |
| T13 worker supervisor | `b5d2dd5` | 12 files, +1221 | supervisor（hash pin/沙箱/重启上限/无孤儿）+ `workers/mock-provider` + 6 tests |
| T4 seam 模拟实证 | `d937838` | 6 files, +610 | `adapters/standalone-host/tests/e2e_seam.rs` + 域层 display_name 校验补齐 |
| T5 vertical gate | `6410777` | 3 files, +195 | `research/vertical-gates.md`（A1–A10 + S1–S5 scope 请求） |

合计（`27fb7f3..HEAD`）：**42 files changed, +5007/−24**；`impl/` 增至 8 个 crate + 1 adapter
+ 2 app + 1 测试 worker + `policies/worker-profiles.json`。

## 2. 实际验证命令及 exit code

| 命令 | exit | 结果 |
|---|---|---|
| `cargo test --manifest-path impl/Cargo.toml --workspace` | 0 | **69 tests 全绿**（阶段 1 的 46 + T11 7 + T12 7 + T13 6 + T4 3） |
| `cargo clippy --manifest-path impl/Cargo.toml --workspace --all-targets` | 0 | 0 warnings |
| `python3 -m unittest discover -s tools -p 'test_*.py'` | 0 | **28 tests OK**（24 + ADR-01..04） |
| `cargo run -p interop-cli --bin xinterop -- doctor --json` | 0 | 真机只读 probe（输出存 `research/platform-probes.md`） |
| `cargo test -p interop-runtime --test worker_isolation -- --nocapture` | 0 | 隔离证据：`actual=PlatformSandbox`、`canary=Denied{permission denied}`、pinned sha256 `09e88d0b…`（与 `shasum` 一致） |
| （说明） | — | 各 task 的 red→green 过程（含中途真实失败：T12 借用错误、T13 borrow、T4 域层缺口）见各 run manifest 的 `commands` 数组 |

**pass**：全部上表；**fail**：无遗留；**blocked**：见 §5；
**not-run**：真机互通（需 §5 的 scope 行 S1–S3）、Linux/Windows probe 字段（无目标机）。

## 3. 使用的 source/fixture IDs 与新引入依赖

- **Source IDs**：R01–R12（T5 gate 的来源盘点，全部 `production_approved=false`；R10=restricted）。
  本阶段**未读取任何新第三方源码**：实现只依赖阶段 1 已锁定的仓内材料。
- **Fixture**：全部为测试内人工构造（fake peer/offer/媒体会话；`Fragmenter` 固定 seed 20_260_915 / 7）。
  新增"假秘密"仅一处：T13 canary（测试自造值，非真实凭据；父进程 env 里的假值用于证明环境清洗）。
- **新引入依赖**：`serde`/`serde_json`（interop-platform/interop-runtime/CLI）、`sha2 0.10`
  （interop-runtime 的 worker hash）——三者都已在阶段 1 记录在案，**无新第三方来源、无 git 依赖**。
- **未引入**：任何网络栈、tokio、windows-rs、GPL/LGPL 库（T5 的 UxPlay 接入仍待用户放行）。

## 4. 功能达到的证据层级

**build-verified**：T11/T12/T13 的全部验收 case（真实进程/管道/文件系统；含 sandbox 的真实 OS 拒绝）。
**simulated**：T4 的两条 seam 闭环（fixture wire，非真机、无网络行为）——如实标注，不升级。
**source-reviewed**：T0（用户采纳记录 + 台账）、T5（来源审查与计划）。
**device-verified**：**本阶段没有新增**。唯一真实设备侧事实是 T12 的真机只读 probe 与 T13 的本机
OS 沙箱行为，二者都不构成互操作意义上的 device-verified。
**明确未声称**：任何 provider 可用性、任何协议支持、任何真机互通（A1–A10 全部 not-run）。

## 5. 尚未确认的 wire/平台条件与下一次 probe（blocked 清单）

| 项 | 障碍 | 下一次 probe | 重评条件 |
|---|---|---|---|
| M01 AirPlay 接收 | PIN 未实现（P-M01-3）；画质/音频归因未做（P-M01-1/2 已降为可选诊断） | P-M01-3 独立实现 `/pair-setup-pin`；A1–A10 真机清单 | 需 scope **S1/S2**；关闭后 → simulated → device |
| T30 UxPlay provider 闭环 | 前置 T28/T29 未开始；GPL 组合审查未闭合 | 先做 T28（媒体形态/时钟/frame）→ T29（sink）→ T30 | **S1** 批准 + T28/T29 完成 |
| F02 Quick Share wire | P-F02-1/2/3 全未关闭；P-F02-1 需抓包批准 | 批准后抓包（脱敏，不入库）+ source-review | **S3** + specs-reviewed/f02 固化 |
| M05 Windows/Linux 能力 | 无目标机；本机 macOS 无公开 P2P/WFD API（`unavailable`） | 目标机 `xinterop doctor` | 有 Windows/Linux 环境 |
| xross-dev `client.v1` 落地程度 | T03 时唯一实物正门是 control 面；ADR-003 要求 scoped 面 | 复核主仓 HEAD（已前移至 `9d44800c`） | 首次对接前 |
| provider 放行 | `production_approved` 全 false | 逐 provider 走批准 → 组合审查 → release manifest | **S5** |

**scope 请求（需用户明确批准，详见 `research/vertical-gates.md` §3）**：
S1 UxPlay 外部引擎接入 · S2 真机互联矩阵 · S3 抓包+脱敏 · S4 xross-dev bridge 窗口 · S5 provider 放行。

## 6. 主仓回归/接口影响与 contract owner 裁决项

- **xross-dev 未被修改**（全程只读）。基线仍锚 `099b6c72`，扫描时 HEAD 已在 `9d44800c`——
  `tools/test_xross_baseline.py` 持续提示；首次实现对接前需复核映射（见 §5）。
- **接口影响**：无。本阶段所有产物都在 lab 内（`impl/` 与文档）；interop 契约仍是独立版本面
  （`interop.api/0.1`），未触碰 `xross.*` 命名空间。
- **已由用户裁决**：ADR-003 执行与信任边界（用户 2026-09-15 原话采纳，记录于
  `provenance/review-log.md`；决议见
  [decisions/adr-003-execution-and-trust-boundary.md](../decisions/adr-003-execution-and-trust-boundary.md)）
  ——产品集成走第一方 bridge、协议执行走隔离 worker、provider 分 A/B/C 部署类；
  LocalSend 作为 class A grandfathered。
- **仍需用户裁决**：§5 的五条 scope 行；以及 F01 LocalSend profile 是否保持 P0 与 reuse 路线
  （阶段 1 遗留）。

## 下一阶段建议（阶段 3 首批）

1. **T28 → T29**（媒体形态/时钟/frame；null/file/native sink）——纯本仓实现，**不需要新权限**，
   可在 S1 未批前先行。
2. **T31**（AirPlay 规范/来源/兼容语料 gate 的正规化）——纯研究，可并行。
3. **T30**（UxPlay provider 闭环 + A1–A10）——**待 S1/S2**；这是第一个 device-verified 目标。
4. **T19–T21**（Quick Share LAN 接收）——待 S3 与 F02 固化。
5. 阶段 3 开始时顺手补：T12 的 Linux/Windows probe 字段（有环境时）、`client.v1` 落地复核。

**不是完成报告**：以上均为待办；阶段 2 的"完成"只指 §2 的测试面与本阶段四类产物的落库。
