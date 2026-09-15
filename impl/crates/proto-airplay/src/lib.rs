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
//!
//! 所有 message payload 只出自自制 fixture（字段见 `specs-reviewed/m01` 的字段级事实表）；
//! 本 crate **不实现** FairPlay/设备认证材料的任何部分，也不含第三方代码。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod audio;
pub mod capability;
pub mod discovery;
pub mod keying;
pub mod mirror;
pub mod rtsp;
pub mod session;
pub mod timing;
