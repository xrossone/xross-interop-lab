//! `proto-quickshare`：Quick Share / UKEY2 受限安全会话核心（plans/02-files.md §T20）。
//!
//! - [`wire`]：UKEY2 消息的 protobuf 最小子集编解码（字段号见 `specs-reviewed/f02` F-06..F-10）；
//! - [`framing`]：TCP 4 字节大端长度前缀 + 分片无关解码（F-05）；
//! - [`crypto`]：P-256 ECDH / SHA-2 / HKDF 组合（成熟 crate；F-13..F-17）；
//! - [`handshake`]：UKEY2 三消息状态机、commitment 校验、Alert 码表、认证串与 next secret；
//! - [`session`]：framing+握手+配额/超时+payload gate（T19-01/T19-02 不变量）。
//!
//! **不在这里**：发现（mDNS/BLE/QR，P-F02-1/3 未关闭）、传输加密与 payload 层（T21）、
//! 任何文件系统或 UI 行为。未实现的能力返回 `unsupported-feature`，不假成功。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod crypto;
pub mod framing;
pub mod handshake;
pub mod session;
pub mod wire;
