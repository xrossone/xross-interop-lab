//! 能力声明与 API 版本协商（docs/01 §2、docs/05 §1）。
//!
//! 能力按 profile+role+platform+evidence 声明——不设一个万能 `supports_cast` 布尔值
//! （CORE-02/03）。缺少证据时返回 `unverified`/`blocked` 状态，不显示绿色支持。

use crate::error::{Error, ErrorCode};
use crate::media::MediaForm;
use serde::{Deserialize, Serialize};

/// API 版本：`interop.api/<major>.<minor>`（docs/05 §1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ApiVersion {
    pub major: u32,
    pub minor: u32,
}

impl ApiVersion {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub fn parse(s: &str) -> Result<Self, Error> {
        let rest = s
            .strip_prefix("interop.api/")
            .ok_or_else(|| Error::new(ErrorCode::InvalidFrame, "API 版本前缀必须是 interop.api/"))?;
        let (major, minor) = rest.split_once('.').ok_or_else(|| {
            Error::new(ErrorCode::InvalidFrame, "API 版本必须是 <major>.<minor>")
        })?;
        let major = major.parse().map_err(|_| invalid_version())?;
        let minor = minor.parse().map_err(|_| invalid_version())?;
        Ok(Self { major, minor })
    }

    /// 双方都能接受的版本。0.x 不稳定：minor 必须精确相等；
    /// stable（major≥1）：同 major 内取较小 minor；major 不同不可协商。
    pub fn negotiate(a: &ApiVersion, b: &ApiVersion) -> Option<ApiVersion> {
        if a.major != b.major {
            return None;
        }
        if a.major == 0 {
            (a.minor == b.minor).then_some(*a)
        } else {
            Some(ApiVersion::new(a.major, a.minor.min(b.minor)))
        }
    }
}

fn invalid_version() -> Error {
    Error::new(ErrorCode::InvalidFrame, "API 版本分量必须是非负整数")
}

impl std::fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "interop.api/{}.{}", self.major, self.minor)
    }
}

/// 协议角色（CORE-02：send/receive/control 分开声明，不合并）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Send,
    Receive,
    Control,
    Server,
    Client,
    Renderer,
}

/// 证据等级投影到能力状态（docs/00 §4）。`blocked` 必须附障碍说明（dossier 层）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceState {
    Catalogued,
    SourceReviewed,
    BuildVerified,
    Simulated,
    DeviceVerified,
    ReleaseQualified,
    Blocked,
}

/// 单个 profile×role 的能力声明（docs/01 §2 规范示例的字段集）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub profile_id: String,
    pub role: Role,
    pub provider_id: String,
    pub state: EvidenceState,
    pub available: bool,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub media_forms: Vec<MediaForm>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

impl Capability {
    /// 路由匹配：profile 与 role 都必须精确相等，且能力可用。
    /// T06-03：role=receive 的能力不匹配 send 查询——方向不静默互换（CORE-05）。
    pub fn matches(&self, profile_id: &str, role: Role) -> bool {
        self.available && self.profile_id == profile_id && self.role == role
    }
}
