//! DMC（Digital Media Controller，plans/03 §T41）：发现 renderer、描述抓取策略、能力检查、URL lease。
//!
//! 三条硬规矩：
//! 1. **描述 URL 抓取策略**（T41-01）：只允许 http、无 userinfo、无 fragment，目标不得是
//!    loopback/link-local/组播/云元数据地址——策略拒绝用 `permission-required`，与语法错误区分；
//! 2. **能力检查不猜测**（T41-02）：只按 renderer 自己声明的 `protocolInfo` 判断能否推送；
//!    不支持就拒绝，**不得回退成屏幕镜像**（DLNA 本来就只是"媒体 URL 推送"）；
//! 3. **URL lease 可撤销**（T41-04）：Stop/取消/超时都立即撤销，撤销后同一入口不可再取。

use interop_contract::error::{Error, ErrorCode};
use std::fmt;

use crate::ssdp::{NotifyMessage, SsdpMessage};

/// 抓取目标（策略通过后的结果）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchTarget {
    pub host: String,
    pub port: u16,
    pub path: String,
    pub url: String,
}

/// 设备描述抓取策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescriptionFetchPolicy {
    pub max_url_bytes: usize,
}

impl Default for DescriptionFetchPolicy {
    fn default() -> Self {
        Self {
            max_url_bytes: 2048,
        }
    }
}

impl DescriptionFetchPolicy {
    /// 检查描述 URL 是否可以抓取。策略拒绝 → `permission-required`；形状问题 → `invalid-frame`。
    pub fn check(&self, url: &str) -> Result<FetchTarget, Error> {
        if url.len() > self.max_url_bytes {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                format!("URL {} 字节超过策略上限 {}", url.len(), self.max_url_bytes),
            )
            .with_phase("discovered"));
        }
        let deny = |why: &str| {
            Err(Error::new(
                ErrorCode::PermissionRequired,
                format!("描述 URL 抓取被策略拒绝：{why}（T41-01）"),
            )
            .with_phase("discovered"))
        };
        let rest = match url.strip_prefix("http://") {
            Some(r) => r,
            None => {
                if url.starts_with("https://") {
                    return deny("UPnP 描述用 http；https 行为未固化，暂不允许");
                }
                return deny("只允许 http:// 描述 URL");
            }
        };
        if rest.contains('#') {
            return deny("URL 不得带 fragment");
        }
        if rest.contains('@') {
            return deny("URL 不得含 userinfo");
        }
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        if authority.is_empty() {
            return deny("缺少主机");
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => {
                let port: u16 = p
                    .parse()
                    .map_err(|_| {
                        Error::new(ErrorCode::InvalidFrame, format!("端口不是整数：{p:?}"))
                            .with_phase("discovered")
                    })?;
                if port == 0 {
                    return Err(Error::new(ErrorCode::InvalidFrame, "端口 0 非法")
                        .with_phase("discovered"));
                }
                (h.to_string(), port)
            }
            None => (authority.to_string(), 80),
        };
        let host_lower = host.trim_matches(['[', ']']).to_ascii_lowercase();
        if host_lower.is_empty() {
            return deny("缺少主机");
        }
        if matches!(host_lower.as_str(), "localhost" | "localhost.localdomain") {
            return deny("loopback 主机名");
        }
        if host_lower.starts_with("127.") || host_lower == "::1" || host_lower == "0" {
            return deny("loopback 地址");
        }
        if host_lower == "0.0.0.0" || host_lower == "::" {
            return deny("未指定地址（0.0.0.0/::）");
        }
        if host_lower.starts_with("169.254.") {
            return deny("link-local 地址（含云元数据 169.254.169.254）");
        }
        if host_lower.starts_with("fe80:") {
            return deny("IPv6 link-local 地址");
        }
        if host_lower.starts_with("224.")
            || host_lower.starts_with("239.")
            || host_lower.starts_with("ff0")
        {
            return deny("组播地址");
        }
        Ok(FetchTarget {
            host,
            port,
            path: path.to_string(),
            url: url.to_string(),
        })
    }
}

/// DMC 侧错误（带明确类别，便于上层区分"策略拒绝"与"协议不符"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmcError {
    FetchPolicy(Error),
    MissingLocation,
    Unsupported(String),
    UnknownRenderer(String),
}

impl DmcError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::FetchPolicy(e) => e.code,
            Self::MissingLocation => ErrorCode::InvalidFrame,
            Self::Unsupported(_) => ErrorCode::UnsupportedProfile,
            Self::UnknownRenderer(_) => ErrorCode::DestinationUnavailable,
        }
    }
}

impl fmt::Display for DmcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FetchPolicy(e) => write!(f, "{}", e.message),
            Self::MissingLocation => write!(f, "alive 通告缺少 LOCATION（无法控制）"),
            Self::Unsupported(why) => write!(
                f,
                "{why}；本节点只做媒体 URL 推送，**不提供屏幕镜像（mirror）替代**"
            ),
            Self::UnknownRenderer(usn) => write!(f, "未知 renderer {usn}"),
        }
    }
}

impl std::error::Error for DmcError {}

/// 观察到的一条 renderer。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererEntry {
    pub usn: String,
    pub nt: String,
    pub location: String,
    pub host: String,
    pub max_age_secs: u64,
    pub last_seen_ms: u64,
    /// renderer 声明的 protocolInfo（来自描述；空 = 未声明，不能推任何东西）
    pub protocol_info: Vec<String>,
}

/// 一次 observe 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub usn: String,
    /// 被移除（byebye）时为空字符串
    pub host: String,
    pub removed: bool,
}

/// renderer 注册表（按 USN 去重；过期由 `live_renderers` 体现）。
#[derive(Debug)]
pub struct RendererRegistry {
    default_max_age_secs: u64,
    policy: DescriptionFetchPolicy,
    entries: Vec<RendererEntry>,
}

impl RendererRegistry {
    pub fn new(default_max_age_secs: u64) -> Self {
        Self {
            default_max_age_secs,
            policy: DescriptionFetchPolicy::default(),
            entries: Vec::new(),
        }
    }

    pub fn policy(&self) -> &DescriptionFetchPolicy {
        &self.policy
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, usn: &str) -> Option<&RendererEntry> {
        self.entries.iter().find(|e| e.usn == usn)
    }

    /// 观察一条 SSDP 通告：alive → 建/更新（LOCATION 必须过抓取策略）；byebye → 移除。
    pub fn observe(&mut self, msg: &SsdpMessage, now_ms: u64) -> Result<Observation, DmcError> {
        let notify: &NotifyMessage = match msg {
            SsdpMessage::Notify(n) => n,
            SsdpMessage::Search(_) => {
                return Err(DmcError::Unsupported(
                    "observe 只接受 NOTIFY（搜索请求是设备侧回答的）".to_string(),
                ))
            }
        };
        match notify.nts.as_str() {
            "ssdp:byebye" => {
                let before = self.entries.len();
                self.entries.retain(|e| e.usn != notify.usn);
                let removed = self.entries.len() != before;
                Ok(Observation {
                    usn: notify.usn.clone(),
                    host: String::new(),
                    removed,
                })
            }
            "ssdp:alive" | "ssdp:update" => {
                let location = notify
                    .location
                    .as_deref()
                    .ok_or(DmcError::MissingLocation)?;
                let target = self
                    .policy
                    .check(location)
                    .map_err(DmcError::FetchPolicy)?;
                let max_age = notify.max_age_secs.unwrap_or(self.default_max_age_secs);
                if let Some(entry) = self.entries.iter_mut().find(|e| e.usn == notify.usn) {
                    entry.nt = notify.nt.clone();
                    entry.location = location.to_string();
                    entry.host = target.host.clone();
                    entry.max_age_secs = max_age;
                    entry.last_seen_ms = now_ms;
                    return Ok(Observation {
                        usn: entry.usn.clone(),
                        host: entry.host.clone(),
                        removed: false,
                    });
                }
                self.entries.push(RendererEntry {
                    usn: notify.usn.clone(),
                    nt: notify.nt.clone(),
                    location: location.to_string(),
                    host: target.host.clone(),
                    max_age_secs: max_age,
                    last_seen_ms: now_ms,
                    protocol_info: Vec::new(),
                });
                Ok(Observation {
                    usn: notify.usn.clone(),
                    host: target.host,
                    removed: false,
                })
            }
            other => Err(DmcError::Unsupported(format!("未知 NTS {other:?}"))),
        }
    }

    /// 仍未过期的 renderer。
    pub fn live_renderers(&self, now_ms: u64) -> Vec<&RendererEntry> {
        self.entries
            .iter()
            .filter(|e| now_ms < e.last_seen_ms.saturating_add(e.max_age_secs.saturating_mul(1000)))
            .collect()
    }

    /// 记录 renderer 声明的 protocolInfo（来自设备描述解析；空列表 = 未声明）。
    pub fn set_protocol_info(&mut self, usn: &str, info: &[String]) -> Result<(), DmcError> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.usn == usn)
            .ok_or_else(|| DmcError::UnknownRenderer(usn.to_string()))?;
        entry.protocol_info = info.to_vec();
        Ok(())
    }

    /// 我们能否用 `<protocol>` + `<mime>` 推送到该 renderer（按它自己的声明判断）。
    pub fn check_push_capability(
        &self,
        usn: &str,
        protocol: &str,
        mime: &str,
    ) -> Result<(), DmcError> {
        let entry = self
            .get(usn)
            .ok_or_else(|| DmcError::UnknownRenderer(usn.to_string()))?;
        if entry.protocol_info.is_empty() {
            return Err(DmcError::Unsupported(format!(
                "renderer {usn} 未声明 protocolInfo"
            )));
        }
        for info in &entry.protocol_info {
            let parts: Vec<&str> = info.split(':').collect();
            if parts.len() < 3 {
                continue;
            }
            if parts[0] != protocol {
                continue;
            }
            let declared_mime = parts[2];
            if declared_mime == "*" || declared_mime.eq_ignore_ascii_case(mime) {
                return Ok(());
            }
        }
        Err(DmcError::Unsupported(format!(
            "renderer {usn} 的 protocolInfo {:?} 不支持 {protocol}/{mime}",
            entry.protocol_info
        )))
    }
}

/// 一个媒体 URL 的租约（播放期间有效）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlLease {
    pub token: String,
    pub url: String,
    pub expires_at_ms: u64,
}

/// URL lease 存储（T41-04：Stop/取消/超时都要撤销）。
#[derive(Debug)]
pub struct UrlLeaseStore {
    ttl_ms: u64,
    leases: Vec<UrlLease>,
    counter: u64,
}

impl UrlLeaseStore {
    /// `ttl_ms` 是租约时长（毫秒）。
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            ttl_ms,
            leases: Vec::new(),
            counter: 0,
        }
    }

    pub fn ttl_ms(&self) -> u64 {
        self.ttl_ms
    }

    /// 发放租约（同一 URL 重复发放会替换旧租约）。
    pub fn grant(&mut self, url: &str, now_ms: u64) -> Result<UrlLease, DmcError> {
        if url.is_empty() {
            return Err(DmcError::Unsupported("空 URL 无法发放租约".to_string()));
        }
        self.leases.retain(|l| l.url != url);
        self.counter += 1;
        let mut entropy = [0u8; 16];
        getrandom::getrandom(&mut entropy).map_err(|e| {
            DmcError::Unsupported(format!("系统熵不可用（lease token）：{e}"))
        })?;
        let token: String = entropy.iter().map(|b| format!("{b:02x}")).collect();
        let lease = UrlLease {
            token,
            url: url.to_string(),
            expires_at_ms: now_ms.saturating_add(self.ttl_ms),
        };
        self.leases.push(lease.clone());
        Ok(lease)
    }

    pub fn is_live(&self, url: &str, now_ms: u64) -> bool {
        self.leases
            .iter()
            .any(|l| l.url == url && now_ms < l.expires_at_ms)
    }

    pub fn live_count(&self, now_ms: u64) -> usize {
        self.leases.iter().filter(|l| now_ms < l.expires_at_ms).count()
    }

    /// 撤销指定租约（取消播放）。
    pub fn revoke(&mut self, lease: UrlLease) {
        self.leases.retain(|l| l.token != lease.token);
    }

    /// 撤销某个 URL 的全部租约（Stop）。
    pub fn revoke_for_url(&mut self, url: &str) {
        self.leases.retain(|l| l.url != url);
    }

    /// 清理过期租约，返回清理条数。
    pub fn sweep(&mut self, now_ms: u64) -> usize {
        let before = self.leases.len();
        self.leases.retain(|l| now_ms < l.expires_at_ms);
        before - self.leases.len()
    }

    pub fn len(&self) -> usize {
        self.leases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leases.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_allows_documentation_and_public_addresses() {
        let p = DescriptionFetchPolicy::default();
        assert!(p.check("http://192.0.2.44:49152/d.xml").is_ok());
        assert!(p.check("http://[2001:db8::1]:49152/d.xml").is_ok(), "文档地址 IPv6 允许");
        assert!(p.check("http://nas.local/desc.xml").is_ok(), "普通主机名允许");
    }

    #[test]
    fn policy_rejects_private_targets_with_policy_error() {
        let p = DescriptionFetchPolicy::default();
        for bad in [
            "http://127.0.0.1/x",
            "http://169.254.169.254/latest/meta-data/",
            "https://192.0.2.44/x",
            "file:///etc/passwd",
        ] {
            let e = p.check(bad).expect_err(bad);
            assert_eq!(e.code, ErrorCode::PermissionRequired, "{bad}");
        }
    }

    #[test]
    fn leases_are_revocable_and_expire() {
        let mut store = UrlLeaseStore::new(1_000);
        let lease = store.grant("http://192.0.2.9/m.mp4", 0).expect("grant");
        assert!(store.is_live("http://192.0.2.9/m.mp4", 500));
        store.revoke(lease);
        assert!(!store.is_live("http://192.0.2.9/m.mp4", 500));
    }
}
