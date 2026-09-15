//! 媒体边界（字段表 F-28/F-29）：RTP 固定头解析与序号记账。
//!
//! 本层**只到记账**：不解 MPEG-TS、不解码、不渲染（F-28/媒体引擎范围外）。
//! 丢包后是否请求关键帧是**本仓策略**（F-29）——字段表只证明 M13 消息存在，
//! 不规定何时发；这里是我们的选择，常量上都标了"本仓策略"。

use interop_contract::error::{Error, ErrorCode};

/// RTP 固定头长度（F-28）。
pub const RTP_FIXED_HEADER_BYTES: usize = 12;
/// CSRC 每项 4 字节。
const CSRC_BYTES: usize = 4;

/// 丢包达到该数量即请求关键帧（**本仓策略**，F-29）。
pub const KEYFRAME_REQUEST_LOSS_THRESHOLD: u32 = 1;
/// 同一原因重复请求关键帧的最小间隔（**本仓策略**）：避免丢包风暴里刷屏。
pub const KEYFRAME_REQUEST_DEBOUNCE_MS: u64 = 200;

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

/// RTP 固定头（F-28：版本位、payload type、M 位、seq/ts/SSRC）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtpHeader {
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    /// 固定头 + CSRC + （可选的）扩展头字节数；载荷从这里开始。
    pub header_len: usize,
    pub payload_len: usize,
}

impl RtpHeader {
    /// F-28：payload type 33 = MPEG-2 TS（来源取值；动态 PT 不在字段表内，故只作观察）。
    pub fn is_mpegts(&self) -> bool {
        self.payload_type == crate::MPEGTS_PAYLOAD_TYPE
    }
}

/// 解析 RTP 头（版本必须是 2，F-28：来源发的是 `0x80` 起头的固定头）。
pub fn parse_header(buf: &[u8]) -> Result<RtpHeader, Error> {
    if buf.len() < RTP_FIXED_HEADER_BYTES {
        return Err(invalid(format!(
            "RTP 头不足 {RTP_FIXED_HEADER_BYTES} 字节：{}",
            buf.len()
        )));
    }
    let version = buf[0] >> 6;
    if version != 2 {
        return Err(invalid(format!("RTP 版本必须为 2（F-28），收到 {version}")));
    }
    let padding = buf[0] & 0x20 != 0;
    let extension = buf[0] & 0x10 != 0;
    let csrc_count = buf[0] & 0x0f;
    let marker = buf[1] & 0x80 != 0;
    let payload_type = buf[1] & 0x7f;
    let sequence = u16::from_be_bytes([buf[2], buf[3]]);
    let timestamp = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let ssrc = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);

    let mut header_len = RTP_FIXED_HEADER_BYTES + usize::from(csrc_count) * CSRC_BYTES;
    if extension {
        if buf.len() < header_len + 4 {
            return Err(invalid("RTP 扩展头声明了扩展位但缺扩展长度字段"));
        }
        let words = u16::from_be_bytes([buf[header_len + 2], buf[header_len + 3]]) as usize;
        header_len += 4 + words * 4;
    }
    if buf.len() < header_len {
        return Err(invalid("RTP 头长度超过实际字节（截断）"));
    }
    let mut payload_len = buf.len() - header_len;
    if padding {
        if payload_len == 0 {
            return Err(invalid("RTP 置了 padding 位但没有载荷可取填充长度"));
        }
        let pad = usize::from(buf[buf.len() - 1]);
        if pad == 0 || pad > payload_len {
            return Err(invalid(format!("RTP padding 长度非法：{pad}")));
        }
        payload_len -= pad;
    }
    Ok(RtpHeader {
        version,
        padding,
        extension,
        marker,
        payload_type,
        sequence,
        timestamp,
        ssrc,
        header_len,
        payload_len,
    })
}

/// 序号记账（F-28/F-29）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamAccount {
    expected: Option<u16>,
    ssrc: Option<u32>,
    pub packets: u64,
    pub payload_bytes: u64,
    pub lost: u32,
    pub duplicates: u32,
    pub reordered: u32,
    pub payload_type_anomalies: u32,
    pub ssrc_changes: u32,
    pub keyframe_requests: u32,
    last_keyframe_request_ms: Option<u64>,
    /// 自上次请求关键帧以来累计的丢包数（本仓策略的输入）。
    loss_since_request: u32,
}

impl StreamAccount {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ssrc(&self) -> Option<u32> {
        self.ssrc
    }

    /// 记账一个 RTP 包。返回 `true` 表示**本次**应请求关键帧（本仓策略，F-29）。
    pub fn on_packet(&mut self, header: &RtpHeader, now_ms: u64) -> bool {
        self.packets += 1;
        self.payload_bytes += header.payload_len as u64;
        if !header.is_mpegts() {
            self.payload_type_anomalies += 1;
        }
        match self.ssrc {
            Some(known) if known != header.ssrc => {
                self.ssrc_changes += 1;
                self.ssrc = Some(header.ssrc);
                self.expected = None;
                self.loss_since_request = self.loss_since_request.saturating_add(1);
                return self.maybe_request_keyframe(now_ms);
            }
            None => self.ssrc = Some(header.ssrc),
            _ => {}
        }

        match self.expected {
            None => {
                self.expected = Some(header.sequence.wrapping_add(1));
                false
            }
            Some(expected) => {
                if header.sequence == expected {
                    self.expected = Some(expected.wrapping_add(1));
                    false
                } else {
                    let delta = header.sequence.wrapping_sub(expected) as i16;
                    if delta > 0 {
                        // 丢包（序号前跳）。
                        self.lost += u32::from(delta as u16);
                        self.loss_since_request =
                            self.loss_since_request.saturating_add(u32::from(delta as u16));
                        self.expected = Some(header.sequence.wrapping_add(1));
                        self.maybe_request_keyframe(now_ms)
                    } else {
                        // 序号回退：乱序或重复。回退 1 个序号视为重复。
                        if delta == -1 {
                            self.duplicates += 1;
                        } else {
                            self.reordered += 1;
                        }
                        // 乱序不触发关键帧请求（F-29：策略只对"缺口"反应）。
                        false
                    }
                }
            }
        }
    }

    fn maybe_request_keyframe(&mut self, now_ms: u64) -> bool {
        if self.loss_since_request < KEYFRAME_REQUEST_LOSS_THRESHOLD {
            return false;
        }
        if let Some(last) = self.last_keyframe_request_ms {
            if now_ms.saturating_sub(last) < KEYFRAME_REQUEST_DEBOUNCE_MS {
                return false;
            }
        }
        self.keyframe_requests += 1;
        self.last_keyframe_request_ms = Some(now_ms);
        self.loss_since_request = 0;
        true
    }

    /// 关键帧到达（`marker` 位或外部告知）→ 清掉待补状态。
    pub fn on_keyframe(&mut self) {
        self.loss_since_request = 0;
    }

    /// 当前是否有未补的缺口（供统计与演示展示）。
    pub fn has_pending_loss(&self) -> bool {
        self.loss_since_request > 0
    }
}
