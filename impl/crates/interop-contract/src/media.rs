//! 媒体形态与 MediaDescriptor（docs/04 §7、docs/05 §2、MEDIA-01/02/03）。

use crate::error::{Error, ErrorCode};
use crate::ids::SessionId;
use serde::{Deserialize, Serialize};

/// 四种媒体输出形态（docs/04 §7）。未知 critical form 必须 fail-closed（T06-02）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaForm {
    /// codec 配置 + access units + timebase/clock 映射
    EncodedStream,
    /// 显式 sample rate/channels/layout/format 的 PCM
    PcmStream,
    /// owner 进程内的系统媒体源/渲染对象（跨 IPC 只给 opaque presentation ID）
    NativePresentation,
    /// URL/resource lease（客户端/TV 自行取内容）
    MediaResource,
}

impl MediaForm {
    /// wire 字符串 → 形态。未知值返回 `unsupported-feature`（不是 default，不是静默忽略）。
    pub fn from_wire(s: &str) -> Result<Self, Error> {
        match s {
            "encoded-stream" => Ok(Self::EncodedStream),
            "pcm-stream" => Ok(Self::PcmStream),
            "native-presentation" => Ok(Self::NativePresentation),
            "media-resource" => Ok(Self::MediaResource),
            other => Err(Error::new(
                ErrorCode::UnsupportedFeature,
                format!("未知媒体形态 {other:?}：fail-closed，不猜测"),
            )
            .with_phase("negotiating")),
        }
    }

    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::EncodedStream => "encoded-stream",
            Self::PcmStream => "pcm-stream",
            Self::NativePresentation => "native-presentation",
            Self::MediaResource => "media-resource",
        }
    }
}

impl TryFrom<&str> for MediaForm {
    type Error = Error;
    fn try_from(s: &str) -> Result<Self, Error> {
        Self::from_wire(s)
    }
}

impl Serialize for MediaForm {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for MediaForm {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_wire(&s).map_err(serde::de::Error::custom)
    }
}

/// 会话意图（MEDIA-01：镜像/音频/URL cast 是不同 SessionIntent，URL cast 不能
/// 作为完整 screen mirror 成功）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionIntent {
    Mirror,
    Audio,
    MediaUrl,
    Screen,
}

/// 单轨描述（MEDIA-03：音视频分轨，保留 clock domain/timebase/PTS）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub codec: String,
    /// format change 递增 format_id，不换会话身份
    pub format_id: u32,
    /// 如 "1/90000"
    pub timebase: String,
    /// 时钟域标识；重连产生新的 clock generation
    pub clock_domain: String,
    /// 音频为 channel layout，视频为 dimensions（如 "1920x1080"）
    pub layout_or_dimensions: Option<String>,
    pub transport_security: String,
    /// extradata 的内容 hash（十六进制），不是 extradata 本身
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codec_extradata_hash: Option<String>,
}

/// 媒体会话描述（docs/05 §2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaDescriptor {
    pub session_id: SessionId,
    pub intent: SessionIntent,
    pub tracks: Vec<Track>,
    pub source_form: MediaForm,
    /// 按能力授权的控制集合（MEDIA-06：live mirror 无 seek 就不含 "seek"）
    #[serde(default)]
    pub controls: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub owner_provider: String,
}
