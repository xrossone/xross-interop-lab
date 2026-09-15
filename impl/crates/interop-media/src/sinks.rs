//! Sink（plans/03 T29）：null / file 全 headless；native 窗口留待桌面会话。
//!
//! 分层立场：
//! - **null sink**：只统计帧/字节/时间基——没有 DISPLAY、没有音频设备也能跑，**不初始化任何 UI**；
//! - **file sink**：把自有测试内容的 payload 顺序落盘，并留下可复核的帧索引（帧数/字节数/路径）；
//! - **native window sink**：本阶段明确返回 `platform-unavailable`——不开假窗口、更不以屏幕录制
//!   假装 raw output（docs/04 §7 的红线）。真窗口留待桌面会话手动验证。
//!
//! PCM 采样率转换必须是**显式节点**（[`ResamplePlan`]）：同率直通，异率重采样且时长不变；
//! 绝不静默按错误采样率播放（那会变速）。

use crate::stream::FormatTracker;
use interop_contract::error::{Error, ErrorCode};
use interop_ipc::media_frame::{flags, FrameKind, MediaFrame};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 单帧/单画面的像素上限（防"巨大 dimensions"：分配前拒绝）。
pub const MAX_PIXELS: u64 = 8192 * 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    /// 解析 `WxH`（形状非法 → `invalid-frame`）。
    pub fn parse(s: &str) -> Result<Self, Error> {
        let (w, h) = s
            .split_once(['x', 'X'])
            .ok_or_else(|| invalid(format!("dimensions {s:?} 必须是 WxH")))?;
        let width: u32 = w
            .trim()
            .parse()
            .map_err(|_| invalid(format!("宽度非法：{w:?}")))?;
        let height: u32 = h
            .trim()
            .parse()
            .map_err(|_| invalid(format!("高度非法：{h:?}")))?;
        if width == 0 || height == 0 {
            return Err(invalid("宽高必须为正"));
        }
        Ok(Self { width, height })
    }

    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// 像素预算检查（**分配前**）：超限 → `resource-limit`。
    pub fn check_bounded(&self, max_pixels: u64) -> Result<(), Error> {
        if self.area() > max_pixels {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!(
                    "dimensions {}x{} = {} 像素超过上限 {max_pixels}（分配前拒绝）",
                    self.width,
                    self.height,
                    self.area()
                ),
            )
            .with_phase("negotiating"));
        }
        Ok(())
    }
}

/// 采样率转换计划（显式；`Passthrough` 只在同率时出现）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResamplePlan {
    Passthrough,
    Resample {
        source_rate: u32,
        device_rate: u32,
        /// 每输出样本对应的源样本位置步长 = source_rate / device_rate
        ratio_num: u32,
        ratio_den: u32,
    },
}

impl ResamplePlan {
    pub fn plan(source_rate: u32, device_rate: u32) -> Result<Self, Error> {
        if source_rate == 0 || device_rate == 0 {
            return Err(invalid("采样率必须为正"));
        }
        if source_rate == device_rate {
            return Ok(Self::Passthrough);
        }
        let g = gcd(source_rate, device_rate);
        Ok(Self::Resample {
            source_rate,
            device_rate,
            ratio_num: source_rate / g,
            ratio_den: device_rate / g,
        })
    }

    /// 对交错 S16LE 样本应用计划（线性插值）。`channels` 用于按帧交错处理。
    /// 时长保持：输出帧数 = 输入帧数 × device_rate / source_rate（**不变速**）。
    pub fn apply_s16le(&self, input: &[i16], channels: u8) -> Vec<i16> {
        let channels = channels.max(1) as usize;
        let plan = *self;
        let (src_rate, dst_rate) = match plan {
            ResamplePlan::Passthrough => return input.to_vec(),
            ResamplePlan::Resample {
                source_rate,
                device_rate,
                ..
            } => (source_rate as u128, device_rate as u128),
        };
        let frames_in = input.len() / channels;
        if frames_in == 0 {
            return Vec::new();
        }
        let frames_out = ((frames_in as u128 * dst_rate) / src_rate) as usize;
        let mut out = vec![0i16; frames_out * channels];
        for f in 0..frames_out {
            // 源位置（定点：pos = f * src / dst）
            let pos_num = f as u128 * src_rate;
            let idx = (pos_num / dst_rate) as usize;
            let frac = (pos_num % dst_rate) as u64;
            let idx = idx.min(frames_in - 1);
            let next = (idx + 1).min(frames_in - 1);
            for c in 0..channels {
                let a = input[idx * channels + c] as i64;
                let b = input[next * channels + c] as i64;
                let mixed = a + ((b - a) * frac as i64) / dst_rate as i64;
                out[f * channels + c] = mixed.clamp(i16::MIN as i64, i16::MAX as i64) as i16;
            }
        }
        out
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// sink 统计（收尾时返回；diagnostics/demo 直接消费）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SinkStats {
    pub frames: u64,
    pub bytes: u64,
    pub first_pts: Option<i64>,
    pub last_pts: Option<i64>,
    pub format_changes: u64,
    pub discontinuities: u64,
    pub codec: Option<String>,
    pub dimensions: Option<Dimensions>,
    /// file sink 的落盘路径
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkKind {
    Null,
    File,
    /// 桌面会话的真实窗口（本阶段不可用）
    NativeWindow,
}

#[derive(Debug, Clone, Default)]
pub struct SinkOptions {
    pub dimensions: Option<Dimensions>,
    pub path: Option<PathBuf>,
    /// 像素预算（默认 [`MAX_PIXELS`]）
    pub max_pixels: Option<u64>,
}

pub trait MediaSink {
    fn kind(&self) -> SinkKind;
    fn on_config(&mut self, codec: &str, format_id: u32, dims: Option<Dimensions>)
        -> Result<(), Error>;
    fn on_frame(&mut self, frame: &MediaFrame) -> Result<(), Error>;
    fn stats(&self) -> SinkStats;
    fn finish(&mut self) -> Result<SinkStats, Error>;
}

/// 打开 sink。native 窗口在本阶段明确不可用（在 dimensions 校验之后、分配之前）。
pub fn open_sink(kind: SinkKind, opts: SinkOptions) -> Result<Box<dyn MediaSink>, Error> {
    let max_pixels = opts.max_pixels.unwrap_or(MAX_PIXELS);
    if let Some(d) = opts.dimensions {
        d.check_bounded(max_pixels)?;
    }
    match kind {
        SinkKind::Null => Ok(Box::new(NullSink::new(opts.dimensions))),
        SinkKind::File => {
            let path = opts.path.clone().ok_or_else(|| {
                invalid("file sink 必须给 path（不猜默认落盘位置）")
            })?;
            Ok(Box::new(FileSink::new(&path, opts.dimensions)?))
        }
        SinkKind::NativeWindow => Err(Error::new(
            ErrorCode::PlatformUnavailable,
            "native window sink 未实现：需要桌面会话与真实窗口（留待手动验证；不以屏幕录制假装 raw output）",
        )
        .with_phase("negotiating")),
    }
}

/// 统计型 sink：headless 默认后端。
struct NullSink {
    stats: SinkStats,
    tracker: FormatTracker,
}

impl NullSink {
    fn new(dims: Option<Dimensions>) -> Self {
        Self {
            stats: SinkStats {
                dimensions: dims,
                ..SinkStats::default()
            },
            tracker: FormatTracker::new(),
        }
    }
}

impl MediaSink for NullSink {
    fn kind(&self) -> SinkKind {
        SinkKind::Null
    }

    fn on_config(
        &mut self,
        codec: &str,
        format_id: u32,
        dims: Option<Dimensions>,
    ) -> Result<(), Error> {
        // sink 收到 `on_config` 即表示"调用方要求重启该格式的解码/输出"：
        // 「变化必须带 discontinuity」的校验属于协议层（只有它看得见 sender 的 config 帧语义，
        // 例如 `proto-airplay::mirror::MirrorTrack`）；sink 侧不重复判它，否则协议层校验通过
        // 之后仍会在 sink 里被拒（2026-09-15 T33 发现的接口缺口）。
        let event = self.tracker.observe_config(format_id, true)?;
        if matches!(event, crate::stream::FormatEvent::Initialized | crate::stream::FormatEvent::Reset)
        {
            self.stats.format_changes += 1;
        }
        self.stats.codec = Some(codec.to_string());
        if dims.is_some() {
            self.stats.dimensions = dims;
        }
        Ok(())
    }

    fn on_frame(&mut self, frame: &MediaFrame) -> Result<(), Error> {
        self.stats.frames += 1;
        self.stats.bytes += frame.payload.len() as u64;
        if frame.has_flag(flags::DISCONTINUITY) {
            self.stats.discontinuities += 1;
        }
        if let Some(pts) = frame.pts_if_present() {
            if self.stats.first_pts.is_none() {
                self.stats.first_pts = Some(pts);
            }
            self.stats.last_pts = Some(pts);
        }
        Ok(())
    }

    fn stats(&self) -> SinkStats {
        self.stats.clone()
    }

    fn finish(&mut self) -> Result<SinkStats, Error> {
        Ok(self.stats.clone())
    }
}

/// 文件 sink：自有内容顺序落盘（payload 串接），并记录帧索引统计。
struct FileSink {
    file: File,
    stats: SinkStats,
    tracker: FormatTracker,
}

impl FileSink {
    fn new(path: &Path, dims: Option<Dimensions>) -> Result<Self, Error> {
        let file = File::create(path).map_err(|e| {
            Error::new(
                ErrorCode::DestinationUnavailable,
                format!("无法创建 file sink {}：{e}", path.display()),
            )
            .with_phase("committing")
        })?;
        Ok(Self {
            file,
            stats: SinkStats {
                dimensions: dims,
                path: Some(path.to_string_lossy().to_string()),
                ..SinkStats::default()
            },
            tracker: FormatTracker::new(),
        })
    }
}

impl MediaSink for FileSink {
    fn kind(&self) -> SinkKind {
        SinkKind::File
    }

    fn on_config(
        &mut self,
        codec: &str,
        format_id: u32,
        dims: Option<Dimensions>,
    ) -> Result<(), Error> {
        // sink 收到 `on_config` 即表示"调用方要求重启该格式的解码/输出"：
        // 「变化必须带 discontinuity」的校验属于协议层（只有它看得见 sender 的 config 帧语义，
        // 例如 `proto-airplay::mirror::MirrorTrack`）；sink 侧不重复判它，否则协议层校验通过
        // 之后仍会在 sink 里被拒（2026-09-15 T33 发现的接口缺口）。
        let event = self.tracker.observe_config(format_id, true)?;
        if matches!(event, crate::stream::FormatEvent::Initialized | crate::stream::FormatEvent::Reset)
        {
            self.stats.format_changes += 1;
        }
        self.stats.codec = Some(codec.to_string());
        if dims.is_some() {
            self.stats.dimensions = dims;
        }
        Ok(())
    }

    fn on_frame(&mut self, frame: &MediaFrame) -> Result<(), Error> {
        // codec config 不进内容文件（只有 access unit / PCM block 是媒体内容）
        if matches!(frame.kind, FrameKind::EncodedAccessUnit | FrameKind::PcmBlock) {
            self.file.write_all(&frame.payload).map_err(|e| {
                Error::new(
                    ErrorCode::DestinationUnavailable,
                    format!("file sink 写入失败：{e}"),
                )
                .with_phase("transferring")
            })?;
            self.stats.bytes += frame.payload.len() as u64;
        }
        self.stats.frames += 1;
        if frame.has_flag(flags::DISCONTINUITY) {
            self.stats.discontinuities += 1;
        }
        if let Some(pts) = frame.pts_if_present() {
            if self.stats.first_pts.is_none() {
                self.stats.first_pts = Some(pts);
            }
            self.stats.last_pts = Some(pts);
        }
        Ok(())
    }

    fn stats(&self) -> SinkStats {
        self.stats.clone()
    }

    fn finish(&mut self) -> Result<SinkStats, Error> {
        self.file.flush().map_err(|e| {
            Error::new(
                ErrorCode::DestinationUnavailable,
                format!("file sink flush 失败：{e}"),
            )
            .with_phase("committing")
        })?;
        Ok(self.stats.clone())
    }
}

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("negotiating")
}
