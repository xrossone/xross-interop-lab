//! `interop-platform`：平台侧输入——发现源观察（T11）与只读平台 probe（T12）。
//!
//! 本 crate 只承载"外部世界告诉我们什么"的数据结构与形状校验；
//! **信任、去重、路由决策都不在这里**（authority 在 `interop-runtime`）。
//! 所有字段都是不受信任输入：展示名不做转义假设、identity claim 不构成身份。
//!
//! [`probe`] 只读（白名单命令、不经 shell、只保留接口名）；[`radio`] 的 arbiter
//! **永不自行改网络**——唯一出口是注入的 `RadioDriver`，且只在用户批准后调用。
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod discovery;
pub mod probe;
pub mod radio;
