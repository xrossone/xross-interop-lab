//! T28 验收（媒体形态/时钟/有界队列）：plans/03-casting.md §T28。
//!
//! - T28-02 timebase 分母 0 → `invalid-frame`；
//! - T28-03 format_id 变化必须伴随 decoder reset（无 reset 的流必须被拒——把这种流当成功的实现会红）；
//! - T28-04 native-only source 请求 export → `unsupported-feature`；
//! - 三套时钟域分离；无 PTS 不伪造时间；有界队列显式背压。

use interop_contract::error::ErrorCode;
use interop_contract::ids::PresentationId;
use interop_ipc::media_frame::{flags, FrameKind, MediaFrame};
use interop_media::clock::{ClockMap, Timebase};
use interop_media::descriptor::{EncodedSource, MediaSource, NativePresentationRef, PcmFormat, PcmSource};
use interop_media::stream::{FormatEvent, FormatTracker, FrameQueue};

/// T28-02：timebase 分母 0 → invalid-frame。
#[test]
fn t28_02_zero_denominator_timebase_is_rejected() {
    assert_eq!(
        Timebase::new(1, 0).expect_err("分母 0").code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        Timebase::parse("1/0").expect_err("分母 0").code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        Timebase::parse("90000").expect_err("缺分母").code,
        ErrorCode::InvalidFrame
    );
    let tb = Timebase::parse("1/90000").expect("合法 timebase");
    assert_eq!(tb.as_wire(), "1/90000");
    assert_eq!(tb.ticks_to_micros(90_000), 1_000_000, "90000 ticks = 1s");
}

/// T28-03：format_id 变化必须伴随 decoder reset；没有 reset 的流必须被拒绝。
#[test]
fn t28_03_format_change_requires_decoder_reset() {
    let mut tracker = FormatTracker::new();
    assert_eq!(
        tracker.observe_config(1, false).expect("首帧 config"),
        FormatEvent::Initialized
    );
    assert_eq!(
        tracker.observe_config(1, false).expect("重复 config"),
        FormatEvent::Unchanged,
        "同 format_id 重发 config 不是变化"
    );
    assert_eq!(
        tracker.observe_config(2, true).expect("新 format + discontinuity"),
        FormatEvent::Reset,
        "变化 + reset 信号 = 合法旋转/分辨率切换"
    );
    assert_eq!(tracker.current_format_id(), Some(2));

    // 反例：format 变了却没有 reset 信号 —— 必须拒绝（"测试必须失败"的那条）
    let mut naive = FormatTracker::new();
    naive.observe_config(1, false).expect("首个 config");
    let err = naive
        .observe_config(2, false)
        .expect_err("format 变化无 decoder reset 必须拒绝");
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    assert_eq!(
        naive.current_format_id(),
        Some(1),
        "拒绝后状态不得推进（否则 decoder 会用错配置）"
    );
}

/// T28-04：native-only source 请求 export → unsupported-feature。
#[test]
fn t28_04_native_only_source_cannot_export() {
    let native = MediaSource::NativeOnly(NativePresentationRef {
        presentation_id: PresentationId::try_from("pres_demo_1").expect("id"),
    });
    assert_eq!(
        native.export_encoded().expect_err("native 无 export").code,
        ErrorCode::UnsupportedFeature
    );

    let encoded = MediaSource::Encoded(EncodedSource {
        codec: "h264".into(),
        format_id: 1,
        timebase: Timebase::parse("1/90000").expect("tb"),
        codec_extradata_hash: Some("aa".repeat(32)),
    });
    assert_eq!(encoded.export_encoded().expect("encoded 可导出").codec, "h264");

    let pcm = MediaSource::Pcm(PcmSource {
        sample_rate: 44_100,
        channels: 2,
        layout: "stereo".into(),
        sample_format: PcmFormat::S16Le,
        timebase: Timebase::parse("1/44100").expect("tb"),
    });
    assert_eq!(
        pcm.export_encoded().expect_err("PCM 不是 encoded").code,
        ErrorCode::UnsupportedFeature
    );
}

/// 三套时钟域：远端 timebase ↔ 本地单调；无 PTS 不伪造；wall time 另行显式。
#[test]
fn clocks_keep_domains_separate_and_never_fabricate_pts() {
    let tb = Timebase::parse("1/90000").expect("tb");
    let map = ClockMap::new(tb, 1_000 /* 本地锚点 ms */, 900_000 /* 远端锚点 ticks */);
    assert_eq!(map.local_ms_for(Some(900_000)), Some(1_000), "锚点自身");
    assert_eq!(
        map.local_ms_for(Some(990_000)),
        Some(2_000),
        "90000 ticks = 1s → 本地 +1000ms"
    );
    assert_eq!(map.local_ms_for(None), None, "没有 PTS 就不给时间");
    assert!(!map.has_pts(None));
    assert!(map.has_pts(Some(0)));

    // frame 的 NO_PTS 位与 ClockMap 配合：不把 0 当合法时间
    let f = MediaFrame {
        kind: FrameKind::EncodedAccessUnit,
        stream_id: 1,
        flags: flags::NO_PTS,
        sequence: 1,
        pts: 0,
        payload: vec![],
    };
    assert_eq!(map.local_ms_for(f.pts_if_present()), None);
}

/// 有界队列：显式背压（超限 resource-limit），pop 释放预算。
#[test]
fn frame_queue_is_bounded_with_explicit_backpressure() {
    let mut q = FrameQueue::new(2, 16);
    let mk = |seq: u64, len: usize| MediaFrame {
        kind: FrameKind::EncodedAccessUnit,
        stream_id: 1,
        flags: 0,
        sequence: seq,
        pts: seq as i64,
        payload: vec![0u8; len],
    };
    q.push(mk(1, 8)).expect("first");
    q.push(mk(2, 8)).expect("second");
    assert_eq!(q.len(), 2);
    assert_eq!(q.bytes(), 16);
    assert_eq!(
        q.push(mk(3, 1)).expect_err("队列满必须显式拒绝").code,
        ErrorCode::ResourceLimit
    );
    let popped = q.pop().expect("pop");
    assert_eq!(popped.sequence, 1);
    assert_eq!(q.bytes(), 8, "pop 释放字节预算");
    q.push(mk(3, 8)).expect("有空间后可以继续");

    // 单帧超过总预算：直接拒绝，不进队列
    let mut small = FrameQueue::new(4, 8);
    assert_eq!(
        small.push(mk(9, 9)).expect_err("单帧超预算").code,
        ErrorCode::ResourceLimit
    );
}
