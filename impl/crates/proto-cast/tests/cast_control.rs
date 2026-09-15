//! T43-01..04 验收测试（plans/03-casting.md §T43；字段表见 `specs-reviewed/m08-cast-media-control.md`）。
//!
//! - T43-01 TLS/auth 失败 → 不启动播放；
//! - T43-02 app launch 失败 → 明确错误、不是 media loading；
//! - T43-03 停止或会话被接收端终止 → 清理 URL 与控制状态；
//! - T43-04 只 URL 可用 → screen 能力保持 false。
//!
//! 全部字节自制；测试里的 app ID 是占位值（不是任何真实注册 app ID）。

#![allow(clippy::result_large_err)]

use interop_contract::error::ErrorCode;
use proto_cast::castv2::{CastMessage, Payload};
use proto_cast::controller::{
    validate_media_url, ChannelGate, Controller, ControllerState, Event, OpenGateForTesting,
    UnavailableGate, HEARTBEAT_PING_MS,
};
use proto_cast::discovery::{DeviceStatus, ReceiverInfo, DEFAULT_PORT, SERVICE_TYPE};
use proto_cast::namespaces::{
    decode_message, CastPayload, PlayerState, StreamType, Volume, NS_CONNECTION, NS_HEARTBEAT,
    NS_MEDIA, NS_RECEIVER,
};
use proto_cast::{MAX_BODY_BYTES, RECEIVER_PLATFORM_ID};

const TEST_APP_ID: &str = "TESTAPP01"; // 自制占位
const TEST_SENDER_ID: &str = "sender-test";
const TEST_MEDIA_URL: &str = "http://192.0.2.30:8000/media/clip.mp4";
const TEST_CONTENT_TYPE: &str = "video/mp4";

fn controller(gate: Box<dyn ChannelGate>) -> Controller {
    Controller::new(gate, TEST_SENDER_ID, vec![TEST_APP_ID.to_string()]).expect("可建控制器")
}

fn txt(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 从对端（receiver-0）发来的一条消息。
fn inbound(source: &str, destination: &str, payload: CastPayload, request_id: Option<u64>) -> CastMessage {
    payload
        .into_message(source, destination, request_id)
        .expect("可编码")
}

fn msg_text(message: &CastMessage) -> String {
    message.payload.as_utf8().unwrap_or("").to_string()
}

// ------------------------------------------------------------------ T43-01

#[test]
fn t43_01_gate_closed_sends_nothing() {
    let mut controller = controller(Box::new(UnavailableGate));
    // 闸门未开：连 CONNECT 都发不出去。
    let err = controller.connect(0).unwrap_err();
    assert_eq!(err.code, ErrorCode::VendorGated, "生产闸门必须是 vendor-gated");
    assert!(err.message.contains("不实现"), "拒绝理由必须说清为何不可用");
    let stats = controller.stats();
    assert_eq!(stats.sent, 0, "T43-01：没有任何命令被发出");
    assert_eq!(stats.state, ControllerState::Idle);
    assert_eq!(stats.gate, "unavailable");

    // 直接尝试 load 也不行（状态检查 + 闸门）。
    assert!(controller.load(TEST_MEDIA_URL, TEST_CONTENT_TYPE, None, 0).is_err());
    assert!(controller.play(0).is_err());
    let stats = controller.stats();
    assert_eq!(stats.sent, 0);
    assert!(!stats.media_url_held, "T43-01：没有播放被启动，也没有 URL 被占用");
    assert!(!stats.screen_capability);
}

// ------------------------------------------------------------------ T43-02

#[test]
fn t43_02_launch_error_is_explicit_and_not_media_loading() {
    let mut controller = controller(Box::new(OpenGateForTesting));
    controller.connect(1_000).expect("CONNECT 可发");
    let connected = inbound(RECEIVER_PLATFORM_ID, TEST_SENDER_ID, CastPayload::Connected, None);
    let outcome = controller.on_message(&connected, 1_050).expect("CONNECTED 可处理");
    assert_eq!(outcome.events, vec![Event::Connected]);
    assert_eq!(controller.state(), ControllerState::Connected);

    // app ID 不在配置里 → 直接拒绝（F-17）。
    let err = controller.launch("UNAUTHORIZED", 1_100).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    assert!(err.message.contains("不在本节点配置"));

    let launch = controller.launch(TEST_APP_ID, 1_150).expect("LAUNCH 可发");
    let request_id = {
        let (payload, request_id) = decode_message(&launch.outbound[0]).expect("可解析");
        assert_eq!(payload, CastPayload::Launch { app_id: TEST_APP_ID.into() });
        request_id
    };
    assert_eq!(controller.state(), ControllerState::Launching);

    // 接收端明确拒绝拉起：这是 **launch 失败**，不是 media loading 失败。
    let launch_error = inbound(
        RECEIVER_PLATFORM_ID,
        TEST_SENDER_ID,
        CastPayload::LaunchError {
            reason: "NOT_FOUND".into(),
            app_id: Some(TEST_APP_ID.into()),
        },
        request_id,
    );
    let outcome = controller.on_message(&launch_error, 1_200).expect("可处理");
    assert_eq!(outcome.events.len(), 1);
    match &outcome.events[0] {
        Event::LaunchFailed { reason } => {
            assert!(reason.contains("NOT_FOUND"), "原因必须原样带出：{reason}");
            assert!(reason.contains(TEST_APP_ID));
        }
        other => panic!("T43-02：必须是 LaunchFailed，收到 {other:?}"),
    }
    assert_eq!(controller.state(), ControllerState::Connected, "失败后回到 connected");
    assert_eq!(controller.app_id(), None);
    assert!(!controller
        .stats()
        .screen_capability);

    // 失败之后不得进入 LOAD（否则就是把 launch 失败误当成 media loading）。
    let err = controller.load(TEST_MEDIA_URL, TEST_CONTENT_TYPE, None, 1_250).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    assert!(err.message.contains("T43-02"), "错误信息要写明这条约束");
    assert_eq!(controller.media_url(), None);
    assert!(!controller.stats().media_url_held);
}

// ------------------------------------------------------------------ T43-03

/// 走到"媒体会话已建立"的控制器（含自制对端回复）。
fn launched_controller() -> Controller {
    let mut controller = controller(Box::new(OpenGateForTesting));
    controller.connect(1_000).expect("CONNECT");
    let connected = inbound(RECEIVER_PLATFORM_ID, TEST_SENDER_ID, CastPayload::Connected, None);
    controller.on_message(&connected, 1_010).expect("CONNECTED");
    let launch = controller.launch(TEST_APP_ID, 1_020).expect("LAUNCH");
    let launch_request = decode_message(&launch.outbound[0]).expect("可解析").1;
    let status = inbound(
        RECEIVER_PLATFORM_ID,
        TEST_SENDER_ID,
        CastPayload::ReceiverStatus(proto_cast::ReceiverStatus {
            applications: vec![proto_cast::namespaces::ApplicationStatus {
                app_id: TEST_APP_ID.into(),
                display_name: Some("Test Receiver".into()),
                session_id: Some("session-1".into()),
                transport_id: Some("transport-1".into()),
                namespaces: vec![NS_MEDIA.into()],
                status_text: None,
            }],
            volume: Some(Volume {
                level: Some(0.5),
                muted: Some(false),
            }),
            is_stand_by: Some(false),
        }),
        launch_request,
    );
    let outcome = controller.on_message(&status, 1_030).expect("RECEIVER_STATUS");
    assert!(matches!(outcome.events[0], Event::Launched { .. }));
    assert_eq!(controller.state(), ControllerState::Launched);

    let load = controller
        .load(TEST_MEDIA_URL, TEST_CONTENT_TYPE, Some("clip"), 1_040)
        .expect("LOAD");
    let load_request = decode_message(&load.outbound[0]).expect("可解析").1;
    assert_eq!(
        load.outbound[0].destination_id, "transport-1",
        "媒体命令发往 transportId（F-10）"
    );
    let media_status = inbound(
        "transport-1",
        TEST_SENDER_ID,
        CastPayload::MediaStatus(Box::new(proto_cast::MediaStatus {
            media_session_id: Some(42),
            player_state: PlayerState::Playing,
            idle_reason: None,
            current_time: Some(3.5),
            media: None,
            volume: None,
        })),
        load_request,
    );
    let outcome = controller.on_message(&media_status, 1_050).expect("MEDIA_STATUS");
    assert!(matches!(outcome.events[0], Event::MediaStatus(_)));
    assert_eq!(controller.state(), ControllerState::MediaSession);
    assert_eq!(controller.media_session_id(), Some(42));
    assert_eq!(controller.media_url(), Some(TEST_MEDIA_URL));
    controller
}

#[test]
fn t43_03_stop_releases_url_and_clears_state() {
    let mut controller = launched_controller();
    let outcome = controller.stop(2_000).expect("STOP 可发");
    let (payload, _) = decode_message(&outcome.outbound[0]).expect("可解析");
    assert_eq!(payload, CastPayload::MediaStop);
    assert_eq!(
        outcome.events,
        vec![Event::MediaUrlReleased {
            url: TEST_MEDIA_URL.to_string(),
            why: "stop"
        }],
        "T43-03：停止必须报告 URL 可释放"
    );
    assert_eq!(controller.media_session_id(), None);
    assert_eq!(controller.media_url(), None);
    let stats = controller.stats();
    assert!(!stats.media_url_held);
    assert!(!stats.screen_capability);
}

#[test]
fn t43_03_receiver_termination_releases_url() {
    // 路径一：接收端的应用列表变空 = 它把会话停了。
    let mut controller = launched_controller();
    let stopped = inbound(
        RECEIVER_PLATFORM_ID,
        TEST_SENDER_ID,
        CastPayload::ReceiverStatus(proto_cast::ReceiverStatus {
            applications: Vec::new(),
            volume: None,
            is_stand_by: Some(true),
        }),
        None,
    );
    let outcome = controller.on_message(&stopped, 3_000).expect("可处理");
    assert_eq!(
        outcome.events,
        vec![Event::MediaUrlReleased {
            url: TEST_MEDIA_URL.to_string(),
            why: "receiver-stopped"
        }]
    );
    assert_eq!(controller.state(), ControllerState::Connected);
    assert_eq!(controller.media_session_id(), None);
    assert!(!controller.stats().media_url_held);

    // 路径二：媒体自己转入 idle（FINISHED）同样要释放。
    let mut controller = launched_controller();
    let idle = inbound(
        "transport-1",
        TEST_SENDER_ID,
        CastPayload::MediaStatus(Box::new(proto_cast::MediaStatus {
            media_session_id: None,
            player_state: PlayerState::Idle,
            idle_reason: Some("FINISHED".into()),
            current_time: None,
            media: None,
            volume: None,
        })),
        None,
    );
    let outcome = controller.on_message(&idle, 3_100).expect("可处理");
    assert!(outcome.events.iter().any(|event| matches!(
        event,
        Event::MediaUrlReleased { why: "media-idle", .. }
    )));
    assert!(!controller.stats().media_url_held);

    // 路径三：断开连接也要释放。
    let mut controller = launched_controller();
    let outcome = controller.close(3_200).expect("CLOSE 可发");
    assert!(outcome
        .events
        .iter()
        .any(|event| matches!(event, Event::MediaUrlReleased { why: "close", .. })));
    assert_eq!(controller.state(), ControllerState::Ended);
    assert!(!controller.stats().media_url_held);
}

// ------------------------------------------------------------------ T43-04

#[test]
fn t43_04_url_only_never_claims_screen() {
    let mut controller = controller(Box::new(OpenGateForTesting));
    assert!(!controller.screen_capability());
    controller.connect(0).expect("CONNECT");
    assert!(!controller.stats().screen_capability);
    let launched = launched_controller();
    assert!(!launched.screen_capability(), "T43-04：有活动媒体会话也不声明屏幕能力");
    assert!(!launched.stats().screen_capability);
    assert_eq!(launched.stats().state, ControllerState::MediaSession);
}

// ------------------------------------------------------------------ 信封与载荷

#[test]
fn envelope_roundtrip_and_limits() {
    let message = CastPayload::Ping
        .into_message(TEST_SENDER_ID, RECEIVER_PLATFORM_ID, None)
        .expect("可编码");
    let frame = message.encode_frame().expect("可成帧");
    let (decoded, consumed) = CastMessage::decode_frame(&frame).expect("可解帧");
    assert_eq!(consumed, frame.len());
    assert_eq!(decoded, message);
    assert_eq!(decoded.namespace, NS_HEARTBEAT);
    assert_eq!(decoded.payload.payload_type(), proto_cast::PayloadType::String);

    // 二进制载荷（F-03）。
    let binary = CastMessage {
        protocol_version: 0,
        source_id: TEST_SENDER_ID.into(),
        destination_id: RECEIVER_PLATFORM_ID.into(),
        namespace: NS_CONNECTION.into(),
        payload: Payload::Binary(vec![1, 2, 3]),
    };
    let frame = binary.encode_frame().expect("可成帧");
    assert_eq!(CastMessage::decode_frame(&frame).unwrap().0, binary);
    // 控制面拒绝 BINARY（F-03：控制消息是 STRING）。
    assert_eq!(
        decode_message(&binary).unwrap_err().code,
        ErrorCode::UnsupportedFeature
    );

    // required 字段缺失：只写 protocol_version（field 1），其余 required 字段全缺。
    let truncated = vec![0x08, 0x00];
    assert!(CastMessage::decode_body(&truncated).is_err());

    // 未知协议版本。
    let mut bad_version = message.clone();
    bad_version.protocol_version = 9;
    assert_eq!(
        bad_version.encode_body().unwrap_err().code,
        ErrorCode::InvalidFrame
    );

    // 长度前缀声明超限 → 分配前拒绝。
    let mut huge = vec![0xff, 0xff, 0xff, 0xff];
    huge.extend_from_slice(&[0u8; 8]);
    assert_eq!(
        CastMessage::decode_frame(&huge).unwrap_err().code,
        ErrorCode::ResourceLimit
    );
    assert_eq!(MAX_BODY_BYTES, 65_536, "F-04 的 64 KiB 上限");

    // 长度前缀与实际不符 → 截断。
    let frame = message.encode_frame().expect("可成帧");
    assert_eq!(
        CastMessage::decode_frame(&frame[..frame.len() - 1]).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
}

#[test]
fn chunked_fields_are_refused() {
    // F-05：continued / remaining_length 一律 unsupported-feature。
    let message = CastPayload::Ping
        .into_message(TEST_SENDER_ID, RECEIVER_PLATFORM_ID, None)
        .expect("可编码");
    let mut body = message.encode_body().expect("可编码");
    body.push(0x40); // field 8 (continued), varint
    body.push(0x01);
    assert_eq!(
        CastMessage::decode_body(&body).unwrap_err().code,
        ErrorCode::UnsupportedFeature
    );
    let mut body = message.encode_body().expect("可编码");
    body.push(0x48); // field 9 (remaining_length), varint
    body.push(0x10);
    assert_eq!(
        CastMessage::decode_body(&body).unwrap_err().code,
        ErrorCode::UnsupportedFeature
    );
}

#[test]
fn payload_namespaces_and_negatives() {
    // connection
    let connect = CastPayload::Connect {
        user_agent: "xross-interop/0.1".into(),
        sender_info: proto_cast::SenderInfo {
            sdk_type: 2,
            version: "0.1".into(),
            browser_version: "1".into(),
            platform: 0,
            connection_type: 0,
        },
    };
    let text = connect.encode(Some(7)).expect("可编码");
    assert!(text.contains("\"type\":\"CONNECT\""), "{text}");
    assert!(text.contains("\"origin\":{}"));
    assert!(text.contains("\"userAgent\""));
    assert!(text.contains("\"senderInfo\""));
    assert!(text.contains("\"sdkType\":2"));
    let (decoded, request_id) = CastPayload::decode(NS_CONNECTION, &text).expect("可解码");
    assert_eq!(request_id, Some(7));
    assert_eq!(decoded, connect);

    // media：LOAD 的键集合（F-12）。
    let load = CastPayload::Load(proto_cast::LoadRequest {
        content_id: TEST_MEDIA_URL.into(),
        content_type: TEST_CONTENT_TYPE.into(),
        stream_type: StreamType::Buffered,
        title: Some("clip".into()),
        autoplay: true,
        current_time: None,
    });
    let text = load.encode(Some(9)).expect("可编码");
    assert!(text.contains("\"contentId\""), "{text}");
    assert!(text.contains("\"contentType\""));
    assert!(text.contains("\"streamType\":\"BUFFERED\""));
    assert!(text.contains("\"autoplay\":true"));
    assert!(text.contains("\"customData\":{}"));
    assert!(!text.contains("duration"), "F-12：LOAD 不写 duration");
    assert_eq!(CastPayload::decode(NS_MEDIA, &text).unwrap().0, load);

    // SEEK 的 resumeState（F-13）。
    let seek = CastPayload::Seek {
        current_time: 12.5,
        resume_state: "PLAYBACK_START",
    };
    let text = seek.encode(Some(10)).expect("可编码");
    assert!(text.contains("\"resumeState\":\"PLAYBACK_START\""));
    assert_eq!(CastPayload::decode(NS_MEDIA, &text).unwrap().0, seek);
    let bad = r#"{"type":"SEEK","currentTime":1.0,"resumeState":"PLAYBACK_PAUSE"}"#;
    assert_eq!(
        CastPayload::decode(NS_MEDIA, bad).unwrap_err().code,
        ErrorCode::InvalidFrame
    );

    // LOAD 缺 media / contentId；customData 非对象（F-12）。
    for bad in [
        r#"{"type":"LOAD","autoplay":true}"#,
        r#"{"type":"LOAD","media":{"contentType":"video/mp4","streamType":"BUFFERED"}}"#,
        r#"{"type":"LOAD","media":{"contentId":"http://h/a.mp4","contentType":"","streamType":"BUFFERED"}}"#,
        r#"{"type":"LOAD","media":{"contentId":"http://h/a.mp4","contentType":"video/mp4","streamType":"LIVE"},"customData":[]}"#,
        r#"{"type":"LOAD","media":{"contentId":"http://h/a.mp4","contentType":"video/mp4","streamType":"SIDEWAYS"}}"#,
    ] {
        assert_eq!(
            CastPayload::decode(NS_MEDIA, bad).unwrap_err().code,
            ErrorCode::InvalidFrame,
            "{bad}"
        );
    }

    // 未知 playerState / streamType（F-15）。
    assert_eq!(
        CastPayload::decode(NS_MEDIA, r#"{"type":"MEDIA_STATUS","playerState":"DANCING"}"#)
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    // 未知命名空间 / 已知命名空间的未知类型。
    assert_eq!(
        CastPayload::decode("urn:x-cast:com.example.custom", r#"{"type":"PING"}"#)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedFeature
    );
    assert_eq!(
        CastPayload::decode(NS_RECEIVER, r#"{"type":"QUEUE_INSERT"}"#)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedFeature
    );
    // 缺 type 键。
    assert_eq!(
        CastPayload::decode(NS_HEARTBEAT, r#"{"requestId":1}"#).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // 非对象载荷。
    assert!(CastPayload::decode(NS_HEARTBEAT, "[1,2]").is_err());
    // 非法 JSON。
    assert!(CastPayload::decode(NS_HEARTBEAT, "{oops}").is_err());
}

// ------------------------------------------------------------------ 请求关联与心跳

#[test]
fn request_id_correlation_and_timeouts() {
    let mut controller = controller(Box::new(OpenGateForTesting));
    let connect = controller.connect(0).expect("CONNECT");
    let request_id = decode_message(&connect.outbound[0]).unwrap().1.unwrap();
    assert_eq!(request_id, 1, "requestId 逐条递增（F-16）");

    // 伪造的 requestId → 明确拒绝。
    let forged = inbound(RECEIVER_PLATFORM_ID, TEST_SENDER_ID, CastPayload::Connected, Some(999));
    assert_eq!(
        controller.on_message(&forged, 10).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // 正常应答关账。
    let ok = inbound(RECEIVER_PLATFORM_ID, TEST_SENDER_ID, CastPayload::Connected, Some(request_id));
    controller.on_message(&ok, 20).expect("可关账");
    assert_eq!(controller.stats().pending, 0);

    // 未应答的请求在超时后产生事件（F-16 的 10 s 实现取值）。
    controller.launch(TEST_APP_ID, 30).expect("LAUNCH");
    assert_eq!(controller.stats().pending, 1);
    let outcome = controller.tick(100).expect("推进");
    assert!(outcome.events.is_empty(), "未到超时不该产生事件");
    let outcome = controller.tick(30 + proto_cast::REQUEST_TIMEOUT_MS + 1).expect("推进");
    assert!(outcome
        .events
        .iter()
        .any(|event| matches!(event, Event::RequestTimedOut { .. })));
    assert_eq!(controller.stats().timeouts, 1);
}

#[test]
fn heartbeat_ping_pong_and_expiry() {
    let mut controller = controller(Box::new(OpenGateForTesting));
    controller.connect(1_000).expect("CONNECT");
    let connected = inbound(RECEIVER_PLATFORM_ID, TEST_SENDER_ID, CastPayload::Connected, Some(1));
    controller.on_message(&connected, 1_000).expect("CONNECTED");

    // 到点发 PING（F-09 的 10 s 取值）。
    let early = controller.tick(1_000 + HEARTBEAT_PING_MS - 1).expect("推进");
    assert!(early.outbound.is_empty());
    let due = controller.tick(1_000 + HEARTBEAT_PING_MS).expect("推进");
    assert_eq!(due.outbound.len(), 1);
    assert_eq!(due.outbound[0].namespace, NS_HEARTBEAT);
    assert!(msg_text(&due.outbound[0]).contains("PING"));
    assert_eq!(
        decode_message(&due.outbound[0]).unwrap().1,
        None,
        "心跳不注入 requestId"
    );

    // 收到 PING 要回 PONG（回给发件人）。
    let ping = inbound(RECEIVER_PLATFORM_ID, TEST_SENDER_ID, CastPayload::Ping, None);
    let outcome = controller.on_message(&ping, 1_000 + HEARTBEAT_PING_MS + 1).expect("可处理");
    assert_eq!(outcome.outbound.len(), 1);
    assert!(msg_text(&outcome.outbound[0]).contains("PONG"));
    assert_eq!(outcome.outbound[0].destination_id, RECEIVER_PLATFORM_ID);
    assert_eq!(controller.stats().pings, 1);

    // 长时间无入站消息 → 连接失效。
    let outcome = controller
        .tick(1_000 + HEARTBEAT_PING_MS + 1 + proto_cast::HEARTBEAT_EXPIRY_MS + 1)
        .expect("推进");
    assert!(outcome.events.contains(&Event::ChannelExpired));
    assert_eq!(controller.state(), ControllerState::Ended);
}

#[test]
fn media_url_shape_is_validated() {
    assert!(validate_media_url("http://192.0.2.30:8000/a.mp4").is_ok());
    assert!(validate_media_url("https://cast.example/a/b.mp4?token=1").is_ok());
    for bad in [
        "file:///etc/passwd",
        "http://user:pass@192.0.2.30/a.mp4",
        "http:///a.mp4",
        "rtsp://192.0.2.30/a.mp4",
    ] {
        assert_eq!(validate_media_url(bad).unwrap_err().code, ErrorCode::InvalidFrame, "{bad}");
    }
    // LOAD 之前就校验：非法 URL 不进入消息。
    let mut controller = launched_controller();
    assert!(controller.load("file:///etc/passwd", "video/mp4", None, 9_000).is_err());
    assert_eq!(controller.media_url(), Some(TEST_MEDIA_URL), "合法 URL 未被替换");
}

// ------------------------------------------------------------------ 发现解析

#[test]
fn discovery_txt_parsing() {
    let info = ReceiverInfo::from_txt(
        "_googlecast._tcp.local.",
        &txt(&[
            ("id", "0000aaaa-bbbb-cccc-dddd-eeeeffff0000"),
            ("fn", "客厅那块屏"),
            ("md", "Chromecast Ultra"),
            ("ve", "05"),
            ("st", "0"),
            ("ca", "5"),
            ("unknown-key", "ignored"),
        ]),
        DEFAULT_PORT,
    )
    .expect("可解析");
    assert_eq!(info.friendly_name, "客厅那块屏");
    assert_eq!(info.status, Some(DeviceStatus::Idle));
    assert_eq!(info.capability_bits, Some(5));
    assert_eq!(info.port, DEFAULT_PORT);
    assert_eq!(SERVICE_TYPE, "_googlecast._tcp");

    let busy = ReceiverInfo::from_txt(
        "_googlecast._tcp.local.",
        &txt(&[("id", "x"), ("fn", "y"), ("st", "1")]),
        8009,
    )
    .expect("可解析");
    assert_eq!(busy.status, Some(DeviceStatus::Busy));

    // 负向：非本服务类型、缺 id/fn、st 非法、ca 非法。
    for (service, pairs) in [
        ("_airplay._tcp.local.", txt(&[("id", "x"), ("fn", "y")])),
        ("_googlecast._tcp.local.", txt(&[("fn", "y")])),
        ("_googlecast._tcp.local.", txt(&[("id", "x")])),
        ("_googlecast._tcp.local.", txt(&[("id", ""), ("fn", "y")])),
        ("_googlecast._tcp.local.", txt(&[("id", "x"), ("fn", "y"), ("st", "7")])),
        ("_googlecast._tcp.local.", txt(&[("id", "x"), ("fn", "y"), ("ca", "zz")])),
    ] {
        assert_eq!(
            ReceiverInfo::from_txt(service, &pairs, 8009).unwrap_err().code,
            ErrorCode::InvalidFrame,
            "{service} {pairs:?}"
        );
    }
}
