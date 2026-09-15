//! CASTV2 信封层（字段表 F-01..F-06）：`CastMessage` 的字段号、required 约束、帧长度与上限。
//!
//! 字节布局逐条对应字段表：`protocol_version`=1、`source_id`=2、`destination_id`=3、`namespace`=4、
//! `payload_type`=5、`payload_utf8`=6、`payload_binary`=7、`continued`=8、`remaining_length`=9。
//! **分块（8/9）不实现**：参考实现里也没有实现路径（F-05），收到即 `unsupported-feature`。
//!
//! 本模块自带一个最小 protobuf 读写子集（varint/长度前缀字段），与 `proto-quickshare` 的同名子集
//! 各自独立——协议 crate 之间不互相依赖，避免把一条协议线的取舍带进另一条。

use interop_contract::error::{Error, ErrorCode};

/// 帧头：4 字节长度前缀（F-04 的 `kHeaderSize = sizeof(uint32_t)`）。
pub const HEADER_BYTES: usize = 4;
/// 正文上限 64 KiB（F-04：`kMaxBodySize = 65536`，参考实现在序列化与反序列化两侧都拒绝）。
pub const MAX_BODY_BYTES: usize = 65_536;
/// 协议版本白名单（F-02：CASTV2_1_0..CASTV2_1_3）。
pub const PROTOCOL_VERSIONS: [u8; 4] = [0, 1, 2, 3];

/// 平台端点 id（F-06）。
pub const SENDER_PLATFORM_ID: &str = "sender-0";
pub const RECEIVER_PLATFORM_ID: &str = "receiver-0";
/// 通配目的地（F-06）。
pub const WILDCARD_DESTINATION: &str = "*";

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn unsupported(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, msg)
}

/// 载荷类型（F-03：STRING=0、BINARY=1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadType {
    String,
    Binary,
}

impl PayloadType {
    fn code(self) -> u64 {
        match self {
            Self::String => 0,
            Self::Binary => 1,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "STRING",
            Self::Binary => "BINARY",
        }
    }
}

/// 载荷（F-03：两个字段二选一）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    Utf8(String),
    Binary(Vec<u8>),
}

impl Payload {
    pub fn payload_type(&self) -> PayloadType {
        match self {
            Self::Utf8(_) => PayloadType::String,
            Self::Binary(_) => PayloadType::Binary,
        }
    }

    pub fn as_utf8(&self) -> Option<&str> {
        match self {
            Self::Utf8(text) => Some(text),
            Self::Binary(_) => None,
        }
    }
}

/// 一条 CASTV2 消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastMessage {
    pub protocol_version: u8,
    pub source_id: String,
    pub destination_id: String,
    pub namespace: String,
    pub payload: Payload,
}

impl CastMessage {
    /// 文本载荷消息（控制面绝大多数消息都是 STRING）。
    pub fn text(
        source_id: impl Into<String>,
        destination_id: impl Into<String>,
        namespace: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version: 0,
            source_id: source_id.into(),
            destination_id: destination_id.into(),
            namespace: namespace.into(),
            payload: Payload::Utf8(payload.into()),
        }
    }

    /// 编码正文（不含 4 字节长度前缀）。
    pub fn encode_body(&self) -> Result<Vec<u8>, Error> {
        if !PROTOCOL_VERSIONS.contains(&self.protocol_version) {
            return Err(invalid(format!(
                "未知 protocol_version {}（F-02 只固定 0..3）",
                self.protocol_version
            )));
        }
        for (field, value) in [
            ("source_id", &self.source_id),
            ("destination_id", &self.destination_id),
            ("namespace", &self.namespace),
        ] {
            if value.is_empty() {
                return Err(invalid(format!("{field} 不得为空（F-01 的 required 字段）")));
            }
        }
        let mut out = Vec::new();
        write_varint_field(&mut out, 1, u64::from(self.protocol_version));
        write_bytes_field(&mut out, 2, self.source_id.as_bytes());
        write_bytes_field(&mut out, 3, self.destination_id.as_bytes());
        write_bytes_field(&mut out, 4, self.namespace.as_bytes());
        write_varint_field(&mut out, 5, self.payload.payload_type().code());
        match &self.payload {
            Payload::Utf8(text) => write_bytes_field(&mut out, 6, text.as_bytes()),
            Payload::Binary(bytes) => write_bytes_field(&mut out, 7, bytes),
        }
        if out.len() > MAX_BODY_BYTES {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("正文超过 {MAX_BODY_BYTES} 字节上限（F-04，序列化前拒绝）"),
            ));
        }
        Ok(out)
    }

    /// 解码正文。
    pub fn decode_body(body: &[u8]) -> Result<Self, Error> {
        if body.len() > MAX_BODY_BYTES {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("正文超过 {MAX_BODY_BYTES} 字节上限（F-04，解析前拒绝）"),
            ));
        }
        let mut reader = Reader::new(body);
        let mut protocol_version: Option<u64> = None;
        let mut source_id: Option<String> = None;
        let mut destination_id: Option<String> = None;
        let mut namespace: Option<String> = None;
        let mut payload_type: Option<u64> = None;
        let mut payload_utf8: Option<String> = None;
        let mut payload_binary: Option<Vec<u8>> = None;

        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, Wire::Varint) => protocol_version = Some(reader.varint()?),
                (2, Wire::Bytes) => source_id = Some(reader.string()?),
                (3, Wire::Bytes) => destination_id = Some(reader.string()?),
                (4, Wire::Bytes) => namespace = Some(reader.string()?),
                (5, Wire::Varint) => payload_type = Some(reader.varint()?),
                (6, Wire::Bytes) => payload_utf8 = Some(reader.string()?),
                (7, Wire::Bytes) => payload_binary = Some(reader.bytes()?.to_vec()),
                (8, Wire::Varint) => {
                    // F-05：分块不实现——continued 一位出现就拒绝，不尝试拼接。
                    let continued = reader.varint()?;
                    if continued != 0 {
                        return Err(unsupported(
                            "收到 continued = true：CASTV2 分块（F-05）未实现，明确拒绝",
                        ));
                    }
                }
                (9, Wire::Varint) => {
                    // F-05：remaining_length 只在分块里出现，同样拒绝。
                    return Err(unsupported(
                        "收到 remaining_length 字段：CASTV2 分块（F-05）未实现，明确拒绝",
                    ));
                }
                _ => reader.skip(wire)?,
            }
        }

        let protocol_version = protocol_version
            .ok_or_else(|| invalid("缺 protocol_version（F-01 的 required 字段）"))?;
        if protocol_version > u64::from(u8::MAX) {
            return Err(invalid("protocol_version 超出字节范围"));
        }
        let protocol_version = protocol_version as u8;
        if !PROTOCOL_VERSIONS.contains(&protocol_version) {
            return Err(invalid(format!(
                "未知 protocol_version {protocol_version}（F-02 只固定 0..3）"
            )));
        }
        let source_id = source_id.ok_or_else(|| invalid("缺 source_id（required）"))?;
        let destination_id = destination_id.ok_or_else(|| invalid("缺 destination_id（required）"))?;
        let namespace = namespace.ok_or_else(|| invalid("缺 namespace（required）"))?;
        let payload_type = payload_type.ok_or_else(|| invalid("缺 payload_type（required）"))?;
        if source_id.is_empty() || destination_id.is_empty() || namespace.is_empty() {
            return Err(invalid("required 字段不得为空"));
        }
        let payload = match (payload_type, payload_utf8, payload_binary) {
            (0, Some(text), None) => Payload::Utf8(text),
            (1, None, Some(bytes)) => Payload::Binary(bytes),
            (0, Some(_), Some(_)) | (1, Some(_), Some(_)) => {
                return Err(invalid("STRING 与 BINARY 载荷同时出现（F-03：二选一）"))
            }
            (0, None, None) => return Err(invalid("payload_type = STRING 但没有 payload_utf8")),
            (1, None, None) => return Err(invalid("payload_type = BINARY 但没有 payload_binary")),
            (0, None, Some(_)) => return Err(invalid("payload_type = STRING 却带了 payload_binary")),
            (1, Some(_), None) => return Err(invalid("payload_type = BINARY 却带了 payload_utf8")),
            (other, _, _) => return Err(invalid(format!("未知 payload_type {other}（F-03 只固定 0/1）"))),
        };
        Ok(Self {
            protocol_version,
            source_id,
            destination_id,
            namespace,
            payload,
        })
    }

    /// 编码整帧（4 字节大端长度前缀 + 正文）。
    pub fn encode_frame(&self) -> Result<Vec<u8>, Error> {
        let body = self.encode_body()?;
        let mut out = Vec::with_capacity(HEADER_BYTES + body.len());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// 解码整帧，返回 `(消息, consumed)`。
    pub fn decode_frame(frame: &[u8]) -> Result<(Self, usize), Error> {
        if frame.len() < HEADER_BYTES {
            return Err(invalid("帧不足 4 字节长度前缀"));
        }
        let declared = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        if declared > MAX_BODY_BYTES {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("长度前缀声明 {declared} 字节，超过 {MAX_BODY_BYTES}（F-04，分配前拒绝）"),
            ));
        }
        if frame.len() < HEADER_BYTES + declared {
            return Err(invalid(format!(
                "长度前缀声明 {declared} 字节但只剩 {} 字节（截断）",
                frame.len() - HEADER_BYTES
            )));
        }
        let body = &frame[HEADER_BYTES..HEADER_BYTES + declared];
        let message = Self::decode_body(body)?;
        Ok((message, HEADER_BYTES + declared))
    }
}

// ---------------------------------------------------------------- 最小 protobuf 子集

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire {
    Varint,
    Fixed64,
    Bytes,
    Fixed32,
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn varint(&mut self) -> Result<u64, Error> {
        let mut value: u64 = 0;
        let mut shift = 0;
        loop {
            let byte = *self
                .buf
                .get(self.pos)
                .ok_or_else(|| invalid("varint 截断"))?;
            self.pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift >= 64 {
                return Err(invalid("varint 超过 64 位"));
            }
        }
    }

    fn next_field(&mut self) -> Result<Option<(u32, Wire)>, Error> {
        if self.pos >= self.buf.len() {
            return Ok(None);
        }
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        let wire = match key & 0x7 {
            0 => Wire::Varint,
            1 => Wire::Fixed64,
            2 => Wire::Bytes,
            5 => Wire::Fixed32,
            other => return Err(invalid(format!("未知 protobuf wire type {other}"))),
        };
        if field == 0 {
            return Err(invalid("field number 0 非法"));
        }
        Ok(Some((field, wire)))
    }

    fn bytes(&mut self) -> Result<&'a [u8], Error> {
        let len = self.varint()? as usize;
        let end = self
            .pos
            .checked_add(len)
            .filter(|end| *end <= self.buf.len())
            .ok_or_else(|| invalid("长度前缀字段截断"))?;
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn string(&mut self) -> Result<String, Error> {
        let raw = self.bytes()?;
        String::from_utf8(raw.to_vec()).map_err(|_| invalid("字符串字段不是合法 UTF-8"))
    }

    fn skip(&mut self, wire: Wire) -> Result<(), Error> {
        match wire {
            Wire::Varint => {
                self.varint()?;
            }
            Wire::Fixed64 => self.advance(8)?,
            Wire::Fixed32 => self.advance(4)?,
            Wire::Bytes => {
                self.bytes()?;
            }
        }
        Ok(())
    }

    fn advance(&mut self, n: usize) -> Result<(), Error> {
        self.pos = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.buf.len())
            .ok_or_else(|| invalid("字段截断"))?;
        Ok(())
    }
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn write_varint_field(out: &mut Vec<u8>, field: u32, value: u64) {
    write_varint(out, u64::from(field) << 3);
    write_varint(out, value);
}

fn write_bytes_field(out: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    write_varint(out, (u64::from(field) << 3) | 2);
    write_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}
