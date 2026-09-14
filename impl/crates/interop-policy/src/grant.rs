//! Scoped grant（docs/07 §2）：host authority 是 grant 的唯一来源；
//! grant 绑定 client/provider/session/entry/direction/目的/总字节/有效期。
//! 复制 token 到另一 session、从 receive 转 send、跨 scope 全部失败。

use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::{EntryId, LeaseId, SessionId};

/// 授权方向。receive grant 不能 read，send grant 不能 write（FILE-08）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Read,
    Write,
}

/// 授权范围。provider 请求只携带 scope 值，**永不携带本机路径**（FILE-08：
/// provider 不拿源文件任意路径；路径解析是 host/存储层的职责）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceScope {
    Entry { entry_id: EntryId },
    MediaSession { session_id: SessionId },
    Listener { profile_id: String },
    /// 主产品专有 scope（Vault 等）——standalone host 的可签发集合之外
    Vault,
}

impl ResourceScope {
    pub fn kind(&self) -> ScopeKind {
        match self {
            Self::Entry { .. } => ScopeKind::Entry,
            Self::MediaSession { .. } => ScopeKind::MediaSession,
            Self::Listener { .. } => ScopeKind::Listener,
            Self::Vault => ScopeKind::Vault,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    Entry,
    MediaSession,
    Listener,
    Vault,
}

/// 单条 scoped grant。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub grant_id: LeaseId,
    /// 使用该 grant 的 provider/client 标识（绑定 subject，不可转借）
    pub subject: String,
    pub session_id: SessionId,
    pub scope: ResourceScope,
    pub direction: Direction,
    /// 目的（save/present/send...）；与 scope 一样参与精确匹配
    pub purpose: String,
    /// 累计字节预算；None = 本 host 未设预算（standalone 默认设）
    pub byte_budget: Option<u64>,
    /// 单调毫秒到期时间（fake clock 注入；到期未 commit 即作废）
    pub expires_at_ms: u64,
    /// 已消费字节（累计；由 host 记账）
    pub consumed_bytes: u64,
}

impl Grant {
    /// 校验一次访问请求。任何不匹配都是显式错误码，默认拒绝：
    /// - subject/session/scope/direction 不匹配 → `auth-denied`
    /// - 已过期 → `offer-expired`
    /// - 预算超限（累计）→ `resource-limit`
    pub fn authorize(
        &self,
        subject: &str,
        session_id: &SessionId,
        scope: &ResourceScope,
        direction: Direction,
        bytes: u64,
        now_ms: u64,
    ) -> Result<(), Error> {
        if subject != self.subject {
            return Err(denied("grant subject 不匹配"));
        }
        if *session_id != self.session_id {
            return Err(denied("grant session 不匹配（token 不可跨会话复制）"));
        }
        if *scope != self.scope {
            return Err(denied("grant scope 不匹配"));
        }
        if direction != self.direction {
            return Err(denied("grant direction 不匹配"));
        }
        if now_ms > self.expires_at_ms {
            return Err(Error::new(ErrorCode::OfferExpired, "grant 已到期")
                .with_phase("committing"));
        }
        if let Some(budget) = self.byte_budget {
            if self.consumed_bytes + bytes > budget {
                return Err(Error::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "字节预算超限：consumed={} + requested={} > budget={}",
                        self.consumed_bytes, bytes, budget
                    ),
                )
                .with_phase("transferring"));
            }
        }
        Ok(())
    }

    /// host 记账：把一次已授权的用量计入累计消费。
    pub fn consume(&mut self, bytes: u64) {
        self.consumed_bytes += bytes;
    }
}

fn denied(why: &str) -> Error {
    Error::new(ErrorCode::AuthDenied, format!("默认拒绝：{why}"))
}
