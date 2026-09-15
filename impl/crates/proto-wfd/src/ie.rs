//! WFD 子元素与 IE 边界（字段表 F-23/F-24）。
//!
//! **本模块只实现子元素（subelement）字节**：id `0x00` = 设备信息子元素，载荷 6 字节
//! （设备信息 2 + 控制端口 2 + 最大吞吐 2，网络字节序；F-23 的字面量与 R37 的平台调用一致）。
//!
//! **完整 WFD IE 不在这里**：容器需要的 OUI 与 OUI type 字节在 R32/R33/R34/R37 四个来源里
//! 都不存在（F-24），照抄任何猜测值都等于编造协议常量。因此 [`WfdIeContainer`] 的两个入口
//! 一律返回 `unsupported-feature`，把"我们不知道"写成可执行的拒绝，而不是写成一个看起来能用的常量表。

use interop_contract::error::{Error, ErrorCode};

/// 设备信息子元素 id（F-23）。
pub const DEVICE_INFO_SUBELEMENT_ID: u8 = 0x00;

/// 子元素载荷上限（**本仓策略**，不是协议常量）：设备信息子元素只有 6 字节。
pub const MAX_SUBELEMENT_BYTES: usize = 64;

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn blocked(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, msg)
}

/// 一个子元素：`id(1) + 长度(2, 大端) + 载荷`（F-23 的 9 字节字面量即此结构）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WfdSubelement {
    pub id: u8,
    pub payload: Vec<u8>,
}

impl WfdSubelement {
    pub fn new(id: u8, payload: Vec<u8>) -> Result<Self, Error> {
        if payload.len() > MAX_SUBELEMENT_BYTES {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!(
                    "子元素载荷超过本仓上限 {MAX_SUBELEMENT_BYTES} 字节（策略值）：{}",
                    payload.len()
                ),
            ));
        }
        Ok(Self { id, payload })
    }

    /// 解析一个子元素，返回 `(子元素, consumed)`。
    pub fn parse(buf: &[u8]) -> Result<(Self, usize), Error> {
        if buf.len() < 3 {
            return Err(invalid("子元素至少需要 id + 长度两字段（F-23）"));
        }
        let id = buf[0];
        let declared = u16::from_be_bytes([buf[1], buf[2]]) as usize;
        if declared > MAX_SUBELEMENT_BYTES {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("子元素长度字段超过本仓上限 {MAX_SUBELEMENT_BYTES}（分配前拒绝）"),
            ));
        }
        if buf.len() < 3 + declared {
            return Err(invalid("子元素载荷截断（长度字段与实际字节不符）"));
        }
        Ok((
            Self {
                id,
                payload: buf[3..3 + declared].to_vec(),
            },
            3 + declared,
        ))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(3 + self.payload.len());
        out.push(self.id);
        out.extend_from_slice(&(self.payload.len() as u16).to_be_bytes());
        out.extend_from_slice(&self.payload);
        out
    }
}

/// 设备信息子元素的载荷（F-23）：设备信息 + 控制端口 + 最大吞吐。
///
/// **设备信息字段保持不透明**：位图语义（设备类型/会话可用等）在四个来源里都没有出现
/// （F-24），因此这里只搬运 u16，不解释任何一位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInfoSubelement {
    /// 不透明字段：位语义未固化（F-24），不得据此判断对端类型或能力。
    pub device_info: u16,
    pub control_port: u16,
    /// 最大吞吐（来源取值 `0x00c8` = 200，单位未固化）。
    pub max_throughput: u16,
}

impl DeviceInfoSubelement {
    pub fn new(control_port: u16, max_throughput: u16) -> Self {
        Self {
            device_info: 0,
            control_port,
            max_throughput,
        }
    }

    pub fn to_subelement(&self) -> WfdSubelement {
        let mut payload = Vec::with_capacity(6);
        payload.extend_from_slice(&self.device_info.to_be_bytes());
        payload.extend_from_slice(&self.control_port.to_be_bytes());
        payload.extend_from_slice(&self.max_throughput.to_be_bytes());
        WfdSubelement {
            id: DEVICE_INFO_SUBELEMENT_ID,
            payload,
        }
    }

    pub fn from_subelement(subelement: &WfdSubelement) -> Result<Self, Error> {
        if subelement.id != DEVICE_INFO_SUBELEMENT_ID {
            return Err(invalid(format!(
                "只认设备信息子元素 id 0x00（F-23），收到 0x{:02X}",
                subelement.id
            )));
        }
        if subelement.payload.len() != 6 {
            return Err(invalid(format!(
                "设备信息子元素载荷必须是 6 字节（F-23），收到 {}",
                subelement.payload.len()
            )));
        }
        let read = |at: usize| u16::from_be_bytes([subelement.payload[at], subelement.payload[at + 1]]);
        Ok(Self {
            device_info: read(0),
            control_port: read(2),
            max_throughput: read(4),
        })
    }
}

/// 完整 WFD IE 的容器——**本仓不实现**（F-24）。
///
/// 存在这个类型只为把拒绝写成一处、可被测试断言，而不是散落在调用点。
pub struct WfdIeContainer;

impl WfdIeContainer {
    /// 解析完整 IE：永远返回 `unsupported-feature`（P-M05-2 未关闭）。
    pub fn parse(_bytes: &[u8]) -> Result<Vec<WfdSubelement>, Error> {
        Err(blocked(
            "完整 WFD IE 需要 OUI 与 OUI type 字节：四个来源（R32/R33/R34/R37）里都没有出现，本仓不臆造；\
             需要 P-M05-2 抓包关闭后实现（字段表 F-24）",
        ))
    }

    /// 构造完整 IE：同上，永远返回 `unsupported-feature`。
    pub fn build(_subelements: &[WfdSubelement]) -> Result<Vec<u8>, Error> {
        Err(blocked(
            "完整 WFD IE 的容器格式未固化（F-24，P-M05-2 未关闭）：拒绝构造，避免把猜测值写进线上字节",
        ))
    }
}
