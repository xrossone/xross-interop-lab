//! T20 验收（plans/02-files.md §T20）+ T19-01/T19-02 的会话不变量。
//!
//! - T20-01 确认码/commitment 不匹配 → `pairing-failed`、不交付 payload；
//! - T20-02 重放旧 handshake frame → 拒绝；
//! - T20-03 长度溢出/截断 protobuf → `invalid-frame`，无 panic；
//! - T20-04 任意分片 → 与完整输入同结果；
//! - T19-01 只被发现而握手失败 → 状态不得是 transfer 成功；
//! - T19-02 QR 打开可发现 ≠ 认证 → 仍需完整会话 + 确认码。
//!
//! 字段号全部来自 `specs-reviewed/f02-quickshare-lan.md` 的 F-05..F-18（R18 规范/proto）。
//! 这里**不测**发现/QR 广播/传输加密/payload 层——它们没有实现，也不允许实现。

use interop_contract::error::ErrorCode;
use proto_quickshare::framing::{encode_frame, FrameDecoder, DEFAULT_MAX_FRAME_BYTES};
use proto_quickshare::handshake::{
    AlertType, ClientConfig, DiscoveryProvenance, HandshakeConfig, Ukey2Client, Ukey2Server,
    KEY_SCHEDULE_HKDF_SHA256,
};
use proto_quickshare::session::{Event, GateRefusal, QuickShareSession, Role};
use proto_quickshare::wire::{self, Ukey2MessageType};

const NEXT_PROTOCOL: &str = "AES_256_CBC-HMAC_SHA256";

fn server() -> Ukey2Server {
    Ukey2Server::with_fixed_entropy(HandshakeConfig::nearby_default(), [7u8; 32], &[0x11u8; 32])
        .expect("服务端构造")
}

fn client() -> Ukey2Client {
    Ukey2Client::with_fixed_entropy(
        ClientConfig::nearby_default(NEXT_PROTOCOL),
        [9u8; 32],
        &[0x22u8; 32],
    )
    .expect("客户端构造")
}

/// 服务端视角跑一次完整握手，返回（session, 客户端侧密钥）。
fn drive_server_session(chunk_sizes: &[usize]) -> (QuickShareSession, proto_quickshare::handshake::SessionKeys) {
    // 服务端用固定熵（`with_server`）：这样不同分片模式之间的握手字节完全一致，
    // 等价性检查才能比对 auth 串/确认码——分片不应改变任何一个字节的结果。
    let mut session = QuickShareSession::with_server(server());
    let mut cli = client();
    let init = cli.start().expect("client init");
    feed_chunked(&mut session, 0, &encode_frame(&init), chunk_sizes);
    let sinit = session
        .take_outbound()
        .pop()
        .expect("服务端必须产出 ServerInit");
    let (finish, cli_keys) = cli.handle_server_init(&sinit).expect("客户端完成握手");
    feed_chunked(&mut session, 500, &encode_frame(&finish), chunk_sizes);
    (session, cli_keys)
}

fn feed_chunked(session: &mut QuickShareSession, now_ms: u64, bytes: &[u8], chunk_sizes: &[usize]) -> Vec<Event> {
    let mut events = Vec::new();
    let mut idx = 0usize;
    let mut k = 0usize;
    while idx < bytes.len() {
        let want = chunk_sizes.get(k % chunk_sizes.len()).copied().unwrap_or(bytes.len()).max(1);
        let n = want.min(bytes.len() - idx);
        events.extend(session.feed(now_ms, &bytes[idx..idx + n]).expect("喂入"));
        idx += n;
        k += 1;
    }
    if bytes.is_empty() {
        events.extend(session.feed(now_ms, bytes).expect("空喂入"));
    }
    events
}

/// T20-01：commitment 不匹配 → Alert、不建立、无密钥。
#[test]
fn t20_01_commitment_mismatch_is_rejected() {
    let mut srv = server();
    let mut cli = client();
    let client_init = cli.start().expect("client init");
    let server_init = srv
        .handle_client_init(&client_init, 1_000)
        .expect("server init");
    let (client_finish, _client_keys) = cli.handle_server_init(&server_init).expect("client finish");

    // 篡改 ClientFinish 里的 public_key：commitment 是对原消息的 SHA-512
    let tampered = wire::tamper_client_finished_public_key(&client_finish, &[0xAB; 32]);

    let alert = srv
        .handle_client_finish(&tampered, 1_100)
        .expect_err("commitment 不符必须被拒");
    assert_eq!(alert.alert_type, AlertType::BadMessageData);
    assert!(!srv.is_established(), "不得进入 Established");
    assert!(srv.session_keys().is_none(), "失败后不得持有会话密钥");
}

/// T20-01（后半）+ T19-01：握手失败后 payload 一律不得交付。
#[test]
fn t20_01b_handshake_failure_never_delivers_payload() {
    let mut session = QuickShareSession::new(Role::Server, HandshakeConfig::nearby_default());
    session.note_discovery(DiscoveryProvenance::Qr); // 被 QR 发现 ≠ 认证

    let mut cli = client();
    let init = cli.start().expect("init");
    let _ = feed_chunked(&mut session, 0, &encode_frame(&init), &[usize::MAX]);
    let sinit = session.take_outbound().pop().expect("ServerInit");
    let (finish, _) = cli.handle_server_init(&sinit).expect("finish");
    let tampered = wire::tamper_client_finished_public_key(&finish, &[0x01; 32]);

    let events = feed_chunked(&mut session, 1, &encode_frame(&tampered), &[usize::MAX]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Alert {
                alert_type: AlertType::BadMessageData,
                ..
            }
        )),
        "必须产生 commitment 不符的 Alert：{events:?}"
    );
    assert!(!session.is_established());
    assert_eq!(session.payloads_delivered(), 0, "T19-01：握手失败不得记 transfer 成功");
    assert_eq!(
        session.deliver_payload_bytes(4096).expect_err("payload gate 必须关闭"),
        GateRefusal::NotEstablished
    );
    assert_eq!(session.payloads_delivered(), 0);
}

/// T20-02：重放旧 handshake frame → 拒绝（不复用同一 random/commitment）。
#[test]
fn t20_02_replayed_client_init_is_rejected() {
    let mut srv = server();
    let mut cli = client();
    let client_init = cli.start().expect("init");
    let _server_init = srv.handle_client_init(&client_init, 10).expect("首次接受");

    let alert = srv
        .handle_client_init(&client_init, 20)
        .expect_err("同一 ClientInit 重放必须拒绝");
    assert!(
        matches!(
            alert.alert_type,
            AlertType::IncorrectMessage | AlertType::BadMessageType | AlertType::BadRandom
        ),
        "重放应报 INCORRECT_MESSAGE/BAD_MESSAGE_TYPE/BAD_RANDOM，实得 {:?}",
        alert.alert_type
    );
    assert!(!srv.is_established());

    // 会话层同样拒绝：第二次 ClientInit 不得被当成新握手
    let mut session = QuickShareSession::new(Role::Server, HandshakeConfig::nearby_default());
    let _ = feed_chunked(&mut session, 0, &encode_frame(&client_init), &[usize::MAX]);
    let events = feed_chunked(&mut session, 1, &encode_frame(&client_init), &[usize::MAX]);
    assert!(events.iter().any(|e| matches!(e, Event::Alert { .. })));
    assert!(!session.is_established());
}

/// T20-03：长度溢出/截断 protobuf → invalid-frame，无 panic。
#[test]
fn t20_03_length_overflow_and_truncation_are_invalid_frame() {
    // 1) 帧声明长度超过本仓上限：读取前拒绝
    let mut decoder = FrameDecoder::new(DEFAULT_MAX_FRAME_BYTES);
    let mut huge = (DEFAULT_MAX_FRAME_BYTES + 1).to_be_bytes().to_vec();
    huge.extend_from_slice(&[0u8; 8]);
    assert_eq!(
        decoder.push(&huge).expect_err("超限帧必须拒绝").code,
        ErrorCode::ResourceLimit,
        "读取前 resource-limit"
    );

    // 2) 零长度帧
    let mut decoder = FrameDecoder::new(DEFAULT_MAX_FRAME_BYTES);
    assert_eq!(
        decoder.push(&0u32.to_be_bytes()).expect_err("零长").code,
        ErrorCode::InvalidFrame
    );

    // 3) protobuf 截断（声明的字段长度大于剩余字节）
    let truncated = vec![0x12, 0x10, 0x01, 0x02]; // field2 LEN, len=16, 仅 2 字节
    assert_eq!(
        wire::decode_ukey2_message(&truncated).expect_err("截断").code,
        ErrorCode::InvalidFrame
    );

    // 4) varint 溢出（>10 字节）
    let mut varint_bomb = vec![0x08];
    varint_bomb.extend_from_slice(&[0xFFu8; 11]);
    assert_eq!(
        wire::decode_ukey2_message(&varint_bomb).expect_err("varint 溢出").code,
        ErrorCode::InvalidFrame
    );

    // 5) 合法 ClientInit 的任意截断都不得 panic
    let mut cli = client();
    let init = cli.start().expect("init");
    let mut cut = 0usize;
    while cut < init.len() {
        let _ = wire::decode_ukey2_message(&init[..cut]);
        cut += 1;
    }
}

/// T20-04：任意分片与完整输入同结果（同状态、同帧数、同错误）。
#[test]
fn t20_04_arbitrary_fragmentation_matches_whole_input() {
    let patterns: Vec<Vec<usize>> = vec![
        vec![usize::MAX],   // 整段
        vec![1],            // 逐字节
        vec![2, 5, 13],     // 混合
        vec![7],            // 固定 7
    ];
    let mut outcomes = Vec::new();
    for pattern in &patterns {
        let (session, cli_keys) = drive_server_session(pattern);
        let server_auth = session
            .session_keys()
            .map(|k| k.auth_string().to_vec())
            .unwrap_or_default();
        assert!(session.is_established(), "分片 {pattern:?} 必须建立会话");
        assert_eq!(
            server_auth,
            cli_keys.auth_string().to_vec(),
            "分片 {pattern:?}：双方 auth 串必须一致"
        );
        assert_eq!(
            session.session_keys().map(|k| k.pin_code().to_string()),
            Some(cli_keys.pin_code().to_string()),
            "分片 {pattern:?}：双方确认码必须一致"
        );
        outcomes.push((
            session.is_established(),
            session.payloads_delivered(),
            session.session_keys().map(|k| k.pin_code().to_string()),
            server_auth.clone(),
        ));
    }
    let first = &outcomes[0];
    for (i, o) in outcomes.iter().enumerate() {
        assert_eq!(o.0, first.0, "分片模式 {i} 的建立状态不一致");
        assert_eq!(o.1, first.1, "分片模式 {i} 的 payload 计数不一致");
        assert_eq!(o.2, first.2, "分片模式 {i} 的确认码不一致（分片不应改变结果）");
        assert_eq!(o.3, first.3, "分片模式 {i} 的 auth 串不一致（分片不应改变结果）");
    }
}

/// T19-02：QR/可发现性不改变认证要求；确认码未核对时 payload gate 仍关闭。
#[test]
fn t19_02_qr_discovery_does_not_authenticate() {
    let (mut session, cli_keys) = drive_server_session(&[usize::MAX]);
    session.note_discovery(DiscoveryProvenance::Qr);
    assert!(session.is_established());
    assert_eq!(
        session.session_keys().expect("keys").pin_code(),
        cli_keys.pin_code(),
        "双方 PIN 必须一致"
    );

    assert_eq!(
        session.deliver_payload_bytes(1).expect_err("未核对确认码前不得交付"),
        GateRefusal::ConfirmationRequired,
        "T19-02：QR 可发现 ≠ 认证"
    );
    let pin = session
        .session_keys()
        .expect("keys")
        .pin_code()
        .to_string();
    let wrong = if pin == "0000" { "0001".to_string() } else { "0000".to_string() };
    assert_eq!(
        session.confirm_code(&wrong).expect_err("错误确认码必须拒绝").code,
        ErrorCode::PairingFailed
    );
    assert_eq!(session.payloads_delivered(), 0, "错误确认码后仍不得交付");

    session.confirm_code(&pin).expect("正确确认码");
    assert!(session.deliver_payload_bytes(1024).is_ok(), "确认后允许交付");
    assert_eq!(session.payloads_delivered(), 1);
}

/// 规范纪律：握手校验必须给出规范里的 Alert 码（F-07/F-08/F-09/F-12/F-18）。
#[test]
fn handshake_validation_alerts_follow_spec_codes() {
    let mut cli = client();
    let init = cli.start().expect("init");

    let mut srv = server();
    let bad_random = wire::rebuild_client_init(&init, Some(vec![0u8; 31]), None, None, None);
    assert_eq!(
        srv.handle_client_init(&bad_random, 0).expect_err("31 字节 random").alert_type,
        AlertType::BadRandom
    );

    let mut srv = server();
    let bad_version = wire::rebuild_client_init(&init, None, Some(2), None, None);
    assert_eq!(
        srv.handle_client_init(&bad_version, 0).expect_err("version 2").alert_type,
        AlertType::BadVersion
    );

    let mut srv = server();
    let no_commitments = wire::rebuild_client_init(&init, None, None, Some(vec![]), None);
    assert_eq!(
        srv.handle_client_init(&no_commitments, 0).expect_err("空 commitments").alert_type,
        AlertType::BadHandshakeCipher
    );

    let mut srv = server();
    let unknown_cipher = wire::rebuild_client_init(
        &init,
        None,
        None,
        Some(vec![(200, vec![0xAA; 64])]), // CURVE25519_SHA512：本仓不实现
        None,
    );
    assert_eq!(
        srv.handle_client_init(&unknown_cipher, 0).expect_err("未实现 cipher").alert_type,
        AlertType::BadHandshakeCipher
    );

    let mut srv = server();
    let bad_protocol = wire::rebuild_client_init(&init, None, None, None, Some("nope".to_string()));
    assert_eq!(
        srv.handle_client_init(&bad_protocol, 0).expect_err("next_protocol").alert_type,
        AlertType::BadNextProtocol
    );

    let mut srv = server();
    let wrong_type = wire::with_message_type(&init, Ukey2MessageType::ServerInit);
    assert_eq!(
        srv.handle_client_init(&wrong_type, 0).expect_err("类型不符").alert_type,
        AlertType::BadMessageType
    );
}

/// F-15 冲突的可执行固化：规范文本（HKDF-SHA512）与实现（HKDF-SHA256）派生的 auth 串不同。
#[test]
fn f15_hkdf_hash_conflict_is_visible_in_derived_keys() {
    assert_eq!(
        KEY_SCHEDULE_HKDF_SHA256, "HKDF-SHA256",
        "当前采用实现侧（HKDF-SHA256）；规范文本差异待真机裁决"
    );
    let mut srv = server();
    let mut cli = client();
    let init = cli.start().expect("init");
    let sinit = srv.handle_client_init(&init, 0).expect("sinit");
    let (finish, cli_keys) = cli.handle_server_init(&sinit).expect("finish");
    srv.handle_client_finish(&finish, 1).expect("establish");

    let spec_variant = proto_quickshare::crypto::auth_string_spec_variant(
        cli_keys.dh_shared_secret(),
        cli_keys.transcript(),
        32,
    );
    assert_ne!(
        spec_variant,
        cli_keys.auth_string(),
        "SHA-512 变体必须与实现侧不同：这正是需要真机裁决的点（F-15）"
    );
}

/// 超时（本仓策略值）：握手未在期限内完成 → Alert、不建立、无 payload。
#[test]
fn handshake_deadline_is_enforced() {
    let mut session = QuickShareSession::new(Role::Server, HandshakeConfig::nearby_default());
    let mut cli = client();
    let init = cli.start().expect("init");
    let _ = feed_chunked(&mut session, 0, &encode_frame(&init), &[usize::MAX]);
    let events = session.feed(60_001, &[]).expect("时间推进");
    assert!(
        events.iter().any(|e| matches!(e, Event::Alert { .. })),
        "超时必须产生 Alert 事件"
    );
    assert!(!session.is_established());
    assert_eq!(session.payloads_delivered(), 0);
    assert!(session.feed(60_002, &[]).is_err(), "已终止会话不得继续喂入");
}
