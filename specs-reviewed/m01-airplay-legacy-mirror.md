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

### T34 增量（2026-09-15）：音频 profile（AP1 实时 / AP2 打包与 SSRC）

来源代号：**A** = `UxPlay@d9791de`（本仓 `references/external/r01-uxplay`）、**B** = `shairplay-rust@2fb72b3`
（`references/external/r02-shairplay-rust`）。只取键名/常量/结构事实，不复制表达。

**流类型（`streams[].type`）**

| 项 | 事实 | 来源 |
|---|---|---|
| 96 | 实时音频（A 的 legacy 路径；B 的 AP2 路径也用它做实时音频，键含 `shk`） | UxPlay@d9791de:lib/raop_handlers.h:976,1035-1040；shairplay-rust@2fb72b3:src/raop/handlers_ap2.rs:626,632-649 |
| 103 | AP2 **buffered** 音频：TCP + ChaCha20-Poly1305 + AAC；响应含 `audioBufferSize` | shairplay-rust@2fb72b3:src/raop/buffered_audio.rs:1-5；src/raop/handlers_ap2.rs:747-801（`audioBufferSize` :797-800） |
| 110 | 镜像视频（响应只有 `dataPort`/`type`，**没有** `controlPort`） | UxPlay@d9791de:lib/raop_handlers.h:948,968-971；shairplay-rust@2fb72b3:src/raop/handlers_ap2.rs:842-845,907 |
| 130 | AP2 遥控数据通道（请求 `seed`，响应 `streamID`/`dataPort`） | shairplay-rust@2fb72b3:src/raop/handlers_ap2.rs:805-832（`seed` :815，`streamID`/`dataPort` :826-829） |
| 120 | Apple Music 视频：参考实现明说未实现 | shairplay-rust@2fb72b3:src/raop/handlers_ap2.rs:401（注释） |

**AP1 实时音频（type 96）的请求/响应键**

| 项 | 事实 | 来源 |
|---|---|---|
| 请求键 | `controlPort`（对端控制端口；注释：为 0 则不激活重传请求）、`ct`、`spf`、`audioFormat`，以及**可选**的 `isMedia`、`usingScreen` | UxPlay@d9791de:lib/raop_handlers.h:984-986,988-990,1000-1002,1004-1005,1007-1013,1015-1021 |
| 响应键 | `dataPort`、`controlPort`、`type`（回本端端口；type 原样回 96） | UxPlay@d9791de:lib/raop_handlers.h:1035-1040 |
| 采样率 | AP1 固定 **44100**：`#define AUDIO_SAMPLE_RATE 44100`，注释写明所有已支持格式都是这个率 | UxPlay@d9791de:lib/raop_handlers.h:27,981 |
| `audioFormat` | A 只把该值读出来交给回调并记录日志，**不做任何解码**（不得据此推断字段含义） | UxPlay@d9791de:lib/raop_handlers.h:994,1004-1005；uxplay.cpp:2517-2519 |

**压缩类型 `ct`（A 的四值表，注释即参考实现的 caps）**

| 项 | 事实 | 来源 |
|---|---|---|
| ct=1 | 线性 PCM（未压缩）：44100/16/2，S16LE | UxPlay@d9791de:renderers/audio_renderer.c:57-58 |
| ct=2 | **ALAC**：44100/16/2，`spf=352` | UxPlay@d9791de:renderers/audio_renderer.c:60-62；lib/raop_rtp.c:568 |
| ct=4 | **AAC-LC**：44100/2，`spf=1024` | UxPlay@d9791de:renderers/audio_renderer.c:64-65 |
| ct=8 | **AAC-ELD**：44100/2，`spf=480`；AAC-ELD 的时间戳步长即 480，ALAC 为 352 | UxPlay@d9791de:renderers/audio_renderer.c:67-68；lib/raop_rtp.c:569 |
| 冲突（同一仓内） | `lib/raop_rtp.c:113` 注释把 ct=4 称作 "AAC-MAIN"，而 renderers 的 caps 与解码器都按 **AAC-LC**（MPEG-4 对象类型 2）处理 → **以实现侧（caps/解码器）为准**，注释文本视为笔误，记冲突待真机裁决 | UxPlay@d9791de:lib/raop_rtp.c:113 vs renderers/audio_renderer.c:64-65 |
| AAC-ELD 的 no-data 标记 | 流起始的初始包正文可能是 4 字节标记 `0x00 0x68 0x34 0x00`（仅 AAC-ELD） | UxPlay@d9791de:lib/raop_rtp.c:566-567 |
| 线性 PCM 无 spf | PCM 行没有 spf 取值（caps 里未给）→ 不得替它编一个 | UxPlay@d9791de:renderers/audio_renderer.c:57-58 |
| 实现侧另用 `cn` | B 的 AP1 广告里 `cn=0` = PCM、`cn=1` = ALAC（与 `ct` 是**两套**编号，别混） | shairplay-rust@2fb72b3:src/raop/types.rs:28-50 |

**AP2 打包 `audioFormat`（与 RTP SSRC 不是一回事）**

| 项 | 事实 | 来源 |
|---|---|---|
| 0x00040000 | ALAC 44100/16/2 | shairplay-rust@2fb72b3:src/codec/alac.rs:47-53（注释 :43-45 明说这是 "format-capability bit values, not RTP SSRC values"） |
| 0x00080000 | ALAC 44100/24/2 | shairplay-rust@2fb72b3:src/codec/alac.rs:54-58 |
| 0x00100000 | ALAC 48000/16/2 | shairplay-rust@2fb72b3:src/codec/alac.rs:59-63 |
| 0x00200000 | ALAC 48000/24/2 | shairplay-rust@2fb72b3:src/codec/alac.rs:64-68 |
| 其它值 | 解析为 None（**不猜**） | shairplay-rust@2fb72b3:src/codec/alac.rs:69 |
| AP2 的 `sr`/`spf` | type 96 请求可带 `sr`（默认 44100）与 `spf`（默认 352）；`sr` 也出现在另一处流字典（44100） | shairplay-rust@2fb72b3:src/raop/handlers_ap2.rs:632-639,1035 |

**AP2 buffered（103）RTP SSRC 魔数：解密后的 `packet[8:12]`**

| 项 | 事实 | 来源 |
|---|---|---|
| 0 | 未知/未识别（不触发解码器切换） | shairplay-rust@2fb72b3:src/codec/aac.rs:12-14 |
| 0x0000FACE | ALAC 44100/16/2 | shairplay-rust@2fb72b3:src/codec/aac.rs:16-18 |
| 0x15000000 | ALAC 48000/24/2 | shairplay-rust@2fb72b3:src/codec/aac.rs:19-21 |
| 0x16000000 | AAC 44100 24f/2 | shairplay-rust@2fb72b3:src/codec/aac.rs:22-24 |
| 0x17000000 | AAC 48000 24f/2 | shairplay-rust@2fb72b3:src/codec/aac.rs:25-27 |
| 0x27000000 | AAC 48000 24f **5.1** | shairplay-rust@2fb72b3:src/codec/aac.rs:28-30 |
| 0x28000000 | AAC 48000 24f **7.1** | shairplay-rust@2fb72b3:src/codec/aac.rs:31-33 |
| 读取位置与语义 | 与 `seq`/`timestamp` 同处 RTP 头（4 字节 SSRC）；**格式变化**（非 0 且与上一值不同）触发解码器重建 | shairplay-rust@2fb72b3:src/raop/buffered_audio.rs:231-239 |
| 上游出处 | B 的注释写明这批魔数来自 shairport-sync `player.h` | shairplay-rust@2fb72b3:src/codec/aac.rs:5-6 |

**配对端点与配对存储（T34）**

| 项 | 事实 | 来源 |
|---|---|---|
| 端点集 | `/pair-pin-start`、`/pair-setup-pin`、`/pair-setup`、`/pair-verify`、`/fp-setup` | UxPlay@d9791de:lib/raop.c:406-422 |
| **不是 TLV8** | A 仓全仓 grep `tlv` **零命中**：`/pair-setup-pin` 用 binary plist，`/pair-setup` 收发**裸 32 字节** Ed25519 公钥（`application/octet-stream`）→ 不得声称 AirPlay 配对走 TLV8 | UxPlay@d9791de:lib/raop_handlers.h:436-461（32 字节 :447,:452-459）；全仓 grep |
| `/pair-setup-pin` 三步键 | step1 请求 `{method,user}` → 响应 `{pk,salt}`；step2 请求 `{pk,proof}`（SHA1 系 20 B）→ 响应 `{proof}`；step3 请求 `{epk,authTag}` → 响应 `{epk,authTag}`；失败 → RTSP **470** | UxPlay@d9791de:lib/raop_handlers.h:303-308,342-346,381-383,421-425,431-432 |
| `/pair-verify` 两步 | step 字节 1：请求 32(X25519)+32(Ed25519)，响应 32 公钥 + 64 签名；step 字节 0：校验签名、无正文（`check_register` 在 step 1 处把关） | UxPlay@d9791de:lib/raop_handlers.h:488-540,504-516 |
| SRP/PIN 语义 | 以 4 位 PIN 为 SRP password；PIN 首次使用后重置；口令重试上限 `MAX_PW_ATTEMPTS 3` | UxPlay@d9791de:lib/raop_handlers.h:327-332,30,657-662 |
| 长期身份与配对材料 | 配件 Ed25519 长期密钥对（可存 `~/.uxplay.pem`，默认路径在启用 PIN 时设置）；配对期临时 X25519（`ecdh_secret`）**不持久化**；控制器 Ed25519 公钥在 step3 存入 `client_pk[32]` | UxPlay@d9791de:lib/pairing.c:96,101-106,171-180,629；uxplay.cpp:3222-3234；lib/crypto.c:400-409 |
| 配对会话状态字段 | `pairing_session_s{status, ed_ours, ed_theirs, ecdh_ours, ecdh_theirs, ecdh_secret[32], username[25], client_pk[32], pair_setup, srp}`；SRP 状态 `{salt[16], verifier[256], session_key[40], private_key[32]}` | UxPlay@d9791de:lib/pairing.c:32-66；lib/pairing.h:21-34 |
| 已配对控制器注册表 | 文件默认 `~/.uxplay.register`，**每行 `pk,device_id,name`**（pk 为 base64 定长 44 字符）；**只追加、无删除路径**；SETUP 的 `deviceID` 与配对会话 `username` 用 `strcmp` 比对后才登记 | UxPlay@d9791de:uxplay.cpp:3191-3197,3201-3217,2642-2647,2651-2665；lib/raop_handlers.h:756-762 |
| 库侧配对存储接口（B） | `PairingStore{get/put/remove/has_any_pairing/load_identity/save_identity}`；`MemoryPairingStore` **重启即失**；库内**不落盘**，持久化只出现在示例（JSON 含 `mac`/`identity_seed`/`paired_keys`） | shairplay-rust@2fb72b3:src/raop/types.rs:133-205；examples/player/main.rs:191-215,238-272 |
| 一次性配对提示位 | `OneTimePairingRequired` = statusFlags **bit 9**：只在**首个成功配对之前**广播（已配对设备应走 pair-verify 而不是被推回 setup） | shairplay-rust@2fb72b3:src/raop/types.rs:146-152 |
| 身份必须持久化的理由 | B 的注释：不持久化时"曾经配过的 iPhone 会发加密数据而连接失败" | shairplay-rust@2fb72b3:src/raop/types.rs:135-137 |
| `/fp-setup` | **永久 vendor-gated**（FairPlay/设备认证材料）：本仓不实现、不绕过、不解析其载荷 | decisions/provider-adoption.json（P-FAIRPLAY）；provenance/airplay-inputs.json（R06） |

### T34 禁止边界（不得进入实现上下文）

- A 仓 `lib/playfair/`（含三组 16 字节密钥数组与置换表）与 B 仓 `src/crypto/`（`fairplay*.rs` 预计算表、
  `pairing_homekit.rs` 默认 transient PIN 常量、`airport.key` RSA 私钥经 `include_str!` 打进二进制）
  → **一律不取**；见 `provenance/airplay-inputs.json` 的 R01/R02 边界。
- 本切片**不实现** SRP / X25519 / Ed25519：`/pair-setup-pin` 与 `/pair-verify` 只做**键名与状态校验**，
  交换本身明确 `unsupported-feature`。
- `/fp-setup` 与任何 FairPlay/设备认证载荷**永久 vendor-gated**：本仓不实现、不解析、不绕过，
  也不引入任何厂商密钥材料。
- **禁止推断**："登记过的公钥 ⇒ 该设备已认证"。存储层只回答"是否登记过"；认证结论由 keying seam
  给出（当前 `UnavailableKeying` → SETUP 503）。

## 能力声明（T31-01：视频/音频必须分开，不合并成"支持 AirPlay"）

| 能力 | 状态 | 依据 |
|---|---|---|
| 视频（legacy mirror，H.264） | **链路已通、画质未定（blocked）** | 实测：UxPlay 视频✅ / shairplay-rust 链路✅但画质❌（P-M01-1） |
| 音频（legacy ALAC） | **未验证（blocked）** | 实测：UxPlay/shairplay-rust 均无声，根因未定（P-M01-2） |
| PIN 配对（pair-setup-pin） | **未实现（blocked）** | 本仓尚无实现（P-M01-3 计划中） |
| transient 直通 | **实测可用（参考实现）** | 实测 2026-09-15：iPad 跳过配对直连 `/fp-setup` |
| AP1 实时音频 profile（`ct`/`spf`/采样率的解析与校验，T34） | **implemented（headless 解析层）** | 字段表 T34 增量（`ct` 四值表）；**不声明能解码**：ALAC/AAC 仍需解码器 |
| AP2 音频 profile（打包 `audioFormat` 与 SSRC 魔数的解析，T34） | **implemented（headless 解析层）** | 字段表 T34 增量；未知值一律拒绝，不猜 |
| AP2 buffered 音频（type 103：ChaCha20-Poly1305 + AAC 解码） | **not-implemented（blocked）** | 需 ChaCha 与 AAC 解码器（参考实现用 symphonia）；本仓不做 |
| 配对存储（登记/查询/遗忘 + 身份种子保管，T34） | **implemented（headless 存储层，无加密）** | 字段表 T34 配对存储行；**不得据此声称设备已认证** |
| 配对握手（`/pair-setup-pin` 三步 SRP、`/pair-verify` 签名） | **not-implemented（blocked）** | 需 SRP/X25519/Ed25519 实现与密钥材料 → 端点只做键名校验后明确拒绝 |
| `/fp-setup`（FairPlay / 设备认证） | **永久 vendor-gated** | decisions/provider-adoption.json（P-FAIRPLAY） |

**禁止**把上面任何一行写成"AirPlay 支持可用"：视频与音频各自 blocked，PIN 未实现。

## 兼容语料计划（fixture corpus）

见 `evidence/airplay-corpus/corpus-plan.json`：全部 fixture **自制**（字段出自本表），
**真机矩阵条目 `device_required=true` 且 `status=blocked`**（需 S1/S2 批准后才能跑）。
T31-03：自测（自制 fixture 往返）**不替代** iPhone/iPad 真实互通。
