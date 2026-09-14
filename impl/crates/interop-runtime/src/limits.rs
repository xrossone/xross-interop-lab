//! 资源上限（docs/01 §8 / docs/05 §6 的产品预算）。

/// 事件缓冲双上限：条数与总字节（docs/05 §6：4096 条且 ≤16MiB）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventLimits {
    pub max_entries: usize,
    pub max_total_bytes: usize,
}

impl Default for EventLimits {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            max_total_bytes: 16 * 1024 * 1024,
        }
    }
}

impl EventLimits {
    /// 非法配置（0 上限）直接拒绝，避免静默不设防。
    pub fn validate(&self) -> bool {
        self.max_entries > 0 && self.max_total_bytes > 0
    }
}
