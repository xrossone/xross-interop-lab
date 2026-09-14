//! `interop-contract`：Xross Interop 的领域契约 crate。
//!
//! 承载领域对象、统一错误、能力声明与 JSON 契约（plans/01 T06 起逐块填充：
//! ids / capability / offer / media / error + JSON Schema + golden vectors）。
//!
//! 依赖规则（docs/04 §3）：不依赖 Tokio、网络栈、GUI、codec 或 Xross 账号栈；
//! 仅允许基本序列化/标识类型等小型依赖。该规则由
//! `tests/t01_workspace_hygiene.rs` 以分层白名单强制执行。
#![forbid(unsafe_code)]
