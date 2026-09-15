//! WFD/Miracast 场景（T37）：消息层/协商/子元素/RTP 记账的本地闭环。
//!
//! 诚实前提：**没有无线、没有 P2P、没有真实发送端**——不发 RTSP 请求、不开 socket、
//! 不构造 WFD IE 容器（OUI 未固化，F-24）。所有字节都是自制 fixture：
//! 这些数字证明的是"编解码/顺序约束/协商取舍正确"，不是"能收到你手机的投屏"。
//! 真机序列（P-M05-2）、平台入口（P-M05-1）与真机 codec 矩阵（P-M05-3）在 blocked 清单里。

use crate::report::WfdReport;
use proto_wfd::ie::{DeviceInfoSubelement, WfdIeContainer};
use proto_wfd::messages::{classify, Request, Response, Role, WfdMessage, WFD_METHOD_SET};
use proto_wfd::negotiate::{
    parse_audio_codecs, parse_video_formats, RtpPorts, Transport, VideoFormatSet,
};
use proto_wfd::rtp::{parse_header, StreamAccount};
use proto_wfd::session::{ReceivedMediaSupport, SinkSession, SinkState, KEEPALIVE_INTERVAL_MS};
use proto_wfd::{AudioCodecSet, CONTROL_PORT, RTP_PORT_FALLBACK};
use serde_json::json;

/// 自制视频广告（演示用假设值；不代表本仓能解码/显示）。
const DEMO_VIDEO: &str =
    "28 00 02 10 00000020 00000000 00000000 00 0004 0001 00 none none";
/// 自制音频广告（LPCM bit0+bit1、AAC bit0）。
const DEMO_AUDIO: &str = "LPCM 00000003 00, AAC 00000001 00";

fn request(method: &str, uri: &str, cseq: u64, extra: &[(&str, &str)], body: &str) -> Request {
    let mut raw = format!("{method} {uri} RTSP/1.0\r\nCSeq: {cseq}\r\n");
    for (name, value) in extra {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        raw.push_str(&format!(
            "Content-Type: text/parameters\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    Request::parse(raw.as_bytes())
        .map(|(req, _)| req)
        .unwrap_or_else(|_| Request {
            method: String::new(),
            uri: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
        })
}

fn response(status: u16, cseq: u64, extra: &[(&str, &str)], body: &str) -> Response {
    let raw = proto_wfd::messages::build_response(status, "OK", cseq, extra, body);
    Response::parse(&raw)
        .map(|(resp, _)| resp)
        .unwrap_or_else(|_| Response {
            status: 0,
            reason: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
        })
}

fn rtp_packet(seq: u16, ssrc: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0x80, 33];
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&(u32::from(seq) * 3000).to_be_bytes());
    out.extend_from_slice(&ssrc.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn demo_support() -> ReceivedMediaSupport {
    let video = VideoFormatSet::parse(DEMO_VIDEO).expect("自制视频广告");
    let audio = AudioCodecSet::parse(DEMO_AUDIO).expect("自制音频广告");
    ReceivedMediaSupport::hypothetical(video, audio)
}

fn m4_body(rtp_ports: &str) -> String {
    format!(
        "wfd_video_formats: {DEMO_VIDEO}\r\n\
         wfd_audio_codecs: {DEMO_AUDIO}\r\n\
         wfd_presentation_URL: rtsp://192.0.2.10:7236/wfd1.0/streamid=0 none\r\n\
         wfd_client_rtp_ports: {rtp_ports}\r\n"
    )
}

fn failed_report() -> WfdReport {
    WfdReport {
        evidence_level: crate::report::EVIDENCE_LEVEL,
        wire: crate::report::WIRE,
        messages: json!({}),
        session: json!({}),
        negotiation: json!({}),
        ie: json!({}),
        rtp: json!({}),
        rejects: Vec::new(),
        blocked: Vec::new(),
    }
}

pub fn wfd_scenario() -> (WfdReport, bool) {
    let mut ok = true;

    // ---- 消息层：M 编号/方法/方向（F-02..F-11）----
    let mut table = Vec::new();
    for message in [
        WfdMessage::M1Options,
        WfdMessage::M2Options,
        WfdMessage::M3GetParameter,
        WfdMessage::M4SetParameter,
        WfdMessage::M5TriggerSetup,
        WfdMessage::M6Setup,
        WfdMessage::M7Play,
        WfdMessage::M8Teardown,
        WfdMessage::M13IdrRequest,
        WfdMessage::M16KeepAlive,
    ] {
        table.push(json!({
            "m": message.number(),
            "method": message.method(),
            "by": match message.originator() {
                Role::Source => "source",
                Role::Sink => "sink",
            },
        }));
    }
    // M1/M2 同形（同一份字节，按发送方区分）。
    let options_bytes = request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], "");
    let m1 = classify(&options_bytes, Role::Source);
    let m2 = classify(&options_bytes, Role::Sink);
    let same_shape = matches!(m1, Ok(WfdMessage::M1Options)) && matches!(m2, Ok(WfdMessage::M2Options));
    if !same_shape {
        ok = false;
    }

    // ---- 会话：M1→M2→M3→M4→M5→SETUP→PLAY→(缺口)M13→keep-alive→超时 ----
    let mut rejects: Vec<serde_json::Value> = Vec::new();
    let mut session = match SinkSession::new(demo_support(), 20000) {
        Ok(s) => s,
        Err(_) => return (failed_report(), false),
    };
    let mut steps: Vec<String> = Vec::new();
    let m1_req = request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], "");
    match session.on_request(&m1_req, 1_000) {
        Ok(resp) => {
            let public = resp
                .headers
                .iter()
                .find(|(k, _)| k == "Public")
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            steps.push(format!("M1 入站 → 200 Public={public}"));
        }
        Err(e) => {
            ok = false;
            steps.push(format!("M1 失败：{:?}", e.code));
        }
    }
    if let Ok(bytes) = session.options() {
        if let Ok((m2_req, _)) = Request::parse(&bytes) {
            let cseq = m2_req.cseq().unwrap_or(0);
            let resp = response(200, cseq, &[("Public", WFD_METHOD_SET)], "");
            if session.on_response(&resp, 1_050).is_ok() {
                steps.push(format!("M2 出站 → 200（CSeq {cseq}）"));
            } else {
                ok = false;
                steps.push("M2 应答处理失败".into());
            }
        }
    }
    let m3 = request(
        "GET_PARAMETER",
        "rtsp://localhost/wfd1.0",
        2,
        &[],
        "wfd_video_formats\r\nwfd_audio_codecs\r\nwfd_client_rtp_ports\r\n",
    );
    let advertised = match session.on_request(&m3, 1_100) {
        Ok(resp) => {
            steps.push("M3 入站 → 200（应答体 = 本端实现清单）".into());
            resp.body_str().unwrap_or("").to_string()
        }
        Err(e) => {
            ok = false;
            steps.push(format!("M3 失败：{:?}", e.code));
            String::new()
        }
    };
    let m4 = request(
        "SET_PARAMETER",
        "rtsp://localhost/wfd1.0",
        3,
        &[],
        &m4_body("RTP/AVP/UDP;unicast 20000 20002 mode=play"),
    );
    match session.on_request(&m4, 1_200) {
        Ok(_) => {
            if let Some(n) = session.negotiated() {
                steps.push(format!(
                    "M4 入站 → 协商：profile=0x{:02X} level=0x{:02X} 音频={} URL={}",
                    n.video.profile.0,
                    n.video.level.0,
                    n.audio.codec.name(),
                    n.presentation_url
                ));
            }
        }
        Err(e) => {
            ok = false;
            steps.push(format!("M4 失败：{:?}", e.code));
        }
    }
    let m5 = request(
        "SET_PARAMETER",
        "rtsp://localhost/wfd1.0",
        4,
        &[],
        "wfd_trigger_method: SETUP\r\n",
    );
    match session.on_request(&m5, 1_300) {
        Ok(_) => steps.push("M5 触发（wfd_trigger_method: SETUP）".into()),
        Err(e) => {
            ok = false;
            steps.push(format!("M5 失败：{:?}", e.code));
        }
    }
    let mut session_id = String::new();
    match session.setup() {
        Ok(bytes) => {
            if let Ok((setup_req, _)) = Request::parse(&bytes) {
                let cseq = setup_req.cseq().unwrap_or(0);
                steps.push(format!(
                    "M6 出站 SETUP Transport={}",
                    setup_req.transport().unwrap_or("")
                ));
                let resp = response(
                    200,
                    cseq,
                    &[
                        ("Session", "7123456;timeout=30"),
                        ("Transport", "RTP/AVP/UDP;unicast;server_port=21000-21001"),
                    ],
                    "",
                );
                match session.on_response(&resp, 1_400) {
                    Ok(_) => {
                        session_id = session.session_id().unwrap_or("").to_string();
                        steps.push(format!("SETUP 应答 → Session={session_id}"));
                    }
                    Err(e) => {
                        ok = false;
                        steps.push(format!("SETUP 应答失败：{:?}", e.code));
                    }
                }
            }
        }
        Err(e) => {
            ok = false;
            steps.push(format!("SETUP 失败：{:?}", e.code));
        }
    }
    match session.play() {
        Ok(bytes) => {
            if let Ok((play_req, _)) = Request::parse(&bytes) {
                let cseq = play_req.cseq().unwrap_or(0);
                let resp = response(200, cseq, &[], "");
                if session.on_response(&resp, 1_500).is_ok() {
                    steps.push("M7 出站 PLAY → 200（进入 playing）".into());
                } else {
                    ok = false;
                }
            }
        }
        Err(e) => {
            ok = false;
            steps.push(format!("PLAY 失败：{:?}", e.code));
        }
    }

    // 连续 3 包 → 无缺口；第 4 包跳号 → 按本仓策略请求关键帧（M13）。
    let mut keyframe_m13 = String::new();
    for seq in 1..4u16 {
        let _ = session.on_rtp(&rtp_packet(seq, 0xDEADBEEF, &[0u8; 188]), 2_000);
    }
    match session.on_rtp(&rtp_packet(9, 0xDEADBEEF, &[0u8; 188]), 2_050) {
        Ok(Some(bytes)) => {
            if let Ok((m13, _)) = Request::parse(&bytes) {
                keyframe_m13 = format!(
                    "M13 method={} uri={} session={} body={:?}",
                    m13.method,
                    m13.uri,
                    m13.session().unwrap_or(""),
                    m13.body_str().unwrap_or("")
                );
            }
        }
        Ok(None) => {
            ok = false;
            steps.push("缺口未触发关键帧请求".into());
        }
        Err(e) => {
            ok = false;
            steps.push(format!("媒体包被拒：{:?}", e.code));
        }
    }
    let keepalive_sent = session
        .keepalive_due(1_500 + KEEPALIVE_INTERVAL_MS + 1)
        .ok()
        .flatten()
        .is_some();
    if !keepalive_sent {
        ok = false;
    }
    let timeout = session.tick(1_500 + proto_wfd::SESSION_TIMEOUT_MS + 1).is_some();
    let stats = session.stats();
    if stats.state != SinkState::Ended || stats.keyframe_requests != 1 {
        ok = false;
    }

    let session_json = json!({
        "steps": steps,
        "session_id": session_id,
        "advertised": advertised,
        "keyframe_request": keyframe_m13,
        "keepalive_sent": keepalive_sent,
        "timeout": timeout,
        "messages_in": stats.messages_in,
        "messages_out": stats.messages_out,
        "packets": stats.packets,
        "lost": stats.lost,
        "keyframe_requests": stats.keyframe_requests,
        "player_available": stats.player_available,
    });

    // ---- 协商：字段解析 + 分歧的可观察形态 + 空广告的拒绝 ----
    let video = VideoFormatSet::parse(DEMO_VIDEO).expect("自制视频广告");
    let audio = AudioCodecSet::parse(DEMO_AUDIO).expect("自制音频广告");
    let descriptor = &video.formats[0];
    let negotiation = json!({
        "native": { "raw": format!("0x{:02X}", video.raw_native), "table": "cea", "index": video.native.index },
        "video": {
            "profile": format!("0x{:02X}", descriptor.profile.0),
            "level": format!("0x{:02X}", descriptor.level.0),
            "cea_sup": format!("0x{:08X}", descriptor.cea_sup),
            "max_slice_num": descriptor.max_slice_num(),
            "frame_skipping": descriptor.frame_skipping_allowed(),
            "wire": descriptor.to_wire(),
        },
        "audio": {
            "count": audio.codecs.len(),
            "lpcm_44100_2ch": audio.codecs[0].lpcm_44100_16_2ch(),
            "lpcm_48000_2ch": audio.codecs[0].lpcm_48000_16_2ch(),
            "aac_48k_2ch": audio.codecs[1].aac_48k_16_2ch(),
            "wire": audio.to_wire(),
        },
        "ports_quirk": {
            "port1_zero_suppresses_keepalive": RtpPorts::parse("RTP/AVP/UDP;unicast 20000 0 mode=play")
                .map(|p| p.suppress_keepalive())
                .unwrap_or(false),
            "distinct_rtcp": RtpPorts::parse("RTP/AVP/UDP;unicast 20000 20002 mode=play")
                .map(|p| p.rtcp_port())
                .unwrap_or(0),
            "aosp_strict_would_reject_port1_nonzero": RtpPorts::parse("RTP/AVP/UDP;unicast 20000 20002 mode=play")
                .map(|p| p.strict_aosp_view_would_reject())
                .unwrap_or(false),
        },
        "transport": {
            "udp_client_port": Transport::parse("RTP/AVP/UDP;unicast;client_port=20000-20001")
                .map(|(t, f)| format!("{} fallback={}", t.to_wire(), f))
                .unwrap_or_default(),
            "udp_without_client_port_falls_back": Transport::parse("RTP/AVP/UDP;unicast")
                .map(|(t, f)| format!("{} fallback={} port={}", t.to_wire(), f, RTP_PORT_FALLBACK))
                .unwrap_or_default(),
            "tcp_interleaved": Transport::parse("RTP/AVP/TCP;interleaved=0-1")
                .map(|(t, _)| t.to_wire())
                .unwrap_or_default(),
        },
        "empty_advertisement": {
            "video_wire": VideoFormatSet::none().to_wire(),
            "audio_wire": AudioCodecSet::none().to_wire(),
        },
    });

    // ---- 子元素与 IE 边界（F-23/F-24）----
    let device = DeviceInfoSubelement::new(CONTROL_PORT, 0x00c8);
    let subelement_bytes = device.to_subelement().to_bytes();
    let subelement_roundtrip = proto_wfd::ie::WfdSubelement::parse(&subelement_bytes)
        .ok()
        .and_then(|(sub, consumed)| {
            DeviceInfoSubelement::from_subelement(&sub)
                .ok()
                .map(|back| (consumed, back))
        });
    let (consumed, back) = subelement_roundtrip.unwrap_or((0, device));
    if consumed != 9 || back.control_port != CONTROL_PORT {
        ok = false;
    }
    let container_build = WfdIeContainer::build(&[device.to_subelement()])
        .err()
        .map(|e| format!("{:?}", e.code))
        .unwrap_or_else(|| "accepted(unexpected)".to_string());
    let container_parse = WfdIeContainer::parse(&subelement_bytes)
        .err()
        .map(|e| format!("{:?}", e.code))
        .unwrap_or_else(|| "accepted(unexpected)".to_string());
    if container_build != "UnsupportedFeature" || container_parse != "UnsupportedFeature" {
        ok = false;
    }
    let ie = json!({
        "device_subelement": {
            "bytes": subelement_bytes.len(),
            "id": subelement_bytes[0],
            "length_field": u16::from_be_bytes([subelement_bytes[1], subelement_bytes[2]]),
            "control_port": back.control_port,
            "max_throughput": back.max_throughput,
            "roundtrip": consumed == 9,
        },
        "container": {
            "build": container_build,
            "parse": container_parse,
            "reason": "OUI/OUI type 未固化（F-24）：不臆造容器格式",
        },
    });

    // ---- RTP 记账（F-28/F-29）----
    let mut account = StreamAccount::new();
    let mut first_ok = true;
    for seq in 1..5u16 {
        let header = parse_header(&rtp_packet(seq, 0x01020304, &[0u8; 188])).expect("自制 RTP 包");
        if account.on_packet(&header, 1_000) {
            first_ok = false;
        }
    }
    let gap = parse_header(&rtp_packet(8, 0x01020304, &[0u8; 188])).expect("自制 RTP 包");
    let requested = account.on_packet(&gap, 2_000);
    let reorder = parse_header(&rtp_packet(6, 0x01020304, &[0u8; 188])).expect("自制 RTP 包");
    let reorder_requested = account.on_packet(&reorder, 2_100);
    let mut other_pt = rtp_packet(7, 0x01020304, &[0u8; 8]);
    other_pt[1] = 96;
    let other = parse_header(&other_pt).expect("自制 RTP 包");
    account.on_packet(&other, 2_200);
    if !(first_ok && requested && !reorder_requested) {
        ok = false;
    }
    let rtp = json!({
        "packets": account.packets,
        "payload_bytes": account.payload_bytes,
        "lost": account.lost,
        "reordered": account.reordered,
        "duplicates": account.duplicates,
        "ssrc_changes": account.ssrc_changes,
        "payload_type_anomalies": account.payload_type_anomalies,
        "keyframe_requests": account.keyframe_requests,
        "sequential_requests_none": first_ok,
        "loss_requests_keyframe": requested,
        "reorder_does_not": !reorder_requested,
    });

    // ---- 负向集（全部按字段表的拒绝形态）----
    let mut negative = |label: &str, code: String| {
        rejects.push(json!({ "case": label, "outcome": code }));
    };
    fn code_of<T>(result: Result<T, interop_contract::error::Error>) -> String {
        result
            .map(|_| "accepted(unexpected)".to_string())
            .unwrap_or_else(|e| format!("{:?}", e.code))
    }

    // 顺序：M4 早于 M3。
    let mut early = SinkSession::new(demo_support(), 20000).expect("会话");
    let early_m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 1, &[], &m4_body("RTP/AVP/UDP;unicast 20000 20002 mode=play"));
    negative("M4 早于 M3", code_of(early.on_request(&early_m4, 0)));
    // 方向：把 M6（本应由 sink 发起）当入站消息交给会话——角色检查在会话里，不在 classify 里。
    let setup_inbound = request("SETUP", "rtsp://192.0.2.10/wfd1.0/streamid=0", 9, &[("Transport", "RTP/AVP/UDP;unicast;client_port=20000")], "");
    let mut direction_session = SinkSession::new(demo_support(), 20000).expect("会话");
    negative("M6 出现在入站方向", direction_session.on_request(&setup_inbound, 0).map(|_| "accepted(unexpected)".to_string()).unwrap_or_else(|e| format!("{:?}", e.code)));
    // 触发值非 SETUP。
    let bad_trigger = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 5, &[], "wfd_trigger_method: PAUSE\r\n");
    negative("M5 触发值非 SETUP", code_of(classify(&bad_trigger, Role::Source)));
    // 未知方法。
    negative("未知方法 DESCRIBE", code_of(Request::parse(b"DESCRIBE rtsp://h/wfd1.0 RTSP/1.0\r\nCSeq: 1\r\n\r\n")));
    // 重复 Content-Length（走私）。
    negative("重复 Content-Length", code_of(Request::parse(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\nabc")));
    // HDCP 要求。
    let mut hdcp_session = SinkSession::new(demo_support(), 20000).expect("会话");
    hdcp_session.on_request(&request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], ""), 0).ok();
    hdcp_session.on_request(&request("GET_PARAMETER", "rtsp://localhost/wfd1.0", 2, &[], "wfd_video_formats\r\n"), 0).ok();
    let hdcp_m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &(m4_body("RTP/AVP/UDP;unicast 20000 20002 mode=play") + "wfd_content_protection: HDCP2.0 port=5000\r\n"));
    negative("对端要求 HDCP", code_of(hdcp_session.on_request(&hdcp_m4, 0)));
    // URL 形状。
    negative("URL 带 userinfo", code_of(proto_wfd::session::validate_presentation_url("rtsp://u:p@192.0.2.10/wfd1.0/streamid=0")));
    negative("URL 路径不是 streamid=0", code_of(proto_wfd::session::validate_presentation_url("rtsp://192.0.2.10/wfd1.0/streamid=1")));
    // 端口畸形。
    negative("wfd_client_rtp_ports port0=0", code_of(RtpPorts::parse("RTP/AVP/UDP;unicast 0 0 mode=play")));
    // 描述符畸形。
    negative("视频描述符字段不足", code_of(parse_video_formats("28 00 02 10 00000020")));
    negative("profile 位图全 0", code_of(parse_video_formats("28 00 00 10 00000020 00000000 00000000 00 0004 0001 00 none none")));
    negative("未知音频编码名", code_of(parse_audio_codecs("OPUS 00000001 00")));
    // 空广告下协商必须失败（不假装能显示）。
    let empty = SinkSession::new(ReceivedMediaSupport::none(), 20000).expect("会话");
    let mut empty = empty;
    empty.on_request(&request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], ""), 0).ok();
    empty.on_request(&request("GET_PARAMETER", "rtsp://localhost/wfd1.0", 2, &[], "wfd_video_formats\r\n"), 0).ok();
    let empty_m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &m4_body("RTP/AVP/UDP;unicast 20000 20002 mode=play"));
    negative("生产路径（广告为空）下的 M4", code_of(empty.on_request(&empty_m4, 0)));
    // RTP 负向。
    let mut bad_rtp = rtp_packet(1, 1, &[0u8; 4]);
    bad_rtp[0] = 0x40;
    negative("RTP 版本位不是 2", code_of(parse_header(&bad_rtp)));
    negative("RTP 头截断", code_of(parse_header(&[0x80, 33, 0, 1])));

    let report = WfdReport {
        evidence_level: crate::report::EVIDENCE_LEVEL,
        wire: crate::report::WIRE,
        messages: json!({ "table": table, "m1_m2_same_shape": same_shape }),
        session: session_json,
        negotiation,
        ie,
        rtp,
        rejects,
        blocked: vec![
            "完整 WFD IE 的构造/解析（F-24：OUI 与 OUI type 在四个来源里都不存在；需 P-M05-2 抓包）".into(),
            "P2P 组形成与 IE 播发（F-25 + P-M05-1：macOS 无公开 WFD API，不触碰平台 P2P 接口）".into(),
            "HDCP 内容保护握手（F-22：密钥交换属设备/厂商材料，仅解析取值并拒绝）".into(),
            "TS 解复用 / 解码 / 画面呈现（F-28 只到 RTP 记账；本 crate 不含播放器）".into(),
            "真实发送端（Android/Windows）与 TV/适配器 codec 矩阵（P-M05-2/P-M05-3）".into(),
            "UIBC 输入回传与厂商扩展参数（wfd_hwe_*/microsoft_*，F-31）".into(),
        ],
    };
    (report, ok)
}
