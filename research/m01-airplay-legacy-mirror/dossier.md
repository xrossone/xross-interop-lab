```json xross-dossier
{
  "profile_id": "airplay.legacy-mirror",
  "catalog_id": "M01",
  "priority": "P0→P2（receiver P0，sender P2）",
  "status": "active-research",
  "capability_status": "blocked",
  "directions": {
    "receive": {
      "evidence_level": "device-verified",
      "capability_status": "blocked",
      "evidence_refs": ["device-run-2026-09-15-airplay-poc"],
      "obstacles": [
        "shairplay-rust transient 链路视频画质扭曲：根因未定（openh264 用法 vs 数据 vs 解密），P-M01-1 一锤定音",
        "音频无声：ALAC 流已建立，--resample 后仍无声，P-M01-2",
        "PIN 配对 /pair-setup-pin 未实现（v0.10.0 404）：需按 UxPlay 移植 legacy SRP，P-M01-3"
      ],
      "reeval_conditions": "P-M01-1 分辨数据/解码器责任；音频每秒计数日志定位；pair-setup-pin 移植后 iPad PIN 直连"
    },
    "send": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": [],
      "notes": "反向投 AppleTV 独立子任务（catalog M01：receiver≠sender），本阶段不动"
    }
  },
  "sources": [
    {"id": "R01", "name": "FDH2/UxPlay", "commit": "d9791de116b8", "license": "GPL-3.0（文件级混合）", "usage": "参考 oracle + pair-setup-pin 事实来源（raop_handlers.h:275-430, srp.c）"},
    {"id": "R02", "name": "metaneutrons/shairplay-rust", "commit": "2fb72b30b658", "license": "LGPL-3.0-or-later", "usage": "首选技术路线（Gate 1 审计已过）"},
    {"id": "R10", "name": "openairplay/airplay-spec", "commit": "00063da0e7f1", "license": "无 LICENSE（restricted）", "usage": "事实引用，注明出处"},
    {"id": "internal", "name": "docs/research/airplay-research.md", "commit": "本仓库（2026-09-15）", "license": "自有", "usage": "真机 POC 运行记录与协议实测事实"}
  ],
  "unknown_fields": [
    {"field": "PIN 一次性语义（UxPlay snprintf(pin,6,'%04u',pin%10000); if(pin<10000) pin=0 的'一次性'解读待证）", "probe": "P-M01-4"},
    {"field": "视频流跨包 AES-CTR 计数器连续性 / 多 slice 帧边界处理", "probe": "P-M01-1"},
    {"field": "音频 ALAC 解密输出是否进入 ring（解密失败是否被静默吞）", "probe": "P-M01-2"},
    {"field": "iPhone 各机型/OS 版本的 feature 位差异（transient vs PIN vs HomeKit）", "probe": "P-M01-5"},
    {"field": "FairPlay/DRM 后续阶段（本 profile 不声称、不触碰）", "probe": null}
  ],
  "probes": [
    {"id": "P-M01-1", "question": "视频扭曲是数据坏还是解码器弱？", "method": "NAL 落盘 .h264 → ffplay -f h264 播放 A/B（airplay-research §7 P1 建议 4）", "status": "planned"},
    {"id": "P-M01-2", "question": "音频在 ring→cpal 哪一段丢失？", "method": "AudioSession 回调加每秒计数日志 + RUST_LOG=shairplay=trace（§7 P2）", "status": "planned"},
    {"id": "P-M01-3", "question": "/pair-setup-pin（binary plist 三步 legacy SRP-SHA1）移植后 iPad PIN 配对能否走通？", "method": "按 UxPlay raop_handlers.h:275-430 移植；SRP 会话状态跨 TCP 连接存活（实测每请求换连接）", "status": "planned"},
    {"id": "P-M01-4", "question": "PIN 是否一次性？", "method": "配对成功后同 PIN 二次连接实验 + UxPlay 代码复读", "status": "planned"},
    {"id": "P-M01-5", "question": "feature 位/配对模式在 iPhone/iPad/Mac 各版本上的矩阵？", "method": "20 次重连 + 具名设备矩阵（catalog 验收）", "status": "planned"}
  ]
}
```

# Protocol Dossier — airplay.legacy-mirror（M01）

## A. Scope

- **Profile**：AirPlay legacy screen mirroring（接收 P0；发送 P2 独立子任务）。
- **系统入口**：iOS/iPadOS 控制中心"屏幕镜像"；macOS AirPlay 接收（本机自带 Receiver 会抢占同名广播，测试需避开——见运行记录 §3.1 ⚠️）。
- **场景**：iPhone/iPad/Mac → 本节点受控画面+声音；20 次重连稳定性；排除：DRM/FairPlay 内容录制、反向发送（另验收）、AirPlay 2 多房间（M03）。
- **品牌辨析**：legacy mirror（本 profile）≠ AirPlay 2 audio（M03）≠ RAOP classic audio（M02）≠ AirPlay media URL（M04）。

## B. Sources and provenance

见机器头。关键折算说明（goal 要求）：
- **device-run-2026-09-15-airplay-poc**（device-verified）：iPad Pro 12.9（iPad13,8, AirPlay/960.13.1）↔ MacBook Pro（macOS 26.6 arm64 有线）。UxPlay 基线：镜像视频成功（用户 Terminal 启动 + GST_MACOS=ON），音频无声；shairplay-rust transient：直连成功、视频窗口打开（1442x1080↔1920x1080 随旋转），画质扭曲、无声。**这是 link-level device-verified + capability blocked**，不等于"AirPlay 接收已可用"。
- **user-observed-uxplay-iphone-mirror**（user-observation，catalogued）：设计包记录的用户观察，未固定版本，不可升级证据。
- UxPlay 双 clone 说明：lab 管理克隆 d9791de；POC 用用户自有 clone 44024fe（含 GHSA-479c-ww7g-wgp8 修复）。引用行号以 44024fe 树为准。

## C. Wire layers（实测+source-reviewed；来源 airplay-research.md §5 与 UxPlay/shairplay-rust 源码）

| 层 | 事实 | 证据 |
|---|---|---|
| discovery | mDNS `_airplay._tcp` + `_raop._tcp`；TXT 特性位决定模式；transient 模式广播 UxPlay 兼容 legacy 位（features.rs 有测试钉死） | 实测（dns-sd 验证广播）+ src |
| 认证（transient） | PIN-less transient pairing：iPad 跳过配对直接 `/fp-setup` | 实测走通 |
| 认证（PIN） | `POST /pair-pin-start` → `POST /pair-setup-pin`：**binary plist 三步 legacy SRP-SHA1**（Step1 实测 86 字节 `{method:"pin",user:<device_id>}` → `{pk,salt}`；Step2 `{pk,proof}` → `{proof}`（20B SHA1 系）；Step3 `{epk(32B ed25519),authTag(16B GCM)}`）。**不是 HomeKit TLV** | UxPlay raop_handlers.h:275-430 + 实测字节 |
| transport | RTSP :5001；**iPad 每个 RTSP 请求换新 TCP 连接**（实测 /info、pair-* 全不同源端口）→ 配对/SRP 状态必须跨连接存活 | 实测 |
| 视频 | SETUP stream_type=110 → TCP；128 字节帧头（payload_len u32 LE + type u16 + timestamp u64 @8-15）；AES-128-CTR（密钥从 audio aeskey 三段推导）；AvcC ≈30B，旋转/切换时重发；库交付 NAL，解码归宿主 | 实测（日志+分辨率变化） |
| 音频 | SETUP stream_type=96 → Legacy ALAC（AES-CBC ekey），44100Hz；RTP UDP（use_udp）；`/audioMode default`；`/feedback` 每 2s 心跳 | 实测 |
| 取消/重连 | stop 回收缓冲（待实现验证）；重连矩阵 = P-M01-5 | 计划 |

## D. Message/state map

接收方向：broadcast(mDNS) → [配对：transient 直通 | PIN 三步] → `/fp-setup` → SETUP(110 video)+SETUP(96 audio) → streaming（视频 TCP 帧 + 音频 RTP）→ format change（旋转 → AvcC 重发，format_id 递增）→ stop。provider 记录 `wire_phase`，投影到 docs/05 §4 媒体状态机（requested→authorizing→negotiating→buffering→active→draining→ended）。字段级状态表在 P-M01-1/2/3 关闭后固化进 specs-reviewed。

## E. Platform contract

- macOS 接收：GStreamer 管线需要 NSApplication 主线程（GST_MACOS wrapper 经验，airplay-research §3.1）——headless 模式必须用 null/file sink，不开窗口（MEDIA-04）。
- 无需特殊系统权限；但与系统自带 AirPlay Receiver 冲突（同名广播）需 probe 提示。
- 发送方向（P2）：需 CoreMedia/MediaSource 权限，另立 dossier，本档不管。

## F. Compatibility matrix

| 对端 | 结果 | 证据 |
|---|---|---|
| iPad Pro 12.9 (iPad13,8, AirPlay/960.13.1) → UxPlay @44024fe | 视频✅ 音频❌ | device-run-2026-09-15（§4.1） |
| 同上 → shairplay-rust v0.10.0 transient | 直连✅ 视频链路✅ 画质❌ 音频❌ | device-run-2026-09-15（§4.2） |
| 同上 → shairplay-rust PIN 模式 | ❌（/pair-setup-pin 404，iPad 无限重试 M1） | device-run-2026-09-15（§4.2 时间线） |
| iPhone（未固定版本）→ UxPlay | ✅（用户观察，unpinned） | user-observed-uxplay-iphone-mirror |

## G. Security review

- 未认证输入：RTSP 请求（认证前可达——UxPlay GHSA-479c-ww7g-wgp8 教训：1.73.7 前认证前路径有洞）；binary plist 解析（深度/大小预算）；RTTSP/帧头长度字段（128B 头的 payload_len 校验）。
- 密钥材料：airport.key = 领域公开 RSA key（非机密，audit 已确认）；不使用任何未授权设备认证材料（FairPlay 不碰）。
- decoder：openh264/H.265 损坏码流处理独立预算（docs/07 §5）。
- 供应链：shairplay-rust Gate 1 审计已过（291 包全 crates.io、无 build.rs、deny.toml、CI pin SHA——airplay-research §2）。

## H. Decision

**路线：shairplay-rust（LGPL）独立 provider worker + UxPlay 作为外部参考 oracle**（与 T05 决议一致）。理由：Gate 1 已过、链路已通、剩解码/音频/pin 三件事各有明确 probe。独立进程隔离 LGPL 与未来 FairPlay 边界。重评条件：P-M01-1/2/3 关闭后升 simulated→真机矩阵。

## I. Implementation input release

- 允许：本 dossier、airplay-research.md、specs-reviewed/m01（固化后）、shairplay-rust（LGPL worker 单元）、UxPlay 行级事实引用（注明 commit+行）。
- 禁止：UxPlay/PlayFair 代码复制进 impl/；FairPlay 常量/表；未固定 commit 的声明。
- 未关闭阻塞：P-M01-1（画质）、P-M01-2（音频）、P-M01-3（PIN）——实现任务在这三项关闭前**停在 probe 阶段**，不输出虚假 completed（Gate D1）。
- 验收 case：catalog M01（iPhone/iPad/Mac→PC 受控画面声音；20 次重连）。
- 负责 task：T15+ media 分卷。
