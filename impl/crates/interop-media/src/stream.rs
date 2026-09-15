//! 流状态（plans/03 T28）：format 变化校验 + 有界帧队列。
//!
//! **format 变化必须伴随 decoder reset**：sender 旋转屏幕/换分辨率时会重发 codec config 并把
//! `format_id` 递增——接收侧若看到 `format_id` 变了却没有 discontinuity 信号，说明要么 sender 说谎、
//! 要么中间丢了 config 帧；两种情况都必须拒绝（继续解码会用错配置，产出花屏/崩溃）。

use interop_contract::error::{Error, ErrorCode};
use interop_ipc::media_frame::MediaFrame;
use std::collections::VecDeque;

/// codec config 观察结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatEvent {
    /// 首个 config
    Initialized,
    /// 同 format_id 重发（合法：sender 周期性重发）
    Unchanged,
    /// 新 format_id + reset 信号（合法：旋转/切换）
    Reset,
}

/// 追踪当前 format，强制"变化必伴随 reset"。
#[derive(Debug, Default)]
pub struct FormatTracker {
    current: Option<u32>,
}

impl FormatTracker {
    pub fn new() -> Self {
        Self { current: None }
    }

    pub fn current_format_id(&self) -> Option<u32> {
        self.current
    }

    /// 观察一帧 codec config。拒绝时**状态不推进**。
    pub fn observe_config(
        &mut self,
        format_id: u32,
        discontinuity: bool,
    ) -> Result<FormatEvent, Error> {
        match self.current {
            None => {
                self.current = Some(format_id);
                Ok(FormatEvent::Initialized)
            }
            Some(cur) if cur == format_id => Ok(FormatEvent::Unchanged),
            Some(_) if discontinuity => {
                self.current = Some(format_id);
                Ok(FormatEvent::Reset)
            }
            Some(cur) => Err(Error::new(
                ErrorCode::InvalidFrame,
                format!(
                    "format_id {cur}→{format_id} 变化缺少 decoder reset（discontinuity）：拒绝继续解码"
                ),
            )
            .with_phase("negotiating")),
        }
    }
}

/// 有界帧队列：满即显式 `resource-limit`（背压交给上游暂停读取，不静默丢帧）。
#[derive(Debug)]
pub struct FrameQueue {
    max_frames: usize,
    max_bytes: usize,
    frames: VecDeque<MediaFrame>,
    bytes: usize,
}

impl FrameQueue {
    pub fn new(max_frames: usize, max_bytes: usize) -> Self {
        Self {
            max_frames,
            max_bytes,
            frames: VecDeque::new(),
            bytes: 0,
        }
    }

    pub fn push(&mut self, frame: MediaFrame) -> Result<(), Error> {
        let size = frame.payload.len();
        if self.frames.len() >= self.max_frames || self.bytes + size > self.max_bytes {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!(
                    "帧队列已满（frames {}/{}, bytes {}+{} > {}）：背压给上游",
                    self.frames.len(),
                    self.max_frames,
                    self.bytes,
                    size,
                    self.max_bytes
                ),
            )
            .with_phase("transferring"));
        }
        self.bytes += size;
        self.frames.push_back(frame);
        Ok(())
    }

    pub fn pop(&mut self) -> Option<MediaFrame> {
        let frame = self.frames.pop_front()?;
        self.bytes -= frame.payload.len();
        Some(frame)
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }
}
