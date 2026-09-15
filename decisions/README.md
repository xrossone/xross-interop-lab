# 决议与清单（decisions/）

本目录放**可被审查的决议/清单**，不放讨论稿。写 ADR 用
`references/research/xross-interop-plan-2026-09-15/templates/adr.md` 的 section 结构；
机器可读的清单（allowlist / baseline / provider 表）见下表；ADR 状态由
[tools/test_decision_records.py](../tools/test_decision_records.py) 强制（ADR-01..04）。

## 机器可读清单

| 文件 | 内容 | 状态 |
| --- | --- | --- |
| [source-allowlist.json](source-allowlist.json) | T02 来源 intake 结论；restricted: R10/R13/R16 | 全部 `production_approved=false`（待用户裁决） |
| [provider-adoption.json](provider-adoption.json) | T05 六项 provider 路线（reuse/worker/independent/vendor） | 同上；FairPlay 永久 vendor-gated |
| [xross-contract-baseline.json](xross-contract-baseline.json) | T03 xross-dev 契约锚点（30 条）+ adapter seam | 锚定 xross-dev `099b6c72`；HEAD 已前移，实现前需复核 |

## ADR 台账

| ADR | 决议 | Status |
| --- | --- | --- |
| [adr-003-execution-and-trust-boundary.md](adr-003-execution-and-trust-boundary.md) | 执行与信任边界：产品集成走 xrossd 内第一方 interop bridge（协议名到此为止），协议执行走隔离低权限 worker（只经 scoped 能力面，禁用 control.v1 全权）；provider 分 A/B/C 部署类 | accepted（2026-09-15 用户采纳） |

## 待用户裁决（open）

- **T05 provider 放行**：`provider-adoption.json` / `source-allowlist.json` 中所有
  `production_approved` 仍为 `false`。核心是 F01 LocalSend profile 是否保持 P0 且走 reuse（主仓 adapter）；
  其余（GStreamer 插件矩阵、windows-rs intake 等）按计划推进项。
- **阶段 3 scope 行（首个真机 vertical）**：AirPlay via UxPlay 外部引擎 / Quick Share LAN 接收需要用户
  新开 scope——**清单与理由见 [../research/vertical-gates.md](../research/vertical-gates.md) §3**：
  S1 UxPlay 外部引擎接入、S2 真机互联矩阵、S3 抓包+脱敏、S4 xross-dev bridge 窗口、S5 provider 放行。
  （执行与信任边界本身已裁决：见上表 ADR-003，accepted，不再属于 open。）
- **xross-dev 基线复核**：T03 映射锚定 `099b6c72`，xross-dev HEAD 已前移；
  首次实现对接前复核映射是否仍成立（`tools/test_xross_baseline.py` 会提示）。

## 规则

- 无证据不写决议：结论必须有真实 run/commit/hash 或用户裁决记录（模板 §Evidence）。
- 决议一旦 accepted，改动必须新开 ADR 标 `superseded`，不允许原地改结论；依据不成立的决议
  必须撤回（标 `withdrawn` 并在台账留一行原因），不留在台账里冒充有效。
