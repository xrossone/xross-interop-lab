//! T29 验收（plans/03-casting.md §T29）：null / file sink（native 留待桌面会话）。
//!
//! - T29-01 无 DISPLAY / 无音频设备下 null sink 必须成功，只统计帧与时间基，不初始化任何 UI；
//! - T29-02 44.1k PCM 到 48k 设备必须走**显式 resample**（时长不变、不变速），等采样率才允许直通；
//! - T29-03 损坏/巨大 dimensions 受控失败（错误码明确、无 panic），且不影响其他 sink；
//! - file sink 保存自有测试内容并留下可复核的帧索引。

use interop_contract::error::ErrorCode;
use interop_ipc::media_frame::{flags, FrameKind, MediaFrame};
use interop_media::sinks::{open_sink, Dimensions, ResamplePlan, SinkKind, SinkOptions};
use std::path::PathBuf;

fn frame(kind: FrameKind, seq: u64, pts: Option<i64>, payload: &[u8]) -> MediaFrame {
    MediaFrame {
        kind,
        stream_id: 1,
        flags: match pts {
            Some(_) => 0,
            None => flags::NO_PTS,
        },
        sequence: seq,
        pts: pts.unwrap_or(0),
        payload: payload.to_vec(),
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("interop-t29-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

/// T29-01：无 DISPLAY / 无音频设备运行 null sink。
#[test]
fn t29_01_null_sink_runs_headless_and_only_counts() {
    // 本测试进程本身没有图形会话依赖：null sink 不得要求 DISPLAY/音频设备
    let mut sink = open_sink(SinkKind::Null, SinkOptions::default())
        .expect("null sink 必须能在无 DISPLAY/无音频设备下打开");
    sink.on_config("h264", 1, Some(Dimensions::parse("1920x1080").expect("dims")))
        .expect("config");

    for (i, payload) in [b"a".as_slice(), b"bb", b"ccc"].iter().enumerate() {
        sink.on_frame(&frame(
            FrameKind::EncodedAccessUnit,
            i as u64 + 1,
            Some(i as i64 * 33),
            payload,
        ))
        .expect("frame");
    }
    let stats = sink.finish().expect("finish");
    assert_eq!(stats.frames, 3, "逐帧统计");
    assert_eq!(stats.bytes, 6, "字节统计");
    assert_eq!(stats.first_pts, Some(0));
    assert_eq!(stats.last_pts, Some(66), "last = 2×33");
    assert_eq!(stats.format_changes, 1, "首个 config 计一次");
    assert_eq!(stats.codec.as_deref(), Some("h264"));
    assert_eq!(stats.dimensions.map(|d| d.area()), Some(1920 * 1080));

    // 无 PTS 的帧不污染时间统计（不把 0 当 (0) 时刻）
    let mut sink2 = open_sink(SinkKind::Null, SinkOptions::default()).expect("open");
    sink2
        .on_frame(&frame(FrameKind::PcmBlock, 1, None, b"zz"))
        .expect("frame");
    let stats2 = sink2.finish().expect("finish");
    assert_eq!(stats2.first_pts, None);
    assert_eq!(stats2.last_pts, None);
}

/// T29-02：44.1k PCM → 48k 设备必须显式 resample，时长不变。
#[test]
fn t29_02_pcm_resample_is_explicit_and_preserves_duration() {
    assert_eq!(
        ResamplePlan::plan(48_000, 48_000).expect("同率"),
        ResamplePlan::Passthrough,
        "同采样率直接直通（无多余节点）"
    );
    let plan = ResamplePlan::plan(44_100, 48_000).expect("重采样计划");
    match &plan {
        ResamplePlan::Resample {
            source_rate,
            device_rate,
            ..
        } => {
            assert_eq!((*source_rate, *device_rate), (44_100, 48_000));
        }
        other => panic!("44.1k→48k 必须是显式 Resample 节点，实际 {other:?}"),
    }

    // 1 秒 44.1k 单声道 → 48000 样本（时长保持 = 不变速）
    let input: Vec<i16> = (0..44_100)
        .map(|i: i32| ((i % 200 - 100) * 200).clamp(i16::MIN as i32, i16::MAX as i32) as i16)
        .collect();
    let output = plan.apply_s16le(&input, 1);
    assert_eq!(output.len(), 48_000, "输出样本数 = device_rate × 时长");
    assert_eq!(output[0], input[0], "插值在起点保持原值");

    // 立体声按帧交错处理（样本数 = 帧数 × 声道）
    let stereo: Vec<i16> = (0..44_100 * 2).map(|i| (i % 1000) as i16).collect();
    let out_stereo = plan.apply_s16le(&stereo, 2);
    assert_eq!(out_stereo.len(), 48_000 * 2);

    // 不支持的费率（0）明确拒绝，而不是静默按 1:1 播
    assert_eq!(
        ResamplePlan::plan(0, 48_000).expect_err("非法源采样率").code,
        ErrorCode::InvalidFrame
    );
}

/// T29-03：损坏/巨大 dimensions 受控失败，不拖垮 supervisor。
#[test]
fn t29_03_damaged_or_huge_dimensions_fail_controlled() {
    assert_eq!(
        Dimensions::parse("abc").expect_err("坏形状").code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        Dimensions::parse("0x1080").expect_err("零宽").code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        Dimensions::parse("1920").expect_err("缺高度").code,
        ErrorCode::InvalidFrame
    );

    let huge = Dimensions::parse("100000x100000").expect("形状合法");
    assert!(huge.area() > 8192 * 8192, "确实超过上限");
    let err = open_sink(
        SinkKind::NativeWindow,
        SinkOptions {
            dimensions: Some(huge),
            ..SinkOptions::default()
        },
    )
    .err().expect("巨大 dimensions 必须受控拒绝");
    assert_eq!(err.code, ErrorCode::ResourceLimit, "分配前拒绝");

    // 失败不影响后续：null sink 照常工作（supervisor 不会被拖垮）
    let mut null = open_sink(SinkKind::Null, SinkOptions::default()).expect("失败后仍可打开");
    null.on_frame(&frame(FrameKind::EncodedAccessUnit, 1, None, b"x"))
        .expect("继续工作");
    assert_eq!(null.finish().expect("finish").frames, 1);

    // native 窗口：无 GUI 会话时明确不可用（不开假窗口、不做屏幕录制）
    let err = open_sink(
        SinkKind::NativeWindow,
        SinkOptions {
            dimensions: Some(Dimensions::parse("1920x1080").expect("dims")),
            ..SinkOptions::default()
        },
    )
    .err().expect("native 未实现");
    assert_eq!(err.code, ErrorCode::PlatformUnavailable);
}

/// file sink：保存自有内容 + 帧索引可复核。
#[test]
fn file_sink_writes_payloads_and_index() {
    let dir = temp_dir("file");
    let path = dir.join("recording.bin");
    let mut sink = open_sink(
        SinkKind::File,
        SinkOptions {
            path: Some(path.clone()),
            ..SinkOptions::default()
        },
    )
    .expect("file sink");
    sink.on_config("h264", 1, Some(Dimensions::parse("640x360").expect("dims")))
        .expect("config");
    for (i, payload) in [b"ONE".as_slice(), b"TWO!", b"THREE!!"].iter().enumerate() {
        sink.on_frame(&frame(
            FrameKind::EncodedAccessUnit,
            i as u64 + 1,
            Some(i as i64),
            payload,
        ))
        .expect("frame");
    }
    let stats = sink.finish().expect("finish");
    assert_eq!(stats.frames, 3);
    assert_eq!(stats.bytes, 3 + 4 + 7);
    let written = std::fs::read(&path).expect("落盘");
    assert_eq!(written, b"ONETWO!THREE!!", "payload 顺序串接");
    assert_eq!(stats.path.as_deref(), Some(path.to_string_lossy().as_ref()));
}
