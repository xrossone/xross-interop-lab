//! receiver 侧控制面（T45 可行性 gate 的实现部分）。
//!
//! **可达边界（F-34）**：桌面 receiver 能做的是"可被发现 + CONNECT/CONNECTED + 心跳 + 状态记账 +
//! LAUNCH 白名单"，**不是**"被 stock sender 连上"——认证在消息层，sender 把 `AuthResponse` 的证书链
//! 走到**它自己的信任库**，默认只信厂商根（F-26/F-28/F-33）。因此：
//!
//! - 生产闸门 [`VendorTrustRequired`]：**恒拒绝**——本仓没有、也不会获取厂商签发的设备凭据材料；
//! - 演示闸门 [`TestRootTrust`]：**只有调用方显式构造**时存在，名字里就写着 `test-root`，
//!   实现里不含任何证书/密钥（它只是"我们知道自己在用测试根"的显式标记，不是绕过）；
//! - **不内置任何 app id**：LAUNCH 只接受调用方配置过的 id（F-17/F-32），未配置即拒绝；
//! - 不出现任何屏幕镜像能力声明（本 profile 只做控制面/URL 语义）。

use crate::castv2::{RECEIVER_PLATFORM_ID, SENDER_PLATFORM_ID};
use crate::namespaces::{
    ApplicationStatus, CastPayload, ReceiverStatus, NS_CONNECTION, NS_HEARTBEAT, NS_RECEIVER,
    TYPE_CONNECTED, TYPE_PONG,
};
use interop_contract::error::{Error, ErrorCode};

/// receiver 侧会话状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverState {
    /// 未连接。
    Idle,
    /// 收到 CONNECT 并回了 CONNECTED。
    Connected,
    /// 已拉起某个 app。
    Launched,
    /// 连接已关闭（CLOSE）或会话已停止。
    Closed,
}

/// 闸门决定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustDecision {
    /// 放行（仅 test-root 演示路径可能返回它）。
    Allow,
    /// 拒绝，附带理由（生产路径恒此值）。
    Refuse { reason: String, trusted_root: &'static str },
}

/// 信任闸门：**只做放行/拒绝的决定，不做任何握手**。
pub trait SenderTrustGate {
    fn name(&self) -> &'static str;
    /// 发送方是否能被接受。`sender_trust_store_has_us` 由调用方如实告知
    /// （真实语义：**对方的信任库里有没有我们的证书**）。
    fn decide(&self, sender_trust_store_has_us: bool) -> TrustDecision;
}

/// 生产闸门：恒拒绝（F-26..F-28：需要厂商签发、经其根背书的设备凭据材料）。
#[derive(Debug, Default)]
pub struct VendorTrustRequired;

impl SenderTrustGate for VendorTrustRequired {
    fn name(&self) -> &'static str {
        "vendor-required"
    }

    fn decide(&self, _sender_trust_store_has_us: bool) -> TrustDecision {
        TrustDecision::Refuse {
            reason: "证书链必须根到厂商信任的 CA；本仓不获取、不伪造该材料（T45-03：产品集成停止）"
                .to_string(),
            trusted_root: "vendor",
        }
    }
}

/// 演示闸门：**仅调用方显式构造**时存在，用于本仓 sender↔receiver 自配对闭环。
#[derive(Debug, Default)]
pub struct TestRootTrust;

impl SenderTrustGate for TestRootTrust {
    fn name(&self) -> &'static str {
        "test-root"
    }

    fn decide(&self, sender_trust_store_has_us: bool) -> TrustDecision {
        if sender_trust_store_has_us {
            TrustDecision::Allow
        } else {
            TrustDecision::Refuse {
                reason: "test-root 只在对方**显式**信任同一测试根时成立；stock sender 默认不信任它"
                    .to_string(),
                trusted_root: "test-root",
            }
        }
    }
}

/// LAUNCH 结果（**拒绝必须带理由与错误码，不写成功**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchOutcome {
    Launched {
        app_id: String,
        transport_id: String,
        session_id: String,
    },
    Refused {
        code: ErrorCode,
        reason: String,
    },
}

/// receiver 侧会话（控制面记账）。
pub struct ReceiverSession {
    gate: Box<dyn SenderTrustGate>,
    /// 本节点**允许**被拉起的 app id（由调用方配置；本层绝不内置）。
    configured_apps: Vec<String>,
    state: ReceiverState,
    sender_trust_store_has_us: bool,
    connected_senders: u32,
    launched_app: Option<(String, String, String)>,
    pongs: u32,
    statuses_sent: u32,
    refused_launches: u32,
    close_seen: bool,
}

impl ReceiverSession {
    /// 生产默认：`VendorTrustRequired`（恒拒绝）+ **空** app 白名单。
    pub fn new() -> Self {
        Self::with_gate(Box::new(VendorTrustRequired), Vec::new())
    }

    /// 演示路径：调用方显式给出 test-root 闸门与它允许拉起的 app id。
    pub fn test_root_for_demo(allowed_apps: Vec<String>) -> Self {
        Self::with_gate(Box::new(TestRootTrust), allowed_apps)
    }

    pub fn with_gate(gate: Box<dyn SenderTrustGate>, configured_apps: Vec<String>) -> Self {
        Self {
            gate,
            configured_apps,
            state: ReceiverState::Idle,
            sender_trust_store_has_us: false,
            connected_senders: 0,
            launched_app: None,
            pongs: 0,
            statuses_sent: 0,
            refused_launches: 0,
            close_seen: false,
        }
    }

    pub fn gate_name(&self) -> &'static str {
        self.gate.name()
    }

    pub fn state(&self) -> ReceiverState {
        self.state
    }

    pub fn configured_apps(&self) -> &[String] {
        &self.configured_apps
    }

    /// 调用方如实告知：**对端信任库里有没有我们的证书**（真实世界的判定依据）。
    pub fn note_sender_trust(&mut self, has_us: bool) {
        self.sender_trust_store_has_us = has_us;
    }

    /// 收到 CONNECT：闸门放行才回 CONNECTED（F-30：**仅当 sender 给了协议版本才回**）。
    pub fn on_connect(
        &mut self,
        sender_protocol_version: Option<u32>,
    ) -> Result<Option<CastPayload>, Error> {
        match self.gate.decide(self.sender_trust_store_has_us) {
            TrustDecision::Allow => {}
            TrustDecision::Refuse { reason, trusted_root } => {
                // 闸门拒绝：**不改变状态、不建立半开的会话**（T43-01 的同一纪律）。
                return Err(Error::new(
                    ErrorCode::VendorGated,
                    format!("receiver 侧拒绝连接（信任根={trusted_root}）：{reason}"),
                ));
            }
        }
        self.connected_senders += 1;
        self.state = ReceiverState::Connected;
        // 来源：CONNECTED 只在 sender 提供了协议版本时回复。
        Ok(sender_protocol_version.map(|_| CastPayload::Connected))
    }

    /// 收到心跳 PING → 回 PONG。
    pub fn on_ping(&mut self) -> Result<CastPayload, Error> {
        if self.state == ReceiverState::Idle || self.state == ReceiverState::Closed {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "未连接或已关闭的 receiver 不应答心跳",
            ));
        }
        self.pongs += 1;
        Ok(CastPayload::Pong)
    }

    /// 收到 LAUNCH：**只接受调用方配置过的 app id**（本层不内置任何 id，也不申请）。
    pub fn on_launch(&mut self, app_id: &str) -> Result<LaunchOutcome, Error> {
        if self.state != ReceiverState::Connected && self.state != ReceiverState::Launched {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "LAUNCH 需要先建立连接（CONNECT/CONNECTED）",
            ));
        }
        if !self.configured_apps.iter().any(|a| a == app_id) {
            self.refused_launches += 1;
            return Ok(LaunchOutcome::Refused {
                code: ErrorCode::DestinationUnavailable,
                reason: format!(
                    "app id {app_id} 未在本节点配置（本仓不内置、不申请 app id；F-17/F-32）"
                ),
            });
        }
        // transportId/sessionId 由本端生成（来源把它们放在 RECEIVER_STATUS 里）。
        let transport_id = format!("{app_id}-{}-web", self.connected_senders);
        let session_id = format!("{app_id}-session-{}", self.connected_senders);
        self.launched_app = Some((app_id.to_string(), transport_id.clone(), session_id.clone()));
        self.state = ReceiverState::Launched;
        Ok(LaunchOutcome::Launched {
            app_id: app_id.to_string(),
            transport_id,
            session_id,
        })
    }

    /// 当前 `RECEIVER_STATUS`（launch 成功后必须再广播一次——来源行为）。
    pub fn status(&mut self) -> CastPayload {
        self.statuses_sent += 1;
        let applications = match &self.launched_app {
            Some((app_id, transport_id, session_id)) => vec![ApplicationStatus {
                app_id: app_id.clone(),
                display_name: None,
                session_id: Some(session_id.clone()),
                transport_id: Some(transport_id.clone()),
                namespaces: vec![crate::namespaces::NS_MEDIA.to_string()],
                status_text: None,
            }],
            None => Vec::new(),
        };
        CastPayload::ReceiverStatus(ReceiverStatus {
            applications,
            volume: None,
            is_stand_by: Some(self.state != ReceiverState::Launched),
        })
    }

    /// 收到 CLOSE：回到 Idle 并丢弃 app 记账（**不留半开状态**）。
    pub fn on_close(&mut self) {
        self.close_seen = true;
        self.launched_app = None;
        self.state = ReceiverState::Closed;
    }

    /// 报告用的记账（含闸门名字：产品路径必须显示 vendor-required）。
    pub fn accounting(&self) -> ReceiverAccounting {
        ReceiverAccounting {
            gate: self.gate.name(),
            state: self.state,
            connected_senders: self.connected_senders,
            pongs: self.pongs,
            statuses_sent: self.statuses_sent,
            refused_launches: self.refused_launches,
            close_seen: self.close_seen,
            holding_app: self.launched_app.is_some(),
        }
    }
}

impl Default for ReceiverSession {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ReceiverSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ReceiverSession(gate={}, state={:?}, senders={}, pongs={}, apps={:?})",
            self.gate.name(),
            self.state,
            self.connected_senders,
            self.pongs,
            self.configured_apps
        )
    }
}

/// 记账快照（只读报告）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverAccounting {
    pub gate: &'static str,
    pub state: ReceiverState,
    pub connected_senders: u32,
    pub pongs: u32,
    pub statuses_sent: u32,
    pub refused_launches: u32,
    pub close_seen: bool,
    pub holding_app: bool,
}

/// receiver 广播的 TXT 记录（**只构造/解析，不组播**；键来自 F-31）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverInfo {
    pub unique_id: String,
    /// `ve`：来源默认 2。
    pub version: u32,
    /// `ca`：能力位掩码（本仓只透传调用方给的值）。
    pub capabilities: u32,
    /// `st`：0 = idle、1 = busy-join。
    pub status: u32,
    pub friendly_name: String,
    pub model_name: String,
}

/// `st` 取值（F-31）。
pub const STATUS_IDLE: u32 = 0;
/// 忙（有会话在跑）。
pub const STATUS_BUSY_JOIN: u32 = 1;

/// TXT 里允许出现的键（F-31：`rs`/`bs`/`nf`/`ic`/`cd`/`rm` 在来源里**不存在** → 不得构造）。
pub const KNOWN_TXT_KEYS: [&str; 6] = ["id", "ve", "ca", "st", "fn", "md"];

/// 广播的服务类型（F-19/F-31）。
pub const SERVICE_TYPE: &str = "_googlecast._tcp";
/// 真实设备的连接端口（F-19 来源取值）。
pub const PORT_REAL_DEVICE: u16 = 8009;
/// 参考实现独立 receiver 的端口（F-31 来源取值；**与真实设备取值分开引用，不合并**）。
pub const PORT_REFERENCE_RECEIVER: u16 = 8010;

impl ReceiverInfo {
    /// 从会话状态构造（`st` 由是否有 app 在跑决定）。
    pub fn from_state(session: &ReceiverSession, unique_id: &str, name: &str, model: &str) -> Self {
        Self {
            unique_id: unique_id.to_string(),
            version: 2,
            capabilities: 0,
            status: if session.accounting().holding_app {
                STATUS_BUSY_JOIN
            } else {
                STATUS_IDLE
            },
            friendly_name: name.to_string(),
            model_name: model.to_string(),
        }
    }

    /// 构造 TXT 键值对（键顺序固定，便于对照）。
    pub fn to_txt(&self) -> Vec<(String, String)> {
        vec![
            ("id".to_string(), self.unique_id.clone()),
            ("ve".to_string(), self.version.to_string()),
            ("ca".to_string(), self.capabilities.to_string()),
            ("st".to_string(), self.status.to_string()),
            ("fn".to_string(), self.friendly_name.clone()),
            ("md".to_string(), self.model_name.clone()),
        ]
    }

    /// 解析 TXT 键值对：**未登记的键一律拒绝**，缺 `id` 或 `st` 取值超出 0/1 也拒绝。
    pub fn from_txt(pairs: &[(String, String)]) -> Result<Self, Error> {
        let mut id = None;
        let mut version = None;
        let mut capabilities = None;
        let mut status = None;
        let mut friendly = None;
        let mut model = None;
        for (key, value) in pairs {
            if !KNOWN_TXT_KEYS.contains(&key.as_str()) {
                return Err(Error::new(
                    ErrorCode::InvalidFrame,
                    format!("TXT 出现未登记的键 {key}（F-31 只有 id/ve/ca/st/fn/md）"),
                ));
            }
            let number = |what: &str| -> Result<u32, Error> {
                value.parse::<u32>().map_err(|_| {
                    Error::new(
                        ErrorCode::InvalidFrame,
                        format!("TXT {what} 不是无符号整数：{value}"),
                    )
                })
            };
            match key.as_str() {
                "id" => id = Some(value.clone()),
                "ve" => version = Some(number("ve")?),
                "ca" => capabilities = Some(number("ca")?),
                "st" => {
                    let st = number("st")?;
                    if st != STATUS_IDLE && st != STATUS_BUSY_JOIN {
                        return Err(Error::new(
                            ErrorCode::InvalidFrame,
                            format!("TXT st={st} 超出来源登记取值 0/1（F-31）"),
                        ));
                    }
                    status = Some(st);
                }
                "fn" => friendly = Some(value.clone()),
                "md" => model = Some(value.clone()),
                _ => unreachable!("已在上面拒绝"),
            }
        }
        Ok(Self {
            unique_id: id.ok_or_else(|| {
                Error::new(ErrorCode::InvalidFrame, "TXT 缺 id（发现的关键字段）")
            })?,
            version: version.unwrap_or(2),
            capabilities: capabilities.unwrap_or(0),
            status: status.unwrap_or(STATUS_IDLE),
            friendly_name: friendly.unwrap_or_default(),
            model_name: model.unwrap_or_default(),
        })
    }
}

/// 类型字面量（receiver 侧应答用；与 `namespaces` 的同名字面量是同一批字段）。
pub const TYPES: [&str; 2] = [TYPE_CONNECTED, TYPE_PONG];

/// 命名空间集合（receiver 侧服务的四个）。
pub const SERVED_NAMESPACES: [&str; 4] = [NS_CONNECTION, NS_HEARTBEAT, NS_RECEIVER, "urn:x-cast:com.google.cast.media"];

/// 平台 ID（来源取值的两个角色名）。
pub const PLATFORM_IDS: [&str; 2] = [SENDER_PLATFORM_ID, RECEIVER_PLATFORM_ID];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_gate_refuses_and_leaves_no_session() {
        let mut session = ReceiverSession::new();
        session.note_sender_trust(true); // 即便"对端信任我们"，生产闸门也拒绝
        let err = session.on_connect(Some(3)).expect_err("生产路径恒拒绝");
        assert_eq!(err.code, ErrorCode::VendorGated);
        assert_eq!(session.state(), ReceiverState::Idle, "拒绝后不得留下半开会话");
        assert_eq!(session.accounting().connected_senders, 0);
        assert_eq!(session.gate_name(), "vendor-required");
    }

    #[test]
    fn test_root_gate_allows_only_when_the_peer_trusts_us() {
        let mut session = ReceiverSession::test_root_for_demo(vec!["DEMOAPP".to_string()]);
        assert_eq!(session.gate_name(), "test-root");
        assert_eq!(
            session.on_connect(Some(3)).unwrap_err().code,
            ErrorCode::VendorGated,
            "对方不信任测试根时同样拒绝"
        );
        session.note_sender_trust(true);
        assert_eq!(
            session.on_connect(Some(3)).expect("放行"),
            Some(CastPayload::Connected)
        );
        assert_eq!(session.on_connect(None).expect("放行"), None, "未给协议版本不回 CONNECTED");
    }

    #[test]
    fn launch_only_accepts_configured_app_ids() {
        let mut session = ReceiverSession::test_root_for_demo(vec!["DEMOAPP".to_string()]);
        session.note_sender_trust(true);
        let _ = session.on_connect(Some(3)).expect("连接");
        assert!(matches!(
            session.on_launch("DEMOAPP").expect("允许"),
            LaunchOutcome::Launched { .. }
        ));
        match session.on_launch("NOT-CONFIGURED").expect("不报错但拒绝") {
            LaunchOutcome::Refused { code, reason } => {
                assert_eq!(code, ErrorCode::DestinationUnavailable);
                assert!(reason.contains("未在本节点配置"));
            }
            other => panic!("未配置的 app 不得被拉起：{other:?}"),
        }
        assert_eq!(session.accounting().refused_launches, 1);
    }

    #[test]
    fn txt_roundtrip_and_unknown_key_rejected() {
        let session = ReceiverSession::new();
        let info = ReceiverInfo::from_state(&session, "abcd1234", "客厅", "xross-demo");
        let txt = info.to_txt();
        assert_eq!(txt.len(), 6);
        assert_eq!(
            ReceiverInfo::from_txt(&txt).expect("往返"),
            info,
            "TXT 往返等值"
        );
        let mut bad = txt.clone();
        bad.push(("rs".to_string(), "1".to_string()));
        assert_eq!(
            ReceiverInfo::from_txt(&bad).unwrap_err().code,
            ErrorCode::InvalidFrame,
            "未登记键必须拒绝（F-31）"
        );
        let mut bad_st = txt.clone();
        bad_st[3] = ("st".to_string(), "7".to_string());
        assert_eq!(
            ReceiverInfo::from_txt(&bad_st).unwrap_err().code,
            ErrorCode::InvalidFrame
        );
        let no_id: Vec<(String, String)> = txt.into_iter().filter(|(k, _)| k != "id").collect();
        assert_eq!(
            ReceiverInfo::from_txt(&no_id).unwrap_err().code,
            ErrorCode::InvalidFrame
        );
    }

    #[test]
    fn close_drops_the_app_and_stops_answering() {
        let mut session = ReceiverSession::test_root_for_demo(vec!["DEMOAPP".to_string()]);
        session.note_sender_trust(true);
        let _ = session.on_connect(Some(3)).expect("连接");
        let _ = session.on_launch("DEMOAPP").expect("拉起");
        assert!(session.accounting().holding_app);
        let _ = session.on_ping().expect("连接中应答心跳");
        session.on_close();
        assert_eq!(session.state(), ReceiverState::Closed);
        assert!(!session.accounting().holding_app, "关闭后不得仍持有 app");
        assert_eq!(
            session.on_ping().unwrap_err().code,
            ErrorCode::InvalidFrame,
            "关闭后不应答心跳"
        );
    }
}
