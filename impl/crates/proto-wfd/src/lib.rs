//! WFD（Wi-Fi Display / Miracast）接收侧控制面与媒体协商的 headless 核心（plans/03 T37）。
//!
//! 实现依据 `specs-reviewed/m05-miracast-wfd.md` 的字段级事实表（每行带来源 commit + 文件 + 行）。
//! 本 crate **只做字段表里 impl-allowed = yes 的行**：
//!
//! - 消息层（F-01..F-16）：RTSP 方法与 M 编号的对应、方向与顺序约束、`CSeq`/`Session` 语义、
//!   presentation URL 形状、`Transport` 与 `wfd_client_rtp_ports` 解析；
//! - 协商层（F-17..F-21）：`wfd_video_formats`/`wfd_audio_codecs` 描述符解析、交集选择（无交集即拒绝）；
//! - 子元素层（F-23）：subelement `0x00`（设备信息 + 控制端口 + 最大吞吐）的编解码；
//! - 媒体边界（F-28/F-29）：RTP 固定头解析与序号记账；丢包后按**本仓策略**请求关键帧（M13）。
//!
//! 明确不做（对应字段表的 blocked / not-implemented 行，纪律由 `tools/test_wfd_gate.py` 机器保证）：
//!
//! - **不构造也不解析完整 WFD IE**（F-24：OUI 与 OUI type 在 R32/R33/R34/R37 四个来源里都不存在，
//!   实现不得臆造）——只处理子元素字节；
//! - **不触碰平台 P2P API**（F-25：NetworkManager / HarmonyOS Wi-Fi kit / Android WifiP2pManager）；
//! - **不实现 HDCP 握手**（F-22：密钥交换属设备/厂商材料；对端要求内容保护时明确拒绝，
//!   绝不静默忽略后继续）；
//! - **不做 TS 解复用、不解码、不渲染**（F-28）：媒体侧只到 RTP 记账；
//! - 不实现 UIBC 输入回传、不实现厂商扩展参数（`wfd_hwe_*`/`microsoft_*`，F-31 登记）。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

/// 错误消息里回显输入时用的**截断预览**（本仓策略：最多 128 字节）。
///
/// 回显的字节是不可信的：把整行/整段塞进 `format!` 会让"1 MiB 的正文"变成"若干 MiB 的
/// 错误消息"（对抗性扫描抓到的第二类放大）。这里只保留开头，并注明被截掉多少字节。
pub(crate) fn preview(value: &str) -> String {
    const MAX: usize = 128;
    if value.len() <= MAX {
        return format!("{value:?}");
    }
    let mut end = MAX;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{:?}…(+{}B)", &value[..end], value.len() - end)
}

pub mod ie;
pub mod messages;
pub mod negotiate;
pub mod rtp;
pub mod session;

pub use ie::{DeviceInfoSubelement, WfdIeContainer, WfdSubelement, DEVICE_INFO_SUBELEMENT_ID};
pub use messages::{
    parse_parameter_body, parse_parameter_names, Request, Response, WfdMessage, Headers,
    WFD_METHOD_SET,
    MAX_BODY_BYTES, MAX_HEADER_BYTES, MAX_HEADERS,
};
pub use negotiate::{
    parse_audio_codecs, parse_video_formats, select_common, select_common_audio, AudioCodec,
    AudioCodecSet, AudioCodecDescriptor,
    ContentProtection, H264Level, H264Profile, NativeResolution, ResolutionTable, RtpPorts,
    Transport, VideoFormatDescriptor, VideoFormatSet,
};
pub use rtp::{parse_header, RtpHeader, StreamAccount};
pub use session::{
    validate_presentation_url, EndReason, NegotiatedStream, ReceivedMediaSupport, SessionStats,
    SinkSession, SinkState, KEEPALIVE_INTERVAL_MS, SESSION_TIMEOUT_MS,
};

/// RTSP 控制端口（F-01：R32 `sinkctl.c:73`、R37 `const_def.h:89`、R34 `WifiDisplaySource.h:38` 三处同值）。
pub const CONTROL_PORT: u16 = 7236;

/// WFD 流路径（F-12）。
pub const STREAM_PATH: &str = "/wfd1.0/streamid=0";

/// 控制 URI（F-11/F-12：R33 `wfd-client.c:384` 与 R34 `sendM16` 字面量、R37 `WFD_RTSP_URL_DEFAULT`）。
pub const CONTROL_URI: &str = "rtsp://localhost/wfd1.0";

/// 缺 `client_port` 时的回退 RTP 端口（F-16：R34 `WifiDisplaySource.cpp:1049-1051`，老 LG dongle quirk）。
pub const RTP_PORT_FALLBACK: u16 = 19000;

/// MPEG-TS over RTP 的 payload type（F-28：R34 `Sender.cpp:307` 的 `rtp[1] = 33`）。
/// 只作为**观察值**记录与统计，不作为硬性拒绝条件（动态 PT 的协商不在字段表内）。
pub const MPEGTS_PAYLOAD_TYPE: u8 = 33;
