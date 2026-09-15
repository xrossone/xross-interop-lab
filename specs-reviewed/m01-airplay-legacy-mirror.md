# Reviewed Wire Spec — airplay.legacy-mirror（M01）

**状态**：source-reviewed + 真机部分实测（transient 链路）。**来源**：docs/research/airplay-research.md（实测），
UxPlay @44024fe（pair-setup-pin 事实），shairplay-rust @2fb72b3（事实引用）。
**实现路线（2026-09-15 决议）**：独立实现；UxPlay=外部行为 oracle/fallback；shairplay-rust 仅事实参考（不链接）。
**FairPlay/DRM 不在本 spec 范围。**

## 发现（Discovery）

- mDNS 服务：`_airplay._tcp` + `_raop._tcp`（同时广播）。
- TXT 特性位决定功能协商；**transient 模式广播 UxPlay 兼容 legacy 位**（shairplay-rust `src/net/features.rs` 有 `video_receiver_uses_uxplay_features` 测试钉死该组合）。
- macOS 注意：系统自带 AirPlay Receiver 同名广播干扰（实测记录）。

## 配对（两条路径，实测确认）

1. **transient（PIN-less）**：iPad 跳过配对，直连 `/fp-setup`。（实测走通）
2. **PIN**：`POST /pair-pin-start` → 用户见码 → `POST /pair-setup-pin`，**binary plist 三步 legacy SRP-SHA1**（非 HomeKit TLV）：
   - Step1（实测 86 字节）：请求 dict `{method:"pin"(string), user:"<device_id>"}`；服务端以 4 位 PIN 为 SRP password；响应 `{pk:<data>, salt:<data>}`。
   - Step2：请求 `{pk:<client pubkey>, proof:<client M>}`；SHA1 系 proof=20 字节；响应 `{proof:<20B>}`。
   - Step3：请求 `{epk:<32B ed25519>, authTag:<16B GCM>}`；响应 `{epk, authTag}`。
   - ⚠️ **iPad 每个 RTSP 请求换新 TCP 连接**（实测 /info、pair-pin-start、pair-setup-pin 均不同源端口）→ SRP/配对会话状态必须跨连接存活（UxPlay 用长连接 conn->session；我方按 session/全局单槽设计）。
   - 参考实现：UxPlay `lib/raop_handlers.h:275-430` + `lib/srp.c`（SHA1 系，2023 改造版）。

## 会话建立（实测 legacy 流程）

```
POST /fp-setup → SETUP(stream_type=110, video) → SETUP(stream_type=96, audio) → RTP/镜像流
```

## 视频流（stream_type=110，TCP）

- 帧头 128 字节：`payload_len` u32 LE + `type` u16 + `timestamp` u64 @ bytes 8-15（+ 其余字段待逐字段固化）。
- AES-128-CTR 解密；密钥从 audio aeskey 三段推导（"Stage 3 derivation"，库内日志文案）。
- `PacketKind::{AvcC, HvcC, Payload, Plist, Other}` 分发；**库只交付 NAL，解码归宿主**。
- AvcC ≈ 30 字节，旋转/切换时重发（实测多次）；实测分辨率 1442x1080 ↔ 1920x1080。
- **开放项 P-M01-1**：跨包 CTR 计数器连续性 / 多 slice 帧边界 → 画质扭曲根因未定。

## 音频流（stream_type=96）

- Legacy ALAC（AES-CBC，ekey），44100 Hz（实测固定观察）；RTP UDP（use_udp=true，control_rport）。
- `POST /audioMode mode="default"`；`POST /feedback` 每 2 秒心跳（elapsed_ms 字段单调增——语义待考）。
- **开放项 P-M02 相关**：无声问题未解（resample 已试）。

## 状态机投影（interop 侧）

```
broadcast → [transient 直通 | PIN 三步] → fp-setup → setup(110)+setup(96) → active(视频帧+音频RTP)
                                                        │ 旋转/切换 → AvcC 重发（format_id 递增，不换 session 身份）
                                                        └ stop → draining → ended（回收全部缓冲）
```

## 预算与解析边界（docs/07 §4 强制）

RTSP/plist/帧头长度字段全部先检查后分配；绝对 deadline；认证前输入按不可信处理（UxPlay GHSA-479c-ww7g-wgp8 教训）。

## 待固化（P-M01-1/2/3/4/5）

帧头逐字段表、PIN 一次性语义、20 次重连矩阵、iPhone/iPad/Mac feature 位矩阵。

---

# T31 gate 补充（2026-09-15）：字段级事实表 + 能力声明 + 语料计划

**规则**：下表**每一行的"来源"列必须可复查**（`tools/test_airplay_gate.py` 强制）。来源三种：
`实测 2026-09-15`（本机真机记录 → `docs/research/airplay-research.md`）、
`UxPlay@<commit>:<file>:<line>`（行级事实引用，不复制表达）、
`shairplay-rust@<commit>:<file>`（同上）。**没有来源的字段一律写"待固化"，不得填猜测值。**

## 字段级事实表

### 发现与传输

| 项 | 事实 | 来源 |
|---|---|---|
| mDNS 服务类型 | `_airplay._tcp` + `_raop._tcp` 同时广播 | 实测 2026-09-15（dns-sd） |
| 模式选择 | TXT 特性位决定 transient/PIN；transient 广播 UxPlay 兼容 legacy 位 | shairplay-rust@2fb72b3:src/net/features.rs |
| 控制面端口 | RTSP **5001** | 实测 2026-09-15 |
| 连接模型 | **iPad 每个 RTSP 请求换新 TCP 连接**（/info、pair-* 全不同源端口）→ 配对/SRP 状态必须跨连接存活 | 实测 2026-09-15 |

### 配对（PIN 三步，binary plist；非 HomeKit TLV）

| 步 | 请求字段 | 响应字段 | 来源 |
|---|---|---|---|
| Step1（实测 86 字节） | `{method:"pin"(string), user:"<device_id>"}`；Content-Type 含 `apple-binary-plist` | `{pk:<data>, salt:<data>}`；`application/x-apple-binary-plist` | 实测 2026-09-15 + UxPlay@44024fe:lib/raop_handlers.h:275-430 |
| Step2 | `{pk:<client pubkey>, proof:<client M>}`（SHA1 系，proof 20B） | `{proof:<20B>}` | 实测 2026-09-15 + UxPlay@44024fe:lib/srp.c |
| Step3 | `{epk:<32B ed25519>, authTag:<16B GCM>}` | `{epk, authTag}` | 实测 2026-09-15 |
| PIN 生命期 | 4 位；UxPlay 重置语义（一次性？） | 待固化（P-M01-4） | UxPlay@44024fe:lib/raop_handlers.h（pin%10000）→ 待考 |

### 会话与流

| 项 | 事实 | 来源 |
|---|---|---|
| legacy 建立顺序 | `/fp-setup` → `SETUP(stream_type=110, video)` → `SETUP(stream_type=96, audio)` → 流 | 实测 2026-09-15 |
| 视频传输 | stream_type=110，TCP；128 字节帧头：`payload_len` u32 LE、`type` u16、`timestamp` u64 @ bytes 8-15（**其余字段待固化 P-M01-1**） | 实测 2026-09-15 |
| 视频加密 | AES-128-CTR；密钥经 "Stage 3 derivation from aeskey_audio" | 实测日志 2026-09-15 + shairplay-rust@2fb72b3（事实引用） |
| 视频分帧 | `PacketKind::{AvcC, HvcC, Payload, Plist, Other}`；库只交付 NAL，解码归宿主 | shairplay-rust@2fb72b3（结构事实） |
| 旋转/切换 | AvcC（≈30B）重发，`format_id` 递增，不换 session 身份；实测分辨率 1442x1080 ↔ 1920x1080 | 实测 2026-09-15 |
| 音频传输 | stream_type=96；Legacy ALAC（AES-CBC via ekey），实测 44100Hz；RTP **UDP**（`use_udp=true`, `control_rport`） | 实测 2026-09-15 |
| 音频控制 | `POST /audioMode mode="default"`；`POST /feedback` 每 2s（`elapsed_ms` 单调增，语义待考） | 实测 2026-09-15 |
| 停止 | `stop` → 回收全部缓冲（本仓实现待验证） | 计划（T33） |
| `/fp-setup` 载荷 | **不固化**：属 FairPlay/设备认证材料 → 永久 vendor-gated | decisions/provider-adoption.json（P-FAIRPLAY） |

## 能力声明（T31-01：视频/音频必须分开，不合并成"支持 AirPlay"）

| 能力 | 状态 | 依据 |
|---|---|---|
| 视频（legacy mirror，H.264） | **链路已通、画质未定（blocked）** | 实测：UxPlay 视频✅ / shairplay-rust 链路✅但画质❌（P-M01-1） |
| 音频（legacy ALAC） | **未验证（blocked）** | 实测：UxPlay/shairplay-rust 均无声，根因未定（P-M01-2） |
| PIN 配对（pair-setup-pin） | **未实现（blocked）** | 本仓尚无实现（P-M01-3 计划中） |
| transient 直通 | **实测可用（参考实现）** | 实测 2026-09-15：iPad 跳过配对直连 `/fp-setup` |

**禁止**把上面任何一行写成"AirPlay 支持可用"：视频与音频各自 blocked，PIN 未实现。

## 兼容语料计划（fixture corpus）

见 `evidence/airplay-corpus/corpus-plan.json`：全部 fixture **自制**（字段出自本表），
**真机矩阵条目 `device_required=true` 且 `status=blocked`**（需 S1/S2 批准后才能跑）。
T31-03：自测（自制 fixture 往返）**不替代** iPhone/iPad 真实互通。
