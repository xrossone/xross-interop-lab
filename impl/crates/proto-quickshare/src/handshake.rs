//! UKEY2 握手状态机（plans/02-files.md §T20）。
//!
//! 规则全部来自 `specs-reviewed/f02-quickshare-lan.md` 的字段表：
//! - F-07：错误以 Alert 返回，码表固定，全部致命；
//! - F-08/F-09：`version` 必须 1、`random` 恰好 32 字节；
//! - F-11：commitment = `SHA-512(序列化的 ClientFinish Ukey2Message)`，服务端**先验 commitment 再解析**；
//! - F-12：cipher 选择＝按客户端给定顺序取第一个可接受的（实现侧裁决）；
//! - F-13：只实现 `P256_SHA512`；
//! - F-16：认证串/next secret 依赖 M1‖M2（两方完整消息，不含 TCP 长度前缀）；
//! - F-18：`next_protocol` 必须匹配 `AES_256_CBC-HMAC_SHA256`。
//!
//! 安全立场：握手失败 = 无密钥、无 payload 通道（T19-01）；`random` 复用/重放一律拒绝。

use crate::crypto::{self, DhKeyPair, NONCE_BYTES};
use crate::wire::{
    self, ClientFinished, ClientInit, ServerInit, Ukey2MessageType, HANDSHAKE_CIPHER_P256_SHA512,
};
use interop_contract::error::{Error, ErrorCode};

/// 本模块采用的密钥派生哈希标签（把 F-15 的规范/实现冲突固化成断言）。
pub const KEY_SCHEDULE_HKDF_SHA256: &str = "HKDF-SHA256";

/// 认证串 / next secret 长度：UKEY2 本身不规定，由下一协议给出（F-16）。Nearby 用 32 字节。
pub const DERIVED_SECRET_LEN: usize = 32;

pub use crate::wire::Ukey2AlertType as AlertType;

/// 握手失败：按规范必须发 Alert 并关闭连接（F-07）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeAlert {
    pub alert_type: AlertType,
    pub detail: String,
}

impl HandshakeAlert {
    pub fn new(alert_type: AlertType, detail: impl Into<String>) -> Self {
        Self {
            alert_type,
            detail: detail.into(),
        }
    }

    /// 规范形态的 Alert 消息（可直接回写对端）。
    pub fn to_message(&self) -> Vec<u8> {
        wire::encode_alert(self.alert_type, &self.detail)
    }
}

impl std::fmt::Display for HandshakeAlert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "alert {:?}: {}", self.alert_type, self.detail)
    }
}

/// 会话密钥材料（Debug 打码：密钥不进日志；不提供 Serialize）。
#[derive(Clone)]
pub struct SessionKeys {
    auth: Vec<u8>,
    next: Vec<u8>,
    transcript: Vec<u8>,
    dhs: [u8; 32],
    pin: String,
}

impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SessionKeys(<auth {} bytes redacted>, <next {} bytes redacted>, pin={})",
            self.auth.len(),
            self.next.len(),
            self.pin
        )
    }
}

impl SessionKeys {
    fn derive(dhs: [u8; 32], transcript: Vec<u8>) -> Self {
        let auth = crypto::auth_string(&dhs, &transcript, DERIVED_SECRET_LEN);
        let next = crypto::next_protocol_secret(&dhs, &transcript, DERIVED_SECRET_LEN);
        let pin = crypto::pin_code(&auth);
        Self {
            auth,
            next,
            transcript,
            dhs,
            pin,
        }
    }

    pub fn auth_string(&self) -> &[u8] {
        &self.auth
    }

    /// 恒定时间比较（确认码/认证串比较不做成时序侧信道）。
    pub fn auth_string_matches(&self, other: &[u8]) -> bool {
        constant_time_eq(&self.auth, other)
    }

    pub fn next_protocol_secret(&self) -> &[u8] {
        &self.next
    }

    pub fn transcript(&self) -> &[u8] {
        &self.transcript
    }

    pub fn dh_shared_secret(&self) -> &[u8] {
        &self.dhs
    }

    /// 4 位确认码（F-17）。
    pub fn pin_code(&self) -> &str {
        &self.pin
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 服务端配置：支持的 cipher 与 next_protocol 白名单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeConfig {
    pub supported_ciphers: Vec<i32>,
    pub next_protocols: Vec<String>,
}

impl HandshakeConfig {
    /// Nearby Share/Quick Share 的取值（F-13 只支持 P256_SHA512；F-18 的 next_protocol 串）。
    pub fn nearby_default() -> Self {
        Self {
            supported_ciphers: vec![HANDSHAKE_CIPHER_P256_SHA512],
            next_protocols: vec!["AES_256_CBC-HMAC_SHA256".to_string()],
        }
    }
}

/// 客户端配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    pub next_protocol: String,
    pub cipher: i32,
}

impl ClientConfig {
    pub fn nearby_default(next_protocol: &str) -> Self {
        Self {
            next_protocol: next_protocol.to_string(),
            cipher: HANDSHAKE_CIPHER_P256_SHA512,
        }
    }
}

/// 端点如何被发现（记录用；**不构成认证**——T19-02）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryProvenance {
    /// 扫码/QR 打开
    Qr,
    /// 网络广播发现（本仓**未实现**任何发现源；这里只记录"对端声称是这么被找到的"）
    Broadcast,
    /// 用户手输/外部通道
    Manual,
}

// ---------------------------------------------------------------- 客户端

#[derive(Debug)]
enum ClientState {
    Start,
    InitSent {
        keypair: DhKeyPair,
        client_init: Vec<u8>,
        client_finished: Vec<u8>,
    },
    Established,
}

/// UKEY2 客户端。
pub struct Ukey2Client {
    config: ClientConfig,
    state: ClientState,
    keys: Option<SessionKeys>,
    random: [u8; NONCE_BYTES],
    fixed_scalar: Option<Vec<u8>>,
}

impl std::fmt::Debug for Ukey2Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ukey2Client(config={:?}, state={:?})", self.config, self.state)
    }
}

impl Ukey2Client {
    /// 生产路径：系统熵。
    pub fn new(config: ClientConfig) -> Result<Self, Error> {
        Ok(Self {
            config,
            state: ClientState::Start,
            keys: None,
            random: crypto::random_nonce()?,
            fixed_scalar: None,
        })
    }

    /// 测试/语料注入：确定性 random 与标量（**不得**用于生产路径）。
    pub fn with_fixed_entropy(
        config: ClientConfig,
        random: [u8; NONCE_BYTES],
        scalar: &[u8],
    ) -> Result<Self, Error> {
        let _ = DhKeyPair::from_scalar_bytes(scalar)?;
        Ok(Self {
            config,
            state: ClientState::Start,
            keys: None,
            random,
            fixed_scalar: Some(scalar.to_vec()),
        })
    }

    /// 第一步：生成密钥对 + commitment，产出 `ClientInit` 消息（F-08/F-11）。
    pub fn start(&mut self) -> Result<Vec<u8>, Error> {
        if !matches!(self.state, ClientState::Start) {
            return Err(Error::new(
                ErrorCode::AuthDenied,
                "start() 只能在握手开始前调用一次",
            )
            .with_phase("authorizing"));
        }
        let keypair = match &self.fixed_scalar {
            Some(s) => DhKeyPair::from_scalar_bytes(s)?,
            None => DhKeyPair::generate()?,
        };
        let client_finished = wire::encode_client_finished(&ClientFinished {
            public_key: crypto::public_key_bytes(keypair.public()),
        });
        let commitment = crypto::sha512(&client_finished).to_vec();
        let client_init = wire::encode_client_init(&ClientInit {
            version: 1,
            random: self.random.to_vec(),
            cipher_commitments: vec![(self.config.cipher, commitment)],
            next_protocol: self.config.next_protocol.clone(),
        });
        self.state = ClientState::InitSent {
            keypair,
            client_init: client_init.clone(),
            client_finished,
        };
        Ok(client_init)
    }

    /// 第二步：校验 `ServerInit`，产出 `ClientFinish`，并派生本端密钥（F-09/F-16）。
    pub fn handle_server_init(
        &mut self,
        frame: &[u8],
    ) -> Result<(Vec<u8>, SessionKeys), HandshakeAlert> {
        let (client_finished, m1) = match &self.state {
            ClientState::InitSent {
                client_init,
                client_finished,
                ..
            } => (client_finished.clone(), client_init.clone()),
            _ => {
                return Err(HandshakeAlert::new(
                    AlertType::IncorrectMessage,
                    "未发出 ClientInit 就收到 ServerInit",
                ))
            }
        };

        let message = wire::decode_ukey2_message(frame)
            .map_err(|e| HandshakeAlert::new(AlertType::BadMessage, e.to_string()))?;
        if message.message_type != Ukey2MessageType::ServerInit {
            return Err(HandshakeAlert::new(
                AlertType::BadMessageType,
                format!("期望 SERVER_INIT，收到 {}", message.message_type.as_wire()),
            ));
        }
        let init = wire::decode_server_init(&message.message_data)
            .map_err(|e| HandshakeAlert::new(AlertType::BadMessageData, e.to_string()))?;
        if init.version != 1 {
            return Err(HandshakeAlert::new(
                AlertType::BadVersion,
                format!("version={}（要求 1）", init.version),
            ));
        }
        if init.random.len() != NONCE_BYTES {
            return Err(HandshakeAlert::new(
                AlertType::BadRandom,
                format!("random={} 字节（要求 {NONCE_BYTES}）", init.random.len()),
            ));
        }
        if init.handshake_cipher != self.config.cipher {
            return Err(HandshakeAlert::new(
                AlertType::BadHandshakeCipher,
                format!(
                    "服务端选择 cipher={}，客户端只提交了 {}",
                    init.handshake_cipher, self.config.cipher
                ),
            ));
        }
        let peer_key = wire::decode_generic_public_key(&init.public_key)
            .map_err(|e| HandshakeAlert::new(AlertType::BadPublicKey, e.to_string()))?;

        let dhs = {
            let ClientState::InitSent { keypair, .. } = &self.state else {
                return Err(HandshakeAlert::new(
                    AlertType::IncorrectMessage,
                    "状态在解析期间发生变化",
                ));
            };
            keypair
                .dh(&peer_key)
                .map_err(|e| HandshakeAlert::new(AlertType::BadPublicKey, e.to_string()))?
        };

        let transcript: Vec<u8> = [m1.as_slice(), frame].concat();
        let keys = SessionKeys::derive(dhs, transcript);
        self.state = ClientState::Established;
        self.keys = Some(keys.clone());
        Ok((client_finished, keys))
    }

    pub fn is_established(&self) -> bool {
        matches!(self.state, ClientState::Established)
    }

    pub fn session_keys(&self) -> Option<&SessionKeys> {
        self.keys.as_ref()
    }
}

// ---------------------------------------------------------------- 服务端

#[derive(Debug)]
enum ServerState {
    Start,
    InitReceived {
        peer_commitment: Vec<u8>,
        m1: Vec<u8>,
        m2: Vec<u8>,
        keypair: DhKeyPair,
    },
    Established,
}

/// UKEY2 服务端。
pub struct Ukey2Server {
    config: HandshakeConfig,
    state: ServerState,
    keys: Option<SessionKeys>,
    random: [u8; NONCE_BYTES],
    seen_randoms: Vec<Vec<u8>>,
    fixed_scalar: Option<Vec<u8>>,
}

impl std::fmt::Debug for Ukey2Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ukey2Server(config={:?}, state={:?})", self.config, self.state)
    }
}

impl Ukey2Server {
    pub fn new(config: HandshakeConfig) -> Result<Self, Error> {
        Ok(Self {
            config,
            state: ServerState::Start,
            keys: None,
            random: crypto::random_nonce()?,
            seen_randoms: Vec::new(),
            fixed_scalar: None,
        })
    }

    /// 测试/语料注入：确定性 random 与标量（**不得**用于生产路径）。
    pub fn with_fixed_entropy(
        config: HandshakeConfig,
        random: [u8; NONCE_BYTES],
        scalar: &[u8],
    ) -> Result<Self, Error> {
        let _ = DhKeyPair::from_scalar_bytes(scalar)?;
        Ok(Self {
            config,
            state: ServerState::Start,
            keys: None,
            random,
            seen_randoms: Vec::new(),
            fixed_scalar: Some(scalar.to_vec()),
        })
    }

    /// 处理 `ClientInit`（F-08/F-12/F-18），成功返回要回写的 `ServerInit` 字节。
    pub fn handle_client_init(
        &mut self,
        frame: &[u8],
        _now_ms: u64,
    ) -> Result<Vec<u8>, HandshakeAlert> {
        if !matches!(self.state, ServerState::Start) {
            return Err(HandshakeAlert::new(
                AlertType::IncorrectMessage,
                "本会话已收到过 ClientInit（重放/复用一律拒绝；F-08 的 random 语义）",
            ));
        }
        let message = wire::decode_ukey2_message(frame)
            .map_err(|e| HandshakeAlert::new(AlertType::BadMessage, e.to_string()))?;
        if message.message_type != Ukey2MessageType::ClientInit {
            return Err(HandshakeAlert::new(
                AlertType::BadMessageType,
                format!("期望 CLIENT_INIT，收到 {}", message.message_type.as_wire()),
            ));
        }
        let init = wire::decode_client_init(&message.message_data)
            .map_err(|e| HandshakeAlert::new(AlertType::BadMessageData, e.to_string()))?;
        if init.version != 1 {
            return Err(HandshakeAlert::new(
                AlertType::BadVersion,
                format!("version={}（要求 1）", init.version),
            ));
        }
        if init.random.len() != NONCE_BYTES {
            return Err(HandshakeAlert::new(
                AlertType::BadRandom,
                format!("random={} 字节（要求 {NONCE_BYTES}）", init.random.len()),
            ));
        }
        if self.seen_randoms.iter().any(|r| r == &init.random) {
            return Err(HandshakeAlert::new(
                AlertType::BadRandom,
                "random 复用（replay/reuse protection，F-08）",
            ));
        }
        if init.cipher_commitments.is_empty() {
            return Err(HandshakeAlert::new(
                AlertType::BadHandshakeCipher,
                "ClientInit 没有 cipher_commitments",
            ));
        }

        // F-12（实现侧裁决）：按客户端顺序取第一个本端可接受的 cipher。
        let mut chosen: Option<(i32, Vec<u8>)> = None;
        for (cipher, commitment) in &init.cipher_commitments {
            if commitment.is_empty() {
                return Err(HandshakeAlert::new(
                    AlertType::BadHandshakeCipher,
                    "cipher commitment 为空（格式非法）",
                ));
            }
            if chosen.is_none() && self.config.supported_ciphers.contains(cipher) {
                chosen = Some((*cipher, commitment.clone()));
            }
        }
        let (cipher, peer_commitment) = chosen.ok_or_else(|| {
            HandshakeAlert::new(
                AlertType::BadHandshakeCipher,
                format!(
                    "没有可接受的 cipher（本端支持 {:?}）",
                    self.config.supported_ciphers
                ),
            )
        })?;

        if !self
            .config
            .next_protocols
            .iter()
            .any(|p| p == &init.next_protocol)
        {
            return Err(HandshakeAlert::new(
                AlertType::BadNextProtocol,
                format!(
                    "next_protocol={:?}（本端支持 {:?}）",
                    init.next_protocol, self.config.next_protocols
                ),
            ));
        }

        let keypair = match &self.fixed_scalar {
            Some(s) => DhKeyPair::from_scalar_bytes(s)
                .map_err(|e| HandshakeAlert::new(AlertType::InternalError, e.to_string()))?,
            None => DhKeyPair::generate()
                .map_err(|e| HandshakeAlert::new(AlertType::InternalError, e.to_string()))?,
        };
        let server_init = wire::encode_server_init(&ServerInit {
            version: 1,
            random: self.random.to_vec(),
            handshake_cipher: cipher,
            public_key: crypto::public_key_bytes(keypair.public()),
        });

        self.seen_randoms.push(init.random.clone());
        self.state = ServerState::InitReceived {
            peer_commitment,
            m1: frame.to_vec(),
            m2: server_init.clone(),
            keypair,
        };
        Ok(server_init)
    }

    /// 处理 `ClientFinish`：先验 commitment（F-11），再解析公钥并派生密钥（F-16）。
    pub fn handle_client_finish(
        &mut self,
        frame: &[u8],
        _now_ms: u64,
    ) -> Result<(), HandshakeAlert> {
        let (peer_commitment, m1, m2) = match &self.state {
            ServerState::InitReceived {
                peer_commitment,
                m1,
                m2,
                ..
            } => (peer_commitment.clone(), m1.clone(), m2.clone()),
            _ => {
                return Err(HandshakeAlert::new(
                    AlertType::IncorrectMessage,
                    "未收到 ClientInit 就收到 ClientFinish",
                ))
            }
        };

        let message = wire::decode_ukey2_message(frame)
            .map_err(|e| HandshakeAlert::new(AlertType::BadMessage, e.to_string()))?;
        if message.message_type != Ukey2MessageType::ClientFinish {
            return Err(HandshakeAlert::new(
                AlertType::BadMessageType,
                format!("期望 CLIENT_FINISH，收到 {}", message.message_type.as_wire()),
            ));
        }

        // **先验 commitment，再解析内容**（规范的解释顺序：commitment 在校验步骤 3、解析在 4）。
        let actual = crypto::sha512(frame);
        if !constant_time_eq(&actual, &peer_commitment) {
            return Err(HandshakeAlert::new(
                AlertType::BadMessageData,
                "ClientFinish 与 ClientInit 的 commitment 不符：握手终止",
            ));
        }

        let finished = wire::decode_client_finished(&message.message_data)
            .map_err(|e| HandshakeAlert::new(AlertType::BadMessageData, e.to_string()))?;
        let peer_key = wire::decode_generic_public_key(&finished.public_key)
            .map_err(|e| HandshakeAlert::new(AlertType::BadPublicKey, e.to_string()))?;

        let dhs = {
            let ServerState::InitReceived { keypair, .. } = &self.state else {
                return Err(HandshakeAlert::new(
                    AlertType::IncorrectMessage,
                    "状态在解析期间发生变化",
                ));
            };
            keypair
                .dh(&peer_key)
                .map_err(|e| HandshakeAlert::new(AlertType::BadPublicKey, e.to_string()))?
        };

        let transcript: Vec<u8> = [m1.as_slice(), m2.as_slice()].concat();
        self.state = ServerState::Established;
        self.keys = Some(SessionKeys::derive(dhs, transcript));
        Ok(())
    }

    pub fn is_established(&self) -> bool {
        matches!(self.state, ServerState::Established)
    }

    pub fn session_keys(&self) -> Option<&SessionKeys> {
        self.keys.as_ref()
    }
}

/// 未实现的能力（诚实返回，不假成功）：传输加密与 payload 层在 T21+（字段 F-19/F-20/F-22）。
pub fn transport_encryption_available() -> Result<(), Error> {
    Err(Error::new(
        ErrorCode::UnsupportedFeature,
        "SecureMessage(AES-256-CBC/HMAC-SHA256) 传输加密未实现（plans T21）；字段见 F-19/F-20",
    )
    .with_phase("transferring"))
}

/// 本模块使用的密钥派生标签（供 demo/diagnostics 展示）。
pub fn key_schedule_label() -> &'static str {
    KEY_SCHEDULE_HKDF_SHA256
}
