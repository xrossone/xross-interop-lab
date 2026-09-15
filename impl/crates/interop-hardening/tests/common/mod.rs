// 共用模块会被编进每个测试二进制，各二进制只用到其中一部分 —— 未用项是正常的。
#![allow(dead_code)]

//! 扫描目标表（两个测试二进制共用：契约扫描 + 分配预算）。
//!
//! 每个目标 = 一个吃**线上字节**的解码入口 + 一个**合法基线帧**（变异起点）。
//! 基线由各 crate 自己的编码器产出（不是手写字节），因此"基线被接受"本身就是一个断言：
//! 一旦某个编码器与解码器脱节，这里会先炸。

use interop_contract::capability::ApiVersion;
use interop_ipc::media_frame::{flags, FrameKind, MediaFrame};
use interop_ipc::frame as ipc_frame;
use interop_testkit::adversarial::{corpus, Corpus, Outcome};
use proto_airplay::rtsp::RtspRequest;
use proto_cast::castv2::CastMessage;
use proto_cast::namespaces;
use proto_quickshare::control::{
    decode_keepalive_offline, decode_payload_ack, DisconnectionFrame, KeepAliveFrame,
    PairedKeyMaterial, PairedKeyResultFrame,
};
use proto_quickshare::framing::{encode_frame, FrameDecoder};
use proto_quickshare::handshake::{ClientConfig, HandshakeConfig, Ukey2Client, Ukey2Server};
use proto_quickshare::payload::{encode_payload_transfer_frame, PayloadTransferFrame};
use proto_quickshare::wire;
use proto_upnp::dmc::DescriptionFetchPolicy;
use proto_upnp::soap::SoapMessage;
use proto_upnp::ssdp::SsdpMessage;
use proto_upnp::xml::{parse_document, XmlBudget};
use proto_wfd::ie::WfdSubelement;
use proto_wfd::messages::{parse_parameter_body, parse_parameter_names, Request, Response};
use proto_wfd::negotiate::{AudioCodecSet, ContentProtection, RtpPorts, Transport, VideoFormatSet};
use proto_wfd::rtp::parse_header;

/// 握手协商的 next_protocol（与 T20 测试同一取值）。
pub const NEXT_PROTOCOL: &str = "AES_256_CBC-HMAC_SHA256";

/// 主 seed；每个目标再按 label 派生（见 [`seed_for`]）。
pub const SEED: u64 = 0x5EED_2026_0915;

/// 每类形状（uniform/structured/boundary/mutate）的用例数。
pub const PER_SHAPE: usize = 48;

/// 解码探针：`&[u8]` → 接受/拒绝。
pub type Probe = Box<dyn Fn(&[u8]) -> Outcome>;

/// 一个扫描目标。
pub struct Target {
    /// 标签（必须与 `interop_hardening::SCAN_TARGETS` 一致，测试会强制）。
    pub label: &'static str,
    /// 合法基线帧（变异起点）。
    pub base: Vec<u8>,
    /// 解码探针：`Ok` → `Accepted`，`Err` → `Rejected`；消费长度断言见 `expect_consumed`。
    pub probe: Probe,
    /// 该入口**在真实调用链里**能拿到的最长输入（字节）。大输入断言按它截断：
    /// 给一个调用方早就按 64 KiB 拒掉的解析器喂 1 MiB，测的是不存在的场景。
    /// 取值一律来自实现里的具名常量（不是这里现编的数）。
    pub max_input_bytes: usize,
    /// 结构化解析器的**结构预算**（字节，0 表示不适用）：树/表这类解析器的内存由
    /// "元素数量预算 × 每元素开销"决定，不由输入字节长度决定。
    ///
    /// 每元素开销是**本仓策略上界**（不是协议常量）：实测 `<a x="1"/>` 在
    /// `XmlBudget::default()` 下约 643 字节/元素（name String + attrs BTreeMap + children Vec），
    /// 这里按 1 KiB 记，留约 60% 余量；超了就是"每元素开销失控"，属于要看的信号。
    pub structural_budget_bytes: usize,
    /// **流式**入口在"输入还没带这么多字节"时允许按声明长度分配的上限（字节）。
    ///
    /// 只对流式入口非零（IP1 长度前缀、Quick Share 分帧：它们本来就是"先备好缓冲再读"），
    /// 且这个数字必须来自实现里那个具名的策略常量。**"拒绝阈值"不算**：
    /// `XmlBudget::max_bytes`、media_frame 的 16 MiB、CASTV2 的 64 KiB 都是"超了拒绝"，
    /// 不构成"可以按声明长度先分配"的许可——那些入口的许可是 0。
    pub policy_cap_bytes: usize,
}

/// **只接受拒绝**的入口：(标签, 理由)。blocked/未实现的特性必须有"一个都不接受"的机器断言，
/// 而不是"没崩就算过"——部分解析或部分接受同样是臆造，且会让"已实现"这句话失去边界。
pub const ALWAYS_REJECTS: &[(&str, &str)] = &[
    (
        "wfd/ie/container_parse",
        "F-24：完整 WFD IE 容器的 OUI 在四个来源里都不存在 → 只允许整体拒绝",
    ),
    (
        "airplay/pairstore/check_request",
        "T34：配对交换不实现——形状校验后一律 unsupported-feature；能接受就意味着开始实现配对",
    ),
];

/// 无"按声明长度预分配"许可的入口。
pub const NO_CAP: usize = 0;

/// IP1 / Quick Share 分帧的长度前缀声明上限（本仓策略值，与实现里的常量一致）。
pub const FRAME_CAP: usize = 256 * 1024;

/// 按 label 派生 seed（FNV-1a）。
pub fn seed_for(label: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in label.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    SEED ^ h
}

pub fn outcome(ok: bool) -> Outcome {
    if ok {
        Outcome::Accepted
    } else {
        Outcome::Rejected
    }
}

/// 任意字节 → 可解析字符串（str 型解析器的线上入口都长这样）。
pub fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// 全部目标。
pub fn targets() -> Vec<Target> {
    let (client_init, server_init, client_finished) = handshake_messages();
    let ukey2_inner = |msg: &[u8]| {
        wire::decode_ukey2_message(msg)
            .expect("ukey2 message")
            .message_data
    };
    let (ci_data, si_data, cf_data) = (
        ukey2_inner(&client_init),
        ukey2_inner(&server_init),
        ukey2_inner(&client_finished),
    );
    let keypair = proto_quickshare::crypto::DhKeyPair::generate().expect("keypair");
    let public_key = wire::encode_generic_public_key(keypair.public());
    let header = wire::encode_header(1, 1, Some(&[0u8; 12]), Some(&[0xaa, 0xbb]));
    let hab = wire::encode_header_and_body(&header, &[0x01, 0x02, 0x03]);
    let secure = wire::encode_secure_message(&hab, &[0x77; 8]);
    let d2d = wire::encode_device_to_device(3, &[0x09, 0x08]);

    let keepalive_inner = KeepAliveFrame::new(true, 0).encode();
    let keepalive_offline = KeepAliveFrame::new(true, 0).encode_offline();
    let material = PairedKeyMaterial::new(vec![0x01, 0x02, 0x03], vec![0x04, 0x05])
        .expect("material")
        .encode_inner();
    let paired_result = PairedKeyResultFrame::unable().encode_inner();
    let disconnect = DisconnectionFrame::reply_ack().encode_offline();
    let ack_frame = proto_quickshare::control::encode_payload_ack(0x2a).expect("payload ack");
    let payload_frame =
        encode_payload_transfer_frame(&PayloadTransferFrame::data(7, 4096, 0, true, vec![0xab; 32]))
            .expect("payload frame");
    let framed = encode_frame(&[0x01, 0x02, 0x03]);

    let cast_msg = CastMessage::text(
        "sender-0",
        "receiver-0",
        "urn:x-cast:com.google.cast.receiver",
        r#"{"type":"GET_STATUS","requestId":1}"#,
    );
    let cast_body = cast_msg.encode_body().expect("cast body");
    let cast_framed = cast_msg.encode_frame().expect("cast frame");
    let cast_for_namespaces = cast_msg.clone();

    let media = MediaFrame {
        kind: FrameKind::EncodedAccessUnit,
        stream_id: 1,
        flags: flags::KEYFRAME,
        sequence: 1,
        pts: 12_345,
        payload: vec![0x5a; 64],
    };
    let media_bytes = media.encode();
    let mut ipc_framed = Vec::new();
    ipc_frame::encode_frame(b"hello", &mut ipc_framed);

    let subelement = WfdSubelement::new(0, vec![0x00; 5])
        .expect("subelement")
        .to_bytes();
    let wfd_request = proto_wfd::messages::build_request(
        "OPTIONS",
        "*",
        1,
        None,
        &[("Require", "org.wfa.wfd1.0")],
        "",
    );
    let wfd_response: &[u8] =
        b"RTSP/1.0 200 OK\r\nCSeq: 3\r\nContent-Type: text/parameters\r\nContent-Length: 0\r\n\r\n";
    let rtsp_request: &[u8] =
        b"SETUP rtsp://127.0.0.1/1 RTSP/1.0\r\nCSeq: 2\r\nContent-Length: 0\r\n\r\n";
    let xml_base: &[u8] =
        br#"<root xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><a x="1">t</a><b/></root>"#;
    let ssdp_base = proto_upnp::ssdp::build_search("ssdp:all", 3).expect("ssdp search");
    let soap_base: &[u8] = br#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:GetTransportInfo xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>0</InstanceID></u:GetTransportInfo></s:Body></s:Envelope>"#;

    let mut out: Vec<Target> = Vec::new();
    let mut push = |label: &'static str,
                    base: Vec<u8>,
                    probe: Probe,
                    max_input_bytes: usize,
                    structural_budget_bytes: usize,
                    policy_cap_bytes: usize| {
        out.push(Target {
            label,
            base,
            probe,
            max_input_bytes,
            structural_budget_bytes,
            policy_cap_bytes,
        });
    };
    // 调用方上限的具名来源（大输入断言按这些数字截断）
    let rtsp_cap = proto_airplay::rtsp::MAX_HEADER_BYTES + proto_airplay::rtsp::MAX_BODY_BYTES;
    let wfd_rtsp_cap =
        proto_wfd::messages::MAX_HEADER_BYTES + proto_wfd::messages::MAX_BODY_BYTES;
    let wfd_body_cap = proto_wfd::messages::MAX_BODY_BYTES;
    let wfd_sub_cap = 3 + proto_wfd::ie::MAX_SUBELEMENT_BYTES;
    let ssdp_cap = proto_upnp::ssdp::MAX_MESSAGE_BYTES;
    let xml_cap = XmlBudget::default().max_bytes;
    let cast_body_cap = proto_cast::castv2::MAX_BODY_BYTES;
    let qs_frame_cap = FRAME_CAP;
    let ipc_frame_cap = 4 + interop_ipc::frame::MAX_FRAME;
    let ipc_media_cap = 36 + interop_ipc::media_frame::MAX_PAYLOAD as usize;
    let cast_txt_cap = 8 * 1024; // mDNS TXT 记录的实际上限（单个 TXT 记录 ≤ 255 字节，整条记录远小于此）
    // 结构化解析器的结构预算：字节预算 + 元素预算 × 每元素上界（本仓策略，见字段文档）
    let xml_budget = XmlBudget::default();
    let xml_structural = xml_budget.max_bytes + xml_budget.max_elements * 1024;

    // ---- Quick Share / UKEY2（线上字节）
    push(
        "quickshare/wire/decode_ukey2_message",
        client_init.clone(),
        Box::new(|b| outcome(wire::decode_ukey2_message(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/wire/decode_client_init",
        ci_data,
        Box::new(|b| outcome(wire::decode_client_init(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/wire/decode_server_init",
        si_data,
        Box::new(|b| outcome(wire::decode_server_init(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/wire/decode_client_finished",
        cf_data,
        Box::new(|b| outcome(wire::decode_client_finished(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/wire/decode_generic_public_key",
        public_key,
        Box::new(|b| outcome(wire::decode_generic_public_key(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/wire/decode_secure_message",
        secure,
        Box::new(|b| outcome(wire::decode_secure_message(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/wire/decode_header_and_body",
        hab,
        Box::new(|b| outcome(wire::decode_header_and_body(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/wire/decode_header",
        header,
        Box::new(|b| outcome(wire::decode_header(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/wire/decode_device_to_device",
        d2d,
        Box::new(|b| outcome(wire::decode_device_to_device(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/control/keepalive_frame_decode",
        keepalive_inner,
        Box::new(|b| outcome(KeepAliveFrame::decode(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/control/decode_keepalive_offline",
        keepalive_offline,
        Box::new(|b| outcome(decode_keepalive_offline(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/control/decode_payload_ack",
        ack_frame,
        Box::new(|b| outcome(decode_payload_ack(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/control/paired_key_encryption/decode_inner",
        material,
        Box::new(|b| outcome(PairedKeyMaterial::decode_inner(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/control/paired_key_result/decode_inner",
        paired_result,
        Box::new(|b| outcome(PairedKeyResultFrame::decode_inner(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/control/disconnection/decode_offline",
        disconnect,
        Box::new(|b| outcome(DisconnectionFrame::decode_offline(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/payload/decode_payload_transfer_frame",
        payload_frame,
        Box::new(|b| outcome(proto_quickshare::payload::decode_payload_transfer_frame(b).is_ok())),
        qs_frame_cap,
        0,
        NO_CAP,
    );
    push(
        "quickshare/framing/push",
        framed,
        Box::new(|b| {
            let mut decoder = FrameDecoder::new(256 * 1024);
            outcome(decoder.push(b).is_ok())
        }),
        qs_frame_cap,
        0,
        FRAME_CAP, // 流式分帧：先按声明长度备缓冲，再等字节到达
    );

    // ---- AirPlay
    push(
        "airplay/rtsp/parse",
        rtsp_request.to_vec(),
        Box::new(|b| match RtspRequest::parse(b) {
            Ok((_r, consumed)) => interop_testkit::adversarial::expect_consumed(b.len(), consumed),
            Err(_) => Outcome::Rejected,
        }),
        rtsp_cap,
        0,
        NO_CAP,
    );

    // ---- DLNA / UPnP
    push(
        "upnp/xml/parse_document",
        xml_base.to_vec(),
        Box::new(|b| outcome(parse_document(b, XmlBudget::default()).is_ok())),
        xml_cap,
        xml_structural,
        NO_CAP, // 预算是**拒绝**阈值：先看长度，超了直接拒绝，不按声明长度分配
    );
    push(
        "upnp/ssdp/parse",
        ssdp_base,
        Box::new(|b| outcome(SsdpMessage::parse(b).is_ok())),
        ssdp_cap,
        0,
        NO_CAP,
    );
    push(
        "upnp/soap/parse_raw",
        soap_base.to_vec(),
        Box::new(|b| outcome(SoapMessage::parse_raw(b).is_ok())),
        xml_cap,
        xml_structural,
        NO_CAP,
    );
    push(
        "upnp/dmc/description_fetch_policy",
        b"http://192.168.1.10:49152/desc.xml".to_vec(),
        Box::new(|b| outcome(DescriptionFetchPolicy::default().check(&lossy(b)).is_ok())),
        DescriptionFetchPolicy::default().max_url_bytes,
        0,
        0,
    );

    // ---- Google Cast
    push(
        "cast/castv2/decode_frame",
        cast_framed,
        Box::new(|b| match CastMessage::decode_frame(b) {
            Ok((_m, consumed)) => interop_testkit::adversarial::expect_consumed(b.len(), consumed),
            Err(_) => Outcome::Rejected,
        }),
        4 + cast_body_cap,
        0,
        NO_CAP, // F-04 的 64 KiB 是拒绝阈值；长度前缀的 256 KiB 上限属于流式入口
    );
    push(
        "cast/castv2/decode_body",
        cast_body.clone(),
        Box::new(|b| outcome(CastMessage::decode_body(b).is_ok())),
        cast_body_cap,
        0,
        NO_CAP,
    );
    push(
        "cast/namespaces/decode_message",
        cast_body,
        Box::new(move |b| {
            // 命名空间层吃的是已解出的 CastMessage；变异过的字节同时覆盖"载荷是攻击者控制的 JSON"
            let candidate = match CastMessage::decode_body(b) {
                Ok(decoded) => decoded,
                Err(_) => CastMessage {
                    payload: proto_cast::castv2::Payload::Utf8(lossy(b)),
                    ..cast_for_namespaces.clone()
                },
            };
            outcome(namespaces::decode_message(&candidate).is_ok())
        }),
        cast_body_cap,
        0,
        NO_CAP,
    );

    // ---- WFD / Miracast
    push(
        "wfd/messages/request_parse",
        wfd_request,
        Box::new(|b| match Request::parse(b) {
            Ok((_r, consumed)) => interop_testkit::adversarial::expect_consumed(b.len(), consumed),
            Err(_) => Outcome::Rejected,
        }),
        wfd_rtsp_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/messages/response_parse",
        wfd_response.to_vec(),
        Box::new(|b| match Response::parse(b) {
            Ok((_r, consumed)) => interop_testkit::adversarial::expect_consumed(b.len(), consumed),
            Err(_) => Outcome::Rejected,
        }),
        wfd_rtsp_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/messages/parse_parameter_names",
        b"wfd_audio_codecs\r\nwfd_video_formats\r\n".to_vec(),
        Box::new(|b| outcome(parse_parameter_names(&lossy(b)).is_ok())),
        wfd_body_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/messages/parse_parameter_body",
        b"wfd_video_formats: 00 00 02 04 00000020 00000000 00000000 00 0000 0000 00 none none\r\n"
            .to_vec(),
        Box::new(|b| outcome(parse_parameter_body(&lossy(b)).is_ok())),
        wfd_body_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/ie/subelement_parse",
        subelement,
        Box::new(|b| match WfdSubelement::parse(b) {
            Ok((_s, consumed)) => interop_testkit::adversarial::expect_consumed(b.len(), consumed),
            Err(_) => Outcome::Rejected,
        }),
        wfd_sub_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/rtp/parse_header",
        vec![0x80u8; 12],
        Box::new(|b| outcome(parse_header(b).is_ok())),
        2 * 1024,
        0,
        NO_CAP,
    );
    push(
        "wfd/negotiate/video_formats",
        b"00 00 02 04 00000020 00000000 00000000 00 0000 0000 00 none none".to_vec(),
        Box::new(|b| outcome(VideoFormatSet::parse(&lossy(b)).is_ok())),
        wfd_body_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/negotiate/audio_codecs",
        b"LPCM 00000003 00, AAC 00000001 00".to_vec(),
        Box::new(|b| outcome(AudioCodecSet::parse(&lossy(b)).is_ok())),
        wfd_body_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/negotiate/rtp_ports",
        b"RTP/AVP/UDP;unicast 20000 0 mode=play".to_vec(),
        Box::new(|b| outcome(RtpPorts::parse(&lossy(b)).is_ok())),
        wfd_body_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/negotiate/transport",
        b"RTP/AVP/UDP;unicast;client_port=20000-20001".to_vec(),
        Box::new(|b| outcome(Transport::parse(&lossy(b)).is_ok())),
        wfd_body_cap,
        0,
        NO_CAP,
    );
    push(
        "wfd/negotiate/content_protection",
        b"none".to_vec(),
        Box::new(|b| outcome(ContentProtection::parse(&lossy(b)).is_ok())),
        wfd_body_cap,
        0,
        NO_CAP,
    );

    // ---- 本仓 IPC（长度前缀 + XMD1 媒体帧头）
    push(
        "ipc/media_frame/decode",
        media_bytes,
        Box::new(|b| match MediaFrame::decode(b) {
            Ok((_f, consumed)) => interop_testkit::adversarial::expect_consumed(b.len(), consumed),
            Err(_) => Outcome::Rejected,
        }),
        ipc_media_cap,
        0,
        NO_CAP, // 16 MiB 是"分配前拒绝"的阈值，不是预分配许可
    );
    push(
        "ipc/frame/read_frame",
        ipc_framed,
        Box::new(|b| {
            let mut cursor = std::io::Cursor::new(b.to_vec());
            match ipc_frame::read_frame(&mut cursor) {
                Ok(Some(body)) => {
                    assert!(
                        body.len() <= 256 * 1024,
                        "帧体 {} 字节超过长度前缀的声明上限",
                        body.len()
                    );
                    Outcome::Accepted
                }
                Ok(None) | Err(_) => Outcome::Rejected,
            }
        }),
        ipc_frame_cap,
        0,
        FRAME_CAP,
    );

    // ---- 形状校验器（键名/取值来自对端 RTSP，属于线上输入）
    push(
        "airplay/audio_control/check_shape",
        b"mode".to_vec(),
        Box::new(|b| {
            let key = lossy(b);
            outcome(
                proto_airplay::audio_control::AudioControlRequest::AudioMode
                    .check_shape(
                        "POST",
                        "/audioMode",
                        Some("application/x-apple-binary-plist"),
                        &[key.as_str()],
                    )
                    .is_ok(),
            )
        }),
        256,
        0,
        NO_CAP,
    );
    push(
        "airplay/pairstore/check_request",
        b"password".to_vec(),
        Box::new(|b| {
            let key = lossy(b);
            outcome(
                proto_airplay::pairstore::PairingEndpoint::PairPinStart
                    .check_request(&[key.as_str()])
                    .is_ok(),
            )
        }),
        256,
        0,
        NO_CAP,
    );

    // ---- 补齐的线上入口（gate 要求"每个吃线上字节的解析器都有扫描用例"）
    push(
        "cast/discovery/from_txt",
        b"_googlecast._tcp".to_vec(),
        Box::new(|b| {
            let txt = vec![
                ("id".to_string(), lossy(b)),
                ("fn".to_string(), "客厅电视".to_string()),
                ("ve".to_string(), "05".to_string()),
            ];
            outcome(
                proto_cast::discovery::ReceiverInfo::from_txt(
                    proto_cast::discovery::SERVICE_TYPE,
                    &txt,
                    8009,
                )
                .is_ok(),
            )
        }),
        cast_txt_cap,
        0,
        NO_CAP,
    );
    push(
        "cast/receiver/from_txt",
        b"id=abc123".to_vec(),
        Box::new(|b| {
            let pairs = vec![
                ("id".to_string(), "abc123".to_string()),
                ("fn".to_string(), lossy(b)),
            ];
            outcome(proto_cast::receiver::ReceiverInfo::from_txt(&pairs).is_ok())
        }),
        cast_txt_cap,
        0,
        NO_CAP,
    );
    push(
        "cast/namespaces/decode",
        b"{\"type\":\"GET_STATUS\",\"requestId\":1}".to_vec(),
        Box::new(|b| {
            outcome(
                proto_cast::namespaces::CastPayload::decode(
                    "urn:x-cast:com.google.cast.receiver",
                    &lossy(b),
                )
                .is_ok(),
            )
        }),
        cast_body_cap,
        0,
        NO_CAP,
    );
    push(
        "upnp/soap/action_from_wire",
        b"Play".to_vec(),
        Box::new(|b| outcome(proto_upnp::soap::SoapAction::from_wire(&lossy(b)).is_ok())),
        256,
        0,
        NO_CAP,
    );
    push(
        "upnp/dms/browse_flag_from_wire",
        b"BrowseDirectChildren".to_vec(),
        Box::new(|b| {
            outcome(proto_upnp::dms::BrowseFlag::from_wire(&lossy(b)).is_ok())
        }),
        64,
        0,
        NO_CAP,
    );
    push(
        "wfd/ie/container_parse",
        vec![0x00, 0x06, 0x00, 0x00],
        Box::new(|b| {
            // F-24：OUI 在四个来源里都不存在 → 完整 WFD IE **一律拒绝**，绝不部分解析。
            // 这里不仅要求不 panic，还要求"一个都不接受"（见 decode_campaign 的专门断言）。
            outcome(proto_wfd::ie::WfdIeContainer::parse(b).is_ok())
        }),
        wfd_sub_cap,
        0,
        NO_CAP,
    );
    push(
        "contract/media/from_wire",
        b"encoded-stream".to_vec(),
        Box::new(|b| outcome(interop_contract::media::MediaForm::from_wire(&lossy(b)).is_ok())),
        256,
        0,
        NO_CAP,
    );

    // ---- 契约
    push(
        "contract/capability/api_version",
        b"interop.api/0.1".to_vec(),
        Box::new(|b| outcome(ApiVersion::parse(&lossy(b)).is_ok())),
        256,
        0,
        NO_CAP,
    );

    out
}

/// 一个目标的完整语料：基线变异（testkit 的四类形状）+ 长度炸弹。
pub fn target_corpus(target: &Target) -> Corpus {
    let mut c = corpus(seed_for(target.label), target.label, &target.base, PER_SHAPE);
    c.inputs.extend(length_bombs(&target.base));
    c
}

/// 跑一次确定性握手，拿到三个真实消息（语料基线）。
fn handshake_messages() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut cli = Ukey2Client::with_fixed_entropy(
        ClientConfig::nearby_default(NEXT_PROTOCOL),
        [9u8; 32],
        &[0x22u8; 32],
    )
    .expect("client");
    let mut srv = Ukey2Server::with_fixed_entropy(
        HandshakeConfig::nearby_default(),
        [7u8; 32],
        &[0x11u8; 32],
    )
    .expect("server");
    let client_init = cli.start().expect("client init");
    let server_init = srv.handle_client_init(&client_init, 0).expect("server init");
    let (client_finished, _keys) = cli.handle_server_init(&server_init).expect("client finish");
    (client_init, server_init, client_finished)
}

/// 长度炸弹：**输入很小，声明的长度很大**。这正是"长度先于检查"的实现会露馅的地方。
///
/// 极值停在 16 MiB（模块文档说了原因：4 GiB 的失败模式是进程 abort，抓不住也不该抓）。
pub fn length_bombs(base: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = vec![
        vec![0x00; 4],
        vec![0xff; 4],
        vec![0x80; 4],
        vec![0x01, 0x00, 0x00, 0x00], // 16 MiB 大端
        vec![0x00, 0x10, 0x00, 0x00], // 16 MiB 大端（按位读时是 1 MiB）
        vec![0x00, 0x04, 0x00, 0x00], // 256 KiB（长度前缀的声明上限）
        vec![0x00, 0x00, 0x00, 0x01], // 小端 16 MiB
        vec![0x7f, 0xff, 0xff, 0xff],
        vec![0x80, 0x80, 0x80, 0x80, 0x08], // protobuf：超长 varint
        vec![0x0a, 0xff, 0xff, 0xff, 0x7f], // protobuf：字段 1、长度 0x0FFFFFFF
        vec![0x12, 0x00, 0x10, 0x00],       // protobuf：字段 2、长度 1 MiB（只有 0 字节正文）
        b"Content-Length: 16777216\r\n\r\n".to_vec(),
        b"Content-Length: 8589934592\r\n\r\n".to_vec(),
        b"<?xml version=\"1.0\"?><a>&#x0;</a>".to_vec(),
    ];
    // 在炸弹后面接上基线的一部分与一段结构化填充：让"先读长度再读正文"的实现在正文处露馅。
    for bomb in out.clone() {
        let mut with_tail = bomb.clone();
        with_tail.extend_from_slice(&base[..base.len().min(32)]);
        with_tail.extend_from_slice(b"AAAAAAAAAAAAAAAA");
        out.push(with_tail);
    }
    out
}

/// 超长输入（结构化填充），用于"分配随时间/输入是否成比例"的粗测。
pub fn big_input(len: usize) -> Vec<u8> {
    let mut gen = interop_testkit::adversarial::InputGen::new(seed_for("big-input"));
    let mut out = gen.structured(len);
    // 保证内部有可被解析的分隔符（XML/RTSP/SOAP 系解析器才走得到深处）
    if out.len() >= 8 {
        out[..4].copy_from_slice(b"<a> ");
    }
    out
}

/// 1 MiB 级的**形状化**大输入。
///
/// 通用随机字节证明不了"最坏形状"：分配放大只会在**合法的开头 + 大量重复**下露出来
/// （先收集后校验的实现遇到 1 MiB 合法 token 串才会把 token 表撑爆）。所以每种语法各给一份。
pub fn big_shaped_inputs() -> Vec<(&'static str, Vec<u8>)> {
    const N: usize = 1024 * 1024;
    let mut out: Vec<(&'static str, Vec<u8>)> = vec![("generic-structured", big_input(N))];
    out.push(("space-tokens", repeat_to(b"a ", N)));
    out.push((
        "rtp-ports-tokens",
        repeat_to(b"RTP/AVP/UDP;unicast 20000 0 mode=play ", N),
    ));
    out.push(("audio-descriptors", repeat_to(b"LPCM 00000003 00, ", N)));
    out.push((
        "video-descriptors",
        repeat_to(
            b"00 00 02 04 00000020 00000000 00000000 00 0000 0000 00 none none, ",
            N,
        ),
    ));
    out.push(("crlf-lines", repeat_to(b"wfd_audio_codecs\r\n", N)));
    out.push(("rtsp-headers", {
        let mut v = b"OPTIONS * RTSP/1.0\r\n".to_vec();
        v.extend_from_slice(&repeat_to(b"X: y\r\n", N));
        v
    }));
    out.push(("xml-nesting", {
        // 深度炸弹：预算必须在**递归之前**生效（否则这里是爆栈，不是 Err）
        let k = N / 7;
        let mut v = b"<a>".repeat(k);
        v.extend_from_slice(&b"</a>".repeat(k));
        v
    }));
    out.push(("xml-siblings", {
        let mut v = b"<root>".to_vec();
        v.extend_from_slice(&repeat_to(b"<a x=\"1\"/>", N));
        v.extend_from_slice(b"</root>");
        v
    }));
    out.push(("soap-body-children", {
        let mut v = br#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>"#.to_vec();
        v.extend_from_slice(&repeat_to(b"<a>1</a>", N));
        v.extend_from_slice(b"</s:Body></s:Envelope>");
        v
    }));
    out.push(("json-object", {
        let mut v = b"{\"a\":1".to_vec();
        v.extend_from_slice(&repeat_to(b",\"b\":2", N));
        v.extend_from_slice(b"}");
        v
    }));
    out.push(("protobuf-empty-fields", repeat_to(b"\x0a\x00", N)));
    out.push(("varint-continuation", repeat_to(b"\x80", N)));
    out.push(("declared-frame-length", {
        let mut v = vec![0x00, 0x04, 0x00, 0x00]; // 声明 256 KiB
        v.extend_from_slice(&repeat_to(b"Z", N));
        v
    }));
    out
}

fn repeat_to(unit: &[u8], total: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(total + unit.len());
    while out.len() < total {
        out.extend_from_slice(unit);
    }
    out.truncate(total);
    out
}
