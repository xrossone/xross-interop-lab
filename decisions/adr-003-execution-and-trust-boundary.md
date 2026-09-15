# ADR-003：执行与信任边界（外部协议引擎默认不在 xrossd 内执行）

- **Status**：accepted（2026-09-15 用户采纳）
- **Date / owner / affected task-profile IDs**：2026-09-15；提出 = agent（依据用户转达的架构评审），
  裁决 = 用户；影响本仓 T11/T12/T13（已排期）、T30/T21（首个真机 vertical）、全部后续 provider 接入与
  deployment class 划分；profiles：F01/F02/M01/M05/M07/M08。
- **Context**：基础阶段期末留下的开放问题——interop 进产品时作为第一方角色（`xross.control.v1` 全权）
  还是独立进程（`xross.client.v1` scoped actions）？支持本决议的事实：①xrossd 持有设备身份、vault、
  SSH/远程输入、文件与 shell 等敏感能力（`research/xross-top-level-design.md` §3/§4）；②外来协议输入
  （RTSP、binary plist、protobuf、MPEG-TS/RTP、crypto framing、H.264/H.265、私有 TLV）是受攻击面最大的
  解析器，AirPlay parser RCE 若落在 authority process 内，爆炸半径直指 vault/文件/shell/输入；
  ③主仓分层已把外来协议定为 `adapter/` 隔离区、明文"一个设备一个 daemon"、能力平面
  "a peer never reaches a head"；④T05 已判定 provider 级 worker 走外部进程（许可与崩溃隔离双理由）。
  二选一的提法把"产品集成形态"与"执行隔离"两个正交维度压成了一个，任何一边都必须被牺牲。
- **Decision**：
  1. **产品集成 = 第一方**：interop 进产品时以 xrossd 内的第一方 interop bridge/head 形式提供能力；
     bridge 是产品语义落点（foreign event → canonical intent），**协议名到此为止**。
  2. **协议执行 = 隔离 worker**：外部协议引擎默认不在 xrossd 进程内执行；由 supervisor 以固定
     binary hash/参数白名单、parent pipe bootstrap、清环境与无关 FD、有限重启的方式运行低权限 worker。
  3. **seam 只用 scoped 能力面**：worker 与 interop 组件对 Xross 的接入目标是 `xross.client.v1`
     （scoped actions）；client.v1 落地前，过渡期只允许受限凭据 + HostPorts 等效 scoped 面，
     **禁止使用 `xross.control.v1` 全权**。
  4. **deployment class A/B/C 三分**（新 provider 必须先在 `decisions/provider-adoption.json` 声明）：
     **A** = in-process trusted adapter（许可干净、parser 足够硬化、集成收益明确、攻击面可接受；
     LocalSend 已 production 化，grandfathered，不因架构统一而强制外移）；
     **B** = sandboxed native provider（多数未来项：AirPlay/QuickShare/Miracast userspace core，隔离 worker）；
     **C** = platform-owned provider（Windows MiracastReceiver API、未来厂商 SDK：经 capability bridge 调
     系统能力，不重写系统网络层）。
  5. **迁移方向**：单个 provider 经安全/许可/运维复核后可迁回 in-process；默认不整体内置。
  6. **不承诺**：进程边界 ≠ sandbox。隔离等级以实测证据为准并如实标注（macOS 无真 sandbox 时标
     `process-only`，并据此限制可发布 profile）。
- **Alternatives**：(a) 全第一方 in-process —— 否决：parser RCE 直连 authority 能力，爆炸半径不可接受；
  (b) bridge 也出进程、全独立 —— 否决：device directory/consent/transfer history/notification 会各自为政，
  形成第二套 Xross，违反一设备一 daemon 与 capability plane，且控制面本就是第一方专有；
  (c) 维持二选一 —— 否决：牺牲产品集成或牺牲隔离，二者不可兼得；
  (d) 为 interop 新建 daemon 身份 —— 否决：north-star §6.4 一设备一 daemon。
- **Authority/security**：worker 不持有 identity/vault/fabric 凭据，只碰临时 media/协议会话；worker 与
  bridge 之间只传结构化事件（canonical intent）。越权形状一律视为缺陷：worker 请求控制面全权、
  worker 自建身份或监听、bridge 把协议原始字节转发给内部服务。
- **Licensing/provenance**：本决议不改变任何 provider 的许可结论（`production_approved` 全 false 保持）；
  GPL/LGPL 的组合边界仍由独立发布单元承担（UxPlay 外部二进制、GStreamer 插件清单）。
- **Compatibility/migration**：LocalSend 主仓 adapter 不受影响（class A grandfathered）；本仓 `impl/`
  现有 standalone 形态即"已拆分形态"的可运行证明；本决议不要求立即改造 T07/T09/T10 既有契约，
  只约束新增 provider 的进程与凭据形状；对主仓 N/N−1 无接口影响（interop.api/0.1 不变）。
- **Tests**：T13-01..04（canary / 任意 shell / 重启风暴 / 无孤儿）证明 worker 边界与隔离等级；
  T11-01（同名同 IP 不同 identity 不合并）证明 bridge 不吞协议身份；T4 模拟闭环断言"无 control 面全权
  调用、scoped 面默认拒绝"。真机 worker 隔离在 T30 复验。
- **Rollback**：单 provider 迁回 in-process 需新开 ADR 并附安全/许可复核；整体撤销本决议（回到二选一）
  会波及 worker supervisor、T11 registry、T30 provider 形状——属 supersede 级变更，不允许原地改结论。
- **Evidence**：用户 2026-09-15 采纳原话（「采纳，你可以自己set goal然后开始」，记录于
  `provenance/review-log.md`）；来源评审要点与逐条核对见 `docs/goal-phase-2.md` T0；事实锚
  `research/xross-top-level-design.md`、`decisions/xross-contract-baseline.json`；
  provider 分类落 `decisions/provider-adoption.json`。
