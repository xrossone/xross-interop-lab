//! T21 验收（plans/02-files.md §T21，headless 部分）：传输闭环与 payload 层。
//!
//! - T21-01 两个 payloadID 混入同一条目 → 拒绝且不发布；
//! - T21-02 用户拒绝 → 对端收到 REJECT、无最终文件；
//! - T21-03 10 GiB 合成传输（配额允许）→ 常数级内存流式；
//! - T21-04 发送方伪造 filename 穿越 → FILE-05 规则拒绝。
//!
//! 另加 SecureMessage 层（F-19/F-20）的正负向：密钥链、序号严格递增、篡改与重放拒绝。
//! 字段号来源见 `specs-reviewed/f02-quickshare-lan.md` 的 F-19/F-20/F-22/F-25..F-28。
//! **不发现在本文档覆盖范围**（P-F02-1/3 未关闭）：本文件不测任何发现行为。

use interop_contract::error::ErrorCode;
use proto_quickshare::handshake::{
    ClientConfig, HandshakeConfig, Ukey2Client, Ukey2Server,
};
use proto_quickshare::payload::{
    ChunkSink, PacketType, PayloadAssembler, PayloadHeader, PayloadKind, PayloadTransferFrame,
};
use proto_quickshare::receive::{
    FileOffer, ReceiveDecision, ReceiveSession, ResponseStatus,
};
use proto_quickshare::secure_message::{ChannelRole, D2DKeySchedule, SecureMessageChannel};

const NEXT_PROTOCOL: &str = "AES_256_CBC-HMAC_SHA256";

/// 跑一次真实 UKEY2 握手，返回（客户端 next secret, 服务端 next secret）。
fn handshake_next_secrets() -> (Vec<u8>, Vec<u8>) {
    let mut cli = Ukey2Client::with_fixed_entropy(
        ClientConfig::nearby_default(NEXT_PROTOCOL),
        [0x31u8; 32],
        &[0x41u8; 32],
    )
    .expect("client");
    let mut srv = Ukey2Server::with_fixed_entropy(
        HandshakeConfig::nearby_default(),
        [0x32u8; 32],
        &[0x42u8; 32],
    )
    .expect("server");
    let init = cli.start().expect("init");
    let sinit = srv.handle_client_init(&init, 0).expect("sinit");
    let (finish, cli_keys) = cli.handle_server_init(&sinit).expect("finish");
    srv.handle_client_finish(&finish, 1).expect("establish");
    (
        cli_keys.next_protocol_secret().to_vec(),
        srv.session_keys().expect("keys").next_protocol_secret().to_vec(),
    )
}

fn channels() -> (SecureMessageChannel, SecureMessageChannel) {
    let (cli_secret, srv_secret) = handshake_next_secrets();
    let cli_schedule = D2DKeySchedule::derive(&cli_secret).expect("schedule");
    let srv_schedule = D2DKeySchedule::derive(&srv_secret).expect("schedule");
    (
        SecureMessageChannel::new(cli_schedule, ChannelRole::Client),
        SecureMessageChannel::new(srv_schedule, ChannelRole::Server),
    )
}

/// SecureMessage：双向加解密 + 序号严格递增（F-19/F-20）。
#[test]
fn secure_message_roundtrip_and_monotonic_sequence() {
    let (mut cli, mut srv) = channels();
    let plaintext = b"introduction-frame-bytes".to_vec();

    let sealed = cli.seal(&plaintext).expect("seal");
    let opened = srv.open(&sealed).expect("open");
    assert_eq!(opened, plaintext, "服务端必须解出原文");
    assert_eq!(srv.expected_sequence(), 2, "首条序号为 1，收完后期望 2");

    // 服务端回一条（独立序号）
    let back = srv.seal(b"response").expect("seal back");
    assert_eq!(cli.open(&back).expect("open back"), b"response");
    assert_eq!(cli.expected_sequence(), 2, "客户端序号独立计数");

    // 第二条
    let second = cli.seal(b"second").expect("seal 2");
    assert_eq!(srv.open(&second).expect("open 2"), b"second");
    assert_eq!(srv.expected_sequence(), 3);
}

/// SecureMessage：篡改（HMAC 不符 / 密文改一位）必须 `integrity-failed`，且不得推进序号。
#[test]
fn secure_message_tamper_is_integrity_failed() {
    let (mut cli, mut srv) = channels();
    let sealed = cli.seal(b"payload").expect("seal");

    let mut tampered = sealed.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert_eq!(
        srv.open(&tampered).expect_err("篡改必须拒绝").code,
        ErrorCode::IntegrityFailed
    );
    assert_eq!(srv.expected_sequence(), 1, "被拒的帧不得推进序号");

    // 密文本体（body）改一位：HMAC 覆盖 header_and_body，同样必须被抓到
    let mut flipped = sealed.clone();
    let mid = flipped.len() / 2;
    flipped[mid] ^= 0x20;
    assert!(srv.open(&flipped).is_err(), "密文改动必须被拒");
}

/// SecureMessage：重放已接受过的帧 → 拒绝（序号必须严格递增）。
#[test]
fn secure_message_replay_is_rejected() {
    let (mut cli, mut srv) = channels();
    let first = cli.seal(b"one").expect("seal 1");
    let second = cli.seal(b"two").expect("seal 2");

    assert_eq!(srv.open(&second).expect_err("跳号必须拒绝").code, ErrorCode::IntegrityFailed,
        "顺序必须严格（不接受跳号或乱序）");
    assert_eq!(srv.open(&first).expect("顺序正确"), b"one");
    assert_eq!(
        srv.open(&first).expect_err("重放必须拒绝").code,
        ErrorCode::IntegrityFailed
    );
    assert_eq!(srv.open(&second).expect("第二条"), b"two");
}

/// T21-01：两个 payloadID 混入同一条目 → 拒绝且不发布。
#[test]
fn t21_01_two_payload_ids_never_land_in_one_entry() {
    let root = temp_root("t21-01");
    let mut session = ReceiveSession::open(&root, 1 << 20).expect("session");
    let offer = FileOffer::new("photo.jpg", 4, None)
        .with_payload_id(4)
        .with_mime_hint(Some("image/jpeg".to_string()));
    session
        .on_introduction(&[offer], ReceiveDecision::Accept)
        .expect("introduction");
    let entry_id = session
        .entries()
        .first()
        .map(|e| e.entry_id.clone())
        .expect("entry");
    assert!(
        session.has_pending_entry(&entry_id),
        "条目必须已建立（有写句柄）"
    );

    // 正确的 payload id = 4；这里先写 2 字节
    let ok = PayloadTransferFrame::data(4, 4, 0, false, b"AB".to_vec());
    session.on_payload_frame(&ok).expect("首个 chunk");

    // 混入另一个 payload id：必须拒绝（不写入、不发布）
    let foreign = PayloadTransferFrame::data(9, 4, 2, true, b"CD".to_vec());
    let err = session
        .on_payload_frame(&foreign)
        .expect_err("另一个 payloadID 必须拒绝");
    assert!(
        matches!(err.code, ErrorCode::InvalidFrame | ErrorCode::UnsupportedProfile),
        "错误码应为 invalid-frame/profile，实得 {:?}",
        err.code
    );
    assert_eq!(session.published_count(), 0, "不得发布任何文件");
    assert_eq!(session.entry_written(&entry_id), Some(2), "混入的 chunk 不得写入");

    cleanup(&root);
}

/// T21-02：用户拒绝 → 对端收到 REJECT，且本地无任何最终文件。
#[test]
fn t21_02_user_reject_means_no_file() {
    let root = temp_root("t21-02");
    let mut session = ReceiveSession::open(&root, 1 << 20).expect("session");
    let offer = FileOffer::new("report.pdf", 1024, None);
    let status = session
        .on_introduction(&[offer], ReceiveDecision::Reject)
        .expect("introduction");
    assert_eq!(status, ResponseStatus::Reject, "必须回 REJECT 状态");
    assert!(session.entries().is_empty(), "拒绝后不得建立条目/写句柄");
    assert_eq!(session.published_count(), 0);
    assert!(!root.join("report.pdf").exists(), "不得产生最终文件");

    // 拒绝之后任何 payload 帧都不得被接受
    let frame = PayloadTransferFrame::data(1, 1024, 0, true, vec![0u8; 8]);
    assert!(session.on_payload_frame(&frame).is_err(), "拒绝后不得接收数据");
    assert_eq!(session.published_count(), 0);
    cleanup(&root);
}

/// 计数 sink：证明 payload 层是常数内存（不累积、不重排）。
struct CountingSink {
    total: u64,
    max_chunk: usize,
    seen_offsets: Vec<u64>,
}

impl ChunkSink for CountingSink {
    fn write_chunk(&mut self, offset: u64, body: &[u8]) -> Result<(), interop_contract::error::Error> {
        self.seen_offsets.push(offset);
        self.total += body.len() as u64;
        self.max_chunk = self.max_chunk.max(body.len());
        Ok(())
    }
}

/// T21-03：10 GiB 合成传输（配额允许）→ 常数级内存流式，不累积缓冲。
#[test]
fn t21_03_ten_gib_streams_without_buffering() {
    const TOTAL: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB
    const CHUNK: usize = 1024 * 1024; // 1 MiB

    let mut assembler = PayloadAssembler::new(PayloadHeader {
        id: 7,
        kind: PayloadKind::File,
        total_size: TOTAL,
        file_name: Some("big.iso".to_string()),
        parent_folder: None,
    })
    .expect("assembler");
    let mut sink = CountingSink {
        total: 0,
        max_chunk: 0,
        seen_offsets: Vec::new(),
    };

    let chunk = vec![0xA5u8; CHUNK];
    let mut offset = 0u64;
    let chunks = TOTAL / CHUNK as u64;
    let mut frames = 0u64;
    while offset < TOTAL {
        let last = offset + CHUNK as u64 == TOTAL;
        let frame = PayloadTransferFrame::data(7, TOTAL, offset, last, chunk.clone());
        assembler.accept(&frame, &mut sink).expect("chunk");
        offset += CHUNK as u64;
        frames += 1;
    }
    assert_eq!(frames, chunks);
    assert_eq!(sink.total, TOTAL, "全部字节都流过了 sink");
    assert_eq!(sink.max_chunk, CHUNK, "单次交给 sink 的字节不超过一个 chunk（常数内存）");
    assert!(assembler.is_complete(), "LAST_CHUNK 之后应判定完成");
    assert_eq!(assembler.written(), TOTAL);

    // 第一条与最后一条 offset 连续性（顺序写，无重排）
    assert_eq!(sink.seen_offsets.first().copied(), Some(0));
    assert_eq!(sink.seen_offsets.last().copied(), Some(TOTAL - CHUNK as u64));

    // 完成后再来一帧：拒绝
    let extra = PayloadTransferFrame::data(7, TOTAL, TOTAL, true, vec![0u8; 4]);
    assert!(assembler.accept(&extra, &mut sink).is_err(), "完成后不得再接受");
}

/// T21-04：发送方伪造 filename 穿越 → FILE-05 规则拒绝（形状层，先于任何写操作）。
#[test]
fn t21_04_forged_filename_traversal_is_rejected() {
    for bad in [
        "../../etc/passwd",
        "..",
        "a/b",
        "C:evil.txt",
        "CON.txt",
        "nul\u{0}byte",
    ] {
        let root = temp_root("t21-04");
        let mut session = ReceiveSession::open(&root, 1 << 20).expect("session");
        let offer = FileOffer::new(bad, 8, None);
        let outcome = session.on_introduction(&[offer], ReceiveDecision::Accept);
        assert!(
            outcome.is_err(),
            "伪造名称 {bad:?} 必须被拒绝（FILE-05）"
        );
        assert_eq!(session.published_count(), 0);
        assert!(session.entries().is_empty(), "被拒名称不得建立条目");
        cleanup(&root);
    }

    // 合法名称仍然可用（避免把规则做成"什么都拒"）
    let root = temp_root("t21-04-ok");
    let mut session = ReceiveSession::open(&root, 1 << 20).expect("session");
    session
        .on_introduction(
            &[FileOffer::new("photo (1).jpg", 4, Some(vec!["Photos".to_string()]))],
            ReceiveDecision::Accept,
        )
        .expect("合法名称");
    let entry_id = session.entries()[0].entry_id.clone();
    session
        .on_payload_frame(&PayloadTransferFrame::data(1, 4, 0, true, b"DATA".to_vec()))
        .expect("chunk");
    let receipt = session.finish_entry(&entry_id).expect("commit");
    assert_eq!(receipt.relative_path, "Photos/photo (1).jpg");
    assert_eq!(session.published_count(), 1);
    cleanup(&root);
}

/// 配额：声明超过预算 → NOT_ENOUGH_SPACE（不建立写句柄、不落盘）。
#[test]
fn over_budget_offer_is_not_enough_space() {
    let root = temp_root("budget");
    let mut session = ReceiveSession::open(&root, 1024).expect("session");
    let status = session
        .on_introduction(&[FileOffer::new("huge.bin", 4096, None)], ReceiveDecision::Accept)
        .expect("introduction");
    assert_eq!(status, ResponseStatus::NotEnoughSpace);
    assert!(session.entries().is_empty());
    assert_eq!(session.published_count(), 0);
    cleanup(&root);
}

/// 绑定纪律：payload 帧的 total_size 与 header 不符 → 拒绝。
#[test]
fn payload_total_size_mismatch_is_rejected() {
    let root = temp_root("mismatch");
    let mut session = ReceiveSession::open(&root, 1 << 20).expect("session");
    session
        .on_introduction(&[FileOffer::new("a.bin", 8, None)], ReceiveDecision::Accept)
        .expect("introduction");
    let bad = PayloadTransferFrame::data(1, 16, 0, false, vec![0u8; 4]);
    assert_eq!(
        session.on_payload_frame(&bad).expect_err("total_size 不符").code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(session.published_count(), 0);
    cleanup(&root);
}

/// 控制帧：CANCEL 必须在未完成时拒绝并清理（不得留下最终文件）。
#[test]
fn cancel_mid_transfer_leaves_no_file() {
    let root = temp_root("cancel");
    let mut session = ReceiveSession::open(&root, 1 << 20).expect("session");
    session
        .on_introduction(&[FileOffer::new("late.bin", 16, None)], ReceiveDecision::Accept)
        .expect("introduction");
    session
        .on_payload_frame(&PayloadTransferFrame::data(1, 16, 0, false, vec![0u8; 4]))
        .expect("partial");
    session
        .on_payload_frame(&PayloadTransferFrame::control(PacketType::Control))
        .expect("control 帧本身合法");
    assert_eq!(session.published_count(), 0, "取消后不得有最终文件");
    assert!(session.entries().is_empty(), "取消后条目必须清理");
    cleanup(&root);
}

// ---------------------------------------------------------------- helpers

fn temp_root(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("xqs-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn cleanup(root: &std::path::Path) {
    let _ = std::fs::remove_dir_all(root);
}
