# Xross 顶层设计对齐简报（供 interop 集成参考）

**读于**：xross-dev `a483bd14`（branch content-plane-0044，2026-09-15；只读）
**读了什么**：`docs/handbook/00-north-star.md`、`01-architecture.md`、`09-the-capability-plane.md`、
`status.md`（LocalSend 相关行）、`GO.md`、`docs/product/00-feature-inventory.md` 与
`docs/research/2026-09-09-feature-catalog-reconciliation.md`（互操作相关条目）
**目的**：本 lab 的 interop 工作最终要落进 Xross；先记下总项目的边界、机制与既有裁决，
避免我们设计出与主仓架构冲突的形状。**本文不是对主仓 API 的引用**，行号/符号以该 commit 为准。

## 1. 权威链（scope 归属）

- north-star 首页明示：**"A REFERENCE, NOT AN AUTHORITY"**（用户 2026-08-27 决定：以用户想做的为准；
  north-star 只是 MVP 参考）；§3 的"加一删一"规则已于 2026-08-28 由用户废止——**scope 是用户的**。
- 含义（修正 T03 baseline 的 SC-1）：主仓 MVP 把 media codecs/投屏划出**不构成**对 interop 计划的否决；
  也不构成许可——媒体协议是否进 Xross 产品，需要用户为它开新的 scope 行（见 §6）。
- `01-architecture.md` 自我标注 **AUTHORITATIVE**（从属于 north-star）；两者冲突按文档各自的更新日期与
  用户裁决处理。**我们引用时优先 architecture + ADR，north-star 用于理解"为什么被划出去"。**

## 2. 分层与类别规则（interop 代码将来放哪）

```
L4 clients → L3 platform head（能力提供者角色）→ L2 services（产品语义；adapter=外来协议隔离区）
           → L1b runtime（policy：trust/grants/registry/leases）→ L1a fabric（mechanism，冻结）
```

- 依赖类别由 `scripts/check-boundaries.ts` 机械强制：`adapter` 可依赖 `contract`+`service`；
  **外来协议（LocalSend、以及将来的 Quick Share/AirPlay/Cast provider）的合法位置是 `adapter/` 或独立组件**，
  不得让 fabric 学会任何协议名词（L1b 明令不得出现 "clipboard/offer/PTY" 等产品名词）。
- **命名空间 `xross.*` 保留给第一方签名系统服务**，第三方注册会被 registry 拒绝——interop 契约
  （我们已用 `interop.api/0.1`）不得占用 `xross.*`。
- **一个设备一个 daemon，谁都不许嵌第二套 fabric**（§6.4）。interopd 不能自建 iroh/身份；
  集成模式下只能作为角色接入 xrossd（或经公开 seam 调用）。

## 3. 本地 API 是两个契约（细化 T03 的"唯一 seam"）

| 契约 | 给谁 | 内容 |
|---|---|---|
| `xross.control.v1` | **仅第一方管理**，"never handed to another app" | health/status/diagnostics/trust/revoke/… 135 RPC |
| `xross.client.v1` | **公开 seam** | peer handles、`CallPeer`/`CallPeerStream`（动作由 registry 声明，daemon 自己构造 peer 请求，不转发调用者字节） |

- 凭据按 capability 字符串细分（ADR 0013）：持 client 凭据可读 peers 但不能 revoke。
- **对 interop 的含义**：T03 记录的"XrossHostAdapter = ControlServiceClient"应按形态细分——
  - 若 interop 作为**第一方角色**随产品分发（composition root 之一）→ 可用 control.v1 全权；
  - 若作为**独立进程/第三方形态**（当前 lab 的 standalone 主打）→ 应走 `xross.client.v1` 的
    scoped action seam，或等用户为它定产品形态。**这是一个待用户/架构裁决点**，不自行选定。
- 客户端不持有协议逻辑的方针（L4/§5）与我们的"CLI/headless 权威、Playground 只是消费者"一致。

## 4. 能力平面（媒体/原生能力的唯一合法形状）

- **Plane A（control，operator intent）** vs **Plane B（capability，native 供给）**；
  **"A peer never reaches a head"**：peer → 已授权 service → bridge → head；bridge frame
  **没有 principal 字段**（构造上没有地方放 peer 身份）。
- 能力提供者是**角色**（Swift/C#/Rust 均可），head 是**进程拓扑**，bridge 只是跨进程时的 adapter；
  服务只认 **capability port**（transport-free trait），桌面/手机同一份服务代码。
- 一条总规则：**"a native API is always behind an FFI boundary owned by Rust."**
- **对 interop 的含义**：AirPlay/Miracast 等需要原生解码/音频/屏幕的能力，将来若进 Xross，
  必须走 capability port + head/bridge 形状（或 interop 自带 native 渲染但经 FFI 由 Rust 拥有边界），
  而不是让 provider 自建 socket/身份。我们 standalone 形态的 native player 计划与此兼容。

## 5. 媒体/屏幕的现有边界（决定 interop 媒体如何落进产品）

north-star §4（多处按日期收窄，均用户裁决）现行边界：
- **仍然禁止**：remote desktop、屏幕**流**给 peer、**codec 输出上线**（live 或录制回放都算换帽子的 remote desktop）；
- **已放行**：本机采集/控制作为 L3 能力（截图工具/capture-allow 门控下的**单帧**可过 fabric）、
  **只在采集机上写文件的**本地录制（`xross.record.v1` 无 action-class byte，peer 根本到不了）；成品
  `.mp4` 的**普通文件传输**就是文件传输，不属此禁令。
- **对 interop 的含义**：AirPlay 镜像到我们节点 = 外部发送方 → 我们**本地呈现/落盘**；把镜像内容经
  Xross fabric 转发给另一 peer 属于"codec 输出上线"的禁止形状 → 首批 interop 媒体**离线于 fabric**
  （与 T03 的 `blocked-dependency + mock host 先行` 结论一致，且现在有了主仓成文依据）。

## 6. LocalSend 在总项目里的真实位置（回答"跟 localsend-rs 什么关系"）

- **M5 = LocalSend interop 是 Ship Gate 行**（north-star §3b）：与 stock LocalSend 双向发现/收发 v2
  + 合并设备目录。**这是产品能力，不是参考资料**——`vendors/localsend-rs` 是它的实现库（用户写的那份，
  在 xross-dev 里被 vendor 并使用）。
- status.md 显示 M5 大部分已落（T0/T1/T4）：adapter wire+server+send（`LanSender`）、consent 进
  `TransferReceiver::ask_about_foreign_offer`、`localsend` feature 默认开且**要求 transfer**、
  over-the-wire/TLS 部分欠账（TLS、per-interface announce、LAN 半边 listing 的欠账在 backlog）。
- **本 lab 的 F01 与它的关系**：我们**不重复实现**，复用结论不变；但复用目标是**主仓的 adapter**，
  与用户另一项目的独立 `~/Dev/localsend-rs` checkout 无关（已按用户 2026-09-15 澄清移出输入）。
- 旧功能目录 reconciliation 的对照：FC-12 Nearby file transfer = **built**（即 LocalSend 路线）；
  **FC-42 AirPlay compatibility = "decision — reverse-engineering-heavy; probably not-carried"**
  ——即：**主仓自己的帐本没有 carry AirPlay/Miracast/Cast**；interop lab 正是重新打开这些方向的载体。
  将来要进产品，需用户为它们开新的 scope 行（§1 的权威链允许，但必须显式）。

## 7. 设备目录：一个反直觉的主仓裁决（对我们 T11 直接有用）

- 架构 §6 写"discovery is plural; device list is singular"且"一台设备两种发现 = 一行带两个 source"；
  **但 status.md 记录了后来的实测裁决**：`xross-svc-devices` 的测试 `one_list.rs` 明确断言
  **"a machine visible both ways is TWO rows, deliberately"**——因为合并只能靠 alias/IP/model，
  而这三个都是 LAN 上攻击者可自选的，按名字合并会让网段上任何东西继承已登记设备的信任；
  真正的合流需要 LocalSend 响应里带同根签名的 fabric device id（协议改动，不是 lookup）。
  （该行同时记录这与 plan 0007 stage D 的验收原文相矛盾，且矛盾被显式记录而非私了。）
- **对我们 T11 的含义**：interop 的 EndpointId registry 采用同一立场——
  **默认不跨协议合并**；用户显式 alias 只作 UI 用途；可验证身份绑定需要额外授权（与 docs/04 §5 一致）。
- 路由选择是 transfer service 的一个函数（`svc-devices::route.rs`：fabric 优先、LocalSend 兜底、
  denied 设备**拒绝而非降级**）——interop 的路由不另建模型。

## 8. 当前主仓焦点（排期事实，非意见）

- `GO.md`（2026-09-08）：付费后端七件中四件已建，剩 temporary-transfer R2、Paddle billing、operator console。
- 即：**短期内主仓的资源在计费/运营/后端闭环**，interop 集成落地的窗口需要与用户确认；
  这不影响本 lab 的独立推进（standalone 形态先行）。

## 9. 对本 lab 计划的净影响（行动项）

1. T11 endpoint registry：采用"默认不跨协议合并 + 来源单列"（§7），与主仓 `one_list.rs` 的立场一致。
2. T12 platform probe：形状对齐 capability plane（只读 probe、报告 remedy、不自动改系统配置——CORE-09）。
3. 集成 seam 待裁决：第一方角色（control.v1）vs 独立进程（client.v1 scoped actions）——记入待用户裁决清单。
4. interop 媒体首批**不上 fabric**（§5 成文依据），先本地呈现/落盘；将来上产品需用户开 scope 行。
5. 命名继续 `interop.*`，不碰 `xross.*` 保留空间（§2）。
6. F01 复用目标锁定主仓 adapter（§6）；本文与 T03 映射共同构成集成输入。
