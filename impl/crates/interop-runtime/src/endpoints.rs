//! Endpoint registry（plans/01 T11）：观察 → 过期/去重/路由可用的记录表。
//!
//! 三条不变量（ADR-003 与顶层设计简报 §7 的落地）：
//! 1. **不合并 identity**：去重键是
//!    `(profile_id, source, identity_claim, 地址集合)`——同名、同 IP、同地址都不会让两条
//!    观察变成一条记录；identity claim 变了就是另一条记录（不按 IP 提升信任）。
//! 2. **过期是显式的**：地址候选带 deadline（TTL），接口断开立即作废该接口上的候选；
//!    `sendable_addresses` 永远只返回仍有效的候选。
//! 3. **展示聚合不在这里**：用户 alias 只影响 `presentation_groups` 的派生视图，
//!    不改变 authority 行数、不参与授权（T11-05）。

use interop_contract::capability::Role;
use interop_contract::ids::EndpointId;
use interop_contract::media::MediaForm;
use interop_platform::discovery::{DiscoverySource, EndpointObservation};

/// 地址候选（带生存期）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressCandidate {
    pub host: String,
    pub port: u16,
    pub interface: String,
    pub secure: bool,
    pub expires_at_ms: u64,
}

/// 一条端点记录：一个来源对一台对端设备的观察聚合（**一个授权主体**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointRecord {
    pub endpoint_id: EndpointId,
    pub profile_id: String,
    pub source: DiscoverySource,
    pub display_name: String,
    pub identity_claim: Option<String>,
    pub capabilities: Vec<interop_contract::capability::Capability>,
    pub addresses: Vec<AddressCandidate>,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
}

impl EndpointRecord {
    /// 授权主体标识：profile + 本地 opaque id。同名/同地址的两条记录主体必不相同。
    pub fn subject(&self) -> String {
        format!("{}@{}", self.profile_id, self.endpoint_id)
    }

    /// 对端是否声明了某 profile×role 的能力（声明 ≠ 证据；可用性以本机为准）。
    pub fn supports(&self, profile_id: &str, role: Role) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.available && c.profile_id == profile_id && c.role == role)
    }

    /// 该 profile×role 声明的媒体形态集合（去重、顺序稳定）。
    pub fn media_forms(&self, profile_id: &str, role: Role) -> Vec<MediaForm> {
        let mut out: Vec<MediaForm> = Vec::new();
        for c in &self.capabilities {
            if !c.available || c.profile_id != profile_id || c.role != role {
                continue;
            }
            for f in &c.media_forms {
                if !out.contains(f) {
                    out.push(*f);
                }
            }
        }
        out
    }

    /// 当前仍有效的地址候选（顺序稳定：接口 → host → port）。
    pub fn sendable_addresses(&self, now_ms: u64) -> Vec<&AddressCandidate> {
        let mut live: Vec<&AddressCandidate> = self
            .addresses
            .iter()
            .filter(|a| a.expires_at_ms >= now_ms)
            .collect();
        live.sort_by(|a, b| {
            (a.interface.as_str(), a.host.as_str(), a.port).cmp(&(
                b.interface.as_str(),
                b.host.as_str(),
                b.port,
            ))
        });
        live
    }
}

/// 去重键：同来源同声明同地址集合才是同一条观察；不含展示名。
type DedupeKey = (String, DiscoverySource, Option<String>, Vec<(String, u16, String)>);

fn dedupe_key(o: &EndpointObservation) -> DedupeKey {
    let mut addrs: Vec<(String, u16, String)> = o
        .addresses
        .iter()
        .map(|a| (a.host.clone(), a.port, a.interface.clone()))
        .collect();
    addrs.sort();
    (
        o.profile_id.clone(),
        o.source,
        o.identity_claim.clone(),
        addrs,
    )
}

/// 端点注册表（authority 侧视图；按插入顺序稳定排列）。
#[derive(Debug, Default)]
pub struct EndpointRegistry {
    ttl_ms: u64,
    records: Vec<EndpointRecord>,
}

impl EndpointRegistry {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            ttl_ms,
            records: Vec::new(),
        }
    }

    /// 观察输入：校验（fail-closed）→ 去重（按去重键）→ 刷新地址与声明。
    /// 返回该记录稳定的 `EndpointId`。
    pub fn observe(&mut self, observation: EndpointObservation) -> Result<EndpointId, interop_contract::error::Error> {
        observation.validate()?;
        let key = dedupe_key(&observation);
        let expires_at_ms = observation.observed_at_ms.saturating_add(self.ttl_ms);
        if let Some(rec) = self
            .records
            .iter_mut()
            .find(|r| dedupe_key_of(r) == key)
        {
            rec.display_name = observation.display_name;
            rec.capabilities = observation.capabilities;
            rec.addresses = observation
                .addresses
                .iter()
                .map(|a| AddressCandidate {
                    host: a.host.clone(),
                    port: a.port,
                    interface: a.interface.clone(),
                    secure: a.secure,
                    expires_at_ms,
                })
                .collect();
            rec.last_seen_ms = observation.observed_at_ms;
            return Ok(rec.endpoint_id.clone());
        }
        let id = observation.endpoint_id.clone();
        self.records.push(EndpointRecord {
            endpoint_id: id.clone(),
            profile_id: observation.profile_id,
            source: observation.source,
            display_name: observation.display_name,
            identity_claim: observation.identity_claim,
            capabilities: observation.capabilities,
            addresses: observation
                .addresses
                .iter()
                .map(|a| AddressCandidate {
                    host: a.host.clone(),
                    port: a.port,
                    interface: a.interface.clone(),
                    secure: a.secure,
                    expires_at_ms,
                })
                .collect(),
            first_seen_ms: observation.observed_at_ms,
            last_seen_ms: observation.observed_at_ms,
        });
        Ok(id)
    }

    /// authority 视图：逐来源、逐声明一行，**永不合并**。
    pub fn endpoints(&self) -> &[EndpointRecord] {
        &self.records
    }

    pub fn get(&self, id: &EndpointId) -> Option<&EndpointRecord> {
        self.records.iter().find(|r| &r.endpoint_id == id)
    }

    /// 仍可发送的地址候选（过期/接口断开的候选不在其中）。
    pub fn sendable_addresses(&self, id: &EndpointId, now_ms: u64) -> Vec<&AddressCandidate> {
        self.get(id)
            .map(|r| r.sendable_addresses(now_ms))
            .unwrap_or_default()
    }

    /// 接口断开：该接口上的候选立即作废；返回受影响的记录 id。
    pub fn interface_down(&mut self, interface: &str, now_ms: u64) -> Vec<EndpointId> {
        let mut affected = Vec::new();
        for rec in self.records.iter_mut() {
            let before = rec.addresses.len();
            rec.addresses.retain(|a| a.interface != interface);
            if rec.addresses.len() != before {
                affected.push(rec.endpoint_id.clone());
            }
        }
        self.expire(now_ms);
        affected
    }

    /// 定期清理：过期候选删除；超过 TTL 未见任何观察的记录整条移除。
    /// 返回受影响（候选变化或被移除）的记录 id。
    pub fn expire(&mut self, now_ms: u64) -> Vec<EndpointId> {
        let mut affected = Vec::new();
        let ttl = self.ttl_ms;
        for rec in self.records.iter_mut() {
            let before = rec.addresses.len();
            rec.addresses.retain(|a| a.expires_at_ms >= now_ms);
            if rec.addresses.len() != before {
                affected.push(rec.endpoint_id.clone());
            }
        }
        let mut removed = Vec::new();
        self.records.retain(|r| {
            let stale = r.last_seen_ms.saturating_add(ttl) < now_ms;
            if stale {
                removed.push(r.endpoint_id.clone());
            }
            !stale
        });
        affected.extend(removed);
        affected
    }
}

fn dedupe_key_of(r: &EndpointRecord) -> DedupeKey {
    let mut addrs: Vec<(String, u16, String)> = r
        .addresses
        .iter()
        .map(|a| (a.host.clone(), a.port, a.interface.clone()))
        .collect();
    addrs.sort();
    (r.profile_id.clone(), r.source, r.identity_claim.clone(), addrs)
}

/// 用户显式 alias：只用于展示聚合（例如用户认定"这三行是同一台手机"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAlias {
    pub alias: String,
    pub endpoints: Vec<EndpointId>,
}

/// 派生展示分组：只含仍存在的记录，未知 id 直接丢弃（不新造主体、不提升信任）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationGroup {
    pub alias: String,
    pub members: Vec<EndpointId>,
}

pub fn presentation_groups(reg: &EndpointRegistry, aliases: &[UserAlias]) -> Vec<PresentationGroup> {
    let mut out: Vec<PresentationGroup> = Vec::new();
    for a in aliases {
        let mut members: Vec<EndpointId> = Vec::new();
        for id in &a.endpoints {
            if reg.get(id).is_some() && !members.contains(id) {
                members.push(id.clone());
            }
        }
        if members.is_empty() {
            continue;
        }
        out.push(PresentationGroup {
            alias: a.alias.clone(),
            members,
        });
    }
    out.sort_by(|a, b| a.alias.cmp(&b.alias));
    out
}
