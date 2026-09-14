//! `interop-file`：受限流式存储与原子 commit（core 层；plans/01 T08）。
//!
//! 所有文件 I/O 在 host 存储边界内发生；provider 只有 entry/lease。
//! - [`path`]：不可信名称 → 逐 component 安全路径（形状拒绝：穿越/绝对/
//!   drive/UNC/保留名/分隔符注入）。
//! - [`spool`]：独占临时文件、顺序写 + checked offset、累计字节预算、
//!   hash 验证后原子 rename 发布；中间目录/root 被换成 symlink 在 commit
//!   时检测拒绝（lstat no-follow 检查；严格 TOCTOU-free 的 dirfd 链是
//!   平台特定加固，见 spool 模块内注记）。
//! - [`integrity`]：SHA-256 封装（RustCrypto sha2 crate）。
//!
//! 跨文件系统策略：temp 与 final 都在 spool root 之下（同一文件系统，rename
//! 原子）；把已发布文件移动到其他文件系统是 publish port（上层）的显式
//! copy+fsync+commit 流程，不在本层假装原子。

// 与 workspace 其他 crate 同一决策：域错误 unboxed
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod integrity;
pub mod path;
pub mod spool;
