//! AirPlay 音频 profile 与配对登记（T34）：把 AP1/AP2 的 profile 解析与配对登记层跑成状态输出。
//!
//! 诚实前提：**没有 decoder、没有配对握手、没有厂商材料**。这里证明的是"字段解析与登记的
//! 行为与字段表一致"，不是"能解码"或"能配对"——`/fp-setup` 恒 vendor-gated，
//! 配对握手的端点一律明确 unsupported-feature。

use crate::report::AirplayReport;
use proto_airplay::audio_profile::{
    is_aac_eld_no_data, parse_packed_audio_format, AudioSsrc, CompressionType, RealtimeAudioRequest,
    SsrcTracker, StreamResponse, StreamRouting, StreamType, AAC_ELD_NO_DATA_MARKER,
    AP1_AUDIO_SAMPLE_RATE_HZ,
};
use proto_airplay::pairstore::{
    EndpointPolicy, PairStore, PairingEndpoint, PairingVerdict, MAX_PAIRED_DEVICES,
};
use serde_json::json;

fn code_of<T>(result: Result<T, interop_contract::error::Error>) -> String {
    match result {
        Ok(_) => "accepted(unexpected)".to_string(),
        Err(e) => format!("{:?}", e.code),
    }
}

pub fn airplay_scenario() -> (AirplayReport, bool) {
    let mut ok = true;

    // ---- AP1 实时音频：ct 四值表（spf 为来源取值，PCM 没有 spf）----
    let mut ap1_rows: Vec<serde_json::Value> = Vec::new();
    for (wire_ct, name) in [(1u64, "PCM"), (2, "ALAC"), (4, "AAC-LC"), (8, "AAC-ELD")] {
        let keys = [
            ("controlPort", 5002u64),
            ("ct", wire_ct),
            ("spf", 0),
            ("audioFormat", 0x0004_0000),
        ];
        // spf 用来源默认值（PCM 没有默认值 → 用 0 表示"对端没给"）。
        let source_spf = CompressionType::from_wire(wire_ct)
            .ok()
            .and_then(|c| c.samples_per_frame())
            .map(u64::from)
            .unwrap_or(0);
        let keys = [
            keys[0],
            keys[1],
            ("spf", source_spf),
            keys[3],
        ];
        let req = match RealtimeAudioRequest::validate(&keys) {
            Ok(r) => r,
            Err(_) => {
                ok = false;
                continue;
            }
        };
        if req.compression.decodable_here() != (wire_ct == 1) {
            ok = false;
        }
        ap1_rows.push(json!({
            "ct": wire_ct,
            "codec": name,
            "spf_source": req.compression.samples_per_frame(),
            "spf_declared": req.samples_per_frame,
            "spf_matches_source": req.spf_matches_source(),
            "decodable_here": req.decodable_here(),
        }));
    }

    // ---- AP2：打包 audioFormat 与 RTP SSRC 是两套编号 ----
    let mut packed_rows: Vec<serde_json::Value> = Vec::new();
    for packed in [0x0004_0000u64, 0x0008_0000, 0x0010_0000, 0x0020_0000] {
        match parse_packed_audio_format(packed) {
            Ok(p) => packed_rows.push(json!({
                "packed": format!("0x{packed:08X}"),
                "codec": format!("{:?}", p.codec),
                "rate": p.sample_rate_hz,
                "bits": p.bit_depth,
                "channels": p.channels,
            })),
            Err(_) => ok = false,
        }
    }
    let mut ssrc_rows: Vec<serde_json::Value> = Vec::new();
    for (wire, label) in [
        (0x0000_FACEu32, "ALAC 44100/16/2"),
        (0x1500_0000, "ALAC 48000/24/2"),
        (0x1600_0000, "AAC 44100 f24/2"),
        (0x1700_0000, "AAC 48000 f24/2"),
        (0x2700_0000, "AAC 48000 f24 5.1"),
        (0x2800_0000, "AAC 48000 f24 7.1"),
    ] {
        let ssrc = AudioSsrc::from_u32(wire);
        if ssrc == AudioSsrc::None {
            ok = false;
        }
        ssrc_rows.push(json!({
            "magic": format!("0x{wire:08X}"),
            "label": label,
            "profile": ssrc.profile().map(|p| format!("{:?} {}/{}", p.codec, p.sample_rate_hz, p.bit_depth)),
        }));
    }
    // 交叉检查：两套编号不得互相回退。
    let cross_packed = code_of(parse_packed_audio_format(u64::from(0x0000_FACEu32)));
    let cross_ssrc_is_none = AudioSsrc::from_u32(0x0004_0000) == AudioSsrc::None;
    if cross_packed != "UnsupportedProfile" || !cross_ssrc_is_none {
        ok = false;
    }
    // SSRC 从解密后的 RTP 头 packet[8:12] 取；格式变化只在非 0 且不同时触发。
    let mut header = vec![0u8; 12];
    header[8..12].copy_from_slice(&0x0000_FACEu32.to_be_bytes());
    let from_header = AudioSsrc::from_rtp_header(&header);
    let mut tracker = SsrcTracker::new();
    let first_change = tracker.observe(AudioSsrc::None).is_none()
        && tracker.observe(AudioSsrc::Alac44100S16Stereo).is_some()
        && tracker.observe(AudioSsrc::Alac44100S16Stereo).is_none()
        && tracker.observe(AudioSsrc::Aac48000F24Stereo).is_some();
    if !first_change {
        ok = false;
    }
    let no_data = is_aac_eld_no_data(&AAC_ELD_NO_DATA_MARKER);

    // ---- 流类型：可路由与不实现（不实现的必须给原因）----
    let mut stream_rows: Vec<serde_json::Value> = Vec::new();
    for wire in [96u64, 103, 110, 120, 130] {
        let stream = match StreamType::from_wire(wire) {
            Ok(s) => s,
            Err(_) => {
                ok = false;
                continue;
            }
        };
        let routing = match stream.routing() {
            StreamRouting::Routable => "routable".to_string(),
            StreamRouting::NotImplemented(reason) => format!("not-implemented: {reason}"),
        };
        stream_rows.push(json!({
            "type": wire,
            "routing": routing,
            "response_keys": stream.response_keys(),
        }));
    }

    // ---- 配对登记层：登记/查询/遗忘 + 容量 + JSON 往返 ----
    let mut store = PairStore::new();
    let flags_before = store.status_flags();
    let mut events: Vec<String> = Vec::new();
    let register = |store: &mut PairStore, id: &str, key: u8, name: Option<&str>, now: u64| {
        store.put(id, [key; 32], name.map(str::to_string), now)
    };
    if register(&mut store, "demo-ipad", 0xA1, Some("客厅 iPad"), 1_000).is_err() {
        ok = false;
    }
    events.push(format!(
        "register demo-ipad → {}",
        if store.get("demo-ipad").is_some() { "stored" } else { "missing" }
    ));
    if register(&mut store, "demo-mac", 0xB2, None, 2_000).is_err() {
        ok = false;
    }
    let flags_after = store.status_flags();
    let verdict_known = store.verdict("demo-ipad");
    let verdict_unknown = store.verdict("never-seen");
    events.push(format!("verdict(demo-ipad) = {verdict_known:?}"));
    events.push(format!("verdict(never-seen) = {verdict_unknown:?}"));
    let touched = store.touch("demo-ipad", 9_000);
    let last_seen = store.get("demo-ipad").map(|d| d.last_seen_ms).unwrap_or(0);
    // 身份种子由宿主给（本层不生成）。
    store.set_identity_seed([0x55; 32]);
    let json_ok = match store.to_json().and_then(|s| PairStore::from_json(&s)) {
        Ok(restored) => {
            restored.len() == store.len() && restored.get("demo-mac").is_some()
        }
        Err(_) => false,
    };
    if !json_ok {
        ok = false;
    }
    // 容量上限：满了明确拒绝（不静默淘汰）。
    let mut full = PairStore::with_capacity(1);
    if register(&mut full, "first", 1, None, 1).is_err() {
        ok = false;
    }
    let overflow = code_of(register(&mut full, "second", 2, None, 2));
    let registered_before_removal = store.len();
    let removed = store.remove("demo-mac");

    // ---- 配对端点：五个端点的策略（/fp-setup 恒 vendor-gated）----
    let mut endpoint_rows: Vec<serde_json::Value> = Vec::new();
    for endpoint in PairingEndpoint::all() {
        let policy = match endpoint.policy() {
            EndpointPolicy::VendorGated { .. } => "vendor-gated".to_string(),
            EndpointPolicy::ShapeCheckOnly { .. } => "shape-check-only".to_string(),
        };
        endpoint_rows.push(json!({
            "path": endpoint.path(),
            "policy": policy,
        }));
    }
    let fp_setup = code_of(PairingEndpoint::FpSetup.check_request(&["data"]));
    let setup_pin_unknown_key = code_of(PairingEndpoint::PairSetupPin.check_request(&["srp_a"]));
    let setup_pin_known_keys = code_of(PairingEndpoint::PairSetupPin.check_request(&["method", "user"]));
    let unknown_path = PairingEndpoint::from_path("/pair-add").is_none();
    if fp_setup != "VendorGated"
        || setup_pin_unknown_key != "InvalidFrame"
        || setup_pin_known_keys != "UnsupportedFeature"
        || !unknown_path
    {
        ok = false;
    }

    // ---- 负向：不接受的路径逐条给错误码 ----
    let missing_control_port = code_of(RealtimeAudioRequest::validate(&[
        ("ct", 2),
        ("spf", 352),
        ("audioFormat", 0),
    ]));
    let unknown_ct = code_of(RealtimeAudioRequest::validate(&[
        ("controlPort", 1),
        ("ct", 5),
        ("spf", 352),
        ("audioFormat", 0),
    ]));
    let unknown_packed = code_of(parse_packed_audio_format(0x000C_0000));
    let unknown_stream_type = code_of(StreamType::from_wire(99));
    let buffered_response = code_of(StreamResponse::build(StreamType::BufferedAudio, 1, Some(2)));
    let audio_response_without_control = code_of(StreamResponse::build(StreamType::RealtimeAudio, 1, None));

    let rejects = vec![
        json!({"case": "AP1 缺 controlPort", "outcome": missing_control_port}),
        json!({"case": "未知 ct=5", "outcome": unknown_ct}),
        json!({"case": "未知打包 audioFormat 0x000C0000", "outcome": unknown_packed}),
        json!({"case": "未登记流类型 99", "outcome": unknown_stream_type}),
        json!({"case": "为 103 构造响应", "outcome": buffered_response}),
        json!({"case": "音频响应缺 controlPort", "outcome": audio_response_without_control}),
        json!({"case": "/fp-setup 请求", "outcome": fp_setup.clone()}),
        json!({"case": "/pair-setup-pin 未登记键", "outcome": setup_pin_unknown_key}),
        json!({"case": "/pair-setup-pin 登记键（形状合法）", "outcome": setup_pin_known_keys}),
        json!({"case": "登记表容量溢出", "outcome": overflow}),
    ];
    for case in &rejects {
        if case["outcome"] == "accepted(unexpected)" {
            ok = false;
        }
    }
    if flags_before != 1 << 9 || flags_after != 0 {
        ok = false;
    }
    if verdict_known != PairingVerdict::KnownButUnverified
        || verdict_unknown != PairingVerdict::Unknown
        || !touched
        || !removed
    {
        ok = false;
    }

    let report = AirplayReport {
        evidence_level: "simulated",
        wire: "self-authored-fixture",
        ap1_profiles: json!({
            "sample_rate_hz": AP1_AUDIO_SAMPLE_RATE_HZ,
            "sample_rate_source": "来源取值（UxPlay lib/raop_handlers.h:27；所有已支持格式都是这个率）",
            "rows": ap1_rows,
            "pcm_has_no_spf": CompressionType::LinearPcm.samples_per_frame().is_none(),
        }),
        ap2_profiles: json!({
            "packed": packed_rows,
            "ssrc": ssrc_rows,
            "two_number_spaces": true,
            "cross_space_packed_from_ssrc": cross_packed,
            "cross_space_ssrc_from_packed_is_none": cross_ssrc_is_none,
            "ssrc_from_rtp_header": from_header.map(|s| s.as_u32()),
            "change_detection": first_change,
            "aac_eld_no_data": no_data,
        }),
        pair_store: json!({
            "registered": registered_before_removal,
            "registered_after_removal": store.len(),
            "status_flags_before_any_pairing": flags_before,
            "status_flags_after_pairing": flags_after,
            "one_time_pairing_required_bit": 9,
            "verdict_known": format!("{verdict_known:?}"),
            "verdict_unknown": format!("{verdict_unknown:?}"),
            "claims_authentication": false,
            "touched_updated_last_seen": touched,
            "last_seen_after_touch": last_seen,
            "identity_seed_provided_by_host": store.identity_seed().is_some(),
            "json_roundtrip": json_ok,
            "capacity_limit": MAX_PAIRED_DEVICES,
            "capacity_policy": "满了明确拒绝（resource-limit），不静默淘汰（本仓策略）",
            "removed": removed,
            "events": events,
        }),
        endpoints: json!({
            "rows": endpoint_rows,
            "fp_setup": fp_setup,
            "unknown_path_is_none": unknown_path,
            "handshake_implemented": false,
        }),
        rejects,
        blocked: vec![
            "配对握手（/pair-setup-pin 的 SRP-SHA1 三步、/pair-verify 的 X25519+Ed25519 签名）：本仓不实现 → 形状校验后一律 unsupported-feature".to_string(),
            "/fp-setup（FairPlay/设备认证）：永久 vendor-gated（P-FAIRPLAY）：不实现、不解析、不绕过".to_string(),
            "AP2 buffered 音频（type 103：ChaCha20-Poly1305 + AAC 解码）：本仓不做 → 该流类型明确不实现".to_string(),
            "ALAC/AAC 解码：本仓没有 decoder → 只做 profile 解析，能力广告仍为空（能路由 ≠ 能显示）".to_string(),
            "配对登记 ≠ 认证：本仓没有签名校验路径，已知设备只能得到 known-but-unverified".to_string(),
        ],
    };
    (report, ok)
}
