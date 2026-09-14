# Reviewed Wire Spec — airplay.legacy-mirror（M01）

**状态**：source-reviewed + 真机部分实测（transient 链路）。**来源**：docs/research/airplay-research.md（实测），
UxPlay @44024fe（pair-setup-pin 事实），shairplay-rust @2fb72b3。**FairPlay/DRM 不在本 spec 范围。**

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
