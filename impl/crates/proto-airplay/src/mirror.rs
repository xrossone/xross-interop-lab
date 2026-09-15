//! 视频镜像轨道（plans/03 §T33）：把 AirPlay 控制链协商出的格式与 encoded access unit
//! 路由到 `interop-media` 的 sink。
//!
//! **本模块不含 decoder**：payload 原样进 sink（null 统计 / file 落盘）。因此：
//! - 能力广告保持为空（没有 decoder 就不广告 codec）——`ImplementationInventory` 不动；
//! - 只有 [`MirrorCodec::H264Passthrough`] 是可路由的编码（明确命名"直通"而非"解码"）；
//!   HEVC 等其它 codec 一律 `unsupported-feature`。
//!
//! 背压策略（T33 实现决策）：缓冲区超过预算时**不静默丢帧**，而是进入"等关键帧"恢复态：
//! 丢弃后续非关键帧，直到关键帧到达时清空待处理并重新对齐（画面恢复）。
//! format 变化沿用 `interop-media` 的规则：`format_id` 变化必须带 discontinuity，否则拒绝。

use interop_contract::error::{Error, ErrorCode};
use interop_ipc::media_frame::{flags, FrameKind, MediaFrame, MAX_PAYLOAD};
use interop_media::sinks::{Dimensions, MediaSink, SinkStats, MAX_PIXELS};
use interop_media::stream::{FormatEvent, FormatTracker, FrameQueue};

/// 可路由的编码（**直通**：不解码、不显示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorCodec {
    /// H.264 encoded access unit 直通到 sink
    H264Passthrough,
    /// HEVC：本仓无 decoder → 拒绝
    Hevc,
}

impl MirrorCodec {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::H264Passthrough => "h264",
            Self::Hevc => "hevc",
        }
    }

    fn routable(self) -> Result<Self, Error> {
        match self {
            Self::H264Passthrough => Ok(self),
            Self::Hevc => Err(Error::new(
                ErrorCode::UnsupportedFeature,
                "HEVC 无 decoder：本仓不实现（也不广告）",
            )
            .with_phase("negotiating")),
        }
    }
}

/// 一帧的处置结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forwarded {
    /// 画面帧已交给 sink
    Frame,
    /// codec config 帧已交给 sink 的 `on_config`
    Config,
    /// 流结束帧已交给 sink
    End,
    /// 被丢弃（附原因：等待关键帧 / 超预算）
    Dropped(&'static str),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MirrorStats {
    pub frames_forwarded: u64,
    pub frames_dropped: u64,
    /// 进入"等关键帧"恢复态的次数
    pub recoveries: u64,
    pub configs: u64,
    pub bytes_forwarded: u64,
    pub format_changes: u64,
}

/// 视频镜像轨道（单流）。
pub struct MirrorTrack {
    sink: Box<dyn MediaSink>,
    queue: FrameQueue,
    queue_limits: (usize, usize),
    tracker: FormatTracker,
    codec: Option<MirrorCodec>,
    dims: Option<Dimensions>,
    format_id: u32,
    /// 恢复态：等待关键帧（背压超预算后进入）
    recovering: bool,
    /// 是否即时排空队列（false = 模拟下游背压，测试与真机慢 sink 都用得到）
    drain: bool,
    stats: MirrorStats,
    peak_queued_bytes: usize,
}

impl std::fmt::Debug for MirrorTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MirrorTrack(codec={:?}, dims={:?}, forwarded={}, dropped={}, recovering={})",
            self.codec, self.dims, self.stats.frames_forwarded, self.stats.frames_dropped, self.recovering
        )
    }
}

impl MirrorTrack {
    /// `queue_budget_bytes` 是本轨道的缓冲预算（超过即进入关键帧恢复）。
    pub fn new(sink: Box<dyn MediaSink>, queue_budget_bytes: usize) -> Self {
        let max_frames = 8;
        Self {
            sink,
            queue: FrameQueue::new(max_frames, queue_budget_bytes),
            queue_limits: (max_frames, queue_budget_bytes),
            tracker: FormatTracker::new(),
            codec: None,
            dims: None,
            format_id: 0,
            recovering: false,
            drain: true,
            stats: MirrorStats::default(),
            peak_queued_bytes: 0,
        }
    }

    /// 模拟下游背压：`false` 时帧留在队列里（不会喂给 sink）。
    pub fn set_drain(&mut self, drain: bool) {
        self.drain = drain;
    }

    /// 协商结果落到轨道：codec/尺寸/format_id（`discontinuity` 由控制链给出）。
    pub fn configure(
        &mut self,
        codec: MirrorCodec,
        format_id: u32,
        dims: Dimensions,
        discontinuity: bool,
    ) -> Result<(), Error> {
        let codec = codec.routable()?;
        // 尺寸预算检查在**分配/落 sink 之前**
        dims.check_bounded(MAX_PIXELS)?;
        let event = self.tracker.observe_config(format_id, discontinuity)?;
        if matches!(event, FormatEvent::Initialized | FormatEvent::Reset) {
            self.stats.format_changes += 1;
        }
        self.codec = Some(codec);
        self.dims = Some(dims);
        self.format_id = format_id;
        self.sink
            .on_config(codec.as_wire(), format_id, Some(dims))?;
        // 新格式意味着画面重新开始：退出恢复态并清空旧格式的待处理帧
        self.recovering = false;
        self.flush_queue()?;
        Ok(())
    }

    pub fn codec(&self) -> Option<MirrorCodec> {
        self.codec
    }

    pub fn dimensions(&self) -> Option<Dimensions> {
        self.dims
    }

    pub fn format_id(&self) -> u32 {
        self.format_id
    }

    pub fn stats(&self) -> MirrorStats {
        self.stats
    }

    /// 缓冲高水位（内存趋势代理指标）。
    pub fn peak_queued_bytes(&self) -> usize {
        self.peak_queued_bytes
    }

    pub fn is_recovering(&self) -> bool {
        self.recovering
    }

    /// 处理一帧。配置之前的画面帧、超限 payload、PCM 帧都被明确拒绝。
    pub fn on_frame(&mut self, frame: MediaFrame) -> Result<Forwarded, Error> {
        match frame.kind {
            FrameKind::CodecConfig => {
                let (codec, dims) = match (self.codec, self.dims) {
                    (Some(c), Some(d)) => (c, d),
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidFrame,
                            "codec config 帧到达时尚未协商格式（先 configure）",
                        )
                        .with_phase("negotiating"))
                    }
                };
                self.sink
                    .on_config(codec.as_wire(), self.format_id, Some(dims))?;
                self.stats.configs += 1;
                Ok(Forwarded::Config)
            }
            FrameKind::EncodedAccessUnit => self.on_access_unit(frame),
            FrameKind::PcmBlock => Err(Error::new(
                ErrorCode::UnsupportedFeature,
                "PCM 帧属于音频轨道（AudioTrack），不走视频镜像轨道",
            )
            .with_phase("transferring")),
            FrameKind::End => {
                if self.dims.is_none() {
                    return Err(Error::new(
                        ErrorCode::InvalidFrame,
                        "流结束帧到达时尚未配置格式",
                    )
                    .with_phase("negotiating"));
                }
                self.flush_queue()?;
                Ok(Forwarded::End)
            }
        }
    }

    fn on_access_unit(&mut self, frame: MediaFrame) -> Result<Forwarded, Error> {
        if self.dims.is_none() {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "未协商格式就收到画面帧：拒绝（不得无配置写入）",
            )
            .with_phase("negotiating"));
        }
        if frame.payload.len() > MAX_PAYLOAD as usize {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("画面帧 {} 字节超过 XMD1 上限 {MAX_PAYLOAD}", frame.payload.len()),
            )
            .with_phase("transferring"));
        }
        let keyframe = frame.has_flag(flags::KEYFRAME);

        if self.recovering && !keyframe {
            self.stats.frames_dropped += 1;
            return Ok(Forwarded::Dropped("waiting-keyframe"));
        }
        if self.recovering && keyframe {
            // 关键帧恢复：清空待处理，回到一致状态
            self.flush_queue()?;
            self.recovering = false;
            self.stats.recoveries += 1;
        }

        let size = frame.payload.len();
        match self.queue.push(frame) {
            Ok(()) => {
                self.peak_queued_bytes = self.peak_queued_bytes.max(self.queue.bytes());
                if self.drain {
                    self.flush_queue()?;
                }
                self.stats.frames_forwarded += 1;
                self.stats.bytes_forwarded += size as u64;
                Ok(Forwarded::Frame)
            }
            Err(_) => {
                // 超预算：不静默丢帧——进入"等关键帧"恢复态（丢弃本帧及后续非关键帧）
                self.flush_queue()?;
                self.recovering = true;
                self.stats.frames_dropped += 1;
                Ok(Forwarded::Dropped("budget"))
            }
        }
    }

    /// 把队列里的帧交给 sink。
    fn flush_queue(&mut self) -> Result<(), Error> {
        while let Some(frame) = self.queue.pop() {
            self.sink.on_frame(&frame)?;
        }
        Ok(())
    }

    /// 收尾：排空 + 返回 sink 统计。
    pub fn finish(&mut self) -> Result<SinkStats, Error> {
        self.flush_queue()?;
        self.sink.finish()
    }

    /// 队列当前字节数（诊断用）。
    pub fn queued_bytes(&self) -> usize {
        self.queue.bytes()
    }

    pub fn queue_limits(&self) -> (usize, usize) {
        self.queue_limits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use interop_media::sinks::{open_sink, SinkKind, SinkOptions};

    fn track(budget: usize) -> MirrorTrack {
        let sink = open_sink(
            SinkKind::Null,
            SinkOptions {
                dimensions: None,
                ..SinkOptions::default()
            },
        )
        .expect("sink");
        MirrorTrack::new(sink, budget)
    }

    fn au(seq: u64, keyframe: bool, len: usize) -> MediaFrame {
        MediaFrame {
            kind: FrameKind::EncodedAccessUnit,
            stream_id: 1,
            flags: if keyframe { flags::KEYFRAME } else { 0 },
            sequence: seq,
            pts: seq as i64,
            payload: vec![1u8; len],
        }
    }

    #[test]
    fn backpressure_enters_recovery_and_resumes_at_keyframe() {
        let mut t = track(4096);
        t.configure(MirrorCodec::H264Passthrough, 1, Dimensions::parse("640x480").expect("dims"), false)
            .expect("configure");
        t.set_drain(false); // 模拟下游跟不上

        // 队列容量 8 帧 / 4096 字节：塞满后必须进入恢复态
        let mut saw_budget_drop = false;
        for seq in 1..=12u64 {
            match t.on_frame(au(seq, seq == 1, 1024)) {
                Ok(Forwarded::Dropped("budget")) => saw_budget_drop = true,
                Ok(Forwarded::Dropped("waiting-keyframe")) => {}
                Ok(_) => {}
                Err(_) => {}
            }
        }
        assert!(saw_budget_drop, "超预算必须进入恢复态而不是静默丢帧");
        assert!(t.is_recovering());

        // 关键帧到达 → 恢复，且待处理被清空
        assert_eq!(t.on_frame(au(13, true, 1024)), Ok(Forwarded::Frame));
        assert!(!t.is_recovering(), "关键帧后应恢复");
        assert_eq!(t.stats().recoveries, 1);
        assert!(t.peak_queued_bytes() <= 4096, "高水位不得超过预算");

        // 排空后 sink 收到帧
        t.set_drain(true);
        assert_eq!(t.on_frame(au(14, false, 512)), Ok(Forwarded::Frame));
        let _ = t.finish().expect("finish");
    }

    #[test]
    fn frames_before_config_and_wrong_kind_are_rejected() {
        let mut t = track(1 << 16);
        assert_eq!(t.on_frame(au(1, true, 16)).expect_err("未配置").code, ErrorCode::InvalidFrame);
        t.configure(MirrorCodec::H264Passthrough, 1, Dimensions::parse("640x480").expect("dims"), false)
            .expect("configure");
        let pcm = MediaFrame {
            kind: FrameKind::PcmBlock,
            stream_id: 2,
            flags: 0,
            sequence: 1,
            pts: 0,
            payload: vec![0u8; 8],
        };
        assert_eq!(t.on_frame(pcm).expect_err("PCM 不走视频轨").code, ErrorCode::UnsupportedFeature);
    }

    #[test]
    fn hevc_is_rejected() {
        let mut t = track(1 << 16);
        assert_eq!(
            t.configure(MirrorCodec::Hevc, 1, Dimensions::parse("640x480").expect("dims"), false)
                .expect_err("HEVC")
                .code,
            ErrorCode::UnsupportedFeature
        );
    }
}
