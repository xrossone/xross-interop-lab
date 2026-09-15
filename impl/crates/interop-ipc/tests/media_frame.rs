//! T28 数据面验收（docs/05 §7）：XMD1 36 字节帧头。
//!
//! - 超 16MiB 的声明 payload **在分配前**拒绝（T28-01）；
//! - 未知 critical flag 位 fail-closed；
//! - 编码/解码严格 36 字节头，粘包逐帧消费。

use interop_contract::error::ErrorCode;
use interop_ipc::media_frame::{check_payload_len, flags, FrameKind, MediaFrame, HEADER_LEN, MAX_PAYLOAD};

fn header(kind: u16, stream_id: u32, fl: u32, seq: u64, pts: i64, payload_len: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(HEADER_LEN);
    v.extend_from_slice(b"XMD1");
    v.extend_from_slice(&1u16.to_be_bytes());
    v.extend_from_slice(&kind.to_be_bytes());
    v.extend_from_slice(&stream_id.to_be_bytes());
    v.extend_from_slice(&fl.to_be_bytes());
    v.extend_from_slice(&seq.to_be_bytes());
    v.extend_from_slice(&pts.to_be_bytes());
    v.extend_from_slice(&payload_len.to_be_bytes());
    v
}

fn frame(kind: FrameKind, flags: u32, seq: u64, pts: i64, payload: &[u8]) -> MediaFrame {
    MediaFrame {
        kind,
        stream_id: 7,
        flags,
        sequence: seq,
        pts,
        payload: payload.to_vec(),
    }
}

/// T28-01：声明 payload 大于 16MiB → 分配前 resource-limit。
#[test]
fn t28_01_payload_over_limit_rejected_before_allocation() {
    // 只有 36 字节 header 到达（payload 一个字节都没有）——仍必须先按声明长度拒绝
    let buf = header(1, 7, flags::KEYFRAME, 1, 0, MAX_PAYLOAD + 1);
    assert_eq!(buf.len(), HEADER_LEN, "fixture 必须只含 header");
    let err = MediaFrame::decode(&buf).expect_err("超限必须拒绝");
    assert_eq!(
        err.code,
        ErrorCode::ResourceLimit,
        "必须是 resource-limit（长度检查先于任何分配/等待）"
    );

    // 编码侧同样有闸门（不靠调用方自觉）
    assert_eq!(
        check_payload_len(MAX_PAYLOAD as usize + 1).expect_err("超限").code,
        ErrorCode::ResourceLimit
    );
    assert!(check_payload_len(MAX_PAYLOAD as usize).is_ok(), "恰好 16MiB 合法");
}

/// 36 字节头 + 精确往返。
#[test]
fn frame_round_trips_with_36_byte_header() {
    let f = frame(FrameKind::PcmBlock, flags::DISCONTINUITY, 42, -1234, b"pcm-bytes");
    let bytes = f.encode();
    assert_eq!(bytes.len(), HEADER_LEN + 9, "头 36 字节 + payload");
    assert_eq!(&bytes[..4], b"XMD1");
    assert_eq!(u16::from_be_bytes([bytes[4], bytes[5]]), 1, "header_version=1");
    let (decoded, used) = MediaFrame::decode(&bytes).expect("round trip");
    assert_eq!(used, bytes.len());
    assert_eq!(decoded, f);
}

/// 未知 critical flag 位 → fail-closed（不是忽略）。
#[test]
fn unknown_critical_flags_are_rejected() {
    let buf = header(1, 1, 0x8000_0000, 1, 0, 0);
    assert_eq!(
        MediaFrame::decode(&buf).expect_err("未知 critical 位").code,
        ErrorCode::InvalidFrame
    );
}

/// 已知 flag 位被接受且如实报告。
#[test]
fn known_flags_are_accepted_and_reported() {
    let buf = header(1, 1, flags::KEYFRAME | flags::NO_PTS, 1, 999, 0);
    let (f, _) = MediaFrame::decode(&buf).expect("已知 flag");
    assert!(f.has_flag(flags::KEYFRAME));
    assert!(f.no_pts(), "NO_PTS 位必须如实暴露");
    assert!(!f.has_flag(flags::DISCONTINUITY));
}

/// 坏 magic / 未知版本 / 未知 kind / 截断 payload 全部明确拒绝。
#[test]
fn malformed_frames_are_rejected() {
    let mut bad_magic = header(1, 1, 0, 1, 0, 0);
    bad_magic[..4].copy_from_slice(b"XMD2");
    assert_eq!(
        MediaFrame::decode(&bad_magic).expect_err("magic").code,
        ErrorCode::InvalidFrame
    );

    let mut bad_version = header(1, 1, 0, 1, 0, 0);
    bad_version[4..6].copy_from_slice(&2u16.to_be_bytes());
    assert_eq!(
        MediaFrame::decode(&bad_version).expect_err("版本").code,
        ErrorCode::VersionUnsupported
    );

    let bad_kind = header(99, 1, 0, 1, 0, 0);
    assert_eq!(
        MediaFrame::decode(&bad_kind).expect_err("未知 kind").code,
        ErrorCode::InvalidFrame
    );

    let truncated = header(1, 1, 0, 1, 0, 8); // 声明 8 字节 payload，实际 0
    assert_eq!(
        MediaFrame::decode(&truncated).expect_err("截断").code,
        ErrorCode::InvalidFrame
    );

    assert_eq!(
        MediaFrame::decode(&[0u8; 10]).expect_err("不足一个头").code,
        ErrorCode::InvalidFrame
    );
}

/// 粘包：每次 decode 只消费一帧。
#[test]
fn concatenated_frames_are_consumed_one_at_a_time() {
    let mut buf = frame(FrameKind::CodecConfig, 0, 1, 0, b"cfg").encode();
    let second = frame(FrameKind::End, 0, 2, 0, b"");
    buf.extend_from_slice(&second.encode());

    let (first, used) = MediaFrame::decode(&buf).expect("first");
    assert_eq!(first.kind, FrameKind::CodecConfig);
    assert_eq!(used, HEADER_LEN + 3);
    let (rest, used2) = MediaFrame::decode(&buf[used..]).expect("second");
    assert_eq!(rest.kind, FrameKind::End);
    assert_eq!(used2, HEADER_LEN);
}
