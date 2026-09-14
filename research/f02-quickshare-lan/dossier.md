```json xross-dossier
{
  "profile_id": "quickshare.lan.v1",
  "catalog_id": "F02",
  "priority": "P1",
  "status": "catalogued",
  "capability_status": "not-implemented",
  "directions": {
    "receive": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["src-r15-neardrop", "src-r16-bada", "src-r17-nearby"],
      "notes": "首验收方向：Android stock Quick Share → 本节点（catalog F02 首个可判定 probe）"
    },
    "send": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["src-r17-nearby", "src-r19-rquickshare"],
      "notes": "Mac→Android 需二维码/显式可发现；确认码一致性未验证"
    }
  },
  "sources": [
    {"id": "R15", "name": "grishka/NearDrop", "commit": "8f1fdc7d5cbc", "license": "Unlicense", "usage": "macOS 实现参考（协议事实提取）"},
    {"id": "R16", "name": "kyujin-cho/Bada", "commit": "9fba4ee696b5", "license": "无根 LICENSE（restricted）", "usage": "standalone Android 移植的结构参考；模块级许可待核"},
    {"id": "R17", "name": "google/nearby", "commit": "2ea517eae15d", "license": "Apache-2.0", "usage": "Nearby Connections 官方参考（含 ukey2 submodule）"},
    {"id": "R18", "name": "google/ukey2", "commit": "10fc737aa901", "license": "Apache-2.0 + NOTICE", "usage": "握手规范/实现参考"},
    {"id": "R19", "name": "Martichou/rquickshare", "commit": "378d8ae96994", "license": "GPL-3.0", "usage": "Rust 结构参考（core_lib/core_bin）；GPL 边界见 T05"}
  ],
  "unknown_fields": [
    {"field": "LAN 发现的确切机制与消息（mDNS 服务类型/TXT、或 Nearby 自有广播）在 stock Android 当前版本的形态", "probe": "P-F02-1"},
    {"field": "UKEY2 握手参数 → 会话密钥 → 传输层加密的绑定方式（与 HTTP 端口/端点的关系）", "probe": "P-F02-2"},
    {"field": "可见性模式（所有人/联系人/隐藏）对 LAN 无账号接收的可达性影响矩阵", "probe": "P-F02-3"},
    {"field": "确认码（PIN）生成与显示的一致性语义", "probe": "P-F02-2"}
  ],
  "probes": [
    {"id": "P-F02-1", "question": "stock Android Quick Share 在 LAN 用什么发现？", "method": "批准 lab 网段抓包（脱敏）+ 对照 r15/r17 发现模块源码", "status": "planned"},
    {"id": "P-F02-2", "question": "UKEY2 握手到加密传输的完整绑定链？", "method": "source-review r18 spec/docs + r19 core_lib wire 结构 + r15 接收路径", "status": "planned"},
    {"id": "P-F02-3", "question": "可见性/接收模式矩阵？", "method": "真机矩阵（两台 Android 不同设置 × 本节点）", "status": "planned"}
  ]
}
```

# Protocol Dossier — quickshare.lan.v1（F02）

## A. Scope

- **Profile**：Google Quick Share 局域网/二维码形态（**global Quick Share**；不覆盖中国互传/华为 Share——不同服务，catalog 明确不合并）。
- **方向**：receive P1（Android stock → 本节点）；send（显式可发现/二维码）随后。
- **场景**：Android 系统分享菜单 → 本节点接收（U1 场景）；10 GiB 级流式受控传输（catalog 验收）。
- **排除**：BLE/Wi-Fi P2P 链路升级（F03 独立 profile）；互联网 token 中继； contacts-only 语义假装可复现。

## B. Sources and provenance

见机器头；全部 commit 与 lock 一致。R16 **restricted**（无根 LICENSE）；R19 GPL（结构参考边界见 T05 决议）。允许进入实现的信息：wire 结构/状态机事实（注明来源）；R15/R17/R18 为宽松许可但仍按组件审查后才可复用表达。

## C. Wire layers

**状态：catalogued——以下为已知组件级事实，字段级留待 P-F02-*：**

| 层 | 已知 | 待证 |
|---|---|---|
| discovery | Quick Share 基于 Nearby Connections 栈；LAN 形态存在（NearDrop/rquickshare 均实现局域互传）；BLE 广播参与可见性（F03 范围） | 当前 stock 版本的 LAN 发现确切载体（P-F02-1） |
| transport | HTTP(S) 端点承载传输（r15/r19 结构暗示）；Wi-Fi LAN 直连 | 端点/端口/路径（P-F02-2） |
| 认证 | UKEY2（R18）：设备间握手派生会话密钥；确认码一致性 | 握手→传输绑定（P-F02-2） |
| 控制 | offer/accept 模型；接收端确认 | 消息字段集 |
| payload | 流式分块；大文件（10GiB）受控传输 | 分块/校验细节 |
| resume | 未知 | 不宣称 |

## D. Message/state map

投影到 docs/05 §4 文件状态机；provider 记录 wire_phase。字段级 map 待 P-F02-2 关闭。**在 probe 关闭前，实现任务不得根据计划标题创造字节格式（plans/01 执行方式）**。

## E. Platform contract

- stock Android：系统分享入口，无需本方 SDK；可见性模式由对端控制。
- 我方接收平台：macOS/Windows/Linux headless。
- r35 universal-miracast-sink 等 root/privileged 路线**不进入**本 profile（gated）。

## F. Compatibility matrix

| 对端 | 状态 |
|---|---|
| stock Android（Pixel/Samsung 各 1）→ 本节点 | **not-run**（P-F02-1/2/3 关闭后按 catalog 验收执行） |

## G. Security review

- UKEY2 会话密钥与 Xross 身份严格分离（SEC-01：配对成功 ≠ 可访问 Shell/Vault）。
- 未知来源 offer 默认拒绝；10GiB 流式受控传输走 interop-file 预算/T08 防线。
- 抓包脱敏（SEC-08）：P-F02-1 的 pcap 不入库（captures/ ignored，脱敏摘录进 evidence/）。

## H. Decision

**路线：independent implementation（Rust core），参考 R15/R17/R18 事实 + R19 结构**（T05 正式化）。GPL 边界：rquickshare 只作架构参考（core_lib/core_bin 分层思想），不复制表达；若未来需要直接复用 → 独立 worker 进程 + 独立发布单元。重评条件：P-F02-1/2 关闭形成 wire spec 后。

## I. Implementation input release

- 允许：本 dossier、T02 intake 事实、（P-F02-2 关闭后的）specs-reviewed/f02。
- 禁止：R16 全部表达（restricted）；R19 代码复制；stock 抓包原文入库。
- 未关闭阻塞：P-F02-1/2/3——实现停在 probe 前。
- 验收 case：catalog F02（Android→Mac 收件；Mac→Android 二维码；确认码一致；10GiB 流式）。
- 负责 task：T15+ files 分卷。
