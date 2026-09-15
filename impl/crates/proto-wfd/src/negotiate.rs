//! 媒体协商（字段表 F-17..F-22）：`wfd_video_formats` / `wfd_audio_codecs` / 端口与传输参数。
//!
//! 字面量形状与字段序逐条对应字段表；**没有**任何"按品牌特判"的分支（F-30 blocked）。
//! 空集合序列化为 `none`（F-04：来源把 `none` 作为"不支持"的取值处理）。

use crate::messages::{
    PARAM_AUDIO_CODECS, PARAM_CLIENT_RTP_PORTS, PARAM_PRESENTATION_URL, PARAM_VIDEO_FORMATS,
};
use crate::{preview, RTP_PORT_FALLBACK};
use interop_contract::error::{Error, ErrorCode};

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn unsupported(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, msg)
}

fn hex_u8(token: &str, what: &str) -> Result<u8, Error> {
    u8::from_str_radix(token, 16)
        .map_err(|_| invalid(format!("{what} 不是合法十六进制字节：{}", preview(token))))
}

// ---------------------------------------------------------------- 分辨率表（F-19）

/// 分辨率表（F-19：`native & 0x7` 选表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionTable {
    Cea,
    Vesa,
    Hh,
}

impl ResolutionTable {
    pub fn from_bits(bits: u16) -> Result<Self, Error> {
        match bits & 0x7 {
            0 => Ok(Self::Cea),
            1 => Ok(Self::Vesa),
            2 => Ok(Self::Hh),
            other => Err(invalid(format!("未知分辨率表编号 {other}（F-19 只固定 0/1/2）"))),
        }
    }
}

/// `native` 字段拆出的表与索引（F-19：低位选表、高位作索引）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeResolution {
    pub table: ResolutionTable,
    pub index: u8,
}

impl NativeResolution {
    pub fn parse(native: u16) -> Result<Self, Error> {
        Ok(Self {
            table: ResolutionTable::from_bits(native)?,
            index: (native >> 3) as u8,
        })
    }
}

// ---------------------------------------------------------------- H.264 profile / level（F-18）

/// H.264 profile 位图（F-18：R37 `wfd_session_def.h:231-238` 的位序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct H264Profile(pub u8);

impl H264Profile {
    pub const CBP: u8 = 1 << 0;
    pub const CHP: u8 = 1 << 1;
    pub const RHP: u8 = 1 << 2;
    pub const BP: u8 = 1 << 3;
    pub const MP: u8 = 1 << 4;
    pub const HIP: u8 = 1 << 5;

    /// 已知位以外的位 → 明确拒绝（不静默忽略对端声明的能力位）。
    pub fn validate(self) -> Result<Self, Error> {
        let known = Self::CBP | Self::CHP | Self::RHP | Self::BP | Self::MP | Self::HIP;
        if self.0 == 0 {
            return Err(invalid("profile 位图为 0：对端没声明任何 H.264 profile"));
        }
        if self.0 & !known != 0 {
            return Err(invalid(format!(
                "profile 位图含未登记位：0x{:02X}（F-18 只固定低 6 位）",
                self.0
            )));
        }
        Ok(self)
    }

    pub fn contains(self, other: H264Profile) -> bool {
        self.0 & other.0 != 0
    }
}

/// H.264 level 位图（F-18：R37 `wfd_session_def.h:241-250` 的位序）。
///
/// **裁决记录（F-32）**：本仓按**位图**解释该字节——两个来源支持这一读法
/// （R37 的位序枚举；R33 `wfd-video-codec.c:331-362` 的码率表按 1/2/4/8/16 这些位值索引）。
/// 把同一字节当"级别数字"（如 `0x02` 读成 4.2）在来源里没有依据，故不采用；
/// 该字节的真实解释仍需 P-M05-2 抓包确认。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct H264Level(pub u8);

impl H264Level {
    pub const L3_1: u8 = 1 << 0;
    pub const L3_2: u8 = 1 << 1;
    pub const L4_0: u8 = 1 << 2;
    pub const L4_1: u8 = 1 << 3;
    pub const L4_2: u8 = 1 << 4;
    pub const L5_0: u8 = 1 << 5;
    pub const L5_1: u8 = 1 << 6;
    pub const L5_2: u8 = 1 << 7;

    pub fn contains(self, other: H264Level) -> bool {
        self.0 & other.0 != 0
    }

    /// 双方都声明的最高级别（位序号越大越"高"，按 F-18 的位序）。
    pub fn highest_common(self, other: H264Level) -> Option<H264Level> {
        let common = self.0 & other.0;
        if common == 0 {
            return None;
        }
        let highest = 1u8 << (7 - common.leading_zeros() as u8);
        Some(H264Level(highest))
    }
}

// ---------------------------------------------------------------- 视频描述符（F-17）

/// 单条视频格式描述符（F-17 的字段序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFormatDescriptor {
    pub profile: H264Profile,
    pub level: H264Level,
    pub cea_sup: u32,
    pub vesa_sup: u32,
    pub hh_sup: u32,
    pub latency: u8,
    pub min_slice_size: u16,
    pub slice_params: u16,
    pub frame_rate_ctrl: u8,
    /// 尾部占位字段（来源注释标注为最大宽/高，`none` = 未指定）。
    pub max_width: Option<u16>,
    pub max_height: Option<u16>,
}

/// 视频描述符允许被收集的 token 上限（**本仓策略值**）：F-17 的字段是 9..13 段。
/// 存在的唯一理由是给"先收集后校验"封顶，不改变任何接受/拒绝判定。
pub const MAX_VIDEO_DESCRIPTOR_TOKENS: usize = 32;

impl VideoFormatDescriptor {
    /// `slice_params` 的位布局（F-17：R33 `wfd-video-codec.c:281-287`）。
    pub fn max_slice_num(&self) -> u32 {
        if self.min_slice_size == 0 {
            // F-17：min_slice_size = 0 表示不支持切片，来源把 max_slice_num 归 1。
            1
        } else {
            u32::from(self.slice_params & 0x1ff) + 1
        }
    }

    pub fn max_slice_size_ratio(&self) -> u8 {
        ((self.slice_params >> 10) & 0x7) as u8
    }

    pub fn frame_skipping_allowed(&self) -> bool {
        self.frame_rate_ctrl & 0x1 != 0
    }

    fn parse(tokens: &[&str]) -> Result<Self, Error> {
        if tokens.len() < 9 {
            // F-17：来源要求至少 9 段（R33 `wfd-video-codec.c:270-272`）。
            return Err(invalid(format!(
                "视频描述符字段不足：{} < 9（F-17）",
                tokens.len()
            )));
        }
        let placeholder = |token: &str, what: &str| -> Result<Option<u16>, Error> {
            if token.eq_ignore_ascii_case("none") {
                Ok(None)
            } else {
                u16::from_str_radix(token, 16)
                    .map(Some)
                    .map_err(|_| {
                        invalid(format!(
                            "{what} 既不是 none 也不是合法十六进制：{}",
                            preview(token)
                        ))
                    })
            }
        };
        Ok(Self {
            profile: H264Profile(hex_u8(tokens[0], "profile")?).validate()?,
            level: H264Level(hex_u8(tokens[1], "level")?),
            cea_sup: u32::from_str_radix(tokens[2], 16)
                .map_err(|_| invalid(format!("cea_sup 非法：{:?}", tokens[2])))?,
            vesa_sup: u32::from_str_radix(tokens[3], 16)
                .map_err(|_| invalid(format!("vesa_sup 非法：{:?}", tokens[3])))?,
            hh_sup: u32::from_str_radix(tokens[4], 16)
                .map_err(|_| invalid(format!("hh_sup 非法：{:?}", tokens[4])))?,
            latency: hex_u8(tokens[5], "latency")?,
            min_slice_size: u16::from_str_radix(tokens[6], 16)
                .map_err(|_| invalid(format!("min_slice_size 非法：{:?}", tokens[6])))?,
            slice_params: u16::from_str_radix(tokens[7], 16)
                .map_err(|_| invalid(format!("slice_params 非法：{:?}", tokens[7])))?,
            frame_rate_ctrl: hex_u8(tokens[8], "frame_rate_ctrl")?,
            max_width: if tokens.len() > 9 {
                placeholder(tokens[9], "max_width")?
            } else {
                None
            },
            max_height: if tokens.len() > 10 {
                placeholder(tokens[10], "max_height")?
            } else {
                None
            },
        })
    }

    pub fn to_wire(&self) -> String {
        let dim = |v: Option<u16>| match v {
            Some(x) => format!("{x:04X}"),
            None => "none".to_string(),
        };
        format!(
            "{:02X} {:02X} {:08X} {:08X} {:08X} {:02X} {:04X} {:04X} {:02X} {} {}",
            self.profile.0,
            self.level.0,
            self.cea_sup,
            self.vesa_sup,
            self.hh_sup,
            self.latency,
            self.min_slice_size,
            self.slice_params,
            self.frame_rate_ctrl,
            dim(self.max_width),
            dim(self.max_height)
        )
    }
}

/// `wfd_video_formats` 的整个取值（F-17：`native` + 首选显示模式 + 描述符列表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFormatSet {
    pub native: NativeResolution,
    pub raw_native: u16,
    pub preferred_display_mode: u8,
    pub formats: Vec<VideoFormatDescriptor>,
}

impl VideoFormatSet {
    /// 空集合：不支持任何视频格式（序列化为 `none`）。
    pub fn none() -> Self {
        Self {
            native: NativeResolution {
                table: ResolutionTable::Cea,
                index: 0,
            },
            raw_native: 0,
            preferred_display_mode: 0,
            formats: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.trim().eq_ignore_ascii_case("none") {
            return Ok(Self::none());
        }
        // F-17：整值被拆成三段——native、首选显示模式、描述符列表（列表本身可含空格）。
        let mut parts = value.splitn(3, ' ');
        let native_token = parts.next().unwrap_or("");
        let preferred_token = parts.next().ok_or_else(|| {
            invalid("wfd_video_formats 需要 `native preferred 描述符列表` 三段（F-17）")
        })?;
        let list = parts.next().ok_or_else(|| {
            invalid("wfd_video_formats 需要 `native preferred 描述符列表` 三段（F-17）")
        })?;
        let raw_native = u16::from_str_radix(native_token, 16)
            .map_err(|_| invalid(format!("native 字段非法：{native_token:?}")))?;
        let preferred_display_mode = hex_u8(preferred_token, "preferred_display_mode")?;
        let mut formats = Vec::new();
        for descriptor in list.split(',') {
            let descriptor = descriptor.trim();
            if descriptor.is_empty() {
                return Err(invalid("空的视频描述符（列表里有连续逗号）"));
            }
            // 收集上限是**本仓策略值**：F-17 的字段是 9..13 段，多出的段与今天一样被忽略，
            // 但"先全量收集再校验"会让 1 MiB 输入换来十几 MiB 的 token 表（分帧/长度纪律同理）。
            let tokens: Vec<&str> = descriptor
                .split_whitespace()
                .take(MAX_VIDEO_DESCRIPTOR_TOKENS)
                .collect();
            formats.push(VideoFormatDescriptor::parse(&tokens)?);
        }
        Ok(Self {
            native: NativeResolution::parse(raw_native)?,
            raw_native,
            preferred_display_mode,
            formats,
        })
    }

    pub fn to_wire(&self) -> String {
        if self.formats.is_empty() {
            return "none".to_string();
        }
        let list = self
            .formats
            .iter()
            .map(|f| f.to_wire())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{:02X} {:02X} {}",
            self.raw_native, self.preferred_display_mode, list
        )
    }
}

// ---------------------------------------------------------------- 音频描述符（F-20/F-21）

/// 音频编码（F-20：名字字面量 LPCM/AAC/AC3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Lpcm,
    Aac,
    Ac3,
}

impl AudioCodec {
    pub fn name(self) -> &'static str {
        match self {
            Self::Lpcm => "LPCM",
            Self::Aac => "AAC",
            Self::Ac3 => "AC3",
        }
    }

    fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "LPCM" => Ok(Self::Lpcm),
            "AAC" => Ok(Self::Aac),
            "AC3" => Ok(Self::Ac3),
            other => Err(invalid(format!("未知音频编码名 {other:?}（F-20）"))),
        }
    }
}

/// 单条音频描述符（F-20：`CODEC MODES LATENCY`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioCodecDescriptor {
    pub codec: AudioCodec,
    pub modes: u32,
    pub latency_field: u8,
}

impl AudioCodecDescriptor {
    /// F-20：末字段按 `× 5 ms` 解释（单一来源 R33 `wfd-audio-codec.c:114`，已标注）。
    pub fn latency_ms(&self) -> u32 {
        u32::from(self.latency_field) * 5
    }

    /// F-21：AAC bit0 = 48 kHz/16 bit/2ch。
    pub fn aac_48k_16_2ch(&self) -> bool {
        self.codec == AudioCodec::Aac && self.modes & 1 != 0
    }

    /// F-21：LPCM bit0 = 44.1 kHz/16 bit/2ch。
    pub fn lpcm_44100_16_2ch(&self) -> bool {
        self.codec == AudioCodec::Lpcm && self.modes & 1 != 0
    }

    /// F-21：LPCM bit1 = 48 kHz/16 bit/2ch。
    pub fn lpcm_48000_16_2ch(&self) -> bool {
        self.codec == AudioCodec::Lpcm && self.modes & 2 != 0
    }

    fn parse(descriptor: &str) -> Result<Self, Error> {
        // take(4)：判定"恰好 3 段"只需要看到第 4 段，不必把整串切成 token 表。
        let tokens: Vec<&str> = descriptor.split_whitespace().take(4).collect();
        if tokens.len() != 3 {
            return Err(invalid(format!(
                "音频描述符需要恰好三段 `CODEC MODES LATENCY`（F-20）：{}",
                preview(descriptor)
            )));
        }
        Ok(Self {
            codec: AudioCodec::parse(tokens[0])?,
            modes: u32::from_str_radix(tokens[1], 16)
                .map_err(|_| invalid(format!("modes 字段非法：{:?}", tokens[1])))?,
            latency_field: hex_u8(tokens[2], "latency")?,
        })
    }

    pub fn to_wire(&self) -> String {
        format!(
            "{} {:08X} {:02X}",
            self.codec.name(),
            self.modes,
            self.latency_field
        )
    }
}

/// `wfd_audio_codecs` 的整个取值。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioCodecSet {
    pub codecs: Vec<AudioCodecDescriptor>,
}

impl AudioCodecSet {
    pub fn none() -> Self {
        Self { codecs: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.codecs.is_empty()
    }

    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.trim().eq_ignore_ascii_case("none") {
            return Ok(Self::none());
        }
        let mut codecs = Vec::new();
        for descriptor in value.split(',') {
            let descriptor = descriptor.trim();
            if descriptor.is_empty() {
                return Err(invalid("空的音频描述符（列表里有连续逗号）"));
            }
            codecs.push(AudioCodecDescriptor::parse(descriptor)?);
        }
        Ok(Self { codecs })
    }

    pub fn to_wire(&self) -> String {
        if self.codecs.is_empty() {
            return "none".to_string();
        }
        self.codecs
            .iter()
            .map(|c| c.to_wire())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// ---------------------------------------------------------------- 端口与传输（F-13..F-16）

/// `wfd_client_rtp_ports` 的解析结果（F-13/F-14）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtpPorts {
    pub port0: u16,
    pub port1: u16,
    /// F-14：`port1` 为 0 或等于 `port0` → RTCP 端口无效，按 `port0 + 1` 处理。
    pub rtcp_port_invalid: bool,
}

impl RtpPorts {
    pub fn parse(value: &str) -> Result<Self, Error> {
        // take(5)：判定"恰好 4 段"只需要看到第 5 段（同上：不先全量收集）。
        let tokens: Vec<&str> = value.split_whitespace().take(5).collect();
        if tokens.len() != 4 {
            return Err(invalid(format!(
                "wfd_client_rtp_ports 形状应为 `RTP/AVP/UDP;unicast <p0> <p1> mode=play`（F-13）：{}",
                preview(value)
            )));
        }
        if tokens[0] != "RTP/AVP/UDP;unicast" {
            return Err(invalid(format!(
                "只认 `RTP/AVP/UDP;unicast` profile（F-13）：{:?}",
                tokens[0]
            )));
        }
        if tokens[3] != "mode=play" {
            return Err(invalid(format!("缺 `mode=play`（F-13/14）：{:?}", tokens[3])));
        }
        let port0: u16 = tokens[1]
            .parse()
            .map_err(|_| invalid(format!("port0 非法：{:?}", tokens[1])))?;
        let port1: u16 = tokens[2]
            .parse()
            .map_err(|_| invalid(format!("port1 非法：{:?}", tokens[2])))?;
        if port0 == 0 {
            // F-13：来源把 port0 = 0 判为畸形。
            return Err(invalid("port0 不得为 0（F-13）"));
        }
        let rtcp_port_invalid = port1 == 0 || port1 == port0;
        if rtcp_port_invalid && port0 == u16::MAX {
            return Err(invalid("port0 为 65535 时无法按 F-14 取 port0+1"));
        }
        Ok(Self {
            port0,
            port1,
            rtcp_port_invalid,
        })
    }

    /// RTCP 端口（F-14：无效时取 `port0 + 1`）。
    pub fn rtcp_port(&self) -> u16 {
        if self.rtcp_port_invalid {
            self.port0 + 1
        } else {
            self.port1
        }
    }

    /// F-14：RTCP 端口无效时来源会禁用 keep-alive。
    pub fn suppress_keepalive(&self) -> bool {
        self.rtcp_port_invalid
    }

    /// F-13 的两处来源对 `port1` 要求不同：R34 的 source 要求 `port1 == 0`，
    /// R33 的 source 接受 `port1 != 0`（只要不等于 `port0`）。这里如实暴露差异，
    /// 供调用方决定策略——不替来源二选一。
    pub fn strict_aosp_view_would_reject(&self) -> bool {
        self.port1 != 0
    }
}

/// `Transport` 头解析结果（F-15/F-16）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    UdpUnicast {
        client_port: u16,
        rtcp_client_port: Option<u16>,
    },
    TcpInterleaved {
        first_channel: u8,
        second_channel: u8,
    },
}

impl Transport {
    /// 解析 `Transport` 头；`used_fallback_port` 为真表示命中了 F-16 的 19000 回退。
    pub fn parse(value: &str) -> Result<(Self, bool), Error> {
        let mut parts = value.split(';');
        let profile = parts.next().unwrap_or("").trim();
        let mut unicast = false;
        let mut client_port: Option<u16> = None;
        let mut rtcp_client_port: Option<u16> = None;
        let mut interleaved: Option<(u8, u8)> = None;
        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if part == "unicast" {
                unicast = true;
            } else if let Some(raw) = part.strip_prefix("client_port=") {
                let (a, b) = match raw.split_once('-') {
                    Some((a, b)) => (a, Some(b)),
                    None => (raw, None),
                };
                let a: u16 = a
                    .parse()
                    .map_err(|_| invalid(format!("client_port 非法：{raw:?}")))?;
                if a == 0 {
                    return Err(invalid("client_port 不得为 0（F-15）"));
                }
                client_port = Some(a);
                if let Some(b) = b {
                    let b: u16 = b
                        .parse()
                        .map_err(|_| invalid(format!("client_port 第二端口非法：{raw:?}")))?;
                    if b == 0 {
                        return Err(invalid("client_port 第二端口不得为 0（F-15）"));
                    }
                    rtcp_client_port = Some(b);
                }
            } else if let Some(raw) = part.strip_prefix("interleaved=") {
                let (a, b) = raw
                    .split_once('-')
                    .ok_or_else(|| invalid(format!("interleaved 需要 `a-b`：{raw:?}")))?;
                let a: u8 = a
                    .parse()
                    .map_err(|_| invalid(format!("interleaved 通道非法：{raw:?}")))?;
                let b: u8 = b
                    .parse()
                    .map_err(|_| invalid(format!("interleaved 通道非法：{raw:?}")))?;
                if b < a {
                    return Err(invalid(format!("interleaved 通道顺序非法：{raw:?}")));
                }
                interleaved = Some((a, b));
            }
            // server_port 等其余参数对收流侧无用：忽略值但不拒绝（F-15 只固定形状）。
        }

        match profile {
            "RTP/AVP/UDP" | "RTP/AVP" => {
                if interleaved.is_some() {
                    return Err(invalid("UDP profile 不应携带 interleaved（F-15）"));
                }
                let _ = unicast;
                match client_port {
                    Some(client_port) => Ok((
                        Self::UdpUnicast {
                            client_port,
                            rtcp_client_port,
                        },
                        false,
                    )),
                    // F-16：老 LG dongle 只写 `RTP/AVP/UDP;unicast`，此时按固定 19000 收流。
                    None => Ok((
                        Self::UdpUnicast {
                            client_port: RTP_PORT_FALLBACK,
                            rtcp_client_port: None,
                        },
                        true,
                    )),
                }
            }
            "RTP/AVP/TCP" => {
                let (first_channel, second_channel) =
                    interleaved.ok_or_else(|| invalid("TCP profile 缺 interleaved（F-15）"))?;
                Ok((
                    Self::TcpInterleaved {
                        first_channel,
                        second_channel,
                    },
                    false,
                ))
            }
            other => Err(invalid(format!("未知 Transport profile {other:?}（F-15）"))),
        }
    }

    pub fn to_wire(&self) -> String {
        match self {
            Self::UdpUnicast {
                client_port,
                rtcp_client_port,
            } => match rtcp_client_port {
                Some(rtcp) => format!("RTP/AVP/UDP;unicast;client_port={client_port}-{rtcp}"),
                None => format!("RTP/AVP/UDP;unicast;client_port={client_port}"),
            },
            Self::TcpInterleaved {
                first_channel,
                second_channel,
            } => format!("RTP/AVP/TCP;interleaved={first_channel}-{second_channel}"),
        }
    }
}

// ---------------------------------------------------------------- 内容保护（F-22）

/// `wfd_content_protection` 的取值（F-22）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentProtection {
    None,
    /// 对端要求 HDCP：本仓**不实现**握手，必须明确拒绝（绝不静默忽略后继续）。
    Hdcp {
        version: String,
        port: Option<u16>,
        attributes: Vec<(String, String)>,
    },
}

impl ContentProtection {
    pub fn parse(value: &str) -> Result<Self, Error> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("none") {
            return Ok(Self::None);
        }
        for version in ["2.0", "2.1"] {
            let prefix = format!("HDCP{version} ");
            if let Some(rest) = value.strip_prefix(prefix.as_str()) {
                let mut port = None;
                let mut attributes = Vec::new();
                for attribute in rest.split(',') {
                    let attribute = attribute.trim();
                    if attribute.is_empty() {
                        continue;
                    }
                    let (name, raw) = attribute
                        .split_once('=')
                        .ok_or_else(|| invalid(format!("HDCP 属性缺少 `=`：{attribute:?}")))?;
                    let name = name.trim().to_string();
                    let raw = raw.trim().to_string();
                    if name == "port" {
                        port = Some(
                            raw.parse()
                                .map_err(|_| invalid(format!("HDCP port 非法：{raw:?}")))?,
                        );
                    }
                    attributes.push((name, raw));
                }
                return Ok(Self::Hdcp {
                    version: version.to_string(),
                    port,
                    attributes,
                });
            }
        }
        Err(invalid(format!(
            "未知的 wfd_content_protection 取值 {}（F-22 只固定 none 与 HDCP2.0/2.1 前缀）",
            preview(value)
        )))
    }

    /// 对端是否要求内容保护（要求 → 本仓必须拒绝）。
    pub fn requires_hdcp(&self) -> bool {
        matches!(self, Self::Hdcp { .. })
    }

    /// 我们对外声明的取值（F-22：R32/R37 的 sink 都声明 `none`）。
    pub fn advertised(&self) -> &'static str {
        "none"
    }
}

// ---------------------------------------------------------------- 交集选择

/// 取双方都支持的第一条视频格式（按**远端声明的顺序**，不排序、不猜画质偏好）。
///
/// 判据：profile 位图有交集、level 位图有交集、三张分辨率表至少一张有交集。
/// 无交集 → `unsupported-feature`（**不静默降级**）。
pub fn select_common(
    local: &VideoFormatSet,
    remote: &VideoFormatSet,
) -> Result<VideoFormatDescriptor, Error> {
    if local.is_empty() || remote.is_empty() {
        return Err(unsupported(
            "本端或对端未声明任何视频格式（广告为空 → 不能假装能显示）",
        ));
    }
    for candidate in &remote.formats {
        for ours in &local.formats {
            let profile_ok = ours.profile.contains(candidate.profile);
            let level_ok = ours.level.highest_common(candidate.level).is_some();
            let resolution_ok = (ours.cea_sup & candidate.cea_sup) != 0
                || (ours.vesa_sup & candidate.vesa_sup) != 0
                || (ours.hh_sup & candidate.hh_sup) != 0;
            if profile_ok && level_ok && resolution_ok {
                return Ok(candidate.clone());
            }
        }
    }
    Err(unsupported(
        "双方声明里没有 profile/level/分辨率三重交集（F-17/F-18）",
    ))
}

/// 取双方都支持的第一条音频格式（Lpcm/Aac 优先于 Ac3，仅按远端顺序扫描）。
pub fn select_common_audio(
    local: &AudioCodecSet,
    remote: &AudioCodecSet,
) -> Result<AudioCodecDescriptor, Error> {
    if local.is_empty() || remote.is_empty() {
        return Err(unsupported("本端或对端未声明任何音频编码"));
    }
    for candidate in &remote.codecs {
        if let Some(ours) = local
            .codecs
            .iter()
            .find(|c| c.codec == candidate.codec && c.modes & candidate.modes != 0)
        {
            return Ok(AudioCodecDescriptor {
                codec: candidate.codec,
                modes: ours.modes & candidate.modes,
                latency_field: candidate.latency_field,
            });
        }
    }
    Err(unsupported("双方声明里没有共同的音频编码/mode 位"))
}

/// 解析 `wfd_video_formats` 取值（F-17）。
pub fn parse_video_formats(value: &str) -> Result<VideoFormatSet, Error> {
    VideoFormatSet::parse(value)
}

/// 解析 `wfd_audio_codecs` 取值（F-20）。
pub fn parse_audio_codecs(value: &str) -> Result<AudioCodecSet, Error> {
    AudioCodecSet::parse(value)
}

/// `wfd_video_formats` 等参数的登记名（供演示与测试引用，避免各处硬编码字符串）。
pub const PARAM_NAMES: [&str; 4] = [
    PARAM_VIDEO_FORMATS,
    PARAM_AUDIO_CODECS,
    PARAM_CLIENT_RTP_PORTS,
    PARAM_PRESENTATION_URL,
];
