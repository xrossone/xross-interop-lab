//! `interop-testkit`：conformance testkit 与证据记录（plans/01 T14）。
//!
//! - [`clock`]：注入式 fake clock（所有 timeout/deadline 语义用单调毫秒）。
//! - [`peer`]：确定性分片/重排注入（固定 seed 可重复；xorshift64* 无外部依赖）。
//! - [`evidence`]：fixture 登记（生成算法 + SHA-256，禁止静默重录）、
//!   run manifest 构建（simulated/device/manual 分级不可僭越）、日志脱敏检查。
//!
//! fixture 均为自制内容（记录生成算法与 hash），不复制他人测试向量。

// 与 workspace 同一决策：域错误 unboxed
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod clock;
pub mod evidence;
pub mod peer;
