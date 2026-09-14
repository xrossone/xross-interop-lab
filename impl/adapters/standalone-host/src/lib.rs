//! `standalone-host`：独立本地 host——最小权限的 standalone 实现（docs/04 §4）。
//!
//! 显式本地 policy：profile 默认关闭、用户显式启用；批准有效期 60 秒
//! （docs/01 §8）；可签发 scope 只允许 Entry（Vault 等主产品 scope 永不签发，
//! 认证/配对成功也不放大 scope——SEC-01/T07-03）。
//!
//! integrated 模式（XrossHostAdapter）另行实现同一组 port；本 crate 不创建
//! 账号、不创建 Iroh/网络实例，纯内存 + 注入时钟。

// 与 interop-contract 同一决策：域错误 unboxed
#![allow(clippy::result_large_err)]

use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::SessionId;
use interop_contract::offer::{OfferEntry, ShareOffer};
use interop_policy::grant::{Direction, Grant, ResourceScope, ScopeKind};
use interop_runtime::host::{AdmissionRequest, ContentRequest, FakeHost, SinkHandle};

/// standalone 本地策略（显式配置，默认值即最小权限）。
#[derive(Debug, Clone)]
pub struct StandalonePolicy {
    /// 用户显式启用的 profile（默认关闭一切监听/接收——docs/00 §3.10）
    pub enabled_profiles: Vec<String>,
    /// 交互审批 deadline（docs/01 §8：60 秒）
    pub interactive_deadline_ms: u64,
    /// 批准后 grant 的有效期（过期不落最终文件——T07-02）
    pub grant_ttl_ms: u64,
}

impl Default for StandalonePolicy {
    fn default() -> Self {
        Self {
            enabled_profiles: vec!["localsend.v2".into(), "quickshare.lan.v1".into()],
            interactive_deadline_ms: 60_000,
            grant_ttl_ms: 60_000,
        }
    }
}

/// standalone 可签发的 scope 种类：只有受限 entry 读写。
/// Vault/MediaSession/Listener 需要更强的 authority（integrated host 或
/// 用户显式 media 启用），本实现永不签发。
pub fn default_issuable_scopes() -> Vec<ScopeKind> {
    vec![ScopeKind::Entry]
}

/// standalone host：policy + 内存 fake authority 的组合。
#[derive(Debug)]
pub struct StandaloneHost {
    policy: StandalonePolicy,
    host: FakeHost,
}

impl StandaloneHost {
    /// 不接受任何身份/网络参数：standalone 模式没有账号也没有 fabric（INT-02）。
    pub fn new(policy: StandalonePolicy, host: FakeHost) -> Self {
        let mut host = host;
        host.set_issuable_scopes(default_issuable_scopes());
        Self { policy, host }
    }

    pub fn policy(&self) -> &StandalonePolicy {
        &self.policy
    }

    /// AdmissionPort：profile 显式启用 + scope 全部在可签发集合内才批准。
    /// 任一 scope 越权 → 整体 `auth-denied`（fail closed，不裁剪后部分批准）。
    pub fn admit(&mut self, req: AdmissionRequest) -> Result<Vec<Grant>, Error> {
        let offer: &ShareOffer = &req.offer;
        if !self.policy.enabled_profiles.contains(&offer.profile_id) {
            return Err(Error::new(
                ErrorCode::PlatformUnavailable,
                format!("profile {} 未启用（standalone 默认关闭）", offer.profile_id),
            )
            .with_phase("authorizing")
            .with_profile(offer.profile_id.clone()));
        }
        for scope in &req.requested_scopes {
            if !default_issuable_scopes().contains(&scope.kind()) {
                return Err(Error::new(
                    ErrorCode::AuthDenied,
                    "请求包含 standalone 不可签发的 scope：认证/配对成功也不放大 scope",
                )
                .with_phase("authorizing"));
            }
        }
        let subject = provider_subject(&offer.profile_id);
        let session_id = SessionId::try_from(format!("ses_{}", offer.attempt_id))
            .map_err(|e| Error::new(ErrorCode::InvalidFrame, e))?;
        let mut grants = Vec::new();
        for entry in &offer.entries {
            let total = req
                .offer
                .total_bytes
                .map(|t| t.0)
                .unwrap_or_else(|| entry.declared_size.0);
            grants.push(self.host.issue_scoped(
                &subject,
                &session_id,
                &ResourceScope::Entry {
                    entry_id: entry.entry_id.clone(),
                },
                Direction::Write,
                Some(total),
                req.now_ms + self.policy.grant_ttl_ms,
            )?);
        }
        self.host.record_history(format!(
            "admitted offer={} profile={} grants={}",
            offer.offer_id, offer.profile_id, grants.len()
        ));
        Ok(grants)
    }

    pub fn open_sink(
        &mut self,
        subject: &str,
        grant: &Grant,
        entry: &OfferEntry,
        now_ms: u64,
    ) -> Result<SinkHandle, Error> {
        self.host.open_sink(subject, grant, entry, now_ms)
    }

    pub fn sink_append(
        &mut self,
        grant: &Grant,
        handle: SinkHandle,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<(), Error> {
        self.host.sink_append(grant, handle, bytes, now_ms)
    }

    pub fn sink_commit(&mut self, grant: &Grant, now_ms: u64) -> Result<(), Error> {
        self.host.sink_commit(grant, now_ms)
    }

    pub fn published_count(&self) -> usize {
        self.host.published_count()
    }

    pub fn has_committed(&self, grant: &Grant) -> bool {
        self.host.has_committed(grant)
    }

    pub fn content_read(
        &mut self,
        subject: &str,
        req: &ContentRequest,
        now_ms: u64,
    ) -> Result<Vec<u8>, Error> {
        self.host.content_read(subject, req, now_ms)
    }

    pub fn media_open(
        &mut self,
        subject: &str,
        profile_id: &str,
        now_ms: u64,
    ) -> Result<SessionId, Error> {
        self.host.media_open(subject, profile_id, now_ms)
    }

    pub fn network_listen(
        &mut self,
        subject: &str,
        profile_id: &str,
        now_ms: u64,
    ) -> Result<(), Error> {
        self.host.network_listen(subject, profile_id, now_ms)
    }
}

/// standalone mock 约定：provider 身份 = `<profile 前缀>-provider`。
/// integrated 模式（XrossHostAdapter）会换成主仓真实主体标识。
fn provider_subject(profile_id: &str) -> String {
    let prefix = profile_id.split('.').next().unwrap_or(profile_id);
    format!("{prefix}-provider")
}
