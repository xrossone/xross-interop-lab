//! `interop-runtime`：会话/事件/路由/预算编排（orchestrator 层）。
//!
//! - [`host`]：HostPorts 与内存 fake host（T07）。
//! - [`session`]：会话注册表——唯一裁决 authority，终态不可逆、取消幂等、
//!   worker 崩溃隔离（T10；docs/05 §4）。
//! - [`events`]：instance_id+sequence 事件流，4096 条/16MiB 双上限，
//!   淘汰 cursor 返回 gap+快照建议（CORE-07）。
//! - [`limits`]：资源上限常量与校验。
//! - [`endpoints`]：endpoint registry——观察去重/过期，**永不合并 identity**（T11）。
//! - [`routing`]：purpose/能力/安全/平台状态四项检查与稳定选路（T11）。
//! - [`workers`]：worker supervisor——binary hash pin、参数/环境白名单、有限重启、
//!   关闭无孤儿，以及如实标注的隔离等级（T13）。
//!
//! 不持有 Xross 账号，不创建 Iroh 实例——INT-02。

// 与 workspace 同一决策：域错误 unboxed
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod endpoints;
pub mod events;
pub mod host;
pub mod limits;
pub mod routing;
pub mod session;
pub mod workers;
