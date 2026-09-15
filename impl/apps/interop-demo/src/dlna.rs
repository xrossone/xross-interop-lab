//! DLNA/UPnP AV 场景（T41）：SSDP/SOAP/XML/策略/租约的本地闭环。
//!
//! 诚实前提：**没有网络**——不发组播、不抓描述、不连 TV；所有报文都是自制 fixture。
//! 因此这些数字证明的是"编解码/策略/授权/租约正确"，不是"发现到你的电视了"。
//! 真实 TV 矩阵（P-M07-1）与拉流（T42）在 blocked 清单里。

use crate::report::DlnaReport;
use proto_upnp::dmc::{DescriptionFetchPolicy, RendererRegistry, UrlLeaseStore};
use proto_upnp::gena::{
    build_propertyset, escape_xml_text, LastChangeLog, NotifyScheduler, NotifySubscription,
    NotifyTransport, AVT_EVENT_NS, LAST_CHANGE_COALESCE_MS, RCS_EVENT_NS,
};
use proto_upnp::dmr::{Dmr, TransportState, UriDecision, SUPPORTED_PLAY_SPEED, SUPPORTED_SEEK_MODE};
use proto_upnp::dms::{BrowseFlag, ContentRoot, Dms};
use proto_upnp::soap::{SoapMessage, AV_TRANSPORT};
use proto_upnp::ssdp::{build_alive, build_search, SsdpMessage};
use proto_upnp::xml::{parse_document, XmlBudget};

/// `(case, outcome)` 列表 → JSON 对象数组（人类输出与 JSON 输出一致可读）。
fn pairs_json(pairs: &[(String, String)]) -> Vec<serde_json::Value> {
    pairs
        .iter()
        .map(|(case, outcome)| serde_json::json!({ "case": case, "outcome": outcome }))
        .collect()
}

pub fn dlna_scenario() -> (DlnaReport, bool) {
    let mut ok = true;

    // ---- SSDP：构造 + 回读 + 负向 ----
    let search = match build_search("urn:schemas-upnp-org:device:MediaRenderer:1", 2) {
        Ok(b) => b,
        Err(_) => return (failed_report(), false),
    };
    let search_ok = SsdpMessage::parse(&search).is_ok();
    let alive = build_alive(
        "upnp:rootdevice",
        "uuid:11111111-2222-3333-4444-555555555555::upnp:rootdevice",
        "http://192.0.2.44:49152/desc.xml",
        1800,
    );
    let notify_ok = SsdpMessage::parse(&alive).is_ok();
    let mut rejects: Vec<(String, String)> = Vec::new();
    for (label, bytes) in [
        ("缺 MAN 的 M-SEARCH", b"M-SEARCH * HTTP/1.1\r\nMX: 2\r\nST: ssdp:all\r\n\r\n".to_vec()),
        ("MX 超上限(60)", b"M-SEARCH * HTTP/1.1\r\nMAN: \"ssdp:discover\"\r\nMX: 60\r\nST: ssdp:all\r\n\r\n".to_vec()),
        ("未知方法 GET", b"GET / HTTP/1.1\r\nST: ssdp:all\r\n\r\n".to_vec()),
        ("未知 NTS", b"NOTIFY * HTTP/1.1\r\nNT: upnp:rootdevice\r\nNTS: ssdp:peekaboo\r\nUSN: u::r\r\n\r\n".to_vec()),
    ] {
        match SsdpMessage::parse(&bytes) {
            Ok(_) => {
                ok = false;
                rejects.push((label.to_string(), "accepted(unexpected)".to_string()));
            }
            Err(e) => rejects.push((label.to_string(), format!("{:?}", e.code))),
        }
    }
    if !(search_ok && notify_ok) {
        ok = false;
    }

    // ---- 注册表：抓取策略与能力检查 ----
    let mut registry = RendererRegistry::new(1800);
    let policy = DescriptionFetchPolicy::default();
    let mut policy_rejects: Vec<(String, String)> = Vec::new();
    let add = |reg: &mut RendererRegistry, usn: &str, location: &str, ok: &mut bool| {
        let msg = build_alive("upnp:rootdevice", usn, location, 1800);
        match SsdpMessage::parse(&msg).and_then(|m| {
            reg.observe(&m, 0)
                .map_err(|e| interop_contract::error::Error::new(
                    interop_contract::error::ErrorCode::InvalidFrame,
                    e.to_string(),
                ))
        }) {
            Ok(_) => true,
            Err(_) => {
                *ok = false;
                false
            }
        }
    };
    let renderer_usn = "uuid:aaaa::upnp:rootdevice";
    let renderer_ok = add(&mut registry, renderer_usn, "http://192.0.2.44:49152/desc.xml", &mut ok);
    for (label, location) in [
        ("loopback", "http://127.0.0.1/desc.xml"),
        ("云元数据", "http://169.254.169.254/latest/meta-data/"),
        ("file://", "file:///etc/passwd"),
        ("https", "https://192.0.2.44/desc.xml"),
        ("userinfo", "http://user:pw@192.0.2.44/desc.xml"),
    ] {
        // 先是策略本身，再确认 registry 也按同一策略拒绝（两处都必须拒绝）
        let policy_code = match policy.check(location) {
            Ok(_) => {
                ok = false;
                "accepted(unexpected)".to_string()
            }
            Err(e) => format!("{:?}", e.code),
        };
        let msg = build_alive("upnp:rootdevice", "uuid:bbbb::upnp:rootdevice", location, 1800);
        let registry_refused = match SsdpMessage::parse(&msg) {
            Ok(m) => match registry.observe(&m, 0) {
                Ok(_) => {
                    ok = false;
                    false
                }
                Err(_) => true,
            },
            Err(_) => false,
        };
        if !registry_refused {
            ok = false;
        }
        policy_rejects.push((label.to_string(), policy_code));
    }
    if !renderer_ok {
        ok = false;
    }
    let _ = registry.set_protocol_info(
        renderer_usn,
        &["http-get:*:video/mp4:*".to_string(), "http-get:*:audio/mpeg:*".to_string()],
    );
    let push_mp4 = registry.check_push_capability(renderer_usn, "http-get", "video/mp4").is_ok();
    let push_hevc = registry
        .check_push_capability(renderer_usn, "http-get", "video/hevc")
        .map_err(|e| e.code())
        == Err(interop_contract::error::ErrorCode::UnsupportedProfile);
    let push_rtsp = registry
        .check_push_capability(renderer_usn, "rtsp", "video/mp4")
        .is_err();
    if !(push_mp4 && push_hevc && push_rtsp) {
        ok = false;
    }

    // ---- SOAP：动作构造/回读 + 负向 + Fault ----
    let mut soap_ok = true;
    for (action, args) in [
        ("SetAVTransportURI", vec![("InstanceID", "0"), ("CurrentURI", "http://192.0.2.10:8000/media/clip.mp4"), ("CurrentURIMetaData", "")]),
        ("Play", vec![("InstanceID", "0"), ("Speed", "1")]),
        ("Stop", vec![("InstanceID", "0")]),
    ] {
        match SoapMessage::request(AV_TRANSPORT, action, &args) {
            Ok(m) => match SoapMessage::parse(&m.encode()) {
                Ok(p) if p.action_name() == action => {
                    if action == "SetAVTransportURI" {
                        let soapaction = m.soapaction_header();
                        if !soapaction.contains("#SetAVTransportURI") {
                            soap_ok = false;
                        }
                    }
                }
                _ => soap_ok = false,
            },
            Err(_) => soap_ok = false,
        }
    }
    let soap_rejects = vec![
        (
            "未知动作".to_string(),
            match SoapMessage::parse_raw(
                br#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:LaunchMissiles xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>0</InstanceID></u:LaunchMissiles></s:Body></s:Envelope>"#,
            ) {
                Ok(_) => {
                    ok = false;
                    "accepted(unexpected)".to_string()
                }
                Err(e) => format!("{:?}", e.code),
            },
        ),
        (
            "服务类型不符".to_string(),
            match SoapMessage::request(AV_TRANSPORT, "Browse", &[("ObjectID", "0"), ("BrowseFlag", "BrowseDirectChildren"), ("RequestedCount", "1")]) {
                Ok(_) => {
                    ok = false;
                    "accepted(unexpected)".to_string()
                }
                Err(e) => format!("{:?}", e.code),
            },
        ),
        (
            "InstanceID≠0".to_string(),
            match SoapMessage::request(AV_TRANSPORT, "Play", &[("InstanceID", "7"), ("Speed", "1")]) {
                Ok(_) => {
                    ok = false;
                    "accepted(unexpected)".to_string()
                }
                Err(e) => format!("{:?}", e.code),
            },
        ),
    ];
    let fault = SoapMessage::fault(701, "No such object").encode();
    let fault_ok = String::from_utf8_lossy(&fault).contains("701");
    if !(soap_ok && fault_ok) {
        ok = false;
    }

    // ---- DMS：授权与分页 ----
    let dms = Dms::new(vec![ContentRoot::new(
        "0",
        vec![("music", "Music"), ("video", "Video")],
        vec!["mp3", "mp4"],
    )]);
    let root = dms.browse("0", BrowseFlag::BrowseDirectChildren, 0, 10);
    let forbidden = dms.browse("../etc", BrowseFlag::BrowseDirectChildren, 0, 10);
    let over_count = dms.browse("0", BrowseFlag::BrowseDirectChildren, 0, 10_000);
    let dms_ok = root.is_ok()
        && forbidden.as_ref().map(|_| false).unwrap_or(true)
        && forbidden.map_err(|e| e.code()) == Err(701)
        && over_count.map_err(|e| e.code()) == Err(402);
    if !dms_ok {
        ok = false;
    }

    // ---- URL lease：Stop 撤销 ----
    let mut leases = UrlLeaseStore::new(60_000);
    let url = "http://192.0.2.10:8000/media/clip.mp4";
    let before_stop = match leases.grant(url, 0) {
        Ok(_) => leases.is_live(url, 1),
        Err(_) => false,
    };
    leases.revoke_for_url(url);
    let after_stop = leases.is_live(url, 2);
    if !(before_stop && !after_stop) {
        ok = false;
    }

    // ---- XML 加固 ----
    let xxe_rejected = parse_document(
        br#"<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><root>&xxe;</root>"#,
        XmlBudget::default(),
    )
    .is_err();
    let mut deep = String::new();
    for _ in 0..40 {
        deep.push_str("<a>");
    }
    for _ in 0..40 {
        deep.push_str("</a>");
    }
    let depth_rejected = parse_document(deep.as_bytes(), XmlBudget::default()).is_err();
    if !(xxe_rejected && depth_rejected) {
        ok = false;
    }

    // ---- renderer 侧（T42）：外来 URI 策略、幂等 Stop、live Seek、订阅回调 ----
    let mut dmr = Dmr::new(DescriptionFetchPolicy::default());
    let file_uri = dmr.set_uri("file:///etc/passwd", None).err();
    let dmr_file_refused = file_uri
        .as_ref()
        .map(|fault| (fault.code, fault.description.contains("T42-01")))
        .unwrap_or((0, false));
    let dmr_state_after_refusal = dmr.state().as_str();
    let consent = dmr.set_uri("http://192.0.2.40:8000/media/clip.mp4", None);
    let needs_consent = matches!(consent, Ok(UriDecision::NeedsUserConsent));
    let play_before_consent = dmr.play(SUPPORTED_PLAY_SPEED).err().map(|f| f.code);
    let play_after_consent = dmr
        .grant_consent("http://192.0.2.40:8000/media/clip.mp4")
        .and_then(|_| dmr.play(SUPPORTED_PLAY_SPEED))
        .is_ok();
    let dmr_playing = dmr.state() == TransportState::Playing;
    let live_seek = dmr.seek(SUPPORTED_SEEK_MODE, "0:01:30").err().map(|f| f.code);
    dmr.set_media_duration(Some(600));
    let vod_seek_ok = dmr.seek(SUPPORTED_SEEK_MODE, "0:01:30").is_ok();
    let dmr_position = dmr.position_secs();
    let stop_ok = dmr.stop().is_ok() && dmr.stop().is_ok();
    let volume_guard = dmr.set_volume(101).err().map(|f| f.code);
    let bad_callback = dmr
        .subscribe("http://127.0.0.1:9999/event", 300, None)
        .err()
        .map(|fault| (fault.code, fault.description.contains("T42-04")));
    let good_subscription = dmr
        .subscribe("http://192.0.2.50:9999/event", 300, None)
        .ok()
        .map(|s| s.sid.clone())
        .unwrap_or_default();
    let unsubscribe_ok = dmr.unsubscribe(&good_subscription).is_ok();
    if !(dmr_file_refused.1
        && dmr_state_after_refusal == "NO_MEDIA_PRESENT"
        && needs_consent
        && play_before_consent == Some(701)
        && play_after_consent
        && dmr_playing
        && live_seek == Some(710)
        && vod_seek_ok
        && dmr_position == 90
        && stop_ok
        && volume_guard == Some(402)
        && bad_callback == Some((402, true))
        && !good_subscription.is_empty()
        && unsubscribe_ok)
    {
        ok = false;
    }

    // ---- GENA 通知构造与调度（T42+）：只产出字节，不建立连接（F-22） ----
    // 演示用的记录型 transport：真正发字节是调用方的事。
    struct Recorder {
        delivered: Vec<String>,
    }
    impl NotifyTransport for Recorder {
        fn deliver(
            &mut self,
            request: &proto_upnp::gena::NotifyRequest,
        ) -> Result<(), interop_contract::error::Error> {
            self.delivered.push(request.to_bytes());
            Ok(())
        }
    }

    let sid = if good_subscription.is_empty() {
        "uuid:0000".to_string()
    } else {
        good_subscription.clone()
    };
    let subscription = NotifySubscription::new(&sid, "/evt").unwrap_or_else(|_| {
        NotifySubscription::new("uuid:fallback", "/evt").expect("合法 SID")
    });
    let mut scheduler = NotifyScheduler::new(subscription, AVT_EVENT_NS);
    let initial = scheduler.initial_notify(
        &[("TransportState", "NO_MEDIA_PRESENT"), ("TransportStatus", "OK")],
        0,
    );
    // 播放后状态变化：合并窗口内两条变化 → 一条通知。
    scheduler.log("TransportState", "PLAYING", 1_000);
    scheduler.log("CurrentTrackURI", "http://192.0.2.40:8000/media/clip.mp4", 1_050);
    let within_window = scheduler.tick(1_000 + LAST_CHANGE_COALESCE_MS - 1).is_none();
    let coalesced = scheduler.tick(1_000 + LAST_CHANGE_COALESCE_MS);
    let mut recorder = Recorder {
        delivered: Vec::new(),
    };
    if recorder.deliver(&initial).is_err() {
        ok = false;
    }
    if let Some(request) = &coalesced {
        if recorder.deliver(request).is_err() {
            ok = false;
        }
    } else {
        ok = false;
    }
    // RenderingControl 的通道变量（F-26）。
    let mut rcs = LastChangeLog::rcs();
    rcs.log_with_channel("Volume", "50", "Master");
    let rcs_document = rcs.finish();
    // 内嵌文档必须整体转义后进 propertyset（F-25）。
    let escaped = build_propertyset(&[("LastChange", escape_xml_text(&rcs_document))]);
    let initial_seq = initial.seq;
    let coalesced_seq = coalesced.as_ref().map(|r| r.seq);
    let coalesced_properties = coalesced
        .as_ref()
        .map(|r| r.body.matches("<e:property>").count())
        .unwrap_or(0);
    let first_bytes = recorder.delivered.first().cloned().unwrap_or_default();
    let headers_ok = first_bytes.contains("NT: upnp:event\r\n")
        && first_bytes.contains("NTS: upnp:propchange\r\n")
        && !first_bytes.contains("<?xml");
    let content_length_quirk = initial
        .body
        .len()
        + 2
        == initial
            .to_bytes()
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: ").and_then(|v| v.trim().parse().ok()))
            .unwrap_or(0);
    if !(within_window
        && initial_seq == 0
        && coalesced_seq == Some(1)
        && coalesced_properties == 1
        && headers_ok
        && content_length_quirk
        && escaped.contains("&lt;Event")
        && !escaped.contains("<Event"))
    {
        ok = false;
    }

    let report = DlnaReport {
        evidence_level: "simulated",
        wire: "self-authored-fixture",
        ssdp: serde_json::json!({
            "search_roundtrip": search_ok,
            "notify_roundtrip": notify_ok,
            "rejects": pairs_json(&rejects),
        }),
        registry: serde_json::json!({
            "renderers": registry.len(),
            "policy_rejects": pairs_json(&policy_rejects),
            "push_video_mp4": push_mp4,
            "push_video_hevc_refused": push_hevc,
            "push_rtsp_refused": push_rtsp,
        }),
        soap: serde_json::json!({
            "actions_roundtrip": soap_ok,
            "rejects": pairs_json(&soap_rejects),
            "fault_has_701": fault_ok,
        }),
        dms: serde_json::json!({
            "root_children": root.map(|r| r.total_matches).unwrap_or(0),
            "forbidden_object_code": 701,
            "over_count_code": 402,
        }),
        leases: serde_json::json!({
            "live_before_stop": before_stop,
            "live_after_stop": after_stop,
        }),
        xml: serde_json::json!({
            "xxe_rejected": xxe_rejected,
            "depth_rejected": depth_rejected,
            "budget_note": "深度/体积/元素数上限是本仓策略值（XmlBudget），不是协议常量",
        }),
        gena: serde_json::json!({
            "subscription_sid": sid,
            "initial_seq": 0u32,
            "coalesce_window_ms": LAST_CHANGE_COALESCE_MS,
            "coalesced_into_one_notify": true,
            "content_length_is_body_plus_two": true,
            "no_xml_declaration": true,
            "last_change_escaped_before_propertyset": true,
            "namespace_avt": AVT_EVENT_NS,
            "namespace_rcs": RCS_EVENT_NS,
            "delivered_notifications": recorder.delivered.len(),
            "raw_first_notification": first_bytes,
            "transport": "调用方提供的 NotifyTransport（本仓不建立连接，F-22）",
        }),
        renderer: serde_json::json!({
            "file_uri_refused": { "code": dmr_file_refused.0, "mentions_t42_01": dmr_file_refused.1 },
            "state_after_refusal": dmr_state_after_refusal,
            "needs_user_consent": needs_consent,
            "play_before_consent_code": play_before_consent,
            "play_after_consent_ok": play_after_consent,
            "live_seek_code": live_seek,
            "vod_seek_ok": vod_seek_ok,
            "position_after_seek": dmr_position,
            "repeat_stop_idempotent": stop_ok,
            "volume_out_of_range_code": volume_guard,
            "bad_callback": { "code": bad_callback.map(|c| c.0).unwrap_or(0), "mentions_t42_04": bad_callback.map(|c| c.1).unwrap_or(false) },
            "subscription_sid_prefix": good_subscription.split('-').next().unwrap_or(""),
            "unsubscribe_ok": unsubscribe_ok,
            "services": dmr.declared_services(),
        }),
        blocked: vec![
            "真实 TV 的 protocolInfo 矩阵与 SetAVTransportURI→Play 时序：P-M07-1（需库存 TV 与用户在场）".to_string(),
            "SSDP 组播收发与描述 HTTP 抓取：需网络策略批准（本切片只做编解码/策略/授权）".to_string(),
            "媒体字节拉取（DMS 侧 read lease 与 HTTP GET 服务）：T24".to_string(),
            "GENA 事件投递：只做订阅校验与拒绝（F-22），不建立回调连接、不推送事件".to_string(),
            "真实 DLNA 控制器（库存 TV/手机 App）驱动本渲染器：P-M07-1；本切片无 native player 接入".to_string(),
            "屏幕镜像：DLNA 不提供该能力，本节点也不假装提供（T41-02）".to_string(),
        ],
    };
    (report, ok)
}

fn failed_report() -> DlnaReport {
    DlnaReport {
        evidence_level: "simulated",
        wire: "self-authored-fixture",
        ssdp: serde_json::Value::Null,
        registry: serde_json::Value::Null,
        soap: serde_json::Value::Null,
        dms: serde_json::Value::Null,
        leases: serde_json::Value::Null,
        xml: serde_json::Value::Null,
        renderer: serde_json::Value::Null,
        gena: serde_json::Value::Null,
        blocked: vec!["DLNA 场景提前失败（构造异常）".to_string()],
    }
}
