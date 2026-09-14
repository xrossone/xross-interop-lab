//! 长度前缀 frame codec：u32 BE + payload；上限 256 KiB；0/超限在分配前拒绝。

use interop_contract::error::{Error, ErrorCode};
use std::io::Read;

/// docs/01 §8 / docs/05 §6：IPC control frame 最大 256 KiB。
pub const MAX_FRAME: usize = 262_144;

/// 编码一帧到 out（4 字节 BE 长度 + payload）。
pub fn encode_frame(payload: &[u8], out: &mut Vec<u8>) {
    assert!(
        payload.len() <= MAX_FRAME,
        "payload 超过 MAX_FRAME：编码侧契约违规"
    );
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
}

/// 帧读取器（包装任意 Read，含最恶劣的逐字节分片 reader）。
pub struct FrameReader<R: Read> {
    reader: R,
}

impl<R: Read> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// 读一帧；返回 `Ok(None)` 表示对端在帧边界干净关闭。
    pub fn read_frame(&mut self) -> Result<Option<Vec<u8>>, Error> {
        read_frame(&mut self.reader)
    }
}

/// 自由函数形式（interopd 使用）。
pub fn read_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>, Error> {
    let mut header = [0u8; 4];
    fill_exact(reader, &mut header)?;
    let len = u32::from_be_bytes(header) as usize;
    // 分配前检查：0 长度与超限直接拒绝（T09-02；不发分配）
    if len == 0 {
        return Err(Error::new(ErrorCode::InvalidFrame, "0 长度帧被拒绝"));
    }
    if len > MAX_FRAME {
        return Err(Error::new(
            ErrorCode::InvalidFrame,
            format!("帧长度 {len} 超过上限 {MAX_FRAME}（分配前拒绝）"),
        ));
    }
    let mut body = vec![0u8; len]; // 此时 len ≤ 256KiB，分配有界
    fill_exact(reader, &mut body)?;
    Ok(Some(body))
}

/// read_exact 的错误映射版（read_exact 内部循环，天然容忍逐字节分片）。
fn fill_exact(reader: &mut impl Read, buf: &mut [u8]) -> Result<(), Error> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                return Err(Error::new(
                    ErrorCode::InvalidFrame,
                    "对端在帧中途关闭连接",
                ))
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(Error::new(
                    ErrorCode::InvalidFrame,
                    format!("读帧 I/O 错误：{e}"),
                ))
            }
        }
    }
    Ok(())
}
