//! `interop-policy`：策略、授权、token/lease 约束（core 层——无网络栈依赖）。
//!
//! T07：scoped grant 模型（docs/07 §2）：grant 绑定 subject/session/scope/
//! direction/purpose/预算/expiry；默认拒绝跨 scope 请求（authorize 的任何
//! 不匹配都是 auth-denied，没有通配）。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)] // 与 interop-contract 同一决策：域错误 unboxed

pub mod grant;

