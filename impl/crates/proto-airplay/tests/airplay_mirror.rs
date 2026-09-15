//! T33 验收（plans/03-casting.md §T33）：AirPlay legacy 镜像接收路径（媒体面，headless 部分）。
//!
//! - T33-01 横竖屏往返 → format 更新、画面恢复、配置前/无 reset 的帧被拒（无越界写入）；
//! - T33-02 30 分钟合成音画 → 记录 drift / 丢帧 / 内存趋势（缓冲高水位有界）；
//! - T33-03 20 次断开重连 → 每次独立 key 与 clock generation，资源无持续增长；
//! - T33-04 HEVC 不支持 → 不广告该能力（能力广告仍为空）。
//!
//! **本 task 不含 decoder**：encoded access unit 直接进 sink（null 统计 / file 落盘）；
//! 因此能力广告保持为空 —— 这是 T33-04 的核心断言，也避免"能路由"被误读成"能显示"。
//! 真机（iPhone → 本机音视频）仍需 S1/S2，见 `manual_tests`。

use interop_contract::error::ErrorCode;
use interop_contract::ids::SessionId;
use interop_ipc::media_frame::{flags, FrameKind, MediaFrame};
use interop_media::sinks::{open_sink, Dimensions, SinkKind, SinkOptions};
use interop_media::stream::FormatTracker;
use proto_airplay::audio::AudioTrack;
use proto_airplay::capability::{
    advertised_features, assert_advertised_matches_implementation, AdvertisedFeature,
    ImplementationInventory,
};
use proto_airplay::keying::{KeyingProvider, StreamKeys};
use proto_airplay::mirror::{Forwarded, MirrorCodec, MirrorTrack};
use proto_airplay::rtsp::RtspRequest;
use proto_airplay::session::AirPlayReceiver;
use proto_airplay::timing::{ClockGenerations, GenerationClock};
use std::collections::BTreeSet;

fn dims(s: &str) -> Dimensions {
    Dimensions::parse(s).expect("dims")
}

fn video_frame(seq: u64, pts: i64, keyframe: bool, payload: usize) -> MediaFrame {
    MediaFrame {
        kind: FrameKind::EncodedAccessUnit,
        stream_id: 1,
        flags: if keyframe { flags::KEYFRAME } else { 0 },
        sequence: seq,
        pts,
        payload: vec![(seq % 251) as u8; payload],
    }
}

fn config_frame(seq: u64, payload: usize) -> MediaFrame {
    MediaFrame {
        kind: FrameKind::CodecConfig,
        stream_id: 1,
        flags: 0,
        sequence: seq,
        pts: 0,
        payload: vec![0xC0; payload],
    }
}

fn null_track(budget: usize) -> MirrorTrack {
    let sink = open_sink(
        SinkKind::Null,
        SinkOptions {
            dimensions: None,
            ..SinkOptions::default()
        },
    )
    .expect("null sink");
    MirrorTrack::new(sink, budget)
}

/// T33-01：横竖屏往返——format 更新、画面恢复、非法配置被拒。
#[test]
fn t33_01_orientation_round_trip_updates_format_and_resumes() {
    let mut track = null_track(1 << 20);

    // 首次配置（Initialized）→ 3 帧
    track
        .configure(MirrorCodec::H264Passthrough, 1, dims("1920x1080"), false)
        .expect("首次配置");
    for seq in 1..=3 {
        assert_eq!(
            track.on_frame(video_frame(seq, seq as i64 * 33_333, seq == 1, 4096)),
            Ok(Forwarded::Frame),
            "配置后帧必须放行"
        );
    }

    // 转竖屏：必须带 discontinuity（Reset），画面立即恢复
    track
        .configure(MirrorCodec::H264Passthrough, 2, dims("1080x1920"), true)
        .expect("旋转配置");
    for seq in 4..=6 {
        assert_eq!(
            track.on_frame(video_frame(seq, seq as i64 * 33_333, seq == 4, 4096)),
            Ok(Forwarded::Frame)
        );
    }

    // 转回横屏
    track
        .configure(MirrorCodec::H264Passthrough, 3, dims("1920x1080"), true)
        .expect("转回配置");
    for seq in 7..=9 {
        assert_eq!(
            track.on_frame(video_frame(seq, seq as i64 * 33_333, seq == 7, 4096)),
            Ok(Forwarded::Frame)
        );
    }

    let stats = track.stats();
    assert_eq!(stats.frames_forwarded, 9, "每次旋转后画面必须恢复");
    assert_eq!(stats.format_changes, 3);
    assert_eq!(stats.frames_dropped, 0);
    assert_eq!(track.dimensions(), Some(dims("1920x1080")), "最终尺寸应为横屏");

    // 负向 1：format_id 变化但没有 discontinuity → 拒绝（T28-03 的规则）
    assert_eq!(
        track
            .configure(MirrorCodec::H264Passthrough, 4, dims("1280x720"), false)
            .expect_err("缺 reset 的 format 变化必须拒绝")
            .code,
        ErrorCode::InvalidFrame
    );

    // 负向 2：配置之前送帧 → 拒绝（不得有任何越界/无配置写入）
    let mut fresh = null_track(1 << 20);
    assert_eq!(
        fresh
            .on_frame(video_frame(1, 0, true, 1024))
            .expect_err("未配置就送帧必须拒绝")
            .code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(fresh.stats().frames_forwarded, 0);

    // 负向 3：尺寸超过预算（分配前拒绝）
    let mut big = null_track(1 << 20);
    assert_eq!(
        big.configure(MirrorCodec::H264Passthrough, 1, dims("20000x12000"), false)
            .expect_err("巨大 dimensions 必须拒绝")
            .code,
        ErrorCode::ResourceLimit
    );

    // 负向 4：config 帧缺 payload 之外——payload 超过 XMD1 上限时不得进入 sink
    let mut bounded = null_track(1 << 20);
    bounded
        .configure(MirrorCodec::H264Passthrough, 1, dims("1920x1080"), false)
        .expect("配置");
    let mut huge = video_frame(1, 0, true, 1024);
    huge.payload = vec![0u8; interop_ipc::media_frame::MAX_PAYLOAD as usize + 1];
    assert_eq!(
        bounded.on_frame(huge).expect_err("超过 payload 上限").code,
        ErrorCode::ResourceLimit
    );

    // config 帧本身走后端 on_config（不占画面帧计数）
    assert_eq!(track.on_frame(config_frame(10, 32)), Ok(Forwarded::Config));
}

/// T33-02：30 分钟合成音画 → drift / 丢帧 / 内存趋势（缓冲高水位有界且不增长）。
#[test]
fn t33_02_thirty_minute_synthetic_av_records_drift_and_bounded_memory() {
    const MINUTES: u64 = 30;
    const FPS: u64 = 30;
    const AUDIO_BLOCK_MS: u64 = 20;
    const SOURCE_RATE: u32 = 16_000; // 16 kHz mono PCM（自制）
    const DEVICE_RATE: u32 = 48_000; // 设备 48 kHz → 必须显式重采样

    let video_frames = MINUTES * 60 * FPS; // 54,000
    let audio_blocks = MINUTES * 60 * 1000 / AUDIO_BLOCK_MS; // 90,000
    let samples_per_block = (SOURCE_RATE as u64 * AUDIO_BLOCK_MS / 1000) as usize; // 320

    let video_sink = open_sink(
        SinkKind::Null,
        SinkOptions {
            dimensions: None,
            ..SinkOptions::default()
        },
    )
    .expect("video sink");
    let mut video = MirrorTrack::new(video_sink, 256 * 1024);
    video
        .configure(MirrorCodec::H264Passthrough, 1, dims("1920x1080"), false)
        .expect("配置");

    let audio_clock = GenerationClock::new(
        1,
        interop_media::clock::Timebase::new(1, 1_000_000).expect("timebase"),
        0,
        0,
    )
    .expect("clock");
    let audio_sink = open_sink(
        SinkKind::Null,
        SinkOptions {
            dimensions: None,
            ..SinkOptions::default()
        },
    )
    .expect("audio sink");
    let mut audio = AudioTrack::new(audio_sink, DEVICE_RATE, SOURCE_RATE, 1, audio_clock).expect("audio track");

    // 合成时间轴：本地单调时间 = 远端 pts（无抖动）→ drift 应为 0；再注入 1ms 抖动检查统计口径
    let mut peak_half = 0usize;
    for i in 0..video_frames {
        let pts_us = (i * 1_000_000 / FPS) as i64;
        let frame = video_frame(i + 1, pts_us, i % (FPS * 2) == 0, 200);
        assert_eq!(video.on_frame(frame), Ok(Forwarded::Frame), "视频帧 {i}");
        if i == video_frames / 2 {
            peak_half = video.peak_queued_bytes();
        }
    }
    for i in 0..audio_blocks {
        let pts_us = (i * AUDIO_BLOCK_MS * 1000) as i64;
        let local_ms = (pts_us / 1000) as u64 + (i % 2); // 0/1 ms 抖动
        let block = MediaFrame {
            kind: FrameKind::PcmBlock,
            stream_id: 2,
            flags: 0,
            sequence: i + 1,
            pts: pts_us,
            payload: vec![(i % 251) as u8; samples_per_block * 2],
        };
        audio.on_block(block, local_ms).expect("音频块");
    }

    let video_stats = video.stats();
    assert_eq!(video_stats.frames_forwarded, video_frames);
    assert_eq!(video_stats.frames_dropped, 0, "合成流不应丢帧");
    assert_eq!(video_stats.format_changes, 1);

    let audio_stats = audio.stats();
    assert_eq!(audio_stats.blocks_forwarded, audio_blocks);
    assert_eq!(audio_stats.resampled_blocks, audio_blocks, "16k→48k 每块都必须显式重采样");
    assert_eq!(audio_stats.samples_out, audio_blocks * samples_per_block as u64 * 3);

    let drift = audio.drift();
    assert_eq!(drift.samples, audio_blocks, "每块都要有 drift 采样");
    assert!(drift.max_abs_drift_ms <= 2, "抖动注入 ≤2ms，实测 {}", drift.max_abs_drift_ms);
    assert!(drift.mean_drift_ms.abs() <= 2);

    // 内存趋势：缓冲高水位有界，且后半程不增长
    assert!(video.peak_queued_bytes() <= 256 * 1024, "高水位不得超过预算");
    assert_eq!(video.peak_queued_bytes(), peak_half, "后半程不得继续增长");
    assert!(audio.peak_resample_bytes() <= samples_per_block * 2 * 4, "重采样缓冲为常数级");
}

/// 计数 keying provider：每次派生都不同（用于 T33-03 的"独立 key"断言）。
#[derive(Default)]
struct CountingKeying {
    calls: std::sync::atomic::AtomicU64,
}

impl KeyingProvider for CountingKeying {
    fn name(&self) -> &'static str {
        "counting-test-only"
    }

    fn available(&self) -> bool {
        true
    }

    fn derive_stream_keys(&self) -> Result<StreamKeys, interop_contract::error::Error> {
        let n = self
            .calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        // 自造字节（不是任何真实材料）：只用于证明每次会话拿到的是**新的** key 材料
        let bytes: Vec<u8> = (0..32u8).map(|i| i.wrapping_add(n as u8)).collect();
        Ok(StreamKeys::from_bytes(bytes))
    }
}

fn request(method: &str, uri: &str, cseq: u64, body: &[u8]) -> Vec<u8> {
    let mut out = format!("{method} {uri} RTSP/1.0\r\nCSeq: {cseq}\r\n").into_bytes();
    if !body.is_empty() {
        out.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    out
}

/// T33-03：20 次断开重连 → 每次独立 key/clock generation，资源无持续增长。
#[test]
fn t33_03_twenty_reconnects_use_fresh_keys_and_clocks() {
    // 跨会话共享同一个计数器（`KeyingProvider` 是 `&self`；Arc 保证计数持续）
    struct Shared(std::sync::Arc<CountingKeying>);
    impl KeyingProvider for Shared {
        fn name(&self) -> &'static str {
            self.0.name()
        }
        fn available(&self) -> bool {
            self.0.available()
        }
        fn derive_stream_keys(&self) -> Result<StreamKeys, interop_contract::error::Error> {
            self.0.derive_stream_keys()
        }
    }

    let keying = std::sync::Arc::new(CountingKeying::default());
    let mut rx = AirPlayReceiver::new(4, Box::new(Shared(keying.clone())));
    let mut generations = ClockGenerations::new();
    let mut generation_ids = BTreeSet::new();
    let mut frames_total = 0u64;

    for round in 0..20u64 {
        let now = 1_000_000 + round * 10_000;
        let sid: SessionId = rx.accept_connection(now).expect("连接");

        for (method, uri, cseq, body) in [
            ("POST", "/pair-setup-pin", 1u64, vec![0u8; 4]),
            ("SETUP", "rtsp://198.51.100.9/stream", 2, vec![]),
        ] {
            let bytes = request(method, uri, cseq, &body);
            let (req, _) = RtspRequest::parse(&bytes).expect("fixture 合法");
            let resp = rx.handle(&sid, &req, now + cseq).expect("handle");
            assert_eq!(resp.status.code(), 200, "round {round} 的 {method} 必须成功");
        }
        assert!(rx.has_stream(&sid), "round {round} 必须建流");

        // 每次会话一个**新的** clock generation
        let generation = generations.next();
        assert!(generation_ids.insert(generation), "generation 必须唯一");
        let clock = GenerationClock::new(
            generation,
            interop_media::clock::Timebase::new(1, 1_000_000).expect("timebase"),
            now,
            0,
        )
        .expect("clock");

        let mut track = null_track(1 << 18);
        track
            .configure(MirrorCodec::H264Passthrough, generation as u32, dims("1920x1080"), false)
            .expect("配置");
        for seq in 1..=5u64 {
            assert_eq!(
                track.on_frame(video_frame(seq, seq as i64 * 33_333, seq == 1, 2048)),
                Ok(Forwarded::Frame)
            );
            frames_total += 1;
        }
        let _ = clock.generation();

        // 断开：资源立即回收
        let bytes = request("TEARDOWN", "/stream", 9, b"");
        let (req, _) = RtspRequest::parse(&bytes).expect("fixture 合法");
        rx.handle(&sid, &req, now + 90).expect("teardown");
        rx.disconnect(&sid, now + 100);
        assert_eq!(rx.active_sessions(), 0, "round {round}: 会话必须回收");
        assert_eq!(rx.reserved_ports(), 0, "round {round}: 端口必须回收");
        assert_eq!(rx.tracked_sessions(), 0, "round {round}: 记录必须回收");
    }

    assert_eq!(generation_ids.len(), 20, "20 次会话 20 个 generation");
    assert_eq!(frames_total, 100);
    assert_eq!(
        keying.calls.load(std::sync::atomic::Ordering::SeqCst),
        20,
        "每次会话都必须向 keying 要一次新的流密钥（不跨会话复用）"
    );
}

/// T33-04：HEVC 不支持 → 不广告；能力广告在本 task 之后仍为空。
#[test]
fn t33_04_hevc_is_never_advertised() {
    let inv = ImplementationInventory::current();
    assert!(
        advertised_features(&inv).is_empty(),
        "没有 decoder 就没有可广告的能力（T33 只做路由，不做解码）"
    );
    let mut claimed: BTreeSet<AdvertisedFeature> = BTreeSet::new();
    claimed.insert(AdvertisedFeature::VideoReceiverHevc);
    assert_eq!(
        assert_advertised_matches_implementation(&claimed, &inv)
            .expect_err("广告 HEVC 必须失败")
            .code,
        ErrorCode::UnsupportedFeature
    );

    // 媒体路径同样拒绝 HEVC / ALAC（没有对应 decoder）
    let mut track = null_track(1 << 18);
    assert_eq!(
        track
            .configure(MirrorCodec::Hevc, 1, dims("1920x1080"), false)
            .expect_err("HEVC 必须拒绝")
            .code,
        ErrorCode::UnsupportedFeature
    );
    let mut tracker = FormatTracker::new();
    assert_eq!(tracker.observe_config(1, false).expect("first"), interop_media::stream::FormatEvent::Initialized);

    let sink = open_sink(
        SinkKind::Null,
        SinkOptions {
            dimensions: None,
            ..SinkOptions::default()
        },
    )
    .expect("sink");
    let clock = GenerationClock::new(
        1,
        interop_media::clock::Timebase::new(1, 1_000_000).expect("timebase"),
        0,
        0,
    )
    .expect("clock");
    let mut audio =
        AudioTrack::new(sink, 48_000, 44_100, 2, clock).expect("audio track");
    let alac = MediaFrame {
        kind: FrameKind::EncodedAccessUnit,
        stream_id: 3,
        flags: 0,
        sequence: 1,
        pts: 0,
        payload: vec![0u8; 128],
    };
    assert_eq!(
        audio
            .on_block(alac, 0)
            .expect_err("ALAC 需要 decoder：必须拒绝")
            .code,
        ErrorCode::UnsupportedFeature
    );
}

/// 断链之后再次连接：generation 单调递增（不给新会话发旧 generation）。
#[test]
fn clock_generations_are_monotonic_across_reconnects() {
    let mut gens = ClockGenerations::new();
    let first = gens.next();
    let second = gens.next();
    let third = gens.next();
    assert!(first < second && second < third);
    assert_eq!(gens.count(), 3);
}
