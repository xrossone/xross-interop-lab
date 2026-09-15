//! ShareOffer 文件报价（docs/05 §2、FILE-02）。
//!
//! offer 先有元信息、后授权、再接收内容：未批准不得 materialize 到最终路径。
//! `display_name` 是不受信任文本（UI 转义渲染）；目录结构用
//! `relative_components` 逐 component 表达，构造/校验时即拒绝穿越形状
//! （逐 component 的落盘安全在 interop-file/T08，本层做域形状校验）。

use crate::error::{Error, ErrorCode};
use crate::ids::{EndpointId, EntryId, OfferId};
use crate::U64;
use serde::{Deserialize, Serialize};

/// 报价内单条目。`wire_payload_id` 是 provider 私有句柄（对端协议的分块标识）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferEntry {
    pub entry_id: EntryId,
    /// 不受信任的显示名：不构成可执行安全依据（docs/05 §2）
    pub display_name: String,
    /// 可选目录结构；每 component 非空、非 `.`/`..`、无 NUL 与分隔符
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_components: Option<Vec<String>>,
    /// 声明的 MIME——不构成安全依据
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type_hint: Option<String>,
    /// 声明大小（可能与实际不符；落盘按预算与完整性校验）
    pub declared_size: U64,
    /// provider 私有 wire 标识
    pub wire_payload_id: String,
}

impl OfferEntry {
    /// 域形状校验：目录 components **与 display_name** 都必须能作为单个路径段。
    /// display_name 是不受信任文本；域层拒绝穿越/分隔符/控制字符形状，
    /// 更严格的平台规则（Windows 保留名、冒号/ADS）在 interop-file/T08 的落盘边界。
    pub fn validate(&self) -> Result<(), Error> {
        validate_components(self.relative_components.as_deref())?;
        validate_name(&self.display_name)
    }
}

/// 文件报价。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareOffer {
    pub offer_id: OfferId,
    pub endpoint_id: EndpointId,
    pub profile_id: String,
    pub entries: Vec<OfferEntry>,
    /// None = 对端未声明总量（不得伪造精确进度，FILE-03）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<U64>,
    /// UTC RFC3339 显示用 deadline（单调 timeout 由运行时另行计时）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    #[serde(default)]
    pub requested_actions: Vec<String>,
    pub trust_context: String,
    pub attempt_id: String,
}

impl ShareOffer {
    pub fn validate(&self) -> Result<(), Error> {
        for e in &self.entries {
            e.validate()?;
        }
        Ok(())
    }
}

fn validate_components(components: Option<&[String]>) -> Result<(), Error> {
    let Some(components) = components else {
        return Ok(());
    };
    for c in components {
        validate_component(c)?;
    }
    Ok(())
}

/// 单个路径段（也用于 display_name）：非空、非 `.`/`..`、无 NUL/控制字符/分隔符、长度受限。
fn validate_component(c: &str) -> Result<(), Error> {
    if c.is_empty() || c == "." || c == ".." {
        return Err(Error::new(
            ErrorCode::InvalidFrame,
            format!("非法路径组件 {c:?}"),
        )
        .with_phase("offered"));
    }
    if c.len() > 255 {
        return Err(Error::new(
            ErrorCode::InvalidFrame,
            format!("路径组件超过 255 字节：{c:?}"),
        )
        .with_phase("offered"));
    }
    if c.bytes().any(|b| b == 0 || b < 0x20 || b == 0x7f) {
        return Err(Error::new(
            ErrorCode::InvalidFrame,
            format!("路径组件含 NUL/控制字符：{c:?}"),
        )
        .with_phase("offered"));
    }
    if c.contains('/') || c.contains('\\') {
        return Err(Error::new(
            ErrorCode::InvalidFrame,
            format!("路径组件含分隔符：{c:?}"),
        )
        .with_phase("offered"));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), Error> {
    validate_component(name)
        .map_err(|_| Error::new(ErrorCode::InvalidFrame, format!("非法显示名 {name:?}")).with_phase("offered"))
}
