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
