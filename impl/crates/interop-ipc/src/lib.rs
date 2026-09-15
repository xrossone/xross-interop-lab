//! `interop-ipc`：本地控制面 IPC（docs/05 §6；plans/01 T09）。
//!
//! Carrier：Unix domain stream / Windows named pipe 之上的
//! **u32 big-endian 长度前缀 + UTF-8 JSON-RPC 2.0**（单对象，禁批量）。
//! 长度上限 256 KiB（262144），0/超限/坏 UTF-8/非 JSON 一律 `invalid-frame`
//! 且不分配。hello 先认证（版本协商 + scoped token），之后才接受 allowlist
//! 方法；未知方法 → `unsupported-method`；major 不兼容 → `version-unsupported`
//! 并关闭连接。无 HTTP 公网 listener。
//!
//! 本 crate 是纯 codec + 连接状态机：socket 绑定/peer-UID 校验在
//! `apps/interopd`（平台相关层）。

// 与 workspace 同一决策：域错误 unboxed
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod auth;
pub mod frame;
pub mod media_frame;
pub mod server;
