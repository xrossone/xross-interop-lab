//! payload 层（plans/02-files.md §T21；字段表 F-22/F-25..F-28）。
//!
//! 字段号来源：R17 `connections/implementation/proto/offline_wire_formats.proto`（Apache-2.0）——
//! `OfflineFrame{version=1(V1=1), v1=2}`、`V1Frame{type=1(PAYLOAD_TRANSFER=3), payload_transfer=4}`、
//! `PayloadTransferFrame{packet_type=1(DATA=1,CONTROL=2,PAYLOAD_ACK=3), payload_header=2,
//! payload_chunk=3, control_message=4}`、
//! `PayloadHeader{id=1,type=2(BYTES=1,FILE=2,STREAM=3),total_size=3,is_sensitive=4,file_name=5,parent_folder=6}`、
//! `PayloadChunk{flags=1(LAST_CHUNK=0x1), offset=2, body=3, index=4}`。
//! `FileMetadata{name=1,type=2,payload_id=3,size=4,mime_type=5,id=6,parent_folder=7,is_sensitive_content=9}`
//! 来自 R15 汇集自 Chromium 的 `wire_format.proto`（见 provenance：仅取字段号事实）。
//!
//! 内存纪律（T21-03）：本层**从不累积** payload 字节——每个 chunk 直接交给 [`ChunkSink`]；
//! 组装器只保留"已写多少字节 / 下一个 offset / 是否完成"这类 O(1) 状态。

use interop_contract::error::{Error, ErrorCode};

use crate::wire::{self, Reader};

/// `PayloadChunk.flags` 的 `LAST_CHUNK`（F-22）。
pub const FLAG_LAST_CHUNK: i32 = 0x1;

/// 单个 chunk 的字节上限（本仓保守值：避免单帧 enormous 分配；不是协议常量）。
pub const MAX_CHUNK_BYTES: usize = 4 * 1024 * 1024;

/// `PayloadHeader.PayloadType`（F-22）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadKind {
    Bytes,
    File,
    Stream,
}

impl PayloadKind {
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Bytes => 1,
            Self::File => 2,
            Self::Stream => 3,
        }
    }

    pub fn from_i64(v: i64) -> Result<Self, Error> {
        match v {
            1 => Ok(Self::Bytes),
            2 => Ok(Self::File),
            3 => Ok(Self::Stream),
            other => Err(invalid(format!("未知 PayloadType {other}（fail-closed）"))),
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Bytes => "bytes",
            Self::File => "file",
            Self::Stream => "stream",
        }
    }
}

/// `PayloadTransferFrame.PacketType`（F-22）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    Data,
    Control,
    PayloadAck,
}

impl PacketType {
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Data => 1,
            Self::Control => 2,
            Self::PayloadAck => 3,
        }
    }

    pub fn from_i64(v: i64) -> Result<Self, Error> {
        match v {
            1 => Ok(Self::Data),
            2 => Ok(Self::Control),
            3 => Ok(Self::PayloadAck),
            other => Err(invalid(format!("未知 PacketType {other}（fail-closed）"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadHeader {
    pub id: i64,
    pub kind: PayloadKind,
    pub total_size: u64,
    pub file_name: Option<String>,
    pub parent_folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadChunk {
    pub flags: i32,
    pub offset: u64,
    pub body: Vec<u8>,
}

impl PayloadChunk {
    pub fn is_last(&self) -> bool {
        self.flags & FLAG_LAST_CHUNK != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadTransferFrame {
    pub packet_type: PacketType,
    pub header: Option<PayloadHeader>,
    pub chunk: Option<PayloadChunk>,
}

impl PayloadTransferFrame {
    pub fn data(id: i64, total_size: u64, offset: u64, last: bool, body: Vec<u8>) -> Self {
        Self {
            packet_type: PacketType::Data,
            header: Some(PayloadHeader {
                id,
                kind: PayloadKind::File,
                total_size,
                file_name: None,
                parent_folder: None,
            }),
            chunk: Some(PayloadChunk {
                flags: if last { FLAG_LAST_CHUNK } else { 0 },
                offset,
                body,
            }),
        }
    }

    /// 控制帧（`PAYLOAD_CANCELED` 等事件的载体；事件号不在本 task 使用）。
    pub fn control(packet_type: PacketType) -> Self {
        Self {
            packet_type,
            header: None,
            chunk: None,
        }
    }
}

// ---------------------------------------------------------------- 编解码

pub fn encode_payload_transfer_frame(frame: &PayloadTransferFrame) -> Result<Vec<u8>, Error> {
    let mut inner = Vec::new();
    wire::write_varint_field(&mut inner, 1, frame.packet_type.as_i64() as u64);
    if let Some(h) = &frame.header {
        let mut hb = Vec::new();
        wire::write_varint_field(&mut hb, 1, h.id as u64);
        wire::write_varint_field(&mut hb, 2, h.kind.as_i64() as u64);
        wire::write_varint_field(&mut hb, 3, h.total_size);
        if let Some(name) = &h.file_name {
            wire::write_bytes_field(&mut hb, 5, name.as_bytes());
        }
        if let Some(parent) = &h.parent_folder {
            wire::write_bytes_field(&mut hb, 6, parent.as_bytes());
        }
        wire::write_bytes_field(&mut inner, 2, &hb);
    }
    if let Some(c) = &frame.chunk {
        if c.body.len() > MAX_CHUNK_BYTES {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("chunk {} 字节超过本仓上限 {MAX_CHUNK_BYTES}", c.body.len()),
            )
            .with_phase("transferring"));
        }
        let mut cb = Vec::with_capacity(c.body.len() + 16);
        wire::write_varint_field(&mut cb, 1, c.flags as u64);
        wire::write_varint_field(&mut cb, 2, c.offset);
        wire::write_bytes_field(&mut cb, 3, &c.body);
        wire::write_bytes_field(&mut inner, 3, &cb);
    }
    encode_v1_offline(&inner)
}

/// `OfflineFrame{version=1, v1=V1Frame{type=3(PAYLOAD_TRANSFER), payload_transfer=4}}`。
fn encode_v1_offline(payload_transfer: &[u8]) -> Result<Vec<u8>, Error> {
    let mut v1 = Vec::new();
    wire::write_varint_field(&mut v1, 1, 3); // V1Frame.FrameType.PAYLOAD_TRANSFER
    wire::write_bytes_field(&mut v1, 4, payload_transfer);
    let mut offline = Vec::new();
    wire::write_varint_field(&mut offline, 1, 1); // OfflineFrame.Version.V1
    wire::write_bytes_field(&mut offline, 2, &v1);
    Ok(offline)
}

pub fn decode_payload_transfer_frame(buf: &[u8]) -> Result<PayloadTransferFrame, Error> {
    let mut r = Reader::new(buf);
    let (version_field, version_wire) = r.next_key()?;
    if version_field != 1 || version_wire != 0 {
        return Err(invalid("OfflineFrame 必须以 version 字段开始"));
    }
    if r.varint()? != 1 {
        return Err(Error::new(
            ErrorCode::VersionUnsupported,
            "OfflineFrame.version 不是 V1（只支持 V1）",
        )
        .with_phase("transferring"));
    }
    let mut v1_bytes: Option<&[u8]> = None;
    while !r.eof() {
        let (field, wire_type) = r.next_key()?;
        match (field, wire_type) {
            (2, 2) => v1_bytes = Some(r.bytes()?),
            _ => r.skip(wire_type)?,
        }
    }
    let v1_bytes = v1_bytes.ok_or_else(|| invalid("OfflineFrame 缺 v1"))?;
    let mut vr = Reader::new(v1_bytes);
    let mut frame_type = None;
    let mut payload_bytes: Option<&[u8]> = None;
    while !vr.eof() {
        let (field, wire_type) = vr.next_key()?;
        match (field, wire_type) {
            (1, 0) => frame_type = Some(vr.varint()?),
            (4, 2) => payload_bytes = Some(vr.bytes()?),
            _ => vr.skip(wire_type)?,
        }
    }
    match frame_type {
        Some(3) => {}
        Some(other) => {
            return Err(Error::new(
                ErrorCode::UnsupportedFeature,
                format!("V1Frame.type={other} 不是 PAYLOAD_TRANSFER(3)：本层只处理 payload 帧"),
            )
            .with_phase("transferring"))
        }
        None => return Err(invalid("V1Frame 缺 type")),
    }
    let payload_bytes = payload_bytes.ok_or_else(|| invalid("V1Frame 缺 payload_transfer"))?;

    let mut pr = Reader::new(payload_bytes);
    let mut packet_type = None;
    let mut header = None;
    let mut chunk = None;
    while !pr.eof() {
        let (field, wire_type) = pr.next_key()?;
        match (field, wire_type) {
            (1, 0) => packet_type = Some(PacketType::from_i64(pr.varint()? as i64)?),
            (2, 2) => header = Some(decode_payload_header(pr.bytes()?)?),
            (3, 2) => chunk = Some(decode_payload_chunk(pr.bytes()?)?),
            _ => pr.skip(wire_type)?,
        }
    }
    Ok(PayloadTransferFrame {
        packet_type: packet_type.ok_or_else(|| invalid("PayloadTransferFrame 缺 packet_type"))?,
        header,
        chunk,
    })
}

fn decode_payload_header(buf: &[u8]) -> Result<PayloadHeader, Error> {
    let mut r = Reader::new(buf);
    let mut id = None;
    let mut kind = None;
    let mut total_size = None;
    let mut file_name = None;
    let mut parent_folder = None;
    while !r.eof() {
        let (field, wire_type) = r.next_key()?;
        match (field, wire_type) {
            (1, 0) => id = Some(r.varint()? as i64),
            (2, 0) => kind = Some(PayloadKind::from_i64(r.varint()? as i64)?),
            (3, 0) => total_size = Some(r.varint()?),
            (5, 2) => {
                file_name = Some(
                    std::str::from_utf8(r.bytes()?)
                        .map_err(|_| invalid("file_name 不是合法 UTF-8"))?
                        .to_string(),
                )
            }
            (6, 2) => {
                parent_folder = Some(
                    std::str::from_utf8(r.bytes()?)
                        .map_err(|_| invalid("parent_folder 不是合法 UTF-8"))?
                        .to_string(),
                )
            }
            _ => r.skip(wire_type)?,
        }
    }
    Ok(PayloadHeader {
        id: id.ok_or_else(|| invalid("PayloadHeader 缺 id"))?,
        kind: kind.ok_or_else(|| invalid("PayloadHeader 缺 type"))?,
        total_size: total_size.ok_or_else(|| invalid("PayloadHeader 缺 total_size"))?,
        file_name,
        parent_folder,
    })
}

fn decode_payload_chunk(buf: &[u8]) -> Result<PayloadChunk, Error> {
    let mut r = Reader::new(buf);
    let mut flags = None;
    let mut offset = None;
    let mut body = None;
    while !r.eof() {
        let (field, wire_type) = r.next_key()?;
        match (field, wire_type) {
            (1, 0) => flags = Some(r.varint()? as i32),
            (2, 0) => offset = Some(r.varint()?),
            (3, 2) => body = Some(r.bytes()?.to_vec()),
            _ => r.skip(wire_type)?,
        }
    }
    let body = body.ok_or_else(|| invalid("PayloadChunk 缺 body"))?;
    if body.len() > MAX_CHUNK_BYTES {
        return Err(Error::new(
            ErrorCode::ResourceLimit,
            format!("chunk {} 字节超过本仓上限 {MAX_CHUNK_BYTES}", body.len()),
        )
        .with_phase("transferring"));
    }
    Ok(PayloadChunk {
        flags: flags.unwrap_or(0),
        offset: offset.ok_or_else(|| invalid("PayloadChunk 缺 offset"))?,
        body,
    })
}

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("transferring")
}

// ---------------------------------------------------------------- 组装（常数内存）

/// payload 字节的落点。实现方必须**立即消费** `body`（本层不保留引用）。
pub trait ChunkSink {
    fn write_chunk(&mut self, offset: u64, body: &[u8]) -> Result<(), Error>;
}

/// 单条 payload 的顺序组装器：只保留 O(1) 状态，绝不缓冲 payload。
#[derive(Debug)]
pub struct PayloadAssembler {
    header: PayloadHeader,
    written: u64,
    complete: bool,
}

impl PayloadAssembler {
    pub fn new(header: PayloadHeader) -> Result<Self, Error> {
        if header.kind != PayloadKind::File {
            return Err(Error::new(
                ErrorCode::UnsupportedFeature,
                format!("本 task 只组装 FILE payload（收到 {}）", header.kind.as_wire()),
            )
            .with_phase("transferring"));
        }
        Ok(Self {
            header,
            written: 0,
            complete: false,
        })
    }

    pub fn payload_id(&self) -> i64 {
        self.header.id
    }

    pub fn total_size(&self) -> u64 {
        self.header.total_size
    }

    pub fn written(&self) -> u64 {
        self.written
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// 接受一条数据帧：校验 id / total_size / offset / LAST_CHUNK，然后直接流给 sink。
    pub fn accept(&mut self, frame: &PayloadTransferFrame, sink: &mut dyn ChunkSink) -> Result<(), Error> {
        if self.complete {
            return Err(invalid("payload 已完成，不再接受数据帧"));
        }
        if frame.packet_type != PacketType::Data {
            return Err(Error::new(
                ErrorCode::UnsupportedFeature,
                "非 DATA 帧走控制路径（不交给组装器）",
            )
            .with_phase("transferring"));
        }
        let header = frame
            .header
            .as_ref()
            .ok_or_else(|| invalid("数据帧缺 payload_header"))?;
        if header.id != self.header.id {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                format!(
                    "payload id {} 与本条目 {} 不符：拒绝（禁止把多个 payload 混进同一条目）",
                    header.id, self.header.id
                ),
            )
            .with_phase("transferring"));
        }
        if header.total_size != self.header.total_size {
            return Err(invalid(format!(
                "total_size {} 与已声明 {} 不符",
                header.total_size, self.header.total_size
            )));
        }
        let chunk = frame
            .chunk
            .as_ref()
            .ok_or_else(|| invalid("数据帧缺 payload_chunk"))?;
        if chunk.offset != self.written {
            return Err(invalid(format!(
                "chunk offset {} 与当前长度 {} 不符（只支持顺序写）",
                chunk.offset, self.written
            )));
        }
        let end = self
            .written
            .checked_add(chunk.body.len() as u64)
            .ok_or_else(|| invalid("offset 溢出"))?;
        if end > self.header.total_size {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("写入将超过声明的 total_size {}（拒绝）", self.header.total_size),
            )
            .with_phase("transferring"));
        }
        // LAST_CHUNK 的一致性检查放在**写入之前**：协议违规不产生任何部分副作用
        let fills_payload = end == self.header.total_size;
        if chunk.is_last() && !fills_payload {
            return Err(invalid(format!(
                "LAST_CHUNK 出现在 {end}，但声明总长 {}（不得提前终止；未写入）",
                self.header.total_size
            )));
        }
        if !chunk.is_last() && fills_payload {
            return Err(invalid(
                "已写满 total_size 却缺少 LAST_CHUNK：视为未完成（未写入）",
            ));
        }
        sink.write_chunk(chunk.offset, &chunk.body)?;
        self.written = end;
        if chunk.is_last() {
            self.complete = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let frame = PayloadTransferFrame::data(11, 4, 0, true, b"DATA".to_vec());
        let bytes = encode_payload_transfer_frame(&frame).expect("encode");
        let decoded = decode_payload_transfer_frame(&bytes).expect("decode");
        assert_eq!(decoded, frame);
    }

    #[test]
    fn unknown_packet_type_is_rejected() {
        let mut inner = Vec::new();
        wire::write_varint_field(&mut inner, 1, 99);
        let bytes = encode_v1_offline(&inner).expect("wrap");
        assert_eq!(
            decode_payload_transfer_frame(&bytes).expect_err("未知 packet_type").code,
            ErrorCode::InvalidFrame
        );
    }

    #[test]
    fn assembler_rejects_offset_gaps_and_mixed_ids() {
        let mut a = PayloadAssembler::new(PayloadHeader {
            id: 1,
            kind: PayloadKind::File,
            total_size: 8,
            file_name: None,
            parent_folder: None,
        })
        .expect("assembler");
        struct Count(u64);
        impl ChunkSink for Count {
            fn write_chunk(&mut self, _o: u64, b: &[u8]) -> Result<(), Error> {
                self.0 += b.len() as u64;
                Ok(())
            }
        }
        let mut sink = Count(0);
        assert!(a
            .accept(&PayloadTransferFrame::data(1, 8, 4, false, vec![0; 4]), &mut sink)
            .is_err());
        assert!(a
            .accept(&PayloadTransferFrame::data(2, 8, 0, false, vec![0; 4]), &mut sink)
            .is_err());
        assert!(a
            .accept(&PayloadTransferFrame::data(1, 8, 0, false, vec![0; 4]), &mut sink)
            .is_ok());
        assert!(a
            .accept(&PayloadTransferFrame::data(1, 8, 4, false, vec![0; 4]), &mut sink)
            .is_err(), "写满但缺 LAST_CHUNK 必须拒绝");
        assert_eq!(sink.0, 4);
    }
}
