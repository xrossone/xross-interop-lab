//! T5 验收（docs/goal-phase-3.md）：demo CLI 必须输出**真实执行**的状态，且不把 fixture 当真实设备。
//!
//! - 正向：`xinterop-demo --json` 退出码 0，schema/关键状态来自实际 crate 执行结果；
//! - 负向：未知子命令退出码 2；`discovery` 输出不得声称已实现网络发现源；
//! - 诚实性：`evidence_level=simulated`、`wire=self-authored-fixture`、blocked/manual 清单非空。

use serde_json::Value;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_xinterop-demo")
}

fn run_json(args: &[&str]) -> (i32, Value) {
    let out = Command::new(bin())
        .args(args)
        .output()
        .expect("demo 二进制必须可执行");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let parsed: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout 必须是合法 JSON：{e}\nstdout={stdout}\nstderr={}", String::from_utf8_lossy(&out.stderr))
    });
    (code, parsed)
}

/// D-01：demo 有真实发现/会话/sink/数据面状态，且如实标注非真机证据。
#[test]
fn d01_demo_reports_real_state_with_honest_labels() {
    let (code, v) = run_json(&["run", "--json"]);
    assert_eq!(code, 0, "全部场景通过时退出码必须是 0");
    assert_eq!(v["tool"], "xinterop-demo");
    assert_eq!(v["interface_version"], "interop.api/0.1");
    assert_eq!(v["evidence_level"], "simulated", "无真机证据：不得写成 device-verified");
    assert_eq!(v["wire"], "self-authored-fixture");
    assert_eq!(v["ok"], true);
}

/// D-02：发现面——只注册不合并（同名同址两条 authority 行），TTL/接口断开后候选作废；
/// 且明确不声称已实现网络发现源。
#[test]
fn d02_discovery_never_merges_identity_and_expires_addresses() {
    let (code, v) = run_json(&["discovery", "--json"]);
    assert_eq!(code, 0);
    let reg = &v["discovery"]["registry"];
    assert_eq!(reg["same_name_same_address_rows"], 2, "同名同址不同来源不得合并");
    assert_eq!(
        reg["authority_rows"].as_array().expect("rows").len(),
        2,
        "authority 视图逐来源一行"
    );
    assert_eq!(reg["expiry"]["sendable_before_ttl"], 1);
    assert_eq!(reg["expiry"]["sendable_after_ttl"], 0, "TTL 过期后候选作废");
    assert_eq!(
        reg["expiry"]["sendable_after_interface_down"], 0,
        "接口断开立即作废该接口候选"
    );
    let sources = v["discovery"]["implemented_discovery_sources"]
        .as_array()
        .expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["source"], "fake");
    assert_eq!(sources[0]["network"], false, "fake 源不产生网络行为");
    assert!(
        v["discovery"]["feature_string_status"]
            .as_str()
            .expect("状态")
            .contains("待固化"),
        "未固化的 AirPlay 位值必须诚实标注"
    );
}

/// D-03：会话面——生产 keying（FairPlay 位置留空）下认证不推进、SETUP 503 且不建流；
/// 测试 keying 下状态机可走完并回收；framing 负向用例全部拒绝。
#[test]
fn d03_session_state_machine_and_framing() {
    let (code, v) = run_json(&["session", "--json"]);
    assert_eq!(code, 0);
    let prod = &v["session"]["production_keying"];
    assert_eq!(prod["keying"], "unavailable(vendor-gated)");
    assert_eq!(prod["setup_status"], 503, "密钥不可得 → 503（能力缺席）");
    assert_eq!(prod["has_stream_after_setup"], false);
    assert_eq!(prod["allocated_video_bytes_after_setup"], 0, "认证前不得分配视频缓冲");
    assert_eq!(prod["fp_setup_error"], "vendor-gated", "FairPlay 不实现");
    assert_eq!(
        prod["advertised_features"].as_array().expect("features").len(),
        0,
        "没有 decoder 就没有可广告的能力"
    );
    let pair_step = prod["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|s| s["step"] == "POST /pair-setup-pin")
        .expect("必须有 pair 步骤");
    assert_eq!(pair_step["state_after"], "unauthenticated", "无 keying 时配对不推进状态");

    let fake = &v["session"]["test_keying"];
    assert_eq!(fake["keying"], "fake-test-only");
    assert!(fake["warning"].is_string(), "测试替身必须带警告标注");
    assert_eq!(fake["setup_status"], 200);
    assert_eq!(fake["state_after_setup"], "streaming");
    assert_eq!(fake["has_stream_after_setup"], true);
    assert_eq!(fake["allocated_video_bytes_after_setup"], 131072);
    assert_eq!(fake["has_stream_after_teardown"], false, "TEARDOWN 后不得留流");
    assert_eq!(fake["reserved_ports_after_teardown"], 0, "TEARDOWN 后端口必须回收");
    assert_eq!(fake["tracked_sessions_after_disconnect"], 0);

    for case in v["session"]["protocol_hardening"].as_array().expect("hardening") {
        assert_eq!(case["outcome"], "rejected", "case={case}");
        assert!(
            matches!(case["error_code"].as_str(), Some("invalid-frame") | Some("resource-limit")),
            "case={case}"
        );
    }
    let churn = &v["session"]["churn"];
    assert_eq!(churn["iterations"], 100);
    assert_eq!(churn["active_sessions"], 0);
    assert_eq!(churn["reserved_ports"], 0);
    assert_eq!(churn["tracked_sessions"], 0);
    assert_eq!(v["session"]["concurrency_cap"]["second_accept"], "busy");
}

/// D-04：sink 面——null 统计、file 落盘字节、显式 resample（时长不变）、
/// native 窗口明确不可用、巨大 dimensions 分配前拒绝。
#[test]
fn d04_sink_stats_and_limits() {
    let (code, v) = run_json(&["sink", "--json"]);
    assert_eq!(code, 0);
    let null = &v["sink"]["null"];
    assert_eq!(null["kind"], "null");
    assert_eq!(null["frames"], 6);
    assert!(null["bytes"].as_u64().expect("bytes") > 0);
    assert_eq!(null["format_changes"], 1);
    assert_eq!(null["codec"], "h264");

    let file = &v["sink"]["file"];
    assert_eq!(file["kind"], "file");
    assert_eq!(
        file["file_size_bytes"], file["bytes"],
        "落盘字节 = 统计字节（codec config 不进内容文件）"
    );
    assert!(std::path::Path::new(file["path"].as_str().expect("path")).exists());

    let audio = &v["sink"]["audio"];
    assert_eq!(audio["plan"]["kind"], "resample");
    assert_eq!(audio["plan"]["ratio_num"], 147);
    assert_eq!(audio["plan"]["ratio_den"], 160);
    assert_eq!(audio["frames_in"], 441);
    assert_eq!(audio["frames_out"], 480, "44.1k→48k 显式重采样");
    assert_eq!(audio["duration_ms_in"], audio["duration_ms_out"], "时长不变（不变速）");
    assert_eq!(v["sink"]["passthrough"]["kind"], "passthrough");
    assert_eq!(v["sink"]["native_window"]["error_code"], "platform-unavailable");
    assert_eq!(v["sink"]["oversized_dimensions"]["error_code"], "resource-limit");
}

/// D-05：数据面——XMD1 头 36 字节，往返一致；截断/未知 kind/未知 critical flag/超限都被拒。
#[test]
fn d05_media_frame_roundtrip_and_negatives() {
    let (code, v) = run_json(&["media", "--json"]);
    assert_eq!(code, 0);
    assert_eq!(v["media_plane"]["header_len"], 36);
    assert_eq!(v["media_plane"]["roundtrip"]["decoded_equal"], true);
    let negatives = v["media_plane"]["negatives"].as_array().expect("negatives");
    assert_eq!(negatives.len(), 4);
    let oversize = negatives
        .iter()
        .find(|n| n["case"].as_str().expect("case").contains("17MiB"))
        .expect("必须包含超限用例");
    assert_eq!(oversize["error_code"], "resource-limit", "分配前拒绝");
}

/// D-06：未知子命令退出码 2；人类可读模式包含关键结论（不是只在 JSON 里成立）。
#[test]
fn d06_usage_errors_and_human_output() {
    let out = Command::new(bin()).arg("nonsense").output().expect("可执行");
    assert_eq!(out.status.code(), Some(2), "未知子命令必须非 0");

    let out = Command::new(bin()).arg("run").output().expect("可执行");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    for needle in ["xinterop-demo", "SETUP", "503", "vendor-gated", "待固化"] {
        assert!(text.contains(needle), "人类输出缺少 {needle:?}：\n{text}");
    }
}

/// D-08：Quick Share / UKEY2 场景——真实握手、分片等价、规范 Alert 码、payload gate 关闭语义。
#[test]
fn d08_quickshare_handshake_and_gate() {
    let (code, v) = run_json(&["qshare", "--json"]);
    assert_eq!(code, 0);
    let q = &v["quickshare"];
    assert_eq!(q["evidence_level"], "simulated");
    assert_eq!(q["framing"]["length_prefix_bytes"], 4);
    assert_eq!(q["framing"]["byte_order"], "big-endian");
    assert_eq!(q["handshake"]["cipher"], "P256_SHA512(100)");
    assert_eq!(q["handshake"]["key_schedule"], "HKDF-SHA256");
    assert_eq!(q["handshake"]["established"], true);
    assert_eq!(q["handshake"]["auth_strings_match"], true);
    assert_eq!(q["handshake"]["next_secrets_match"], true);
    assert_eq!(q["handshake"]["pin_matches"], true, "两实例的 4 位确认码必须一致");
    assert_eq!(q["handshake"]["auth_string_bytes"], 32);
    assert_eq!(q["fragmentation"]["consistent"], true, "分片不应改变结果（T20-04）");

    let gate = &q["payload_gate"];
    assert_eq!(gate["before_confirmation"], "confirmation-required", "T19-02：未确认码不放行");
    assert_eq!(gate["wrong_code"], "PairingFailed");
    assert_eq!(gate["after_confirmation"], "open");

    let negatives = q["negatives"].as_array().expect("negatives");
    let by_case = |needle: &str| -> Option<String> {
        negatives
            .iter()
            .find(|n| n["case"].as_str().expect("case").contains(needle))
            .map(|n| n["outcome"].as_str().expect("outcome").to_string())
    };
    assert_eq!(by_case("commitment").as_deref(), Some("BadMessageData"));
    assert_eq!(by_case("random 31").as_deref(), Some("BadRandom"));
    assert_eq!(by_case("version=2").as_deref(), Some("BadVersion"));
    assert_eq!(by_case("next_protocol").as_deref(), Some("BadNextProtocol"));
    assert_eq!(by_case("帧长度 0").as_deref(), Some("InvalidFrame"));
    assert_eq!(by_case("帧长度超过").as_deref(), Some("ResourceLimit"));
    let t = &q["transport"];
    assert_eq!(t["secure_message"]["roundtrip"], true);
    assert_eq!(t["secure_message"]["tamper_rejected"], true);
    assert_eq!(t["payload"]["foreign_payload_id_rejected"], true, "T21-01 绑定检查");
    assert_eq!(t["published"]["status"], "accept");
    assert_eq!(t["published"]["hash_matches_content"], true, "落盘 hash 必须等于源内容 hash");
    assert_eq!(t["published"]["published_count"], 1);
    assert_eq!(q["conflict"]["adopted"], "HKDF-SHA256");
    assert!(!q["blocked"].as_array().expect("blocked").is_empty());
}

/// D-09：AirPlay 镜像路径段——格式往返、背压关键帧恢复、显式重采样、能力广告为空。
#[test]
fn d09_airplay_mirror_path() {
    let (code, v) = run_json(&["mirror", "--json"]);
    assert_eq!(code, 0);
    let m = &v["mirror"];
    assert_eq!(m["evidence_level"], "simulated");
    assert_eq!(m["video"]["format_changes"], 3, "横屏→竖屏→横屏三次配置");
    assert_eq!(m["video"]["frames_forwarded"], 279);
    assert_eq!(m["video"]["recoveries"], 1, "超预算后必须走关键帧恢复");
    assert_eq!(m["video"]["recovering_then_keyframe_recovered"], true);
    assert!(
        m["video"]["peak_queued_bytes"].as_u64().expect("peak") <= m["video"]["queue_budget_bytes"].as_u64().expect("budget"),
        "高水位不得超过预算"
    );
    assert_eq!(m["audio"]["blocks_forwarded"], 200);
    assert_eq!(m["audio"]["resampled_blocks"], 200, "44.1k→48k 必须显式重采样每一块");
    assert_eq!(m["audio"]["samples_out"], 384_000);
    assert_eq!(m["drift"]["samples"], 200);
    assert!(m["drift"]["max_abs_drift_ms"].as_i64().expect("drift") <= 3);
    assert_eq!(m["capability"]["hevc_rejected"], true);
    assert!(
        m["capability"]["advertised_features"].as_array().expect("features").is_empty(),
        "本路径无 decoder：能力广告必须为空"
    );
    assert!(!m["blocked"].as_array().expect("blocked").is_empty());
}

/// D-10：DLNA/UPnP AV 段——SSDP/SOAP 往返、抓取策略、能力检查、受限 DMS、URL lease 撤销。
#[test]
fn d10_dlna_control_path() {
    let (code, v) = run_json(&["dlna", "--json"]);
    assert_eq!(code, 0);
    let d = &v["dlna"];
    assert_eq!(d["evidence_level"], "simulated");
    assert_eq!(d["ssdp"]["search_roundtrip"], true);
    assert_eq!(d["ssdp"]["notify_roundtrip"], true);
    assert_eq!(d["ssdp"]["rejects"].as_array().expect("rejects").len(), 4, "M-SEARCH/NOTIFY 负向");
    assert_eq!(d["registry"]["renderers"], 1);
    let policy_rejects = d["registry"]["policy_rejects"].as_array().expect("policy");
    assert_eq!(policy_rejects.len(), 5);
    for case in policy_rejects {
        assert_eq!(case["outcome"], "PermissionRequired", "T41-01：策略拒绝用 permission-required");
    }
    assert_eq!(d["registry"]["push_video_mp4"], true);
    assert_eq!(d["registry"]["push_video_hevc_refused"], true, "T41-02：未声明 codec 必须拒绝");
    assert_eq!(d["registry"]["push_rtsp_refused"], true, "未声明协议必须拒绝");
    assert_eq!(d["soap"]["actions_roundtrip"], true);
    assert_eq!(d["soap"]["fault_has_701"], true);
    assert_eq!(d["soap"]["rejects"].as_array().expect("soap rejects").len(), 3);
    assert_eq!(d["dms"]["root_children"], 2);
    assert_eq!(d["dms"]["forbidden_object_code"], 701, "T41-03：越权 objectID → 701");
    assert_eq!(d["dms"]["over_count_code"], 402);
    assert_eq!(d["leases"]["live_before_stop"], true);
    assert_eq!(d["leases"]["live_after_stop"], false, "T41-04：Stop 必须撤销 URL lease");
    assert_eq!(d["xml"]["xxe_rejected"], true, "XXE 必须被拒");
    assert_eq!(d["xml"]["depth_rejected"], true);
    assert!(!d["blocked"].as_array().expect("blocked").is_empty());
}

/// D-11：WFD/Miracast 段——消息方向表、完整会话序列、协商字段、IE 边界、RTP 记账。
#[test]
fn d11_wfd_control_path() {
    let (code, v) = run_json(&["wfd", "--json"]);
    assert_eq!(code, 0);
    let w = &v["wfd"];
    assert_eq!(w["evidence_level"], "simulated");

    // 消息层：M1/M2 同形，按发送方区分。
    assert_eq!(w["messages"]["table"].as_array().expect("table").len(), 10);
    assert_eq!(w["messages"]["m1_m2_same_shape"], true);

    // 会话：走到 PLAYING 并超时结束；关键帧请求与 keep-alive 各发生一次。
    assert!(!w["session"]["session_id"].as_str().unwrap_or("").is_empty());
    assert_eq!(w["session"]["timeout"], true);
    assert_eq!(w["session"]["keepalive_sent"], true);
    assert_eq!(w["session"]["keyframe_requests"], 1);
    assert_eq!(w["session"]["player_available"], false, "本 crate 无播放器：不得声称能显示");
    assert!(
        w["session"]["advertised"].as_str().unwrap_or("").contains("wfd_video_formats: 28 00 02 10"),
        "M3 应答体必须来自本端实现清单"
    );
    let m13 = w["session"]["keyframe_request"].as_str().unwrap_or("");
    assert!(m13.contains("uri=rtsp://localhost/wfd1.0"), "M13 走控制 URI：{m13}");
    assert!(m13.contains("wfd_idr_request"), "M13 正文是裸参数名：{m13}");

    // 协商：字段解析 + 分歧以可观察形态暴露 + 空广告。
    assert_eq!(w["negotiation"]["native"]["table"], "cea");
    assert_eq!(w["negotiation"]["native"]["index"], 5);
    assert_eq!(w["negotiation"]["video"]["max_slice_num"], 2);
    assert_eq!(w["negotiation"]["audio"]["lpcm_48000_2ch"], true);
    assert_eq!(w["negotiation"]["audio"]["aac_48k_2ch"], true);
    assert_eq!(w["negotiation"]["ports_quirk"]["port1_zero_suppresses_keepalive"], true);
    assert_eq!(w["negotiation"]["ports_quirk"]["aosp_strict_would_reject_port1_nonzero"], true);
    assert_eq!(w["negotiation"]["empty_advertisement"]["video_wire"], "none");
    assert_eq!(w["negotiation"]["empty_advertisement"]["audio_wire"], "none");

    // IE 边界：子元素可往返，容器必须拒绝（F-24）。
    assert_eq!(w["ie"]["device_subelement"]["bytes"], 9);
    assert_eq!(w["ie"]["device_subelement"]["id"], 0);
    assert_eq!(w["ie"]["device_subelement"]["length_field"], 6);
    assert_eq!(w["ie"]["device_subelement"]["control_port"], 7236);
    assert_eq!(w["ie"]["device_subelement"]["roundtrip"], true);
    assert_eq!(w["ie"]["container"]["build"], "UnsupportedFeature", "完整 IE 必须拒绝构造（F-24）");
    assert_eq!(w["ie"]["container"]["parse"], "UnsupportedFeature");

    // RTP 记账：连续不请求、缺口请求、乱序不请求。
    assert_eq!(w["rtp"]["sequential_requests_none"], true);
    assert_eq!(w["rtp"]["loss_requests_keyframe"], true);
    assert_eq!(w["rtp"]["reorder_does_not"], true);
    assert_eq!(w["rtp"]["keyframe_requests"], 1);
    assert_eq!(w["rtp"]["payload_type_anomalies"], 1, "非 33 的 PT 只记账不拒绝");

    // 负向集：没有任何一条被"接受"。
    let rejects = w["rejects"].as_array().expect("rejects");
    assert!(rejects.len() >= 15, "负向覆盖不足：{}", rejects.len());
    for case in rejects {
        let outcome = case["outcome"].as_str().unwrap_or("");
        assert_ne!(outcome, "accepted(unexpected)", "负向被接受：{}", case["case"]);
        assert!(
            matches!(
                outcome,
                "InvalidFrame" | "UnsupportedFeature" | "UnsupportedMethod" | "ResourceLimit"
            ),
            "未知错误码 {outcome}"
        );
    }
    assert!(!w["blocked"].as_array().expect("blocked").is_empty());
}

/// D-12：Google Cast 段——信封/载荷键、sender 会话与 T43-01..04 四条约束。
#[test]
fn d12_cast_sender_path() {
    let (code, v) = run_json(&["cast", "--json"]);
    assert_eq!(code, 0);
    let c = &v["cast"];
    assert_eq!(c["evidence_level"], "simulated");

    // 信封：往返 + 上限 + 分块/版本拒绝。
    assert_eq!(c["envelope"]["roundtrip"], true);
    assert_eq!(c["envelope"]["binary_roundtrip"], true);
    assert_eq!(c["envelope"]["max_body_bytes"], 65536, "F-04 的 64 KiB 上限");
    assert_eq!(c["envelope"]["chunked"], "UnsupportedFeature", "F-05：分块不实现");
    assert_eq!(c["envelope"]["oversize"], "ResourceLimit");
    assert_eq!(c["envelope"]["version_whitelist"], "[0, 1, 2, 3]");

    // 载荷键：LOAD 不写 duration；SEEK 只认 PLAYBACK_START。
    assert_eq!(c["namespaces"]["media"]["load_writes_duration"], false);
    let load_payload = c["namespaces"]["media"]["load_payload"].as_str().unwrap_or("");
    assert!(load_payload.contains(r#""contentId""#), "{load_payload}");
    assert!(load_payload.contains(r#""streamType":"BUFFERED""#));
    assert!(!load_payload.contains("duration"));
    assert!(c["namespaces"]["media"]["seek_payload"]
        .as_str()
        .unwrap_or("")
        .contains("PLAYBACK_START"));
    assert_eq!(
        c["namespaces"]["media"]["namespace"],
        "urn:x-cast:com.google.cast.media"
    );

    // 会话：走到 media-session；T43-01..04 四条约束各自成立。
    assert_eq!(c["session"]["state"], "media-session");
    assert_eq!(c["session"]["gate_closed_code"], "VendorGated", "T43-01");
    assert_eq!(c["session"]["gate_closed_sent"], 0, "T43-01：认证失败不启动播放");
    assert_eq!(c["session"]["receiver_terminated_released"], true, "T43-03");
    assert_eq!(c["session"]["media_url_held_after_stop"], false, "T43-03");
    assert_eq!(c["session"]["screen_capability"], false, "T43-04：只推媒体 URL");
    assert_eq!(c["session"]["heartbeat_ping_due"], 1);
    assert_eq!(c["session"]["heartbeat_pong"], 1);
    assert_eq!(c["session"]["request_timeout"], true);

    // 发现只解析、不组播。
    assert_eq!(c["discovery"]["service_type"], "_googlecast._tcp");
    assert_eq!(c["discovery"]["port"], 8009);
    assert_eq!(c["discovery"]["multicast"], false);

    // 负向：无一被接受。
    let rejects = c["rejects"].as_array().expect("rejects");
    assert!(rejects.len() >= 15, "负向覆盖不足：{}", rejects.len());
    for case in rejects {
        let outcome = case["outcome"].as_str().unwrap_or("");
        assert_ne!(outcome, "accepted(unexpected)", "负向被接受：{}", case["case"]);
        assert!(
            matches!(
                outcome,
                "InvalidFrame" | "UnsupportedFeature" | "UnsupportedMethod" | "ResourceLimit"
                    | "VendorGated"
            ),
            "未知错误码 {outcome}"
        );
    }
    assert!(!c["blocked"].as_array().expect("blocked").is_empty());

    // 人类输出同样给出这四条结论。
    let out = Command::new(bin()).arg("cast").output().expect("可执行");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Google Cast（T43"), "{text}");
    assert!(text.contains("screen_capability=false"), "{text}");
    assert!(text.contains("VendorGated"), "{text}");
    assert!(text.contains("T43-01") && text.contains("T43-03") && text.contains("T43-04"), "{text}");
}

/// D-07：blocked 与待用户手动清单必须随输出给出（本阶段不允许"看起来全绿"）。
#[test]
fn d07_blocked_and_manual_lists_are_reported() {
    let (_, v) = run_json(&["run", "--json"]);
    let blocked = v["blocked"].as_array().expect("blocked");
    let manual = v["manual_tests"].as_array().expect("manual");
    assert!(!blocked.is_empty(), "blocked 清单不得为空");
    assert!(!manual.is_empty(), "待用户手动清单不得为空");
    assert!(
        manual.iter().any(|m| m.as_str().expect("str").contains("真机")),
        "必须列出真机测试项"
    );
    assert!(
        manual
            .iter()
            .any(|m| m.as_str().expect("str").contains("Quick Share")),
        "必须列出 Quick Share 真机/抓包测试项"
    );
}

/// D-13：Quick Share 控制帧（T21+）——keep-alive 节奏/判死与 paired-key 交换必须真跑并如实报告。
#[test]
fn d13_quickshare_control_frames() {
    let (code, v) = run_json(&["qshare", "--json"]);
    assert_eq!(code, 0);
    let c = &v["quickshare"]["control"];

    // keep-alive：10 s 来源取值 + 30 s 本仓策略，两端走真实字节。
    let k = &c["keepalive"];
    assert_eq!(k["interval_ms"], 10_000);
    assert_eq!(k["timeout_ms"], 30_000);
    assert!(
        k["interval_source"]
            .as_str()
            .unwrap_or("")
            .contains("来源取值"),
        "{k:?}"
    );
    assert!(
        k["timeout_source"]
            .as_str()
            .unwrap_or("")
            .contains("本仓策略"),
        "{k:?}"
    );
    assert_eq!(k["cadence_ok"], true, "节奏必须恰好 10 s");
    assert_eq!(k["wire_roundtrip_ok"], true, "帧必须经真实字节往返");
    let sent = k["sent"].as_u64().expect("sent");
    assert_eq!(k["acks_returned"], sent, "每一帧都应拿到 ack");
    assert!(sent >= 7, "65 s 内至少 7 帧：{sent}");

    // 对端静默：判定明确（有毫秒值）且期间我们确实还在发心跳。
    let died = k["silent_peer_detected_ms"].as_u64().expect("判死时刻");
    assert!(
        died > 30_000 && died <= 31_000,
        "判死应在阈值之后立刻发生：{died}"
    );
    assert!(k["silent_peer_heartbeats_sent"].as_u64().unwrap_or(0) >= 3);

    // paired-key：层号、BYTES 载体、材料往返。
    let p = &c["paired_key"];
    assert_eq!(p["inner_type"], "PAIRED_KEY_ENCRYPTION(3)");
    assert_eq!(p["inner_type_ok"], true);
    assert_eq!(p["material_roundtrip"], true);
    assert_eq!(
        p["our_result_status"], "Some(Unable)",
        "参考实现回 UNABLE（F-33）"
    );

    // 核心诚实性：默认策略**不因 SUCCESS 免确认码**；只有显式开关才跳过。
    assert_eq!(p["peer_unable_decision"], "RequireConfirmation");
    assert_eq!(p["peer_success_decision_default"], "RequireConfirmation");
    assert_eq!(p["peer_success_decision_opted_in"], "SkipConfirmation");
    assert_eq!(p["skips_confirmation_by_default"], false);

    // 控制帧负向：层号混用与 FILE 载荷冒充都必须被拒。
    let rejects = c["rejects"].as_array().expect("rejects");
    assert!(rejects.len() >= 2);
    for case in rejects {
        assert_eq!(case["outcome"], "InvalidFrame", "负向未拒绝：{case}");
    }

    // 人类输出同样给出这些结论。
    let out = Command::new(bin()).arg("qshare").output().expect("可执行");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("T19/T20/T21+"), "{text}");
    assert!(
        text.contains("10000 ms") && text.contains("30000 ms"),
        "{text}"
    );
    assert!(text.contains("RequireConfirmation"), "{text}");
    assert!(text.contains("默认不免，F-32"), "{text}");
}

/// D-14：AirPlay 音频 profile 与配对登记（T34）——解析必须按字段表，登记不得声称认证。
#[test]
fn d14_airplay_audio_profiles_and_pair_store() {
    let (code, v) = run_json(&["airplay", "--json"]);
    assert_eq!(code, 0);
    let a = &v["airplay"];
    assert_eq!(a["evidence_level"], "simulated");

    // AP1：ct 四值表 + spf 为来源取值 + PCM 没有 spf + 只有 PCM 能在本仓解码。
    let ap1 = &a["ap1_profiles"];
    assert_eq!(ap1["sample_rate_hz"], 44100);
    let rows = ap1["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 4, "ct 四值（1/2/4/8）");
    let by_ct = |ct: u64| {
        rows.iter()
            .find(|r| r["ct"].as_u64() == Some(ct))
            .unwrap_or_else(|| panic!("缺 ct={ct}"))
    };
    assert_eq!(by_ct(2)["codec"], "ALAC");
    assert_eq!(by_ct(2)["spf_source"], 352);
    assert_eq!(by_ct(4)["spf_source"], 1024);
    assert_eq!(by_ct(8)["spf_source"], 480);
    assert_eq!(by_ct(1)["spf_source"], Value::Null, "PCM 无 spf：不得编造");
    assert_eq!(ap1["pcm_has_no_spf"], true);
    for (ct, decodable) in [(1u64, true), (2, false), (4, false), (8, false)] {
        assert_eq!(by_ct(ct)["decodable_here"], decodable, "ct={ct}");
        assert_eq!(by_ct(ct)["spf_matches_source"], ct != 1, "ct={ct} 的 spf 一致性");
    }

    // AP2：两套编号（打包值 vs SSRC 魔数）互不回退；SSRC 从 RTP 头 packet[8:12] 取。
    let ap2 = &a["ap2_profiles"];
    assert_eq!(ap2["packed"].as_array().expect("packed").len(), 4);
    assert_eq!(ap2["ssrc"].as_array().expect("ssrc").len(), 6);
    assert_eq!(ap2["two_number_spaces"], true);
    assert_eq!(ap2["cross_space_packed_from_ssrc"], "UnsupportedProfile");
    assert_eq!(ap2["cross_space_ssrc_from_packed_is_none"], true);
    assert_eq!(ap2["ssrc_from_rtp_header"], 0x0000_FACEu32);
    assert_eq!(ap2["change_detection"], true, "0 不触发、同值不重复、换值才触发");
    assert_eq!(ap2["aac_eld_no_data"], true);

    // 配对登记：结论只有两种，且**不声称认证**；bit 9 只在没有配对时置位。
    let store = &a["pair_store"];
    assert_eq!(store["verdict_known"], "KnownButUnverified");
    assert_eq!(store["verdict_unknown"], "Unknown");
    assert_eq!(store["claims_authentication"], false, "登记 ≠ 认证（T34 硬边界）");
    assert_eq!(store["one_time_pairing_required_bit"], 9);
    assert_eq!(store["status_flags_before_any_pairing"], 512, "bit 9 = 1<<9");
    assert_eq!(store["status_flags_after_pairing"], 0);
    assert_eq!(store["json_roundtrip"], true);
    assert_eq!(store["removed"], true);
    assert_eq!(store["touched_updated_last_seen"], true);
    assert_eq!(store["identity_seed_provided_by_host"], true, "身份种子由宿主给");

    // 端点策略：四个 shape-check-only + /fp-setup vendor-gated；握手未实现。
    let endpoints = a["endpoints"]["rows"].as_array().expect("endpoints");
    assert_eq!(endpoints.len(), 5);
    let fp = endpoints
        .iter()
        .find(|r| r["policy"] == "vendor-gated")
        .expect("必须有 vendor-gated 端点");
    assert_eq!(fp["path"], "/fp-setup");
    assert_eq!(a["endpoints"]["handshake_implemented"], false);
    assert_eq!(a["endpoints"]["fp_setup"], "VendorGated");

    // 负向：逐条给错误码，无一被接受。
    let rejects = a["rejects"].as_array().expect("rejects");
    assert!(rejects.len() >= 10, "负向覆盖不足：{}", rejects.len());
    for case in rejects {
        let outcome = case["outcome"].as_str().unwrap_or("");
        assert_ne!(outcome, "accepted(unexpected)", "负向被接受：{}", case["case"]);
        assert!(
            matches!(
                outcome,
                "InvalidFrame" | "UnsupportedProfile" | "UnsupportedFeature" | "ResourceLimit"
                    | "VendorGated"
            ),
            "未知错误码 {outcome}"
        );
    }
    assert!(!a["blocked"].as_array().expect("blocked").is_empty());

    // 人类输出复述同一批结论。
    let out = Command::new(bin()).arg("airplay").output().expect("可执行");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("AirPlay 音频 profile 与配对登记（T34）"), "{text}");
    assert!(text.contains("KnownButUnverified"), "{text}");
    assert!(text.contains("vendor-gated"), "{text}");
    assert!(text.contains("spf 无（来源未给，不编造）"), "{text}");
}

/// D-15：Cast receiver 可达边界（T45）——产品路径恒拒绝、test-root 闭环、TXT 键与端口取值。
#[test]
fn d15_cast_receiver_boundary() {
    let (code, v) = run_json(&["cast", "--json"]);
    assert_eq!(code, 0);
    let rv = &v["cast"]["receiver"];

    // 产品路径：即便对端信任我们也拒绝，且不留半开会话。
    assert_eq!(rv["vendor_path"]["gate"], "vendor-required");
    assert_eq!(rv["vendor_path"]["even_if_peer_trusts_us"], "VendorGated");
    assert_eq!(rv["vendor_path"]["state_after_refusal"], "Idle", "拒绝后不得留下会话");
    assert_eq!(rv["vendor_path"]["senders_admitted"], 0);

    // test-root 闭环：只在对方显式信任同一测试根时成立，且**不是** stock 兼容。
    assert_eq!(rv["test_root_path"]["gate"], "test-root");
    assert_eq!(
        rv["test_root_path"]["refused_when_peer_does_not_trust_test_root"],
        "VendorGated"
    );
    assert_eq!(rv["test_root_path"]["connected"], true);
    assert_eq!(rv["test_root_path"]["connected_only_with_protocol_version"], true);
    assert_eq!(rv["test_root_path"]["launched"], true);
    assert_eq!(rv["test_root_path"]["refused_unconfigured_app"], "DestinationUnavailable");
    assert_eq!(rv["test_root_path"]["pong"], true);
    assert_eq!(rv["test_root_path"]["closed_released_app"], true, "关闭后必须释放 app");
    assert_eq!(rv["test_root_path"]["stock_compatible"], false, "T45-01：不得写成 stock 兼容");

    // TXT：6 个来源登记的键、往返、未登记键拒绝；两个端口取值分别引用。
    let txt = &rv["txt"];
    assert_eq!(txt["service_type"], "_googlecast._tcp");
    assert_eq!(txt["keys"].as_array().expect("keys").len(), 6);
    assert_eq!(txt["roundtrip"], true);
    assert_eq!(txt["unknown_key_refused"], "InvalidFrame");
    assert_eq!(txt["port_real_device"], 8009);
    assert_eq!(txt["port_reference_receiver"], 8010);

    // blocked 清单必须写明 stock 路径与 CAF 的边界。
    let blocked = v["cast"]["blocked"].as_array().expect("blocked");
    let joined = blocked
        .iter()
        .filter_map(|b| b.as_str())
        .collect::<Vec<_>>()
        .join("|");
    assert!(joined.contains("stock sender"), "{joined}");
    assert!(joined.contains("CAF"), "{joined}");

    // 人类输出复述同一批结论（并保留 T43 的立场）。
    let out = Command::new(bin()).arg("cast").output().expect("可执行");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("T45 receiver"), "{text}");
    assert!(text.contains("stock 兼容=false"), "{text}");
    assert!(text.contains("screen_capability=false"), "{text}");
}
