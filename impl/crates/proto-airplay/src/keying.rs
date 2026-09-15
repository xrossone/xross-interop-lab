//! 密钥获取 seam（plans/03 T32）：FairPlay/设备认证材料的位置。
//!
//! **本仓不实现密钥获取**：FairPlay 材料永久 vendor-gated（`decisions/provider-adoption.json`
//! P-FAIRPLAY；`provenance/airplay-inputs.json` R06 allowed_in_crate=false）。
//! 生产实现缺席 ⇒ 依赖它的能力缺席：没有 keying 时 `SETUP` 返回 503，不建流、不分配缓冲。
//!
//! 将来若获得授权（或接入外部合规 provider），只需实现本 trait——协议其余部分不用改。

use interop_contract::error::{Error, ErrorCode};

/// 流密钥（Debug 打码：密钥不进日志）。
#[derive(Clone, PartialEq, Eq)]
pub struct StreamKeys {
    bytes: Vec<u8>,
}

impl std::fmt::Debug for StreamKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StreamKeys(<{} bytes redacted>)", self.bytes.len())
    }
}

impl StreamKeys {
    /// 生产代码应只在解密路径内使用；不提供 Serialize/Display。
    pub fn expose_for_decrypt(&self) -> &[u8] {
        &self.bytes
    }
}

/// 密钥提供者（FairPlay 位置）。
pub trait KeyingProvider {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;
    /// 从已验证的配对会话派生流密钥。未授权实现返回 `vendor-gated`。
    fn derive_stream_keys(&self) -> Result<StreamKeys, Error>;
}

/// 默认实现：什么也不做——本仓没有 FairPlay 授权，能力随之缺席。
#[derive(Debug, Default)]
pub struct UnavailableKeying;

impl KeyingProvider for UnavailableKeying {
    fn name(&self) -> &'static str {
        "unavailable(vendor-gated)"
    }

    fn available(&self) -> bool {
        false
    }

    fn derive_stream_keys(&self) -> Result<StreamKeys, Error> {
        Err(Error::new(
            ErrorCode::VendorGated,
            "FairPlay/设备认证材料 vendor-gated：本仓不实现密钥获取（keying seam 留空）",
        )
        .with_phase("authorizing"))
    }
}

/// **测试用**确定性 keying：自造字节（不是任何真实材料），只用于验证状态机与资源记账。
#[derive(Debug)]
pub struct FakeKeying {
    keys: StreamKeys,
}

impl FakeKeying {
    pub fn deterministic() -> Self {
        Self {
            keys: StreamKeys {
                bytes: (0..32u8).map(|i| i.wrapping_mul(7).wrapping_add(3)).collect(),
            },
        }
    }
}

impl KeyingProvider for FakeKeying {
    fn name(&self) -> &'static str {
        "fake-test-only"
    }

    fn available(&self) -> bool {
        true
    }

    fn derive_stream_keys(&self) -> Result<StreamKeys, Error> {
        Ok(self.keys.clone())
    }
}
