//! UKEY2 消息编解码（plans/02-files.md §T20）。
//!
//! 字段号不是猜的：`F-06..F-11` 行给出 R18 规范/proto 的来源（`ukey.proto`、`securemessage.proto`、
//! README §Message Framing）。这里只用**极小的 protobuf 子集**（varint + length-delimited），
//! 因为 UKEY2 的消息集合是固定的几条；不引入 protobuf 代码生成，也就没有第三方表达。
//!
//! 解析规矩（T20-03）：长度先检查后分配；varint 最多 10 字节；截断一律 `invalid-frame`；
//! 未知字段按 proto2 语义跳过（但不接受未知类型码）。

use interop_contract::error::{Error, ErrorCode};

/// 单个 length-delimited 字段的上限（本仓保守值；防"巨大长度字段"拖垮内存）。
pub const MAX_FIELD_BYTES: usize = 1024 * 1024;

/// `Ukey2Message.Type`（F-06）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ukey2MessageType {
    Alert,
    ClientInit,
    ServerInit,
    ClientFinish,
}

impl Ukey2MessageType {
    pub fn as_i32(self) -> i32 {
        match self {
            Self::Alert => 1,
            Self::ClientInit => 2,
            Self::ServerInit => 3,
            Self::ClientFinish => 4,
        }
    }

    pub fn from_i32(v: i32) -> Result<Self, Error> {
        match v {
            1 => Ok(Self::Alert),
            2 => Ok(Self::ClientInit),
            3 => Ok(Self::ServerInit),
            4 => Ok(Self::ClientFinish),
            other => Err(invalid(format!("未知 Ukey2Message.Type {other}（F-06 只定义 0..4）"))),
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Alert => "alert",
            Self::ClientInit => "client-init",
            Self::ServerInit => "server-init",
            Self::ClientFinish => "client-finish",
        }
    }
}

/// `Ukey2Alert.AlertType`（F-07）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ukey2AlertType {
    BadMessage,
    BadMessageType,
    IncorrectMessage,
    BadMessageData,
    BadVersion,
    BadRandom,
    BadHandshakeCipher,
    BadNextProtocol,
    BadPublicKey,
    InternalError,
}

impl Ukey2AlertType {
    pub fn as_i32(self) -> i32 {
        match self {
            Self::BadMessage => 1,
            Self::BadMessageType => 2,
            Self::IncorrectMessage => 3,
            Self::BadMessageData => 4,
            Self::BadVersion => 100,
            Self::BadRandom => 101,
            Self::BadHandshakeCipher => 102,
            Self::BadNextProtocol => 103,
            Self::BadPublicKey => 104,
            Self::InternalError => 200,
        }
    }
}

/// 握手 cipher 枚举（F-13）。
pub const HANDSHAKE_CIPHER_P256_SHA512: i32 = 100;
pub const HANDSHAKE_CIPHER_CURVE25519_SHA512: i32 = 200;

/// 公钥类型（`securemessage.proto`：`PublicKeyType.EC_P256 = 1`）。
pub const PUBLIC_KEY_TYPE_EC_P256: i32 = 1;

// ---------------------------------------------------------------- protobuf 最小子集

pub(crate) fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("negotiating")
}

pub(crate) fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub(crate) fn write_varint_field(out: &mut Vec<u8>, field: u32, v: u64) {
    write_varint(out, (field as u64) << 3);
    write_varint(out, v);
}

pub(crate) fn write_bytes_field(out: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    write_varint(out, ((field as u64) << 3) | 2);
    write_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    /// 跳过的未知字段数（proto2 语义：忽略；记录下来便于诊断）
    pub skipped_unknown: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            pos: 0,
            skipped_unknown: 0,
        }
    }

    pub(crate) fn eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    pub(crate) fn byte(&mut self) -> Result<u8, Error> {
        let b = *self
            .buf
            .get(self.pos)
            .ok_or_else(|| invalid("protobuf 读取越界（截断）"))?;
        self.pos += 1;
        Ok(b)
    }

    pub(crate) fn varint(&mut self) -> Result<u64, Error> {
        let mut result: u64 = 0;
        for shift in 0..10u32 {
            let b = self.byte()?;
            if shift == 9 && b > 1 {
                return Err(invalid("varint 溢出（第 10 字节 > 1）"));
            }
            result |= ((b & 0x7F) as u64) << (7 * shift);
            if b & 0x80 == 0 {
                return Ok(result);
            }
        }
        Err(invalid("varint 超过 10 字节上限"))
    }

    /// 读 length-delimited 字段：**先查长度再切片**（不预分配）。
    pub(crate) fn bytes(&mut self) -> Result<&'a [u8], Error> {
        let len = self.varint()?;
        if len > MAX_FIELD_BYTES as u64 {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("protobuf 字段声明 {len} 字节，超过上限 {MAX_FIELD_BYTES}（分配前拒绝）"),
            )
            .with_phase("negotiating"));
        }
        let len = len as usize;
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| invalid("长度溢出"))?;
        if end > self.buf.len() {
            return Err(invalid(format!(
                "protobuf 字段截断：声明 {len} 字节，剩余 {}",
                self.buf.len() - self.pos
            )));
        }
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub(crate) fn skip(&mut self, wire_type: u64) -> Result<(), Error> {
        match wire_type {
            0 => {
                self.varint()?;
            }
            1 => {
                self.pos = self.pos.checked_add(8).filter(|p| *p <= self.buf.len())
                    .ok_or_else(|| invalid("fixed64 截断"))?;
            }
            2 => {
                self.bytes()?;
            }
            5 => {
                self.pos = self.pos.checked_add(4).filter(|p| *p <= self.buf.len())
                    .ok_or_else(|| invalid("fixed32 截断"))?;
            }
            other => return Err(invalid(format!("不支持的 protobuf wire type {other}"))),
        }
        self.skipped_unknown += 1;
        Ok(())
    }

    /// 下一条字段的 (field_number, wire_type)。
    pub(crate) fn next_key(&mut self) -> Result<(u32, u64), Error> {
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        if field == 0 {
            return Err(invalid("字段号 0 非法"));
        }
        Ok((field, key & 0x7))
    }
}

// ---------------------------------------------------------------- UKEY2 外层消息

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ukey2Message {
    pub message_type: Ukey2MessageType,
    pub message_data: Vec<u8>,
}

/// 编码 `Ukey2Message{ message_type=1, message_data=2 }`（F-06）。
pub fn encode_ukey2_message(message_type: Ukey2MessageType, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    write_varint_field(&mut out, 1, message_type.as_i32() as u64);
    write_bytes_field(&mut out, 2, data);
    out
}

pub fn decode_ukey2_message(buf: &[u8]) -> Result<Ukey2Message, Error> {
    let mut r = Reader::new(buf);
    let mut message_type = None;
    let mut data: Option<Vec<u8>> = None;
    while !r.eof() {
        let (field, wire) = r.next_key()?;
        match (field, wire) {
            (1, 0) => message_type = Some(Ukey2MessageType::from_i32(r.varint()? as i32)?),
            (2, 2) => data = Some(r.bytes()?.to_vec()),
            _ => r.skip(wire)?,
        }
    }
    Ok(Ukey2Message {
        message_type: message_type.ok_or_else(|| invalid("Ukey2Message 缺少 message_type"))?,
        message_data: data.unwrap_or_default(),
    })
}

/// 编码 `Ukey2Alert{ type=1, error_message=2 }`（F-07）。
pub fn encode_alert(alert_type: Ukey2AlertType, message: &str) -> Vec<u8> {
    let mut inner = Vec::new();
    write_varint_field(&mut inner, 1, alert_type.as_i32() as u64);
    write_bytes_field(&mut inner, 2, message.as_bytes());
    encode_ukey2_message(Ukey2MessageType::Alert, &inner)
}

// ---------------------------------------------------------------- ClientInit（F-08）

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInit {
    pub version: i32,
    pub random: Vec<u8>,
    /// (handshake_cipher, commitment)，顺序即客户端偏好
    pub cipher_commitments: Vec<(i32, Vec<u8>)>,
    pub next_protocol: String,
}

pub fn encode_client_init(init: &ClientInit) -> Vec<u8> {
    let mut inner = Vec::new();
    write_varint_field(&mut inner, 1, init.version as u64);
    write_bytes_field(&mut inner, 2, &init.random);
    for (cipher, commitment) in &init.cipher_commitments {
        let mut c = Vec::new();
        write_varint_field(&mut c, 1, *cipher as u64);
        write_bytes_field(&mut c, 2, commitment);
        write_bytes_field(&mut inner, 3, &c);
    }
    write_bytes_field(&mut inner, 4, init.next_protocol.as_bytes());
    encode_ukey2_message(Ukey2MessageType::ClientInit, &inner)
}

pub fn decode_client_init(data: &[u8]) -> Result<ClientInit, Error> {
    let mut r = Reader::new(data);
    let mut version = None;
    let mut random = None;
    let mut commitments = Vec::new();
    let mut next_protocol = None;
    while !r.eof() {
        let (field, wire) = r.next_key()?;
        match (field, wire) {
            (1, 0) => version = Some(r.varint()? as i32),
            (2, 2) => random = Some(r.bytes()?.to_vec()),
            (3, 2) => {
                let inner = r.bytes()?;
                let mut ir = Reader::new(inner);
                let mut cipher = None;
                let mut commitment = None;
                while !ir.eof() {
                    let (f, w) = ir.next_key()?;
                    match (f, w) {
                        (1, 0) => cipher = Some(ir.varint()? as i32),
                        (2, 2) => commitment = Some(ir.bytes()?.to_vec()),
                        _ => ir.skip(w)?,
                    }
                }
                commitments.push((
                    cipher.ok_or_else(|| invalid("CipherCommitment 缺 handshake_cipher"))?,
                    commitment.ok_or_else(|| invalid("CipherCommitment 缺 commitment"))?,
                ));
            }
            (4, 2) => {
                let raw = r.bytes()?;
                next_protocol = Some(
                    std::str::from_utf8(raw)
                        .map_err(|_| invalid("next_protocol 不是合法 UTF-8"))?
                        .to_string(),
                );
            }
            _ => r.skip(wire)?,
        }
    }
    Ok(ClientInit {
        version: version.ok_or_else(|| invalid("ClientInit 缺 version"))?,
        random: random.ok_or_else(|| invalid("ClientInit 缺 random"))?,
        cipher_commitments: commitments,
        next_protocol: next_protocol.unwrap_or_default(),
    })
}

// ---------------------------------------------------------------- ServerInit（F-09）

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInit {
    pub version: i32,
    pub random: Vec<u8>,
    pub handshake_cipher: i32,
    pub public_key: Vec<u8>,
}

pub fn encode_server_init(init: &ServerInit) -> Vec<u8> {
    let mut inner = Vec::new();
    write_varint_field(&mut inner, 1, init.version as u64);
    write_bytes_field(&mut inner, 2, &init.random);
    write_varint_field(&mut inner, 3, init.handshake_cipher as u64);
    write_bytes_field(&mut inner, 4, &init.public_key);
    encode_ukey2_message(Ukey2MessageType::ServerInit, &inner)
}

pub fn decode_server_init(data: &[u8]) -> Result<ServerInit, Error> {
    let mut r = Reader::new(data);
    let mut version = None;
    let mut random = None;
    let mut cipher = None;
    let mut public_key = None;
    while !r.eof() {
        let (field, wire) = r.next_key()?;
        match (field, wire) {
            (1, 0) => version = Some(r.varint()? as i32),
            (2, 2) => random = Some(r.bytes()?.to_vec()),
            (3, 0) => cipher = Some(r.varint()? as i32),
            (4, 2) => public_key = Some(r.bytes()?.to_vec()),
            _ => r.skip(wire)?,
        }
    }
    Ok(ServerInit {
        version: version.ok_or_else(|| invalid("ServerInit 缺 version"))?,
        random: random.ok_or_else(|| invalid("ServerInit 缺 random"))?,
        handshake_cipher: cipher.ok_or_else(|| invalid("ServerInit 缺 handshake_cipher"))?,
        public_key: public_key.ok_or_else(|| invalid("ServerInit 缺 public_key"))?,
    })
}

// ---------------------------------------------------------------- ClientFinished（F-10）

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientFinished {
    pub public_key: Vec<u8>,
}

pub fn encode_client_finished(f: &ClientFinished) -> Vec<u8> {
    let mut inner = Vec::new();
    write_bytes_field(&mut inner, 1, &f.public_key);
    encode_ukey2_message(Ukey2MessageType::ClientFinish, &inner)
}

pub fn decode_client_finished(data: &[u8]) -> Result<ClientFinished, Error> {
    let mut r = Reader::new(data);
    let mut public_key = None;
    while !r.eof() {
        let (field, wire) = r.next_key()?;
        match (field, wire) {
            (1, 2) => public_key = Some(r.bytes()?.to_vec()),
            _ => r.skip(wire)?,
        }
    }
    Ok(ClientFinished {
        public_key: public_key.ok_or_else(|| invalid("ClientFinished 缺 public_key"))?,
    })
}

// ---------------------------------------------------------------- GenericPublicKey

/// `GenericPublicKey{ type=1, ec_p256_public_key=2 }`，其中 `EcP256PublicKey{ x=1, y=2 }`
/// （`securemessage.proto`；x/y 为大端两补码字节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericPublicKey {
    pub key_type: i32,
    pub x: Vec<u8>,
    pub y: Vec<u8>,
}

pub fn encode_generic_public_key(key: &GenericPublicKey) -> Vec<u8> {
    let mut inner = Vec::new();
    write_varint_field(&mut inner, 1, key.key_type as u64);
    let mut ec = Vec::new();
    write_bytes_field(&mut ec, 1, &key.x);
    write_bytes_field(&mut ec, 2, &key.y);
    write_bytes_field(&mut inner, 2, &ec);
    inner
}

pub fn decode_generic_public_key(data: &[u8]) -> Result<GenericPublicKey, Error> {
    let mut r = Reader::new(data);
    let mut key_type = None;
    let mut coords: Option<(Vec<u8>, Vec<u8>)> = None;
    while !r.eof() {
        let (field, wire) = r.next_key()?;
        match (field, wire) {
            (1, 0) => key_type = Some(r.varint()? as i32),
            (2, 2) => {
                let mut ir = Reader::new(r.bytes()?);
                let mut x = None;
                let mut y = None;
                while !ir.eof() {
                    let (f, w) = ir.next_key()?;
                    match (f, w) {
                        (1, 2) => x = Some(ir.bytes()?.to_vec()),
                        (2, 2) => y = Some(ir.bytes()?.to_vec()),
                        _ => ir.skip(w)?,
                    }
                }
                coords = Some((
                    x.ok_or_else(|| invalid("EcP256PublicKey 缺 x"))?,
                    y.ok_or_else(|| invalid("EcP256PublicKey 缺 y"))?,
                ));
            }
            _ => r.skip(wire)?,
        }
    }
    let key_type = key_type.ok_or_else(|| invalid("GenericPublicKey 缺 type"))?;
    let (x, y) = coords.ok_or_else(|| invalid("GenericPublicKey 缺 ec_p256_public_key"))?;
    Ok(GenericPublicKey { key_type, x, y })
}

// ---------------------------------------------------------------- 测试辅助（负向用例）

/// 把 ClientFinish 的 public_key 换成给定字节（用于 commitment 不符的负向用例）。
/// 只用于测试与语料构造：`specs-reviewed/f02` 的 F-11 行。
pub fn tamper_client_finished_public_key(frame: &[u8], replacement: &[u8]) -> Vec<u8> {
    let message = decode_ukey2_message(frame).expect("负向用例输入必须是合法 Ukey2Message");
    let mut finished = decode_client_finished(&message.message_data).expect("合法 ClientFinished");
    let tampered = GenericPublicKey {
        key_type: PUBLIC_KEY_TYPE_EC_P256,
        x: replacement.to_vec(),
        y: replacement.to_vec(),
    };
    finished.public_key = encode_generic_public_key(&tampered);
    encode_client_finished(&finished)
}

/// 重建 ClientInit 的某个字段（其余保持不变），用于逐项校验负向用例。
pub fn rebuild_client_init(
    frame: &[u8],
    random: Option<Vec<u8>>,
    version: Option<i32>,
    commitments: Option<Vec<(i32, Vec<u8>)>>,
    next_protocol: Option<String>,
) -> Vec<u8> {
    let message = decode_ukey2_message(frame).expect("合法 Ukey2Message");
    let mut init = decode_client_init(&message.message_data).expect("合法 ClientInit");
    if let Some(r) = random {
        init.random = r;
    }
    if let Some(v) = version {
        init.version = v;
    }
    if let Some(c) = commitments {
        init.cipher_commitments = c;
    }
    if let Some(p) = next_protocol {
        init.next_protocol = p;
    }
    encode_client_init(&init)
}

/// 只改外层 message_type（用于"类型不符"负向用例）。
pub fn with_message_type(frame: &[u8], message_type: Ukey2MessageType) -> Vec<u8> {
    let message = decode_ukey2_message(frame).expect("合法 Ukey2Message");
    encode_ukey2_message(message_type, &message.message_data)
}

// ---------------------------------------------------------------- SecureMessage（F-19/F-20）
//
// 字段号来源：R18 `securemessage.proto`（`SecureMessage{header_and_body=1, signature=2}`、
// `HeaderAndBody{header=1, body=2}`、`Header{signature_scheme=1, encryption_scheme=2,
// verification_key_id=3, decryption_key_id=4, iv=5, public_metadata=6}`）、
// R18 `securegcm.proto`（`GcmMetadata{type=1, version=2}`，`DEVICE_TO_DEVICE_MESSAGE=13`）、
// R18 `device_to_device_messages.proto`（`DeviceToDeviceMessage{message=1, sequence_number=2}`）。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecureMessage {
    pub header_and_body: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderAndBody {
    pub header: Vec<u8>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub signature_scheme: i64,
    pub encryption_scheme: i64,
    pub verification_key_id: Option<Vec<u8>>,
    pub decryption_key_id: Option<Vec<u8>>,
    pub iv: Option<Vec<u8>>,
    pub public_metadata: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceToDeviceMessage {
    pub message: Vec<u8>,
    pub sequence_number: i32,
}

pub fn encode_secure_message(header_and_body: &[u8], signature: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(header_and_body.len() + signature.len() + 8);
    write_bytes_field(&mut out, 1, header_and_body);
    write_bytes_field(&mut out, 2, signature);
    out
}

pub fn decode_secure_message(buf: &[u8]) -> Result<SecureMessage, Error> {
    let mut r = Reader::new(buf);
    let mut hab = None;
    let mut sig = None;
    while !r.eof() {
        let (field, wire_type) = r.next_key()?;
        match (field, wire_type) {
            (1, 2) => hab = Some(r.bytes()?.to_vec()),
            (2, 2) => sig = Some(r.bytes()?.to_vec()),
            _ => r.skip(wire_type)?,
        }
    }
    Ok(SecureMessage {
        header_and_body: hab.ok_or_else(|| invalid("SecureMessage 缺 header_and_body"))?,
        signature: sig.ok_or_else(|| invalid("SecureMessage 缺 signature"))?,
    })
}

pub fn encode_header_and_body(header: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(header.len() + body.len() + 8);
    write_bytes_field(&mut out, 1, header);
    write_bytes_field(&mut out, 2, body);
    out
}

pub fn decode_header_and_body(buf: &[u8]) -> Result<HeaderAndBody, Error> {
    let mut r = Reader::new(buf);
    let mut header = None;
    let mut body = None;
    while !r.eof() {
        let (field, wire_type) = r.next_key()?;
        match (field, wire_type) {
            (1, 2) => header = Some(r.bytes()?.to_vec()),
            (2, 2) => body = Some(r.bytes()?.to_vec()),
            _ => r.skip(wire_type)?,
        }
    }
    Ok(HeaderAndBody {
        header: header.ok_or_else(|| invalid("HeaderAndBody 缺 header"))?,
        body: body.ok_or_else(|| invalid("HeaderAndBody 缺 body"))?,
    })
}

pub fn encode_header(
    signature_scheme: i64,
    encryption_scheme: i64,
    iv: Option<&[u8]>,
    public_metadata: Option<&[u8]>,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint_field(&mut out, 1, signature_scheme as u64);
    write_varint_field(&mut out, 2, encryption_scheme as u64);
    if let Some(iv) = iv {
        write_bytes_field(&mut out, 5, iv);
    }
    if let Some(md) = public_metadata {
        write_bytes_field(&mut out, 6, md);
    }
    out
}

pub fn decode_header(buf: &[u8]) -> Result<Header, Error> {
    let mut r = Reader::new(buf);
    let mut h = Header {
        signature_scheme: 0,
        encryption_scheme: 0,
        verification_key_id: None,
        decryption_key_id: None,
        iv: None,
        public_metadata: None,
    };
    while !r.eof() {
        let (field, wire_type) = r.next_key()?;
        match (field, wire_type) {
            (1, 0) => h.signature_scheme = r.varint()? as i64,
            (2, 0) => h.encryption_scheme = r.varint()? as i64,
            (3, 2) => h.verification_key_id = Some(r.bytes()?.to_vec()),
            (4, 2) => h.decryption_key_id = Some(r.bytes()?.to_vec()),
            (5, 2) => h.iv = Some(r.bytes()?.to_vec()),
            (6, 2) => h.public_metadata = Some(r.bytes()?.to_vec()),
            _ => r.skip(wire_type)?,
        }
    }
    Ok(h)
}

/// `GcmMetadata{ type=1, version=2 }`（F-20：D2D 消息用 type=13、version=1）。
pub fn encode_gcm_metadata(metadata_type: i64, version: i64) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint_field(&mut out, 1, metadata_type as u64);
    write_varint_field(&mut out, 2, version as u64);
    out
}

pub fn encode_device_to_device(sequence_number: i32, message: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    write_bytes_field(&mut out, 1, message);
    write_varint_field(&mut out, 2, sequence_number as u64);
    out
}

pub fn decode_device_to_device(buf: &[u8]) -> Result<DeviceToDeviceMessage, Error> {
    let mut r = Reader::new(buf);
    let mut message = None;
    let mut sequence = None;
    while !r.eof() {
        let (field, wire_type) = r.next_key()?;
        match (field, wire_type) {
            (1, 2) => message = Some(r.bytes()?.to_vec()),
            (2, 0) => sequence = Some(r.varint()? as i32),
            _ => r.skip(wire_type)?,
        }
    }
    Ok(DeviceToDeviceMessage {
        message: message.unwrap_or_default(),
        sequence_number: sequence.ok_or_else(|| invalid("DeviceToDeviceMessage 缺 sequence_number"))?,
    })
}
