//! 注入式假时钟：测试中全部 timeout/deadline 用单调毫秒驱动。

/// 单调毫秒假时钟（起点可设，只前进）。
#[derive(Debug, Clone)]
pub struct FakeClock {
    now_ms: u64,
}

impl FakeClock {
    pub fn new(start_ms: u64) -> Self {
        Self { now_ms: start_ms }
    }

    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// 前进指定毫秒（0 是合法 no-op；不允许倒流——没有回退 API）。
    pub fn advance(&mut self, delta_ms: u64) {
        self.now_ms = self.now_ms.saturating_add(delta_ms);
    }
}
