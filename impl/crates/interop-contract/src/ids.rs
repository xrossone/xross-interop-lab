//! opaque 本地标识新类型（docs/05 §1）。
//!
//! 全部公共 ID 为不可猜测的本地 opaque string：不携带 path/token/remote address。
//! 形状约束：`[A-Za-z0-9._-]{1,64}` 且不以 `.` 开头。生产值由运行时 CSPRNG 生成；
//! 本 crate 只做形状校验（fixture 才用 `ofr_demo` 类可读值）。

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! opaque_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// 形状校验：1..=64 字符，`[A-Za-z0-9._-]`，不以 `.` 开头（防 `..`/隐藏语义）。
            fn validate(s: &str) -> Result<(), &'static str> {
                if s.is_empty() || s.len() > 64 {
                    return Err("ID 长度必须在 1..=64");
                }
                if s.starts_with('.') {
                    return Err("ID 不得以 . 开头");
                }
                if !s
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                {
                    return Err("ID 只允许 [A-Za-z0-9._-]");
                }
                Ok(())
            }
        }

        impl TryFrom<String> for $name {
            type Error = &'static str;
            fn try_from(s: String) -> Result<Self, Self::Error> {
                Self::validate(&s)?;
                Ok(Self(s))
            }
        }

        impl<'a> TryFrom<&'a str> for $name {
            type Error = &'static str;
            fn try_from(s: &'a str) -> Result<Self, Self::Error> {
                Self::validate(s)?;
                Ok(Self(s.to_string()))
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

opaque_id!(
    /// 设备/外部端点观察标识（≠ MAC/IP/设备名，本地注册表生成）
    EndpointId
);
opaque_id!(/// 文件报价标识
 OfferId);
opaque_id!(/// 会话标识
 SessionId);
opaque_id!(/// 报价内条目标识
 EntryId);
opaque_id!(/// scoped lease 标识
 LeaseId);
opaque_id!(/// 呈现会话标识（NativePresentation 的 opaque token）
 PresentationId);
opaque_id!(/// 证据 run 标识
 RunId);
