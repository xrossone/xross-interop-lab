//! `interop-media`：媒体形态、时钟域与帧队列（plans/03 T28；sink 见后续 T29）。
//!
//! - [`descriptor`]：`EncodedStream`/`PcmStream`/`NativePresentation`/`MediaResource` 四类
//!   typed source。native **只传 opaque presentation id**，没有 export 能力，也不可序列化裸指针。
//! - [`clock`]：远端 timebase、本地单调、wall time **三域分离**；无 PTS 不伪造时间。
//! - [`stream`]：format 变化必须伴随 decoder reset（`FormatTracker`）；有界帧队列显式背压。
//!
//! 本 crate 不做解码、不开窗口、不联网：图形/解码后端在 worker（隔离进程）内。

// 与 workspace 同一决策：域错误 unboxed
#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod clock;
pub mod descriptor;
pub mod stream;
