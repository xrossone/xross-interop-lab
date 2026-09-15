//! Google Cast（CASTV2）sender 侧控制面的 headless 核心（plans/03 T43）。
//!
//! 实现依据 `specs-reviewed/m08-cast-media-control.md` 的字段级事实表（每行带来源 commit + 文件 + 行）：
//!
//! - [`castv2`]：CASTV2 信封（字段号 1..9、required 约束、4 字节长度前缀、64 KiB 上限）；
//! - [`namespaces`]：connection / heartbeat / receiver / media 四组载荷的类型与键；
//! - [`controller`]：sender 控制会话（连接 → 拉起 app → 媒体会话 → 收尾）与四条 T43 约束；
//! - [`discovery`]：`_googlecast._tcp` 的 TXT 键值解析（**只解析，不组播**）。
//!
//! 明确不做（对应字段表的 blocked / not-implemented 行，纪律由 `tools/test_cast_gate.py` 机器保证）：
//!
//! - **不做设备认证握手**（F-18）：设备侧凭据材料本仓没有，`ChannelGate` 的生产实现恒拒绝 →
//!   一条控制命令都发不出去；未认证就发命令是**测试失败**；
//! - **不申请、不伪造 app ID**（F-17/F-23）：只允许拉起调用方显式配置的 app ID；
//! - **不做屏幕镜像**（T43-04）：本 profile 只推媒体 URL，`screen_capability()` 恒 `false`；
//! - **不服务媒体字节、不做实时 streaming**（F-25、T44）；
//! - **不做分块**（F-05）：收到分块字段一律 `unsupported-feature`；
//! - 不实现队列与倍速播放（F-11 的 `QUEUE_*`/`SET_PLAYBACK_RATE`）。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod castv2;
pub mod controller;
pub mod discovery;
pub mod namespaces;
pub mod receiver;

pub use castv2::{
    CastMessage, Payload, PayloadType, HEADER_BYTES, MAX_BODY_BYTES, PROTOCOL_VERSIONS,
    RECEIVER_PLATFORM_ID, SENDER_PLATFORM_ID, WILDCARD_DESTINATION,
};
pub use controller::{
    ChannelGate, Controller, ControllerState, ControllerStats, Event, OpenGateForTesting, Outcome,
    UnavailableGate, HEARTBEAT_EXPIRY_MS, HEARTBEAT_PING_MS, HEARTBEAT_PONG_MS,
    REQUEST_TIMEOUT_MS,
};
pub use discovery::{DeviceStatus, ReceiverInfo, DEFAULT_PORT, SERVICE_TYPE};
pub use namespaces::{
    CastPayload, GenericMediaMetadata, LoadRequest, MediaInformation, MediaStatus, PlayerState,
    ReceiverStatus, SenderInfo, StreamType, Volume, NS_CONNECTION, NS_HEARTBEAT, NS_MEDIA,
    NS_RECEIVER,
};
