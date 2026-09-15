//! `interop-demo`：把已实现的协议核心跑成**可复核的真实状态输出**（docs/goal-phase-3.md T5）。
//!
//! 立场：
//! - 输出里的每个数字都来自真实 crate 执行（发现注册表/RTSP 状态机/资源记账/sink 统计/XMD1 编解码）；
//! - 但 wire 全部是**自制 fixture**，不连接任何真实设备 ⇒ 证据等级恒为 `simulated`，不冒充真机；
//! - blocked 与「待用户手动测试」清单随输出一并给出，避免"看起来全绿"；
//! - Tauri 壳留待需要人看界面时再加：本 crate 不做 GUI、不开窗口、不联网。
//!
//! 子命令（默认 `run`）：`run` | `discovery` | `session` | `sink` | `media`；`--json` 输出结构化报告。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod dlna;
pub mod mirror;
pub mod quickshare;
pub mod report;
pub mod scenarios;

use report::*;

/// blocked 清单：本阶段**明确不做或做不到**的部分（障碍 + 重评条件）。
pub const BLOCKED: &[&str] = &[
    "FairPlay/设备认证材料永久 vendor-gated：没有 keying 的 SETUP 一律 503，不建流不分配缓冲",
    "S1：外部引擎（UxPlay 等）与解码链路未获批 → 音视频接收实现缺席，能力广告为空",
    "真实 mDNS/SSDP 网络发现源未实现（只有 fake 源）：真机发现矩阵需 S2 设备与用户在场",
    "Quick Share 字节格式未固化（P-F02-1 抓包、P-F02-3 可见性矩阵未关闭）→ 不写 wire 字段",
    "native window sink 未实现：无 GUI 自动化，且不以屏幕录制假装 raw output",
    "Quick Share 发现与 QR 路径未实现（P-F02-1/3 未关闭）：不写未固化的发现字节",
    "AirPlay 镜像无 decoder/无窗口：能路由到 sink，但不能显示（能力广告保持为空）",
    "DLNA 发现/抓取未实现（只有编解码与策略）：SSDP 组播收发与描述 HTTP 抓取待网络策略批准",
];

/// 待用户手动测试清单（真机/交互/GUI/外部引擎）。
pub const MANUAL_TESTS: &[&str] = &[
    "真机投屏矩阵（iPhone/iPad → 本机：发现/连接/视频/音频/旋转/重连/10-30-60 分钟）——需 S1/S2",
    "Quick Share 抓包（P-F02-1）与两台 Android 可见性矩阵（P-F02-3）——需 S3",
    "Tauri demo 壳 GUI 交互验证（窗口/渲染/点击）——headless CLI 覆盖不到",
    "native 视频窗口 sink：只能在桌面会话内验证（不以屏幕录制假装 raw output）",
    "本机广播的 mDNS/TXT 字节与真实接收端对照（AirPlay features 位值固化需要真机抓包）",
    "与 stock Android 完成一次 Quick Share 握手并比对 4 位确认码（裁决 F-12/F-15 两处规范/实现冲突）",
    "AirPlay 真机镜像（iPhone/iPad → 本机）与 30 分钟墙钟长跑（drift/丢帧/RSS 趋势）——需 S1/S2",
    "DLNA 真机矩阵：库存 TV 的 protocolInfo 与实际 SetAVTransportURI→Play 时序（P-M07-1）",
];

/// 跑全部场景并汇总（JSON 与人类输出共用同一份数据）。
pub fn compute_report() -> DemoReport {
    let (discovery, d_ok) = scenarios::discovery_scenario();
    let (session, s_ok) = scenarios::session_scenario();
    let (sink, k_ok) = scenarios::sink_scenario();
    let (media_plane, m_ok) = scenarios::media_scenario();
    let (qshare, q_ok) = quickshare::quickshare_scenario();
    let (mirror, r_ok) = mirror::mirror_scenario();
    let (dlna, l_ok) = dlna::dlna_scenario();
    let generated_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    DemoReport {
        tool: TOOL,
        interface_version: INTERFACE_VERSION,
        evidence_level: EVIDENCE_LEVEL,
        wire: WIRE,
        generated_unix_ms,
        ok: d_ok && s_ok && k_ok && m_ok && q_ok && r_ok && l_ok,
        discovery,
        session,
        sink,
        media_plane,
        quickshare: qshare,
        mirror,
        dlna,
        blocked: BLOCKED.iter().map(|s| s.to_string()).collect(),
        manual_tests: MANUAL_TESTS.iter().map(|s| s.to_string()).collect(),
    }
}

/// 人类可读渲染。`section` 为 `None` 时输出全部；否则只输出该段（顶部仍打印诚实标注）。
pub fn render_human(r: &DemoReport, section: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} — {}（evidence_level={}, wire={}）\n",
        r.tool, r.interface_version, r.evidence_level, r.wire
    ));
    out.push_str("注意：输出全部来自本机真实 crate 执行，但 wire 是自制 fixture，未连接任何真实设备。\n");

    let want = |name: &str| section.is_none() || section == Some(name);

    if want("discovery") {
        let d = &r.discovery;
        out.push_str("\n## 发现面\n");
        out.push_str(&format!(
            "Bonjour 服务类型 : {}\n",
            d.mdns_service_types.join(", ")
        ));
        out.push_str(&format!(
            "TXT 键值          : {:?}（RDATA {} 字节）\n",
            d.txt_pairs,
            d.txt_rdata_hex.len() / 2
        ));
        out.push_str(&format!("features 位串     : {}\n", d.feature_string_status));
        out.push_str(&format!(
            "已实现发现源      : {}（不产生网络行为）\n",
            d.implemented_discovery_sources
                .iter()
                .map(|s| format!("{} (network={})", s.source, s.network))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let reg = &d.registry;
        out.push_str(&format!(
            "注册表            : {} 行 authority（同名同址 {} 行，TTL {}s）\n",
            reg.authority_rows.len(),
            reg.same_name_same_address_rows,
            reg.ttl_s
        ));
        for row in &reg.authority_rows {
            out.push_str(&format!(
                "  - {} source={} name={:?} claim={:?}\n",
                row.subject, row.source, row.display_name, row.identity_claim
            ));
            for a in &row.addresses {
                out.push_str(&format!(
                    "      地址 {host}:{port} if={iface} secure={secure} expires={expires}\n",
                    host = a.host,
                    port = a.port,
                    iface = a.interface,
                    secure = a.secure,
                    expires = a.expires_at_ms
                ));
            }
            for c in &row.capabilities {
                out.push_str(&format!(
                    "      能力 {} role={} available={} state={}\n",
                    c.profile_id, c.role, c.available, c.state
                ));
            }
        }
        out.push_str(&format!(
            "地址候选          : TTL 前 {} → TTL 后 {} → 接口断开后 {}\n",
            reg.expiry.sendable_before_ttl,
            reg.expiry.sendable_after_ttl,
            reg.expiry.sendable_after_interface_down
        ));
        for g in &reg.alias_groups {
            out.push_str(&format!(
                "展示聚合（不参与授权）: {} = {}\n",
                g.alias,
                g.members.join(" + ")
            ));
        }
        for n in &d.notes {
            out.push_str(&format!("  · {n}\n"));
        }
    }

    if want("session") {
        let s = &r.session;
        out.push_str("\n## 会话面\n");
        out.push_str(&format!(
            "framing 上限      : header {} B / body {} B / headers {}\n",
            s.framing.max_header_bytes, s.framing.max_body_bytes, s.framing.max_headers
        ));
        for case in &s.protocol_hardening {
            out.push_str(&format!(
                "  - {:<28} {} {}\n",
                case.case,
                case.outcome,
                case.error_code.clone().unwrap_or_default()
            ));
        }
        for run in [&s.production_keying, &s.test_keying] {
            out.push_str(&format!("\n### keying = {}\n", run.keying));
            if let Some(w) = run.warning {
                out.push_str(&format!("警告：{w}\n"));
            }
            out.push_str(&format!(
                "广告能力          : {:?}\n",
                run.advertised_features
            ));
            for step in &run.steps {
                out.push_str(&format!(
                    "  - {:<22} → {} state={} stream={} buf={} B\n",
                    step.step,
                    match (step.status, &step.error_code) {
                        (Some(code), _) => format!("RTSP {code}"),
                        (None, Some(e)) => format!("error {e}"),
                        (None, None) => "n/a".to_string(),
                    },
                    step.state_after,
                    step.has_stream,
                    step.allocated_video_bytes
                ));
            }
            out.push_str(&format!(
                "SETUP 结果        : status={} state={} stream={} allocated={} B\n",
                run.setup_status,
                run.state_after_setup,
                run.has_stream_after_setup,
                run.allocated_video_bytes_after_setup
            ));
            out.push_str(&format!(
                "TEARDOWN 后       : stream={} reserved_ports={}\n",
                run.has_stream_after_teardown, run.reserved_ports_after_teardown
            ));
            if let Some(e) = &run.fp_setup_error {
                out.push_str(&format!("FairPlay（/fp-setup）: {e}\n"));
            }
        }
        out.push_str(&format!(
            "\n快速连接/断开 {} 次: active={} reserved_ports={} tracked={}\n",
            s.churn.iterations, s.churn.active_sessions, s.churn.reserved_ports, s.churn.tracked_sessions
        ));
        out.push_str(&format!(
            "并发上限 {}        : 第二次 accept={}，断开后={}\n",
            s.concurrency_cap.max_sessions, s.concurrency_cap.second_accept, s.concurrency_cap.after_disconnect
        ));
    }

    if want("sink") {
        let k = &r.sink;
        out.push_str("\n## sink 面\n");
        out.push_str(&format!("喂入帧数          : {}\n", k.frames_fed));
        for stats in [&k.null, &k.file] {
            out.push_str(&format!(
                "{:<5} sink        : frames={} bytes={} pts=[{:?}..{:?}] codec={} dims={} changes={}\n",
                stats.kind,
                stats.frames,
                stats.bytes,
                stats.first_pts,
                stats.last_pts,
                stats.codec.clone().unwrap_or_default(),
                stats.dimensions.clone().unwrap_or_default(),
                stats.format_changes
            ));
            if let Some(p) = &stats.path {
                out.push_str(&format!("      落盘 {}（{} 字节）\n", p, stats.file_size_bytes.unwrap_or(0)));
            }
        }
        out.push_str(&format!(
            "音频              : {} {}Hz→{}Hz ratio={}/{} frames {}→{}（{}ms→{}ms, speed_ratio={:.3}）\n",
            k.audio.plan.kind,
            k.audio.plan.source_rate,
            k.audio.plan.device_rate,
            k.audio.plan.ratio_num,
            k.audio.plan.ratio_den,
            k.audio.frames_in,
            k.audio.frames_out,
            k.audio.duration_ms_in,
            k.audio.duration_ms_out,
            k.audio.speed_ratio
        ));
        out.push_str(&format!(
            "native 窗口       : {}（{}）\n",
            k.native_window.outcome,
            k.native_window.error_code.clone().unwrap_or_default()
        ));
        out.push_str(&format!(
            "20000x12000       : {}（area={} > max_pixels={}）\n",
            k.oversized_dimensions.error_code, k.oversized_dimensions.area, k.oversized_dimensions.max_pixels
        ));
    }

    if want("media") {
        let m = &r.media_plane;
        out.push_str("\n## 数据面（XMD1）\n");
        out.push_str(&format!(
            "头长 {} 字节，payload 上限 {}，已知 flags 0x{:08x}\n",
            m.header_len, m.max_payload, m.known_flags
        ));
        out.push_str(&format!(
            "往返              : payload={} B encoded={} B consumed={} equal={}\n",
            m.roundtrip.payload_bytes,
            m.roundtrip.encoded_bytes,
            m.roundtrip.consumed,
            m.roundtrip.decoded_equal
        ));
        out.push_str(&format!("头字节            : {}\n", m.roundtrip.header_hex));
        for case in &m.negatives {
            out.push_str(&format!(
                "  - {:<32} {} {}\n",
                case.case,
                case.outcome,
                case.error_code.clone().unwrap_or_default()
            ));
        }
    }

    if want("dlna") {
        let d = &r.dlna;
        out.push_str("\n## DLNA/UPnP AV（T41）\n");
        out.push_str(&format!(
            "SSDP              : M-SEARCH 往返={} 通告往返={}\n",
            d.ssdp["search_roundtrip"], d.ssdp["notify_roundtrip"]
        ));
        for case in d.ssdp["rejects"].as_array().into_iter().flatten() {
            out.push_str(&format!(
                "  - {:<24} {}\n",
                case["case"].as_str().unwrap_or(""),
                case["outcome"].as_str().unwrap_or("")
            ));
        }
        out.push_str(&format!(
            "注册表            : renderer={}；推送 video/mp4={} hevc 拒绝={} rtsp 拒绝={}\n",
            d.registry["renderers"],
            d.registry["push_video_mp4"],
            d.registry["push_video_hevc_refused"],
            d.registry["push_rtsp_refused"]
        ));
        for case in d.registry["policy_rejects"].as_array().into_iter().flatten() {
            out.push_str(&format!(
                "  - 抓取策略拒绝 {:<10} {}\n",
                case["case"].as_str().unwrap_or(""),
                case["outcome"].as_str().unwrap_or("")
            ));
        }
        out.push_str(&format!(
            "SOAP              : 动作往返={} Fault(701)={}\n",
            d.soap["actions_roundtrip"], d.soap["fault_has_701"]
        ));
        for case in d.soap["rejects"].as_array().into_iter().flatten() {
            out.push_str(&format!(
                "  - {:<16} {}\n",
                case["case"].as_str().unwrap_or(""),
                case["outcome"].as_str().unwrap_or("")
            ));
        }
        out.push_str(&format!(
            "受限 DMS          : 根子项={} 越权 objectID={} 超分页={}\n",
            d.dms["root_children"], d.dms["forbidden_object_code"], d.dms["over_count_code"]
        ));
        out.push_str(&format!(
            "URL lease         : Stop 前={} Stop 后={}\n",
            d.leases["live_before_stop"], d.leases["live_after_stop"]
        ));
        out.push_str(&format!(
            "XML 加固          : XXE 拒绝={} 深度拒绝={}\n",
            d.xml["xxe_rejected"], d.xml["depth_rejected"]
        ));
        for b in &d.blocked {
            out.push_str(&format!("  · blocked: {b}\n"));
        }
    }

    if want("mirror") {
        let m = &r.mirror;
        out.push_str("\n## AirPlay 镜像路径（T33）\n");
        for (label, outcome) in &m.steps {
            out.push_str(&format!("  - {label:<22} {outcome}\n"));
        }
        out.push_str(&format!(
            "视频              : forwarded={} dropped={} format_changes={} recoveries={}\n",
            m.video["frames_forwarded"],
            m.video["frames_dropped"],
            m.video["format_changes"],
            m.video["recoveries"]
        ));
        out.push_str(&format!(
            "背压/恢复         : 高水位 {} B / 预算 {} B，超预算丢帧 {}，关键帧恢复={}\n",
            m.video["peak_queued_bytes"],
            m.video["queue_budget_bytes"],
            m.video["budget_drops_during_backpressure"],
            m.video["recovering_then_keyframe_recovered"]
        ));
        out.push_str(&format!(
            "音频              : {} 块，重采样 {} 块，{} → {} 样本（高水位 {} B）\n",
            m.audio["blocks_forwarded"],
            m.audio["resampled_blocks"],
            m.audio["samples_in"],
            m.audio["samples_out"],
            m.audio["peak_resample_bytes"]
        ));
        out.push_str(&format!(
            "漂移（代 {}）     : 采样 {}，max={}ms mean={}ms\n",
            m.drift["generation"], m.drift["samples"], m.drift["max_abs_drift_ms"], m.drift["mean_drift_ms"]
        ));
        out.push_str(&format!(
            "能力广告          : {:?}（HEVC 拒绝={}）\n",
            m.capability["advertised_features"], m.capability["hevc_rejected"]
        ));
        for b in &m.blocked {
            out.push_str(&format!("  · blocked: {b}\n"));
        }
    }

    if want("qshare") {
        let q = &r.quickshare;
        out.push_str("\n## Quick Share / UKEY2（T19/T20）\n");
        out.push_str(&format!(
            "framing           : {} 字节大端长度前缀，本仓上限 {} B（非协议常量，F-05）\n",
            q.framing["length_prefix_bytes"], q.framing["default_max_frame_bytes"]
        ));
        out.push_str(&format!(
            "握手              : cipher={} key_schedule={} established={}\n",
            q.handshake["cipher"], q.handshake["key_schedule"], q.handshake["established"]
        ));
        out.push_str(&format!(
            "双方一致          : auth={} next_secret={} pin={}（pin={}）\n",
            q.handshake["auth_strings_match"],
            q.handshake["next_secrets_match"],
            q.handshake["pin_matches"],
            q.handshake["pin"]
        ));
        out.push_str(&format!(
            "分片等价          : {}（{} 种切分）\n",
            q.fragmentation["consistent"], q.fragmentation["modes"]
        ));
        for case in &q.negatives {
            out.push_str(&format!("  - {:<34} {}\n", case.0, case.1));
        }
        if !q.transport.is_null() {
            out.push_str(&format!(
                "传输链路（T21）   : SecureMessage 往返={} 篡改拒绝={}（序号严格 +1）\n",
                q.transport["secure_message"]["roundtrip"],
                q.transport["secure_message"]["tamper_rejected"]
            ));
            let pubv = &q.transport["published"];
            out.push_str(&format!(
                "payload 落盘      : {} 块 × {} B → published={} 字节={} hash一致={}\n",
                q.transport["payload"]["chunks"],
                q.transport["payload"]["chunk_bytes"],
                pubv["relative_path"],
                pubv["bytes"],
                pubv["hash_matches_content"]
            ));
            out.push_str(&format!(
                "payloadID 绑定    : 混入外来 id 被拒={}\n",
                q.transport["payload"]["foreign_payload_id_rejected"]
            ));
        }
        out.push_str(&format!(
            "payload gate      : 未确认={} 错误码={} 确认后={}\n",
            q.payload_gate["before_confirmation"],
            q.payload_gate["wrong_code"],
            q.payload_gate["after_confirmation"]
        ));
        out.push_str(&format!(
            "F-15 冲突         : 采用 {}；{} 待真机裁决\n",
            q.conflict["adopted"], q.conflict["awaiting"]
        ));
        for b in &q.blocked {
            out.push_str(&format!("  · blocked: {b}\n"));
        }
    }

    if section.is_none() {
        out.push_str("\n## blocked（本阶段明确不做/做不到）\n");
        for b in &r.blocked {
            out.push_str(&format!("  - {b}\n"));
        }
        out.push_str("\n## 待用户手动测试\n");
        for m in &r.manual_tests {
            out.push_str(&format!("  - {m}\n"));
        }
        out.push_str(&format!(
            "\n汇总：{}\n",
            if r.ok { "全部场景断言通过（evidence_level=simulated，非真机）" } else { "存在未通过断言：见上" }
        ));
    }
    out
}

/// 从参数解析子命令与 `--json`。返回 `(section, json)`；未知子命令返回 `Err`。
pub fn parse_args(args: &[String]) -> Result<(Option<&'static str>, bool), String> {
    let mut section = None;
    let mut json = false;
    for a in args {
        match a.as_str() {
            "--json" => json = true,
            "run" => section = None,
            "discovery" => section = Some("discovery"),
            "session" => section = Some("session"),
            "sink" => section = Some("sink"),
            "media" => section = Some("media"),
            "qshare" | "quickshare" => section = Some("qshare"),
            "mirror" => section = Some("mirror"),
            "dlna" => section = Some("dlna"),
            "--help" | "-h" => return Err("help".to_string()),
            other => return Err(format!("未知子命令 {other:?}")),
        }
    }
    Ok((section, json))
}
