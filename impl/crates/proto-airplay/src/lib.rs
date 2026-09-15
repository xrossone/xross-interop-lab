//! `proto-airplay`：AirPlay legacy receiver 的控制链核心（plans/03 T32）。
//!
//! - [`rtsp`]：严格 RTSP framing（缺失/重复 `Content-Length` 拒绝、长度检查先于分配）；
//! - [`capability`]：**只广告已实现能力**（广告未实现 codec = 能力一致性检查失败）；
//! - [`discovery`]：两种 Bonjour 服务类型 + 可诚实广播的 TXT 键值（AirPlay 位值待固化）；
//! - [`keying`]：密钥获取 seam——`UnavailableKeying` 是默认（FairPlay 永久 vendor-gated）；
//! - [`session`]：连接/会话/端口记账，认证前不建流、TEARDOWN/断开即回收；
//! - [`mirror`]/[`audio`]/[`timing`]（T33）：把协商出的格式与 encoded/PCM 帧路由到
//!   `interop-media` 的 sink，含关键帧恢复、显式重采样与按连接的 clock generation。
//!   **不含 decoder**：能力广告因此仍为空（能路由 ≠ 能显示）。
//! - [`audio_control`]（T36）：`/audioMode` 与 `/feedback` 的**形状校验与观测记账**——
//!   `elapsed_ms` 语义待考，故**不实现**心跳语义、不据此推时钟。
//! - [`audio_profile`]（T34）：AP1 的 `ct`/`spf`/采样率与 AP2 的**两套编号**（打包 `audioFormat`
//!   与 RTP SSRC 魔数）解析；未知值一律拒绝。
//! - [`pairstore`]（T34）：配对**登记层**（登记/查询/遗忘 + 身份种子保管 + 端点策略）。
//!   **登记 ≠ 认证**：不实现 SRP/X25519/Ed25519，`/fp-setup` 永久 vendor-gated。
//!
//! 所有 message payload 只出自自制 fixture（字段见 `specs-reviewed/m01` 的字段级事实表）；
//! 本 crate **不实现** FairPlay/设备认证材料的任何部分，也不含第三方代码。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod audio;
pub mod audio_control;
pub mod audio_profile;
pub mod capability;
pub mod discovery;
pub mod keying;
pub mod mirror;
pub mod pairstore;
pub mod rtsp;
pub mod session;
pub mod timing;
