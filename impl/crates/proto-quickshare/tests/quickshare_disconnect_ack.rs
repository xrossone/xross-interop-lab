//! T22headless 验收：DisconnectionFrame（safe-to-disconnect）与 PAYLOAD_ACK（语料 qs-025..qs-030）。
//!
//! 全部字节自制；字段号与规则来自 `specs-reviewed/f02` 的 F-34..F-40（每行带 R17/R15 行级引用）。
//! 这里证明的是**帧与决策与参考实现一致**，不是"真机走通了 safe-to-disconnect"。

#![allow(clippy::result_large_err)]

use interop_contract::error::ErrorCode;
use proto_quickshare::control::{
    classify_payload_frame, decode_payload_ack, encode_payload_ack, should_send_payload_ack,
    AckOutcome, DisconnectAction, DisconnectionFrame, KeepAlivePolicy, KeepAliveSource,
    PayloadAckTracker, PayloadFrameClass, INDETERMINATE_TOTAL_SIZE, KEEPALIVE_INTERVAL_MS,
    KEEPALIVE_TIMEOUT_MS, OUTER_FIELD_DISCONNECTION, OUTER_TYPE_DISCONNECTION,
};
use proto_quickshare::payload::{
    decode_payload_transfer_frame, encode_payload_transfer_frame, PayloadChunk, PayloadHeader,
    PayloadKind, PacketType, PayloadTransferFrame,
};

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

/// 从外层 `OfflineFrame` 取 `V1Frame.type`（长度前缀是真 varint）。
fn offline_type(buf: &[u8]) -> u64 {
    let (len, n) = read_varint(&buf[3..]);
    let v1 = &buf[3 + n..3 + n + len as usize];
    read_varint(&v1[1..]).0
}

// ---------------------------------------------------------------- F-34/F-36

#[test]
fn disconnection_frame_roundtrips_all_four_byte_shapes() {
    let cases = [
        // R17 的构造器：两个 bool 都显式写出（false 也写）。
        (
            "R17 (false,false)",
            DisconnectionFrame::new(Some(false), Some(false)),
        ),
        // NearDrop：一个字段都不设（空正文）。
        ("NearDrop 空正文", DisconnectionFrame::empty()),
        ("发起 (true,false)", DisconnectionFrame::new(Some(true), Some(false))),
        ("应答 (true,true)", DisconnectionFrame::new(Some(true), Some(true))),
    ];
    for (label, frame) in cases {
        let bytes = frame.encode_offline();
        assert_eq!(
            offline_type(&bytes),
            OUTER_TYPE_DISCONNECTION as u64,
            "{label}: 外层 type 必须是 DISCONNECTION(6)"
        );
        let decoded = DisconnectionFrame::decode_offline(&bytes).expect("可回读");
        assert_eq!(decoded, frame, "{label}: 往返必须等值");

        // 字段**存在性**必须保留：空正文解出来两个都是 None，而不是 Some(false)。
        match label {
            "NearDrop 空正文" => {
                assert_eq!(decoded.request_safe_to_disconnect, None);
                assert_eq!(decoded.ack_safe_to_disconnect, None);
                assert!(!decoded.has_request(), "空正文不得报告字段存在");
            }
            "R17 (false,false)" => {
                assert_eq!(decoded.request_safe_to_disconnect, Some(false));
                assert!(decoded.has_request(), "显式 false 必须报存在");
                assert!(decoded.has_ack());
            }
            _ => {}
        }
    }

    // 空正文与显式 false 是**不同字节**（这正是 F-36 的要点）。
    assert_ne!(
        DisconnectionFrame::empty().encode_offline(),
        DisconnectionFrame::new(Some(false), Some(false)).encode_offline()
    );
    assert_eq!(OUTER_FIELD_DISCONNECTION, 7, "外层字段号");
}

#[test]
fn disconnection_negatives() {
    // 外层类型不符、截断、版本不符都拒绝。
    let mut other_type = DisconnectionFrame::empty().encode_offline();
    let (len, n) = read_varint(&other_type[3..]);
    let v1_start = 3 + n;
    other_type[v1_start + 1] = 5; // KEEP_ALIVE
    other_type.truncate(v1_start + len as usize + 1);
    assert_eq!(
        DisconnectionFrame::decode_offline(&other_type).unwrap_err().code,
        ErrorCode::UnsupportedFeature
    );
    assert!(DisconnectionFrame::decode_offline(&[0x08]).is_err());
    let mut wrong_version = DisconnectionFrame::empty().encode_offline();
    wrong_version[1] = 2;
    assert_eq!(
        DisconnectionFrame::decode_offline(&wrong_version).unwrap_err().code,
        ErrorCode::VersionUnsupported
    );
}

// ---------------------------------------------------------------- F-35

#[test]
fn disconnection_receive_rules_are_three_way() {
    // ① 未请求（缺席或 false）→ 立即关闭。
    for frame in [
        DisconnectionFrame::empty(),
        DisconnectionFrame::new(Some(false), None),
        DisconnectionFrame::new(None, Some(true)),
    ] {
        assert_eq!(
            frame.decision(),
            DisconnectAction::CloseNow,
            "{frame:?} 应立即关闭"
        );
    }
    // ② request=1, ack=1 → 标记并通知停止等待，**不回帧**。
    assert_eq!(
        DisconnectionFrame::new(Some(true), Some(true)).decision(),
        DisconnectAction::MarkedAndNotified
    );
    // ③ request=1, ack=0 → 标记（不通知）+ 回一帧 (true,true)。
    match DisconnectionFrame::new(Some(true), Some(false)).decision() {
        DisconnectAction::MarkedAndReply(reply) => {
            assert_eq!(reply, DisconnectionFrame::new(Some(true), Some(true)));
            assert_eq!(
                reply,
                DisconnectionFrame::reply_ack(),
                "回帧形状固定为 (true,true)"
            );
            // 回帧本身可编码、可回读，且仍走外层 6/字段 7。
            let bytes = reply.encode_offline();
            assert_eq!(offline_type(&bytes), 6);
            assert_eq!(DisconnectionFrame::decode_offline(&bytes).expect("可回读"), reply);
        }
        other => panic!("(true,false) 必须产生回帧：{other:?}"),
    }
    // request=true 但 ack 缺席：来源只检查 request → 走 ③ 的"不通知"分支但**没有** ack 值；
    // 本仓按 ack 缺席（false 语义）处理，仍产生回帧。
    assert!(matches!(
        DisconnectionFrame::new(Some(true), None).decision(),
        DisconnectAction::MarkedAndReply(_)
    ));
}

// ---------------------------------------------------------------- F-37/F-39

#[test]
fn payload_ack_shape_and_classification() {
    let bytes = encode_payload_ack(4242).expect("可编码");
    // 外层仍是 PAYLOAD_TRANSFER(3)；正文只有 header{id,total_size=-1}。
    let frame = decode_payload_transfer_frame(&bytes).expect("可解码");
    assert_eq!(frame.packet_type, PacketType::PayloadAck);
    let header = frame.header.as_ref().expect("有 header");
    assert_eq!(header.id, 4242);
    assert_eq!(
        header.total_size, INDETERMINATE_SIZE_U64_LOCAL,
        "total_size 必须是 protobuf int64 -1（kIndeterminateSize）"
    );
    assert!(frame.chunk.is_none(), "ack 帧不得带 chunk");
    assert_eq!(decode_payload_ack(&bytes).expect("可解码"), 4242);
    assert_eq!(INDETERMINATE_TOTAL_SIZE, u64::MAX);

    // 分类：DATA → Data，ACK → Ack，CONTROL → 明确拒绝（F-39：官方标注已废弃）。
    assert_eq!(
        classify_payload_frame(&frame).expect("可分类"),
        PayloadFrameClass::Ack
    );
    let data = PayloadTransferFrame::data(7, 4, 0, true, vec![0u8; 4]);
    assert_eq!(
        classify_payload_frame(&data).expect("可分类"),
        PayloadFrameClass::Data
    );
    let control = PayloadTransferFrame::control(PacketType::Control);
    let err = classify_payload_frame(&control).expect_err("CONTROL 必须拒绝");
    assert_eq!(err.code, ErrorCode::UnsupportedFeature);
    assert!(
        err.message.contains("PAYLOAD_ACK"),
        "拒绝理由要指向替代路径：{}",
        err.message
    );

    // 负例：把 DATA 帧当 ack 解 → 拒绝；ack 带 chunk → 拒绝；total_size 不是 -1 → 拒绝。
    assert_eq!(
        decode_payload_ack(&encode_payload_transfer_frame(&data).expect("可编码"))
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    let mut ack_with_chunk = frame.clone();
    ack_with_chunk.chunk = Some(PayloadChunk {
        flags: 0,
        offset: 0,
        body: vec![1, 2, 3],
    });
    assert_eq!(
        decode_payload_ack(&encode_payload_transfer_frame(&ack_with_chunk).expect("可编码"))
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    let sized = PayloadTransferFrame {
        packet_type: PacketType::PayloadAck,
        header: Some(PayloadHeader {
            id: 9,
            kind: PayloadKind::File,
            total_size: 123,
            file_name: None,
            parent_folder: None,
        }),
        chunk: None,
    };
    assert_eq!(
        decode_payload_ack(&encode_payload_transfer_frame(&sized).expect("可编码"))
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
}

// ---------------------------------------------------------------- F-38

#[test]
fn payload_ack_policy_excludes_bytes_and_non_last_chunks() {
    // 只有"非 BYTES 的末块"才发 ack（来源的启用条件明确排除 BYTES）。
    assert!(!should_send_payload_ack(PayloadKind::Bytes, true), "BYTES 不发 ack");
    assert!(should_send_payload_ack(PayloadKind::File, true));
    assert!(should_send_payload_ack(PayloadKind::Stream, true));
    assert!(!should_send_payload_ack(PayloadKind::File, false), "非末块不发");
    assert!(!should_send_payload_ack(PayloadKind::Bytes, false));

    // 收到 ack 的三种分支：未知 → 忽略；incoming → 忽略；outgoing → 标记。
    let mut tracker = PayloadAckTracker::new();
    tracker.register_outgoing(11);
    tracker.register_incoming(12);
    assert_eq!(tracker.on_ack(11), AckOutcome::Marked);
    assert!(tracker.acked(11));
    assert_eq!(
        tracker.on_ack(99),
        AckOutcome::IgnoredUnknownPayload,
        "未知 payload 的 ack 只忽略、不报错（来源行为）"
    );
    assert_eq!(
        tracker.on_ack(12),
        AckOutcome::IgnoredIncomingPayload,
        "对 incoming payload 的 ack 忽略"
    );
    assert!(!tracker.acked(12));
    // 重复 ack 仍算标记（幂等），不产生新状态。
    assert_eq!(tracker.on_ack(11), AckOutcome::Marked);
}

// ---------------------------------------------------------------- F-40

#[test]
fn keepalive_negotiation_validates_and_falls_back() {
    // 缺席 → 本仓策略值，来源标注为本仓策略。
    let policy = KeepAlivePolicy::from_negotiation(None, None).expect("可回退");
    assert_eq!(policy.interval_ms, KEEPALIVE_INTERVAL_MS);
    assert_eq!(policy.timeout_ms, KEEPALIVE_TIMEOUT_MS);
    assert_eq!(policy.source, KeepAliveSource::RepoPolicy);
    assert_eq!(KeepAlivePolicy::repo_default(), policy);

    // 请求侧同时给了两个值 → 采纳并标注"协商"。
    let negotiated = KeepAlivePolicy::from_negotiation(Some(15_000), Some(45_000)).expect("合法");
    assert_eq!(negotiated.interval_ms, 15_000);
    assert_eq!(negotiated.timeout_ms, 45_000);
    assert_eq!(negotiated.source, KeepAliveSource::Negotiated);

    // 响应侧只给 timeout：interval 回退本仓值。
    let partial = KeepAlivePolicy::from_negotiation(None, Some(45_000)).expect("合法");
    assert_eq!(partial.interval_ms, KEEPALIVE_INTERVAL_MS);
    assert_eq!(partial.timeout_ms, 45_000);
    assert_eq!(partial.source, KeepAliveSource::Negotiated);

    // 非法值：非正、timeout < interval、超出 u32。
    for (interval, timeout) in [
        (Some(0i64), Some(30_000i64)),
        (Some(-1), Some(30_000)),
        (Some(10_000), Some(0)),
        (Some(10_000), Some(5_000)),
        (Some(i64::from(u32::MAX) + 1), None),
    ] {
        assert_eq!(
            KeepAlivePolicy::from_negotiation(interval, timeout)
                .unwrap_err()
                .code,
            ErrorCode::InvalidFrame,
            "interval={interval:?} timeout={timeout:?} 必须拒绝"
        );
    }
}

/// `-1` 在 `PayloadHeader.total_size`（u64）里的呈现（protobuf int64 二补数）。
const INDETERMINATE_SIZE_U64_LOCAL: u64 = u64::MAX;
