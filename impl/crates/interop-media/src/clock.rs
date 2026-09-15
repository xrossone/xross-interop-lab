//! 时钟域（plans/03 T28）：远端 timebase / 本地单调 / wall time **三域分离**。
//!
//! - 远端 PTS 只在 `ClockMap` 的锚点上换算成本地单调毫秒，**不混用 wall time**；
//! - 没有 PTS 就不给时间（`None` 进 `None` 出）——不把 0 当合法时刻；
//! - wall time 只用于显示/日志，不参与同步。

use interop_contract::error::{Error, ErrorCode};

/// 远端 timebase，形如 `1/90000`。**分母 0 非法**（T28-02）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timebase {
    pub numerator: u32,
    pub denominator: u32,
}

impl Timebase {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, Error> {
        if denominator == 0 {
            return Err(invalid("timebase 分母不得为 0"));
        }
        if numerator == 0 {
            return Err(invalid("timebase 分子不得为 0"));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub fn parse(s: &str) -> Result<Self, Error> {
        let (num, den) = s
            .split_once('/')
            .ok_or_else(|| invalid(format!("timebase {s:?} 必须是 <num>/<den>")))?;
        let numerator: u32 = num
            .trim()
            .parse()
            .map_err(|_| invalid(format!("timebase 分子非法：{num:?}")))?;
        let denominator: u32 = den
            .trim()
            .parse()
            .map_err(|_| invalid(format!("timebase 分母非法：{den:?}")))?;
        Self::new(numerator, denominator)
    }

    pub fn as_wire(&self) -> String {
        format!("{}/{}", self.numerator, self.denominator)
    }

    /// ticks → 微秒（i128 中间量防溢出）。
    pub fn ticks_to_micros(&self, ticks: i64) -> i64 {
        let micros = (ticks as i128) * (self.numerator as i128) * 1_000_000
            / (self.denominator as i128);
        micros.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }
}

/// 时钟域标识（诊断/事件里如实标注来源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockDomain {
    /// 远端流的时间戳（按 timebase）
    RemoteTimebase,
    /// 本地单调毫秒（超时/预算/回收用）
    LocalMonotonic,
    /// wall time（只用于显示）
    WallTime,
}

impl ClockDomain {
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::RemoteTimebase => "remote-timebase",
            Self::LocalMonotonic => "local-monotonic",
            Self::WallTime => "wall-time",
        }
    }
}

/// 远端 ticks ↔ 本地单调毫秒的锚定换算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockMap {
    timebase: Timebase,
    anchor_local_ms: u64,
    anchor_remote_ticks: i64,
}

impl ClockMap {
    pub fn new(timebase: Timebase, anchor_local_ms: u64, anchor_remote_ticks: i64) -> Self {
        Self {
            timebase,
            anchor_local_ms,
            anchor_remote_ticks,
        }
    }

    pub fn timebase(&self) -> &Timebase {
        &self.timebase
    }

    pub fn has_pts(&self, pts: Option<i64>) -> bool {
        pts.is_some()
    }

    /// 远端 PTS → 本地单调毫秒。`None` 进 `None` 出（无 PTS 不伪造）。
    pub fn local_ms_for(&self, pts: Option<i64>) -> Option<i64> {
        let pts = pts?;
        let delta_ticks = pts.checked_sub(self.anchor_remote_ticks)?;
        let delta_ms = self.timebase.ticks_to_micros(delta_ticks) / 1000;
        self.anchor_local_ms
            .checked_add_signed(delta_ms)
            .map(|v| v as i64)
    }
}

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("negotiating")
}
