//! AirPlay 镜像路径场景（T33）：把协商格式、画面帧、音频块与背压恢复跑成状态输出。
//!
//! 诚实前提：**没有 decoder**——encoded access unit 直通 sink（null 统计），能力广告为空。
//! 因此这里的数字证明的是"路由/背压/时钟/资源记账正确"，不是"能看到画面"。

use crate::report::MirrorReport;
use interop_contract::error::ErrorCode;
use interop_ipc::media_frame::{flags, FrameKind, MediaFrame};
use interop_media::clock::Timebase;
use interop_media::sinks::{open_sink, Dimensions, SinkKind, SinkOptions};
use proto_airplay::audio::AudioTrack;
use proto_airplay::capability::{advertised_features, ImplementationInventory};
use proto_airplay::mirror::{Forwarded, MirrorCodec, MirrorTrack};
use proto_airplay::timing::{ClockGenerations, GenerationClock};

const FPS: u64 = 30;

fn frame(seq: u64, keyframe: bool, payload: usize) -> MediaFrame {
    MediaFrame {
        kind: FrameKind::EncodedAccessUnit,
        stream_id: 1,
        flags: if keyframe { flags::KEYFRAME } else { 0 },
        sequence: seq,
        pts: (seq * 1_000_000 / FPS) as i64,
        payload: vec![(seq % 251) as u8; payload],
    }
}

pub fn mirror_scenario() -> (MirrorReport, bool) {
    let mut ok = true;

    let sink = match open_sink(
        SinkKind::Null,
        SinkOptions {
            dimensions: None,
            ..SinkOptions::default()
        },
    ) {
        Ok(s) => s,
        Err(_) => return (failed_report(), false),
    };
    let mut track = MirrorTrack::new(sink, 256 * 1024);

    // 横屏 3 秒 → 旋转 → 竖屏 3 秒 → 转回
    let mut steps: Vec<(String, String)> = Vec::new();
    for (label, format_id, dims, discontinuity) in [
        ("configure 1920x1080", 1u32, "1920x1080", false),
        ("rotate → 1080x1920", 2, "1080x1920", true),
        ("rotate → 1920x1080", 3, "1920x1080", true),
    ] {
        let d = match Dimensions::parse(dims) {
            Ok(d) => d,
            Err(_) => return (failed_report(), false),
        };
        let outcome = track.configure(MirrorCodec::H264Passthrough, format_id, d, discontinuity);
        if outcome.is_err() {
            ok = false;
        }
        steps.push((
            label.to_string(),
            if outcome.is_ok() { "ok".to_string() } else { "rejected".to_string() },
        ));
        for i in 0..FPS * 3 {
            if track.on_frame(frame(i + 1, i % (FPS * 2) == 0, 4096)) != Ok(Forwarded::Frame) {
                ok = false;
            }
        }
    }

    // 背压：下游停顿时队列会满 → 进入关键帧恢复
    track.set_drain(false);
    let mut budget_drops = 0u64;
    for seq in 100..=140u64 {
        match track.on_frame(frame(seq, false, 16 * 1024)) {
            Ok(Forwarded::Dropped("budget")) => budget_drops += 1,
            Ok(_) => {}
            Err(_) => ok = false,
        }
    }
    let recovering_before_keyframe = track.is_recovering();
    let recovered = track.on_frame(frame(200, true, 8 * 1024)) == Ok(Forwarded::Frame);
    track.set_drain(true);
    if !recovering_before_keyframe || !recovered {
        ok = false;
    }

    let video_stats = track.stats();
    let peak_queued = track.peak_queued_bytes();

    // 音频：44.1k 源 → 48k 设备（显式重采样），并记录漂移
    let mut generations = ClockGenerations::new();
    let generation = generations.next();
    let clock = match GenerationClock::new(
        generation,
        Timebase::new(1, 1_000_000).expect("timebase"),
        0,
        0,
    ) {
        Ok(c) => c,
        Err(_) => return (failed_report(), false),
    };
    let audio_sink = match open_sink(
        SinkKind::Null,
        SinkOptions {
            dimensions: None,
            ..SinkOptions::default()
        },
    ) {
        Ok(s) => s,
        Err(_) => return (failed_report(), false),
    };
    let mut audio = match AudioTrack::new(audio_sink, 48_000, 44_100, 2, clock) {
        Ok(a) => a,
        Err(_) => return (failed_report(), false),
    };
    const BLOCK_MS: u64 = 20;
    let samples_per_block = (44_100u64 * BLOCK_MS / 1000) as usize; // 882
    for i in 0..200u64 {
        let pts_us = (i * BLOCK_MS * 1000) as i64;
        let block = MediaFrame {
            kind: FrameKind::PcmBlock,
            stream_id: 2,
            flags: 0,
            sequence: i + 1,
            pts: pts_us,
            payload: vec![(i % 251) as u8; samples_per_block * 2 * 2], // 2ch S16LE
        };
        let local_ms = pts_us as u64 / 1000 + (i % 3); // 0..2ms 抖动
        if audio.on_block(block, local_ms).is_err() {
            ok = false;
        }
    }
    let audio_stats = audio.stats();
    let drift = audio.drift();
    if audio_stats.resampled_blocks != 200 || drift.max_abs_drift_ms > 3 {
        ok = false;
    }

    // 能力广告：本 task 之后仍为空（能路由 ≠ 能显示）
    let advertised: Vec<String> = advertised_features(&ImplementationInventory::current())
        .iter()
        .map(|f| f.as_wire().to_string())
        .collect();
    if !advertised.is_empty() {
        ok = false;
    }
    let hevc_rejected = track
        .configure(
            MirrorCodec::Hevc,
            9,
            Dimensions::parse("1920x1080").expect("dims"),
            true,
        )
        .map_err(|e| e.code)
        == Err(ErrorCode::UnsupportedFeature);

    let report = MirrorReport {
        evidence_level: "simulated",
        wire: "self-authored-fixture",
        steps,
        video: serde_json::json!({
            "frames_forwarded": video_stats.frames_forwarded,
            "frames_dropped": video_stats.frames_dropped,
            "format_changes": video_stats.format_changes,
            "recoveries": video_stats.recoveries,
            "bytes_forwarded": video_stats.bytes_forwarded,
            "peak_queued_bytes": peak_queued,
            "queue_budget_bytes": track.queue_limits().1,
            "budget_drops_during_backpressure": budget_drops,
            "recovering_then_keyframe_recovered": recovering_before_keyframe && recovered,
        }),
        audio: serde_json::json!({
            "plan": format!("{:?}", audio.plan()),
            "blocks_forwarded": audio_stats.blocks_forwarded,
            "resampled_blocks": audio_stats.resampled_blocks,
            "samples_in": audio_stats.samples_in,
            "samples_out": audio_stats.samples_out,
            "peak_resample_bytes": audio.peak_resample_bytes(),
        }),
        drift: serde_json::json!({
            "generation": drift.generation,
            "samples": drift.samples,
            "max_abs_drift_ms": drift.max_abs_drift_ms,
            "mean_drift_ms": drift.mean_drift_ms,
        }),
        capability: serde_json::json!({
            "advertised_features": advertised,
            "hevc_rejected": hevc_rejected,
            "note": "本路径不含 decoder：能力广告保持为空（广告未实现能力会让能力一致性测试失败）",
        }),
        blocked: vec![
            "真机镜像（iPhone/iPad → 本机）：需 S1（外部引擎/解码材料）与 S2（真机矩阵）".to_string(),
            "屏幕呈现：native window sink 未实现（无 GUI 自动化，不以屏幕录制假装 raw output）".to_string(),
            "30 分钟墙钟运行与 RSS 趋势：本 demo 用合成时间轴 + 缓冲高水位代理；真机长跑在待用户清单".to_string(),
            "AirPlay features TXT 位串与 PIN 一致性：待真机抓包/实测裁决".to_string(),
        ],
    };
    (report, ok)
}

fn failed_report() -> MirrorReport {
    MirrorReport {
        evidence_level: "simulated",
        wire: "self-authored-fixture",
        steps: vec![],
        video: serde_json::Value::Null,
        audio: serde_json::Value::Null,
        drift: serde_json::Value::Null,
        capability: serde_json::Value::Null,
        blocked: vec!["镜像场景提前失败（sink 或时钟构造异常）".to_string()],
    }
}
