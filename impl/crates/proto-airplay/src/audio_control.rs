//! 音频路径的两条控制请求（T36）：`/audioMode` 与 `/feedback` 的**形状校验与观测记账**。
//!
//! 字段来源见 `specs-reviewed/m01-airplay-legacy-mirror.md` 的「T36 增量」（实测 2026-09-15，
//! 记录在 `docs/research/airplay-research.md:157`）。**本模块刻意只做两件事**：
//!
//! 1. **形状校验**：方法/路径/Content-Type/键名（`mode`、`elapsed_ms`）；
//! 2. **观测记账**：`/feedback` 的到达间隔与 `elapsed_ms` 单调性——**只记录观测**。
//!
//! **不做什么**（语义未固化，做了就是编）：不把 `/feedback` 当心跳、不据 `elapsed_ms` 推任何时钟或
//! 播放位置。实测记录写的是「这是请求处理耗时字段被复用的痕迹，**待考**」——在真机对跑把语义钉死之前，
//! 本模块只回答"观测到了什么"。lab gate 机器检查本文件里不出现 clock/playback_position 之类的推断。
//!
//! **plist 体按不透明处理**：允许的来源清单里没有 bplist 的字节级规格（T36 行已标注待固化），
//! 因此键名由调用方提取后传入（与配对路径同一处置）。

use interop_contract::error::{Error, ErrorCode};
use std::collections::BTreeSet;

/// 配对/音频控制请求共用的 Content-Type（T36 行：与配对路径同类）。
pub const BINARY_PLIST_CONTENT_TYPE: &str = "application/x-apple-binary-plist";
/// `/feedback` 的实测到达间隔（**来源取值**，不是规范常量）。
pub const OBSERVED_FEEDBACK_INTERVAL_MS: u64 = 2_000;
/// 观测间隔偏离实测值的容差（**本仓策略**：只用于报告，不用于拒绝）。
pub const FEEDBACK_INTERVAL_TOLERANCE_MS: u64 = 500;

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn unsupported(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, msg)
}

/// 音频路径的两条控制请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioControlRequest {
    /// `POST /audioMode`：请求体含键 `mode`（实测取值 `"default"`）。
    AudioMode,
    /// `POST /feedback`：请求体含键 `elapsed_ms`（实测单调增，语义待考）。
    Feedback,
}

impl AudioControlRequest {
    /// 从路径解析（未知路径 → `None`，调用方按 not-found 处理）。
    pub fn from_path(path: &str) -> Option<Self> {
        match path {
            "/audioMode" => Some(Self::AudioMode),
            "/feedback" => Some(Self::Feedback),
            _ => None,
        }
    }

    pub fn path(self) -> &'static str {
        match self {
            Self::AudioMode => "/audioMode",
            Self::Feedback => "/feedback",
        }
    }

    /// 该请求**允许出现**的键（其余键一律拒绝：未固化的键不猜）。
    pub fn allowed_keys(self) -> &'static [&'static str] {
        match self {
            Self::AudioMode => &["mode"],
            Self::Feedback => &["elapsed_ms"],
        }
    }

    /// 实测观察到的 `mode` 取值（**只有这一个**；不得据此声称别的取值被支持）。
    pub const OBSERVED_MODE_DEFAULT: &'static str = "default";

    /// 形状校验：方法必须是 POST、路径必须匹配、Content-Type 必须是 binary plist、
    /// 键必须恰好落在 [`Self::allowed_keys`] 内且必需键齐全。
    ///
    /// 返回该请求携带的键集合（调用方据此做进一步语义判断）。
    pub fn check_shape(
        self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        keys: &[&str],
    ) -> Result<AudioControlShape, Error> {
        if !method.eq_ignore_ascii_case("POST") {
            return Err(invalid(format!(
                "{} 只观察到 POST（实测），收到 {method}",
                self.path()
            )));
        }
        if path != self.path() {
            return Err(invalid(format!("路径不匹配：期望 {}，收到 {path}", self.path())));
        }
        match content_type {
            Some(value) if value.eq_ignore_ascii_case(BINARY_PLIST_CONTENT_TYPE) => {}
            Some(other) => {
                return Err(invalid(format!(
                    "Content-Type 必须是 {BINARY_PLIST_CONTENT_TYPE}（与配对路径同类），收到 {other}"
                )))
            }
            None => {
                return Err(invalid(format!(
                    "{} 缺 Content-Type（必须是 {BINARY_PLIST_CONTENT_TYPE}）",
                    self.path()
                )))
            }
        }
        let allowed: BTreeSet<&str> = self.allowed_keys().iter().copied().collect();
        for key in keys {
            if !allowed.contains(key) {
                return Err(invalid(format!(
                    "{} 出现未登记的键 {key}（字段表只登记 {:?}）",
                    self.path(),
                    self.allowed_keys()
                )));
            }
        }
        for required in self.allowed_keys() {
            if !keys.contains(required) {
                return Err(invalid(format!("{} 缺必需键 {required}", self.path())));
            }
        }
        Ok(AudioControlShape {
            request: self,
            keys: keys.iter().map(|k| (*k).to_string()).collect(),
        })
    }
}

/// 形状校验通过后的请求（键集合原样保留）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioControlShape {
    pub request: AudioControlRequest,
    pub keys: Vec<String>,
}

impl AudioControlShape {
    /// `mode` 的取值是否落在实测观测到的集合里（只有 `"default"`）。
    ///
    /// 不在这里拒绝其它取值——**没有来源**说其它取值非法，只报告"超出实测观测"。
    pub fn mode_is_observed(&self, mode: &str) -> bool {
        mode == AudioControlRequest::OBSERVED_MODE_DEFAULT
    }
}

/// `/feedback` 的**观测**记账（语义未固化：只记录观测到的事实）。
#[derive(Debug, Clone, Default)]
pub struct FeedbackObservation {
    samples: u64,
    intervals_ms: Vec<u64>,
    monotonic: bool,
    last_value: Option<u64>,
    last_arrival_ms: Option<u64>,
    worst_interval_ms: u64,
    deviations_from_observed: u64,
}

impl FeedbackObservation {
    pub fn new() -> Self {
        Self {
            monotonic: true,
            ..Self::default()
        }
    }

    /// 记一次到达：`elapsed_ms` 是请求体里的值，`now_ms` 是到达时刻（调用方给）。
    ///
    /// **只记账**：单调性、到达间隔、与实测 2 s 的偏离。不做任何时钟推断。
    pub fn observe(&mut self, elapsed_ms: u64, now_ms: u64) {
        self.samples += 1;
        if let Some(previous) = self.last_value {
            if elapsed_ms < previous {
                self.monotonic = false;
            }
        }
        self.last_value = Some(elapsed_ms);
        if let Some(previous_arrival) = self.last_arrival_ms {
            let interval = now_ms.saturating_sub(previous_arrival);
            self.intervals_ms.push(interval);
            self.worst_interval_ms = self.worst_interval_ms.max(interval);
            let observed = OBSERVED_FEEDBACK_INTERVAL_MS;
            let tolerance = FEEDBACK_INTERVAL_TOLERANCE_MS;
            if interval.abs_diff(observed) > tolerance {
                self.deviations_from_observed += 1;
            }
        }
        self.last_arrival_ms = Some(now_ms);
    }

    pub fn samples(&self) -> u64 {
        self.samples
    }

    pub fn intervals_ms(&self) -> &[u64] {
        &self.intervals_ms
    }

    /// `elapsed_ms` 是否单调不减（实测如此）。**不代表任何语义**。
    pub fn monotonic(&self) -> bool {
        self.monotonic
    }

    pub fn deviations_from_observed(&self) -> u64 {
        self.deviations_from_observed
    }

    /// 报告的语义标注（必须随观测一起给出）。
    pub fn semantics_note(&self) -> &'static str {
        "elapsed_ms 的语义**待考**（实测记录：疑为请求处理耗时字段被复用）：本仓只记录观测，不实现心跳语义、不据此推时钟"
    }
}

/// 对外部 `elapsed_ms` 值的**保守**校验：只要求能放进 u64 且不是负数（plist 可能给 i64）。
pub fn parse_elapsed_ms(raw: i64) -> Result<u64, Error> {
    if raw < 0 {
        return Err(unsupported(format!(
            "{raw} 是负数：字段表只观察到单调增的非负值（语义待考），负值一律不接受"
        )));
    }
    Ok(raw as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_accepts_the_observed_forms() {
        let shape = AudioControlRequest::AudioMode
            .check_shape("POST", "/audioMode", Some(BINARY_PLIST_CONTENT_TYPE), &["mode"])
            .expect("实测形态");
        assert_eq!(shape.request, AudioControlRequest::AudioMode);
        assert!(shape.mode_is_observed("default"));
        assert!(!shape.mode_is_observed("lowLatency"), "没有来源说别的取值被支持");
        assert_eq!(AudioControlRequest::from_path("/feedback"), Some(AudioControlRequest::Feedback));
        assert!(AudioControlRequest::from_path("/other").is_none());
    }

    #[test]
    fn shape_rejections_are_explicit() {
        let cases = [
            ("GET", "/audioMode", Some(BINARY_PLIST_CONTENT_TYPE), vec!["mode"]),
            ("POST", "/audioModeX", Some(BINARY_PLIST_CONTENT_TYPE), vec!["mode"]),
            ("POST", "/audioMode", Some("application/octet-stream"), vec!["mode"]),
            ("POST", "/audioMode", None, vec!["mode"]),
            ("POST", "/audioMode", Some(BINARY_PLIST_CONTENT_TYPE), vec![]),
            ("POST", "/audioMode", Some(BINARY_PLIST_CONTENT_TYPE), vec!["mode", "extra"]),
        ];
        for (method, path, ct, keys) in cases {
            let err = AudioControlRequest::AudioMode
                .check_shape(method, path, ct, &keys)
                .expect_err("必须拒绝");
            assert_eq!(err.code, ErrorCode::InvalidFrame, "{method} {path} {ct:?} {keys:?}");
        }
    }

    #[test]
    fn feedback_observation_records_without_interpreting() {
        let mut observation = FeedbackObservation::new();
        assert!(observation.monotonic(), "空观测视为单调");
        observation.observe(1_000, 10_000);
        observation.observe(1_500, 12_000);
        observation.observe(2_100, 14_100);
        assert_eq!(observation.samples(), 3);
        assert!(observation.monotonic());
        assert_eq!(observation.intervals_ms(), &[2_000, 2_100]);
        assert_eq!(observation.deviations_from_observed(), 0, "2.0/2.1 s 都在容差内");
        // 值回退 → 单调性变假（只记录，不报错）；间隔 3.0 s 才明显偏离实测的 2 s
        // （容差 500 ms ⇒ 2.4 s 仍在容差内，3.0 s 不是）。
        observation.observe(2_050, 17_100);
        assert!(!observation.monotonic());
        assert_eq!(observation.intervals_ms(), &[2_000, 2_100, 3_000]);
        assert_eq!(observation.deviations_from_observed(), 1, "3.0 s 超出容差");
        // 语义标注必须随观测给出。
        assert!(observation.semantics_note().contains("待考"));
    }

    #[test]
    fn elapsed_ms_negatives_are_refused() {
        assert_eq!(parse_elapsed_ms(0).expect("非负"), 0);
        assert_eq!(parse_elapsed_ms(1234).expect("非负"), 1234);
        assert_eq!(
            parse_elapsed_ms(-1).unwrap_err().code,
            ErrorCode::UnsupportedFeature
        );
    }
}
