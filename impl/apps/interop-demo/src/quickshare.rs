//! Quick Share / UKEY2 场景（T19/T20）：把已实现的握手与 framing 跑成状态输出。
//!
//! 立场：跑的是**真实**密码原语与状态机（P-256 ECDH、SHA-512 commitment、HKDF-SHA256），
//! 但两方都在本进程内、没有连任何设备 ⇒ 只证明"我们的实现自洽"，不证明与 Android 互通。
//! 发现/QR/传输加密/payload 层未实现（P-F02-1/3 未关闭）——报告里如实标 blocked。

use crate::report::QuickShareReport;
use proto_quickshare::control::{
    classify_payload_frame, decode_keepalive_offline, decode_payload_ack, encode_payload_ack,
    should_send_payload_ack, unwrap_bytes_payload, wrap_inner_as_bytes_payload, AckOutcome,
    DisconnectAction, DisconnectionFrame, KeepAlivePolicy, KeepAliveSource, KeepAliveTracker,
    PairedKeyDecision, PairedKeyEvent, PairedKeyExchange, PairedKeyMaterial, PairedKeyResultFrame,
    PairedKeyStatus, PayloadAckTracker, PayloadFrameClass, INNER_TYPE_PAIRED_KEY_ENCRYPTION,
    KEEPALIVE_INTERVAL_MS, KEEPALIVE_TIMEOUT_MS,
};
use proto_quickshare::payload::{PacketType, PayloadKind};
use serde_json::json;
use proto_quickshare::framing::{encode_frame, DEFAULT_MAX_FRAME_BYTES, LENGTH_PREFIX_BYTES};
use proto_quickshare::handshake::{
    ClientConfig, DiscoveryProvenance, HandshakeConfig, Ukey2Client, Ukey2Server,
    KEY_SCHEDULE_HKDF_SHA256,
};
use proto_quickshare::payload::PayloadTransferFrame;
use proto_quickshare::receive::{FileOffer, ReceiveDecision, ReceiveSession};
use proto_quickshare::secure_message::{ChannelRole, D2DKeySchedule, SecureMessageChannel};
use proto_quickshare::session::{Event, GateRefusal, QuickShareSession};
use proto_quickshare::wire::{self, Ukey2MessageType};

const NEXT_PROTOCOL: &str = "AES_256_CBC-HMAC_SHA256";

/// 自制负向用例：commitment 不符 / random 长度非法 / version 非法 / cipher 未实现 /
/// next_protocol 不匹配 / 外层类型不符 —— 逐项记录规范 Alert 码。
pub fn quickshare_scenario() -> (QuickShareReport, bool) {
    let mut ok = true;

    // ---- 正向：两实例完整握手（生产熵；客户端用固定标量以便复现，仅演示用）----
    let mut client = match Ukey2Client::with_fixed_entropy(
        ClientConfig::nearby_default(NEXT_PROTOCOL),
        [0x5Au8; 32],
        &[0x33u8; 32],
    ) {
        Ok(c) => c,
        Err(_) => {
            return (failed_report(), false);
        }
    };
    let mut server = match Ukey2Server::new(HandshakeConfig::nearby_default()) {
        Ok(s) => s,
        Err(_) => return (failed_report(), false),
    };
    let init = match client.start() {
        Ok(v) => v,
        Err(_) => return (failed_report(), false),
    };
    let server_init = match server.handle_client_init(&init, 0) {
        Ok(v) => v,
        Err(_) => return (failed_report(), false),
    };
    let (client_finish, client_keys) = match client.handle_server_init(&server_init) {
        Ok(v) => v,
        Err(_) => return (failed_report(), false),
    };
    if server.handle_client_finish(&client_finish, 1).is_err() {
        ok = false;
    }
    let (established, auth_matches, pin_matches, next_matches, auth_len, next_len, transcript_len, pin) =
        match server.session_keys() {
            Some(k) => (
                client.is_established() && server.is_established(),
                k.auth_string_matches(client_keys.auth_string()),
                k.pin_code() == client_keys.pin_code(),
                k.next_protocol_secret() == client_keys.next_protocol_secret(),
                k.auth_string().len(),
                k.next_protocol_secret().len(),
                k.transcript().len(),
                k.pin_code().to_string(),
            ),
            None => return (failed_report(), false),
        };
    if !(established && auth_matches && pin_matches && next_matches) {
        ok = false;
    }

    // ---- 分片等价（T20-04）：同一条流按 4 种切分喂入会话，结果必须一致 ----
    let mut signatures: Vec<String> = Vec::new();
    for chunk in [usize::MAX, 1, 7] {
        // 服务端也用固定材料（自制 fixture）：否则每种切分的密钥都不同，无法比较等价性。
        let fixed_server = match Ukey2Server::with_fixed_entropy(
            HandshakeConfig::nearby_default(),
            [0x77u8; 32],
            &[0x88u8; 32],
        ) {
            Ok(s) => s,
            Err(_) => return (failed_report(), false),
        };
        let mut session = QuickShareSession::with_server(fixed_server);
        let mut cli = match Ukey2Client::with_fixed_entropy(
            ClientConfig::nearby_default(NEXT_PROTOCOL),
            [0x5Au8; 32],
            &[0x33u8; 32],
        ) {
            Ok(c) => c,
            Err(_) => return (failed_report(), false),
        };
        let init = match cli.start() {
            Ok(v) => v,
            Err(_) => return (failed_report(), false),
        };
        let _ = feed_chunked(&mut session, 0, &encode_frame(&init), chunk);
        let sinit = match session.take_outbound().pop() {
            Some(v) => v,
            None => return (failed_report(), false),
        };
        let (finish, _) = match cli.handle_server_init(&sinit) {
            Ok(v) => v,
            Err(_) => return (failed_report(), false),
        };
        let _ = feed_chunked(&mut session, 100, &encode_frame(&finish), chunk);
        let sig = match session.session_keys() {
            Some(k) => format!("{}:{}", k.pin_code(), k.auth_string().len()),
            None => {
                ok = false;
                "not-established".to_string()
            }
        };
        signatures.push(sig);
    }
    let fragmentation_consistent = signatures.windows(2).all(|w| w[0] == w[1]);
    if !fragmentation_consistent {
        ok = false;
    }

    // ---- 负向：规范 Alert 码 ----
    let mut negatives: Vec<(String, String)> = Vec::new();

    // commitment 不符
    let mut srv_a = match Ukey2Server::new(HandshakeConfig::nearby_default()) {
        Ok(s) => s,
        Err(_) => return (failed_report(), false),
    };
    let mut cli_a = match Ukey2Client::with_fixed_entropy(
        ClientConfig::nearby_default(NEXT_PROTOCOL),
        [0x01u8; 32],
        &[0x44u8; 32],
    ) {
        Ok(c) => c,
        Err(_) => return (failed_report(), false),
    };
    if let Ok(init_a) = cli_a.start() {
        if let Ok(sinit_a) = srv_a.handle_client_init(&init_a, 0) {
            if let Ok((finish_a, _)) = cli_a.handle_server_init(&sinit_a) {
                let tampered = wire::tamper_client_finished_public_key(&finish_a, &[0xAB; 32]);
                negatives.push((
                    "commitment 不符（ClientFinish 被改）".to_string(),
                    match srv_a.handle_client_finish(&tampered, 1) {
                        Ok(()) => {
                            ok = false;
                            "accepted(unexpected)".to_string()
                        }
                        Err(a) => format!("{:?}", a.alert_type),
                    },
                ));
            }
        }
    }

    // 逐项字段非法（都用同一个合法 ClientInit 重建）
    let mut cli_b = match Ukey2Client::with_fixed_entropy(
        ClientConfig::nearby_default(NEXT_PROTOCOL),
        [0x02u8; 32],
        &[0x55u8; 32],
    ) {
        Ok(c) => c,
        Err(_) => return (failed_report(), false),
    };
    let init_b = match cli_b.start() {
        Ok(v) => v,
        Err(_) => return (failed_report(), false),
    };
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "random 31 字节",
            wire::rebuild_client_init(&init_b, Some(vec![0u8; 31]), None, None, None),
        ),
        (
            "version=2",
            wire::rebuild_client_init(&init_b, None, Some(2), None, None),
        ),
        (
            "cipher_commitments 为空",
            wire::rebuild_client_init(&init_b, None, None, Some(vec![]), None),
        ),
        (
            "cipher=CURVE25519（本仓未实现）",
            wire::rebuild_client_init(&init_b, None, None, Some(vec![(200, vec![0xAA; 64])]), None),
        ),
        (
            "next_protocol 不匹配",
            wire::rebuild_client_init(&init_b, None, None, None, Some("nope".into())),
        ),
        (
            "外层类型不符",
            wire::with_message_type(&init_b, Ukey2MessageType::ServerInit),
        ),
    ];
    for (label, bytes) in cases {
        let mut srv = match Ukey2Server::new(HandshakeConfig::nearby_default()) {
            Ok(s) => s,
            Err(_) => return (failed_report(), false),
        };
        let code = match srv.handle_client_init(&bytes, 0) {
            Ok(_) => {
                ok = false;
                "accepted(unexpected)".to_string()
            }
            Err(a) => format!("{:?}", a.alert_type),
        };
        negatives.push((label.to_string(), code));
    }

    // framing 负向：超长 / 零长
    let mut decoder = proto_quickshare::framing::FrameDecoder::new(DEFAULT_MAX_FRAME_BYTES);
    let mut huge = (DEFAULT_MAX_FRAME_BYTES + 1).to_be_bytes().to_vec();
    huge.extend_from_slice(&[0u8; 4]);
    negatives.push((
        "帧长度超过本仓上限".to_string(),
        match decoder.push(&huge) {
            Ok(_) => {
                ok = false;
                "accepted(unexpected)".to_string()
            }
            Err(e) => format!("{:?}", e.code),
        },
    ));
    let mut decoder = proto_quickshare::framing::FrameDecoder::new(DEFAULT_MAX_FRAME_BYTES);
    negatives.push((
        "帧长度 0".to_string(),
        match decoder.push(&0u32.to_be_bytes()) {
            Ok(_) => {
                ok = false;
                "accepted(unexpected)".to_string()
            }
            Err(e) => format!("{:?}", e.code),
        },
    ));

    // ---- payload gate：未核对确认码前拒绝；核对后放行 ----
    let mut session = QuickShareSession::with_server(server);
    session.note_discovery(DiscoveryProvenance::Qr);
    let gate_before = match session.deliver_payload_bytes(1024) {
        Ok(()) => {
            ok = false;
            "open(unexpected)".to_string()
        }
        Err(GateRefusal::NotEstablished) => "not-established".to_string(),
        Err(GateRefusal::ConfirmationRequired) => "confirmation-required".to_string(),
    };
    // 用真实确认码打开 gate
    let wrong = if pin == "0000" { "0001".to_string() } else { "0000".to_string() };
    let wrong_code = match session.confirm_code(&wrong) {
        Ok(()) => {
            ok = false;
            "accepted(unexpected)".to_string()
        }
        Err(e) => format!("{:?}", e.code),
    };
    let gate_after = match session.confirm_code(&pin) {
        Ok(()) => match session.deliver_payload_bytes(1024) {
            Ok(()) => "open".to_string(),
            Err(r) => {
                ok = false;
                format!("refused({r:?})")
            }
        },
        Err(e) => {
            ok = false;
            format!("confirm-failed({:?})", e.code)
        }
    };

    // ---- 传输链路（T21）：SecureMessage 加解密 + payload 流式落盘 ----
    let transport = run_transport_chain(&mut ok);

    // ---- 控制帧（T21+）：keep-alive 节奏与 paired-key 交换 ----
    let control = run_control_chain(&mut ok);

    let report = QuickShareReport {
        evidence_level: "simulated",
        wire: "self-authored-fixture",
        framing: serde_json::json!({
            "length_prefix_bytes": LENGTH_PREFIX_BYTES,
            "byte_order": "big-endian",
            "default_max_frame_bytes": DEFAULT_MAX_FRAME_BYTES,
            "max_frame_bytes_note": "来源 R15 SANE_FRAME_LENGTH；本仓保守上限，非协议常量（F-05）",
        }),
        handshake: serde_json::json!({
            "cipher": "P256_SHA512(100)",
            "key_schedule": KEY_SCHEDULE_HKDF_SHA256,
            "client_init_bytes": init.len(),
            "server_init_bytes": server_init.len(),
            "client_finish_bytes": client_finish.len(),
            "established": established,
            "auth_string_bytes": auth_len,
            "next_secret_bytes": next_len,
            "auth_strings_match": auth_matches,
            "next_secrets_match": next_matches,
            "pin": pin,
            "pin_matches": pin_matches,
            "transcript_bytes": transcript_len,
        }),
        fragmentation: serde_json::json!({
            "modes": [1048576usize, 1, 7],
            "signatures": signatures,
            "consistent": fragmentation_consistent,
        }),
        negatives,
        transport,
        control,
        payload_gate: serde_json::json!({
            "before_confirmation": gate_before,
            "wrong_code": wrong_code,
            "after_confirmation": gate_after,
            "payloads_delivered": session.payloads_delivered(),
        }),
        conflict: serde_json::json!({
            "id": "F-15",
            "claim": "规范文本说 HKDF 用握手 cipher 的哈希（SHA-512）；R18 实现与 R15 都是 HKDF-SHA256",
            "adopted": KEY_SCHEDULE_HKDF_SHA256,
            "test": "crypto::auth_string_spec_variant() 与实现派生结果必须不同（crates/proto-quickshare 测试已固化）",
            "awaiting": "与 stock Android 的成功握手 + PIN 比对（P-F02-2 关闭条件）",
        }),
        blocked: vec![
            "LAN 发现（mDNS `_FC9F5ED42C8A._tcp.`/BLE 触发）：P-F02-1 需用户抓包 → 不实现".to_string(),
            "QR/可见性隐藏实例（AES-GCM 名称加密、TLV）：P-F02-1/3 未关闭 → 不实现".to_string(),
            "paired-key 材料的**内容语义**（signed_data/secret_id_hash 怎么算）：不可离线推导，需跟 Google 服务器对话 → 只做帧与状态机，材料由调用方给".to_string(),
            "配对存储（免 4 位确认码）：本仓没有，默认策略一律要求用户核对（F-32）".to_string(),
            "4 位确认码与 stock Android 的一致性：需真机比对（当前是 R15 实现的兼容启发式）".to_string(),
            "F-12 cipher 选择规则冲突（规范概览 vs 实现）：以实现侧为准，待真机裁决".to_string(),
        ],
    };
    (report, ok)
}

fn feed_chunked(
    session: &mut QuickShareSession,
    now_ms: u64,
    bytes: &[u8],
    chunk: usize,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let n = chunk.min(bytes.len() - idx).max(1);
        match session.feed(now_ms, &bytes[idx..idx + n]) {
            Ok(ev) => events.extend(ev),
            Err(_) => break,
        }
        idx += n;
    }
    events
}

fn failed_report() -> QuickShareReport {
    QuickShareReport {
        evidence_level: "simulated",
        wire: "self-authored-fixture",
        framing: serde_json::Value::Null,
        handshake: serde_json::Value::Null,
        fragmentation: serde_json::Value::Null,
        negatives: vec![],
        transport: serde_json::Value::Null,
        payload_gate: serde_json::Value::Null,
        control: serde_json::Value::Null,
        conflict: serde_json::Value::Null,
        blocked: vec!["Quick Share 场景提前失败（熵源或状态机异常）".to_string()],
    }
}

/// 跑一遍"握手 → D2D 密钥 → SecureMessage → payload 分块 → 接收会话落盘"的完整链路。
///
/// 全在本进程内：客户端用真实 SecureMessage 把自己造的 48 KiB 文件分 3 块发出，
/// 服务端解密后交给 `ReceiveSession` 流式落盘并原子发布；同时报告篡改拒绝与绑定的负向结果。
fn run_transport_chain(ok: &mut bool) -> serde_json::Value {
    // 1) 真实握手（固定材料，便于复现；生产路径用系统熵）
    let mut cli = match Ukey2Client::with_fixed_entropy(
        ClientConfig::nearby_default(NEXT_PROTOCOL),
        [0x21u8; 32],
        &[0x51u8; 32],
    ) {
        Ok(c) => c,
        Err(_) => return serde_json::Value::Null,
    };
    let mut srv = match Ukey2Server::with_fixed_entropy(
        HandshakeConfig::nearby_default(),
        [0x22u8; 32],
        &[0x52u8; 32],
    ) {
        Ok(s) => s,
        Err(_) => return serde_json::Value::Null,
    };
    let Ok(init) = cli.start() else {
        return serde_json::Value::Null;
    };
    let Ok(sinit) = srv.handle_client_init(&init, 0) else {
        return serde_json::Value::Null;
    };
    let Ok((finish, cli_keys)) = cli.handle_server_init(&sinit) else {
        return serde_json::Value::Null;
    };
    if srv.handle_client_finish(&finish, 1).is_err() {
        *ok = false;
    }
    let Some(srv_keys) = srv.session_keys() else {
        return serde_json::Value::Null;
    };

    let (Ok(cli_sched), Ok(srv_sched)) = (
        D2DKeySchedule::derive(cli_keys.next_protocol_secret()),
        D2DKeySchedule::derive(srv_keys.next_protocol_secret()),
    ) else {
        *ok = false;
        return serde_json::Value::Null;
    };
    // 篡改用例用**独立的通道对**：被拒帧不会推进接收方序号，若与主通道混用，
    // 之后的所有帧都会因序号不符被拒（真实协议里这意味着连接必须断开，而不是跳过）。
    let mut tamper_sender = SecureMessageChannel::new(cli_sched.clone(), ChannelRole::Client);
    let mut tamper_receiver = SecureMessageChannel::new(srv_sched.clone(), ChannelRole::Server);
    let mut sender = SecureMessageChannel::new(cli_sched, ChannelRole::Client);
    let mut receiver = SecureMessageChannel::new(srv_sched, ChannelRole::Server);

    // 2) SecureMessage 往返 + 篡改拒绝
    let sealed_ok = match sender.seal(b"introduction") {
        Ok(f) => receiver.open(&f).is_ok(),
        Err(_) => false,
    };
    let tamper_rejected = match tamper_sender.seal(b"tamper-me") {
        Ok(f) => {
            let mut bad = f.clone();
            let last = bad.len() - 1;
            bad[last] ^= 0x01;
            tamper_receiver.open(&bad).is_err()
        }
        Err(_) => false,
    };
    let divergence_note = "被拒的完整性失败会使双方序号发散：上线实现必须断开连接（不静默重同步）";
    if !(sealed_ok && tamper_rejected) {
        *ok = false;
    }

    // 3) payload 分块 → 接收会话落盘
    let root = std::env::temp_dir().join(format!("xinterop-demo-qs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let content: Vec<u8> = (0..48 * 1024).map(|i| (i % 251) as u8).collect();
    let chunk = 16 * 1024usize;
    let published;
    let binding_rejected;
    match ReceiveSession::open(&root, 1 << 20) {
        Ok(mut session) => {
            let offer = FileOffer::new("demo-transfer.bin", content.len() as u64, None)
                .with_payload_id(7)
                .with_mime_hint(Some("application/octet-stream".to_string()));
            let status = session
                .on_introduction(&[offer], ReceiveDecision::Accept)
                .map(|s| s.as_wire().to_string())
                .unwrap_or_else(|_| "error".to_string());
            let accepted = status == "accept";
            let entry_id = session
                .entries()
                .first()
                .map(|e| e.entry_id.clone())
                .unwrap_or_default();

            // 每个 chunk 都经真实 SecureMessage 发送/接收，再交给 payload 组装器
            for (i, part) in content.chunks(chunk).enumerate() {
                let offset = (i * chunk) as u64;
                let last = offset + part.len() as u64 == content.len() as u64;
                let frame = PayloadTransferFrame::data(7, content.len() as u64, offset, last, part.to_vec());
                let bytes = match proto_quickshare::payload::encode_payload_transfer_frame(&frame) {
                    Ok(b) => b,
                    Err(_) => {
                        *ok = false;
                        break;
                    }
                };
                let sealed = match sender.seal(&bytes) {
                    Ok(s) => s,
                    Err(_) => {
                        *ok = false;
                        break;
                    }
                };
                let plain = match receiver.open(&sealed) {
                    Ok(p) => p,
                    Err(_) => {
                        *ok = false;
                        break;
                    }
                };
                match proto_quickshare::payload::decode_payload_transfer_frame(&plain)
                    .and_then(|f| session.on_payload_frame(&f))
                {
                    Ok(()) => {}
                    Err(_) => {
                        *ok = false;
                        break;
                    }
                }
            }

            // 混入另一个 payload id：必须被拒（T21-01）
            let foreign = PayloadTransferFrame::data(99, 4, 0, true, vec![0u8; 4]);
            binding_rejected = session.on_payload_frame(&foreign).is_err();

            published = match session.finish_entry(&entry_id) {
                Ok(view) => serde_json::json!({
                    "status": status,
                    "accepted": accepted,
                    "relative_path": view.relative_path,
                    "bytes": view.bytes,
                    "sha256": view.sha256_hex,
                    "expected_sha256": hex_sha256(&content),
                    "hash_matches_content": view.sha256_hex == hex_sha256(&content),
                    "published_count": session.published_count(),
                }),
                Err(e) => {
                    *ok = false;
                    serde_json::json!({
                        "status": status,
                        "accepted": accepted,
                        "error_code": format!("{:?}", e.code),
                        "error": e.message,
                        "published_count": session.published_count(),
                    })
                }
            };
            if !binding_rejected || !accepted {
                *ok = false;
            }
        }
        Err(_) => {
            *ok = false;
            return serde_json::Value::Null;
        }
    }
    let _ = std::fs::remove_dir_all(&root);

    serde_json::json!({
        "secure_message": {
            "roundtrip": sealed_ok,
            "tamper_rejected": tamper_rejected,
            "sequence": "严格 +1（重放/跳号拒绝）",
            "rejected_frame_divergence": divergence_note,
        },
        "payload": {
            "chunks": content.len().div_ceil(chunk),
            "chunk_bytes": chunk,
            "foreign_payload_id_rejected": binding_rejected,
        },
        "published": published,
    })
}

/// 与 `interop-file` 相同的 SHA-256 十六进制（只用于展示对照）。
fn hex_sha256(bytes: &[u8]) -> String {
    interop_file::integrity::sha256_hex(bytes)
}

/// 跑一遍控制帧链路（T21+，字段行 F-30..F-33）：
/// 1) keep-alive：两端 tracker 按 10 s 推进 65 s 时钟，帧**编成字节再解**（证明线上格式），
///    记录到点节奏、ack 往返；随后让对端静默，验证 30 s 判死并停发（qs-020）。
/// 2) paired-key：内层帧装进 BYTES payload 走一遍双向交换，重点报告**默认策略不免确认码**。
///
/// 全在本进程内、不连任何设备：证明的是"我们的帧与状态机自洽"，不是与 Android 互通。
fn run_control_chain(ok: &mut bool) -> serde_json::Value {
    // ---- keep-alive：双向 65 秒 ----
    let mut side_a = KeepAliveTracker::new();
    let mut side_b = KeepAliveTracker::new();
    let mut gaps: Vec<u64> = Vec::new();
    let mut sent_by_a = 0u32;
    let mut acks_seen_by_a = 0u32;
    let mut last_send_at: Option<u64> = None;
    let mut layer_ok = true;

    for second in 0..=65u64 {
        let now = second * 1_000;
        if let Some(frame) = side_a.on_tick(now) {
            sent_by_a += 1;
            if let Some(prev) = last_send_at {
                gaps.push(now - prev);
            }
            last_send_at = Some(now);

            // 走一遍真实字节（外层帧编解码）再交给 B。
            let wire_bytes = frame.encode_offline();
            let decoded = match decode_keepalive_offline(&wire_bytes) {
                Ok(d) => d,
                Err(_) => {
                    layer_ok = false;
                    break;
                }
            };
            match side_b.on_received(&decoded, now) {
                Ok(Some(ack)) => {
                    let ack_bytes = ack.encode_offline();
                    match decode_keepalive_offline(&ack_bytes) {
                        Ok(ack) => {
                            if side_a.on_received(&ack, now).map(|r| r.is_none()) == Ok(true) {
                                acks_seen_by_a += 1;
                            } else {
                                *ok = false;
                            }
                        }
                        Err(_) => layer_ok = false,
                    }
                }
                // B 判死后不再回应：这是**预期**行为（qs-020 的下半段），不算失败。
                Err(_) => {}
                Ok(None) => {}
            }
        }
        side_a.expired(now);
        side_b.expired(now);
    }

    if !layer_ok || acks_seen_by_a != sent_by_a {
        *ok = false;
    }
    let cadence_ok = !gaps.is_empty() && gaps.iter().all(|g| *g == KEEPALIVE_INTERVAL_MS);

    // ---- 对端静默 → 30 s 判死（本次用独立的一对，避免与上面的 65 s 时钟纠缠）----
    let mut lonely = KeepAliveTracker::new();
    let mut heartbeat = 0u32;
    let mut died_at: Option<u64> = None;
    for second in 0..=40u64 {
        let now = second * 1_000;
        if lonely.on_tick(now).is_some() {
            heartbeat += 1;
        }
        if lonely.expired(now) && died_at.is_none() {
            died_at = Some(now);
        }
    }
    let silent_peer_detected = died_at.is_some() && lonely.stats().received == 0;
    if !silent_peer_detected {
        *ok = false;
    }

    // ---- paired-key：双向交换（材料为演示用固定字节，不是任何真实配对材料）----
    let demo_material = PairedKeyMaterial::new(vec![0x11; 72], vec![0x22; 6]);
    let Ok(demo_material) = demo_material else {
        return serde_json::Value::Null;
    };

    // 我们发起：内层帧 → BYTES payload → **经真实字节回到对端解析**
    let mut ours = PairedKeyExchange::new();
    let our_payload = match ours.begin(&demo_material) {
        Ok(p) => p,
        Err(_) => return serde_json::Value::Null,
    };
    let (inner_bytes, payload_id) = match unwrap_bytes_payload(&our_payload) {
        Ok(v) => v,
        Err(_) => return serde_json::Value::Null,
    };
    let inner_type_ok = offline_type_is(&inner_bytes, INNER_TYPE_PAIRED_KEY_ENCRYPTION);
    let peer_sees_material = PairedKeyMaterial::decode_inner(&inner_bytes).is_ok();
    if !(inner_type_ok && peer_sees_material) {
        *ok = false;
    }

    // 对端收到后回一条 result：参考实现回 UNABLE（F-33）→ 我们仍然要求用户核对确认码。
    let peer_result = PairedKeyResultFrame::unable();
    let peer_payload = match wrap_inner_as_bytes_payload(&peer_result.encode_inner(), payload_id) {
        Ok(p) => p,
        Err(_) => return serde_json::Value::Null,
    };
    let default_decision = match ours.on_bytes_payload(&peer_payload) {
        Ok(PairedKeyEvent::Result { status, decision }) => {
            if status != PairedKeyStatus::Unable {
                *ok = false;
            }
            decision
        }
        _ => {
            *ok = false;
            PairedKeyDecision::RequireConfirmation
        }
    };

    // 对端报 SUCCESS：默认策略**仍然**要求确认码；只有调用方显式开启开关才允许跳过。
    let mut exchange = PairedKeyExchange::new();
    let _ = exchange.begin(&demo_material);
    let success_payload = match wrap_inner_as_bytes_payload(
        &PairedKeyResultFrame {
            status: PairedKeyStatus::Success,
            os_type: Some(1),
        }
        .encode_inner(),
        4242,
    ) {
        Ok(p) => p,
        Err(_) => return serde_json::Value::Null,
    };
    let success_decision_default = match exchange.on_bytes_payload(&success_payload) {
        Ok(PairedKeyEvent::Result { decision, .. }) => decision,
        _ => PairedKeyDecision::RequireConfirmation,
    };
    let mut opted_in = PairedKeyExchange::new().allowing_skip_confirmation();
    let _ = opted_in.begin(&demo_material);
    let success_decision_opted_in = match opted_in.on_bytes_payload(&success_payload) {
        Ok(PairedKeyEvent::Result { decision, .. }) => decision,
        _ => PairedKeyDecision::RequireConfirmation,
    };
    if default_decision != PairedKeyDecision::RequireConfirmation
        || success_decision_default != PairedKeyDecision::RequireConfirmation
        || success_decision_opted_in != PairedKeyDecision::SkipConfirmation
    {
        *ok = false;
    }

    // 对端发来 encryption 帧 → 我们回的必须是 UNABLE（F-33：本仓没有可用的配对材料）
    let mut responder = PairedKeyExchange::new();
    let _ = responder.begin(&demo_material);
    let peer_encryption = match wrap_inner_as_bytes_payload(&demo_material.encode_inner(), 5150) {
        Ok(p) => p,
        Err(_) => return serde_json::Value::Null,
    };
    let reply_status = match responder.on_bytes_payload(&peer_encryption) {
        Ok(PairedKeyEvent::NeedResult { reply }) => match unwrap_bytes_payload(&reply) {
            Ok((bytes, 5150)) => PairedKeyResultFrame::decode_inner(&bytes)
                .map(|f| f.status)
                .ok(),
            _ => None,
        },
        _ => None,
    };
    if reply_status != Some(PairedKeyStatus::Unable) {
        *ok = false;
    }

    // ---- DisconnectionFrame（F-34..F-36）：四种字节形态 + 三路决策 ----
    let shapes = [
        ("R17 (false,false)", DisconnectionFrame::new(Some(false), Some(false))),
        ("NearDrop 空正文", DisconnectionFrame::empty()),
        ("发起 (true,false)", DisconnectionFrame::new(Some(true), Some(false))),
        ("应答 (true,true)", DisconnectionFrame::new(Some(true), Some(true))),
    ];
    let mut disconnection_rows: Vec<serde_json::Value> = Vec::new();
    let mut presence_preserved = true;
    let mut actions = Vec::new();
    for (label, frame) in shapes {
        let bytes = frame.encode_offline();
        let decoded = match DisconnectionFrame::decode_offline(&bytes) {
            Ok(d) => d,
            Err(_) => {
                *ok = false;
                continue;
            }
        };
        if decoded != frame {
            *ok = false;
        }
        // 存在性：空正文解出两个 None，显式 false 解出 Some(false)。
        if (label == "NearDrop 空正文") != (!decoded.has_request() && !decoded.has_ack()) {
            presence_preserved = false;
        }
        disconnection_rows.push(json!({
            "shape": label,
            "request": decoded.request_safe_to_disconnect,
            "ack": decoded.ack_safe_to_disconnect,
            "roundtrip": decoded == frame,
        }));
        match frame.decision() {
            DisconnectAction::CloseNow => actions.push(format!("{label} → 立即关闭")),
            DisconnectAction::MarkedAndNotified => actions.push(format!("{label} → 标记+通知（不回帧）")),
            DisconnectAction::MarkedAndReply(reply) => actions.push(format!(
                "{label} → 标记+回帧({:?},{:?})",
                reply.request_safe_to_disconnect, reply.ack_safe_to_disconnect
            )),
        }
    }
    let empty_vs_false_differ = DisconnectionFrame::empty().encode_offline()
        != DisconnectionFrame::new(Some(false), Some(false)).encode_offline();
    if !presence_preserved || !empty_vs_false_differ || actions.len() != 4 {
        *ok = false;
    }

    // ---- PAYLOAD_ACK（F-37..F-39）：形状、门槛、三分支 ----
    let ack_bytes = match encode_payload_ack(4242) {
        Ok(b) => b,
        Err(_) => return serde_json::Value::Null,
    };
    let ack_id = decode_payload_ack(&ack_bytes).unwrap_or(0);
    let ack_is_ack = match proto_quickshare::payload::decode_payload_transfer_frame(&ack_bytes) {
        Ok(f) => classify_payload_frame(&f) == Ok(PayloadFrameClass::Ack),
        Err(_) => false,
    };
    let ack_total_size = proto_quickshare::payload::decode_payload_transfer_frame(&ack_bytes)
        .ok()
        .and_then(|f| f.header.map(|h| h.total_size));
    let mut tracker = PayloadAckTracker::new();
    tracker.register_outgoing(11);
    tracker.register_incoming(12);
    let branch_marked = tracker.on_ack(11) == AckOutcome::Marked;
    let branch_unknown = tracker.on_ack(99) == AckOutcome::IgnoredUnknownPayload;
    let branch_incoming = tracker.on_ack(12) == AckOutcome::IgnoredIncomingPayload;
    let control_refused = match classify_payload_frame(&PayloadTransferFrame::control(PacketType::Control)) {
        Ok(_) => {
            *ok = false;
            "accepted(unexpected)".to_string()
        }
        Err(e) => format!("{:?}", e.code),
    };
    let ack_gate = json!({
        "bytes_sent_ack": should_send_payload_ack(PayloadKind::Bytes, true),
        "file_last_chunk_ack": should_send_payload_ack(PayloadKind::File, true),
        "file_mid_chunk_ack": should_send_payload_ack(PayloadKind::File, false),
    });
    if !(ack_id == 4242
        && ack_is_ack
        && ack_total_size == Some(u64::MAX)
        && branch_marked
        && branch_unknown
        && branch_incoming
        && control_refused == "UnsupportedFeature")
    {
        *ok = false;
    }

    // ---- keep-alive 协商字段（F-40）：合法值采纳、非法拒绝、缺席回退 ----
    let policy_default = KeepAlivePolicy::from_negotiation(None, None);
    let policy_negotiated = KeepAlivePolicy::from_negotiation(Some(15_000), Some(45_000));
    let policy_bad = KeepAlivePolicy::from_negotiation(Some(10_000), Some(5_000))
        .map(|_| "accepted(unexpected)".to_string())
        .unwrap_or_else(|e| format!("{:?}", e.code));
    let policy_source = match (&policy_default, &policy_negotiated) {
        (Ok(d), Ok(n))
            if d.source == KeepAliveSource::RepoPolicy
                && n.source == KeepAliveSource::Negotiated =>
        {
            json!({"default": "repo-policy", "negotiated": "negotiated"})
        }
        _ => {
            *ok = false;
            serde_json::Value::Null
        }
    };
    if policy_bad != "InvalidFrame" {
        *ok = false;
    }
    let negotiated = policy_negotiated
        .as_ref()
        .map(|p| (p.interval_ms, p.timeout_ms))
        .unwrap_or((0, 0));

    // 负向：层号混用（外层帧用内层编号 3）与 FILE 载荷冒充协商帧。
    let mut inner_numbered = Vec::new();
    let mut v1 = Vec::new();
    v1.extend_from_slice(&[0x08, INNER_TYPE_PAIRED_KEY_ENCRYPTION as u8]);
    v1.extend_from_slice(&[0x32, 0x02, 0x08, 0x01]);
    inner_numbered.extend_from_slice(&[0x08, 0x01]);
    inner_numbered.push(0x12);
    inner_numbered.push(v1.len() as u8);
    inner_numbered.extend_from_slice(&v1);
    let layer_confusion = match decode_keepalive_offline(&inner_numbered) {
        Ok(_) => {
            *ok = false;
            "accepted(unexpected)".to_string()
        }
        Err(e) => format!("{:?}", e.code),
    };
    let file_frame = PayloadTransferFrame::data(7, 4, 0, true, vec![0u8; 4]);
    let file_carrier = match proto_quickshare::payload::encode_payload_transfer_frame(&file_frame) {
        Ok(bytes) => match unwrap_bytes_payload(&bytes) {
            Ok(_) => {
                *ok = false;
                "accepted(unexpected)".to_string()
            }
            Err(e) => format!("{:?}", e.code),
        },
        Err(_) => "encode-failed".to_string(),
    };

    serde_json::json!({
        "keepalive": {
            "interval_ms": KEEPALIVE_INTERVAL_MS,
            "interval_source": "来源取值（PROTOCOL.md:228-230 的 10 秒）",
            "timeout_ms": KEEPALIVE_TIMEOUT_MS,
            "timeout_source": "本仓策略（来源只说 'a while'）",
            "sent": sent_by_a,
            "acks_returned": acks_seen_by_a,
            "gaps_ms": gaps,
            "cadence_ok": cadence_ok,
            "wire_roundtrip_ok": layer_ok,
            "silent_peer_detected_ms": died_at,
            "silent_peer_heartbeats_sent": heartbeat,
        },
        "paired_key": {
            "inner_type": "PAIRED_KEY_ENCRYPTION(3)",
            "inner_type_ok": inner_type_ok,
            "payload_id_preserved": payload_id != 0,
            "material_roundtrip": peer_sees_material,
            "material_source": "演示用固定字节；真实材料不可离线推导（F-32）",
            "our_result_status": format!("{reply_status:?}"),
            "peer_unable_decision": format!("{default_decision:?}"),
            "peer_success_decision_default": format!("{success_decision_default:?}"),
            "peer_success_decision_opted_in": format!("{success_decision_opted_in:?}"),
            "skips_confirmation_by_default": false,
        },
        "disconnection": {
            "frames": disconnection_rows,
            "actions": actions,
            "presence_preserved": presence_preserved,
            "empty_differs_from_explicit_false": empty_vs_false_differ,
            "note": "R17 显式写两个 bool，NearDrop 发空正文——两者字节不同（F-36）",
        },
        "payload_ack": {
            "ack_id": ack_id,
            "total_size_is_indeterminate": ack_total_size == Some(u64::MAX),
            "classified_as_ack": ack_is_ack,
            "gate": ack_gate,
            "branch_marked": branch_marked,
            "branch_unknown": branch_unknown,
            "branch_incoming": branch_incoming,
            "control_path_refused": control_refused,
        },
        "keepalive_negotiation": {
            "source": policy_source,
            "default_interval_ms": KEEPALIVE_INTERVAL_MS,
            "default_timeout_ms": KEEPALIVE_TIMEOUT_MS,
            "negotiated_interval_ms": negotiated.0,
            "negotiated_timeout_ms": negotiated.1,
            "invalid_pair_refused": policy_bad,
        },
        "rejects": [
            {"case": "外层帧用内层编号 3", "outcome": layer_confusion},
            {"case": "FILE 载荷冒充协商帧", "outcome": file_carrier},
            {"case": "CONTROL 包类型（已废弃路径）", "outcome": control_refused},
            {"case": "协商 timeout < interval", "outcome": policy_bad},
        ],
    })
}

/// 从内层 `OfflineFrame` 取 `V1Frame.type`（演示用；长度前缀是真 varint）。
fn offline_type_is(buf: &[u8], expected: i64) -> bool {
    fn read_varint(buf: &[u8]) -> Option<(u64, usize)> {
        let mut value = 0u64;
        let mut shift = 0;
        for (i, byte) in buf.iter().enumerate() {
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some((value, i + 1));
            }
            shift += 7;
        }
        None
    }
    let Some((len, n)) = read_varint(buf.get(3..).unwrap_or_default()) else {
        return false;
    };
    let Some(v1) = buf.get(3 + n..3 + n + len as usize) else {
        return false;
    };
    matches!(read_varint(v1.get(1..).unwrap_or_default()), Some((ty, _)) if ty == expected as u64)
}
