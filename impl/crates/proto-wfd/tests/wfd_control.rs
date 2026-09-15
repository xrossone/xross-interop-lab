//! T37-01..04 验收测试（plans/03 T37；字段表见 `specs-reviewed/m05-miracast-wfd.md`）。
//!
//! - T37-01 媒体协商：`wfd_video_formats`/`wfd_audio_codecs` 描述符解析与交集选择；
//! - T37-02 WFD IE 边界：子元素往返；**完整 IE 必须被拒绝**（F-24：OUI 未固化）；
//! - T37-03 顺序与角色：M1→M2→M3→M4→M5→SETUP→PLAY→…→TEARDOWN 的约束；
//! - T37-04 媒体边界：RTP 记账与按本仓策略（F-29）请求关键帧。
//!
//! 全部字节自制；测试里出现的数值是本仓自取值，不是抓包内容。

#![allow(clippy::result_large_err)]

use interop_contract::error::ErrorCode;
use proto_wfd::ie::{DeviceInfoSubelement, WfdIeContainer, WfdSubelement};
use proto_wfd::messages::{
    build_response, classify, Request, Response, Role, WfdMessage, WFD_METHOD_SET,
};
use proto_wfd::negotiate::{
    parse_audio_codecs, parse_video_formats, select_common, select_common_audio, AudioCodec,
    AudioCodecSet, ContentProtection, H264Level, H264Profile, ResolutionTable, RtpPorts,
    Transport, VideoFormatSet,
};
use proto_wfd::rtp::{parse_header, StreamAccount, KEYFRAME_REQUEST_DEBOUNCE_MS};
use proto_wfd::session::{validate_presentation_url, SinkSession, SinkState};
use proto_wfd::session::ReceivedMediaSupport;

// ------------------------------------------------------------------ 自制 fixture 工具

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
    let (req, consumed) = Request::parse(raw.as_bytes()).expect("请求应当可解析");
    assert_eq!(consumed, raw.len(), "解析必须消费整条消息");
    req
}

fn response(status: u16, cseq: u64, extra: &[(&str, &str)], body: &str) -> Response {
    let raw = build_response(status, "OK", cseq, extra, body);
    let (resp, _) = Response::parse(&raw).expect("应答应当可解析");
    resp
}

/// 自制 RTP 包：版本 2 + PT 33（F-28 的观察值）+ 伪 SSRC/时间戳。
fn rtp_packet(seq: u16, ssrc: u32, marker: bool, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0x80);
    out.push(33 | if marker { 0x80 } else { 0 });
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&(u32::from(seq) * 3000).to_be_bytes());
    out.extend_from_slice(&ssrc.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// 本端"假设支持"的视频格式（测试专用；不代表本仓能解码，见 `ReceivedMediaSupport::hypothetical`）。
fn hypothetical_video() -> VideoFormatSet {
    // native = 0x28 → 表 CEA(0)、索引 5；profile = CHP(0x02)、level = 4.2 位(0x10)。
    VideoFormatSet::parse("28 00 02 10 00000020 00000000 00000000 00 0004 0001 00 none none")
        .expect("自制视频广告应可解析")
}

fn hypothetical_audio() -> AudioCodecSet {
    AudioCodecSet::parse("LPCM 00000003 00, AAC 00000001 00").expect("自制音频广告应可解析")
}

fn hypothetical_support() -> ReceivedMediaSupport {
    ReceivedMediaSupport::hypothetical(hypothetical_video(), hypothetical_audio())
}

/// 对端（source）在 M4 里下发的参数体。
fn m4_body(parts: &[(&str, &str)]) -> String {
    let mut body = String::new();
    for (name, value) in parts {
        body.push_str(&format!("{name}: {value}\r\n"));
    }
    body
}

fn standard_m4() -> String {
    m4_body(&[
        ("wfd_video_formats", "28 00 02 10 00000020 00000000 00000000 00 0004 0001 00 none none"),
        ("wfd_audio_codecs", "LPCM 00000003 00, AAC 00000001 00"),
        ("wfd_presentation_URL", "rtsp://192.0.2.10:7236/wfd1.0/streamid=0 none"),
        // 两个端口不同 → 不命中 F-14 的 RTCP quirk（quirk 单独在 t37_03_keepalive_suppressed 里验）。
        ("wfd_client_rtp_ports", "RTP/AVP/UDP;unicast 20000 20002 mode=play"),
    ])
}

/// 命中 F-14 quirk 的 M4（`port1 = 0`）。
fn quirky_m4() -> String {
    m4_body(&[
        ("wfd_video_formats", "28 00 02 10 00000020 00000000 00000000 00 0004 0001 00 none none"),
        ("wfd_audio_codecs", "LPCM 00000003 00, AAC 00000001 00"),
        ("wfd_presentation_URL", "rtsp://192.0.2.10:7236/wfd1.0/streamid=0 none"),
        ("wfd_client_rtp_ports", "RTP/AVP/UDP;unicast 20000 0 mode=play"),
    ])
}

// ------------------------------------------------------------------ T37-01 媒体协商

#[test]
fn t37_01_video_descriptor_fields_and_roundtrip() {
    let set = hypothetical_video();
    assert_eq!(set.native.table, ResolutionTable::Cea);
    assert_eq!(set.native.index, 5, "native >> 3（F-19）");
    assert_eq!(set.raw_native, 0x28);
    let format = &set.formats[0];
    assert_eq!(format.profile.0, H264Profile::CHP);
    assert_eq!(format.level.0, H264Level::L4_2);
    assert_eq!(format.cea_sup, 0x20);
    assert!(
        !format.frame_skipping_allowed(),
        "frame_rate_ctrl = 0x00 → 不允许跳帧（F-17：只取最低位）"
    );
    // 切片字段位布局（F-17）：低 9 位 = max_slice_num - 1，高 3 位 = 比例。
    assert_eq!(format.max_slice_num(), 2);
    assert_eq!(format.max_slice_size_ratio(), 0);
    assert_eq!(format.to_wire(), "02 10 00000020 00000000 00000000 00 0004 0001 00 none none");

    // 空集合序列化为 none（F-04：来源把 none 当"不支持"处理）。
    assert_eq!(VideoFormatSet::none().to_wire(), "none");
    assert!(VideoFormatSet::none().is_empty());

    // frame_rate_ctrl 最低位 = 允许跳帧（F-17）。
    let skip = VideoFormatSet::parse("28 00 02 10 00000020 00000000 00000000 00 0004 0001 01 none none")
        .expect("可解析");
    assert!(skip.formats[0].frame_skipping_allowed());

    // min_slice_size = 0 → 来源把 max_slice_num 归 1（F-17）。
    let no_slice = VideoFormatSet::parse("08 00 02 10 00000020 00000000 00000000 00 0000 03FF 00 none none")
        .expect("可解析");
    assert_eq!(no_slice.formats[0].max_slice_num(), 1);
}

#[test]
fn t37_01_video_descriptor_negatives() {
    // 字段不足 9 段（F-17）。
    assert_eq!(
        parse_video_formats("08 00 02 10 00000020 00000000 00000000 00").unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // profile 位图全 0。
    assert_eq!(
        parse_video_formats("08 00 00 10 00000020 00000000 00000000 00 0000 0000 00 none none")
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    // profile 含未登记位（0x40 在 F-18 的低 6 位之外）。
    assert_eq!(
        parse_video_formats("08 00 42 10 00000020 00000000 00000000 00 0000 0000 00 none none")
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    // 非十六进制 token。
    assert!(
        parse_video_formats("08 00 02 10 00000020 00000000 00000000 00 0000 0000 ZZ none none").is_err()
    );
    // 三段结构缺失。
    assert!(parse_video_formats("08 00").is_err());
    // 空的描述符（连续逗号）。
    assert!(parse_video_formats("08 00 02 10 00000020 00000000 00000000 00 0000 0000 00 none none, ").is_err());
}

#[test]
fn t37_01_audio_descriptor_and_mode_bits() {
    let set = hypothetical_audio();
    assert_eq!(set.codecs.len(), 2);
    let lpcm = &set.codecs[0];
    assert_eq!(lpcm.codec, AudioCodec::Lpcm);
    assert!(lpcm.lpcm_44100_16_2ch(), "bit0 = 44.1k/16/2（F-21）");
    assert!(lpcm.lpcm_48000_16_2ch(), "bit1 = 48k/16/2（F-21）");
    assert!(!lpcm.aac_48k_16_2ch());
    let aac = &set.codecs[1];
    assert!(aac.aac_48k_16_2ch(), "AAC bit0 = 48k/16/2（F-21）");
    assert_eq!(aac.latency_ms(), 0);
    assert_eq!(set.to_wire(), "LPCM 00000003 00, AAC 00000001 00");
    assert_eq!(AudioCodecSet::none().to_wire(), "none");

    // 末字段按 ×5 ms 解释（F-20，单一来源已标注）。
    let with_latency = AudioCodecSet::parse("AAC 00000001 0A").expect("可解析");
    assert_eq!(with_latency.codecs[0].latency_ms(), 50);

    // 未知编码名 / 段数不对 → invalid-frame。
    assert_eq!(
        parse_audio_codecs("OPUS 00000001 00").unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    assert!(parse_audio_codecs("AAC 00000001").is_err());
    assert!(parse_audio_codecs("AAC 00000001 00 00").is_err());
}

#[test]
fn t37_01_intersection_selection_refuses_when_empty() {
    let remote = hypothetical_video();
    // 本端为空（生产路径）→ 明确拒绝，不静默降级。
    let err = select_common(&VideoFormatSet::none(), &remote).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedFeature);
    // 交集存在 → 取远端声明顺序里第一条双方都支持的。
    let chosen = select_common(&hypothetical_video(), &remote).expect("应当有交集");
    assert_eq!(chosen.profile.0, H264Profile::CHP);
    assert_eq!(chosen.level.highest_common(chosen.level), Some(chosen.level));
    // level 位图无交集 → 拒绝。
    let mut other_level = remote.clone();
    other_level.formats[0].level = H264Level(H264Level::L3_1);
    assert_eq!(
        select_common(&hypothetical_video(), &other_level).unwrap_err().code,
        ErrorCode::UnsupportedFeature
    );
    // 音频：交集为空 → 拒绝；有交集 → 取远端顺序第一条。
    let local_only_ac3 = AudioCodecSet::parse("AC3 00000001 00").unwrap();
    assert_eq!(
        select_common_audio(&local_only_ac3, &hypothetical_audio()).unwrap_err().code,
        ErrorCode::UnsupportedFeature
    );
    let chosen = select_common_audio(&hypothetical_audio(), &hypothetical_audio()).expect("有交集");
    assert_eq!(chosen.codec, AudioCodec::Lpcm);
}

#[test]
fn t37_01_ports_and_transport_shapes() {
    let ports = RtpPorts::parse("RTP/AVP/UDP;unicast 20000 0 mode=play").expect("合法");
    assert_eq!(ports.port0, 20000);
    assert!(ports.rtcp_port_invalid, "port1 = 0 → 按 F-14 无效");
    assert_eq!(ports.rtcp_port(), 20001);
    assert!(ports.suppress_keepalive(), "F-14：RTCP 端口无效 → 抑制 keep-alive");
    assert!(!ports.strict_aosp_view_would_reject(), "port1 = 0 满足 R34 的严格读法");

    let distinct = RtpPorts::parse("RTP/AVP/UDP;unicast 20000 20002 mode=play").expect("合法");
    assert!(!distinct.rtcp_port_invalid);
    assert_eq!(distinct.rtcp_port(), 20002);
    assert!(distinct.strict_aosp_view_would_reject(), "F-13：R34 的 source 会拒绝 port1 != 0");

    let same = RtpPorts::parse("RTP/AVP/UDP;unicast 20000 20000 mode=play").expect("合法");
    assert!(same.rtcp_port_invalid);

    for bad in [
        "RTP/AVP/UDP;unicast 0 0 mode=play",
        "RTP/AVP/UDP;unicast 20000 0 play",
        "RTP/AVP;unicast 20000 0 mode=play",
        "RTP/AVP/UDP;unicast 20000 0",
        "RTP/AVP/UDP;unicast 70000 0 mode=play",
    ] {
        assert_eq!(RtpPorts::parse(bad).unwrap_err().code, ErrorCode::InvalidFrame, "{bad}");
    }

    let (transport, fallback) =
        Transport::parse("RTP/AVP/UDP;unicast;client_port=20000-20001").expect("合法");
    assert_eq!(
        transport,
        Transport::UdpUnicast {
            client_port: 20000,
            rtcp_client_port: Some(20001)
        }
    );
    assert!(!fallback);
    let (transport, fallback) = Transport::parse("RTP/AVP/UDP;unicast").expect("合法");
    assert_eq!(
        transport,
        Transport::UdpUnicast {
            client_port: proto_wfd::RTP_PORT_FALLBACK,
            rtcp_client_port: None
        }
    );
    assert!(fallback, "F-16：缺 client_port 时按 19000 回退并如实标注");
    let (transport, _) = Transport::parse("RTP/AVP/TCP;interleaved=0-1").expect("合法");
    assert_eq!(
        transport,
        Transport::TcpInterleaved {
            first_channel: 0,
            second_channel: 1
        }
    );
    assert_eq!(transport.to_wire(), "RTP/AVP/TCP;interleaved=0-1");

    for bad in [
        "RTP/AVP/UDP;unicast;client_port=0",
        "RTP/AVP/TCP;interleaved=1-0",
        "RTP/AVP/TCP",
        "RTP/AVP/UDP;unicast;interleaved=0-1",
        "RTP/AVP/SCTP;unicast;client_port=1",
    ] {
        assert_eq!(Transport::parse(bad).unwrap_err().code, ErrorCode::InvalidFrame, "{bad}");
    }
}

#[test]
fn t37_01_content_protection_is_refused() {
    assert_eq!(ContentProtection::parse("none").unwrap(), ContentProtection::None);
    let hdcp = ContentProtection::parse("HDCP2.0 port=5000, version=1").expect("可解析");
    assert!(hdcp.requires_hdcp());
    match &hdcp {
        ContentProtection::Hdcp { version, port, attributes } => {
            assert_eq!(version, "2.0");
            assert_eq!(*port, Some(5000));
            assert_eq!(attributes.len(), 2);
        }
        other => panic!("应当是 HDCP 取值：{other:?}"),
    }
    assert!(ContentProtection::parse("HDCP2.1 port=77").unwrap().requires_hdcp());
    assert_eq!(
        ContentProtection::parse("widevine").unwrap_err().code,
        ErrorCode::InvalidFrame,
        "未知取值必须拒绝"
    );
}

// ------------------------------------------------------------------ T37-02 WFD IE 边界

#[test]
fn t37_02_ie_container_is_refused_not_guessed() {
    let subelement = DeviceInfoSubelement::new(proto_wfd::CONTROL_PORT, 0x00c8).to_subelement();
    let err = WfdIeContainer::build(&[subelement]).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedFeature, "F-24：完整 IE 必须拒绝构造");
    assert!(err.message.contains("P-M05-2"), "拒绝理由必须指向未关闭的 probe");
    let err = WfdIeContainer::parse(&[0x00, 0x0a, 0x00, 0x00]).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedFeature, "F-24：容器必须拒绝解析");
}

#[test]
fn t37_02_device_info_subelement_roundtrip() {
    let device = DeviceInfoSubelement {
        device_info: 0x0100,
        control_port: proto_wfd::CONTROL_PORT,
        max_throughput: 0x00c8,
    };
    let bytes = device.to_subelement().to_bytes();
    assert_eq!(bytes.len(), 9, "id + 长度(2) + 载荷(6)（F-23）");
    assert_eq!(bytes[0], 0x00, "设备信息子元素 id = 0x00");
    assert_eq!(u16::from_be_bytes([bytes[1], bytes[2]]), 6, "长度字段为大端 6");
    assert_eq!(&bytes[5..7], &[0x1c, 0x44], "控制端口 7236 大端（F-23）");
    assert_eq!(&bytes[7..9], &[0x00, 0xc8], "最大吞吐 200 大端（F-23）");

    let (parsed, consumed) = WfdSubelement::parse(&bytes).expect("可解析");
    assert_eq!(consumed, 9);
    let back = DeviceInfoSubelement::from_subelement(&parsed).expect("可还原");
    assert_eq!(back, device);
    assert_eq!(back.device_info, 0x0100, "设备信息字段原样搬运，不解释位语义（F-24）");

    // 长度字段与实际字节不符 → invalid-frame。
    assert_eq!(
        WfdSubelement::parse(&[0x00, 0x00, 0x06, 0x01]).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // 载荷长度非法（id 对但长度 != 6）。
    let wrong_len = WfdSubelement::new(0x00, vec![0; 4]).unwrap();
    assert_eq!(
        DeviceInfoSubelement::from_subelement(&wrong_len).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // 未知 id → invalid-frame。
    let wrong_id = WfdSubelement::new(0x01, vec![0; 6]).unwrap();
    assert_eq!(
        DeviceInfoSubelement::from_subelement(&wrong_id).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // 超预算（本仓策略）在分配前拒绝。
    assert_eq!(
        WfdSubelement::parse(&[0x00, 0xff, 0xff, 0x00]).unwrap_err().code,
        ErrorCode::ResourceLimit
    );
}

// ------------------------------------------------------------------ T37-03 顺序与角色

#[test]
fn t37_03_options_phase_and_public_header() {
    let mut session = SinkSession::new(hypothetical_support(), 20000).expect("可建会话");
    assert_eq!(session.state(), SinkState::Idle);

    // 入站 M1：source 发来的 OPTIONS *（F-02）。
    let m1 = request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], "");
    assert_eq!(classify(&m1, Role::Source).unwrap(), WfdMessage::M1Options);
    let resp = session.on_request(&m1, 0).expect("M1 应当被应答");
    let public = resp
        .headers
        .iter()
        .find(|(k, _)| k == "Public")
        .map(|(_, v)| v.as_str())
        .expect("应答必须带 Public");
    assert!(public.contains(WFD_METHOD_SET));
    assert!(public.contains("GET_PARAMETER"));
    assert_eq!(session.state(), SinkState::Options);

    // 同形的 OPTIONS 由 sink 发出时是 M2（方向不靠字节猜，F-02/F-03）。
    assert_eq!(classify(&m1, Role::Sink).unwrap(), WfdMessage::M2Options);

    // 重复 M1 → 拒绝。
    assert_eq!(
        session.on_request(&m1, 0).unwrap_err().code,
        ErrorCode::InvalidFrame
    );

    // 我们发 M2，并处理应答（Public 必须含 org.wfa.wfd1.0）。
    let bytes = session.options().expect("M2 可发");
    let (sent, _) = Request::parse(&bytes).expect("可解析");
    assert_eq!(sent.method, "OPTIONS");
    assert_eq!(sent.uri, "*");
    assert!(sent.require_is_wfd());
    assert_eq!(classify(&sent, Role::Sink).unwrap(), WfdMessage::M2Options);
    assert_eq!(
        session.options().unwrap_err().code,
        ErrorCode::InvalidFrame,
        "M2 不得重复发送"
    );
    let cseq = sent.cseq().unwrap();
    session
        .on_response(
            &response(200, cseq, &[("Public", "org.wfa.wfd1.0, SETUP, PLAY")], ""),
            0,
        )
        .expect("M2 应答应当被接受");

    // 缺 org.wfa.wfd1.0 的 Public → 拒绝。
    let mut other = SinkSession::new(hypothetical_support(), 20000).unwrap();
    let bytes = other.options().unwrap();
    let (sent, _) = Request::parse(&bytes).unwrap();
    assert_eq!(
        other
            .on_response(&response(200, sent.cseq().unwrap(), &[("Public", "SETUP")], ""), 0)
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
}

#[test]
fn t37_03_full_sequence_to_playing_then_keepalive() {
    let mut session = SinkSession::new(hypothetical_support(), 20000).unwrap();
    let m1 = request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], "");
    session.on_request(&m1, 1_000).unwrap();
    let bytes = session.options().unwrap();
    let (m2, _) = Request::parse(&bytes).unwrap();
    session
        .on_response(
            &response(200, m2.cseq().unwrap(), &[("Public", WFD_METHOD_SET)], ""),
            1_000,
        )
        .unwrap();

    // M3：source 询问能力 → 应答体是**本端实现清单推导**的广告。
    let m3 = request(
        "GET_PARAMETER",
        "rtsp://localhost/wfd1.0",
        2,
        &[],
        "wfd_video_formats\r\nwfd_audio_codecs\r\nwfd_client_rtp_ports\r\n",
    );
    let resp = session.on_request(&m3, 1_100).unwrap();
    let body = resp.body_str().unwrap();
    assert!(body.contains("wfd_video_formats: 28 00 02 10"), "广告必须来自本端假设清单");
    assert!(body.contains("wfd_client_rtp_ports: RTP/AVP/UDP;unicast 20000 0 mode=play"));
    assert!(body.contains("wfd_content_protection: none"));
    assert_eq!(session.state(), SinkState::ParamsAnswered);

    // M4 → 协商（端口合法：两个端口不同 → 不抑制 keep-alive）。
    let m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &standard_m4());
    session.on_request(&m4, 1_200).unwrap();
    let negotiated = session.negotiated().expect("应当完成协商");
    assert_eq!(negotiated.presentation_url, "rtsp://192.0.2.10:7236/wfd1.0/streamid=0");
    assert_eq!(negotiated.audio.codec, AudioCodec::Lpcm);
    let ports = negotiated.rtp_ports.unwrap();
    assert!(!ports.suppress_keepalive());
    assert_eq!(ports.rtcp_port(), 20002);
    assert_eq!(session.state(), SinkState::ParamsSet);

    // M5 触发 → SETUP → 应答带 Session → PLAY。
    let m5 = request(
        "SET_PARAMETER",
        "rtsp://localhost/wfd1.0",
        4,
        &[],
        &m4_body(&[("wfd_trigger_method", "SETUP")]),
    );
    session.on_request(&m5, 1_300).unwrap();
    assert_eq!(session.state(), SinkState::SetupTriggered);
    let bytes = session.setup().unwrap();
    let (setup, _) = Request::parse(&bytes).unwrap();
    assert_eq!(setup.method, "SETUP");
    assert_eq!(setup.uri, "rtsp://192.0.2.10:7236/wfd1.0/streamid=0");
    assert_eq!(
        setup.transport(),
        Some("RTP/AVP/UDP;unicast;client_port=20000")
    );
    session
        .on_response(
            &response(
                200,
                setup.cseq().unwrap(),
                &[("Session", "1234567;timeout=30"), ("Transport", "RTP/AVP/UDP;unicast;server_port=21000-21001")],
                "",
            ),
            1_400,
        )
        .unwrap();
    assert_eq!(session.session_id(), Some("1234567"), "Session 取分号前子串（F-07）");
    assert_eq!(session.state(), SinkState::SetupCreated);

    let bytes = session.play().unwrap();
    let (play, _) = Request::parse(&bytes).unwrap();
    assert_eq!(play.method, "PLAY");
    assert_eq!(play.session(), Some("1234567"), "PLAY 必须带 Session（F-08）");
    session.on_response(&response(200, play.cseq().unwrap(), &[], ""), 1_500).unwrap();
    assert_eq!(session.state(), SinkState::Playing);

    // keep-alive：未到点不发，到点才发（F-27 取值的比较）。
    assert!(session.keepalive_due(1_600).unwrap().is_none(), "首包之前不该到点");
    let bytes = session
        .keepalive_due(proto_wfd::KEEPALIVE_INTERVAL_MS + 1_600)
        .unwrap()
        .expect("到点应当发 M16");
    let (m16, _) = Request::parse(&bytes).unwrap();
    assert_eq!(m16.method, "GET_PARAMETER");
    assert_eq!(m16.uri, "rtsp://localhost/wfd1.0", "M16 不是 OPTIONS *（F-11）");
    assert_eq!(m16.session(), Some("1234567"));
    assert_eq!(classify(&m16, Role::Sink).unwrap(), WfdMessage::M16KeepAlive);

    // 会话超时（30 s 取值，F-27）。
    assert_eq!(session.tick(2_000), None);
    assert_eq!(session.tick(1_500 + proto_wfd::SESSION_TIMEOUT_MS + 1), Some("keepalive-timeout"));
    assert_eq!(session.state(), SinkState::Ended);
    assert!(session.keepalive_due(999_999).unwrap().is_none(), "结束后不再发消息");
}

#[test]
fn t37_03_keepalive_suppressed_by_rtcp_quirk() {
    let mut session = SinkSession::new(hypothetical_support(), 20000).unwrap();
    session
        .on_request(&request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], ""), 0)
        .unwrap();
    session
        .on_request(
            &request(
                "GET_PARAMETER",
                "rtsp://localhost/wfd1.0",
                2,
                &[],
                "wfd_client_rtp_ports\r\n",
            ),
            1,
        )
        .unwrap();
    let m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &quirky_m4());
    session.on_request(&m4, 2).unwrap();
    assert!(session.stats().keepalive_suppressed, "F-14：quirk 命中 → 抑制 keep-alive");
}

#[test]
fn t37_03_out_of_order_messages_are_refused() {
    // 未完成 M3 就 M4。
    let mut session = SinkSession::new(hypothetical_support(), 20000).unwrap();
    let m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 1, &[], &standard_m4());
    assert_eq!(
        session.on_request(&m4, 0).unwrap_err().code,
        ErrorCode::InvalidFrame
    );

    // 未 M5 就 SETUP / 未 SETUP 就 PLAY。
    let mut session = SinkSession::new(hypothetical_support(), 20000).unwrap();
    session
        .on_request(&request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], ""), 0)
        .unwrap();
    session
        .on_request(
            &request("GET_PARAMETER", "rtsp://localhost/wfd1.0", 2, &[], "wfd_video_formats\r\n"),
            0,
        )
        .unwrap();
    session
        .on_request(&request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &standard_m4()), 0)
        .unwrap();
    assert_eq!(session.setup().unwrap_err().code, ErrorCode::InvalidFrame, "M5 之前不得 SETUP");
    assert_eq!(session.play().unwrap_err().code, ErrorCode::InvalidFrame, "SETUP 之前不得 PLAY");

    // M5 的触发值只认 SETUP。
    let bad_trigger = request(
        "SET_PARAMETER",
        "rtsp://localhost/wfd1.0",
        4,
        &[],
        &m4_body(&[("wfd_trigger_method", "PAUSE")]),
    );
    assert_eq!(
        session.on_request(&bad_trigger, 0).unwrap_err().code,
        ErrorCode::InvalidFrame
    );

    // CSeq 回退。
    let mut session2 = SinkSession::new(hypothetical_support(), 20000).unwrap();
    session2
        .on_request(&request("OPTIONS", "*", 5, &[("Require", WFD_METHOD_SET)], ""), 0)
        .unwrap();
    let regress = request("OPTIONS", "*", 3, &[("Require", WFD_METHOD_SET)], "");
    assert_eq!(
        session2.on_request(&regress, 0).unwrap_err().code,
        ErrorCode::InvalidFrame
    );

    // 未知方法 / 缺 Require / SETUP 缺 Transport。
    assert_eq!(
        Request::parse(b"DESCRIBE rtsp://host/wfd1.0 RTSP/1.0\r\nCSeq: 1\r\n\r\n")
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedMethod
    );
    assert_eq!(
        session
            .on_request(&request("OPTIONS", "*", 6, &[], ""), 0)
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        Request::parse(b"SETUP rtsp://host/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 7\r\n\r\n")
            .map(|(req, _)| req)
            .and_then(|req| classify(&req, Role::Sink))
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
}

#[test]
fn t37_03_hdcp_and_url_shape_are_refused() {
    let mut session = SinkSession::new(hypothetical_support(), 20000).unwrap();
    session
        .on_request(&request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], ""), 0)
        .unwrap();
    session
        .on_request(
            &request("GET_PARAMETER", "rtsp://localhost/wfd1.0", 2, &[], "wfd_video_formats\r\n"),
            0,
        )
        .unwrap();
    let body = standard_m4() + "wfd_content_protection: HDCP2.0 port=5000\r\n";
    let m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &body);
    let err = session.on_request(&m4, 0).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedFeature, "F-22：HDCP 必须明确拒绝");

    // URL 形状（F-12）。
    assert!(validate_presentation_url("rtsp://192.0.2.10:7236/wfd1.0/streamid=0").is_ok());
    for bad in [
        "http://192.0.2.10/wfd1.0/streamid=0",
        "rtsp://user:pass@192.0.2.10/wfd1.0/streamid=0",
        "rtsp://192.0.2.10/wfd1.0/streamid=1",
        "rtsp://192.0.2.10:notaport/wfd1.0/streamid=0",
        "rtsp:///wfd1.0/streamid=0",
    ] {
        assert_eq!(
            validate_presentation_url(bad).unwrap_err().code,
            ErrorCode::InvalidFrame,
            "{bad}"
        );
    }
    // M4 里的端口畸形同样拒绝。
    let mut session2 = SinkSession::new(hypothetical_support(), 20000).unwrap();
    session2
        .on_request(&request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], ""), 0)
        .unwrap();
    session2
        .on_request(
            &request("GET_PARAMETER", "rtsp://localhost/wfd1.0", 2, &[], "wfd_video_formats\r\n"),
            0,
        )
        .unwrap();
    let body = m4_body(&[
        ("wfd_video_formats", "28 00 02 10 00000020 00000000 00000000 00 0004 0001 00 none none"),
        ("wfd_audio_codecs", "LPCM 00000003 00"),
        ("wfd_presentation_URL", "rtsp://192.0.2.10/wfd1.0/streamid=0 none"),
        ("wfd_client_rtp_ports", "RTP/AVP/UDP;unicast 0 0 mode=play"),
    ]);
    let m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &body);
    assert_eq!(
        session2.on_request(&m4, 0).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
}

#[test]
fn t37_03_production_support_advertises_none() {
    // 生产路径（本仓没有媒体引擎）→ 广告必须是 none，且协商必然失败。
    let mut session = SinkSession::new(ReceivedMediaSupport::none(), 20000).unwrap();
    assert!(ReceivedMediaSupport::none().is_empty());
    session
        .on_request(&request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], ""), 0)
        .unwrap();
    let resp = session
        .on_request(
            &request("GET_PARAMETER", "rtsp://localhost/wfd1.0", 2, &[], "wfd_video_formats\r\n"),
            0,
        )
        .unwrap();
    assert!(resp.body_str().unwrap().contains("wfd_video_formats: none"));
    let m4 = request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &standard_m4());
    assert_eq!(
        session.on_request(&m4, 0).unwrap_err().code,
        ErrorCode::UnsupportedFeature,
        "广告为空 + 有交集要求 → 明确拒绝（不假装能显示）"
    );
    assert!(!session.stats().player_available, "本 crate 没有播放器：这一位恒为 false");
}

// ------------------------------------------------------------------ T37-04 媒体边界

#[test]
fn t37_04_rtp_accounting_and_keyframe_policy() {
    let header = parse_header(&rtp_packet(100, 0xAABBCCDD, false, &[0u8; 188])).unwrap();
    assert_eq!(header.version, 2);
    assert!(header.is_mpegts(), "PT 33（F-28）");
    assert_eq!(header.sequence, 100);
    assert_eq!(header.ssrc, 0xAABBCCDD);
    assert_eq!(header.payload_len, 188);
    assert_eq!(header.header_len, 12);

    let mut account = StreamAccount::new();
    assert!(!account.on_packet(&header, 0));
    for seq in 101..105 {
        let h = parse_header(&rtp_packet(seq, 0xAABBCCDD, false, &[0u8; 188])).unwrap();
        assert!(!account.on_packet(&h, 1_000), "连续序号不该请求关键帧");
    }
    assert_eq!(account.packets, 5);
    assert_eq!(account.lost, 0);

    // 丢包（序号前跳）→ 按本仓策略请求关键帧（F-29）。
    let gap = parse_header(&rtp_packet(108, 0xAABBCCDD, false, &[0u8; 188])).unwrap();
    assert!(account.on_packet(&gap, 2_000), "缺口 → 请求关键帧");
    assert_eq!(account.lost, 3);
    assert_eq!(account.keyframe_requests, 1);

    // 去抖窗口内再次丢包 → 不重复请求。
    let gap2 = parse_header(&rtp_packet(120, 0xAABBCCDD, false, &[0u8; 188])).unwrap();
    assert!(!account.on_packet(&gap2, 2_000 + KEYFRAME_REQUEST_DEBOUNCE_MS - 1));
    assert_eq!(account.keyframe_requests, 1);
    assert!(account.has_pending_loss(), "缺口仍未补");

    // 过了去抖窗口 → 再请求一次。
    let gap3 = parse_header(&rtp_packet(140, 0xAABBCCDD, false, &[0u8; 188])).unwrap();
    assert!(account.on_packet(&gap3, 2_000 + KEYFRAME_REQUEST_DEBOUNCE_MS + 1));
    assert_eq!(account.keyframe_requests, 2);
    account.on_keyframe();
    assert!(!account.has_pending_loss());

    // 乱序（回退多于 1）与重复（回退 1）都不触发关键帧请求。
    let reorder = parse_header(&rtp_packet(139, 0xAABBCCDD, false, &[0u8; 188])).unwrap();
    assert!(!account.on_packet(&reorder, 9_999));
    let duplicate = parse_header(&rtp_packet(140, 0xAABBCCDD, false, &[0u8; 188])).unwrap();
    assert!(!account.on_packet(&duplicate, 9_999));
    assert_eq!(account.reordered, 1);
    assert_eq!(account.duplicates, 1);
    assert_eq!(account.keyframe_requests, 2, "乱序/重复不产生关键帧请求");

    // SSRC 变化 = 流被替换（F-28 观察层）→ 记一次并请求关键帧。
    let other = parse_header(&rtp_packet(1, 0x11, false, &[0u8; 188])).unwrap();
    assert!(account.on_packet(&other, 20_000));
    assert_eq!(account.ssrc_changes, 1);

    // 非 33 的 payload type 只记账、不拒绝（动态 PT 不在字段表内）。
    let mut other_pt = rtp_packet(2, 0x11, false, &[0u8; 4]);
    other_pt[1] = 96;
    let h = parse_header(&other_pt).unwrap();
    account.on_packet(&h, 21_000);
    assert_eq!(account.payload_type_anomalies, 1);
}

#[test]
fn t37_04_rtp_negatives() {
    // 版本位不是 2。
    let mut bad = rtp_packet(1, 1, false, &[0u8; 4]);
    bad[0] = 0x40;
    assert_eq!(parse_header(&bad).unwrap_err().code, ErrorCode::InvalidFrame);
    // 头截断。
    assert_eq!(
        parse_header(&[0x80, 33, 0, 1]).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // padding 位为 1 但填充长度非法（0）。
    let mut padded = rtp_packet(1, 1, false, &[0u8; 4]);
    padded[0] |= 0x20;
    padded.push(0);
    assert_eq!(parse_header(&padded).unwrap_err().code, ErrorCode::InvalidFrame);
    // CSRC 数超出实际字节。
    let mut csrc = rtp_packet(1, 1, false, &[0u8; 4]);
    csrc[0] |= 0x0f;
    assert_eq!(parse_header(&csrc).unwrap_err().code, ErrorCode::InvalidFrame);
}

#[test]
fn t37_04_session_media_path_and_keyframe_request() {
    let mut session = playing_session();
    // 未进入 PLAYING 的会话拒绝媒体包。
    let mut idle = SinkSession::new(hypothetical_support(), 20000).unwrap();
    assert_eq!(
        idle.on_rtp(&rtp_packet(1, 1, false, &[0u8; 8]), 0).unwrap_err().code,
        ErrorCode::InvalidFrame
    );

    // 连续包不产生关键帧请求。
    for seq in 1..4 {
        assert!(session
            .on_rtp(&rtp_packet(seq, 0xDEADBEEF, false, &[0u8; 188]), 1_000)
            .unwrap()
            .is_none());
    }
    // 缺口 → 会话直接给出 M13 字节。
    let bytes = session
        .on_rtp(&rtp_packet(9, 0xDEADBEEF, false, &[0u8; 188]), 2_000)
        .unwrap()
        .expect("缺口应当产生 M13");
    let (m13, _) = Request::parse(&bytes).unwrap();
    assert_eq!(m13.method, "SET_PARAMETER");
    assert_eq!(m13.session(), Some("1234567"));
    assert!(m13.body_str().unwrap().contains("wfd_idr_request"), "F-10");
    assert_eq!(classify(&m13, Role::Sink).unwrap(), WfdMessage::M13IdrRequest);

    let stats = session.stats();
    assert_eq!(stats.state, SinkState::Playing);
    assert_eq!(stats.packets, 4);
    assert_eq!(stats.lost, 5);
    assert_eq!(stats.keyframe_requests, 1);
    assert!(!stats.player_available);
    assert!(stats.messages_out >= 4);
}

/// 走到 PLAYING 的会话（自制序列）。
fn playing_session() -> SinkSession {
    let mut session = SinkSession::new(hypothetical_support(), 20000).unwrap();
    session
        .on_request(&request("OPTIONS", "*", 1, &[("Require", WFD_METHOD_SET)], ""), 0)
        .unwrap();
    let bytes = session.options().unwrap();
    let (m2, _) = Request::parse(&bytes).unwrap();
    session
        .on_response(&response(200, m2.cseq().unwrap(), &[("Public", WFD_METHOD_SET)], ""), 0)
        .unwrap();
    session
        .on_request(
            &request("GET_PARAMETER", "rtsp://localhost/wfd1.0", 2, &[], "wfd_video_formats\r\n"),
            0,
        )
        .unwrap();
    session
        .on_request(&request("SET_PARAMETER", "rtsp://localhost/wfd1.0", 3, &[], &standard_m4()), 0)
        .unwrap();
    session
        .on_request(
            &request(
                "SET_PARAMETER",
                "rtsp://localhost/wfd1.0",
                4,
                &[],
                &m4_body(&[("wfd_trigger_method", "SETUP")]),
            ),
            0,
        )
        .unwrap();
    let bytes = session.setup().unwrap();
    let (setup, _) = Request::parse(&bytes).unwrap();
    session
        .on_response(
            &response(200, setup.cseq().unwrap(), &[("Session", "1234567")], ""),
            0,
        )
        .unwrap();
    let bytes = session.play().unwrap();
    let (play, _) = Request::parse(&bytes).unwrap();
    session
        .on_response(&response(200, play.cseq().unwrap(), &[], ""), 0)
        .unwrap();
    session
}
