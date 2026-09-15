//! Google Cast 场景（T43）：CASTV2 信封、载荷键、sender 会话与四条 T43 约束的本地闭环。
//!
//! 诚实前提：**没有网络、没有 TLS、没有真实设备**——不连接任何 Chromecast，也不做设备认证
//! （认证材料本仓不可得，生产闸门恒拒绝）。所有字节与 app ID / URL / TXT 值都是自制占位：
//! 这些数字证明的是"信封与载荷编解码、控制顺序、清理路径、能力声明正确"，
//! 不是"投到你家电视上了"。真机认证强制点（P-M08-1）在 blocked 清单里。

use crate::report::CastReport;
use proto_cast::castv2::CastMessage;
use proto_cast::controller::{
    validate_media_url, Controller, Event, OpenGateForTesting, UnavailableGate, HEARTBEAT_PING_MS,
    REQUEST_TIMEOUT_MS,
};
use proto_cast::discovery::{DeviceStatus, ReceiverInfo, DEFAULT_PORT};
use proto_cast::namespaces::{
    decode_message, ApplicationStatus, CastPayload, MediaStatus, PlayerState, ReceiverStatus,
    Volume, NS_CONNECTION, NS_HEARTBEAT, NS_MEDIA, NS_RECEIVER,
};
use proto_cast::RECEIVER_PLATFORM_ID;
use serde_json::json;

/// 自制占位（不是任何真实注册 app ID）。
const DEMO_APP_ID: &str = "DEMOAPP1";
const DEMO_SENDER_ID: &str = "sender-demo";
const DEMO_MEDIA_URL: &str = "http://192.0.2.30:8000/media/clip.mp4";
const DEMO_CONTENT_TYPE: &str = "video/mp4";

fn inbound(source: &str, destination: &str, payload: CastPayload, request_id: Option<u64>) -> CastMessage {
    payload
        .into_message(source, destination, request_id)
        .expect("可编码")
}

fn failed_report() -> CastReport {
    CastReport {
        evidence_level: crate::report::EVIDENCE_LEVEL,
        wire: crate::report::WIRE,
        envelope: json!({}),
        namespaces: json!({}),
        session: json!({}),
        receiver: json!({}),
        discovery: json!({}),
        rejects: Vec::new(),
        blocked: Vec::new(),
    }
}

/// 统一把 `Result` 折成错误码字符串（被接受 = 负向失败）。
fn code_of<T>(result: Result<T, interop_contract::error::Error>) -> String {
    result
        .map(|_| "accepted(unexpected)".to_string())
        .unwrap_or_else(|e| format!("{:?}", e.code))
}

pub fn cast_scenario() -> (CastReport, bool) {
    let mut ok = true;
    let mut rejects: Vec<serde_json::Value> = Vec::new();
    let mut negative = |label: &str, code: String| {
        rejects.push(json!({ "case": label, "outcome": code }));
    };

    // ---- 信封：字段号往返 + 上限 + 分块 ----
    let ping = CastPayload::Ping
        .into_message(DEMO_SENDER_ID, RECEIVER_PLATFORM_ID, None)
        .expect("可编码");
    let frame = ping.encode_frame().expect("可成帧");
    let roundtrip = CastMessage::decode_frame(&frame)
        .map(|(decoded, consumed)| decoded == ping && consumed == frame.len())
        .unwrap_or(false);

    let binary = CastMessage {
        protocol_version: 0,
        source_id: DEMO_SENDER_ID.into(),
        destination_id: RECEIVER_PLATFORM_ID.into(),
        namespace: NS_CONNECTION.into(),
        payload: proto_cast::Payload::Binary(vec![0xde, 0xad]),
    };
    let binary_roundtrip = CastMessage::decode_frame(&binary.encode_frame().expect("可成帧"))
        .map(|(decoded, _)| decoded == binary)
        .unwrap_or(false);
    let binary_on_control = code_of(decode_message(&binary));

    let mut chunked = ping.encode_body().expect("可编码");
    chunked.push(0x40); // field 8 = continued
    chunked.push(0x01);
    let chunked_refused = code_of(CastMessage::decode_body(&chunked));

    let mut huge = vec![0xff, 0xff, 0xff, 0xff];
    huge.extend_from_slice(&[0u8; 8]);
    let oversize = code_of(CastMessage::decode_frame(&huge));

    // 未知协议版本。
    let mut bad_version = ping.clone();
    bad_version.protocol_version = 9;
    let bad_version_code = code_of(bad_version.encode_body());
    if !(roundtrip && binary_roundtrip) {
        ok = false;
    }

    let envelope = json!({
        "body_bytes": ping.encode_body().map(|b| b.len()).unwrap_or(0),
        "frame_bytes": frame.len(),
        "roundtrip": roundtrip,
        "binary_roundtrip": binary_roundtrip,
        "binary_on_control_plane": binary_on_control,
        "chunked": chunked_refused,
        "oversize": oversize,
        "version_whitelist": format!("{:?}", proto_cast::PROTOCOL_VERSIONS),
        "bad_version": bad_version_code,
        "max_body_bytes": proto_cast::MAX_BODY_BYTES,
    });

    // ---- 四个命名空间的载荷键 ----
    let connect_text = CastPayload::Connect {
        user_agent: "xross-interop/0.1".into(),
        sender_info: proto_cast::SenderInfo {
            sdk_type: 2,
            version: "0.1".into(),
            browser_version: "1".into(),
            platform: 0,
            connection_type: 0,
        },
    }
    .encode(Some(1))
    .expect("可编码");
    let load = CastPayload::Load(proto_cast::LoadRequest {
        content_id: DEMO_MEDIA_URL.into(),
        content_type: DEMO_CONTENT_TYPE.into(),
        stream_type: proto_cast::StreamType::Buffered,
        title: Some("demo clip".into()),
        autoplay: true,
        current_time: None,
    });
    let load_text = load.encode(Some(2)).expect("可编码");
    let seek_text = CastPayload::Seek {
        current_time: 12.5,
        resume_state: "PLAYBACK_START",
    }
    .encode(Some(3))
    .expect("可编码");
    let namespaces = json!({
        "connection": {
            "namespace": NS_CONNECTION,
            "types": ["CONNECT", "CLOSE", "CONNECTED"],
            "connect_payload": connect_text,
        },
        "heartbeat": { "namespace": NS_HEARTBEAT, "types": ["PING", "PONG"] },
        "receiver": {
            "namespace": NS_RECEIVER,
            "types": ["LAUNCH", "GET_STATUS", "STOP", "SET_VOLUME", "RECEIVER_STATUS", "LAUNCH_ERROR"],
        },
        "media": {
            "namespace": NS_MEDIA,
            "types": ["LOAD", "PLAY", "PAUSE", "STOP", "SEEK", "GET_STATUS", "MEDIA_STATUS", "LOAD_FAILED"],
            "load_payload": load_text,
            "load_writes_duration": load_text.contains("duration"),
            "seek_payload": seek_text,
        },
    });
    if load_text.contains("duration") {
        ok = false; // F-12：LOAD 不写 duration
    }

    // ---- sender 会话：连接 → 拉起 → 载入 → 播放 → 停止 ----
    let mut steps: Vec<String> = Vec::new();
    let mut controller = match Controller::new(
        Box::new(OpenGateForTesting),
        DEMO_SENDER_ID,
        vec![DEMO_APP_ID.to_string()],
    ) {
        Ok(controller) => controller,
        Err(_) => return (failed_report(), false),
    };
    // 生产闸门（认证不可得）：连 CONNECT 都发不出去。
    let mut blocked_controller = match Controller::new(
        Box::new(UnavailableGate),
        DEMO_SENDER_ID,
        vec![DEMO_APP_ID.to_string()],
    ) {
        Ok(controller) => controller,
        Err(_) => return (failed_report(), false),
    };
    let gate_closed = code_of(blocked_controller.connect(0));
    let gate_sent = blocked_controller.stats().sent;
    if gate_closed != "VendorGated" || gate_sent != 0 {
        ok = false;
    }

    let connect = match controller.connect(1_000) {
        Ok(outcome) => outcome,
        Err(_) => return (failed_report(), false),
    };
    let connect_request = decode_message(&connect.outbound[0]).ok().and_then(|(_, id)| id);
    steps.push(format!(
        "CONNECT 出站 → {}（requestId {:?}）",
        connect.outbound[0].destination_id, connect_request
    ));
    let connected = inbound(RECEIVER_PLATFORM_ID, DEMO_SENDER_ID, CastPayload::Connected, connect_request);
    if let Ok(outcome) = controller.on_message(&connected, 1_050) {
        if outcome.events.contains(&Event::Connected) {
            steps.push("CONNECTED 入站 → connected".into());
        } else {
            ok = false;
        }
    }
    negative(
        "LAUNCH 未配置 app ID（controller）",
        code_of(controller.launch("UNKNOWN", 1_060)),
    );

    let launch = match controller.launch(DEMO_APP_ID, 1_100) {
        Ok(outcome) => outcome,
        Err(_) => return (failed_report(), false),
    };
    let launch_request = decode_message(&launch.outbound[0]).ok().and_then(|(_, id)| id);
    let status = inbound(
        RECEIVER_PLATFORM_ID,
        DEMO_SENDER_ID,
        CastPayload::ReceiverStatus(ReceiverStatus {
            applications: vec![ApplicationStatus {
                app_id: DEMO_APP_ID.into(),
                display_name: Some("Demo Receiver".into()),
                session_id: Some("session-demo".into()),
                transport_id: Some("transport-demo".into()),
                namespaces: vec![NS_MEDIA.into()],
                status_text: None,
            }],
            volume: Some(Volume {
                level: Some(0.4),
                muted: Some(false),
            }),
            is_stand_by: Some(false),
        }),
        launch_request,
    );
    match controller.on_message(&status, 1_150) {
        Ok(outcome) => {
            if let Some(Event::Launched {
                app_id,
                transport_id,
                session_id,
            }) = outcome.events.first()
            {
                steps.push(format!(
                    "LAUNCH → RECEIVER_STATUS → app={app_id} transport={:?} session={:?}",
                    transport_id, session_id
                ));
            }
        }
        Err(_) => ok = false,
    }

    let load_out = match controller.load(DEMO_MEDIA_URL, DEMO_CONTENT_TYPE, Some("demo clip"), 1_200) {
        Ok(outcome) => outcome,
        Err(_) => return (failed_report(), false),
    };
    let load_request = decode_message(&load_out.outbound[0]).ok().and_then(|(_, id)| id);
    steps.push(format!(
        "LOAD 出站 → {}（媒体命令发往 transportId）",
        load_out.outbound[0].destination_id
    ));
    let media_status = inbound(
        "transport-demo",
        DEMO_SENDER_ID,
        CastPayload::MediaStatus(Box::new(MediaStatus {
            media_session_id: Some(7),
            player_state: PlayerState::Playing,
            idle_reason: None,
            current_time: Some(1.5),
            media: None,
            volume: None,
        })),
        load_request,
    );
    match controller.on_message(&media_status, 1_250) {
        Ok(outcome) => {
            if outcome.events.iter().any(|e| matches!(e, Event::MediaStatus(_))) {
                steps.push(format!(
                    "MEDIA_STATUS 入站 → playerState=PLAYING mediaSessionId={:?}",
                    controller.media_session_id()
                ));
            }
        }
        Err(_) => ok = false,
    }
    let play = controller.play(1_300).ok().map(|o| o.outbound.len()).unwrap_or(0);
    let seek = controller.seek(30.0, 1_320).ok().map(|o| o.outbound.len()).unwrap_or(0);
    let pause = controller.pause(1_340).ok().map(|o| o.outbound.len()).unwrap_or(0);
    if play + seek + pause != 3 {
        ok = false;
    }
    steps.push("PLAY/SEEK/PAUSE 各 1 条（media 会话内）".to_string());

    // 停止 → URL 必须释放（T43-03）。
    let stop = match controller.stop(1_400) {
        Ok(outcome) => outcome,
        Err(_) => return (failed_report(), false),
    };
    let stop_released = stop.events.iter().any(|e| matches!(
        e,
        Event::MediaUrlReleased { why: "stop", .. }
    ));
    if !stop_released {
        ok = false;
    }
    steps.push("STOP → MediaUrlReleased(why=stop)，mediaSessionId 清空".into());

    // 接收端把应用停掉 → 也要释放。
    let mut second = demo_session();
    let terminated = inbound(
        RECEIVER_PLATFORM_ID,
        DEMO_SENDER_ID,
        CastPayload::ReceiverStatus(ReceiverStatus {
            applications: Vec::new(),
            volume: None,
            is_stand_by: Some(true),
        }),
        None,
    );
    let receiver_stopped = second
        .on_message(&terminated, 2_000)
        .map(|outcome| {
            outcome
                .events
                .iter()
                .any(|e| matches!(e, Event::MediaUrlReleased { why: "receiver-stopped", .. }))
        })
        .unwrap_or(false);
    if !receiver_stopped {
        ok = false;
    }

    // 心跳与超时（都走时间推进，不做真实等待）。
    let mut hb = connected_controller();
    let ping_due = hb
        .tick(1_300 + HEARTBEAT_PING_MS)
        .map(|outcome| outcome.outbound.len())
        .unwrap_or(0);
    let hb_ping = inbound(RECEIVER_PLATFORM_ID, DEMO_SENDER_ID, CastPayload::Ping, None);
    let pong = hb
        .on_message(&hb_ping, 1_300 + HEARTBEAT_PING_MS + 1)
        .map(|outcome| outcome.outbound.len())
        .unwrap_or(0);
    if ping_due != 1 || pong != 1 {
        ok = false;
    }
    // 未应答请求超时。
    let mut timeout_controller = connected_controller();
    timeout_controller
        .launch(DEMO_APP_ID, 3_000)
        .expect("LAUNCH 可发");
    let timed_out = timeout_controller
        .tick(3_000 + REQUEST_TIMEOUT_MS + 1)
        .map(|outcome| {
            outcome
                .events
                .iter()
                .any(|e| matches!(e, Event::RequestTimedOut { .. }))
        })
        .unwrap_or(false);
    if !timed_out {
        ok = false;
    }

    let session = json!({
        "steps": steps,
        "state": controller.state().as_str(),
        "gate_closed_code": gate_closed,
        "gate_closed_sent": gate_sent,
        "media_url_held_after_stop": controller.stats().media_url_held,
        "screen_capability": controller.screen_capability(),
        "receiver_terminated_released": receiver_stopped,
        "heartbeat_ping_due": ping_due,
        "heartbeat_pong": pong,
        "request_timeout": timed_out,
        "sent": controller.stats().sent,
        "received": controller.stats().received,
    });

    // ---- 发现解析（只解析，不组播） ----
    let txt: Vec<(String, String)> = [
        ("id", "0000aaaa-bbbb-cccc-dddd-eeeeffff0000"),
        ("fn", "客厅那块屏"),
        ("md", "Chromecast Ultra"),
        ("ve", "05"),
        ("st", "0"),
        ("ca", "5"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let receiver_info = ReceiverInfo::from_txt("_googlecast._tcp.local.", &txt, DEFAULT_PORT);
    let discovery_note = match &receiver_info {
        Ok(info) => format!(
            "{} / {} / st={:?} / ca={:?} / port={}",
            info.friendly_name,
            info.model_name.clone().unwrap_or_default(),
            info.status,
            info.capability_bits,
            info.port
        ),
        Err(_) => {
            ok = false;
            "解析失败".to_string()
        }
    };
    if receiver_info.as_ref().ok().and_then(|i| i.status) != Some(DeviceStatus::Idle) {
        ok = false;
    }

    // ---- 负向集 ----
    negative("控制面 BINARY 载荷", binary_on_control);
    negative("分块（continued）", chunked_refused);
    negative("长度前缀超 64 KiB", oversize);
    negative("未知协议版本", bad_version_code);
    negative("未知命名空间", code_of(CastPayload::decode("urn:x-cast:com.example.x", r#"{"type":"PING"}"#)));
    negative("已登记命名空间的未登记类型", code_of(CastPayload::decode(NS_RECEIVER, r#"{"type":"QUEUE_INSERT"}"#)));
    negative("未知 playerState", code_of(CastPayload::decode(NS_MEDIA, r#"{"type":"MEDIA_STATUS","playerState":"DANCING"}"#)));
    negative("SEEK resumeState 非 PLAYBACK_START", code_of(CastPayload::decode(NS_MEDIA, r#"{"type":"SEEK","currentTime":1.0,"resumeState":"PLAYBACK_PAUSE"}"#)));
    negative("LOAD 缺 media", code_of(CastPayload::decode(NS_MEDIA, r#"{"type":"LOAD","autoplay":true}"#)));
    negative("LOAD 的 contentId 为空", code_of(CastPayload::decode(NS_MEDIA, r#"{"type":"LOAD","media":{"contentId":"","contentType":"video/mp4","streamType":"BUFFERED"}}"#)));
    negative("载荷缺 type 键", code_of(CastPayload::decode(NS_HEARTBEAT, r#"{"requestId":1}"#)));
    negative("媒体 URL 形状非法", code_of(validate_media_url("file:///etc/passwd")));
    negative("TXT 缺 id", code_of(ReceiverInfo::from_txt(
        "_googlecast._tcp.local.",
        &[( "fn".to_string(), "x".to_string())],
        DEFAULT_PORT,
    )));
    negative("非本服务类型", code_of(ReceiverInfo::from_txt(
        "_airplay._tcp.local.",
        &txt,
        DEFAULT_PORT,
    )));
    // 未认证就发命令（生产闸门）。
    negative("生产闸门下的 CONNECT", gate_closed.clone());
    negative("生产闸门下的 LOAD", code_of(blocked_controller.load(DEMO_MEDIA_URL, DEMO_CONTENT_TYPE, None, 0)));

    // ---- receiver 侧（T45）：产品路径恒拒绝；test-root 演示闭环单独记账 ----
    let receiver = run_receiver_path(&mut ok);

    let report = CastReport {
        evidence_level: crate::report::EVIDENCE_LEVEL,
        wire: crate::report::WIRE,
        envelope,
        namespaces,
        session,
        receiver,
        discovery: json!({
            "service_type": proto_cast::SERVICE_TYPE,
            "port": DEFAULT_PORT,
            "txt_keys": ["id", "fn", "md", "ve", "st", "ca"],
            "parsed": discovery_note,
            "multicast": false,
        }),
        rejects,
        blocked: vec![
            "设备认证握手（F-18：设备侧凭据材料不可得）——生产闸门恒拒绝，因此没有任何真机通道".into(),
            "app 注册与 app ID 签发（F-17/F-23：注册体系在 Google 侧；本仓只用调用方配置的 ID）".into(),
            "TLS 传输层与真实 Chromecast/Google TV 的接受条件（P-M08-1）".into(),
            "媒体字节服务与实时 streaming（F-25 / T44）".into(),
            "CASTV2 分块（F-05：参考实现也没有实现路径，收到即拒绝）".into(),
            "TXT 字段矩阵与各代设备差异（P-M08-3）".into(),
            "stock sender 连上本机 receiver（F-28/F-33）：认证在消息层、默认 sender 只信厂商根；重评条件 = P-M08-1 + P-M08-4（Chrome 开发者证书参数）".into(),
            "CAF / 托管 receiver app（F-32：来源未固化 → 不得声称可行；重评条件 = P-M08-2）".into(),
        ],
    };
    (report, ok)
}

/// 演示用：只走到 connected 的控制器。
fn connected_controller() -> Controller {
    let mut controller = Controller::new(
        Box::new(OpenGateForTesting),
        DEMO_SENDER_ID,
        vec![DEMO_APP_ID.to_string()],
    )
    .expect("控制器");
    controller.connect(1_000).expect("CONNECT");
    // 首条命令的 requestId 从 1 开始（F-16：逐条单调）。
    let connected = inbound(
        RECEIVER_PLATFORM_ID,
        DEMO_SENDER_ID,
        CastPayload::Connected,
        Some(1),
    );
    let _ = controller.on_message(&connected, 1_010);
    controller
}

/// 演示用：跑完到"媒体会话中"的控制器（自制对端回复）。
fn demo_session() -> Controller {
    let mut controller = connected_controller();
    let launch = controller.launch(DEMO_APP_ID, 1_020).expect("LAUNCH");
    let launch_request = decode_message(&launch.outbound[0]).ok().and_then(|(_, id)| id);
    let status = inbound(
        RECEIVER_PLATFORM_ID,
        DEMO_SENDER_ID,
        CastPayload::ReceiverStatus(ReceiverStatus {
            applications: vec![ApplicationStatus {
                app_id: DEMO_APP_ID.into(),
                display_name: None,
                session_id: Some("session-demo".into()),
                transport_id: Some("transport-demo".into()),
                namespaces: vec![NS_MEDIA.into()],
                status_text: None,
            }],
            volume: None,
            is_stand_by: None,
        }),
        launch_request,
    );
    let _ = controller.on_message(&status, 1_030);
    let load = controller
        .load(DEMO_MEDIA_URL, DEMO_CONTENT_TYPE, None, 1_040)
        .expect("LOAD");
    let load_request = decode_message(&load.outbound[0]).ok().and_then(|(_, id)| id);
    let media_status = inbound(
        "transport-demo",
        DEMO_SENDER_ID,
        CastPayload::MediaStatus(Box::new(MediaStatus {
            media_session_id: Some(7),
            player_state: PlayerState::Playing,
            idle_reason: None,
            current_time: Some(0.5),
            media: None,
            volume: None,
        })),
        load_request,
    );
    let _ = controller.on_message(&media_status, 1_050);
    controller
}

/// receiver 侧路径（T45）：把"产品恒拒绝"与"test-root 自配对闭环"分别跑出来。
///
/// 这里证明的是：**门禁与状态机按字段表工作**，不是"能接 stock sender"。
fn run_receiver_path(ok: &mut bool) -> serde_json::Value {
    use proto_cast::receiver::{
        LaunchOutcome, ReceiverInfo, ReceiverSession, ReceiverState, KNOWN_TXT_KEYS,
        PORT_REAL_DEVICE, PORT_REFERENCE_RECEIVER, SERVICE_TYPE, STATUS_BUSY_JOIN, STATUS_IDLE,
    };

    // 1) 产品路径：即便"对端信任我们"也拒绝（材料边界），且不留半开会话。
    let mut vendor = ReceiverSession::new();
    vendor.note_sender_trust(true);
    let vendor_refusal = code_of(vendor.on_connect(Some(3)));
    let vendor_state = format!("{:?}", vendor.state());
    let vendor_senders = vendor.accounting().connected_senders;
    if vendor_refusal != "VendorGated" || vendor_senders != 0 {
        *ok = false;
    }

    // 2) 演示路径：对方不信任测试根 → 拒绝；显式信任 → 闭环（CONNECT→LAUNCH→状态→CLOSE）。
    const DEMO_APP: &str = "DEMOAPP";
    let mut demo = ReceiverSession::test_root_for_demo(vec![DEMO_APP.to_string()]);
    let untrusted = code_of(demo.on_connect(Some(3)));
    demo.note_sender_trust(true);
    let connected = matches!(demo.on_connect(Some(3)), Ok(Some(_)));
    let no_version_reply = matches!(demo.on_connect(None), Ok(None));
    let launched = matches!(demo.on_launch(DEMO_APP), Ok(LaunchOutcome::Launched { .. }));
    let status = match demo.status() {
        proto_cast::namespaces::CastPayload::ReceiverStatus(s) => json!({
            "app_id": s.app_id(),
            "transport_id": s.transport_id(),
            "session_id": s.session_id(),
        }),
        _ => {
            *ok = false;
            serde_json::Value::Null
        }
    };
    let refused_launch = match demo.on_launch("NOT-CONFIGURED") {
        Ok(LaunchOutcome::Refused { code, .. }) => format!("{code:?}"),
        Ok(other) => {
            *ok = false;
            format!("accepted(unexpected): {other:?}")
        }
        Err(e) => {
            *ok = false;
            format!("{:?}", e.code)
        }
    };
    let pong = demo.on_ping().is_ok();
    let info_busy = ReceiverInfo::from_state(&demo, "abcd1234", "客厅", "xross-demo");
    let txt = info_busy.to_txt();
    let txt_roundtrip = ReceiverInfo::from_txt(&txt).ok() == Some(info_busy.clone());
    let unknown_key_refused = code_of(ReceiverInfo::from_txt(
        &txt.iter()
            .cloned()
            .chain(std::iter::once(("rs".to_string(), "1".to_string())))
            .collect::<Vec<_>>(),
    ));
    demo.on_close();
    let closed_released = !demo.accounting().holding_app && demo.state() == ReceiverState::Closed;
    if untrusted != "VendorGated"
        || !connected
        || !no_version_reply
        || !launched
        || refused_launch != "DestinationUnavailable"
        || !pong
        || !txt_roundtrip
        || unknown_key_refused != "InvalidFrame"
        || !closed_released
        || vendor_state != "Idle"
    {
        *ok = false;
    }
    if !(info_busy.status == STATUS_BUSY_JOIN
        && ReceiverInfo::from_state(&ReceiverSession::new(), "x", "y", "z").status == STATUS_IDLE)
    {
        *ok = false;
    }

    json!({
        "vendor_path": {
            "gate": vendor.gate_name(),
            "even_if_peer_trusts_us": vendor_refusal,
            "state_after_refusal": vendor_state,
            "senders_admitted": vendor_senders,
        },
        "test_root_path": {
            "gate": demo.gate_name(),
            "refused_when_peer_does_not_trust_test_root": untrusted,
            "connected": connected,
            "connected_only_with_protocol_version": no_version_reply,
            "launched": launched,
            "status": status,
            "refused_unconfigured_app": refused_launch,
            "pong": pong,
            "closed_released_app": closed_released,
            "stock_compatible": false,
            "note": "test-root 仅用于本仓自配对闭环；stock sender 默认不信任它（F-28/F-34）",
        },
        "txt": {
            "service_type": SERVICE_TYPE,
            "keys": KNOWN_TXT_KEYS,
            "roundtrip": txt_roundtrip,
            "unknown_key_refused": unknown_key_refused,
            "port_real_device": PORT_REAL_DEVICE,
            "port_reference_receiver": PORT_REFERENCE_RECEIVER,
        },
    })
}
