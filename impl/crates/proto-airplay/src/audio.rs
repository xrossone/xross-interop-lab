//! 音频轨道（plans/03 §T33）：PCM 块 + 显式重采样 + 音频时钟漂移。
//!
//! 立场：
//! - **只支持 PCM 块**（`FrameKind::PcmBlock`）。ALAC 等编码需要 decoder → `unsupported-feature`，
//!   能力广告也保持为空；
//! - 采样率不一致时用 `interop-media` 的 [`ResamplePlan`] 做**显式**重采样（时长不变），
//!   绝不静默按错误采样率播放（那会变速）；
//! - 音频时钟走 [`crate::timing::GenerationClock`]：每次连接一代，漂移只记录不掩盖。

use crate::timing::{DriftStats, GenerationClock};
use interop_contract::error::{Error, ErrorCode};
use interop_ipc::media_frame::{FrameKind, MediaFrame};
use interop_media::sinks::{Dimensions, MediaSink, ResamplePlan, SinkStats};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioStats {
    pub blocks_forwarded: u64,
    pub resampled_blocks: u64,
    pub samples_in: u64,
    pub samples_out: u64,
    pub bytes_out: u64,
}

/// 音频轨道（单流 PCM）。
pub struct AudioTrack {
    sink: Box<dyn MediaSink>,
    plan: ResamplePlan,
    channels: u8,
    device_rate: u32,
    source_rate: u32,
    clock: GenerationClock,
    stats: AudioStats,
    peak_resample_bytes: usize,
}

impl std::fmt::Debug for AudioTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AudioTrack({}Hz→{}Hz, ch={}, plan={:?}, blocks={})",
            self.source_rate, self.device_rate, self.channels, self.plan, self.stats.blocks_forwarded
        )
    }
}

impl AudioTrack {
    /// `device_rate` 是输出设备采样率，`source_rate` 是流声明采样率。
    pub fn new(
        sink: Box<dyn MediaSink>,
        device_rate: u32,
        source_rate: u32,
        channels: u8,
        clock: GenerationClock,
    ) -> Result<Self, Error> {
        if channels == 0 {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "声道数必须为正",
            )
            .with_phase("negotiating"));
        }
        let plan = ResamplePlan::plan(source_rate, device_rate)?;
        Ok(Self {
            sink,
            plan,
            channels,
            device_rate,
            source_rate,
            clock,
            stats: AudioStats::default(),
            peak_resample_bytes: 0,
        })
    }

    pub fn plan(&self) -> ResamplePlan {
        self.plan
    }

    pub fn stats(&self) -> AudioStats {
        self.stats
    }

    pub fn drift(&self) -> DriftStats {
        self.clock.stats()
    }

    pub fn clock_generation(&self) -> u64 {
        self.clock.generation()
    }

    /// 重采样缓冲的高水位（常数级：一个块的输出大小）。
    pub fn peak_resample_bytes(&self) -> usize {
        self.peak_resample_bytes
    }

    /// 处理一个 PCM 块：解码成 S16LE 样本 → 显式重采样 → 交给 sink。
    pub fn on_block(&mut self, frame: MediaFrame, local_ms: u64) -> Result<(), Error> {
        if frame.kind != FrameKind::PcmBlock {
            return Err(Error::new(
                ErrorCode::UnsupportedFeature,
                format!(
                    "音频只支持 PCM 块（收到的 kind={}）：编码音频需要 decoder，本仓不实现",
                    frame.kind.as_wire()
                ),
            )
            .with_phase("transferring"));
        }
        if !frame.payload.len().is_multiple_of(2) {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "PCM 块长度不是偶数（S16LE 样本必须成对）",
            )
            .with_phase("transferring"));
        }
        let (pairs, _) = frame.payload.as_chunks::<2>();
        let samples: Vec<i16> = pairs
            .iter()
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        let out = self.plan.apply_s16le(&samples, self.channels);
        self.peak_resample_bytes = self.peak_resample_bytes.max(out.len() * 2);

        let mut payload = Vec::with_capacity(out.len() * 2);
        for s in &out {
            payload.extend_from_slice(&s.to_le_bytes());
        }
        let forwarded = MediaFrame {
            kind: FrameKind::PcmBlock,
            stream_id: frame.stream_id,
            flags: frame.flags,
            sequence: frame.sequence,
            pts: frame.pts,
            payload,
        };
        self.sink.on_frame(&forwarded)?;

        self.stats.blocks_forwarded += 1;
        if !matches!(self.plan, ResamplePlan::Passthrough) {
            self.stats.resampled_blocks += 1;
        }
        self.stats.samples_in += samples.len() as u64;
        self.stats.samples_out += out.len() as u64;
        self.stats.bytes_out += (out.len() * 2) as u64;

        self.clock.observe(frame.pts_if_present(), local_ms);
        Ok(())
    }

    /// 收尾：返回 sink 统计。
    pub fn finish(&mut self) -> Result<SinkStats, Error> {
        self.sink.finish()
    }

    /// sink 侧的格式声明（音频无尺寸；某些后端需要先声明 codec）。
    pub fn declare_format(&mut self, codec: &str, format_id: u32) -> Result<(), Error> {
        self.sink.on_config(codec, format_id, None::<Dimensions>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::ClockGenerations;
    use interop_media::clock::Timebase;
    use interop_media::sinks::{open_sink, SinkKind, SinkOptions};

    fn clock() -> GenerationClock {
        GenerationClock::new(1, Timebase::new(1, 1_000_000).expect("tb"), 0, 0).expect("clock")
    }

    fn track(device: u32, source: u32) -> AudioTrack {
        let sink = open_sink(
            SinkKind::Null,
            SinkOptions {
                dimensions: None,
                ..SinkOptions::default()
            },
        )
        .expect("sink");
        AudioTrack::new(sink, device, source, 1, clock()).expect("track")
    }

    fn pcm(seq: u64, samples: usize) -> MediaFrame {
        MediaFrame {
            kind: FrameKind::PcmBlock,
            stream_id: 2,
            flags: 0,
            sequence: seq,
            pts: seq as i64 * 20_000,
            payload: vec![0x11u8; samples * 2],
        }
    }

    #[test]
    fn resample_is_explicit_and_preserves_duration() {
        let mut t = track(48_000, 16_000);
        assert!(matches!(t.plan(), ResamplePlan::Resample { .. }));
        t.on_block(pcm(1, 320), 0).expect("block"); // 20ms @16k
        let s = t.stats();
        assert_eq!(s.samples_in, 320);
        assert_eq!(s.samples_out, 960, "16k→48k 应输出 3 倍样本（时长不变）");
        assert_eq!(s.resampled_blocks, 1);
        assert!(t.peak_resample_bytes() <= 960 * 2 * 2, "重采样缓冲常数级");
    }

    #[test]
    fn non_pcm_is_rejected() {
        let mut t = track(48_000, 48_000);
        let encoded = MediaFrame {
            kind: FrameKind::EncodedAccessUnit,
            stream_id: 3,
            flags: 0,
            sequence: 1,
            pts: 0,
            payload: vec![0u8; 64],
        };
        assert_eq!(
            t.on_block(encoded, 0).expect_err("ALAC/AAC 无 decoder").code,
            ErrorCode::UnsupportedFeature
        );
        let odd = MediaFrame {
            kind: FrameKind::PcmBlock,
            stream_id: 2,
            flags: 0,
            sequence: 2,
            pts: 0,
            payload: vec![0u8; 3],
        };
        assert_eq!(t.on_block(odd, 0).expect_err("奇数字节").code, ErrorCode::InvalidFrame);
    }

    #[test]
    fn generations_are_passed_through_to_the_track() {
        let mut gens = ClockGenerations::new();
        let g = gens.next();
        let sink = open_sink(
            SinkKind::Null,
            SinkOptions {
                dimensions: None,
                ..SinkOptions::default()
            },
        )
        .expect("sink");
        let clock = GenerationClock::new(g, Timebase::new(1, 1_000_000).expect("tb"), 0, 0)
            .expect("clock");
        let t = AudioTrack::new(sink, 48_000, 48_000, 2, clock).expect("track");
        assert_eq!(t.clock_generation(), g);
    }
}
