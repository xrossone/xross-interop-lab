//! T21+ 验收测试：keep-alive 与 paired-key 控制帧（字段行 F-30..F-33；语料 qs-019..qs-024）。
//!
//! 全部字节自制；paired-key 材料是测试用固定字节——**不代表任何真实配对材料**
//! （F-32：材料不可离线推导，参考实现也只是随机字节）。

#![allow(clippy::result_large_err)]

use interop_contract::error::ErrorCode;
use proto_quickshare::control::{
    decode_keepalive_offline, unwrap_bytes_payload, wrap_inner_as_bytes_payload, KeepAliveFrame,
    KeepAliveTracker, PairedKeyDecision, PairedKeyEvent, PairedKeyExchange, PairedKeyMaterial,
    PairedKeyResultFrame, PairedKeyState, PairedKeyStatus, INNER_TYPE_PAIRED_KEY_ENCRYPTION,
    INNER_TYPE_PAIRED_KEY_RESULT, KEEPALIVE_INTERVAL_MS, KEEPALIVE_TIMEOUT_MS,
    MAX_PAIRED_KEY_BYTES, OUTER_TYPE_KEEP_ALIVE,
};
use proto_quickshare::payload::{decode_payload_transfer_frame, PacketType, PayloadKind};

const SIGNED_DATA: &[u8] = &[0x11; 72]; // 自制占位（参考实现为 72 字节随机）
const SECRET_ID_HASH: &[u8] = &[0x22; 6]; // 自制占位（参考实现为 6 字节随机）

fn material() -> PairedKeyMaterial {
    PairedKeyMaterial::new(SIGNED_DATA.to_vec(), SECRET_ID_HASH.to_vec()).expect("材料合法")
}

fn read_varint(buf: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    let mut shift = 0;
    for (i, byte) in buf.iter().enumerate() {
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return (value, i + 1);
        }
        shift += 7;
    }
    panic!("截断的 varint");
}

/// 从 `OfflineFrame` 里取出 `V1Frame.type`（测试自用；长度前缀是真 varint，不能按固定偏移读）。
fn offline_type(buf: &[u8]) -> u64 {
    assert_eq!(buf[0], 0x08, "OfflineFrame 首字段应是 version");
    assert_eq!(buf[1], 1, "OfflineFrame.Version.V1");
    assert_eq!(buf[2], 0x12, "OfflineFrame 次字段应是 v1");
    let (len, n) = read_varint(&buf[3..]);
    let v1 = &buf[3 + n..3 + n + len as usize];
    assert_eq!(v1[0], 0x08, "V1Frame 首字段应是 type");
    read_varint(&v1[1..]).0
}

// ------------------------------------------------------------------ F-31 keep-alive 帧

#[test]
fn keepalive_frame_roundtrip_and_layer_numbers() {
    for (ack, seq) in [(false, 0u32), (true, 0), (false, 7), (true, 4_294_967_295)] {
        let frame = KeepAliveFrame::new(ack, seq);
        let decoded = KeepAliveFrame::decode(&frame.encode()).expect("可回读");
        assert_eq!(decoded, frame, "KeepAliveFrame 往返（F-31）");

        let offline = frame.encode_offline();
        let decoded = decode_keepalive_offline(&offline).expect("外层帧可回读");
        assert_eq!(decoded, frame, "外层 OfflineFrame 往返");
        // 层号自检：外层 type = 5（F-30）。
        assert_eq!(
            offline_type(&offline),
            OUTER_TYPE_KEEP_ALIVE as u64,
            "外层 V1Frame.type 必须是 KEEP_ALIVE(5)"
        );
    }

    // 参考实现只设 ack、不带 seq（F-31：seq_num 是可选字段）。
    let ack_only = KeepAliveFrame::new(true, 0);
    assert_eq!(ack_only.encode(), vec![0x08, 0x01, 0x10, 0x00]);
}

#[test]
fn keepalive_frame_negatives() {
    // 外层用了**内层**编号（3 = 内层 PAIRED_KEY_ENCRYPTION）：必须拒绝并说明分层。
    let mut inner_numbered = Vec::new();
    let mut v1 = Vec::new();
    v1.extend_from_slice(&[0x08, INNER_TYPE_PAIRED_KEY_ENCRYPTION as u8]); // type = 3
    v1.extend_from_slice(&[0x32, 0x02, 0x08, 0x01]); // keep_alive 字段 6
    inner_numbered.extend_from_slice(&[0x08, 0x01]); // version = V1
    inner_numbered.push(0x12);
    inner_numbered.push(v1.len() as u8);
    inner_numbered.extend_from_slice(&v1);
    let err = decode_keepalive_offline(&inner_numbered).expect_err("内层编号必须拒绝");
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    assert!(err.message.contains("内层"), "{err:?}");

    // 其它外层类型 → unsupported-feature。
    let mut other = Vec::new();
    let mut v1 = Vec::new();
    v1.extend_from_slice(&[0x08, 6]); // type = 6 (DISCONNECTION)
    v1.extend_from_slice(&[0x32, 0x02, 0x08, 0x01]);
    other.extend_from_slice(&[0x08, 0x01]);
    other.push(0x12);
    other.push(v1.len() as u8);
    other.extend_from_slice(&v1);
    assert_eq!(
        decode_keepalive_offline(&other).unwrap_err().code,
        ErrorCode::UnsupportedFeature
    );

    // 缺 keep_alive 字段。
    let mut v1 = Vec::new();
    v1.extend_from_slice(&[0x08, OUTER_TYPE_KEEP_ALIVE as u8]);
    let mut missing = Vec::new();
    missing.extend_from_slice(&[0x08, 0x01]);
    missing.push(0x12);
    missing.push(v1.len() as u8);
    missing.extend_from_slice(&v1);
    assert_eq!(
        decode_keepalive_offline(&missing).unwrap_err().code,
        ErrorCode::InvalidFrame
    );

    // 截断 / 版本不是 V1。
    assert!(decode_keepalive_offline(&[0x08]).is_err());
    let mut wrong_version = KeepAliveFrame::new(true, 1).encode_offline();
    wrong_version[1] = 2;
    assert_eq!(
        decode_keepalive_offline(&wrong_version).unwrap_err().code,
        ErrorCode::VersionUnsupported
    );
}

#[test]
fn keepalive_cadence_and_expiry() {
    let mut tracker = KeepAliveTracker::new();
    assert_eq!(
        tracker.stats().interval_ms,
        KEEPALIVE_INTERVAL_MS,
        "10 s 来源取值"
    );
    assert_eq!(
        tracker.stats().timeout_ms,
        KEEPALIVE_TIMEOUT_MS,
        "超时是本仓策略"
    );

    // 到点才发（间隔 = 10 s）。
    assert!(tracker.on_tick(0).is_some(), "首帧立即发");
    assert!(tracker.on_tick(1_000).is_none());
    assert!(tracker.on_tick(KEEPALIVE_INTERVAL_MS - 1).is_none());
    let second = tracker.on_tick(KEEPALIVE_INTERVAL_MS).expect("到点发");
    assert_eq!(second.seq_num, 2, "序号自增");
    assert!(!second.ack, "主动帧不带 ack");

    // 收到对端帧（ack=false）→ 回一帧 ack=true（R15 行为）。
    let reply = tracker
        .on_received(&KeepAliveFrame::new(false, 1), KEEPALIVE_INTERVAL_MS + 100)
        .expect("可回应")
        .expect("应当回 ack");
    assert!(reply.ack);
    // 收到应答帧（ack=true）→ 不再回。
    assert!(tracker
        .on_received(&KeepAliveFrame::new(true, 0), KEEPALIVE_INTERVAL_MS + 200)
        .expect("可记账")
        .is_none());

    // 超时：阈值内不判死，超过则判死且不再回应（本仓策略）。
    let base = KEEPALIVE_INTERVAL_MS + 200;
    assert!(!tracker.expired(base + KEEPALIVE_TIMEOUT_MS));
    assert!(tracker.expired(base + KEEPALIVE_TIMEOUT_MS + 1));
    assert!(tracker.stats().expired);
    let err = tracker
        .on_received(
            &KeepAliveFrame::new(false, 9),
            base + KEEPALIVE_TIMEOUT_MS + 2,
        )
        .expect_err("判死后不得继续回应");
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    assert!(
        tracker.on_tick(base + KEEPALIVE_TIMEOUT_MS + 3).is_none(),
        "判死后不再发"
    );

    // 计数。
    let stats = tracker.stats();
    assert_eq!(stats.sent, 2);
    assert_eq!(stats.received, 2);
    assert_eq!(stats.acks_sent, 1);
}

// ------------------------------------------------------------------ F-32/F-33 paired-key

#[test]
fn paired_key_material_roundtrip() {
    let full = PairedKeyMaterial {
        signed_data: SIGNED_DATA.to_vec(),
        secret_id_hash: SECRET_ID_HASH.to_vec(),
        optional_signed_data: Some(vec![0x33; 16]),
        qr_code_handshake_data: Some(vec![0x44; 24]),
    };
    let inner = full.encode_inner();
    assert_eq!(
        offline_type(&inner),
        INNER_TYPE_PAIRED_KEY_ENCRYPTION as u64,
        "内层 type = 3（F-30）"
    );
    let decoded = PairedKeyMaterial::decode_inner(&inner).expect("可回读");
    assert_eq!(decoded, full);

    // 只带前两个字段也可（四个字段都是 optional，F-32）。
    let minimal = material();
    assert_eq!(
        PairedKeyMaterial::decode_inner(&minimal.encode_inner()).expect("可回读"),
        minimal
    );

    // 负向：空材料、超上限。
    assert_eq!(
        PairedKeyMaterial::new(Vec::new(), SECRET_ID_HASH.to_vec())
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    let oversized =
        PairedKeyMaterial::new(vec![0; MAX_PAIRED_KEY_BYTES + 1], vec![1, 2]).unwrap_err();
    assert_eq!(oversized.code, ErrorCode::ResourceLimit);

    // 内层帧用了外层编号（7 = 外层 PAIRED_KEY_ENCRYPTION）→ unsupported-feature。
    let mut wrong_layer = Vec::new();
    let mut v1 = Vec::new();
    v1.extend_from_slice(&[0x08, 7]);
    v1.extend_from_slice(&[0x22, 0x02, 0x0a, 0x00]); // 内层字段 4
    wrong_layer.extend_from_slice(&[0x08, 0x01]);
    wrong_layer.push(0x12);
    wrong_layer.push(v1.len() as u8);
    wrong_layer.extend_from_slice(&v1);
    assert_eq!(
        PairedKeyMaterial::decode_inner(&wrong_layer)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedFeature
    );
}

#[test]
fn paired_key_result_statuses_and_unknown_refused() {
    for status in [
        PairedKeyStatus::Unknown,
        PairedKeyStatus::Success,
        PairedKeyStatus::Fail,
        PairedKeyStatus::Unable,
    ] {
        let frame = PairedKeyResultFrame {
            status,
            os_type: Some(1),
        };
        let inner = frame.encode_inner();
        assert_eq!(
            offline_type(&inner),
            INNER_TYPE_PAIRED_KEY_RESULT as u64,
            "内层 type = 4"
        );
        assert_eq!(
            PairedKeyResultFrame::decode_inner(&inner).expect("可回读"),
            frame
        );
    }
    // 参考实现回的就是 UNABLE。
    assert_eq!(
        PairedKeyResultFrame::unable().status,
        PairedKeyStatus::Unable
    );

    // 未知 status（9）→ invalid-frame，且**不得**被当成 SUCCESS。
    let mut inner = Vec::new();
    let mut v1 = Vec::new();
    v1.extend_from_slice(&[0x08, INNER_TYPE_PAIRED_KEY_RESULT as u8]);
    v1.extend_from_slice(&[0x2a, 0x02, 0x08, 0x09]); // status = 9
    inner.extend_from_slice(&[0x08, 0x01]);
    inner.push(0x12);
    inner.push(v1.len() as u8);
    inner.extend_from_slice(&v1);
    let err = PairedKeyResultFrame::decode_inner(&inner).expect_err("未知 status 必须拒绝");
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    assert!(err.message.contains("不得当成 SUCCESS"), "{err:?}");
}

#[test]
fn paired_key_frames_ride_in_bytes_payloads() {
    // 内层帧装进 BYTES payload（F-30 的内层载体）。
    let inner = material().encode_inner();
    let wrapped = wrap_inner_as_bytes_payload(&inner, 1234).expect("可包装");
    let parsed = decode_payload_transfer_frame(&wrapped).expect("可解出 payload 帧");
    assert_eq!(parsed.packet_type, PacketType::Data);
    let header = parsed.header.as_ref().expect("有 header");
    assert_eq!(header.kind, PayloadKind::Bytes, "协商帧必须走 BYTES");
    assert_eq!(header.total_size, inner.len() as u64);
    let chunk = parsed.chunk.as_ref().expect("有 chunk");
    assert_eq!(chunk.offset, 0);
    assert_eq!(chunk.flags, 0, "协商帧不带 LAST_CHUNK（参考实现同此）");

    let (body, payload_id) = unwrap_bytes_payload(&wrapped).expect("可解回内层帧");
    assert_eq!(body, inner);
    assert_eq!(payload_id, 1234);

    // FILE 载荷不是协商载体 → invalid-frame。
    let file_frame =
        proto_quickshare::payload::PayloadTransferFrame::data(7, 10, 0, true, vec![0u8; 10]);
    let file_bytes =
        proto_quickshare::payload::encode_payload_transfer_frame(&file_frame).expect("可编码");
    let err = unwrap_bytes_payload(&file_bytes).expect_err("FILE 载荷必须拒绝");
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    assert!(err.message.contains("BYTES"), "{err:?}");

    // payload id = 0 非法。
    assert_eq!(
        wrap_inner_as_bytes_payload(&inner, 0).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
}

#[test]
fn paired_key_exchange_never_skips_confirmation_by_default() {
    // 默认策略：即使对端报 SUCCESS 也仍然要求用户核对 4 位确认码（本仓没有配对存储）。
    let mut exchange = PairedKeyExchange::new();
    assert_eq!(exchange.state(), PairedKeyState::Idle);
    let _ours = exchange.begin(&material()).expect("可发起");
    assert_eq!(exchange.state(), PairedKeyState::EncryptionSent);

    // 对端回的 result 帧（自制）。
    let peer_success = wrap_inner_as_bytes_payload(
        &PairedKeyResultFrame {
            status: PairedKeyStatus::Success,
            os_type: Some(1),
        }
        .encode_inner(),
        777,
    )
    .expect("可包装");

    match exchange.on_bytes_payload(&peer_success).expect("可处理") {
        PairedKeyEvent::Result { status, decision } => {
            assert_eq!(status, PairedKeyStatus::Success);
            assert_eq!(
                decision,
                PairedKeyDecision::RequireConfirmation,
                "默认策略下 SUCCESS 不得免确认码（F-32：材料不可离线推导）"
            );
        }
        other => panic!("这里的内层是 result 帧：{other:?}"),
    }
    assert_eq!(exchange.peer_status(), Some(PairedKeyStatus::Success));
    assert_eq!(exchange.decision(), PairedKeyDecision::RequireConfirmation);

    // 只有调用方显式开启开关**且**对端报 SUCCESS 才允许跳过。
    let mut opted_in = PairedKeyExchange::new().allowing_skip_confirmation();
    let _ = opted_in.begin(&material()).expect("可发起");
    let outcome = opted_in.on_bytes_payload(&peer_success).expect("可处理");
    assert!(matches!(
        outcome,
        PairedKeyEvent::Result {
            decision: PairedKeyDecision::SkipConfirmation,
            ..
        }
    ));

    // 开了开关但对端报 UNABLE（参考实现的实际取值）→ 仍然要求确认码。
    let mut unable = PairedKeyExchange::new().allowing_skip_confirmation();
    let _ = unable.begin(&material()).expect("可发起");
    let unable_payload =
        wrap_inner_as_bytes_payload(&PairedKeyResultFrame::unable().encode_inner(), 99)
            .expect("可包装");
    assert!(matches!(
        unable.on_bytes_payload(&unable_payload).expect("可处理"),
        PairedKeyEvent::Result {
            status: PairedKeyStatus::Unable,
            decision: PairedKeyDecision::RequireConfirmation,
        }
    ));
}

#[test]
fn paired_key_exchange_replies_unable_and_tracks_state() {
    let mut exchange = PairedKeyExchange::new();
    let ours = exchange.begin(&material()).expect("可发起");
    assert_eq!(exchange.state(), PairedKeyState::EncryptionSent);

    // 对端发来它的 encryption 帧 → 我们需要回一条 result（参考实现回 UNABLE）。
    let peer_encryption =
        wrap_inner_as_bytes_payload(&material().encode_inner(), 42).expect("可包装");
    let event = exchange.on_bytes_payload(&peer_encryption).expect("可处理");
    let reply = match event {
        PairedKeyEvent::NeedResult { reply } => reply,
        other => panic!("应当是 NeedResult：{other:?}"),
    };
    let (inner, payload_id) = unwrap_bytes_payload(&reply).expect("回包可解");
    assert_eq!(payload_id, 42, "回包沿用对端的 payload id");
    assert_eq!(
        PairedKeyResultFrame::decode_inner(&inner)
            .expect("可解")
            .status,
        PairedKeyStatus::Unable,
        "参考实现回 UNABLE（F-33）"
    );
    assert_eq!(exchange.state(), PairedKeyState::EncryptionReceived);

    // 随后对端发来 result → 状态推进，决策随之更新。
    let peer_result = wrap_inner_as_bytes_payload(
        &PairedKeyResultFrame {
            status: PairedKeyStatus::Fail,
            os_type: None,
        }
        .encode_inner(),
        43,
    )
    .expect("可包装");
    assert!(matches!(
        exchange.on_bytes_payload(&peer_result).expect("可处理"),
        PairedKeyEvent::Result {
            status: PairedKeyStatus::Fail,
            ..
        }
    ));
    assert_eq!(exchange.state(), PairedKeyState::ResultReceived);
    assert_eq!(exchange.decision(), PairedKeyDecision::RequireConfirmation);
    let _ = ours;
}
