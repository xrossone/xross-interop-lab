//! `proto-upnp`：DLNA/UPnP AV 控制路径（plans/03 §T41 的 headless 切片）。
//!
//! - [`ssdp`]：SSDP 报文（M-SEARCH/NOTIFY）的严格编解码与预算；
//! - [`xml`]：**加固的 XML 子集**（禁 DTD/实体、深度/体积/元素数预算——预算是本仓策略值）；
//! - [`soap`]：AVTransport/ContentDirectory 的 SOAP 动作与参数校验（未知动作不猜测）；
//! - [`dmc`]：renderer 注册表、描述抓取策略（T41-01）、protocolInfo 能力检查（T41-02）、
//!   URL lease 撤销（T41-04）；
//! - [`dms`]：受限 ContentDirectory（objectID 授权 701、分页上限 402，T41-03）。
//!
//! **不在这里**：真实 TV 的 protocolInfo/时序矩阵（P-M07-1，blocked，见
//! `specs-reviewed/m07-dlna-upnp-av.md`）、GENA 事件投递、媒体字节传输（T24/T42）、
//! 以及任何**屏幕镜像**能力（DLNA 只是媒体 URL 推送，绝不假装 mirror）。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod dmc;
pub mod dmr;
pub mod gena;
pub mod dms;
pub mod soap;
pub mod ssdp;
pub mod xml;
