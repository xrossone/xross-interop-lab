//! demo 报告结构（docs/goal-phase-3.md T5）。
//!
//! 每个报告都携带**证据等级**与 wire 来源：demo 输出不得让人误以为是真机互通结果。

use serde::Serialize;

pub const TOOL: &str = "xinterop-demo";
pub const INTERFACE_VERSION: &str = "interop.api/0.1";
/// demo 只做本机 headless 执行：wire 全部自制 fixture，故上限为 `simulated`。
pub const EVIDENCE_LEVEL: &str = "simulated";
pub const WIRE: &str = "self-authored-fixture";

#[derive(Debug, Clone, Serialize)]
pub struct DemoReport {
    pub tool: &'static str,
    pub interface_version: &'static str,
    pub evidence_level: &'static str,
    pub wire: &'static str,
    pub generated_unix_ms: u128,
    pub ok: bool,
    pub discovery: DiscoveryReport,
    pub session: SessionReport,
    pub sink: SinkReport,
    pub media_plane: MediaPlaneReport,
    pub quickshare: QuickShareReport,
    pub mirror: MirrorReport,
    pub dlna: DlnaReport,
    pub wfd: WfdReport,
    pub cast: CastReport,
    pub blocked: Vec<String>,
    pub manual_tests: Vec<String>,
}

// ---------- Google Cast（T43）----------

#[derive(Debug, Clone, Serialize)]
pub struct CastReport {
    pub evidence_level: &'static str,
    pub wire: &'static str,
    pub envelope: serde_json::Value,
    pub namespaces: serde_json::Value,
    pub session: serde_json::Value,
    pub discovery: serde_json::Value,
    pub rejects: Vec<serde_json::Value>,
    pub blocked: Vec<String>,
}

// ---------- WFD/Miracast（T37）----------

#[derive(Debug, Clone, Serialize)]
pub struct WfdReport {
    pub evidence_level: &'static str,
    pub wire: &'static str,
    pub messages: serde_json::Value,
    pub session: serde_json::Value,
    pub negotiation: serde_json::Value,
    pub ie: serde_json::Value,
    pub rtp: serde_json::Value,
    pub rejects: Vec<serde_json::Value>,
    pub blocked: Vec<String>,
}

// ---------- DLNA/UPnP AV（T41）----------

#[derive(Debug, Clone, Serialize)]
pub struct DlnaReport {
    pub evidence_level: &'static str,
    pub wire: &'static str,
    pub ssdp: serde_json::Value,
    pub registry: serde_json::Value,
    pub soap: serde_json::Value,
    pub dms: serde_json::Value,
    pub leases: serde_json::Value,
    pub xml: serde_json::Value,
    pub blocked: Vec<String>,
}

// ---------- AirPlay 镜像路径（T33）----------

#[derive(Debug, Clone, Serialize)]
pub struct MirrorReport {
    pub evidence_level: &'static str,
    pub wire: &'static str,
    #[serde(serialize_with = "serialize_pairs")]
    pub steps: Vec<(String, String)>,
    pub video: serde_json::Value,
    pub audio: serde_json::Value,
    pub drift: serde_json::Value,
    pub capability: serde_json::Value,
    pub blocked: Vec<String>,
}

// ---------- Quick Share / UKEY2（T19/T20）----------

#[derive(Debug, Clone, Serialize)]
pub struct QuickShareReport {
    pub evidence_level: &'static str,
    pub wire: &'static str,
    pub framing: serde_json::Value,
    pub handshake: serde_json::Value,
    pub fragmentation: serde_json::Value,
    #[serde(serialize_with = "serialize_pairs")]
    pub negatives: Vec<(String, String)>,
    pub transport: serde_json::Value,
    pub payload_gate: serde_json::Value,
    pub conflict: serde_json::Value,
    pub blocked: Vec<String>,
}

fn serialize_pairs<S>(pairs: &[(String, String)], s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(pairs.len()))?;
    for (case, outcome) in pairs {
        seq.serialize_element(&serde_json::json!({ "case": case, "outcome": outcome }))?;
    }
    seq.end()
}

// ---------- 发现面 ----------

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryReport {
    pub evidence_level: &'static str,
    pub local_interfaces: serde_json::Value,
    pub local_probe_commands: serde_json::Value,
    pub platform_support: serde_json::Value,
    pub mdns_service_types: Vec<String>,
    pub txt_pairs: Vec<(String, String)>,
    pub txt_rdata_hex: String,
    pub feature_string_status: String,
    pub implemented_discovery_sources: Vec<DiscoverySourceStatus>,
    pub registry: RegistryReport,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoverySourceStatus {
    pub source: &'static str,
    /// 是否产生真实网络行为（fake 源为 false）
    pub network: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryReport {
    pub ttl_s: u64,
    pub authority_rows: Vec<RowReport>,
    pub same_name_same_address_rows: usize,
    pub alias_groups: Vec<AliasGroupReport>,
    pub expiry: ExpiryReport,
    pub interface_down_affected: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RowReport {
    pub subject: String,
    pub source: &'static str,
    pub profile_id: String,
    pub display_name: String,
    pub identity_claim: Option<String>,
    pub addresses: Vec<AddressReport>,
    pub capabilities: Vec<CapabilityReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddressReport {
    pub host: String,
    pub port: u16,
    pub interface: String,
    pub secure: bool,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityReport {
    pub profile_id: String,
    pub role: String,
    pub provider_id: String,
    pub available: bool,
    pub state: String,
    pub prerequisites: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AliasGroupReport {
    pub alias: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpiryReport {
    pub sendable_before_ttl: usize,
    pub sendable_after_ttl: usize,
    pub sendable_after_interface_down: usize,
}

// ---------- 会话面 ----------

#[derive(Debug, Clone, Serialize)]
pub struct SessionReport {
    pub transport: &'static str,
    pub framing: FramingLimits,
    pub protocol_hardening: Vec<CaseOutcome>,
    pub production_keying: KeyingRunReport,
    pub test_keying: KeyingRunReport,
    pub churn: ChurnReport,
    pub concurrency_cap: CapReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FramingLimits {
    pub max_header_bytes: usize,
    pub max_body_bytes: usize,
    pub max_headers: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseOutcome {
    pub case: String,
    pub outcome: &'static str,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepReport {
    pub step: String,
    pub bytes_in: usize,
    pub status: Option<u16>,
    pub response_bytes: usize,
    pub error_code: Option<String>,
    pub state_after: &'static str,
    pub has_stream: bool,
    pub allocated_video_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyingRunReport {
    pub keying: &'static str,
    pub warning: Option<&'static str>,
    pub advertised_features: Vec<String>,
    pub steps: Vec<StepReport>,
    pub setup_status: u16,
    pub state_after_setup: &'static str,
    pub has_stream_after_setup: bool,
    pub allocated_video_bytes_after_setup: u64,
    pub has_stream_after_teardown: bool,
    pub reserved_ports_after_teardown: usize,
    pub fp_setup_error: Option<String>,
    pub tracked_sessions_after_disconnect: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChurnReport {
    pub iterations: u32,
    pub active_sessions: usize,
    pub reserved_ports: usize,
    pub tracked_sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapReport {
    pub max_sessions: u32,
    pub second_accept: String,
    pub after_disconnect: String,
}

// ---------- sink 面 ----------

#[derive(Debug, Clone, Serialize)]
pub struct SinkReport {
    pub frames_fed: u64,
    pub null: SinkStatsReport,
    pub file: SinkStatsReport,
    pub audio: AudioReport,
    pub passthrough: PlanReport,
    pub native_window: CaseOutcome,
    pub oversized_dimensions: OversizeReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct SinkStatsReport {
    pub kind: &'static str,
    pub frames: u64,
    pub bytes: u64,
    pub first_pts: Option<i64>,
    pub last_pts: Option<i64>,
    pub format_changes: u64,
    pub codec: Option<String>,
    pub dimensions: Option<String>,
    pub path: Option<String>,
    pub file_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PlanReport {
    pub kind: &'static str,
    pub source_rate: u32,
    pub device_rate: u32,
    pub ratio_num: u32,
    pub ratio_den: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AudioReport {
    pub plan: PlanReport,
    pub frames_in: usize,
    pub frames_out: usize,
    pub duration_ms_in: u64,
    pub duration_ms_out: u64,
    /// 输出/输入帧比 ÷ 设备率/源率；1.0 = 不变速
    pub speed_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OversizeReport {
    pub requested: String,
    pub area: u64,
    pub max_pixels: u64,
    pub error_code: String,
}

// ---------- 数据面 ----------

#[derive(Debug, Clone, Serialize)]
pub struct MediaPlaneReport {
    pub header_len: usize,
    pub max_payload: u32,
    pub known_flags: u32,
    pub roundtrip: RoundtripReport,
    pub negatives: Vec<CaseOutcome>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoundtripReport {
    pub payload_bytes: usize,
    pub encoded_bytes: usize,
    pub consumed: usize,
    pub decoded_equal: bool,
    pub header_hex: String,
}
