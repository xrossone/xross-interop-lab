```json xross-dossier
{
  "profile_id": "localsend.v2",
  "catalog_id": "F01",
  "priority": "P0",
  "status": "active-research",
  "capability_status": "on-track",
  "directions": {
    "send": {
      "evidence_level": "source-reviewed",
      "capability_status": "not-implemented",
      "evidence_refs": ["src-r13-protocol", "src-r14-localsend", "src-xrossdev-localsend-adapter"]
    },
    "receive": {
      "evidence_level": "source-reviewed",
      "capability_status": "not-implemented",
      "evidence_refs": ["src-r13-protocol", "src-r14-localsend", "src-xrossdev-localsend-adapter"]
    }
  },
  "sources": [
    {"id": "R13", "name": "localsend/protocol（文档）", "commit": "62bd3406ec80", "license": "无 LICENSE（restricted）", "usage": "协议事实引用，逐条注明出处；不复制文本"},
    {"id": "R14", "name": "localsend/localsend（app）", "commit": "230fb6929626", "license": "Apache-2.0", "usage": "行为参考与回归对照"},
    {"id": "xross-dev", "name": "xross-adapter-localsend + vendored localsend-rs", "commit": "xross-dev@099b6c72 / localsend-rs@428981a", "license": "MIT（主仓）", "usage": "source-reviewed 实现事实；复用路线见 T03/T05"}
  ],
  "unknown_fields": [
    {"field": "announce/register 消息在库存客户端各版本间的完整字段集与兼容矩阵（fingerprint/alias/deviceType/download 等可选位）", "probe": "P-F01-1"},
    {"field": "prepare-upload 的 pin/token 校验细节与 401/403/503 语义在各 app 版本的差异", "probe": "P-F01-2"},
    {"field": "部分接受（选择子集）/会话中途取消/Unicode 与同名文件在库存 macOS/Android/Windows 客户端上的真实行为", "probe": "P-F01-3"},
    {"field": "v2 与 v3（WebRTC 草案）边界——本 profile 明确排除 v3", "probe": null}
  ],
  "probes": [
    {"id": "P-F01-1", "question": "库存 sender 的 announce/register 字段集与 r13 文档的差异？", "method": "批准的 lab 网段内抓包（脱敏）+ 对照 r13 README 与 vendored localsend-rs 解析器", "status": "planned"},
    {"id": "P-F01-2", "question": "upload token 校验与拒绝码语义？", "method": "读 r14 app 源码 + xross-dev adapter consent.rs（400 vs 403）对照", "status": "planned"},
    {"id": "P-F01-3", "question": "目录/首选项 acceptance case（拒绝、部分选择、取消、Unicode、同名）在库存端表现？", "method": "真机矩阵（catalog F01 验收）+ xross-dev tests/localsend_over_the_wire.rs 作回归门槛", "status": "planned"}
  ]
}
```

# Protocol Dossier — localsend.v2（F01）

## A. Scope

- **Profile**：LocalSend v2.x 局域网文件互传；**方向**：send + receive（P0）。
- **系统入口**：各平台 LocalSend app（iOS/Android/macOS/Windows/Linux）；无系统级入口。
- **场景**：手机/电脑 ↔ 本节点的文件/文本双向；排除：v3 WebRTC 草案、互联网中继、把 UDP 发现"改正"为仅 mDNS（catalog 明确禁止）。
- **品牌辨析**：LocalSend ≠ Snapdrop/PairDrop（后者是浏览器 WebRTC 系）；协议默认 UDP 多播 + HTTP(S)，非 mDNS。

## B. Sources and provenance

见机器可读头 `sources`。全部 3 个来源 commit 与 `references/sources.lock.json` 一致（R13/R14），xross-dev 锚定 099b6c72。R13 无 LICENSE → **restricted**：只允许事实引用（端点、字段名、默认端口等），不允许复制文档文本进实现。允许进入实现的信息范围：wire 字段/语义/状态机事实；不引入其 UI 代码。

## C. Wire layers（source-reviewed；每条注明来源）

| 层 | 事实 | 来源 |
|---|---|---|
| discovery | UDP 多播，默认组 `224.0.0.0/24`（Android 兼容原因），默认端口 `53317`；announce JSON 含 alias/deviceModel/fingerprint/port/download 等；**多播失败不致命**——有 /register 的 TCP/SSE 旁路 | r13 README §2/§3.1；adapter discovery.rs |
| transport | HTTPS（自签名证书，fingerprint 经 announce 分发）优先；HTTP 明文用于浏览器 Web Share（r13 README §"unencrypted" 说明） | r13；adapter server.rs（rustls） |
| 控制端点 | `POST /api/localsend/v2/register`（显式加入会话）；`POST /api/localsend/v2/prepare-upload`（发方→收方元信息清单，收方逐文件 accept/reject → sessionId+tokens）；`POST /api/localsend/v2/upload?sessionId&fileId&token`（逐文件字节流）；`POST /api/localsend/v2/cancel?sessionId` | r13 README §3.2-§3.6；adapter server.rs:799-809 |
| 认证 | 无账号；设备 fingerprint（证书哈希）为身份；pin 可选（prepare-upload 返回 401 时 pin 流程） | r13；adapter consent.rs |
| 状态机 | discovered → offered(prepare-upload) → 部分接受 → transferring(upload×N) → finished/cancelled；收方主导文件级决策 | r13；docs/05 §4 投影 |
| 取消 | 任一侧 cancel 端点；token 失效即断 | r13 §3.6 |
| resume | 协议本身无断点续传 → 本 profile **不宣称 resume**（FILE-07） | r13（无 resume 章节的缺席事实） |

## D. Message/state map

方向单列：**send**（我方为 sender）：discover → prepare-upload（收方裁决）→ upload 逐文件 → 完成；**receive**（我方为 receiver）：announce/register → 收 prepare-upload → `manifest_for` 生成内部清单 → ConsentTable（interop 投影为 ShareOffer）→ 逐文件落盘 → 确认。字段级 map 待 P-F01-1/2 后写入 specs-reviewed；**不复制 vendored 代码结构**，由协议事实 + 独立测试约束。

## E. Platform contract

- 我方目标平台：macOS/Windows/Linux headless 优先（U4 场景）。
- 权限：本地网络多播（macOS local-network TCC）、监听端口的防火墙放行；无特殊硬件。
- 平台 probe 项：多播是否可用（企业 AP 常禁）→ 不可用时走 /register 直连扫描（adapter scan.rs 模式）。

## F. Compatibility matrix

| 对端 | 状态 |
|---|---|
| 库存 LocalSend app（macOS/Android/Windows 各 1） | **not-run**——真机矩阵按 catalog 验收执行（P-F01-3） |
| xross-dev 自身（vendored localsend-rs） | 主仓 6 个回归测试为门槛（localsend_over_the_wire 等）——本 lab 未重跑，不声称 |

## G. Security review

- 未认证输入：register/prepare-upload 的 JSON（限制 1 MiB/10000 entries，docs/01 §8）；multipart 边界；文件名路径穿越（T08 interop-file 防线，与协议无关地强制）。
- 自签名 TLS：指纹 pinning 语义；明文 HTTP 仅显式开启。
- URL/子进程/decoder：不适用（无 URL fetch、无媒体解码）。

## H. Decision

**路线：reuse（主仓 xross-adapter-localsend 模式）+ 独立契约投影**（T02/T03 已定）。理由：协议实现已在主仓成熟并有回归；interop 层做 Offer/审批/落盘契约投影（ShareOffer → prepare-upload 清单）。重评条件：若 vendored localsend-rs 升级破坏主仓回归，interop 层不受影响（经契约解耦）。

## I. Implementation input release

- 允许：本 dossier、specs-reviewed/f01-localsend.md（固化后）、r13 事实引用（注明 commit）、mock fixture（自制）。
- 禁止：R13 文档文本复制、vendored 代码复制进 impl/（复用走主仓 adapter，不 vendor）。
- 未关闭阻塞：P-F01-1/2/3。
- 验收 case：catalog F01 首个可判定 probe（三平台双向 + 拒绝/部分/取消/Unicode/同名）。
- 负责 task：T15+ provider 接入分卷。
