//! Radio lease 模型（plans/01 T12；docs/07 §2）。
//!
//! 共享/独占两种模式，arbiter 是唯一的 lease 记账方。**arbiter 自身永不修改网络**：
//! 唯一的出口是注入的 [`RadioDriver`]，且只在"用户批准"之后调用一次 `apply`，
//! 释放/回收时按已批准 [`RollbackPlan`] 调用一次 `rollback`。
//!
//! 硬件/API 不可用时 fail-closed（`hardware-unavailable`）——例如 macOS 没有公开的
//! Wi-Fi Direct API，就明确拒绝 P2P 请求，而不是假装批准。

use crate::probe::SupportLevel;
use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::LeaseId;
use serde::Serialize;

/// 争用的电台资源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RadioResource {
    /// Wi-Fi 射频（共享 = 只读使用既有连接；独占 = 需要断开/切换）
    WifiRadio,
    /// Wi-Fi Direct / P2P 组网
    WifiDirect,
}

impl RadioResource {
    pub fn as_key(&self) -> &'static str {
        match self {
            Self::WifiRadio => "wifi_radio",
            Self::WifiDirect => "wifi_direct",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LeaseMode {
    Shared,
    /// 需要独占射频（会打断既有连接）——必须用户批准 + rollback
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LeaseState {
    Pending,
    Granted,
    Denied,
    Released,
    /// worker 退出后回收
    Reclaimed,
}

/// 批准时登记的恢复计划（释放/回收时执行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RollbackPlan {
    pub summary: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RadioLease {
    pub lease_id: LeaseId,
    pub resource: RadioResource,
    pub mode: LeaseMode,
    pub state: LeaseState,
    /// 申请者（provider/worker 标识）
    pub subject: String,
    pub reason: String,
    pub rollback: Option<RollbackPlan>,
    pub requested_at_ms: u64,
    pub decided_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioRequest {
    pub resource: RadioResource,
    pub mode: LeaseMode,
    pub subject: String,
    pub reason: String,
}

/// 用户裁决（交互审批由调用方收集；本模型不自己弹窗）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    /// 挂起等待用户决定：期间零网络变更
    Pending,
    UserGranted { rollback: RollbackPlan },
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RadioCapabilities {
    pub p2p: SupportLevel,
    pub wifi_display: SupportLevel,
    pub wifi_radio_present: bool,
}

/// 真正改网络的唯一出口。实现必须由 host 提供；arbiter 只在批准后调用。
pub trait RadioDriver {
    fn apply(&mut self, lease: &RadioLease) -> Result<(), Error>;
    fn rollback(&mut self, lease: &RadioLease) -> Result<(), Error>;
    /// 审计日志（fake 记录全部调用；真实实现可返回空）。
    fn calls(&self) -> &[String] {
        &[]
    }
}

/// 记录型 fake 驱动器（测试与模拟闭环用；不做任何真实网络操作）。
#[derive(Debug, Default)]
pub struct FakeRadioDriver {
    calls: Vec<String>,
}

impl FakeRadioDriver {
    pub fn calls(&self) -> &[String] {
        &self.calls
    }
}

impl RadioDriver for FakeRadioDriver {
    fn apply(&mut self, lease: &RadioLease) -> Result<(), Error> {
        self.calls.push(format!("apply:{}", lease.resource.as_key()));
        Ok(())
    }

    fn rollback(&mut self, lease: &RadioLease) -> Result<(), Error> {
        self.calls.push(format!("rollback:{}", lease.resource.as_key()));
        Ok(())
    }

    fn calls(&self) -> &[String] {
        &self.calls
    }
}

/// Radio lease 记账（唯一 authority）。
pub struct RadioArbiter {
    leases: Vec<RadioLease>,
    driver: Box<dyn RadioDriver>,
    caps: RadioCapabilities,
    seq: u32,
}

impl RadioArbiter {
    pub fn new(driver: Box<dyn RadioDriver>, caps: RadioCapabilities) -> Self {
        Self {
            leases: Vec::new(),
            driver,
            caps,
            seq: 0,
        }
    }

    pub fn leases(&self) -> &[RadioLease] {
        &self.leases
    }

    pub fn get(&self, id: &LeaseId) -> Option<&RadioLease> {
        self.leases.iter().find(|l| &l.lease_id == id)
    }

    pub fn pending(&self) -> Vec<&RadioLease> {
        self.leases
            .iter()
            .filter(|l| l.state == LeaseState::Pending)
            .collect()
    }

    /// 驱动器调用审计（性能/安全复核用）。
    pub fn driver_calls(&self) -> &[String] {
        self.driver.calls()
    }

    /// 直接申请（无交互）：独占且与现有活跃 lease 冲突 → `busy`；能力不可用 → `hardware-unavailable`。
    pub fn request(&mut self, req: RadioRequest, now_ms: u64) -> Result<LeaseId, Error> {
        self.check_capability(&req)?;
        if req.mode == LeaseMode::Exclusive && self.has_active_conflict(&req.resource, None) {
            return Err(Error::new(
                ErrorCode::Busy,
                format!(
                    "{} 上已有活跃 lease：独占请求不静默抢占，需用户批准",
                    req.resource.as_key()
                ),
            )
            .with_phase("authorizing"));
        }
        Ok(self.push(req, LeaseState::Granted, None, now_ms))
    }

    /// 带审批的申请：`Pending` 期间零网络变更；`UserGranted` 才调用 `apply`。
    pub fn request_with_approval(
        &mut self,
        req: RadioRequest,
        approval: Approval,
        now_ms: u64,
    ) -> Result<LeaseId, Error> {
        self.check_capability(&req)?;
        match approval {
            Approval::Pending => Ok(self.push(req, LeaseState::Pending, None, now_ms)),
            Approval::Denied => Ok(self.push(req, LeaseState::Denied, None, now_ms)),
            Approval::UserGranted { rollback } => {
                let id = self.push(req, LeaseState::Granted, Some(rollback), now_ms);
                let lease = self.snapshot(&id);
                self.driver.apply(&lease)?;
                Ok(id)
            }
        }
    }

    /// 批准挂起的 lease（登记 rollback 后执行一次 `apply`）。
    pub fn approve(
        &mut self,
        id: &LeaseId,
        rollback: RollbackPlan,
        now_ms: u64,
    ) -> Result<(), Error> {
        {
            let lease = self
                .leases
                .iter_mut()
                .find(|l| &l.lease_id == id)
                .ok_or_else(|| not_found(id))?;
            if lease.state != LeaseState::Pending {
                return Err(Error::new(
                    ErrorCode::Busy,
                    format!("lease {id} 不是 pending（{:?}）", lease.state),
                ));
            }
            lease.state = LeaseState::Granted;
            lease.rollback = Some(rollback);
            lease.decided_at_ms = now_ms;
        }
        let lease = self.snapshot(id);
        self.driver.apply(&lease)
    }

    /// 用户拒绝挂起的 lease（零网络变更）。
    pub fn deny(&mut self, id: &LeaseId, now_ms: u64) -> Result<(), Error> {
        let lease = self
            .leases
            .iter_mut()
            .find(|l| &l.lease_id == id)
            .ok_or_else(|| not_found(id))?;
        if lease.state != LeaseState::Pending {
            return Err(Error::new(
                ErrorCode::Busy,
                format!("lease {id} 不是 pending（{:?}）", lease.state),
            ));
        }
        lease.state = LeaseState::Denied;
        lease.decided_at_ms = now_ms;
        Ok(())
    }

    /// 释放：Granted → Released，并按已批准 rollback 恢复一次。
    pub fn release(&mut self, id: &LeaseId, now_ms: u64) -> Result<(), Error> {
        let (had_rollback, state) = {
            let lease = self
                .leases
                .iter_mut()
                .find(|l| &l.lease_id == id)
                .ok_or_else(|| not_found(id))?;
            let had_rollback = lease.rollback.is_some();
            if lease.state == LeaseState::Granted {
                lease.state = LeaseState::Released;
                lease.decided_at_ms = now_ms;
            }
            (had_rollback, lease.state)
        };
        if state == LeaseState::Released && had_rollback {
            let lease = self.snapshot(id);
            self.driver.rollback(&lease)?;
        }
        Ok(())
    }

    /// worker 退出：回收该 subject 的 Granted lease（执行 rollback），pending 作废。
    /// 返回被回收的 lease id。
    pub fn worker_exit(&mut self, subject: &str, now_ms: u64) -> Vec<LeaseId> {
        let mut reclaimed = Vec::new();
        let mut to_rollback: Vec<LeaseId> = Vec::new();
        for lease in self.leases.iter_mut() {
            if lease.subject != subject {
                continue;
            }
            match lease.state {
                LeaseState::Granted => {
                    lease.state = LeaseState::Reclaimed;
                    lease.decided_at_ms = now_ms;
                    reclaimed.push(lease.lease_id.clone());
                    if lease.rollback.is_some() {
                        to_rollback.push(lease.lease_id.clone());
                    }
                }
                LeaseState::Pending => {
                    lease.state = LeaseState::Denied;
                    lease.decided_at_ms = now_ms;
                }
                _ => {}
            }
        }
        for id in to_rollback {
            let lease = self.snapshot(&id);
            // 回收路径的 rollback 失败不应阻止回收；错误由调用方从日志/审计处理
            let _ = self.driver.rollback(&lease);
        }
        reclaimed
    }

    fn check_capability(&self, req: &RadioRequest) -> Result<(), Error> {
        if !self.caps.wifi_radio_present {
            return Err(Error::new(
                ErrorCode::HardwareUnavailable,
                "本机无 Wi-Fi 射频",
            ));
        }
        if req.resource == RadioResource::WifiDirect
            && self.caps.p2p != SupportLevel::Supported
        {
            return Err(Error::new(
                ErrorCode::HardwareUnavailable,
                format!("Wi-Fi Direct/P2P 不可用（probe: {:?}）", self.caps.p2p),
            ));
        }
        Ok(())
    }

    fn has_active_conflict(&self, resource: &RadioResource, except: Option<&LeaseId>) -> bool {
        self.leases.iter().any(|l| {
            &l.resource == resource
                && matches!(l.state, LeaseState::Granted | LeaseState::Pending)
                && except.map(|e| e != &l.lease_id).unwrap_or(true)
        })
    }

    fn push(
        &mut self,
        req: RadioRequest,
        state: LeaseState,
        rollback: Option<RollbackPlan>,
        now_ms: u64,
    ) -> LeaseId {
        self.seq += 1;
        let id = LeaseId::try_from(format!("rl_{}", self.seq)).expect("lease id 形状合法");
        self.leases.push(RadioLease {
            lease_id: id.clone(),
            resource: req.resource,
            mode: req.mode,
            state,
            subject: req.subject,
            reason: req.reason,
            rollback,
            requested_at_ms: now_ms,
            decided_at_ms: now_ms,
        });
        id
    }

    fn snapshot(&self, id: &LeaseId) -> RadioLease {
        self.get(id).cloned().expect("lease 必须存在")
    }
}

fn not_found(id: &LeaseId) -> Error {
    Error::new(ErrorCode::DestinationUnavailable, format!("未知 lease {id}"))
        .with_phase("authorizing")
}
