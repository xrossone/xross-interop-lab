//! 会话与资源记账（plans/03 T32-02/T32-04）：连接、认证、建流、回收。
//!
//! 硬规矩：
//! - **认证前不建流、不分配视频缓冲**（`authorizing` 与 `streaming` 分开）；
//! - 密钥不可得（FairPlay vendor-gated）→ `SETUP` 503，不建流；
//! - `TEARDOWN`/断开立即回收 session 与端口；快速连接断开 100 次不得残留；
//! - 超过并发上限 → `busy`（不静默超发）。

use crate::capability::{advertised_features, ImplementationInventory};
use crate::keying::KeyingProvider;
use crate::rtsp::{RtspRequest, RtspResponse, RtspStatus};
use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::SessionId;

/// 建流时按"已实现的缓冲"记账：认证前必须是 0。
const VIDEO_BUFFER_BYTES: u64 = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Unauthenticated,
    Authenticated,
    Streaming,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamInfo {
    video_buffer_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionRecord {
    state: SessionState,
    stream: Option<StreamInfo>,
    reserved_port: bool,
}

/// 接收端记账（连接/会话/流端口）。
pub struct AirPlayReceiver {
    max_sessions: u32,
    keying: Box<dyn KeyingProvider>,
    sessions: Vec<(SessionId, SessionRecord)>,
    next_id: u64,
}

impl AirPlayReceiver {
    pub fn new(max_sessions: u32, keying: Box<dyn KeyingProvider>) -> Self {
        Self {
            max_sessions,
            keying,
            sessions: Vec::new(),
            next_id: 1,
        }
    }

    pub fn keying_name(&self) -> &'static str {
        self.keying.name()
    }

    /// 接受一条新连接（占一个并发名额）。满 → `busy`。
    pub fn accept_connection(&mut self, _now_ms: u64) -> Result<SessionId, Error> {
        if self.active_sessions() as u32 >= self.max_sessions {
            return Err(Error::new(
                ErrorCode::Busy,
                format!("并发连接已达上限 {}", self.max_sessions),
            )
            .with_phase("authorizing"));
        }
        let id = SessionId::try_from(format!("ses_ap_{}", self.next_id))
            .map_err(|e| Error::new(ErrorCode::InvalidFrame, e))?;
        self.next_id += 1;
        self.sessions.push((
            id.clone(),
            SessionRecord {
                state: SessionState::Unauthenticated,
                stream: None,
                reserved_port: false,
            },
        ));
        Ok(id)
    }

    /// 断开：整条记录移除（session/端口/缓冲全部回收）。
    pub fn disconnect(&mut self, id: &SessionId, _now_ms: u64) {
        self.sessions.retain(|(sid, _)| sid != id);
    }

    pub fn state(&self, id: &SessionId) -> Option<SessionState> {
        self.record(id).map(|r| r.state)
    }

    pub fn has_stream(&self, id: &SessionId) -> bool {
        self.record(id).map(|r| r.stream.is_some()).unwrap_or(false)
    }

    pub fn allocated_video_bytes(&self, id: &SessionId) -> u64 {
        self.record(id)
            .and_then(|r| r.stream)
            .map(|s| s.video_buffer_bytes)
            .unwrap_or(0)
    }

    pub fn active_sessions(&self) -> usize {
        self.sessions
            .iter()
            .filter(|(_, r)| r.state != SessionState::Ended)
            .count()
    }

    pub fn tracked_sessions(&self) -> usize {
        self.sessions.len()
    }

    pub fn reserved_ports(&self) -> usize {
        self.sessions.iter().filter(|(_, r)| r.reserved_port).count()
    }

    /// 处理一条请求。返回的响应由调用方编码回写。
    pub fn handle(
        &mut self,
        id: &SessionId,
        req: &RtspRequest,
        _now_ms: u64,
    ) -> Result<RtspResponse, Error> {
        let cseq = req.cseq();
        if self.record(id).is_none() {
            return Err(Error::new(
                ErrorCode::DestinationUnavailable,
                "未知会话（已断开？）",
            )
            .with_phase("authorizing"));
        }
        let path = req.uri.split('?').next().unwrap_or(&req.uri).to_string();
        let state = self.state(id).expect("已确认存在");

        if state == SessionState::Ended {
            return Ok(RtspResponse::new(RtspStatus::BadRequest, cseq));
        }

        match (req.method.as_str(), path.as_str()) {
            // /info 在认证前可达（只暴露能力，不暴露设备细节）
            ("GET", "/info") | ("OPTIONS", "/info") => {
                let inv = ImplementationInventory::current();
                let features: Vec<&str> = advertised_features(&inv)
                    .iter()
                    .map(|f| f.as_wire())
                    .collect();
                let body = format!(
                    "features={}\nkeying={}\n",
                    features.join(","),
                    self.keying_name()
                )
                .into_bytes();
                Ok(RtspResponse::new(RtspStatus::Ok, cseq).with_body("text/plain", body))
            }
            ("POST", "/pair-pin-start") => Ok(RtspResponse::new(RtspStatus::Ok, cseq)),
            ("POST", "/pair-setup-pin") => {
                // 配对状态推进只在有 keying 时成立；否则端点可达但状态不推进。
                if self.keying.available() {
                    self.set_state(id, SessionState::Authenticated);
                }
                Ok(RtspResponse::new(RtspStatus::Ok, cseq))
            }
            ("POST", "/fp-setup") => {
                if !self.keying.available() {
                    return Err(Error::new(
                        ErrorCode::VendorGated,
                        "/fp-setup 需要 FairPlay 材料：本仓不实现（vendor-gated）",
                    )
                    .with_phase("authorizing"));
                }
                Ok(RtspResponse::new(RtspStatus::Ok, cseq))
            }
            ("SETUP", _) => {
                // 顺序刻意如此：密钥不可得 → 503（能力缺席）；未认证 → 401（不建流）
                if !self.keying.available() {
                    return Ok(RtspResponse::new(RtspStatus::ServiceUnavailable, cseq));
                }
                if self.state(id) != Some(SessionState::Authenticated) {
                    return Ok(RtspResponse::new(RtspStatus::Unauthorized, cseq));
                }
                self.derive_and_reserve(id)?;
                Ok(RtspResponse::new(RtspStatus::Ok, cseq))
            }
            ("PLAY", _) | ("RECORD", _) => {
                if self.state(id) == Some(SessionState::Streaming) {
                    Ok(RtspResponse::new(RtspStatus::Ok, cseq))
                } else {
                    Ok(RtspResponse::new(RtspStatus::BadRequest, cseq))
                }
            }
            ("TEARDOWN", _) => {
                self.teardown(id);
                Ok(RtspResponse::new(RtspStatus::Ok, cseq))
            }
            _ => Ok(RtspResponse::new(RtspStatus::NotFound, cseq)),
        }
    }

    fn derive_and_reserve(&mut self, id: &SessionId) -> Result<(), Error> {
        // 只有到这里才向 keying 要密钥：拿不到就不建流
        let keys = self.keying.derive_stream_keys()?;
        let _ = keys.expose_for_decrypt().len();
        let record = self.record_mut(id).expect("存在");
        record.stream = Some(StreamInfo {
            video_buffer_bytes: VIDEO_BUFFER_BYTES,
        });
        record.reserved_port = true;
        record.state = SessionState::Streaming;
        Ok(())
    }

    fn teardown(&mut self, id: &SessionId) {
        if let Some(record) = self.record_mut(id) {
            record.stream = None;
            record.reserved_port = false;
            record.state = SessionState::Ended;
        }
    }

    fn set_state(&mut self, id: &SessionId, state: SessionState) {
        if let Some(record) = self.record_mut(id) {
            // 终态不可逆
            if record.state != SessionState::Ended {
                record.state = state;
            }
        }
    }

    fn record(&self, id: &SessionId) -> Option<&SessionRecord> {
        self.sessions
            .iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, r)| r)
    }

    fn record_mut(&mut self, id: &SessionId) -> Option<&mut SessionRecord> {
        self.sessions
            .iter_mut()
            .find(|(sid, _)| sid == id)
            .map(|(_, r)| r)
    }
}
