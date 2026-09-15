//! 发现解析（字段表 F-19）：`_googlecast._tcp` 的 TXT 键值 → `ReceiverInfo`。
//!
//! **只解析，不组播**：本模块不打开 socket、不做 mDNS 查询——TXt 记录由调用方（宿主发现层）提供。
//! TXT 字段矩阵本身仍是 blocked（F-22/P-M08-3），所以这里只认字段表登记的六个键，
//! 未知键**保留但不解释**（不因为它存在就推断能力）。

use interop_contract::error::{Error, ErrorCode};

/// 服务类型（F-19）。
pub const SERVICE_TYPE: &str = "_googlecast._tcp";
/// 连接端口（F-19：pychromecast 用 8009 区分设备与组）。
pub const DEFAULT_PORT: u16 = 8009;

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

/// `st` 取值（F-19：0 = idle、1 = busy）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceStatus {
    Idle,
    Busy,
}

/// 解析后的接收端信息（字段表登记的键）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverInfo {
    /// TXT `id`（设备 UUID；必填——没有它无法区分设备）。
    pub id: String,
    /// TXT `fn`（友好名；必填——没有它无法向用户展示"投给谁"）。
    pub friendly_name: String,
    /// TXT `md`（型号名）。
    pub model_name: Option<String>,
    /// TXT `ve`（版本串）。
    pub version: Option<String>,
    /// TXT `st`（0/1）。
    pub status: Option<DeviceStatus>,
    /// TXT `ca`（能力位掩码，十六进制）。
    pub capability_bits: Option<u32>,
    pub port: u16,
}

impl ReceiverInfo {
    /// 从严解析：服务类型必须是 `_googlecast._tcp`；`id` 与 `fn` 必须存在且非空；
    /// `st`/`ca` 出现时必须合法。未知键忽略（不解释）。
    pub fn from_txt(
        service_type: &str,
        txt: &[(String, String)],
        port: u16,
    ) -> Result<Self, Error> {
        if !service_type.starts_with(SERVICE_TYPE) {
            return Err(invalid(format!(
                "服务类型不是 {SERVICE_TYPE}（F-19）：{service_type:?}"
            )));
        }
        let lookup = |key: &str| {
            txt.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.trim())
        };
        let id = lookup("id")
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("TXT 缺 id（F-19）"))?;
        let friendly_name = lookup("fn")
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("TXT 缺 fn（F-19）"))?;
        let status = match lookup("st") {
            None => None,
            Some("0") => Some(DeviceStatus::Idle),
            Some("1") => Some(DeviceStatus::Busy),
            Some(other) => return Err(invalid(format!("未知的 st 取值 {other:?}（F-19 只有 0/1）"))),
        };
        let capability_bits = match lookup("ca") {
            None => None,
            Some(raw) => Some(
                u32::from_str_radix(raw, 16)
                    .map_err(|_| invalid(format!("ca 不是合法十六进制位掩码：{raw:?}")))?,
            ),
        };
        Ok(Self {
            id: id.to_string(),
            friendly_name: friendly_name.to_string(),
            model_name: lookup("md").map(str::to_string),
            version: lookup("ve").map(str::to_string),
            status,
            capability_bits,
            port,
        })
    }
}
