//! 媒体形态（docs/04 §7；plans/03 T28）：四类 typed source。
//!
//! - `EncodedStream`：codec 配置 + access units + timebase——可 export 给解码/重封装；
//! - `PcmStream`：显式采样率/声道/layout/格式——不按错误采样率播放；
//! - `NativePresentation`：owner 进程内的渲染对象——跨 IPC **只给 opaque id 与控制能力**，
//!   没有 export；要"导出"就只能靠屏幕录制，那正是被禁止的绕行（docs/04 §7）；
//! - `MediaResource`：URL/资源租约——由接收方自行取内容，遵守 fetch/credential policy。

use crate::clock::Timebase;
use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::PresentationId;

/// PCM 采样格式（显式声明，不做隐式转换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmFormat {
    S16Le,
    F32Le,
}

impl PcmFormat {
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::S16Le => "s16le",
            Self::F32Le => "f32le",
        }
    }
}

/// encoded 媒体源（可 export）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedSource {
    pub codec: String,
    /// format 变化时递增；必须伴随 decoder reset（见 [`crate::stream::FormatTracker`]）
    pub format_id: u32,
    pub timebase: Timebase,
    /// extradata 的内容 hash（不是 extradata 本身）
    pub codec_extradata_hash: Option<String>,
}

/// PCM 媒体源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmSource {
    pub sample_rate: u32,
    pub channels: u8,
    pub layout: String,
    pub sample_format: PcmFormat,
    pub timebase: Timebase,
}

/// native 呈现引用：**只有 opaque id**（owner 进程持有真实对象）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePresentationRef {
    pub presentation_id: PresentationId,
}

/// 资源租约引用（URL 由接收方自取）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaResourceRef {
    pub url: String,
    /// 凭据类别（如 none/token/cookie）——具体凭据永不过这条线
    pub credential_class: String,
}

/// 四类媒体源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaSource {
    Encoded(EncodedSource),
    Pcm(PcmSource),
    NativeOnly(NativePresentationRef),
    Resource(MediaResourceRef),
}

impl MediaSource {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Encoded(_) => "encoded-stream",
            Self::Pcm(_) => "pcm-stream",
            Self::NativeOnly(_) => "native-presentation",
            Self::Resource(_) => "media-resource",
        }
    }

    /// 导出 encoded 源。只有 `Encoded` 可以；native-only 明确 `unsupported-feature`
    /// （T28-04：不得以屏幕录制假装 raw output）。
    pub fn export_encoded(&self) -> Result<&EncodedSource, Error> {
        match self {
            Self::Encoded(e) => Ok(e),
            Self::Pcm(_) => Err(unsupported(
                "PCM 源不是 encoded stream：请走 PCM 通道，不要重编码",
            )),
            Self::NativeOnly(_) => Err(unsupported(
                "native-only 源没有 export 能力（只能播放，不能暗中录制）",
            )),
            Self::Resource(_) => Err(unsupported(
                "resource 源由接收方自取，daemon 不代抓内容",
            )),
        }
    }
}

fn unsupported(why: &str) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, why).with_phase("negotiating")
}
