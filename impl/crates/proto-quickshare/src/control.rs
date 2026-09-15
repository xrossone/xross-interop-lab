//! keep-alive 与 paired-key 控制帧（字段行 F-30..F-33；T21+ 的 headless 部分）。
//!
//! **分层是这里的核心事实（F-30）**：两层用**各自的** `V1Frame.FrameType` 编号，同号不同义。
//!
//! ```text
//! 外层（Nearby Connections 层，R17 offline_wire_formats.proto:45,61）
//!   OfflineFrame{v1 = V1Frame{type = 5 (KEEP_ALIVE), keep_alive = 6}}          ← keep-alive 直接走外层
//!   OfflineFrame{v1 = V1Frame{type = 3 (PAYLOAD_TRANSFER), payload_transfer = 4}}
//!        └─ PayloadTransferFrame{header{kind = BYTES}, chunk{body = <内层帧字节>}}
//!              内层（Nearby Share 层，R15 wire_format.proto:189-212）
//!              OfflineFrame{v1 = V1Frame{type = 3 (PAIRED_KEY_ENCRYPTION), paired_key_encryption = 4}}
//!              OfflineFrame{v1 = V1Frame{type = 4 (PAIRED_KEY_RESULT),     paired_key_result = 5}}
//! ```
//!
//! 依据不是猜的：R15 `NearbyConnection.swift:207-226` 把内层 `Sharing_Nearby_Frame` 序列化后交给
//! `sendBytesPayload`，而后者构造的正是外层的 `PayloadTransferFrame{packetType=data, header.type=bytes}`；
//! PROTOCOL.md:204-206 也写明 paired-key 帧"包在 payload 层里"。
//!
//! **本模块只做帧与状态机**（F-32）：paired-key 的材料在参考实现里是随机字节、且明说内容要跟 Google
//! 服务器说话——不可离线推导。因此材料一律由调用方提供（本层不含任何密码学原语或随机源，lab gate 机器检查），
//! 也**不据 `SUCCESS` 免掉 4 位确认码**：本仓没有配对存储，默认策略仍是让用户核对。
//!
//! keep-alive 的 10 秒是**来源取值**（F-31，PROTOCOL.md:228-230），超时阈值是本仓策略；
//! F-40 补充：keep-alive 参数在参考实现里是**握手协商字段**（`keep_alive_interval_millis=8`/
//! `keep_alive_timeout_millis=9`，proto 无默认值）——本仓不实现连接握手，故只做**校验 + 回退**。
//!
//! 本模块另含 T22headless 的两组帧（F-34..F-39）：
//! - [`DisconnectionFrame`]：外层 `DISCONNECTION(6)`/字段 7；**保留字段存在性**（R17 显式写两个 bool，
//!   NearDrop 发空正文，两者字节不同——F-36），并按 F-35 的三路规则给出 [`DisconnectAction`]。
//! - [`encode_payload_ack`]/[`decode_payload_ack`]：`packet_type=PAYLOAD_ACK(3)`，只带
//!   `payload_header{id, total_size=-1}`；[`should_send_payload_ack`] 实现"只有非 BYTES 的末块才 ack"
//!   的门槛，[`PayloadAckTracker`] 实现发送侧的三分支（未知/本端 incoming 一律忽略）。

use crate::payload::{
    decode_payload_transfer_frame, encode_payload_transfer_frame, PacketType, PayloadChunk,
    PayloadHeader, PayloadKind, PayloadTransferFrame,
};
use crate::wire::{self, Reader};
use interop_contract::error::{Error, ErrorCode};

// ---------------------------------------------------------------- 层号常量（F-30）

/// 外层：`V1Frame.FrameType.KEEP_ALIVE`（R17 `offline_wire_formats.proto:45`）。
pub const OUTER_TYPE_KEEP_ALIVE: i64 = 5;
/// 外层：`V1Frame.keep_alive` 字段号（R17 `:61`）。
pub const OUTER_FIELD_KEEP_ALIVE: u32 = 6;
/// 内层：`V1Frame.FrameType.PAIRED_KEY_ENCRYPTION`（R15 `wire_format.proto:193`）。
pub const INNER_TYPE_PAIRED_KEY_ENCRYPTION: i64 = 3;
/// 内层：`V1Frame.FrameType.PAIRED_KEY_RESULT`（R15 `wire_format.proto:194`）。
pub const INNER_TYPE_PAIRED_KEY_RESULT: i64 = 4;
/// 内层：`V1Frame.paired_key_encryption` 字段号（R15 `:207`）。
pub const INNER_FIELD_PAIRED_KEY_ENCRYPTION: u32 = 4;
/// 内层：`V1Frame.paired_key_result` 字段号（R15 `:208`）。
pub const INNER_FIELD_PAIRED_KEY_RESULT: u32 = 5;
/// 外层：`V1Frame.FrameType.DISCONNECTION`（R17 `offline_wire_formats.proto:46`）。
pub const OUTER_TYPE_DISCONNECTION: i64 = 6;
/// 外层：`V1Frame.disconnection` 字段号（R17 `:62`）。
pub const OUTER_FIELD_DISCONNECTION: u32 = 7;
/// `PayloadHeader.total_size` 的"大小未知"取值（protobuf `int64 -1`，R17 `internal_payload.h:39`）。
pub const INDETERMINATE_TOTAL_SIZE: u64 = u64::MAX;

/// keep-alive 发送间隔（F-31：PROTOCOL.md:228-230 的取值，**不是规范常量**）。
pub const KEEPALIVE_INTERVAL_MS: u64 = 10_000;
/// keep-alive 超时阈值（F-31：来源只说"过一段时间会断开"，没有数值 → **本仓策略**）。
pub const KEEPALIVE_TIMEOUT_MS: u64 = 30_000;
/// paired-key 单个字段的上限（**本仓策略**；参考实现用 6 B 与 72 B）。
pub const MAX_PAIRED_KEY_BYTES: usize = 512;

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn unsupported(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, msg)
}

// ---------------------------------------------------------------- keep-alive（F-31）

/// `KeepAliveFrame{ack=1(bool), seq_num=2(uint32)}`（R17 `offline_wire_formats.proto:444-449`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeepAliveFrame {
    /// 该帧是否是上一条 KEEP_ALIVE 的应答（R15 `NearbyConnection.swift:297-301` 收到即回 ack=true）。
    pub ack: bool,
    /// 发送方自己的序号（可选字段；参考实现不设置）。
    pub seq_num: u32,
}

impl KeepAliveFrame {
    pub fn new(ack: bool, seq_num: u32) -> Self {
        Self { ack, seq_num }
    }

    /// 仅 `KeepAliveFrame` 正文。
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.ack {
            wire::write_varint_field(&mut out, 1, 1);
        }
        wire::write_varint_field(&mut out, 2, u64::from(self.seq_num));
        out
    }

    pub fn decode(buf: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(buf);
        let mut frame = Self::default();
        while !reader.eof() {
            let (field, wire_type) = reader.next_key()?;
            match (field, wire_type) {
                (1, 0) => frame.ack = reader.varint()? != 0,
                (2, 0) => {
                    let seq = reader.varint()?;
                    frame.seq_num =
                        u32::try_from(seq).map_err(|_| invalid("seq_num 超出 uint32（F-31）"))?;
                }
                _ => reader.skip(wire_type)?,
            }
        }
        Ok(frame)
    }

    /// 外层整帧：`OfflineFrame{version=V1, v1={type=5, keep_alive=6}}`。
    pub fn encode_offline(&self) -> Vec<u8> {
        let mut v1 = Vec::new();
        wire::write_varint_field(&mut v1, 1, OUTER_TYPE_KEEP_ALIVE as u64);
        wire::write_bytes_field(&mut v1, OUTER_FIELD_KEEP_ALIVE, &self.encode());
        let mut offline = Vec::new();
        wire::write_varint_field(&mut offline, 1, 1); // OfflineFrame.Version.V1
        wire::write_bytes_field(&mut offline, 2, &v1);
        offline
    }
}

/// 解析外层 keep-alive 帧；**层号必须是外层编号**（内层编号 3 一律拒绝，见 F-30）。
pub fn decode_keepalive_offline(buf: &[u8]) -> Result<KeepAliveFrame, Error> {
    let (frame_type, keep_alive) = decode_outer_v1(buf)?;
    if frame_type != OUTER_TYPE_KEEP_ALIVE as u64 {
        return Err(if frame_type == INNER_TYPE_PAIRED_KEY_ENCRYPTION as u64 {
            invalid(format!(
                "V1Frame.type={frame_type} 是**内层**编号（F-30）：外层帧不得用内层枚举"
            ))
        } else {
            unsupported(format!("V1Frame.type={frame_type} 不是 KEEP_ALIVE(5)"))
        });
    }
    let keep_alive =
        keep_alive.ok_or_else(|| invalid("keep-alive 帧缺 keep_alive 字段（外层字段号 6）"))?;
    KeepAliveFrame::decode(keep_alive)
}

/// 解析外层 `OfflineFrame{version=V1, v1}` → `(type, 指定字段的字节)`。
fn decode_outer_v1(buf: &[u8]) -> Result<(u64, Option<&[u8]>), Error> {
    let mut reader = Reader::new(buf);
    let (field, wire_type) = reader.next_key()?;
    if field != 1 || wire_type != 0 {
        return Err(invalid("OfflineFrame 必须以 version 字段开始"));
    }
    if reader.varint()? != 1 {
        return Err(Error::new(
            ErrorCode::VersionUnsupported,
            "OfflineFrame.version 不是 V1（只支持 V1）",
        ));
    }
    let mut v1_bytes: Option<&[u8]> = None;
    while !reader.eof() {
        let (field, wire_type) = reader.next_key()?;
        match (field, wire_type) {
            (2, 2) => v1_bytes = Some(reader.bytes()?),
            _ => reader.skip(wire_type)?,
        }
    }
    let v1_bytes = v1_bytes.ok_or_else(|| invalid("OfflineFrame 缺 v1"))?;
    let mut vr = Reader::new(v1_bytes);
    let mut frame_type: Option<u64> = None;
    let mut keep_alive: Option<&[u8]> = None;
    while !vr.eof() {
        let (field, wire_type) = vr.next_key()?;
        match (field, wire_type) {
            (1, 0) => frame_type = Some(vr.varint()?),
            (f, 2) if f == OUTER_FIELD_KEEP_ALIVE => keep_alive = Some(vr.bytes()?),
            _ => vr.skip(wire_type)?,
        }
    }
    let frame_type = frame_type.ok_or_else(|| invalid("V1Frame 缺 type"))?;
    Ok((frame_type, keep_alive))
}

/// keep-alive 记账（发送节奏 + 超时 + 应答）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepAliveStats {
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub sent: u32,
    pub received: u32,
    pub acks_sent: u32,
    pub expired: bool,
}

#[derive(Debug, Clone)]
pub struct KeepAliveTracker {
    interval_ms: u64,
    timeout_ms: u64,
    next_seq: u32,
    first_sent_ms: Option<u64>,
    last_sent_ms: Option<u64>,
    last_received_ms: Option<u64>,
    sent: u32,
    received: u32,
    acks_sent: u32,
    expired: bool,
}

impl Default for KeepAliveTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl KeepAliveTracker {
    /// 默认节奏：10 s 发送（F-31 来源取值）、30 s 判死（本仓策略）。
    pub fn new() -> Self {
        Self::with_policy(KEEPALIVE_INTERVAL_MS, KEEPALIVE_TIMEOUT_MS)
    }

    pub fn with_policy(interval_ms: u64, timeout_ms: u64) -> Self {
        Self {
            interval_ms,
            timeout_ms,
            next_seq: 1,
            first_sent_ms: None,
            last_sent_ms: None,
            last_received_ms: None,
            sent: 0,
            received: 0,
            acks_sent: 0,
            expired: false,
        }
    }

    pub fn stats(&self) -> KeepAliveStats {
        KeepAliveStats {
            interval_ms: self.interval_ms,
            timeout_ms: self.timeout_ms,
            sent: self.sent,
            received: self.received,
            acks_sent: self.acks_sent,
            expired: self.expired,
        }
    }

    /// 到点则产出一帧（序号自增）；未到点返回 `None`。
    pub fn on_tick(&mut self, now_ms: u64) -> Option<KeepAliveFrame> {
        if self.expired {
            return None;
        }
        let due = match self.last_sent_ms {
            None => true,
            Some(last) => now_ms.saturating_sub(last) >= self.interval_ms,
        };
        if !due {
            return None;
        }
        let frame = KeepAliveFrame::new(false, self.next_seq);
        self.next_seq = self.next_seq.wrapping_add(1);
        self.first_sent_ms.get_or_insert(now_ms);
        self.last_sent_ms = Some(now_ms);
        self.sent += 1;
        Some(frame)
    }

    /// 收到一帧：`ack = true` 时只记账；否则回一帧 `ack = true`（R15 的行为）。
    /// 已判死之后拒绝再回应（不在死连接上继续说话）。
    pub fn on_received(
        &mut self,
        frame: &KeepAliveFrame,
        now_ms: u64,
    ) -> Result<Option<KeepAliveFrame>, Error> {
        if self.expired {
            return Err(invalid(
                "keep-alive 已判超时：连接应当拆除，不再回应（本仓策略）",
            ));
        }
        self.received += 1;
        self.last_received_ms = Some(now_ms);
        if frame.ack {
            return Ok(None);
        }
        self.acks_sent += 1;
        Ok(Some(KeepAliveFrame::new(true, 0)))
    }

    /// 是否已判超时（**阈值与基准都是本仓策略**：基准取最后一次**收到**的对端帧——对端静默才是死的
    /// 信号，自己还在发心跳不能把判死往后推；从未收到过任何对端帧时，以**首个**心跳为基准，
    /// 这样"发了但永远没人理"的连接也会被判死）。
    pub fn expired(&mut self, now_ms: u64) -> bool {
        if self.expired {
            return true;
        }
        let reference = self.last_received_ms.or(self.first_sent_ms);
        if let Some(reference) = reference {
            if now_ms.saturating_sub(reference) > self.timeout_ms {
                self.expired = true;
            }
        }
        self.expired
    }
}

// ---------------------------------------------------------------- paired-key（F-32/F-33）

/// paired-key 材料：**由调用方提供**（F-32：不可离线推导；本层不做任何派生或加密）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PairedKeyMaterial {
    /// `PairedKeyEncryptionFrame.signed_data`（字段号 1）。
    pub signed_data: Vec<u8>,
    /// `PairedKeyEncryptionFrame.secret_id_hash`（字段号 2）。
    pub secret_id_hash: Vec<u8>,
    /// `PairedKeyEncryptionFrame.optional_signed_data`（字段号 3）。
    pub optional_signed_data: Option<Vec<u8>>,
    /// `PairedKeyEncryptionFrame.qr_code_handshake_data`（字段号 4）。
    pub qr_code_handshake_data: Option<Vec<u8>>,
}

impl PairedKeyMaterial {
    pub fn new(signed_data: Vec<u8>, secret_id_hash: Vec<u8>) -> Result<Self, Error> {
        let material = Self {
            signed_data,
            secret_id_hash,
            optional_signed_data: None,
            qr_code_handshake_data: None,
        };
        material.validate()?;
        Ok(material)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.signed_data.is_empty() || self.secret_id_hash.is_empty() {
            return Err(invalid(
                "signed_data 与 secret_id_hash 都不得为空（F-32 的两个字段）",
            ));
        }
        for (name, value) in [
            ("signed_data", Some(&self.signed_data)),
            ("secret_id_hash", Some(&self.secret_id_hash)),
            ("optional_signed_data", self.optional_signed_data.as_ref()),
            (
                "qr_code_handshake_data",
                self.qr_code_handshake_data.as_ref(),
            ),
        ] {
            if let Some(bytes) = value {
                if bytes.len() > MAX_PAIRED_KEY_BYTES {
                    return Err(Error::new(
                        ErrorCode::ResourceLimit,
                        format!(
                            "{name} {} 字节超过本仓上限 {MAX_PAIRED_KEY_BYTES}（策略值）",
                            bytes.len()
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// 内层 `V1Frame{type=3, paired_key_encryption=4}`。
    pub fn encode_inner(&self) -> Vec<u8> {
        let mut inner = Vec::new();
        wire::write_bytes_field(&mut inner, 1, &self.signed_data);
        wire::write_bytes_field(&mut inner, 2, &self.secret_id_hash);
        if let Some(bytes) = &self.optional_signed_data {
            wire::write_bytes_field(&mut inner, 3, bytes);
        }
        if let Some(bytes) = &self.qr_code_handshake_data {
            wire::write_bytes_field(&mut inner, 4, bytes);
        }
        encode_inner_v1(
            INNER_TYPE_PAIRED_KEY_ENCRYPTION,
            INNER_FIELD_PAIRED_KEY_ENCRYPTION,
            &inner,
        )
    }

    /// 解析内层帧（`type` 必须是内层编号 3）。
    pub fn decode_inner(buf: &[u8]) -> Result<Self, Error> {
        let body = decode_inner_v1(
            buf,
            INNER_TYPE_PAIRED_KEY_ENCRYPTION,
            INNER_FIELD_PAIRED_KEY_ENCRYPTION,
            "PAIRED_KEY_ENCRYPTION",
        )?;
        let mut reader = Reader::new(body);
        let mut material = Self::default();
        while !reader.eof() {
            let (field, wire_type) = reader.next_key()?;
            match (field, wire_type) {
                (1, 2) => material.signed_data = reader.bytes()?.to_vec(),
                (2, 2) => material.secret_id_hash = reader.bytes()?.to_vec(),
                (3, 2) => material.optional_signed_data = Some(reader.bytes()?.to_vec()),
                (4, 2) => material.qr_code_handshake_data = Some(reader.bytes()?.to_vec()),
                _ => reader.skip(wire_type)?,
            }
        }
        material.validate()?;
        Ok(material)
    }
}

/// `PairedKeyResultFrame.Status`（R15 `wire_format.proto:343-348`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairedKeyStatus {
    Unknown,
    Success,
    Fail,
    Unable,
}

impl PairedKeyStatus {
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Unknown => 0,
            Self::Success => 1,
            Self::Fail => 2,
            Self::Unable => 3,
        }
    }

    pub fn from_i64(value: i64) -> Result<Self, Error> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Success),
            2 => Ok(Self::Fail),
            3 => Ok(Self::Unable),
            other => Err(invalid(format!(
                "未知 paired-key status {other}（F-33 只有 0..3；不得当成 SUCCESS）"
            ))),
        }
    }
}

/// `PairedKeyResultFrame{status=1, os_type=2}`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairedKeyResultFrame {
    pub status: PairedKeyStatus,
    pub os_type: Option<i32>,
}

impl PairedKeyResultFrame {
    /// 参考实现在收到 encryption 帧后回的就是这个（`UNABLE`，R15 `InboundNearbyConnection.swift:271-277`）。
    pub fn unable() -> Self {
        Self {
            status: PairedKeyStatus::Unable,
            os_type: None,
        }
    }

    /// 内层 `V1Frame{type=4, paired_key_result=5}`。
    pub fn encode_inner(&self) -> Vec<u8> {
        let mut inner = Vec::new();
        wire::write_varint_field(&mut inner, 1, self.status.as_i64() as u64);
        if let Some(os_type) = self.os_type {
            wire::write_varint_field(&mut inner, 2, os_type as u64);
        }
        encode_inner_v1(
            INNER_TYPE_PAIRED_KEY_RESULT,
            INNER_FIELD_PAIRED_KEY_RESULT,
            &inner,
        )
    }

    pub fn decode_inner(buf: &[u8]) -> Result<Self, Error> {
        let body = decode_inner_v1(
            buf,
            INNER_TYPE_PAIRED_KEY_RESULT,
            INNER_FIELD_PAIRED_KEY_RESULT,
            "PAIRED_KEY_RESULT",
        )?;
        let mut reader = Reader::new(body);
        let mut status = None;
        let mut os_type = None;
        while !reader.eof() {
            let (field, wire_type) = reader.next_key()?;
            match (field, wire_type) {
                (1, 0) => status = Some(PairedKeyStatus::from_i64(reader.varint()? as i64)?),
                (2, 0) => {
                    os_type = Some(
                        i32::try_from(reader.varint()?)
                            .map_err(|_| invalid("os_type 超出 int32（F-33）"))?,
                    )
                }
                _ => reader.skip(wire_type)?,
            }
        }
        Ok(Self {
            status: status.ok_or_else(|| invalid("PairedKeyResultFrame 缺 status（F-33）"))?,
            os_type,
        })
    }
}

/// 内层 `OfflineFrame{version=V1, v1={type, <field>}}`。
fn encode_inner_v1(frame_type: i64, field: u32, body: &[u8]) -> Vec<u8> {
    let mut v1 = Vec::new();
    wire::write_varint_field(&mut v1, 1, frame_type as u64);
    wire::write_bytes_field(&mut v1, field, body);
    let mut offline = Vec::new();
    wire::write_varint_field(&mut offline, 1, 1);
    wire::write_bytes_field(&mut offline, 2, &v1);
    offline
}

/// 解析内层帧，返回载荷字节；层号不符即拒绝（F-30：外层编号同样是 3，必须靠**上下文**区分）。
fn decode_inner_v1<'a>(
    buf: &'a [u8],
    expected_type: i64,
    expected_field: u32,
    name: &str,
) -> Result<&'a [u8], Error> {
    let mut reader = Reader::new(buf);
    let (field, wire_type) = reader.next_key()?;
    if field != 1 || wire_type != 0 {
        return Err(invalid("内层 OfflineFrame 必须以 version 字段开始"));
    }
    if reader.varint()? != 1 {
        return Err(Error::new(
            ErrorCode::VersionUnsupported,
            "内层 OfflineFrame.version 不是 V1",
        ));
    }
    let mut v1_bytes: Option<&[u8]> = None;
    while !reader.eof() {
        let (field, wire_type) = reader.next_key()?;
        match (field, wire_type) {
            (2, 2) => v1_bytes = Some(reader.bytes()?),
            _ => reader.skip(wire_type)?,
        }
    }
    let v1_bytes = v1_bytes.ok_or_else(|| invalid("内层 OfflineFrame 缺 v1"))?;
    let mut vr = Reader::new(v1_bytes);
    let mut frame_type: Option<i64> = None;
    let mut body: Option<&[u8]> = None;
    while !vr.eof() {
        let (field, wire_type) = vr.next_key()?;
        match (field, wire_type) {
            (1, 0) => frame_type = Some(vr.varint()? as i64),
            (f, 2) if f == expected_field => body = Some(vr.bytes()?),
            _ => vr.skip(wire_type)?,
        }
    }
    let frame_type = frame_type.ok_or_else(|| invalid("内层 V1Frame 缺 type"))?;
    if frame_type != expected_type {
        return Err(unsupported(format!(
            "内层 V1Frame.type={frame_type} 不是 {name}({expected_type})（F-30 的两层编号）"
        )));
    }
    body.ok_or_else(|| invalid(format!("内层 V1Frame 缺 {name} 载荷字段")))
}

/// 把内层帧装进 BYTES payload（R15 `NearbyConnection.swift:210-226` 的做法：`header.type=bytes`、
/// `chunk.offset=0`、`chunk.flags=0`）。
pub fn wrap_inner_as_bytes_payload(inner_frame: &[u8], payload_id: i64) -> Result<Vec<u8>, Error> {
    if payload_id == 0 {
        return Err(invalid("payload id 不得为 0（F-27/F-28）"));
    }
    let frame = PayloadTransferFrame {
        packet_type: PacketType::Data,
        header: Some(PayloadHeader {
            id: payload_id,
            kind: PayloadKind::Bytes,
            total_size: inner_frame.len() as u64,
            file_name: None,
            parent_folder: None,
        }),
        chunk: Some(PayloadChunk {
            flags: 0,
            offset: 0,
            body: inner_frame.to_vec(),
        }),
    };
    encode_payload_transfer_frame(&frame)
}

/// 解出 BYTES payload 的内层帧字节（**要求 `kind = BYTES`**；FILE 载荷走 T21 的落盘路径）。
pub fn unwrap_bytes_payload(buf: &[u8]) -> Result<(Vec<u8>, i64), Error> {
    let frame = crate::payload::decode_payload_transfer_frame(buf)?;
    let header = frame
        .header
        .ok_or_else(|| invalid("payload 帧缺 header（协商帧必须带 header）"))?;
    if header.kind != PayloadKind::Bytes {
        return Err(invalid(format!(
            "协商帧必须走 BYTES 载荷，收到 {:?}（F-30 的内层载体）",
            header.kind.as_wire()
        )));
    }
    let chunk = frame
        .chunk
        .ok_or_else(|| invalid("payload 帧缺 chunk（协商帧必须带正文）"))?;
    if !chunk.body.is_empty() && chunk.offset != 0 {
        return Err(invalid("协商帧不分片：offset 必须为 0（参考实现同此）"));
    }
    Ok((chunk.body, header.id))
}

/// 配对结果对"4 位确认码"的影响。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairedKeyDecision {
    /// 仍然要求用户核对 4 位确认码（**默认**）。
    RequireConfirmation,
    /// 允许跳过——只在调用方显式开启开关**且**对端报 `SUCCESS` 时成立。
    SkipConfirmation,
}

/// 交换状态（本仓记账）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairedKeyState {
    Idle,
    EncryptionSent,
    EncryptionReceived,
    ResultReceived,
    Complete,
}

/// 交换期间产生的事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairedKeyEvent {
    /// 收到对端的 encryption 帧：需要回一条 result（参考实现回 `UNABLE`）。
    NeedResult { reply: Vec<u8> },
    /// 收到对端的 result 帧。
    Result {
        status: PairedKeyStatus,
        decision: PairedKeyDecision,
    },
}

/// paired-key 交换状态机（F-32/F-33）。材料由调用方给；本层不派生任何秘密。
#[derive(Debug, Clone)]
pub struct PairedKeyExchange {
    allow_skip_confirmation: bool,
    state: PairedKeyState,
    peer_status: Option<PairedKeyStatus>,
    next_payload_id: i64,
}

impl Default for PairedKeyExchange {
    fn default() -> Self {
        Self::new()
    }
}

impl PairedKeyExchange {
    /// 默认策略：**即使对端报 `SUCCESS` 也仍然要求 4 位确认码**（本仓没有配对存储）。
    pub fn new() -> Self {
        Self {
            allow_skip_confirmation: false,
            state: PairedKeyState::Idle,
            peer_status: None,
            next_payload_id: 1000,
        }
    }

    /// 调用方显式开启"允许跳过"（例如宿主已有自己的配对存储并愿意承担判断）。
    pub fn allowing_skip_confirmation(mut self) -> Self {
        self.allow_skip_confirmation = true;
        self
    }

    pub fn state(&self) -> PairedKeyState {
        self.state
    }

    pub fn peer_status(&self) -> Option<PairedKeyStatus> {
        self.peer_status
    }

    pub fn decision(&self) -> PairedKeyDecision {
        match self.peer_status {
            Some(PairedKeyStatus::Success) if self.allow_skip_confirmation => {
                PairedKeyDecision::SkipConfirmation
            }
            _ => PairedKeyDecision::RequireConfirmation,
        }
    }

    /// 发起：把调用方给的材料编成内层帧，再装进 BYTES payload 返回（可直接发送）。
    pub fn begin(&mut self, material: &PairedKeyMaterial) -> Result<Vec<u8>, Error> {
        material.validate()?;
        let inner = material.encode_inner();
        let payload_id = self.next_payload_id;
        self.next_payload_id = self.next_payload_id.wrapping_add(1);
        let wrapped = wrap_inner_as_bytes_payload(&inner, payload_id)?;
        self.state = PairedKeyState::EncryptionSent;
        Ok(wrapped)
    }

    /// 收到一条 BYTES payload：内层是这个交换的帧 → 推进状态并给出事件。
    pub fn on_bytes_payload(&mut self, payload: &[u8]) -> Result<PairedKeyEvent, Error> {
        let (inner, payload_id) = unwrap_bytes_payload(payload)?;
        if let Ok(material) = PairedKeyMaterial::decode_inner(&inner) {
            // 收到的 encryption 帧：材料原样交给调用方（本层不解释、不保存内容语义）。
            let _ = material;
            self.state = match self.state {
                PairedKeyState::EncryptionSent => PairedKeyState::EncryptionReceived,
                other => other,
            };
            let reply_inner = PairedKeyResultFrame::unable().encode_inner();
            let reply = wrap_inner_as_bytes_payload(&reply_inner, payload_id)?;
            return Ok(PairedKeyEvent::NeedResult { reply });
        }
        let result = PairedKeyResultFrame::decode_inner(&inner)?;
        self.peer_status = Some(result.status);
        self.state = match self.state {
            PairedKeyState::EncryptionSent | PairedKeyState::EncryptionReceived => {
                PairedKeyState::ResultReceived
            }
            _ => PairedKeyState::Complete,
        };
        Ok(PairedKeyEvent::Result {
            status: result.status,
            decision: self.decision(),
        })
    }
}

// ---------------------------------------------------------------- DisconnectionFrame（F-34..F-36）

/// 收到 DisconnectionFrame 后本仓要做的动作（F-35 的三路规则）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectAction {
    /// 未请求 safe-to-disconnect（字段缺席或为 false）→ **立即关闭**。
    CloseNow,
    /// `request=1` 且 `ack=1` → 标记该端点已 safe-to-disconnect 并通知停止等待，**不回帧**。
    MarkedAndNotified,
    /// `request=1` 且 `ack` 为 false/缺席 → 标记（不通知）并**回一帧 (true,true)**。
    MarkedAndReply(DisconnectionFrame),
}

/// `DisconnectionFrame{request_safe_to_disconnect=1, ack_safe_to_disconnect=2}`（R17 `:455-463`）。
///
/// **两个字段都是 `Option`**：R17 的构造器总是显式写（false 也写），NearDrop 发的是空正文——
/// 这不是风格差异，而是**不同的字节**（F-36），所以解码必须保留"字段是否存在"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DisconnectionFrame {
    pub request_safe_to_disconnect: Option<bool>,
    pub ack_safe_to_disconnect: Option<bool>,
}

impl DisconnectionFrame {
    pub fn new(request: Option<bool>, ack: Option<bool>) -> Self {
        Self {
            request_safe_to_disconnect: request,
            ack_safe_to_disconnect: ack,
        }
    }

    /// NearDrop 的形态：一个字段都不设（`NearbyConnection.swift:411-423`）。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 应答帧：`(request=1, ack=1)`（R17 `endpoint_manager.cc:402-403`）。
    pub fn reply_ack() -> Self {
        Self::new(Some(true), Some(true))
    }

    pub fn has_request(&self) -> bool {
        self.request_safe_to_disconnect.is_some()
    }

    pub fn has_ack(&self) -> bool {
        self.ack_safe_to_disconnect.is_some()
    }

    /// 外层整帧：`OfflineFrame{version=V1, v1={type=6, disconnection=7}}`。
    pub fn encode_offline(&self) -> Vec<u8> {
        let mut body = Vec::new();
        if let Some(request) = self.request_safe_to_disconnect {
            wire::write_varint_field(&mut body, 1, u64::from(request));
        }
        if let Some(ack) = self.ack_safe_to_disconnect {
            wire::write_varint_field(&mut body, 2, u64::from(ack));
        }
        let mut v1 = Vec::new();
        wire::write_varint_field(&mut v1, 1, OUTER_TYPE_DISCONNECTION as u64);
        wire::write_bytes_field(&mut v1, OUTER_FIELD_DISCONNECTION, &body);
        let mut offline = Vec::new();
        wire::write_varint_field(&mut offline, 1, 1); // OfflineFrame.Version.V1
        wire::write_bytes_field(&mut offline, 2, &v1);
        offline
    }

    /// 解析外层帧；类型必须是 `DISCONNECTION(6)`（内层编号一律拒绝，见 F-30/F-36）。
    pub fn decode_offline(buf: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(buf);
        let (field, wire_type) = reader.next_key()?;
        if field != 1 || wire_type != 0 {
            return Err(invalid("OfflineFrame 必须以 version 字段开始"));
        }
        if reader.varint()? != 1 {
            return Err(Error::new(
                ErrorCode::VersionUnsupported,
                "OfflineFrame.version 不是 V1（只支持 V1）",
            ));
        }
        let mut v1_bytes: Option<&[u8]> = None;
        while !reader.eof() {
            let (field, wire_type) = reader.next_key()?;
            match (field, wire_type) {
                (2, 2) => v1_bytes = Some(reader.bytes()?),
                _ => reader.skip(wire_type)?,
            }
        }
        let v1_bytes = v1_bytes.ok_or_else(|| invalid("OfflineFrame 缺 v1"))?;
        let mut vr = Reader::new(v1_bytes);
        let mut frame_type: Option<u64> = None;
        let mut body: Option<&[u8]> = None;
        while !vr.eof() {
            let (field, wire_type) = vr.next_key()?;
            match (field, wire_type) {
                (1, 0) => frame_type = Some(vr.varint()?),
                (f, 2) if f == OUTER_FIELD_DISCONNECTION => body = Some(vr.bytes()?),
                _ => vr.skip(wire_type)?,
            }
        }
        let frame_type = frame_type.ok_or_else(|| invalid("V1Frame 缺 type"))?;
        if frame_type != OUTER_TYPE_DISCONNECTION as u64 {
            return Err(unsupported(format!(
                "V1Frame.type={frame_type} 不是 DISCONNECTION(6)（外层帧类型不符）"
            )));
        }
        let body = body.ok_or_else(|| invalid("V1Frame 缺 disconnection 字段（外层字段号 7）"))?;
        let mut frame = Self::default();
        let mut br = Reader::new(body);
        while !br.eof() {
            let (field, wire_type) = br.next_key()?;
            match (field, wire_type) {
                (1, 0) => frame.request_safe_to_disconnect = Some(br.varint()? != 0),
                (2, 0) => frame.ack_safe_to_disconnect = Some(br.varint()? != 0),
                _ => br.skip(wire_type)?,
            }
        }
        Ok(frame)
    }

    /// F-35 的三路决策。
    pub fn decision(&self) -> DisconnectAction {
        match self.request_safe_to_disconnect {
            Some(true) => {
                if self.ack_safe_to_disconnect == Some(true) {
                    DisconnectAction::MarkedAndNotified
                } else {
                    DisconnectAction::MarkedAndReply(Self::reply_ack())
                }
            }
            _ => DisconnectAction::CloseNow,
        }
    }
}

// ---------------------------------------------------------------- PAYLOAD_ACK（F-37..F-39）

/// payload 帧的分类（`packet_type` 的三种取值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadFrameClass {
    /// `DATA(1)`：分块数据。
    Data,
    /// `PAYLOAD_ACK(3)`：接收方对**末块**的确认。
    Ack,
}

/// 分类 payload 帧；`CONTROL(2)` **明确拒绝**（F-39：官方已标注 "Use PacketType.PAYLOAD_ACK instead"）。
pub fn classify_payload_frame(frame: &PayloadTransferFrame) -> Result<PayloadFrameClass, Error> {
    match frame.packet_type {
        PacketType::Data => Ok(PayloadFrameClass::Data),
        PacketType::PayloadAck => Ok(PayloadFrameClass::Ack),
        PacketType::Control => Err(unsupported(
            "packet_type=CONTROL 是已废弃路径（官方注「Use PacketType.PAYLOAD_ACK instead」）：\
             本仓不实现 control 事件，收到即拒绝（F-39）",
        )),
    }
}

/// 构造 PAYLOAD_ACK 帧（R17 `offline_frames.cc:242-256`）：只带 `header{id, total_size=-1}`。
pub fn encode_payload_ack(payload_id: i64) -> Result<Vec<u8>, Error> {
    if payload_id <= 0 {
        return Err(invalid("payload id 必须为正（F-27/F-28）"));
    }
    let frame = PayloadTransferFrame {
        packet_type: PacketType::PayloadAck,
        header: Some(PayloadHeader {
            id: payload_id,
            kind: PayloadKind::File,
            total_size: INDETERMINATE_TOTAL_SIZE,
            file_name: None,
            parent_folder: None,
        }),
        chunk: None,
    };
    encode_payload_transfer_frame(&frame)
}

/// 解析 PAYLOAD_ACK 帧，返回它确认的 payload id。
///
/// 要求：`packet_type=PAYLOAD_ACK`、有 header、`total_size == -1`、**不带 chunk**。
pub fn decode_payload_ack(buf: &[u8]) -> Result<i64, Error> {
    let frame = decode_payload_transfer_frame(buf)?;
    if frame.packet_type != PacketType::PayloadAck {
        return Err(invalid(format!(
            "packet_type={:?} 不是 PAYLOAD_ACK(3)",
            frame.packet_type
        )));
    }
    if frame.chunk.is_some() {
        return Err(invalid("PAYLOAD_ACK 不得带 chunk（字段表：只带 payload_header）"));
    }
    let header = frame
        .header
        .ok_or_else(|| invalid("PAYLOAD_ACK 缺 payload_header（无法知道确认哪个 payload）"))?;
    if header.total_size != INDETERMINATE_TOTAL_SIZE {
        return Err(invalid(format!(
            "PAYLOAD_ACK 的 total_size 必须是 -1（kIndeterminateSize），收到 {}",
            header.total_size
        )));
    }
    Ok(header.id)
}

/// 是否应当为该 payload 的这个分块发送 ack（F-38 的门槛）。
///
/// 来源的启用条件明确要求载荷类型**不是 BYTES**——协商类载荷不发 ack；且只有**末块**才发。
pub fn should_send_payload_ack(kind: PayloadKind, is_last_chunk: bool) -> bool {
    is_last_chunk && kind != PayloadKind::Bytes
}

/// 收到 ack 时对发送侧记账的影响（F-38 的三个分支）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckOutcome {
    /// 本端发出的 payload 被确认。
    Marked,
    /// 未知 payload id → **忽略**（来源行为，不当错误）。
    IgnoredUnknownPayload,
    /// 对本端 **incoming** payload 的 ack → **忽略**。
    IgnoredIncomingPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PayloadDirection {
    Outgoing,
    Incoming,
}

/// 发送侧的 payload 记账：只用来判定 ack 的三分支。
#[derive(Debug, Clone, Default)]
pub struct PayloadAckTracker {
    payloads: std::collections::BTreeMap<i64, (PayloadDirection, bool)>,
}

impl PayloadAckTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个**本端发出**的 payload（只有它能被 ack 确认）。
    pub fn register_outgoing(&mut self, payload_id: i64) {
        self.payloads.insert(payload_id, (PayloadDirection::Outgoing, false));
    }

    /// 登记一个**本端接收**的 payload（对它收到 ack 会被忽略）。
    pub fn register_incoming(&mut self, payload_id: i64) {
        self.payloads.insert(payload_id, (PayloadDirection::Incoming, false));
    }

    pub fn on_ack(&mut self, payload_id: i64) -> AckOutcome {
        match self.payloads.get_mut(&payload_id) {
            None => AckOutcome::IgnoredUnknownPayload,
            Some((PayloadDirection::Incoming, _)) => AckOutcome::IgnoredIncomingPayload,
            Some((PayloadDirection::Outgoing, acked)) => {
                *acked = true;
                AckOutcome::Marked
            }
        }
    }

    pub fn acked(&self, payload_id: i64) -> bool {
        matches!(
            self.payloads.get(&payload_id),
            Some((PayloadDirection::Outgoing, true))
        )
    }

    pub fn len(&self) -> usize {
        self.payloads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.payloads.is_empty()
    }
}

// ---------------------------------------------------------------- keep-alive 协商（F-40）

/// keep-alive 取值的来源（报告里必须能区分）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepAliveSource {
    /// 本仓策略值（连接握手不实现时的唯一来源）。
    RepoPolicy,
    /// 来自握手协商字段（`ConnectionRequestFrame`/`ConnectionResponseFrame`）。
    Negotiated,
}

/// 生效的 keep-alive 参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepAlivePolicy {
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub source: KeepAliveSource,
}

impl KeepAlivePolicy {
    /// 本仓策略：10 s 发送（F-31 来源取值）、30 s 判死（本仓策略）。
    pub fn repo_default() -> Self {
        Self {
            interval_ms: KEEPALIVE_INTERVAL_MS,
            timeout_ms: KEEPALIVE_TIMEOUT_MS,
            source: KeepAliveSource::RepoPolicy,
        }
    }

    /// 从协商字段构造：非法值拒绝，缺席的字段回退本仓策略（F-40）。
    ///
    /// `interval` 来自 `ConnectionRequestFrame.keep_alive_interval_millis`；
    /// `timeout` 可能来自请求或响应（`keep_alive_timeout_millis`）。两者在 proto 里都是
    /// `optional int32` 且**没有默认值**。
    pub fn from_negotiation(interval: Option<i64>, timeout: Option<i64>) -> Result<Self, Error> {
        let parse = |value: i64, what: &str| -> Result<u64, Error> {
            if value <= 0 {
                return Err(invalid(format!(
                    "协商的 {what}={value} 必须为正（proto 无默认值，负值/0 一律拒绝）"
                )));
            }
            u32::try_from(value)
                .map(u64::from)
                .map_err(|_| invalid(format!("协商的 {what}={value} 超出 int32 允许的取值上限")))
        };
        let interval_ms = match interval {
            Some(v) => parse(v, "keep_alive_interval_millis")?,
            None => KEEPALIVE_INTERVAL_MS,
        };
        let timeout_ms = match timeout {
            Some(v) => parse(v, "keep_alive_timeout_millis")?,
            None => KEEPALIVE_TIMEOUT_MS,
        };
        if timeout_ms <= interval_ms {
            return Err(invalid(format!(
                "协商的 timeout({timeout_ms}) 必须大于 interval({interval_ms})，否则还没发就先判死"
            )));
        }
        let source = if interval.is_some() || timeout.is_some() {
            KeepAliveSource::Negotiated
        } else {
            KeepAliveSource::RepoPolicy
        };
        Ok(Self {
            interval_ms,
            timeout_ms,
            source,
        })
    }
}
