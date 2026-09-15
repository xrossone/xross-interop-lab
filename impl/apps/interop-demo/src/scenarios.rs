//! 四个 headless 场景：发现 / 会话 / sink / 数据面。
//!
//! 每个场景都用**真实 crate 代码**跑一遍并把结果转成报告；不做任何网络与系统写操作
//! （唯一落盘是 sink 的临时文件，路径在报告里给出）。
//! 场景自带断言，`ok=false` 时 CLI 退出码非 0——demo 不允许"看起来全绿"。

use crate::report::*;
use interop_contract::capability::{Capability, EvidenceState, Role};
use interop_contract::error::Error;
use interop_contract::ids::SessionId;
use interop_ipc::media_frame::{flags as xflags, FrameKind, MediaFrame};
use interop_media::sinks::{
    open_sink, Dimensions, ResamplePlan, SinkKind, SinkOptions, MAX_PIXELS,
};
use interop_platform::discovery::{DiscoverySource, EndpointAddress, FakeDiscovery};
use interop_platform::probe::probe_local;
use interop_runtime::endpoints::{presentation_groups, EndpointRegistry, UserAlias};
use proto_airplay::capability::{advertised_features, ImplementationInventory};
use proto_airplay::discovery::{encode_txt_rdata, feature_string_status, mdns_service_types,
    ServiceAdvertisement};
use proto_airplay::keying::{FakeKeying, KeyingProvider, UnavailableKeying};
use proto_airplay::rtsp::{RtspRequest, MAX_BODY_BYTES, MAX_HEADER_BYTES, MAX_HEADERS};
use proto_airplay::session::{AirPlayReceiver, SessionState};

/// 错误码 → wire 字符串（kebab-case，与 `interop.api/0.1` 一致）。
fn code_str(e: &Error) -> String {
    serde_json::to_value(e.code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{:?}", e.code))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn state_wire(s: SessionState) -> &'static str {
    match s {
        SessionState::Unauthenticated => "unauthenticated",
        SessionState::Authenticated => "authenticated",
        SessionState::Streaming => "streaming",
        SessionState::Ended => "ended",
    }
}

/// 自制 RTSP 请求字节（字段出自 `specs-reviewed/m01` 字段表）。
fn fixture_request(method: &str, uri: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut out = format!("{method} {uri} RTSP/1.0\r\n").into_bytes();
    for (k, v) in headers {
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    out
}

// ---------------------------------------------------------------- 发现面

pub fn discovery_scenario() -> (DiscoveryReport, bool) {
    let mut ok = true;
    let probe = probe_local();

    // 两条**同名同址但来源不同**的观察：authority 视图必须是两行（不合并 identity）。
    let ttl_ms: u64 = 30_000;
    let t0: u64 = 1_000_000;
    let mut reg = EndpointRegistry::new(ttl_ms);
    let mut fake = FakeDiscovery::new();
    let address = EndpointAddress {
        host: "192.0.2.10".to_string(), // TEST-NET-1：文档用地址，不指向真实设备
        port: 7000,
        interface: "en0".to_string(),
        secure: false,
    };
    let peer_capability = Capability {
        profile_id: "airplay-legacy-mirror".to_string(),
        role: Role::Receive,
        provider_id: "proto-airplay".to_string(),
        state: EvidenceState::Blocked,
        available: false, // 对端声明 ≠ 证据；本机能力缺席时不得显示可用
        prerequisites: vec!["S1：外部引擎/FairPlay 材料获批".to_string()],
        media_forms: vec![],
        evidence_ids: vec!["run-2026-09-15-t32-airplay-control".to_string()],
    };
    let mut mdns_obs = fake.announce("airplay-legacy-mirror", "客厅电视", address.clone(), t0);
    mdns_obs.source = DiscoverySource::Mdns;
    mdns_obs.capabilities = vec![peer_capability.clone()];
    let mut ssdp_obs = fake.announce("airplay-legacy-mirror", "客厅电视", address.clone(), t0);
    ssdp_obs.source = DiscoverySource::Ssdp;
    ssdp_obs.identity_claim = Some("airplay:02:00:00:00:00:01".to_string());
    ssdp_obs.capabilities = vec![peer_capability];

    let id_mdns = match reg.observe(mdns_obs) {
        Ok(id) => id,
        Err(e) => return (empty_discovery(&format!("观察输入非法：{e}")), false),
    };
    let id_ssdp = match reg.observe(ssdp_obs) {
        Ok(id) => id,
        Err(e) => return (empty_discovery(&format!("观察输入非法：{e}")), false),
    };

    let rows: Vec<RowReport> = reg
        .endpoints()
        .iter()
        .map(|r| RowReport {
            subject: r.subject(),
            source: r.source.as_wire(),
            profile_id: r.profile_id.clone(),
            display_name: r.display_name.clone(),
            identity_claim: r.identity_claim.clone(),
            addresses: r
                .addresses
                .iter()
                .map(|a| AddressReport {
                    host: a.host.clone(),
                    port: a.port,
                    interface: a.interface.clone(),
                    secure: a.secure,
                    expires_at_ms: a.expires_at_ms,
                })
                .collect(),
            capabilities: r
                .capabilities
                .iter()
                .map(|c| CapabilityReport {
                    profile_id: c.profile_id.clone(),
                    role: format!("{:?}", c.role).to_lowercase(),
                    provider_id: c.provider_id.clone(),
                    available: c.available,
                    state: serde_json::to_value(c.state)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_default(),
                    prerequisites: c.prerequisites.clone(),
                })
                .collect(),
        })
        .collect();

    let same_name_same_address_rows = rows
        .iter()
        .filter(|r| r.display_name == "客厅电视" && r.addresses.iter().any(|a| a.host == "192.0.2.10"))
        .count();
    if same_name_same_address_rows != 2 {
        ok = false;
    }

    let sendable_before_ttl = reg.sendable_addresses(&id_mdns, t0 + 10_000).len();
    let sendable_after_ttl = reg.sendable_addresses(&id_mdns, t0 + ttl_ms + 10_000).len();
    let affected = reg.interface_down("en0", t0 + 10_000);
    let sendable_after_interface_down = reg.sendable_addresses(&id_mdns, t0 + 10_000).len();
    if !(sendable_before_ttl == 1 && sendable_after_ttl == 0 && sendable_after_interface_down == 0) {
        ok = false;
    }

    let aliases = vec![UserAlias {
        alias: "客厅电视（用户认定同一台）".to_string(),
        endpoints: vec![id_mdns.clone(), id_ssdp.clone()],
    }];
    let alias_groups: Vec<AliasGroupReport> = presentation_groups(&reg, &aliases)
        .into_iter()
        .map(|g| AliasGroupReport {
            alias: g.alias,
            members: g.members.iter().map(|m| m.as_ref().to_string()).collect(),
        })
        .collect();

    let advert = ServiceAdvertisement::for_receiver(&ImplementationInventory::current(), 7000, "xross-interop-demo");
    let txt_pairs = advert.txt_pairs();
    let txt_rdata_hex = match encode_txt_rdata(&txt_pairs) {
        Ok(bytes) => hex(&bytes),
        Err(e) => {
            ok = false;
            format!("encode-error:{}", code_str(&e))
        }
    };

    let report = DiscoveryReport {
        evidence_level: EVIDENCE_LEVEL,
        local_interfaces: serde_json::to_value(&probe.network_interfaces).unwrap_or_default(),
        local_probe_commands: serde_json::to_value(&probe.commands).unwrap_or_default(),
        platform_support: serde_json::json!({
            "wifi_p2p": serde_json::to_value(&probe.wifi_p2p).unwrap_or_default(),
            "wifi_display": serde_json::to_value(&probe.wifi_display).unwrap_or_default(),
            "media_outputs": serde_json::to_value(&probe.media_outputs).unwrap_or_default(),
        }),
        mdns_service_types: mdns_service_types().iter().map(|s| s.to_string()).collect(),
        txt_pairs,
        txt_rdata_hex,
        feature_string_status: feature_string_status().to_string(),
        implemented_discovery_sources: vec![DiscoverySourceStatus {
            source: "fake",
            network: false,
        }],
        registry: RegistryReport {
            ttl_s: ttl_ms / 1000,
            authority_rows: rows,
            same_name_same_address_rows,
            alias_groups,
            expiry: ExpiryReport {
                sendable_before_ttl,
                sendable_after_ttl,
                sendable_after_interface_down,
            },
            interface_down_affected: affected.iter().map(|i| i.as_ref().to_string()).collect(),
        },
        notes: vec![
            "地址用 TEST-NET 文档地址（192.0.2.0/24）：不指向真实设备".to_string(),
            "fake 源不产生网络行为：真实 mDNS/SSDP 源未实现（见 blocked）".to_string(),
            "对端声明的能力一律 available=false：声明不是证据".to_string(),
        ],
    };
    (report, ok)
}

fn empty_discovery(why: &str) -> DiscoveryReport {
    DiscoveryReport {
        evidence_level: EVIDENCE_LEVEL,
        local_interfaces: serde_json::Value::Null,
        local_probe_commands: serde_json::Value::Null,
        platform_support: serde_json::Value::Null,
        mdns_service_types: vec![],
        txt_pairs: vec![],
        txt_rdata_hex: String::new(),
        feature_string_status: feature_string_status().to_string(),
        implemented_discovery_sources: vec![],
        registry: RegistryReport {
            ttl_s: 0,
            authority_rows: vec![],
            same_name_same_address_rows: 0,
            alias_groups: vec![],
            expiry: ExpiryReport {
                sendable_before_ttl: 0,
                sendable_after_ttl: 0,
                sendable_after_interface_down: 0,
            },
            interface_down_affected: vec![],
        },
        notes: vec![format!("发现场景提前失败（{why}）")],
    }
}

// ---------------------------------------------------------------- 会话面

pub fn session_scenario() -> (SessionReport, bool) {
    let mut ok = true;

    // framing 负向：全部必须被拒
    let hardening_cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "POST 缺 Content-Length",
            fixture_request("POST", "/pair-setup-pin", &[("CSeq", "1")], b"plist"),
        ),
        (
            "重复 Content-Length",
            fixture_request(
                "POST",
                "/pair-setup-pin",
                &[("CSeq", "1"), ("Content-Length", "5"), ("Content-Length", "6")],
                b"plist",
            ),
        ),
        (
            "Content-Length: 999999",
            fixture_request("POST", "/pair-setup-pin", &[("Content-Length", "999999")], b""),
        ),
        (
            "body 截断（声明 10 实际 3）",
            fixture_request("POST", "/pair-setup-pin", &[("Content-Length", "10")], b"abc"),
        ),
    ];
    let mut protocol_hardening = Vec::new();
    for (case, bytes) in hardening_cases {
        match RtspRequest::parse(&bytes) {
            Ok(_) => {
                ok = false;
                protocol_hardening.push(CaseOutcome {
                    case: case.to_string(),
                    outcome: "accepted(unexpected)",
                    error_code: None,
                });
            }
            Err(e) => protocol_hardening.push(CaseOutcome {
                case: case.to_string(),
                outcome: "rejected",
                error_code: Some(code_str(&e)),
            }),
        }
    }

    let (production, prod_ok) = run_keying_flow(Box::new(UnavailableKeying), None);
    let (test_double, fake_ok) = run_keying_flow(
        Box::new(FakeKeying::deterministic()),
        Some("自造密钥字节（非任何真实材料）：只用于演示状态机与资源记账"),
    );
    ok &= prod_ok && fake_ok;

    if production.setup_status != 503
        || production.has_stream_after_setup
        || production.allocated_video_bytes_after_setup != 0
        || production.state_after_setup != "unauthenticated"
    {
        ok = false;
    }
    if test_double.setup_status != 200
        || test_double.state_after_setup != "streaming"
        || test_double.has_stream_after_teardown
    {
        ok = false;
    }

    // 100 次快速连接/断开：不得残留 session 或端口
    let mut rx = AirPlayReceiver::new(4, Box::new(FakeKeying::deterministic()));
    let mut now = 2_000_000u64;
    for i in 0..100u32 {
        now += 5;
        let id = match rx.accept_connection(now) {
            Ok(id) => id,
            Err(_) => {
                ok = false;
                break;
            }
        };
        if i % 2 == 0 {
            let bytes = fixture_request("GET", "/info", &[("CSeq", "1")], b"");
            if let Ok((req, _)) = RtspRequest::parse(&bytes) {
                let _ = rx.handle(&id, &req, now);
            }
        }
        rx.disconnect(&id, now);
    }
    let churn = ChurnReport {
        iterations: 100,
        active_sessions: rx.active_sessions(),
        reserved_ports: rx.reserved_ports(),
        tracked_sessions: rx.tracked_sessions(),
    };
    if churn.active_sessions != 0 || churn.reserved_ports != 0 || churn.tracked_sessions != 0 {
        ok = false;
    }

    // 并发上限：超出 → busy（不静默超发）；断开后名额释放
    let mut capped = AirPlayReceiver::new(1, Box::new(FakeKeying::deterministic()));
    let first = match capped.accept_connection(now) {
        Ok(id) => id,
        Err(_) => {
            ok = false;
            SessionId::try_from("ses_ap_failed".to_string()).expect("形状合法")
        }
    };
    let second_accept = match capped.accept_connection(now + 1) {
        Ok(_) => {
            ok = false;
            "accepted(unexpected)".to_string()
        }
        Err(e) => code_str(&e),
    };
    capped.disconnect(&first, now + 2);
    let after_disconnect = match capped.accept_connection(now + 3) {
        Ok(_) => "accepted".to_string(),
        Err(e) => {
            ok = false;
            code_str(&e)
        }
    };
    if second_accept != "busy" || after_disconnect != "accepted" {
        ok = false;
    }

    let report = SessionReport {
        transport: "rtsp/1.0-frame",
        framing: FramingLimits {
            max_header_bytes: MAX_HEADER_BYTES,
            max_body_bytes: MAX_BODY_BYTES,
            max_headers: MAX_HEADERS,
        },
        protocol_hardening,
        production_keying: production,
        test_keying: test_double,
        churn,
        concurrency_cap: CapReport {
            max_sessions: 1,
            second_accept,
            after_disconnect,
        },
    };
    (report, ok)
}

/// 走一遍控制链：连接 → /info → 配对 → /fp-setup → SETUP → PLAY → TEARDOWN → 断开。
fn run_keying_flow(
    keying: Box<dyn KeyingProvider>,
    warning: Option<&'static str>,
) -> (KeyingRunReport, bool) {
    let mut ok = true;
    let mut steps: Vec<StepReport> = Vec::new();
    let name = keying.name();
    let mut rx = AirPlayReceiver::new(4, keying);
    let mut now = 1_000_000u64;

    let sid = match rx.accept_connection(now) {
        Ok(id) => id,
        Err(_) => return (failed_keying_run(name, warning), false),
    };

    // 自制请求：GET /info（无 body）、POST /pair-setup-pin（4 字节）、POST /fp-setup（19 字节）、
    // SETUP/PLAY/TEARDOWN（Content-Length: 0）
    let mut fp_setup_error = None;
    let flow: Vec<(&str, &str, Vec<u8>)> = vec![
        (
            "GET /info",
            "1",
            fixture_request("GET", "/info", &[("CSeq", "1"), ("User-Agent", "XrossInteropDemo/0.1")], b""),
        ),
        (
            "POST /pair-setup-pin",
            "2",
            fixture_request(
                "POST",
                "/pair-setup-pin",
                &[("CSeq", "2"), ("Content-Type", "application/octet-stream"), ("Content-Length", "4")],
                &[0x00, 0x01, 0x02, 0x03],
            ),
        ),
        (
            "POST /fp-setup",
            "3",
            fixture_request(
                "POST",
                "/fp-setup",
                &[("CSeq", "3"), ("Content-Type", "application/octet-stream"), ("Content-Length", "19")],
                &[0x46, 0x50, 0x4c, 0x59, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e],
            ),
        ),
        (
            "SETUP /stream",
            "4",
            fixture_request(
                "SETUP",
                "rtsp://198.51.100.20/stream",
                &[("CSeq", "4"), ("Content-Length", "0")],
                b"",
            ),
        ),
    ];

    let mut setup_status = 0u16;
    let mut state_after_setup = "none";
    let mut has_stream_after_setup = false;
    let mut allocated_after_setup = 0u64;

    for (label, _cseq, bytes) in &flow {
        now += 10;
        let (req, _) = match RtspRequest::parse(bytes) {
            Ok(v) => v,
            Err(e) => {
                ok = false;
                steps.push(StepReport {
                    step: label.to_string(),
                    bytes_in: bytes.len(),
                    status: None,
                    response_bytes: 0,
                    error_code: Some(code_str(&e)),
                    state_after: state_wire(rx.state(&sid).unwrap_or(SessionState::Ended)),
                    has_stream: rx.has_stream(&sid),
                    allocated_video_bytes: rx.allocated_video_bytes(&sid),
                });
                continue;
            }
        };
        let (status, resp_bytes, error_code) = match rx.handle(&sid, &req, now) {
            Ok(resp) => {
                let code = resp.status.code();
                (Some(code), resp.encode().len(), None)
            }
            Err(e) => {
                let code = code_str(&e);
                if label.starts_with("POST /fp-setup") {
                    fp_setup_error = Some(code.clone());
                }
                (None, 0, Some(code))
            }
        };
        steps.push(StepReport {
            step: label.to_string(),
            bytes_in: bytes.len(),
            status,
            response_bytes: resp_bytes,
            error_code,
            state_after: state_wire(rx.state(&sid).unwrap_or(SessionState::Ended)),
            has_stream: rx.has_stream(&sid),
            allocated_video_bytes: rx.allocated_video_bytes(&sid),
        });
        if label.starts_with("SETUP") {
            setup_status = status.unwrap_or(0);
            state_after_setup = state_wire(rx.state(&sid).unwrap_or(SessionState::Ended));
            has_stream_after_setup = rx.has_stream(&sid);
            allocated_after_setup = rx.allocated_video_bytes(&sid);
        }
    }

    // 只有真的建了流才 PLAY
    if has_stream_after_setup {
        now += 10;
        let bytes = fixture_request("PLAY", "/stream", &[("CSeq", "5"), ("Content-Length", "0")], b"");
        if let Ok((req, _)) = RtspRequest::parse(&bytes) {
            let (status, resp_bytes, error_code) = match rx.handle(&sid, &req, now) {
                Ok(resp) => (Some(resp.status.code()), resp.encode().len(), None),
                Err(e) => (None, 0, Some(code_str(&e))),
            };
            steps.push(StepReport {
                step: "PLAY /stream".to_string(),
                bytes_in: bytes.len(),
                status,
                response_bytes: resp_bytes,
                error_code,
                state_after: state_wire(rx.state(&sid).unwrap_or(SessionState::Ended)),
                has_stream: rx.has_stream(&sid),
                allocated_video_bytes: rx.allocated_video_bytes(&sid),
            });
        }
    }

    // TEARDOWN：端口/缓冲回收
    now += 10;
    let bytes = fixture_request("TEARDOWN", "/stream", &[("CSeq", "6"), ("Content-Length", "0")], b"");
    if let Ok((req, _)) = RtspRequest::parse(&bytes) {
        let (status, resp_bytes, error_code) = match rx.handle(&sid, &req, now) {
            Ok(resp) => (Some(resp.status.code()), resp.encode().len(), None),
            Err(e) => (None, 0, Some(code_str(&e))),
        };
        steps.push(StepReport {
            step: "TEARDOWN /stream".to_string(),
            bytes_in: bytes.len(),
            status,
            response_bytes: resp_bytes,
            error_code,
            state_after: state_wire(rx.state(&sid).unwrap_or(SessionState::Ended)),
            has_stream: rx.has_stream(&sid),
            allocated_video_bytes: rx.allocated_video_bytes(&sid),
        });
    }
    let has_stream_after_teardown = rx.has_stream(&sid);
    let reserved_ports_after_teardown = rx.reserved_ports();
    if has_stream_after_teardown || reserved_ports_after_teardown != 0 {
        ok = false;
    }

    rx.disconnect(&sid, now + 1);
    let tracked_sessions_after_disconnect = rx.tracked_sessions();
    if tracked_sessions_after_disconnect != 0 {
        ok = false;
    }

    let features: Vec<String> = advertised_features(&ImplementationInventory::current())
        .iter()
        .map(|f| f.as_wire().to_string())
        .collect();

    let report = KeyingRunReport {
        keying: name,
        warning,
        advertised_features: features,
        steps,
        setup_status,
        state_after_setup,
        has_stream_after_setup,
        allocated_video_bytes_after_setup: allocated_after_setup,
        has_stream_after_teardown,
        reserved_ports_after_teardown,
        fp_setup_error,
        tracked_sessions_after_disconnect,
    };
    (report, ok)
}

fn failed_keying_run(keying: &'static str, warning: Option<&'static str>) -> KeyingRunReport {
    KeyingRunReport {
        keying,
        warning,
        advertised_features: vec![],
        steps: vec![],
        setup_status: 0,
        state_after_setup: "none",
        has_stream_after_setup: false,
        allocated_video_bytes_after_setup: 0,
        has_stream_after_teardown: false,
        reserved_ports_after_teardown: 0,
        fp_setup_error: None,
        tracked_sessions_after_disconnect: 0,
    }
}

// ---------------------------------------------------------------- sink 面

/// 自制媒体帧：1 条 codec config + 5 条 encoded access unit（30fps 的 33_333µs 步进）。
fn fixture_frames() -> Vec<MediaFrame> {
    let mut frames = vec![MediaFrame {
        kind: FrameKind::CodecConfig,
        stream_id: 1,
        flags: 0,
        sequence: 0,
        pts: 0,
        payload: (0u8..16).map(|i| i.wrapping_mul(11)).collect(),
    }];
    for i in 1..=5u64 {
        frames.push(MediaFrame {
            kind: FrameKind::EncodedAccessUnit,
            stream_id: 1,
            flags: if i == 1 { xflags::KEYFRAME } else { 0 },
            sequence: i,
            pts: (i as i64) * 33_333,
            payload: vec![(i as u8).wrapping_mul(7); (i as usize) * 137],
        });
    }
    frames
}

pub fn sink_scenario() -> (SinkReport, bool) {
    let mut ok = true;
    let dims = match Dimensions::parse("1920x1080") {
        Ok(d) => Some(d),
        Err(_) => {
            ok = false;
            None
        }
    };
    let frames = fixture_frames();
    let frames_fed = frames.len() as u64;

    // null sink：headless 统计（不初始化任何 UI）
    let mut null = match open_sink(
        SinkKind::Null,
        SinkOptions {
            dimensions: dims,
            ..SinkOptions::default()
        },
    ) {
        Ok(s) => s,
        Err(_) => {
            return (failed_sink_report(frames_fed), false)
        }
    };
    if null.on_config("h264", 1, dims).is_err() {
        ok = false;
    }
    for f in &frames {
        if null.on_frame(f).is_err() {
            ok = false;
        }
    }
    let null_stats = match null.finish() {
        Ok(s) => s,
        Err(_) => {
            ok = false;
            Default::default()
        }
    };

    // file sink：自有内容落盘（codec config 不进内容文件）
    let path = std::env::temp_dir().join(format!("xinterop-demo-{}.bin", std::process::id()));
    let mut file = match open_sink(
        SinkKind::File,
        SinkOptions {
            dimensions: dims,
            path: Some(path.clone()),
            max_pixels: None,
        },
    ) {
        Ok(s) => s,
        Err(_) => {
            return (failed_sink_report(frames_fed), false)
        }
    };
    if file.on_config("h264", 1, dims).is_err() {
        ok = false;
    }
    for f in &frames {
        if file.on_frame(f).is_err() {
            ok = false;
        }
    }
    let file_stats = match file.finish() {
        Ok(s) => s,
        Err(_) => {
            ok = false;
            Default::default()
        }
    };
    let file_size = std::fs::metadata(&path).map(|m| m.len()).ok();
    let au_bytes: u64 = frames
        .iter()
        .filter(|f| matches!(f.kind, FrameKind::EncodedAccessUnit))
        .map(|f| f.payload.len() as u64)
        .sum();
    if file_size != Some(au_bytes) || file_stats.bytes != au_bytes {
        ok = false;
    }

    // 音频：44.1k 源 → 48k 设备必须显式重采样且时长不变
    let audio = match ResamplePlan::plan(44_100, 48_000) {
        Ok(plan) => {
            let input: Vec<i16> = (0..441 * 2)
                .map(|i: i32| ((i % 200 - 100) * 200).clamp(i16::MIN as i32, i16::MAX as i32) as i16)
                .collect();
            let out = plan.apply_s16le(&input, 2);
            let frames_in = input.len() / 2;
            let frames_out = out.len() / 2;
            let duration_ms_in = (frames_in as u64 * 1000) / 44_100;
            let duration_ms_out = (frames_out as u64 * 1000) / 48_000;
            let speed_ratio = if frames_in == 0 {
                0.0
            } else {
                (frames_out as f64 / frames_in as f64) / (48_000.0 / 44_100.0)
            };
            if frames_out != 480 || duration_ms_in != duration_ms_out {
                ok = false;
            }
            let plan_report = match plan {
                ResamplePlan::Passthrough => PlanReport {
                    kind: "passthrough",
                    source_rate: 44_100,
                    device_rate: 48_000,
                    ratio_num: 1,
                    ratio_den: 1,
                },
                ResamplePlan::Resample {
                    source_rate,
                    device_rate,
                    ratio_num,
                    ratio_den,
                } => PlanReport {
                    kind: "resample",
                    source_rate,
                    device_rate,
                    ratio_num,
                    ratio_den,
                },
            };
            AudioReport {
                plan: plan_report,
                frames_in,
                frames_out,
                duration_ms_in,
                duration_ms_out,
                speed_ratio,
            }
        }
        Err(_) => {
            ok = false;
            AudioReport {
                plan: PlanReport {
                    kind: "invalid",
                    source_rate: 0,
                    device_rate: 0,
                    ratio_num: 0,
                    ratio_den: 0,
                },
                frames_in: 0,
                frames_out: 0,
                duration_ms_in: 0,
                duration_ms_out: 0,
                speed_ratio: 0.0,
            }
        }
    };

    // 同率直通：不得重采样
    let passthrough = match ResamplePlan::plan(48_000, 48_000) {
        Ok(ResamplePlan::Passthrough) => {
            let input: Vec<i16> = (0..480 * 2).map(|i: i32| (i % 300) as i16).collect();
            let out = ResamplePlan::Passthrough.apply_s16le(&input, 2);
            if out.len() != input.len() {
                ok = false;
            }
            PlanReport {
                kind: "passthrough",
                source_rate: 48_000,
                device_rate: 48_000,
                ratio_num: 1,
                ratio_den: 1,
            }
        }
        Ok(_) => {
            ok = false;
            PlanReport {
                kind: "resample(unexpected)",
                source_rate: 48_000,
                device_rate: 48_000,
                ratio_num: 0,
                ratio_den: 0,
            }
        }
        Err(_) => {
            ok = false;
            PlanReport {
                kind: "invalid",
                source_rate: 48_000,
                device_rate: 48_000,
                ratio_num: 0,
                ratio_den: 0,
            }
        }
    };

    // native 窗口：本阶段明确不可用（无 GUI 自动化）
    let native_window = match open_sink(
        SinkKind::NativeWindow,
        SinkOptions {
            dimensions: dims,
            ..SinkOptions::default()
        },
    ) {
        Ok(_) => {
            ok = false;
            CaseOutcome {
                case: "native window sink".to_string(),
                outcome: "available(unexpected)",
                error_code: None,
            }
        }
        Err(e) => CaseOutcome {
            case: "native window sink".to_string(),
            outcome: "unavailable",
            error_code: Some(code_str(&e)),
        },
    };

    // 巨大 dimensions：分配前拒绝
    let oversized_dimensions = match Dimensions::parse("20000x12000") {
        Ok(d) => {
            let code = match open_sink(SinkKind::Null, SinkOptions { dimensions: Some(d), ..SinkOptions::default() }) {
                Ok(_) => {
                    ok = false;
                    "accepted(unexpected)".to_string()
                }
                Err(e) => code_str(&e),
            };
            OversizeReport {
                requested: "20000x12000".to_string(),
                area: d.area(),
                max_pixels: MAX_PIXELS,
                error_code: code,
            }
        }
        Err(_) => {
            ok = false;
            OversizeReport {
                requested: "20000x12000".to_string(),
                area: 0,
                max_pixels: MAX_PIXELS,
                error_code: "parse-failed".to_string(),
            }
        }
    };
    if oversized_dimensions.error_code != "resource-limit"
        || native_window.error_code.as_deref() != Some("platform-unavailable")
    {
        ok = false;
    }

    let report = SinkReport {
        frames_fed,
        null: stats_report("null", &null_stats, None),
        file: stats_report("file", &file_stats, file_size),
        audio,
        passthrough,
        native_window,
        oversized_dimensions,
    };
    (report, ok)
}

fn stats_report(
    kind: &'static str,
    s: &interop_media::sinks::SinkStats,
    file_size_bytes: Option<u64>,
) -> SinkStatsReport {
    SinkStatsReport {
        kind,
        frames: s.frames,
        bytes: s.bytes,
        first_pts: s.first_pts,
        last_pts: s.last_pts,
        format_changes: s.format_changes,
        codec: s.codec.clone(),
        dimensions: s.dimensions.map(|d| format!("{}x{}", d.width, d.height)),
        path: s.path.clone(),
        file_size_bytes,
    }
}

fn failed_sink_report(frames_fed: u64) -> SinkReport {
    let empty = SinkStatsReport {
        kind: "failed",
        frames: 0,
        bytes: 0,
        first_pts: None,
        last_pts: None,
        format_changes: 0,
        codec: None,
        dimensions: None,
        path: None,
        file_size_bytes: None,
    };
    SinkReport {
        frames_fed,
        null: empty.clone(),
        file: empty,
        audio: AudioReport {
            plan: PlanReport {
                kind: "invalid",
                source_rate: 0,
                device_rate: 0,
                ratio_num: 0,
                ratio_den: 0,
            },
            frames_in: 0,
            frames_out: 0,
            duration_ms_in: 0,
            duration_ms_out: 0,
            speed_ratio: 0.0,
        },
        passthrough: PlanReport {
            kind: "invalid",
            source_rate: 0,
            device_rate: 0,
            ratio_num: 0,
            ratio_den: 0,
        },
        native_window: CaseOutcome {
            case: "native window sink".to_string(),
            outcome: "not-run",
            error_code: None,
        },
        oversized_dimensions: OversizeReport {
            requested: "20000x12000".to_string(),
            area: 0,
            max_pixels: MAX_PIXELS,
            error_code: "not-run".to_string(),
        },
    }
}

// ---------------------------------------------------------------- 数据面

pub fn media_scenario() -> (MediaPlaneReport, bool) {
    let mut ok = true;
    let frame = MediaFrame {
        kind: FrameKind::EncodedAccessUnit,
        stream_id: 7,
        flags: xflags::KEYFRAME,
        sequence: 1,
        pts: 33_333,
        payload: (0u8..64).map(|i| i.wrapping_mul(3)).collect(),
    };
    let encoded = match frame.try_encode() {
        Ok(b) => b,
        Err(_) => {
            return (failed_media_report(), false)
        }
    };
    let header_hex = hex(&encoded[..interop_ipc::media_frame::HEADER_LEN]);
    let (decoded, consumed) = match MediaFrame::decode(&encoded) {
        Ok(v) => v,
        Err(_) => {
            return (failed_media_report(), false)
        }
    };
    let decoded_equal = decoded == frame && consumed == encoded.len();
    if !decoded_equal {
        ok = false;
    }

    let mut negatives: Vec<CaseOutcome> = Vec::new();
    // 1) 截断头
    negatives.push(expect_reject(
        "截断头（20 < 36 字节）",
        MediaFrame::decode(&encoded[..20]).err(),
        "invalid-frame",
        &mut ok,
    ));
    // 2) 未知 kind：fail-closed，不猜测
    let mut unknown_kind = encoded.clone();
    unknown_kind[6..8].copy_from_slice(&9u16.to_be_bytes());
    negatives.push(expect_reject(
        "未知 kind=9",
        MediaFrame::decode(&unknown_kind).err(),
        "invalid-frame",
        &mut ok,
    ));
    // 3) 未知 critical flag：位 0x8000_0000
    let mut unknown_flag = encoded.clone();
    unknown_flag[12..16].copy_from_slice(&0x8000_0000u32.to_be_bytes());
    negatives.push(expect_reject(
        "未知 critical flag 0x80000000",
        MediaFrame::decode(&unknown_flag).err(),
        "invalid-frame",
        &mut ok,
    ));
    // 4) 声明 payload 17MiB（超过 16MiB 上限）→ **分配前** resource-limit
    let mut oversize_header = Vec::with_capacity(interop_ipc::media_frame::HEADER_LEN);
    oversize_header.extend_from_slice(b"XMD1");
    oversize_header.extend_from_slice(&1u16.to_be_bytes());
    oversize_header.extend_from_slice(&1u16.to_be_bytes());
    oversize_header.extend_from_slice(&1u32.to_be_bytes());
    oversize_header.extend_from_slice(&0u32.to_be_bytes());
    oversize_header.extend_from_slice(&0u64.to_be_bytes());
    oversize_header.extend_from_slice(&0i64.to_be_bytes());
    oversize_header.extend_from_slice(&(17u32 * 1024 * 1024).to_be_bytes());
    negatives.push(expect_reject(
        "声明 payload 17MiB（无实际数据）",
        MediaFrame::decode(&oversize_header).err(),
        "resource-limit",
        &mut ok,
    ));

    let report = MediaPlaneReport {
        header_len: interop_ipc::media_frame::HEADER_LEN,
        max_payload: interop_ipc::media_frame::MAX_PAYLOAD,
        known_flags: xflags::KNOWN,
        roundtrip: RoundtripReport {
            payload_bytes: frame.payload.len(),
            encoded_bytes: encoded.len(),
            consumed,
            decoded_equal,
            header_hex,
        },
        negatives,
    };
    (report, ok)
}

fn expect_reject(
    case: &str,
    err: Option<Error>,
    expected: &str,
    ok: &mut bool,
) -> CaseOutcome {
    match err {
        Some(e) => {
            let code = code_str(&e);
            if code != expected {
                *ok = false;
            }
            CaseOutcome {
                case: case.to_string(),
                outcome: "rejected",
                error_code: Some(code),
            }
        }
        None => {
            *ok = false;
            CaseOutcome {
                case: case.to_string(),
                outcome: "accepted(unexpected)",
                error_code: None,
            }
        }
    }
}

fn failed_media_report() -> MediaPlaneReport {
    MediaPlaneReport {
        header_len: interop_ipc::media_frame::HEADER_LEN,
        max_payload: interop_ipc::media_frame::MAX_PAYLOAD,
        known_flags: xflags::KNOWN,
        roundtrip: RoundtripReport {
            payload_bytes: 0,
            encoded_bytes: 0,
            consumed: 0,
            decoded_equal: false,
            header_hex: String::new(),
        },
        negatives: vec![],
    }
}
