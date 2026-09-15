//! AirPlay 音频 profile（T34）：AP1 实时（`ct`/`spf`/采样率）与 AP2 的**两套**编号。
//!
//! 字段来源见 `specs-reviewed/m01-airplay-legacy-mirror.md` 的「T34 增量」小节，每行带
//! `UxPlay@d9791de:...` / `shairplay-rust@2fb72b3:...` 行级引用。三条纪律：
//!
//! 1. **AP2 打包 `audioFormat` 与 RTP SSRC 魔数是两套编号**（参考实现注释明说
//!    "format-capability bit values, not RTP SSRC values"）→ 本模块给两个独立的解析入口，
//!    绝不互相回退。
//! 2. **未知值一律拒绝**（`UnsupportedProfile`），不猜、不给默认 profile。
//! 3. 本模块**只做解析与校验**：不含任何解码器，也不含任何加密原语（lab gate 机器检查）。

use interop_contract::error::{Error, ErrorCode};

/// AP1 实时音频的采样率（`UxPlay@d9791de:lib/raop_handlers.h:27`：所有已支持格式都是这个率）。
/// **来源取值**，不是本仓自定；AP2 的打包格式另有 48000 的取值（见下）。
pub const AP1_AUDIO_SAMPLE_RATE_HZ: u32 = 44_100;

/// 流类型（`streams[].type`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    /// 96：实时音频（AP1 legacy 路径；AP2 也用，键含 `shk`）。
    RealtimeAudio,
    /// 103：AP2 buffered 音频（TCP + ChaCha20-Poly1305 + AAC，响应含 `audioBufferSize`）。
    BufferedAudio,
    /// 110：镜像视频（响应只有 `dataPort`/`type`）。
    MirrorVideo,
    /// 120：Apple Music 视频——参考实现明说未实现。
    AppleMusicVideo,
    /// 130：AP2 遥控数据通道（请求 `seed`，响应 `streamID`/`dataPort`）。
    RemoteControlData,
}

/// 本仓能否路由某个流类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRouting {
    /// 本仓能处理（T32/T33 的镜像与 PCM 音频路径）。
    Routable,
    /// 明确不实现，附带原因（不得静默接受、不得静默忽略）。
    NotImplemented(&'static str),
}

impl StreamType {
    pub fn from_wire(value: u64) -> Result<Self, Error> {
        match value {
            96 => Ok(Self::RealtimeAudio),
            103 => Ok(Self::BufferedAudio),
            110 => Ok(Self::MirrorVideo),
            120 => Ok(Self::AppleMusicVideo),
            130 => Ok(Self::RemoteControlData),
            other => Err(Error::new(
                ErrorCode::UnsupportedProfile,
                format!("未知流类型 {other}（字段表只登记 96/103/110/120/130，不得猜测）"),
            )),
        }
    }

    pub fn as_wire(self) -> u64 {
        match self {
            Self::RealtimeAudio => 96,
            Self::BufferedAudio => 103,
            Self::MirrorVideo => 110,
            Self::AppleMusicVideo => 120,
            Self::RemoteControlData => 130,
        }
    }

    /// 本仓对该流类型的路由能力（**不实现的部分要说清原因**）。
    pub fn routing(self) -> StreamRouting {
        match self {
            Self::RealtimeAudio => StreamRouting::Routable,
            Self::MirrorVideo => StreamRouting::Routable,
            Self::BufferedAudio => StreamRouting::NotImplemented(
                "AP2 buffered 音频需要 ChaCha20-Poly1305 与 AAC 解码器（参考实现用 symphonia）",
            ),
            Self::AppleMusicVideo => {
                StreamRouting::NotImplemented("参考实现明说 Apple Music 视频未实现")
            }
            Self::RemoteControlData => {
                StreamRouting::NotImplemented("遥控数据通道不在本仓范围（只做媒体与控制面）")
            }
        }
    }

    /// 响应流字典必须带的键（键名与顺序来自字段表；110 **没有** `controlPort`）。
    pub fn response_keys(self) -> &'static [&'static str] {
        match self {
            Self::RealtimeAudio => &["dataPort", "controlPort", "type"],
            Self::BufferedAudio => &["dataPort", "audioBufferSize", "type"],
            Self::MirrorVideo => &["dataPort", "type"],
            Self::AppleMusicVideo | Self::RemoteControlData => &[],
        }
    }

    /// 请求流字典允许出现的键（本仓只认字段表登记过的键；其余键忽略但仍记账）。
    pub fn known_request_keys(self) -> &'static [&'static str] {
        match self {
            Self::RealtimeAudio => &["controlPort", "ct", "spf", "audioFormat", "isMedia", "usingScreen", "type"],
            Self::BufferedAudio => &["audioFormat", "shk", "sr", "spf", "type"],
            Self::MirrorVideo => &["streamConnectionID", "type"],
            Self::AppleMusicVideo | Self::RemoteControlData => &["type"],
        }
    }
}

/// AP1 压缩类型 `ct`（`UxPlay@d9791de:renderers/audio_renderer.c:57-68` 的 caps 注释）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    /// `ct = 1`：线性 PCM（未压缩）44100/16/2。
    LinearPcm,
    /// `ct = 2`：ALAC 44100/16/2，`spf = 352`。
    Alac,
    /// `ct = 4`：AAC-LC 44100/2，`spf = 1024`。
    AacLc,
    /// `ct = 8`：AAC-ELD 44100/2，`spf = 480`。
    AacEld,
}

impl CompressionType {
    pub fn from_wire(value: u64) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::LinearPcm),
            2 => Ok(Self::Alac),
            4 => Ok(Self::AacLc),
            8 => Ok(Self::AacEld),
            other => Err(Error::new(
                ErrorCode::UnsupportedProfile,
                format!("未知压缩类型 ct={other}（字段表只有 1/2/4/8；不得回退到默认 profile）"),
            )),
        }
    }

    pub fn as_wire(self) -> u64 {
        match self {
            Self::LinearPcm => 1,
            Self::Alac => 2,
            Self::AacLc => 4,
            Self::AacEld => 8,
        }
    }

    /// 每帧采样数（来源取值）。**PCM 没有 spf**（caps 未给）→ `None`，不得替它编一个。
    pub fn samples_per_frame(self) -> Option<u16> {
        match self {
            Self::LinearPcm => None,
            Self::Alac => Some(352),
            Self::AacLc => Some(1024),
            Self::AacEld => Some(480),
        }
    }

    /// 本仓**能否解码**该编码：只有 PCM 能直接路由，其余需要 decoder。
    pub fn decodable_here(self) -> bool {
        matches!(self, Self::LinearPcm)
    }
}

/// AAC-ELD 流起始处可能出现的 4 字节 no-data 标记（`UxPlay@d9791de:lib/raop_rtp.c:566-567`）。
pub const AAC_ELD_NO_DATA_MARKER: [u8; 4] = [0x00, 0x68, 0x34, 0x00];

/// 该正文是否是 AAC-ELD 的 no-data 标记（**只对 AAC-ELD 有意义**）。
pub fn is_aac_eld_no_data(body: &[u8]) -> bool {
    body == AAC_ELD_NO_DATA_MARKER
}

/// 音频 profile（编码 + 采样率 + 位深 + 声道）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioProfile {
    pub codec: ProfileCodec,
    pub sample_rate_hz: u32,
    pub bit_depth: u16,
    pub channels: u8,
}

/// profile 里的编码（与 AP1 的 `ct`、AP1 广告的 `cn` 是不同编号空间）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileCodec {
    /// 线性 PCM。
    Pcm,
    /// ALAC（Apple Lossless）。
    Alac,
    /// AAC（AP2 buffered/SSRC 魔数里的 AAC 族）。
    Aac,
}

/// AP2 打包 `audioFormat` 的解析（**capability 位值**，与 RTP SSRC 无关）。
///
/// 四个取值来自 `shairplay-rust@2fb72b3:src/codec/alac.rs:47-69`；其余一律拒绝。
pub fn parse_packed_audio_format(value: u64) -> Result<AudioProfile, Error> {
    let (sample_rate_hz, bit_depth) = match value {
        0x0004_0000 => (44_100u32, 16u16),
        0x0008_0000 => (44_100, 24),
        0x0010_0000 => (48_000, 16),
        0x0020_0000 => (48_000, 24),
        other => {
            return Err(Error::new(
                ErrorCode::UnsupportedProfile,
                format!(
                    "未知打包 audioFormat 0x{other:08X}（字段表只有 0x00040000/0x00080000/0x00100000/0x00200000）"
                ),
            ))
        }
    };
    Ok(AudioProfile {
        codec: ProfileCodec::Alac,
        sample_rate_hz,
        bit_depth,
        channels: 2,
    })
}

/// AP2 buffered 流解密后 RTP SSRC 字段（`packet[8:12]`）的魔数。
///
/// 来自 `shairplay-rust@2fb72b3:src/codec/aac.rs:10-39`（其注释注明上游是 shairport-sync `player.h`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSsrc {
    /// `0`：未知/未识别——**不触发**格式变化。
    None,
    /// `0x0000FACE`：ALAC 44100/16/2。
    Alac44100S16Stereo,
    /// `0x15000000`：ALAC 48000/24/2。
    Alac48000S24Stereo,
    /// `0x16000000`：AAC 44100 24f/2。
    Aac44100F24Stereo,
    /// `0x17000000`：AAC 48000 24f/2。
    Aac48000F24Stereo,
    /// `0x27000000`：AAC 48000 24f 5.1。
    Aac48000F24Surround51,
    /// `0x28000000`：AAC 48000 24f 7.1。
    Aac48000F24Surround71,
}

impl AudioSsrc {
    /// 已知魔数 → 枚举；**未知值返回 [`AudioSsrc::None`]**（参考实现同此：不报错、也不当格式切换）。
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => Self::None,
            0x0000_FACE => Self::Alac44100S16Stereo,
            0x1500_0000 => Self::Alac48000S24Stereo,
            0x1600_0000 => Self::Aac44100F24Stereo,
            0x1700_0000 => Self::Aac48000F24Stereo,
            0x2700_0000 => Self::Aac48000F24Surround51,
            0x2800_0000 => Self::Aac48000F24Surround71,
            _ => Self::None,
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Alac44100S16Stereo => 0x0000_FACE,
            Self::Alac48000S24Stereo => 0x1500_0000,
            Self::Aac44100F24Stereo => 0x1600_0000,
            Self::Aac48000F24Stereo => 0x1700_0000,
            Self::Aac48000F24Surround51 => 0x2700_0000,
            Self::Aac48000F24Surround71 => 0x2800_0000,
        }
    }

    /// 魔数对应的 profile（`None` 变体没有 profile）。
    pub fn profile(self) -> Option<AudioProfile> {
        let (codec, sample_rate_hz, bit_depth, channels) = match self {
            Self::None => return None,
            Self::Alac44100S16Stereo => (ProfileCodec::Alac, 44_100, 16, 2),
            Self::Alac48000S24Stereo => (ProfileCodec::Alac, 48_000, 24, 2),
            Self::Aac44100F24Stereo => (ProfileCodec::Aac, 44_100, 24, 2),
            Self::Aac48000F24Stereo => (ProfileCodec::Aac, 48_000, 24, 2),
            Self::Aac48000F24Surround51 => (ProfileCodec::Aac, 48_000, 24, 6),
            Self::Aac48000F24Surround71 => (ProfileCodec::Aac, 48_000, 24, 8),
        };
        Some(AudioProfile {
            codec,
            sample_rate_hz,
            bit_depth,
            channels,
        })
    }

    /// 从解密后的 RTP 头取 SSRC（`packet[8:12]`，与 `seq`/`timestamp` 同处头部）。
    pub fn from_rtp_header(packet: &[u8]) -> Option<Self> {
        let bytes = packet.get(8..12)?;
        let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        Some(Self::from_u32(value))
    }
}

/// SSRC 变化跟踪：**只有"非 None 且与上一次不同"才算格式变化**（参考实现语义）。
#[derive(Debug, Clone, Default)]
pub struct SsrcTracker {
    current: Option<AudioSsrc>,
}

impl SsrcTracker {
    pub fn new() -> Self {
        Self { current: None }
    }

    /// 返回 `Some(新 profile)` 当且仅当检测到格式变化。
    pub fn observe(&mut self, ssrc: AudioSsrc) -> Option<AudioProfile> {
        if ssrc == AudioSsrc::None || self.current == Some(ssrc) {
            return None;
        }
        self.current = Some(ssrc);
        ssrc.profile()
    }

    pub fn current(&self) -> Option<AudioSsrc> {
        self.current
    }
}

/// 一次 AP1 实时音频 SETUP 请求里与音频 profile 相关的字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeAudioRequest {
    /// 对端控制端口（注释：为 0 则不激活重传请求）。
    pub control_port: u16,
    pub compression: CompressionType,
    /// 对端声明的 `spf`。与来源默认值不一致时只记录，不纠正。
    pub samples_per_frame: u16,
    /// 对端声明的打包 `audioFormat`（AP1 里参考实现只记录不解码 → 本仓同样只记录）。
    pub packed_audio_format: u64,
}

impl RealtimeAudioRequest {
    /// 校验 AP1 实时音频请求：`controlPort`/`ct`/`spf`/`audioFormat` 都必须出现
    /// （来源把它们按必需字段读取，缺失即无法协商）。
    pub fn validate(keys: &[(&str, u64)]) -> Result<Self, Error> {
        let get = |name: &str| keys.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
        let control_port = get("controlPort").ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidFrame,
                "AP1 音频 SETUP 缺 controlPort（字段表按必需字段读取）",
            )
        })?;
        let ct = get("ct").ok_or_else(|| {
            Error::new(ErrorCode::InvalidFrame, "AP1 音频 SETUP 缺 ct")
        })?;
        let spf = get("spf").ok_or_else(|| {
            Error::new(ErrorCode::InvalidFrame, "AP1 音频 SETUP 缺 spf")
        })?;
        let audio_format = get("audioFormat").ok_or_else(|| {
            Error::new(ErrorCode::InvalidFrame, "AP1 音频 SETUP 缺 audioFormat")
        })?;
        let compression = CompressionType::from_wire(ct)?;
        let control_port = u16::try_from(control_port).map_err(|_| {
            Error::new(
                ErrorCode::InvalidFrame,
                format!("controlPort {control_port} 超出 u16"),
            )
        })?;
        let samples_per_frame = u16::try_from(spf)
            .map_err(|_| Error::new(ErrorCode::InvalidFrame, format!("spf {spf} 超出 u16")))?;
        Ok(Self {
            control_port,
            compression,
            samples_per_frame,
            packed_audio_format: audio_format,
        })
    }

    /// 声明的 `spf` 是否与来源取值一致（不一致不是错误，只是需要记录）。
    pub fn spf_matches_source(&self) -> bool {
        match self.compression.samples_per_frame() {
            Some(expected) => expected == self.samples_per_frame,
            None => false,
        }
    }

    /// 本仓能否解码该请求的编码（PCM 可以，ALAC/AAC 需要 decoder）。
    pub fn decodable_here(&self) -> bool {
        self.compression.decodable_here()
    }
}

/// 音频 SETUP 响应（只回本端端口；**结构的键名与顺序来自字段表**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamResponse {
    stream_type: StreamType,
    keys: Vec<(&'static str, u64)>,
}

impl StreamResponse {
    /// 为某流类型构造响应；**不能路由的流类型直接拒绝**（不构造、不静默接受）。
    pub fn build(stream_type: StreamType, data_port: u16, control_port: Option<u16>) -> Result<Self, Error> {
        match stream_type.routing() {
            StreamRouting::Routable => {}
            StreamRouting::NotImplemented(reason) => {
                return Err(Error::new(
                    ErrorCode::UnsupportedFeature,
                    format!("流类型 {} 不实现：{reason}", stream_type.as_wire()),
                ))
            }
        }
        let keys = match stream_type {
            StreamType::RealtimeAudio => {
                let control_port = control_port.ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidFrame,
                        "音频流响应必须带 controlPort（字段表：96 的响应是 dataPort/controlPort/type）",
                    )
                })?;
                vec![
                    ("dataPort", u64::from(data_port)),
                    ("controlPort", u64::from(control_port)),
                    ("type", stream_type.as_wire()),
                ]
            }
            StreamType::MirrorVideo => vec![
                ("dataPort", u64::from(data_port)),
                ("type", stream_type.as_wire()),
            ],
            _ => unreachable!("不可路由的流类型已在上面拒绝"),
        };
        Ok(Self { stream_type, keys })
    }

    pub fn stream_type(&self) -> StreamType {
        self.stream_type
    }

    pub fn keys(&self) -> &[(&'static str, u64)] {
        &self.keys
    }

    /// 响应正文里是否出现了该流类型**不该有**的键（例：110 带 `controlPort`）。
    pub fn has_key(&self, name: &str) -> bool {
        self.keys.iter().any(|(k, _)| *k == name)
    }
}
