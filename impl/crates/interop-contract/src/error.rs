//! 统一错误对象（docs/05 §3）：`code/message/retryable/phase/profile_id/evidence_id/remedy`，
//! 无凭据。未知错误码在反序列化时拒绝（serde enum 默认行为，不得加 default）。

use serde::{Deserialize, Serialize};

/// 初始错误码集合（docs/05 §3）。serde 以 kebab-case 编码；
/// 新增码属于契约演进，需要版本协商，不得静默扩展。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    UnsupportedProfile,
    UnsupportedRole,
    UnsupportedMethod,
    UnsupportedFeature,
    VersionUnsupported,
    PlatformUnavailable,
    PermissionRequired,
    HardwareUnavailable,
    PairingFailed,
    AuthDenied,
    OfferExpired,
    Busy,
    DeadlineExceeded,
    ResourceLimit,
    InvalidFrame,
    IntegrityFailed,
    SourceChanged,
    DestinationUnavailable,
    Cancelled,
    ProviderCrashed,
    EventGap,
    LicenseGated,
    VendorGated,
    ProtectedContent,
}

/// 补救指引（结构化对象，不是任意 shell 命令——docs/07 §8）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remedy {
    pub hint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_ref: Option<String>,
}

/// 统一错误对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remedy: Option<Remedy>,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
            phase: None,
            profile_id: None,
            evidence_id: None,
            remedy: None,
        }
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn with_phase(mut self, phase: impl Into<String>) -> Self {
        self.phase = Some(phase.into());
        self
    }

    pub fn with_profile(mut self, profile_id: impl Into<String>) -> Self {
        self.profile_id = Some(profile_id.into());
        self
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}({}): {}", self.code, self.retryable, self.message)
    }
}

impl std::error::Error for Error {}
