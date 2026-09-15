//! TCP framing（plans/02-files.md §T20；字段表 F-05）。
//!
//! 事实：Quick Share 在 TCP 上每条消息前缀 4 字节**大端**长度（R15 `NearbyShare/NearbyConnection.swift:117`）。
//! 上限：R15 用 `SANE_FRAME_LENGTH = 5 MiB` 做读前检查——那是**实现选择**，不是协议常量，
//! 所以这里把它命名为本仓默认上限并在文档里标明出处，允许调用方调整。
//!
//! 分片无关（T20-04）：解码器只按字节流工作，任意切分都会得到同一条消息。

use interop_contract::error::{Error, ErrorCode};

/// 本仓默认的帧上限（来源：R15 `SANE_FRAME_LENGTH`；**非协议常量**，F-05）。
pub const DEFAULT_MAX_FRAME_BYTES: u32 = 5 * 1024 * 1024;

/// 长度前缀宽度（F-05：4 字节大端）。
pub const LENGTH_PREFIX_BYTES: usize = 4;

/// 字节流 → 完整消息。内部缓冲在获得完整帧前不复制负载以外的内容。
pub struct FrameDecoder {
    max_frame_bytes: u32,
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new(max_frame_bytes: u32) -> Self {
        Self {
            max_frame_bytes,
            buf: Vec::new(),
        }
    }

    pub fn buffered_bytes(&self) -> usize {
        self.buf.len()
    }

    /// 喂入任意长度的片段，返回本次能取出的完整消息（可能 0 条）。
    /// 长度检查发生在**读取之前**：超限直接拒绝，不会为它分配内存。
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        loop {
            if self.buf.len() < LENGTH_PREFIX_BYTES {
                break;
            }
            let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]);
            if len == 0 {
                return Err(Error::new(
                    ErrorCode::InvalidFrame,
                    "帧长度 0：空消息无意义（fail-closed）",
                )
                .with_phase("negotiating"));
            }
            if len > self.max_frame_bytes {
                return Err(Error::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "帧声明 {len} 字节超过上限 {}（读取前拒绝；上限非协议常量，见 F-05）",
                        self.max_frame_bytes
                    ),
                )
                .with_phase("negotiating"));
            }
            let total = LENGTH_PREFIX_BYTES + len as usize;
            if self.buf.len() < total {
                break;
            }
            let frame = self.buf[LENGTH_PREFIX_BYTES..total].to_vec();
            self.buf.drain(..total);
            out.push(frame);
        }
        Ok(out)
    }
}

/// 编码一条帧（4 字节大端长度 + 消息体）。
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip_and_split_points() {
        let frame = encode_frame(b"hello");
        for cut in 0..=frame.len() {
            let mut d = FrameDecoder::new(1024);
            let mut got = d.push(&frame[..cut]).expect("前半");
            got.extend(d.push(&frame[cut..]).expect("后半"));
            assert_eq!(got, vec![b"hello".to_vec()], "切点 {cut}");
        }
    }

    #[test]
    fn two_frames_in_one_push_are_both_returned() {
        let mut d = FrameDecoder::new(1024);
        let stream = [encode_frame(b"a"), encode_frame(b"bb")].concat();
        let got = d.push(&stream).expect("两条");
        assert_eq!(got.len(), 2);
        assert_eq!(d.buffered_bytes(), 0);
    }

    #[test]
    fn oversize_is_rejected_before_reading() {
        let mut d = FrameDecoder::new(16);
        let mut bytes = 17u32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&[0u8; 4]);
        assert_eq!(d.push(&bytes).expect_err("超限").code, ErrorCode::ResourceLimit);
        assert_eq!(d.buffered_bytes(), bytes.len(), "拒绝时没有消费缓冲");
    }
}
