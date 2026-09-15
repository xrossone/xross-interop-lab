//! SecureMessage 传输加密（plans/02-files.md §T21；字段表 F-19/F-20）。
//!
//! 来源（都是行级事实，不是猜的）：
//! - 密钥链 F-19：`D2D_client/server = HKDF-SHA256(ikm=NEXT, salt=SHA256("D2D"), info="client"/"server")`；
//!   四把密钥以 `salt=SHA256("SecureMessage")`、`info="ENC:2"`/`"SIG:1"` 派生（R15 `NearbyConnection.swift:380-395`，
//!   交叉 R18 `d2d_crypto_ops.cc`）。
//! - 信封 F-20：`SecureMessage{header_and_body=1, signature=2}`、`HeaderAndBody{header=1, body=2}`、
//!   `Header{signature_scheme=1(HMAC_SHA256=1), encryption_scheme=2(AES_256_CBC=2), iv=5, public_metadata=6}`
//!   （R18 `securemessage.proto`）；`public_metadata` = `GcmMetadata{type=1 → DEVICE_TO_DEVICE_MESSAGE(13), version=2 → 1}`
//!   （R18 `securegcm.proto`；R15 `PROTOCOL.md:177` 的"version 恒 1、type 恒 DEVICE_TO_DEVICE_MESSAGE"）。
//! - 载荷 `DeviceToDeviceMessage{message=1, sequence_number=2}`，序号必须递增（R18 `device_to_device_messages.proto`；
//!   R15：首条为 1、双方各自独立计数）。
//!
//! 本模块**不实现**曲线/AES/HMAC 内部：AES-256-CBC 用 `aes`+`cbc`，HMAC 用 `hmac`，HKDF 用 `hkdf`。
//! 序号策略是**严格 +1**（跳号/重放/乱序一律拒），比"递增即可"更严——宁可拒绝也不接受可重放窗口。

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use interop_contract::error::{Error, ErrorCode};
use sha2::{Digest, Sha256};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use cbc::{Decryptor, Encryptor};

use crate::wire;

type Aes256CbcEnc = Encryptor<aes::Aes256>;
type Aes256CbcDec = Decryptor<aes::Aes256>;
type HmacSha256 = Hmac<Sha256>;

/// AES-CBC 的 IV 长度（`Header.iv`）。
pub const IV_BYTES: usize = 16;
/// `GcmMetadata.type` 的 `DEVICE_TO_DEVICE_MESSAGE` 值（F-20）。
pub const GCM_TYPE_DEVICE_TO_DEVICE_MESSAGE: i64 = 13;
/// `GcmMetadata.version` 的常量值（F-20：恒为 1）。
pub const GCM_METADATA_VERSION: i64 = 1;
/// 签名方案 `HMAC_SHA256 = 1`（F-20）。
pub const SIG_SCHEME_HMAC_SHA256: i64 = 1;
/// 加密方案 `AES_256_CBC = 2`（F-20）。
pub const ENC_SCHEME_AES_256_CBC: i64 = 2;
/// D2D 密钥派生用的 salt（F-19：`SHA256("D2D")`）。
const D2D_SALT_INPUT: &[u8] = b"D2D";
/// 四把会话密钥的 salt（F-19：`SHA256("SecureMessage")`）。
const SMSG_SALT_INPUT: &[u8] = b"SecureMessage";

fn integrity(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::IntegrityFailed, why.into()).with_phase("transferring")
}

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("transferring")
}

fn hkdf32(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = [0u8; 32];
    hk.expand(info, &mut out).expect("32 ≤ 255·HashLen");
    out
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

/// 一个方向的两把密钥（加密 + 签名）。
#[derive(Clone)]
pub struct DirectionKeys {
    enc_key: [u8; 32],
    sig_key: [u8; 32],
}

impl std::fmt::Debug for DirectionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DirectionKeys(<keys redacted>)")
    }
}

/// 双方密钥表（Debug 打码；不实现 Serialize）。
#[derive(Debug, Clone)]
pub struct D2DKeySchedule {
    /// 客户端方向的密钥（客户端加密、服务端解密）
    client: DirectionKeys,
    /// 服务端方向的密钥（服务端加密、客户端解密）
    server: DirectionKeys,
}

impl D2DKeySchedule {
    /// 由 UKEY2 的 next secret 派生（F-19）。
    pub fn derive(next_secret: &[u8]) -> Result<Self, Error> {
        if next_secret.is_empty() {
            return Err(invalid("next secret 为空：不能派生会话密钥"));
        }
        let d2d_salt = sha256(D2D_SALT_INPUT);
        let smsg_salt = sha256(SMSG_SALT_INPUT);
        let d2d_client = hkdf32(next_secret, &d2d_salt, b"client");
        let d2d_server = hkdf32(next_secret, &d2d_salt, b"server");
        let derive_dir = |ikm: &[u8; 32]| DirectionKeys {
            enc_key: hkdf32(ikm, &smsg_salt, b"ENC:2"),
            sig_key: hkdf32(ikm, &smsg_salt, b"SIG:1"),
        };
        Ok(Self {
            client: derive_dir(&d2d_client),
            server: derive_dir(&d2d_server),
        })
    }
}

/// 本端角色（决定用哪一组密钥加密/解密）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelRole {
    Client,
    Server,
}

/// 双向 SecureMessage 通道：发送用本端密钥，接收用对端密钥（F-19 的"从服务端 POV"交换）。
pub struct SecureMessageChannel {
    send: DirectionKeys,
    recv: DirectionKeys,
    send_sequence: i32,
    expected_sequence: i32,
}

impl std::fmt::Debug for SecureMessageChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SecureMessageChannel(send_seq={}, expected={})",
            self.send_sequence, self.expected_sequence
        )
    }
}

impl SecureMessageChannel {
    pub fn new(schedule: D2DKeySchedule, role: ChannelRole) -> Self {
        let (send, recv) = match role {
            // 客户端：加密用 client 密钥，解密用 server 密钥
            ChannelRole::Client => (schedule.client, schedule.server),
            // 服务端：反过来
            ChannelRole::Server => (schedule.server, schedule.client),
        };
        Self {
            send,
            recv,
            send_sequence: 1,
            expected_sequence: 1,
        }
    }

    pub fn expected_sequence(&self) -> i32 {
        self.expected_sequence
    }

    pub fn next_send_sequence(&self) -> i32 {
        self.send_sequence
    }

    /// 明文 → SecureMessage 字节（含 HMAC 签名）。
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let mut iv = [0u8; IV_BYTES];
        getrandom::getrandom(&mut iv)
            .map_err(|e| invalid(format!("系统熵不可用（IV）：{e}")))?;

        let d2d = wire::encode_device_to_device(self.send_sequence, plaintext);
        let ct = Aes256CbcEnc::new((&self.send.enc_key).into(), (&iv).into())
            .encrypt_padded_vec_mut::<Pkcs7>(&d2d);

        let header = wire::encode_header(
            SIG_SCHEME_HMAC_SHA256,
            ENC_SCHEME_AES_256_CBC,
            Some(&iv),
            Some(&wire::encode_gcm_metadata(
                GCM_TYPE_DEVICE_TO_DEVICE_MESSAGE,
                GCM_METADATA_VERSION,
            )),
        );
        let header_and_body = wire::encode_header_and_body(&header, &ct);
        let mut mac =
            HmacSha256::new_from_slice(&self.send.sig_key).map_err(|e| invalid(e.to_string()))?;
        mac.update(&header_and_body);
        let signature = mac.finalize().into_bytes().to_vec();

        self.send_sequence += 1;
        Ok(wire::encode_secure_message(&header_and_body, &signature))
    }

    /// SecureMessage 字节 → 明文（先验签名，再解密，最后校验序号）。
    pub fn open(&mut self, frame: &[u8]) -> Result<Vec<u8>, Error> {
        let msg = wire::decode_secure_message(frame)?;
        let hb = wire::decode_header_and_body(&msg.header_and_body)?;
        let header = wire::decode_header(&hb.header)?;

        if header.signature_scheme != SIG_SCHEME_HMAC_SHA256 {
            return Err(invalid(format!(
                "签名方案 {} 不是 HMAC_SHA256（F-20）",
                header.signature_scheme
            )));
        }
        if header.encryption_scheme != ENC_SCHEME_AES_256_CBC {
            return Err(invalid(format!(
                "加密方案 {} 不是 AES_256_CBC（F-20）",
                header.encryption_scheme
            )));
        }

        // 1) 先验 HMAC（覆盖 header_and_body），失败即拒——不做任何解密尝试
        let mut mac =
            HmacSha256::new_from_slice(&self.recv.sig_key).map_err(|e| invalid(e.to_string()))?;
        mac.update(&msg.header_and_body);
        mac.verify_slice(&msg.signature)
            .map_err(|_| integrity("HMAC 校验失败：报文被篡改或密钥不符"))?;

        // 2) 再解密（PKCS7 填充错误按完整性失败处理）
        let iv = header
            .iv
            .as_deref()
            .ok_or_else(|| invalid("Header 缺少 iv"))?;
        if iv.len() != IV_BYTES {
            return Err(invalid(format!("iv 长度 {} 不是 {}", iv.len(), IV_BYTES)));
        }
        let pt = Aes256CbcDec::new((&self.recv.enc_key).into(), iv.into())
            .decrypt_padded_vec_mut::<Pkcs7>(&hb.body)
            .map_err(|_| integrity("AES-256-CBC 解密/填充校验失败"))?;

        // 3) 最后校验序号：必须严格等于期望值（跳号=丢帧、重复=重放，都拒绝）
        let d2d = wire::decode_device_to_device(&pt)?;
        if d2d.sequence_number != self.expected_sequence {
            return Err(integrity(format!(
                "序号 {} 与期望 {} 不符（拒绝重放/乱序）",
                d2d.sequence_number, self.expected_sequence
            )));
        }
        self.expected_sequence += 1;
        Ok(d2d.message)
    }
}

/// 供上层构造负向用例：把已签名的报文改一位（HMAC 必然不符）。
pub fn flip_last_byte(frame: &[u8]) -> Vec<u8> {
    let mut out = frame.to_vec();
    if let Some(last) = out.last_mut() {
        *last ^= 0x01;
    }
    out
}

