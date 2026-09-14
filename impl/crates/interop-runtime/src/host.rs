//! HostPorts 与内存 fake host（docs/04 §4、plans/01 T07）。
//!
//! FakeHost 是**可独立构建的内存 authority**：签发 scoped grant、受限 spool
//! 写入、原子 commit 记账（真实落盘在 T08 interop-file）。Media/Network ports
//! 默认 deny（mock host 先行——T03-02；网络监听默认关闭——docs/00 §3.10）。
//! 时钟为注入的单调毫秒，不读真实时钟。

use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::SessionId;
use interop_contract::offer::{OfferEntry, ShareOffer};
use interop_policy::grant::{Direction, Grant, ResourceScope, ScopeKind};

/// admission 请求：offer + provider 申请的 scope 集合。
/// provider 只请求 scope，永不命名本机路径。
#[derive(Debug, Clone)]
pub struct AdmissionRequest {
    pub offer: ShareOffer,
    pub requested_scopes: Vec<ResourceScope>,
    pub now_ms: u64,
}

/// 一次受 grant 保护的访问请求。
#[derive(Debug, Clone)]
pub struct ContentRequest {
    pub session_id: SessionId,
    pub scope: ResourceScope,
    pub direction: Direction,
    pub max_bytes: u64,
}

/// 受限写入句柄（fake spool 的索引）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinkHandle(pub usize);

/// 内存 fake host：实现了 admission/content/history 的 host authority，
/// media/network 默认拒绝。
#[derive(Debug, Default)]
pub struct FakeHost {
    ledger: Vec<Ledger>,
    sinks: Vec<SinkState>,
    history: Vec<String>,
    issuable_scopes: Vec<ScopeKind>,
}

#[derive(Debug)]
struct Ledger {
    grant: Grant,
}

#[derive(Debug)]
struct SinkState {
    grant_id: interop_contract::ids::LeaseId,
    bytes: Vec<u8>,
    committed: bool,
    aborted: bool,
}

impl FakeHost {
    pub fn new() -> Self {
        // fake authority 直接使用时是"全 kinds 可签发"的内存模型；
        // standalone/integrated host 必须用 set_issuable_scopes 收紧。
        Self {
            issuable_scopes: vec![
                ScopeKind::Entry,
                ScopeKind::MediaSession,
                ScopeKind::Listener,
                ScopeKind::Vault,
            ],
            ..Self::default()
        }
    }

    /// 收紧本 host 可签发的 scope 种类（standalone host 只允许 Entry）。
    pub fn set_issuable_scopes(&mut self, kinds: Vec<ScopeKind>) {
        self.issuable_scopes = kinds;
    }

    /// 直接注入 grant（测试用途；生产路径走 admission → issue_scoped）。
    pub fn issue_grant(&mut self, grant: Grant) {
        self.ledger.push(Ledger { grant });
    }

    /// 按可签发集合签发 grant；scope 不在集合内 → `auth-denied`（T07-03）。
    pub fn issue_scoped(
        &mut self,
        subject: &str,
        session_id: &SessionId,
        scope: &ResourceScope,
        direction: Direction,
        byte_budget: Option<u64>,
        expires_at_ms: u64,
    ) -> Result<Grant, Error> {
        let kind = scope.kind();
        if !self.issuable_scopes.contains(&kind) {
            return Err(Error::new(
                ErrorCode::AuthDenied,
                format!("scope kind {kind:?} 不在本 host 的可签发集合内"),
            ));
        }
        let grant = Grant {
            grant_id: interop_contract::ids::LeaseId::try_from(format!(
                "g_{}",
                self.ledger.len()
            ))
            .map_err(|e| Error::new(ErrorCode::ProviderCrashed, e))?,
            subject: subject.to_string(),
            session_id: session_id.clone(),
            scope: scope.clone(),
            direction,
            purpose: match direction {
                Direction::Read => "send".into(),
                Direction::Write => "save".into(),
            },
            byte_budget,
            expires_at_ms,
            consumed_bytes: 0,
        };
        self.ledger.push(Ledger { grant: grant.clone() });
        Ok(grant)
    }

    fn find_mut(&mut self, grant_id: &interop_contract::ids::LeaseId) -> &mut Grant {
        self.ledger
            .iter_mut()
            .map(|l| &mut l.grant)
            .find(|g| &g.grant_id == grant_id)
            .expect("grant 必须先经 issue_grant/issue_scoped 注入")
    }

    /// 受 grant 保护的读。方向/scope/subject 任一不匹配 → `auth-denied`。
    pub fn content_read(
        &mut self,
        subject: &str,
        req: &ContentRequest,
        now_ms: u64,
    ) -> Result<Vec<u8>, Error> {
        let grant = self
            .ledger
            .iter()
            .map(|l| &l.grant)
            .find(|g| g.subject == subject && g.session_id == req.session_id)
            .ok_or_else(|| {
                Error::new(ErrorCode::AuthDenied, "subject/session 无任何 grant")
            })?
            .clone();
        grant.authorize(
            subject,
            &req.session_id,
            &req.scope,
            req.direction,
            req.max_bytes,
            now_ms,
        )?;
        // fake spool：committed sink 的内容回读（真实读取在 T08/T09 经 lease）
        Ok(self
            .sinks
            .iter()
            .filter(|s| s.committed)
            .flat_map(|s| s.bytes.clone())
            .take(req.max_bytes as usize)
            .collect())
    }

    /// 打开受限写 sink（进 spool，不落最终文件——FILE-02）。
    pub fn open_sink(
        &mut self,
        subject: &str,
        grant: &Grant,
        _entry: &OfferEntry,
        now_ms: u64,
    ) -> Result<SinkHandle, Error> {
        grant.authorize(
            subject,
            &grant.session_id,
            &grant.scope,
            grant.direction,
            0,
            now_ms,
        )?;
        self.sinks.push(SinkState {
            grant_id: grant.grant_id.clone(),
            bytes: Vec::new(),
            committed: false,
            aborted: false,
        });
        Ok(SinkHandle(self.sinks.len() - 1))
    }

    /// 追加字节（计入预算）。
    pub fn sink_append(
        &mut self,
        grant: &Grant,
        handle: SinkHandle,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<(), Error> {
        grant.authorize(
            grant.subject.as_str(),
            &grant.session_id,
            &grant.scope,
            grant.direction,
            bytes.len() as u64,
            now_ms,
        )?;
        let sink = &mut self.sinks[handle.0];
        sink.bytes.extend_from_slice(bytes);
        let stored = self.find_mut(&grant.grant_id);
        stored.consume(bytes.len() as u64);
        Ok(())
    }

    /// 原子 commit：过期/超预算/未授权都会失败且不产生最终文件（T07-02）。
    pub fn sink_commit(&mut self, grant: &Grant, now_ms: u64) -> Result<(), Error> {
        grant.authorize(
            grant.subject.as_str(),
            &grant.session_id,
            &grant.scope,
            grant.direction,
            0,
            now_ms,
        )?;
        for s in &mut self.sinks {
            if s.grant_id == grant.grant_id && !s.aborted {
                s.committed = true;
            }
        }
        self.history
            .push(format!("published grant={}", grant.grant_id));
        Ok(())
    }

    /// 已发布（最终文件）计数。
    pub fn published_count(&self) -> usize {
        self.sinks.iter().filter(|s| s.committed).count()
    }

    pub fn has_committed(&self, grant: &Grant) -> bool {
        self.sinks
            .iter()
            .any(|s| s.grant_id == grant.grant_id && s.committed)
    }

    /// HistoryPort（fake：内存事件列表）。
    pub fn record_history(&mut self, event: String) {
        self.history.push(event);
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// MediaHostPort：mock host 先行——standalone 无渲染器时显式不可用
    /// （MEDIA-04：headless null/file sink 合法，不开假窗口）。
    pub fn media_open(
        &mut self,
        _subject: &str,
        _profile_id: &str,
        now_ms: u64,
    ) -> Result<SessionId, Error> {
        let _ = now_ms;
        Err(Error::new(
            ErrorCode::PlatformUnavailable,
            "standalone fake host 未配置任何媒体 sink",
        )
        .with_phase("authorizing"))
    }

    /// NetworkPolicyPort：默认拒绝自动监听（docs/00 §3.10：外部 profile 默认 off）。
    pub fn network_listen(
        &mut self,
        _subject: &str,
        _profile_id: &str,
        now_ms: u64,
    ) -> Result<(), Error> {
        let _ = now_ms;
        Err(Error::new(
            ErrorCode::PermissionRequired,
            "监听默认关闭；须用户显式启用 profile 后由 policy 放行",
        ))
    }
}
