//! T41 验收（plans/03-casting.md §T41，headless 切片）：DLNA/UPnP AV 控制路径。
//!
//! - T41-01 设备 description 指向恶意内网目标 → fetch policy 拒绝；
//! - T41-02 未知/不支持 codec → **禁止假装 screen mirror**；
//! - T41-03 ContentDirectory 请求越权 objectID → 拒绝（701）；
//! - T41-04 stop 播放 → URL lease 按策略撤销。
//!
//! 另有 SSDP/SOAP/XML 的加固断言（F-02/F-05/F-12）：DTD/XXE、深度/体积预算、未知动作、
//! SOAPACTION 服务类型不符。字段号来源见 `specs-reviewed/m07-dlna-upnp-av.md`。
//! 真机 TV（P-M07-1）不在本文件范围。

use interop_contract::error::ErrorCode;
use proto_upnp::dmc::{DescriptionFetchPolicy, DmcError, RendererRegistry, UrlLeaseStore};
use proto_upnp::dms::{BrowseFlag, ContentRoot, Dms};
use proto_upnp::soap::{SoapAction, SoapMessage};
use proto_upnp::ssdp::{SsdpMessage, SsdpMethod, M_SEARCH_MAN};
use proto_upnp::xml::{parse_document, XmlBudget};

const AVT: &str = "urn:schemas-upnp-org:service:AVTransport:1";

fn m_search(st: &str, mx: &str) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: {M_SEARCH_MAN}\r\nMX: {mx}\r\nST: {st}\r\n\r\n"
    )
}

/// SSDP：M-SEARCH/通告解析与按 USN 去重（F-02/F-03/F-04）。
#[test]
fn ssdp_search_and_notify_are_parsed_strictly() {
    let msg = SsdpMessage::parse(m_search("urn:schemas-upnp-org:device:MediaRenderer:1", "2").as_bytes())
        .expect("合法 M-SEARCH");
    match &msg {
        SsdpMessage::Search(req) => {
            assert_eq!(req.method, SsdpMethod::MSearch);
            assert_eq!(req.mx, 2);
            assert_eq!(req.st, "urn:schemas-upnp-org:device:MediaRenderer:1");
            assert!(req.man_is_discover);
        }
        other => panic!("期望 M-SEARCH，实得 {other:?}"),
    }

    // 负向：缺 MAN / MAN 值错 / MX 超上限 / 方法不对
    let no_man = "M-SEARCH * HTTP/1.1\r\nMX: 2\r\nST: ssdp:all\r\n\r\n";
    assert_eq!(
        SsdpMessage::parse(no_man.as_bytes()).expect_err("缺 MAN").code,
        ErrorCode::InvalidFrame
    );
    let bad_man = m_search("ssdp:all", "2").replace(M_SEARCH_MAN, "\"ssdp:not-discover\"");
    assert_eq!(
        SsdpMessage::parse(bad_man.as_bytes()).expect_err("MAN 值错").code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        SsdpMessage::parse(m_search("ssdp:all", "60").as_bytes())
            .expect_err("MX 超上限")
            .code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        SsdpMessage::parse(b"GET / HTTP/1.1\r\nST: ssdp:all\r\n\r\n")
            .expect_err("未知方法")
            .code,
        ErrorCode::InvalidFrame
    );

    // 通告：alive 带 LOCATION/CACHE-CONTROL/USN；byebye 只按 USN 失效
    let alive = "NOTIFY * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nNT: upnp:rootdevice\r\nNTS: ssdp:alive\r\nUSN: uuid:11111111-2222-3333-4444-555555555555::upnp:rootdevice\r\nLOCATION: http://192.0.2.44:49152/desc.xml\r\nCACHE-CONTROL: max-age=1800\r\n\r\n";
    let parsed = SsdpMessage::parse(alive.as_bytes()).expect("合法 NOTIFY");
    match &parsed {
        SsdpMessage::Notify(n) => {
            assert_eq!(n.nts, "ssdp:alive");
            assert_eq!(n.usn, "uuid:11111111-2222-3333-4444-555555555555::upnp:rootdevice");
            assert_eq!(n.location.as_deref(), Some("http://192.0.2.44:49152/desc.xml"));
            assert_eq!(n.max_age_secs, Some(1800));
        }
        other => panic!("期望 NOTIFY，实得 {other:?}"),
    }
    let byebye = alive.replace("ssdp:alive", "ssdp:byebye").replace("LOCATION: http://192.0.2.44:49152/desc.xml\r\n", "");
    match SsdpMessage::parse(byebye.as_bytes()).expect("合法 byebye") {
        SsdpMessage::Notify(n) => assert_eq!(n.nts, "ssdp:byebye"),
        other => panic!("期望 NOTIFY，实得 {other:?}"),
    }
}

/// SSDP：体积/头数预算（F-12 的策略面在 SSDP 同样适用）。
#[test]
fn ssdp_budgets_reject_oversized_messages() {
    let mut huge = m_search("ssdp:all", "2").into_bytes();
    huge.extend_from_slice(&vec![b'x'; 9 * 1024]);
    assert_eq!(
        SsdpMessage::parse(&huge).expect_err("超长 SSDP 报文").code,
        ErrorCode::ResourceLimit
    );

    let mut many_headers = String::from("M-SEARCH * HTTP/1.1\r\n");
    for i in 0..80 {
        many_headers.push_str(&format!("X-{i}: v\r\n"));
    }
    many_headers.push_str("\r\n");
    assert_eq!(
        SsdpMessage::parse(many_headers.as_bytes())
            .expect_err("头数超限")
            .code,
        ErrorCode::ResourceLimit
    );

    assert_eq!(
        SsdpMessage::parse(b"M-SEARCH * HTTP/1.1\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: \r\n\r\n")
            .expect_err("空 ST")
            .code,
        ErrorCode::InvalidFrame
    );
}

/// XML 加固：DTD/实体、深度、元素数、体积（F-12 的本仓策略）。
#[test]
fn xml_is_hardened_against_xxe_and_depth() {
    let budget = XmlBudget::default();

    // DTD → 拒绝（XXE）
    let xxe = br#"<?xml version="1.0"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><root>&xxe;</root>"#;
    assert_eq!(
        parse_document(xxe, budget).expect_err("DTD 必须拒绝").code,
        ErrorCode::InvalidFrame
    );

    // 标准实体允许
    let ok = parse_document(br#"<root attr="a&amp;b">text&lt;x&gt;</root>"#, budget).expect("合法 XML");
    assert_eq!(ok.name, "root");
    assert_eq!(ok.attrs.get("attr").map(String::as_str), Some("a&b"));

    // 深度超预算
    let mut deep = String::new();
    for _ in 0..(budget.max_depth + 2) {
        deep.push_str("<a>");
    }
    for _ in 0..(budget.max_depth + 2) {
        deep.push_str("</a>");
    }
    assert_eq!(
        parse_document(deep.as_bytes(), budget).expect_err("深度超限").code,
        ErrorCode::ResourceLimit
    );

    // 元素数超预算
    let mut many = String::from("<r>");
    for _ in 0..(budget.max_elements + 2) {
        many.push_str("<i/>");
    }
    many.push_str("</r>");
    assert_eq!(
        parse_document(many.as_bytes(), budget).expect_err("元素数超限").code,
        ErrorCode::ResourceLimit
    );

    // 体积超预算 + 非法标签名
    assert_eq!(
        parse_document(&vec![b'x'; budget.max_bytes + 1], budget)
            .expect_err("体积超限")
            .code,
        ErrorCode::ResourceLimit
    );
    assert_eq!(
        parse_document(b"<1bad></1bad>", budget).expect_err("非法标签名").code,
        ErrorCode::InvalidFrame
    );

    // 未闭合标签
    assert_eq!(
        parse_document(b"<root><a></root>", budget).expect_err("标签不匹配").code,
        ErrorCode::InvalidFrame
    );
}

/// SOAP：动作名/参数解析与构造；未知动作与服务类型不符一律拒绝（F-05/F-06）。
#[test]
fn soap_actions_are_validated_not_guessed() {
    // 构造 SetAVTransportURI 并读回
    let msg = SoapMessage::request(
        AVT,
        "SetAVTransportURI",
        &[
            ("InstanceID", "0"),
            ("CurrentURI", "http://192.0.2.10:8000/media/clip.mp4"),
            ("CurrentURIMetaData", ""),
        ],
    )
    .expect("构造");
    let bytes = msg.encode();
    let parsed = SoapMessage::parse(&bytes).expect("解析回读");
    assert_eq!(parsed.service_type, AVT);
    assert_eq!(parsed.action, Some(SoapAction::SetAvTransportUri));
    assert_eq!(parsed.arg("CurrentURI"), Some("http://192.0.2.10:8000/media/clip.mp4"));
    assert_eq!(parsed.arg("InstanceID"), Some("0"));
    assert_eq!(parsed.soapaction_header(), format!("\"{AVT}#SetAVTransportURI\""));

    // 其它动作
    for (name, args) in [
        ("Play", vec![("InstanceID", "0"), ("Speed", "1")]),
        ("Stop", vec![("InstanceID", "0")]),
        ("Seek", vec![("InstanceID", "0"), ("Unit", "REL_TIME"), ("Target", "00:00:10")]),
        ("GetTransportInfo", vec![("InstanceID", "0")]),
    ] {
        let m = SoapMessage::request(AVT, name, &args).expect(name);
        let parsed = SoapMessage::parse(&m.encode()).expect("回读");
        assert_eq!(parsed.action_name(), name);
    }

    // 负向：未知动作、服务类型不符、缺必需参数、InstanceID 非 0、SOAPACTION 头错
    // 构造期就拒绝未知动作（不生成"看起来合法"的消息）
    assert_eq!(
        SoapMessage::request(AVT, "LaunchMissiles", &[("InstanceID", "0")])
            .expect_err("未知动作必须构造期拒绝")
            .code,
        ErrorCode::UnsupportedFeature
    );
    let unknown = SoapMessage::parse_raw(
        br#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:TurnOnTheLights xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>0</InstanceID></u:TurnOnTheLights></s:Body></s:Envelope>"#,
    )
    .expect_err("未知动作必须拒绝");
    assert_eq!(unknown.code, ErrorCode::UnsupportedFeature);

    let wrong_service = SoapMessage::parse_raw(
        br#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:Play xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1"><InstanceID>0</InstanceID></u:Play></s:Body></s:Envelope>"#,
    )
    .expect_err("服务类型不符");
    assert_eq!(wrong_service.code, ErrorCode::UnsupportedFeature);

    let missing_arg = SoapMessage::parse_raw(
        br#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>0</InstanceID></u:SetAVTransportURI></s:Body></s:Envelope>"#,
    )
    .expect_err("缺 CurrentURI");
    assert_eq!(missing_arg.code, ErrorCode::InvalidFrame);

    let bad_instance = SoapMessage::parse_raw(
        br#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>7</InstanceID><Speed>1</Speed></u:Play></s:Body></s:Envelope>"#,
    )
    .expect_err("InstanceID 只支持 0");
    assert_eq!(bad_instance.code, ErrorCode::UnsupportedFeature);

    // 构造期：非法 URI 元数据尺寸
    assert!(SoapMessage::request(
        AVT,
        "SetAVTransportURI",
        &[("InstanceID", "0"), ("CurrentURI", "http://192.0.2.10/x"), ("CurrentURIMetaData", &"m".repeat(64 * 1024))],
    )
    .is_err());
}

/// T41-01：设备描述 URL 的抓取策略。
#[test]
fn t41_01_description_fetch_policy_rejects_malicious_targets() {
    let policy = DescriptionFetchPolicy::default();

    // 允许：普通可路由地址
    let ok = policy.check("http://192.0.2.44:49152/desc.xml").expect("允许内网文档地址");
    assert_eq!(ok.host, "192.0.2.44");
    assert_eq!(ok.port, 49152);

    for bad in [
        "file:///etc/passwd",
        "https://192.0.2.44/desc.xml",            // UPnP 用 http；https 未固化 → 拒绝
        "http://user:pw@192.0.2.44/desc.xml",     // 含 userinfo
        "http://127.0.0.1:8000/desc.xml",         // loopback
        "http://localhost:8000/desc.xml",         // loopback 名字
        "http://169.254.169.254/latest/meta-data/", // 云元数据
        "http://169.254.7.7/desc.xml",            // link-local
        "http://239.255.255.250:1900/desc.xml",   // 组播
        "http://0.0.0.0/desc.xml",                // 未指定地址
        "http://[::1]:8000/desc.xml",             // IPv6 loopback
        "http://192.0.2.44/desc.xml#frag",        // 带 fragment
        "rtsp://192.0.2.44/desc.xml",             // 非 http
    ] {
        let err = policy.check(bad).expect_err(&format!("必须拒绝 {bad}"));
        assert_eq!(
            err.code,
            ErrorCode::PermissionRequired,
            "{bad} 应以 permission-required 拒绝（策略拒绝而非语法错误）"
        );
    }

    // 超长 URL 属语法/预算问题
    let long = format!("http://192.0.2.44/{}.xml", "a".repeat(3000));
    assert_eq!(
        policy.check(&long).expect_err("超长 URL").code,
        ErrorCode::InvalidFrame
    );
}

/// T41-01（续）：registry 只接受通过策略的 LOCATION。
#[test]
fn t41_01_registry_drops_entries_with_bad_locations() {
    let mut reg = RendererRegistry::new(1800);
    let alive = |usn: &str, location: Option<&str>| {
        let mut s = format!("NOTIFY * HTTP/1.1\r\nNT: upnp:rootdevice\r\nNTS: ssdp:alive\r\nUSN: {usn}\r\n");
        if let Some(l) = location {
            s.push_str(&format!("LOCATION: {l}\r\n"));
        }
        s.push_str("\r\n");
        SsdpMessage::parse(s.as_bytes()).expect("fixture")
    };

    let good = reg
        .observe(&alive("uuid:aaaa::upnp:rootdevice", Some("http://192.0.2.44:49152/desc.xml")), 1_000)
        .expect("合法条目");
    assert_eq!(good.host, "192.0.2.44");
    assert_eq!(reg.len(), 1);

    // 恶意目标：不建条目（也不静默忽略——返回明确原因）
    let err = reg
        .observe(&alive("uuid:bbbb::upnp:rootdevice", Some("http://127.0.0.1/x.xml")), 1_000)
        .expect_err("loopback 必须拒绝");
    assert!(matches!(err, DmcError::FetchPolicy(_)));
    assert_eq!(reg.len(), 1, "被拒条目不得进入 registry");

    // 缺 LOCATION 的 alive：不可用（没有描述 URL 就无法做控制）
    let err = reg
        .observe(&alive("uuid:cccc::upnp:rootdevice", None), 1_000)
        .expect_err("缺 LOCATION");
    assert!(matches!(err, DmcError::MissingLocation));

    // 同一 USN 重复通告 → 更新而不是新增（F-04）
    reg.observe(&alive("uuid:aaaa::upnp:rootdevice", Some("http://192.0.2.45:49152/desc.xml")), 1_100)
        .expect("更新");
    assert_eq!(reg.len(), 1);
    assert_eq!(reg.get("uuid:aaaa::upnp:rootdevice").expect("存在").host, "192.0.2.45");

    // byebye → 移除
    let byebye = SsdpMessage::parse(b"NOTIFY * HTTP/1.1\r\nNT: upnp:rootdevice\r\nNTS: ssdp:byebye\r\nUSN: uuid:aaaa::upnp:rootdevice\r\n\r\n").expect("fixture");
    reg.observe(&byebye, 1_200).expect("byebye");
    assert_eq!(reg.len(), 0);

    // 过期（max-age）后不可用
    // 该 alive 没带 CACHE-CONTROL → 用 registry 默认 max-age（秒；与毫秒时间戳换算）
    let mut reg2 = RendererRegistry::new(1);
    reg2.observe(&alive("uuid:dddd::upnp:rootdevice", Some("http://192.0.2.46/desc.xml")), 1_000)
        .expect("条目");
    assert_eq!(reg2.live_renderers(1_500).len(), 1, "1s 内仍存活");
    assert_eq!(reg2.live_renderers(2_001).len(), 0, "超过 max-age 必须过期");
}

/// T41-02：codec/protocolInfo 不支持时必须拒绝，且**不得回退成镜像**。
#[test]
fn t41_02_unsupported_codec_never_falls_back_to_mirroring() {
    let mut reg = RendererRegistry::new(1800);
    let alive = SsdpMessage::parse(
        b"NOTIFY * HTTP/1.1\r\nNT: upnp:rootdevice\r\nNTS: ssdp:alive\r\nUSN: uuid:eeee::upnp:rootdevice\r\nLOCATION: http://192.0.2.50:49152/desc.xml\r\n\r\n",
    )
    .expect("fixture");
    let id = reg.observe(&alive, 0).expect("条目").usn.clone();

    // renderer 只声明 rtsp 与未知 MIME（来自 SSDP/描述解析）
    reg.set_protocol_info(&id, &["rtsp:*:video/mp4:*".to_string()])
        .expect("协议声明");

    // 我们用 http-get 推送 → 必须拒绝，并明确"不提供镜像替代"
    let err = reg
        .check_push_capability(&id, "http-get", "video/mp4")
        .expect_err("未声明的协议必须拒绝");
    assert!(matches!(err, DmcError::Unsupported(_)));
    assert!(
        err.to_string().contains("镜像") || err.to_string().contains("mirror"),
        "拒绝理由必须明确不提供镜像替代：{err}"
    );

    // 声明的协议 + MIME 匹配 → 允许
    reg.set_protocol_info(
        &id,
        &["http-get:*:video/mp4:*".to_string(), "http-get:*:audio/mpeg:*".to_string()],
    )
    .expect("协议声明");
    reg.check_push_capability(&id, "http-get", "video/mp4").expect("允许");
    assert_eq!(
        reg.check_push_capability(&id, "http-get", "video/hevc")
            .expect_err("未声明的 MIME")
            .code(),
        ErrorCode::UnsupportedProfile
    );
}

/// T41-03：ContentDirectory 只暴露批准目录，越权 objectID → 701。
#[test]
fn t41_03_content_directory_rejects_out_of_scope_object_ids() {
    let root = ContentRoot::new(
        "0",
        vec![("music", "Music"), ("video", "Video")],
        vec!["mp3", "mp4"],
    );
    let dms = Dms::new(vec![root]);

    // 允许：根与批准的子节点
    let browse = dms
        .browse("0", BrowseFlag::BrowseDirectChildren, 0, 10)
        .expect("根可浏览");
    assert_eq!(browse.total_matches, 2);
    assert_eq!(browse.number_returned, 2);
    let child = dms.browse("music", BrowseFlag::BrowseMetadata, 0, 1).expect("子节点");
    assert_eq!(child.total_matches, 1);

    // 越权：未知/穿越风格的 objectID
    for bad in ["../etc", "3", "music/../../secret", "urn:x", ""] {
        let err = dms.browse(bad, BrowseFlag::BrowseDirectChildren, 0, 10).expect_err(bad);
        assert_eq!(err.code(), 701, "越权 objectID 必须回 701（{bad}）");
    }

    // 分页上限
    let err = dms
        .browse("0", BrowseFlag::BrowseDirectChildren, 0, 10_000)
        .expect_err("RequestedCount 超上限");
    assert_eq!(err.code(), 402, "非法参数回 402");

    // 未知 BrowseFlag
    let err = dms
        .browse_raw("0", "BrowseEverything", 0, 10)
        .expect_err("未知 BrowseFlag");
    assert_eq!(err.code(), 402);

    // 允许的扩展名过滤（媒体 URL 推送只暴露批准的类型）
    assert!(dms.extension_allowed("mp3"));
    assert!(!dms.extension_allowed("exe"));
}

/// T41-04：Stop / 取消 / 超时都要撤销 URL lease。
#[test]
fn t41_04_stop_revokes_url_leases() {
    let mut leases = UrlLeaseStore::new(60_000); // 60s（毫秒）
    let url = "http://192.0.2.10:8000/media/clip.mp4";

    let lease = leases.grant(url, 1_000).expect("grant");
    assert!(leases.is_live(url, 1_010), "播放中必须可取");
    assert_eq!(leases.live_count(1_010), 1);

    // Stop → 立即撤销
    leases.revoke_for_url(url);
    assert!(!leases.is_live(url, 1_011), "Stop 之后不得再取（T41-04）");
    assert_eq!(leases.live_count(1_011), 0);

    // 超时 → 撤销
    let _ = leases.grant(url, 2_000).expect("grant");
    assert!(leases.is_live(url, 2_010));
    assert_eq!(leases.sweep(2_000 + 60_001), 1, "超时必须被清理");
    assert!(!leases.is_live(url, 2_000 + 60_002));

    // 取消（调用方显式）→ 撤销
    let l = leases.grant(url, 3_000).expect("grant");
    leases.revoke(l);
    assert!(!leases.is_live(url, 3_001));
    assert!(!lease.token.is_empty(), "lease 必须带不可猜测 token");
}

/// SOAP Fault 构造：ContentDirectory 错误码经 Fault 承载（F-08）。
#[test]
fn soap_fault_carries_dlna_error_codes() {
    let fault = SoapMessage::fault(701, "No such object").encode();
    let text = String::from_utf8_lossy(&fault);
    assert!(text.contains("Fault"), "{text}");
    assert!(text.contains("701"), "{text}");
    assert!(text.contains("UPnPError"), "{text}");
    assert!(text.contains("No such object"), "{text}");
}
