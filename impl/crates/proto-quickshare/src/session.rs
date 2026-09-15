//! 受限安全会话（plans/02-files.md §T20）：把 framing + 握手 + 配额/超时 + payload gate 串起来。
//!
//! 三条不变量（T19-01/T19-02 的落地）：
//! 1. **被发现 ≠ 认证**：`note_discovery` 只做记录，不改变任何认证要求；
//! 2. **握手失败不得记 transfer 成功**：`payloads_delivered` 只在 gate 打开后才可能 > 0；
//! 3. **payload gate 需要两次独立成立**：握手 Established **且** 4 位确认码已核对（UKEY2 的
//!    out-of-band 验证是规范要求的步骤，不是可选项）。
//!
//! 本层不做传输加密与 payload 层（T21）——所以 gate 打开后 `deliver_payload_bytes` 只做记账，
//! 真正的分块/加密在后续 task 里接上；这一点由 `transport_encryption_available()` 明确返回未实现。

use crate::framing::{encode_frame, FrameDecoder, DEFAULT_MAX_FRAME_BYTES};
use crate::handshake::{
    AlertType, ClientConfig, DiscoveryProvenance, HandshakeConfig, SessionKeys, Ukey2Client,
    Ukey2Server,
};
use crate::wire::{self, Ukey2MessageType};
use interop_contract::error::{Error, ErrorCode};

/// 握手期限（本仓策略值，非协议常量）：超时即发 Alert 并停用会话。
pub const HANDSHAKE_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Server,
    Client,
}

/// payload gate 拒绝原因（可判定，不模糊）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateRefusal {
    /// 会话未建立（含握手失败/超时）
    NotEstablished,
    /// 已建立但确认码未核对
    ConfirmationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Frame { message_type: Ukey2MessageType },
    Established,
    Alert { alert_type: AlertType, detail: String },
}

/// 一条会话：接收字节 → 推进握手 → 产出事件与待发字节。
pub struct QuickShareSession {
    role: Role,
    decoder: FrameDecoder,
    server: Option<Ukey2Server>,
    client: Option<Ukey2Client>,
    discovery: Option<DiscoveryProvenance>,
    confirmed: bool,
    payloads: u64,
    payload_bytes: u64,
    outbound: Vec<Vec<u8>>,
    started_ms: Option<u64>,
    deadline_ms: u64,
    dead: bool,
}

impl std::fmt::Debug for QuickShareSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "QuickShareSession(role={:?}, established={}, confirmed={}, payloads={}, dead={})",
            self.role,
            self.is_established(),
            self.confirmed,
            self.payloads,
            self.dead
        )
    }
}

impl QuickShareSession {
    /// 服务端会话（接收方）：用系统熵生成自己的 ServerInit 密钥。
    pub fn new(role: Role, config: HandshakeConfig) -> Self {
        let (server, client) = match role {
            Role::Server => (Ukey2Server::new(config.clone()).ok(), None),
            Role::Client => (
                None,
                Ukey2Client::new(ClientConfig::nearby_default(
                    config
                        .next_protocols
                        .first()
                        .map(String::as_str)
                        .unwrap_or("AES_256_CBC-HMAC_SHA256"),
                ))
                .ok(),
            ),
        };
        Self {
            role,
            decoder: FrameDecoder::new(DEFAULT_MAX_FRAME_BYTES),
            server,
            client,
            discovery: None,
            confirmed: false,
            payloads: 0,
            payload_bytes: 0,
            outbound: Vec::new(),
            started_ms: None,
            deadline_ms: HANDSHAKE_TIMEOUT_MS,
            dead: false,
        }
    }

    /// 服务端会话（测试/确定性）：注入固定熵的服务端。
    pub fn with_server(server: Ukey2Server) -> Self {
        Self {
            role: Role::Server,
            decoder: FrameDecoder::new(DEFAULT_MAX_FRAME_BYTES),
            server: Some(server),
            client: None,
            discovery: None,
            confirmed: false,
            payloads: 0,
            payload_bytes: 0,
            outbound: Vec::new(),
            started_ms: None,
            deadline_ms: HANDSHAKE_TIMEOUT_MS,
            dead: false,
        }
    }

    /// 记录发现来源。**只记录**：不改变任何认证要求（T19-02）。
    pub fn note_discovery(&mut self, provenance: DiscoveryProvenance) {
        self.discovery = Some(provenance);
    }

    pub fn discovery_provenance(&self) -> Option<DiscoveryProvenance> {
        self.discovery
    }

    /// 客户端会话：产出本端 ClientInit（作为待发字节）。
    pub fn client_init(&mut self) -> Result<Vec<u8>, Error> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| Error::new(ErrorCode::UnsupportedRole, "本会话不是客户端角色"))?;
        let bytes = client.start()?;
        self.outbound.push(bytes.clone());
        self.started_ms = Some(0);
        Ok(bytes)
    }

    /// 取出待发字节（由调用方决定怎么发送——本层不碰 socket）。
    pub fn take_outbound(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outbound)
    }

    /// 喂入收到的字节：解码帧 → 推进握手 → 返回事件。
    pub fn feed(&mut self, now_ms: u64, chunk: &[u8]) -> Result<Vec<Event>, Error> {
        if self.dead {
            return Err(Error::new(ErrorCode::AuthDenied, "会话已终止（Alert 已发出）")
                .with_phase("authorizing"));
        }
        self.started_ms.get_or_insert(now_ms);
        let started = self.started_ms.unwrap_or(now_ms);

        let mut events = Vec::new();

        // 超时检查（先于处理新字节）：未建立且超过期限即告警并停用。
        if !self.is_established() && now_ms.saturating_sub(started) > self.deadline_ms {
            self.dead = true;
            events.push(Event::Alert {
                alert_type: AlertType::InternalError,
                detail: format!("握手超时（>{}ms，本仓策略值）", self.deadline_ms),
            });
            return Ok(events);
        }

        let frames = self.decoder.push(chunk)?;
        for frame in frames {
            let message = match wire::decode_ukey2_message(&frame) {
                Ok(m) => m,
                Err(e) => {
                    self.dead = true;
                    events.push(Event::Alert {
                        alert_type: AlertType::BadMessage,
                        detail: e.to_string(),
                    });
                    continue;
                }
            };
            events.push(Event::Frame {
                message_type: message.message_type,
            });

            // 按角色与消息类型分派；不属于当前阶段的消息一律 INCORRECT_MESSAGE。
            let (result, established_now) = match self.role {
                Role::Server => self.feed_server(&message.message_type, &frame),
                Role::Client => self.feed_client(&message.message_type, &frame),
            };
            match result {
                Ok(()) => {
                    if established_now {
                        events.push(Event::Established);
                    }
                }
                Err(alert) => {
                    self.dead = true;
                    events.push(Event::Alert {
                        alert_type: alert.alert_type,
                        detail: alert.detail,
                    });
                }
            }
        }
        Ok(events)
    }

    fn feed_server(
        &mut self,
        message_type: &Ukey2MessageType,
        frame: &[u8],
    ) -> (Result<(), crate::handshake::HandshakeAlert>, bool) {
        let Some(server) = self.server.as_mut() else {
            return (
                Err(crate::handshake::HandshakeAlert::new(
                    AlertType::InternalError,
                    "服务端会话缺少握手状态",
                )),
                false,
            );
        };
        match message_type {
            Ukey2MessageType::ClientInit => match server.handle_client_init(frame, 0) {
                Ok(server_init) => {
                    self.outbound.push(server_init);
                    (Ok(()), false)
                }
                Err(a) => (Err(a), false),
            },
            Ukey2MessageType::ClientFinish => match server.handle_client_finish(frame, 0) {
                Ok(()) => (Ok(()), true),
                Err(a) => (Err(a), false),
            },
            _ => (
                Err(crate::handshake::HandshakeAlert::new(
                    AlertType::IncorrectMessage,
                    format!("服务端在握手中不接受 {}", message_type.as_wire()),
                )),
                false,
            ),
        }
    }

    fn feed_client(
        &mut self,
        message_type: &Ukey2MessageType,
        frame: &[u8],
    ) -> (Result<(), crate::handshake::HandshakeAlert>, bool) {
        let Some(client) = self.client.as_mut() else {
            return (
                Err(crate::handshake::HandshakeAlert::new(
                    AlertType::InternalError,
                    "客户端会话缺少握手状态",
                )),
                false,
            );
        };
        match message_type {
            Ukey2MessageType::ServerInit => match client.handle_server_init(frame) {
                Ok((client_finish, _keys)) => {
                    self.outbound.push(client_finish);
                    (Ok(()), false)
                }
                Err(a) => (Err(a), false),
            },
            _ => (
                Err(crate::handshake::HandshakeAlert::new(
                    AlertType::BadMessageType,
                    format!("客户端只接受 SERVER_INIT，收到 {}", message_type.as_wire()),
                )),
                false,
            ),
        }
    }

    pub fn is_established(&self) -> bool {
        match self.role {
            Role::Server => self.server.as_ref().map(|s| s.is_established()).unwrap_or(false),
            Role::Client => self.client.as_ref().map(|c| c.is_established()).unwrap_or(false),
        }
    }

    pub fn session_keys(&self) -> Option<&SessionKeys> {
        match self.role {
            Role::Server => self.server.as_ref().and_then(|s| s.session_keys()),
            Role::Client => self.client.as_ref().and_then(|c| c.session_keys()),
        }
    }

    /// 核对 4 位确认码（UKEY2 的 out-of-band 验证）。不匹配 → `pairing-failed`，gate 保持关闭。
    pub fn confirm_code(&mut self, code: &str) -> Result<(), Error> {
        let keys = self.session_keys().ok_or_else(|| {
            Error::new(ErrorCode::PairingFailed, "会话未建立，无可核对的确认码")
                .with_phase("authorizing")
        })?;
        if constant_time_eq_str(keys.pin_code(), code) {
            self.confirmed = true;
            Ok(())
        } else {
            self.confirmed = false;
            Err(Error::new(
                ErrorCode::PairingFailed,
                "确认码不匹配：不得交付任何 payload（T20-01）",
            )
            .with_phase("authorizing"))
        }
    }

    /// payload gate：握手成功 **且** 确认码已核对。否则明确拒绝（不静默降级）。
    pub fn deliver_payload_bytes(&mut self, n: u64) -> Result<(), GateRefusal> {
        if !self.is_established() || self.dead {
            return Err(GateRefusal::NotEstablished);
        }
        if !self.confirmed {
            return Err(GateRefusal::ConfirmationRequired);
        }
        // 传输加密/分块层未实现（T21）：这里只做记账，不假装已经发出字节。
        self.payloads += 1;
        self.payload_bytes += n;
        Ok(())
    }

    pub fn payloads_delivered(&self) -> u64 {
        self.payloads
    }

    pub fn payload_bytes_recorded(&self) -> u64 {
        self.payload_bytes
    }

    pub fn is_dead(&self) -> bool {
        self.dead
    }
}

fn constant_time_eq_str(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 供调用方发送时使用：把消息包成帧（F-05）。
pub fn frame_message(message: &[u8]) -> Vec<u8> {
    encode_frame(message)
}
