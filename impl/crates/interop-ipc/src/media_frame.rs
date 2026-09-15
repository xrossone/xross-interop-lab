//! XMD1 数据面帧（docs/05 §7；plans/03 T28）。
//!
//! 固定 **36 字节**头：`magic[4]="XMD1" / header_version u16 / kind u16 / stream_id u32 /
//! flags u32 / sequence u64 / pts i64 / payload_len u32`，随后是 payload。
//!
//! 规矩（fail-closed）：
//! - **长度检查先于分配**：声明 payload 超过 16MiB 直接 `resource-limit`，不先分配再判断；
//! - 未知 critical flag 位 → `invalid-frame`（不忽略、不猜测）；
//! - 未知 kind / 坏 magic / 截断 → 明确拒绝；只支持 header_version=1。
//!
//! 裸 pointer/FD 整数**不是**可跨进程的授权（docs/05 §7）：native 呈现只经 opaque id。

use interop_contract::error::{Error, ErrorCode};

/// 固定头长度（字节）。
pub const HEADER_LEN: usize = 36;
/// payload 上限：16 MiB（分配前检查）。
pub const MAX_PAYLOAD: u32 = 16 * 1024 * 1024;
/// 唯一支持的 header 版本。
pub const HEADER_VERSION: u16 = 1;

/// 已定义 flag 位。未知 critical 位一律拒绝（前向兼容走版本协商，不靠忽略）。
pub mod flags {
    pub const KEYFRAME: u32 = 0x1;
    pub const DISCONTINUITY: u32 = 0x2;
    /// 本帧没有 PTS（pts 字段无意义——不得当成 0 时刻）
    pub const NO_PTS: u32 = 0x4;
    /// 本版本已知的全部位；其余位 = 未知 critical。
    pub const KNOWN: u32 = KEYFRAME | DISCONTINUITY | NO_PTS;
}

/// 帧类型（docs/05 §7 的 kind）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// 1：encoded access unit
    EncodedAccessUnit,
    /// 2：PCM block
    PcmBlock,
    /// 3：codec config（format 变化的载体）
    CodecConfig,
    /// 4：end（流结束）
    End,
}

impl FrameKind {
    pub fn from_u16(v: u16) -> Result<Self, Error> {
        match v {
            1 => Ok(Self::EncodedAccessUnit),
            2 => Ok(Self::PcmBlock),
            3 => Ok(Self::CodecConfig),
            4 => Ok(Self::End),
            other => Err(invalid(format!("未知 XMD1 kind {other}：fail-closed，不猜测"))),
        }
    }

    pub fn as_u16(&self) -> u16 {
        match self {
            Self::EncodedAccessUnit => 1,
            Self::PcmBlock => 2,
            Self::CodecConfig => 3,
            Self::End => 4,
        }
    }

    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::EncodedAccessUnit => "encoded-access-unit",
            Self::PcmBlock => "pcm-block",
            Self::CodecConfig => "codec-config",
            Self::End => "end",
        }
    }
}

/// payload 长度闸门：编码与解码两侧共用（不靠调用方自觉）。
pub fn check_payload_len(len: usize) -> Result<(), Error> {
    if len > MAX_PAYLOAD as usize {
        return Err(Error::new(
            ErrorCode::ResourceLimit,
            format!("声明 payload {len} 字节超过上限 {MAX_PAYLOAD}（分配前拒绝）"),
        )
        .with_phase("transferring"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFrame {
    pub kind: FrameKind,
    pub stream_id: u32,
    pub flags: u32,
    pub sequence: u64,
    /// 按 descriptor 的 timebase；`NO_PTS` 位置位时无意义
    pub pts: i64,
    pub payload: Vec<u8>,
}

impl MediaFrame {
    pub fn has_flag(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    pub fn no_pts(&self) -> bool {
        self.has_flag(flags::NO_PTS)
    }

    /// 有 PTS 才返回时间；`NO_PTS` 位置位时返回 `None`（**不把 0 当合法时刻**）。
    pub fn pts_if_present(&self) -> Option<i64> {
        if self.no_pts() {
            None
        } else {
            Some(self.pts)
        }
    }

    /// 编码。payload 已在内存里，超过上限属编程错误 → panic（要错误码请用 [`Self::try_encode`]）。
    pub fn encode(&self) -> Vec<u8> {
        self.try_encode()
            .expect("MediaFrame::encode 超限：payload 已在内存中，属调用方错误")
    }

    pub fn try_encode(&self) -> Result<Vec<u8>, Error> {
        check_payload_len(self.payload.len())?;
        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(b"XMD1");
        out.extend_from_slice(&HEADER_VERSION.to_be_bytes());
        out.extend_from_slice(&self.kind.as_u16().to_be_bytes());
        out.extend_from_slice(&self.stream_id.to_be_bytes());
        out.extend_from_slice(&self.flags.to_be_bytes());
        out.extend_from_slice(&self.sequence.to_be_bytes());
        out.extend_from_slice(&self.pts.to_be_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    /// 解码一帧，返回 `(frame, consumed)`；粘包场景由调用方推进 offset。
    pub fn decode(buf: &[u8]) -> Result<(Self, usize), Error> {
        if buf.len() < HEADER_LEN {
            return Err(invalid(format!(
                "不足一个 XMD1 头（{} < {HEADER_LEN}）",
                buf.len()
            )));
        }
        if &buf[..4] != b"XMD1" {
            return Err(invalid("XMD1 magic 不匹配"));
        }
        let version = u16::from_be_bytes([buf[4], buf[5]]);
        if version != HEADER_VERSION {
            return Err(Error::new(
                ErrorCode::VersionUnsupported,
                format!("XMD1 header 版本 {version} 不支持（只支持 {HEADER_VERSION}）"),
            ));
        }
        let kind = FrameKind::from_u16(u16::from_be_bytes([buf[6], buf[7]]))?;
        let stream_id = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let flags = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
        if flags & !flags::KNOWN != 0 {
            return Err(invalid(format!(
                "未知 critical flag 位 0x{:08x}（已知 0x{:08x}）",
                flags & !flags::KNOWN,
                flags::KNOWN
            )));
        }
        let sequence = u64::from_be_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]);
        let pts = i64::from_be_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]);
        let payload_len = u32::from_be_bytes([buf[32], buf[33], buf[34], buf[35]]);
        // 分配前检查（T28-01）
        check_payload_len(payload_len as usize)?;
        let total = HEADER_LEN + payload_len as usize;
        if buf.len() < total {
            return Err(invalid(format!(
                "payload 截断：声明 {payload_len} 字节，实际只有 {}",
                buf.len() - HEADER_LEN
            )));
        }
        let payload = buf[HEADER_LEN..total].to_vec();
        Ok((
            Self {
                kind,
                stream_id,
                flags,
                sequence,
                pts,
                payload,
            },
            total,
        ))
    }
}

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("transferring")
}
