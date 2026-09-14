```json xross-dossier
{
  "profile_id": "cast.media-control",
  "catalog_id": "M08",
  "priority": "P1",
  "priority_note": "sender+control；receiver app（CAF/Web Receiver）与本 profile 分开",
  "status": "catalogued",
  "capability_status": "not-implemented",
  "directions": {
    "send": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r44-pychromecast", "page-r42-openscreen", "page-s07-cast-getting-started"],
      "notes": "本节点选择本地文件 → 有限 HTTP URL → Chromecast 拉取（U3）；load/play/pause/seek/stop"
    },
    "control": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r44-pychromecast", "page-r45-protocol"],
      "notes": "CASTV2 通道上的 media namespace 控制；sender controller 不自动拥有本地 media 服务"
    },
    "receive": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r43-castreceiver", "page-s09-web-receiver", "page-s08-registration"],
      "notes": "stock sender → 本节点需要 receiver app（CAF/自定义）+ 设备认证——独立 gate，非本 profile 首验收"
    }
  },
  "sources": [
    {"id": "R42", "name": "chromium/openscreen", "commit": "29634014c987", "license": "BSD 风格/混合（逐文件核）", "usage": "libcast/CASTV2/流式公开实现首要参考"},
    {"id": "R43", "name": "googlecast/CastReceiver", "commit": "ddfb06c7fb94", "license": "Apache-2.0", "usage": "CAF receiver 语义参考"},
    {"id": "R44", "name": "home-assistant-libs/pychromecast", "commit": "5cfdb60783f0", "license": "MIT", "usage": "discovery/launch/media 控制 sender 参考"},
    {"id": "R45", "name": "cast-web/protocol", "commit": "c906d2108cbf", "license": "未完成文件级核验（restricted）", "usage": "TLS/protobuf channel 交叉参考；只作事实"},
    {"id": "S07", "name": "Google Cast getting started 文档", "commit": "developers.google.com（2026-09-15 读）", "license": "Google 文档条款", "usage": "sender/receiver 平台区别"},
    {"id": "S08", "name": "Google Cast registration 文档", "commit": "同上", "license": "同上", "usage": "app ID/设备注册 ≠ 通用硬件授权"}
  ],
  "unknown_fields": [
    {"field": "CASTV2 鉴权（DeviceAuth flattop）对第三方 sender 的实际强制点（哪些接收端要求、哪些默认 media receiver 可用）", "probe": "P-M08-1"},
    {"field": "默认 media receiver（CC1AD845 类 app ID）的可用性与 codec/DRM 边界", "probe": "P-M08-1"},
    {"field": "openscreen cast streaming（M09 范围）与本 profile 控制面的接口边界", "probe": "P-M08-2"},
    {"field": "discovery（mDNS _googlecast._tcp TXT）字段在本网络环境的真实值集", "probe": "P-M08-3"}
  ],
  "probes": [
    {"id": "P-M08-1", "question": "lab Chromecast（具名型号）接受无认证 sender 的条件与 media 命令集？", "method": "批准 lab 网段 + pychromecast 对照抓包（脱敏）；不做认证绕过", "status": "planned"},
    {"id": "P-M08-2", "question": "openscreen 的 sender/streaming/receiver 组件边界？", "method": "source-review R42 cast/ 目录（固定 commit）", "status": "planned"},
    {"id": "P-M08-3", "question": "mDNS TXT 字段矩阵（设备名/型号/能力位）？", "method": "lab 环境 dns-sd 枚举 + 脱敏记录", "status": "planned"}
  ]
}
```

# Protocol Dossier — cast.media-control（M08）

## A. Scope

- **Profile**：Google Cast media control / URL **sender**（send + control）。
- **系统入口**：Chrome/Android 的"投放"菜单；本节点作为第三方 sender 经 CASTV2。
- **场景**：U3——本地文件 → 本节点有限 HTTP URL lease → TV 拉取；load/play/pause/seek/stop；stop 撤销令牌。
- **排除**：real-time streaming source（M09 独立 profile，不是"多一个 URL 字段"）；本节点做 receiver（receive 方向单列，依赖 receiver app + 认证 gate，不承诺）；DRM 内容。
- **品牌辨析**：Google Cast ≠ Chromecast 内置 app ≠ Android TV renderer ≠ DLNA。

## B. Sources and provenance

见机器头，commit 与 lock 一致（R45 restricted：只作事实交叉参考）。R42 openscreen 来自 googlesource（T01 时已由 GitHub 镜像锁 commit 29634014）。允许进入实现的信息：协议事实 + openscreen 公开源码（BSD 系，逐文件核后可复用表达）。

## C. Wire layers

| 层 | 已知（catalogued） | 待证 |
|---|---|---|
| discovery | mDNS `_googlecast._tcp`；TXT 含设备 UUID/型号/能力 | 本环境字段矩阵（P-M08-3） |
| transport | CASTV2：TLS + protobuf 消息通道（source/receiver/sender 结构图见 R45/R42） | 通道细节 source-review（P-M08-2） |
| 认证 | 官方 sender 走设备认证/配对；**第三方可通信 ≠ 通过官方认证**（R45 边界）；默认 media receiver 的可用性 | 强制点实测（P-M08-1） |
| 控制 | namespace 消息（CONNECT/CLOSE、media: load/play/pause/seek/stop、GET_STATUS） | 字段集对照（P-M08-1） |
| payload | receiver 按 URL 拉取（本节点有限 HTTP URL lease：短时、单资源、stop 撤销——docs/04 MediaResource + FILE-08） | codec/格式接受集（P-M08-1） |
| 平台 | receiver app（CAF/Web Receiver）运行条件（S09）；app ID/注册（S08）——开发注册≠通用授权 | receiver 方向独立评估 |

## D. Message/state map

send 方向投影（docs/05 §4 媒体状态机）：requested → authorizing（URL lease 签发）→ negotiating（launch app + connect）→ buffering（load + BUFFERING）→ active（PLAYING/PAUSED；seek 按 receiver 能力）→ draining（STOP + 撤销 URL lease）→ ended。字段级 map 待 P-M08-1/2。

## E. Platform contract

- 我方 sender 平台：macOS/Windows/Linux headless 无障碍（纯网络栈 + 本节点 HTTP 服务）。
- URL lease 依赖本节点 HTTP 网关（类似 xross-dev files-http 模式，T03 §6）。
- 无特殊系统权限；网络出站 + 本地监听。

## F. Compatibility matrix

| 对端 | 状态 |
|---|---|
| lab Chromecast（型号待登记）| **not-run**（P-M08-1） |
| stock sender → 本节点 receiver | **not-run**（receiver 方向独立 gate，不承诺） |

## G. Security review

- **URL 策略**（docs/07 §5 全适用）：本节点签发的 media URL 短时/单资源/可撤销；不接受 receiver 回传的任意 URL；TLS 证书校验。
- 控制消息全部未认证输入：protobuf 边界完整、消息大小/深度预算、未知 namespace 拒绝。
- 设备认证材料：不使用未授权材料；第三方 sender 认证边界如实报告（不伪装官方 sender）。
- S08：不做伪造 app ID / 规避注册的尝试。

## H. Decision

**路线：independent implementation（Rust CASTV2 sender），openscreen（R42）为主参考，pychromecast（MIT）为行为对照。** receiver 方向 vendor-gated（认证/授权不确定项 pending-user-review，T05 正式化）。重评条件：P-M08-1 实测后确认默认 media receiver 路径可行性。

## I. Implementation input release

- 允许：本 dossier、（P-M08-2 后的）specs-reviewed/m08、openscreen 固定 commit 树（BSD 系逐文件核后）。
- 禁止：R45 表达复制（restricted）；伪造认证材料；真实家庭 Cast 设备名入库（脱敏）。
- 未关闭阻塞：P-M08-1/2/3。
- 验收 case：catalog M08（本地文件→有限 URL→Chromecast；load/play/pause/seek/stop）。
- 负责 task：T15+ casting 分卷。
