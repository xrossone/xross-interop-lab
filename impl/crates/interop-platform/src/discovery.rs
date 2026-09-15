//! 发现源观察（plans/01 T11；docs/02 的多发现源边界）。
//!
//! 一个发现源（mDNS / SSDP / UDP multicast / fake）报出的是**观察**，不是身份：
//! - `display_name` 不受信任（UI 渲染层负责转义/裁剪）；
//! - `identity_claim` 是协议自报字符串，**不构成可验证身份**，不得用于合并记录；
//! - 地址候选带接口与安全标志：明文候选必须如实标注（`secure=false`），
//!   是否允许明文由用户策略在路由层裁决。
//!
//! 阶段 2 只提供 fake 源（确定性、无网络）；真 mDNS/SSDP 接入属阶段 3 的 provider 工作。

use interop_contract::capability::Capability;
use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::EndpointId;

/// 发现来源类别。不同来源之间**永不**因同名/同地址而合并（T11-01）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiscoverySource {
    Mdns,
    Ssdp,
    UdpMulticast,
    Fake,
}

impl DiscoverySource {
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Mdns => "mdns",
            Self::Ssdp => "ssdp",
            Self::UdpMulticast => "udp-multicast",
            Self::Fake => "fake",
        }
    }
}

/// 一个地址候选：接口 + 传输安全标志。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EndpointAddress {
    pub host: String,
    pub port: u16,
    /// 观察到的网络接口（接口断开时该候选立即失效——T11-02）
    pub interface: String,
    /// 该候选是否具备加密/认证通道；false = 明文，永不默认升级
    pub secure: bool,
}

impl EndpointAddress {
    pub fn validate(&self) -> Result<(), Error> {
        if self.host.is_empty() || self.host.len() > 255 {
            return Err(invalid("host 长度必须在 1..=255"));
        }
        if self.host.bytes().any(|b| b == 0 || b.is_ascii_whitespace()) {
            return Err(invalid("host 不得含空白或 NUL"));
        }
        if self.port == 0 {
            return Err(invalid("port 0 非法"));
        }
        if self.interface.is_empty() || self.interface.len() > 64 {
            return Err(invalid("interface 长度必须在 1..=64"));
        }
        Ok(())
    }
}

/// 一次端点观察（不受信任）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointObservation {
    /// 来源侧声明的本地观察标识（注册表去重后保持稳定）
    pub endpoint_id: EndpointId,
    pub profile_id: String,
    pub source: DiscoverySource,
    /// 不受信任显示名（可能与其他设备重名）
    pub display_name: String,
    /// 协议自报身份声明——仅作证据，不参与合并/授权
    pub identity_claim: Option<String>,
    pub addresses: Vec<EndpointAddress>,
    /// 对端声明的能力（同样不受信任；可用性判断仍以本机证据为准）
    pub capabilities: Vec<Capability>,
    pub observed_at_ms: u64,
}

impl EndpointObservation {
    /// 形状校验：fail-closed（非法输入不得进入 registry）。
    pub fn validate(&self) -> Result<(), Error> {
        validate_token("profile_id", &self.profile_id)?;
        validate_display_name(&self.display_name)?;
        if self.addresses.is_empty() {
            return Err(invalid("观察必须至少含一个地址候选"));
        }
        for a in &self.addresses {
            a.validate()?;
        }
        if let Some(claim) = &self.identity_claim {
            if claim.is_empty() || claim.len() > 256 {
                return Err(invalid("identity claim 长度必须在 1..=256"));
            }
        }
        for c in &self.capabilities {
            validate_token("capability.profile_id", &c.profile_id)?;
        }
        Ok(())
    }
}

fn invalid(why: &str) -> Error {
    Error::new(ErrorCode::InvalidFrame, format!("非法观察输入：{why}")).with_phase("discovered")
}

fn validate_token(field: &str, s: &str) -> Result<(), Error> {
    if s.is_empty() || s.len() > 64 {
        return Err(invalid(&format!("{field} 长度必须在 1..=64")));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(invalid(&format!("{field} 只允许 [A-Za-z0-9._-]")));
    }
    Ok(())
}

/// 显示名是不受信任文本：非空、限长、无控制字符（NUL/换行等一律拒绝）。
fn validate_display_name(name: &str) -> Result<(), Error> {
    if name.is_empty() || name.chars().count() > 256 {
        return Err(invalid("display_name 长度必须在 1..=256"));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(invalid("display_name 不得含控制字符"));
    }
    Ok(())
}

/// 确定性 fake 发现源：id 形如 `ep_fake_<n>`，同一次运行可重复。
/// 只用于测试与模拟闭环（T4）；不产生任何真实网络行为。
#[derive(Debug, Default)]
pub struct FakeDiscovery {
    seq: u32,
}

impl FakeDiscovery {
    pub fn new() -> Self {
        Self { seq: 0 }
    }

    /// 生成一个观察。调用方可在返回后覆写字段构造负向用例。
    pub fn announce(
        &mut self,
        profile_id: &str,
        display_name: &str,
        address: EndpointAddress,
        now_ms: u64,
    ) -> EndpointObservation {
        self.seq += 1;
        EndpointObservation {
            endpoint_id: EndpointId::try_from(format!("ep_fake_{}", self.seq))
                .expect("fake id 形状合法"),
            profile_id: profile_id.to_string(),
            source: DiscoverySource::Fake,
            display_name: display_name.to_string(),
            identity_claim: None,
            addresses: vec![address],
            capabilities: vec![],
            observed_at_ms: now_ms,
        }
    }
}
