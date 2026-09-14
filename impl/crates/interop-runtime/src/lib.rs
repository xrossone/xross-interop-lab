//! `interop-runtime`：会话/事件/路由/预算编排（orchestrator 层）。
//!
//! T07：HostPorts 与内存 fake host（真实 tokio 编排在 T10 引入；
//! 不持有 Xross 账号，不创建 Iroh 实例——INT-02）。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)] // 与 interop-contract 同一决策：域错误 unboxed

pub mod host;
