//! 时钟代际与漂移统计（plans/03 §T33）。
//!
//! 规矩：
//! - **每次连接一个 clock generation**：重连不得复用上一代的时钟映射（否则时间轴会"穿过"断开点）；
//! - 时钟域分离沿用 `interop-media` 的三域：远端 timebase（`ClockMap`）、本地单调、wall time。这里只做
//!   远端 → 本地单调的映射与漂移采样，不伪造未标注的时间；
//! - 漂移是**观测值**，不是被修正掉的误差：本层只记录 max/mean，修正策略留给播放器。

use interop_contract::error::{Error, ErrorCode};
use interop_media::clock::{ClockMap, Timebase};

/// 时钟代际计数器：每次连接发放一个新的 generation。
#[derive(Debug, Default)]
pub struct ClockGenerations {
    next: u64,
}

impl ClockGenerations {
    pub fn new() -> Self {
        Self { next: 1 }
    }

    /// 发放下一代编号（单调递增，从 1 开始）。
    #[allow(clippy::should_implement_trait)] // 语义是"发号"，不是迭代器
    pub fn next(&mut self) -> u64 {
        let g = self.next;
        self.next += 1;
        g
    }

    /// 已发放数量。
    pub fn count(&self) -> u64 {
        self.next - 1
    }
}

/// 一次漂移采样。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriftSample {
    pub pts_ticks: i64,
    /// 按本代映射推算的本地时刻
    pub expected_local_ms: i64,
    /// 实际本地单调时刻
    pub actual_local_ms: u64,
    pub drift_ms: i64,
}

/// 漂移统计（按代）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriftStats {
    pub generation: u64,
    pub samples: u64,
    pub max_abs_drift_ms: i64,
    pub mean_drift_ms: i64,
}

/// 一代时钟：远端 timebase ↔ 本地单调的映射 + 漂移统计。
#[derive(Debug, Clone)]
pub struct GenerationClock {
    generation: u64,
    map: ClockMap,
    samples: u64,
    sum_drift_ms: i64,
    max_abs_drift_ms: i64,
}

impl GenerationClock {
    /// `anchor_local_ms` ↔ `anchor_remote_ticks` 是本代的锚点（来自 SETUP 协商/首个时间戳）。
    pub fn new(
        generation: u64,
        timebase: Timebase,
        anchor_local_ms: u64,
        anchor_remote_ticks: i64,
    ) -> Result<Self, Error> {
        if generation == 0 {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "clock generation 从 1 开始（0 表示未分配）",
            )
            .with_phase("negotiating"));
        }
        // 触发一次换算以捕捉溢出/非法 timebase（ticks_to_micros 内部用 checked 运算）
        let probe = timebase.ticks_to_micros(anchor_remote_ticks);
        if probe == i64::MIN || probe == i64::MAX {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "锚点时间戳换算溢出：拒绝建立时钟映射",
            )
            .with_phase("negotiating"));
        }
        Ok(Self {
            generation,
            map: ClockMap::new(timebase, anchor_local_ms, anchor_remote_ticks),
            samples: 0,
            sum_drift_ms: 0,
            max_abs_drift_ms: 0,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn timebase(&self) -> &Timebase {
        self.map.timebase()
    }

    /// 远端 → 本地单调（无 PTS 时返回 None，不伪造时间）。
    pub fn local_ms_for(&self, pts_ticks: Option<i64>) -> Option<i64> {
        self.map.local_ms_for(pts_ticks)
    }

    /// 观测一帧：返回漂移采样（无 PTS 时 None，且不计数）。
    pub fn observe(&mut self, pts_ticks: Option<i64>, local_ms: u64) -> Option<DriftSample> {
        let pts = pts_ticks?;
        let expected = self.map.local_ms_for(Some(pts))?;
        let drift = local_ms as i64 - expected;
        self.samples += 1;
        self.sum_drift_ms += drift;
        self.max_abs_drift_ms = self.max_abs_drift_ms.max(drift.abs());
        Some(DriftSample {
            pts_ticks: pts,
            expected_local_ms: expected,
            actual_local_ms: local_ms,
            drift_ms: drift,
        })
    }

    pub fn stats(&self) -> DriftStats {
        DriftStats {
            generation: self.generation,
            samples: self.samples,
            max_abs_drift_ms: self.max_abs_drift_ms,
            mean_drift_ms: if self.samples == 0 {
                0
            } else {
                self.sum_drift_ms / self.samples as i64
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generations_are_monotonic_and_never_reused() {
        let mut g = ClockGenerations::new();
        let ids: Vec<u64> = (0..5).map(|_| g.next()).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
        assert_eq!(g.count(), 5);
    }

    #[test]
    fn drift_is_measured_not_hidden() {
        let tb = Timebase::new(1, 1_000_000).expect("timebase");
        let mut clock = GenerationClock::new(1, tb, 0, 0).expect("clock");
        assert_eq!(clock.generation(), 1);
        // 远端 1000µs → 本地应为 1ms；实际 3ms → drift +2ms
        let sample = clock.observe(Some(1_000), 3).expect("sample");
        assert_eq!(sample.expected_local_ms, 1);
        assert_eq!(sample.drift_ms, 2);
        assert_eq!(clock.stats().max_abs_drift_ms, 2);
        assert_eq!(clock.stats().samples, 1);
        // 无 PTS：不产生采样
        assert!(clock.observe(None, 10).is_none());
        assert_eq!(clock.stats().samples, 1);
    }

    #[test]
    fn zero_generation_is_rejected() {
        let tb = Timebase::new(1, 1_000_000).expect("timebase");
        assert_eq!(
            GenerationClock::new(0, tb, 0, 0).expect_err("generation 0").code,
            ErrorCode::InvalidFrame
        );
    }
}
