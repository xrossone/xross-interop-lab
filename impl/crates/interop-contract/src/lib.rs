//! `interop-contract`：Xross Interop 的领域契约 crate。
//!
//! 承载领域对象、统一错误、能力声明与 JSON 契约（plans/01 T06）：
//! - [`ids`]：opaque 标识新类型（不可猜测本地 ID 的形状约束）
//! - [`capability`]：能力声明 + API 版本协商
//! - [`offer`]：ShareOffer 文件报价
//! - [`media`]：MediaDescriptor 与四种媒体形态
//! - [`error`]：统一错误对象与错误码
//!
//! wire 规则（docs/05 §1）：大整数编码为十进制字符串（[`U64`]）；
//! 未知 critical 枚举值拒绝（不映射 default）；未知**字段**忽略（前向兼容，
//! INT-06 混合版本；安全相关新字段走 required_features 协商，fail closed）。
//! 本方发出的 JSON 由 `impl/schemas/interop-api.schema.json` 约束
//! （与 Rust 序列化的一致性由 `tests/contract_schema.rs` 锁定）。
//!
//! 依赖规则（docs/04 §3）：不依赖 Tokio、网络栈、GUI、codec 或 Xross 账号栈；
//! 仅允许基本序列化/标识类型等小型依赖。该规则由
//! `tests/t01_workspace_hygiene.rs` 以分层白名单强制执行。
//!
//! `result_large_err` 的允许是有意为之：Error 是全 crate 统一的域错误，
//! unboxed 返回值对调用方更符合人机工学；体积优化留到 profile 阶段再评估。
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod capability;
pub mod error;
pub mod ids;
pub mod media;
pub mod offer;

mod wire_int;

pub use wire_int::U64;
