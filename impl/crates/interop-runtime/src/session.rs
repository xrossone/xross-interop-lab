//! 会话注册表（CORE-06 / docs/05 §4）：decide/expire/cancel 的唯一序列化
//! authority——同步单点即 actor 语义；终态不可逆；取消幂等（一次清理）；
//! worker 崩溃只影响其挂靠会话（CORE-10）；回收按 absolute deadline。

use interop_contract::ids::SessionId;
use std::collections::HashMap;

/// offer 裁决与会话状态共用的终态集合。
/// Pending 不是终态；Accepted/Expired/Rejected/Cancelled/Failed/Completed 均不可逆。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionState {
    Pending,
    Accepted,
    Rejected,
    Expired,
    Active,
    Cancelling,
    Cancelled,
    Failed,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Active,
    Cancelling,
    Cancelled,
    Failed,
    Completed,
}

#[derive(Debug)]
struct OfferRecord {
    deadline_ms: u64,
    state: DecisionState,
}

#[derive(Debug)]
#[allow(dead_code)] // 元数据字段：HistoryPort（T10+）与诊断使用
struct SessionRecord {
    profile_id: String,
    provider_id: String,
    state: SessionState,
    worker: Option<String>,
    deadline_ms: Option<u64>,
    registered_at_ms: u64,
}

/// 唯一裁决点：所有状态迁移都经此结构序列化。
#[derive(Debug, Default)]
pub struct SessionRegistry {
    offers: HashMap<String, OfferRecord>,
    sessions: HashMap<SessionId, SessionRecord>,
    terminal_decisions: usize,
    cleanups: Vec<String>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个待裁决 offer（deadline 为 absolute 单调毫秒）。
    pub fn register_offer(&mut self, offer_id: &str, deadline_ms: u64) {
        self.offers.insert(
            offer_id.to_string(),
            OfferRecord {
                deadline_ms,
                state: DecisionState::Pending,
            },
        );
    }

    /// 接受裁决：Pending 且未过期 → Accepted；已过期 → Expired；
    /// 已终态 → 原样返回（不可逆）。
    pub fn decide_accept(&mut self, offer_id: &str, now_ms: u64) -> DecisionState {
        let Some(rec) = self.offers.get_mut(offer_id) else {
            return DecisionState::Rejected;
        };
        match rec.state {
            DecisionState::Pending => {
                let terminal = if now_ms > rec.deadline_ms {
                    DecisionState::Expired
                } else {
                    DecisionState::Accepted
                };
                rec.state = terminal;
                self.terminal_decisions += 1;
                terminal
            }
            other => other,
        }
    }

    /// 过期裁决：Pending → Expired；已终态 → 原样返回。
    pub fn expire(&mut self, offer_id: &str, now_ms: u64) -> DecisionState {
        let _ = now_ms; // expire 语义允许提前触发；absolute deadline 见 sweep
        let Some(rec) = self.offers.get_mut(offer_id) else {
            return DecisionState::Expired;
        };
        match rec.state {
            DecisionState::Pending => {
                rec.state = DecisionState::Expired;
                self.terminal_decisions += 1;
                DecisionState::Expired
            }
            other => other,
        }
    }

    /// 已记录的有效终态数（accept/expire race 恰好为 1）。
    pub fn terminal_decisions(&self) -> usize {
        self.terminal_decisions
    }

    // ----- 会话 -----

    pub fn register_session(
        &mut self,
        id: &SessionId,
        profile_id: &str,
        provider_id: &str,
        now_ms: u64,
    ) {
        self.sessions.insert(
            id.clone(),
            SessionRecord {
                profile_id: profile_id.to_string(),
                provider_id: provider_id.to_string(),
                state: SessionState::Active,
                worker: None,
                deadline_ms: None,
                registered_at_ms: now_ms,
            },
        );
    }

    pub fn set_session_deadline(&mut self, id: &SessionId, deadline_ms: u64) {
        if let Some(s) = self.sessions.get_mut(id) {
            s.deadline_ms = Some(deadline_ms);
        }
    }

    pub fn attach_worker(&mut self, id: &SessionId, worker: &str) {
        if let Some(s) = self.sessions.get_mut(id) {
            s.worker = Some(worker.to_string());
        }
    }

    pub fn session_state(&self, id: &SessionId) -> SessionState {
        self.sessions
            .get(id)
            .map(|s| s.state)
            .unwrap_or(SessionState::Failed)
    }

    /// 取消：幂等。第一次触发资源清理（记一条 cleanup 事件），之后重复调用
    /// 状态一致且不再清理（T10-02）。终态（Failed/Completed）不可再取消。
    pub fn cancel(&mut self, id: &SessionId, _now_ms: u64) -> DecisionState {
        let Some(s) = self.sessions.get_mut(id) else {
            return DecisionState::Cancelled;
        };
        match s.state {
            SessionState::Active | SessionState::Cancelling => {
                let first_cleanup = s.state == SessionState::Active;
                s.state = SessionState::Cancelled;
                if first_cleanup {
                    self.cleanups.push(id.to_string());
                }
                DecisionState::Cancelled
            }
            _ => session_as_decision(s.state),
        }
    }

    /// worker 崩溃：其挂靠的所有 Active 会话转 Failed 并清理；其他会话不受
    /// 影响（CORE-10：provider 失败独立降级）。返回受影响的会话列表。
    pub fn worker_crashed(&mut self, worker: &str, now_ms: u64) -> Vec<SessionId> {
        let _ = now_ms;
        let mut affected = Vec::new();
        for (id, s) in self.sessions.iter_mut() {
            if s.worker.as_deref() == Some(worker) && s.state == SessionState::Active {
                s.state = SessionState::Failed;
                affected.push(id.clone());
            }
        }
        for id in &affected {
            self.cleanups.push(id.to_string());
        }
        affected.sort();
        affected
    }

    /// absolute deadline 回收：超期 Active 会话转 Failed 并清理（APP-06）。
    pub fn sweep(&mut self, now_ms: u64) -> Vec<SessionId> {
        let mut expired = Vec::new();
        for (id, s) in self.sessions.iter_mut() {
            if s.state == SessionState::Active {
                if let Some(dl) = s.deadline_ms {
                    if now_ms > dl {
                        s.state = SessionState::Failed;
                        expired.push(id.clone());
                    }
                }
            }
        }
        for id in &expired {
            self.cleanups.push(id.to_string());
        }
        expired.sort();
        expired
    }

    pub fn cleanup_events(&self) -> &[String] {
        &self.cleanups
    }
}

fn session_as_decision(s: SessionState) -> DecisionState {
    match s {
        SessionState::Active => DecisionState::Active,
        SessionState::Cancelling => DecisionState::Cancelling,
        SessionState::Cancelled => DecisionState::Cancelled,
        SessionState::Failed => DecisionState::Failed,
        SessionState::Completed => DecisionState::Completed,
    }
}
