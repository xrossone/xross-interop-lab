//! sender 侧控制会话（计划 §T43 的 `crates/proto-cast/src/controller.rs`）。
//!
//! 分层：**通道闸门 seam → 连接（tp.connection）→ app 拉起（receiver）→ 媒体会话（media）→ 收尾**。
//! 四条 T43 验收约束在代码里的落点：
//!
//! - **T43-01**（认证失败不启动播放）：所有命令在发出前都要过 [`ChannelGate::ensure_open`]；
//!   生产闸门 [`UnavailableGate`] 恒拒绝（F-18 的设备证书材料本仓没有），因此**一条命令都发不出去**；
//! - **T43-02**（launch 失败是明确错误、不是 media loading）：`LAUNCH_ERROR` → [`Event::LaunchFailed`]，
//!   状态回到 `Connected`，后续 `load` 因状态检查被拒；
//! - **T43-03**（停止或被接收端终止要清理 URL 与控制状态）：`stop()`/接收端 STOP/应用消失/
//!   `MEDIA_STATUS` 转入 idle 都产生 [`Event::MediaUrlReleased`] 并清空 `mediaSessionId`；
//! - **T43-04**（只 URL 可用）：[`Controller::screen_capability`] 恒为 `false`——本 profile 只推媒体 URL。
//!
//! 时间由调用方传入（`now_ms`），本层不做真实等待。

use crate::castv2::{CastMessage, RECEIVER_PLATFORM_ID};
use crate::namespaces::{
    decode_message, CastPayload, MediaStatus, PlayerState, ReceiverStatus, SenderInfo, NS_MEDIA,
};
use interop_contract::error::{Error, ErrorCode};

/// 心跳发送间隔（F-09：来源取值 `HB_PING_TIME = 10`，**不是协议常量**）。
pub const HEARTBEAT_PING_MS: u64 = 10_000;
/// 心跳应答窗口（F-09：来源取值 `HB_PONG_TIME = 10`，**不是协议常量**）。
pub const HEARTBEAT_PONG_MS: u64 = 10_000;
/// 连接失效判定（F-09：来源把两者之和作为过期阈值）。
pub const HEARTBEAT_EXPIRY_MS: u64 = HEARTBEAT_PING_MS + HEARTBEAT_PONG_MS;
/// 请求超时（F-16：来源库级默认 10 s，属**实现取值**）。
pub const REQUEST_TIMEOUT_MS: u64 = 10_000;

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn gated(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::VendorGated, msg)
}

/// 通道闸门：正式握手（DeviceAuth）**本仓不实现**，因此生产实现缺席 = 能力缺席。
///
/// 之所以做成 trait 而不是布尔开关：缺的是一整套设备证书材料与签名流程，
/// 不是一个可以被测试替身"打开"的开关；测试替身只是为了让控制流可测。
pub trait ChannelGate {
    /// 通道是否已可用于发送控制命令。
    fn ensure_open(&self) -> Result<(), Error>;
    /// 供报告展示的实现描述（如 "unavailable: device credentials not available"）。
    fn describe(&self) -> &'static str;
}

/// 生产实现：恒拒绝（缺少设备侧凭据材料；F-18）。
pub struct UnavailableGate;

impl ChannelGate for UnavailableGate {
    fn ensure_open(&self) -> Result<(), Error> {
        Err(gated(
            "Cast 通道未建立：设备侧凭据材料不可得，握手流程本仓不实现（F-18）；因此不发送任何命令",
        ))
    }

    fn describe(&self) -> &'static str {
        "unavailable"
    }
}

/// 测试/演示替身：闸门恒开。**不代表本仓实现了握手**——它只让控制流可被验证。
pub struct OpenGateForTesting;

impl ChannelGate for OpenGateForTesting {
    fn ensure_open(&self) -> Result<(), Error> {
        Ok(())
    }

    fn describe(&self) -> &'static str {
        "open-for-testing"
    }
}

/// 控制器状态（本仓记账；字段表没有固定状态名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerState {
    Idle,
    Connecting,
    Connected,
    Launching,
    Launched,
    MediaSession,
    Ended,
}

impl ControllerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Launching => "launching",
            Self::Launched => "launched",
            Self::MediaSession => "media-session",
            Self::Ended => "ended",
        }
    }
}

/// 会话事件。
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Connected,
    Launched {
        app_id: String,
        transport_id: Option<String>,
        session_id: Option<String>,
    },
    /// T43-02：app 拉起失败是**明确错误**，不是 media loading 失败。
    LaunchFailed {
        reason: String,
    },
    MediaStatus(Box<MediaStatus>),
    MediaLoadFailed {
        reason: String,
    },
    /// T43-03：媒体 URL 必须被释放（lease 归宿主，见 F-24）。
    MediaUrlReleased {
        url: String,
        why: &'static str,
    },
    ChannelExpired,
    RequestTimedOut {
        request_id: u64,
        type_name: &'static str,
    },
}

/// 一次推进的产出：要发出的消息 + 事件。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub outbound: Vec<CastMessage>,
    pub events: Vec<Event>,
}

impl Outcome {
    fn message(message: CastMessage) -> Self {
        Self {
            outbound: vec![message],
            events: Vec::new(),
        }
    }
}

struct Pending {
    request_id: u64,
    type_name: &'static str,
    sent_ms: u64,
}

/// 会话统计（演示与证据用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerStats {
    pub state: ControllerState,
    pub gate: &'static str,
    pub sent: u32,
    pub received: u32,
    pub pending: u32,
    pub timeouts: u32,
    pub pings: u32,
    pub pongs: u32,
    pub media_url_held: bool,
    /// 本 profile 只推媒体 URL：屏幕能力恒 false（T43-04）。
    pub screen_capability: bool,
}

/// sender 侧控制器。
pub struct Controller {
    gate: Box<dyn ChannelGate>,
    sender_id: String,
    configured_app_ids: Vec<String>,
    state: ControllerState,
    request_id: u64,
    pending: Vec<Pending>,
    transport_id: Option<String>,
    session_id: Option<String>,
    app_id: Option<String>,
    media_session_id: Option<i64>,
    media_url: Option<String>,
    last_outbound_ms: Option<u64>,
    last_inbound_ms: Option<u64>,
    sent: u32,
    received: u32,
    timeouts: u32,
    pings: u32,
    pongs: u32,
}

impl Controller {
    /// `sender_id`：本端在通道上的标识（由宿主生成）；
    /// `configured_app_ids`：允许拉起的 app ID（F-17：不在配置里的一律拒绝，绝不猜）。
    pub fn new(
        gate: Box<dyn ChannelGate>,
        sender_id: impl Into<String>,
        configured_app_ids: Vec<String>,
    ) -> Result<Self, Error> {
        let sender_id = sender_id.into();
        if sender_id.is_empty() {
            return Err(invalid("sender_id 不得为空（F-01 的 required 字段）"));
        }
        if configured_app_ids.is_empty() {
            return Err(invalid(
                "必须显式配置允许的 app ID（F-17：不伪造、不猜默认 app ID）",
            ));
        }
        Ok(Self {
            gate,
            sender_id,
            configured_app_ids,
            state: ControllerState::Idle,
            request_id: 0,
            pending: Vec::new(),
            transport_id: None,
            session_id: None,
            app_id: None,
            media_session_id: None,
            media_url: None,
            last_outbound_ms: None,
            last_inbound_ms: None,
            sent: 0,
            received: 0,
            timeouts: 0,
            pings: 0,
            pongs: 0,
        })
    }

    pub fn state(&self) -> ControllerState {
        self.state
    }

    pub fn sender_id(&self) -> &str {
        &self.sender_id
    }

    pub fn app_id(&self) -> Option<&str> {
        self.app_id.as_deref()
    }

    pub fn media_session_id(&self) -> Option<i64> {
        self.media_session_id
    }

    pub fn media_url(&self) -> Option<&str> {
        self.media_url.as_deref()
    }

    /// T43-04：本 profile 只做媒体 URL，不提供屏幕镜像。
    pub fn screen_capability(&self) -> bool {
        false
    }

    pub fn stats(&self) -> ControllerStats {
        ControllerStats {
            state: self.state,
            gate: self.gate.describe(),
            sent: self.sent,
            received: self.received,
            pending: self.pending.len() as u32,
            timeouts: self.timeouts,
            pings: self.pings,
            pongs: self.pongs,
            media_url_held: self.media_url.is_some(),
            screen_capability: self.screen_capability(),
        }
    }

    fn next_request_id(&mut self) -> u64 {
        self.request_id += 1;
        self.request_id
    }

    /// 命令出口：先过闸门（T43-01），再记账。
    fn emit(&mut self, payload: CastPayload, now_ms: u64) -> Result<CastMessage, Error> {
        self.gate.ensure_open()?;
        if self.state == ControllerState::Ended {
            return Err(invalid("会话已结束：不再发送命令"));
        }
        let destination = match payload.namespace() {
            NS_MEDIA => self
                .transport_id
                .clone()
                .ok_or_else(|| invalid("媒体命令需要 transportId：先完成 app 拉起（F-10）"))?,
            _ => RECEIVER_PLATFORM_ID.to_string(),
        };
        let request_id = self.next_request_id();
        let message = payload.into_message(&self.sender_id, &destination, Some(request_id))?;
        self.pending.push(Pending {
            request_id,
            type_name: message_type_of(&message),
            sent_ms: now_ms,
        });
        self.sent += 1;
        self.last_outbound_ms = Some(now_ms);
        Ok(message)
    }

    /// 建立连接（tp.connection 的 CONNECT）。
    pub fn connect(&mut self, now_ms: u64) -> Result<Outcome, Error> {
        if self.state != ControllerState::Idle {
            return Err(invalid(format!(
                "CONNECT 只能从 idle 发出，当前状态 {}",
                self.state.as_str()
            )));
        }
        // T43-01：闸门必须在**任何状态变更之前**检查——认证失败不得留下半个会话。
        self.gate.ensure_open()?;
        self.state = ControllerState::Connecting;
        let payload = CastPayload::Connect {
            user_agent: "xross-interop/0.1".to_string(),
            sender_info: SenderInfo {
                sdk_type: 2,
                version: "0.1".to_string(),
                browser_version: String::new(),
                platform: 0,
                connection_type: 0,
            },
        };
        self.emit(payload, now_ms).map(Outcome::message)
    }

    /// 拉起 app（receiver 的 LAUNCH）。
    pub fn launch(&mut self, app_id: &str, now_ms: u64) -> Result<Outcome, Error> {
        if self.state != ControllerState::Connected {
            return Err(invalid(format!(
                "LAUNCH 只能在 CONNECTED 之后发出，当前状态 {}",
                self.state.as_str()
            )));
        }
        if !self.configured_app_ids.iter().any(|id| id == app_id) {
            return Err(invalid(format!(
                "app ID {app_id:?} 不在本节点配置的允许列表里（F-17：不伪造、不猜 app ID）"
            )));
        }
        self.gate.ensure_open()?;
        self.state = ControllerState::Launching;
        self.app_id = Some(app_id.to_string());
        let payload = CastPayload::Launch {
            app_id: app_id.to_string(),
        };
        self.emit(payload, now_ms).map(Outcome::message)
    }

    /// 载入媒体 URL（media 的 LOAD）。
    pub fn load(
        &mut self,
        media_url: &str,
        content_type: &str,
        title: Option<&str>,
        now_ms: u64,
    ) -> Result<Outcome, Error> {
        if self.state != ControllerState::Launched {
            return Err(invalid(format!(
                "LOAD 需要已拉起的 app（T43-02：launch 失败不得被当成 media loading，当前状态 {}）",
                self.state.as_str()
            )));
        }
        validate_media_url(media_url)?;
        let payload = CastPayload::Load(crate::namespaces::LoadRequest {
            content_id: media_url.to_string(),
            content_type: content_type.to_string(),
            stream_type: crate::namespaces::StreamType::Buffered,
            title: title.map(str::to_string),
            autoplay: true,
            current_time: None,
        });
        let outcome = self.emit(payload, now_ms).map(Outcome::message)?;
        self.media_url = Some(media_url.to_string());
        Ok(outcome)
    }

    pub fn play(&mut self, now_ms: u64) -> Result<Outcome, Error> {
        self.media_command(CastPayload::Play, now_ms)
    }

    pub fn pause(&mut self, now_ms: u64) -> Result<Outcome, Error> {
        self.media_command(CastPayload::Pause, now_ms)
    }

    pub fn seek(&mut self, current_time: f64, now_ms: u64) -> Result<Outcome, Error> {
        if !current_time.is_finite() || current_time < 0.0 {
            return Err(invalid("SEEK 的 currentTime 必须是有限非负数（F-13）"));
        }
        self.media_command(
            CastPayload::Seek {
                current_time,
                resume_state: crate::namespaces::RESUME_STATE_PLAYBACK_START,
            },
            now_ms,
        )
    }

    /// 停止播放（T43-03：清空会话状态并报告 URL 必须释放）。
    pub fn stop(&mut self, now_ms: u64) -> Result<Outcome, Error> {
        let mut outcome = self.media_command(CastPayload::MediaStop, now_ms)?;
        self.release_media_url("stop", &mut outcome.events);
        self.media_session_id = None;
        Ok(outcome)
    }

    /// 主动断开（tp.connection 的 CLOSE）。
    pub fn close(&mut self, now_ms: u64) -> Result<Outcome, Error> {
        let _ = now_ms;
        self.gate.ensure_open()?;
        let request_id = self.next_request_id();
        let message = CastPayload::Close
            .into_message(&self.sender_id, RECEIVER_PLATFORM_ID, Some(request_id))?;
        self.sent += 1;
        let mut outcome = Outcome::message(message);
        self.release_media_url("close", &mut outcome.events);
        self.state = ControllerState::Ended;
        self.pending.clear();
        Ok(outcome)
    }

    fn media_command(&mut self, payload: CastPayload, now_ms: u64) -> Result<Outcome, Error> {
        if self.state != ControllerState::MediaSession {
            return Err(invalid(format!(
                "媒体命令需要活动媒体会话，当前状态 {}",
                self.state.as_str()
            )));
        }
        self.gate.ensure_open()?;
        self.emit(payload, now_ms).map(Outcome::message)
    }

    fn release_media_url(&mut self, why: &'static str, events: &mut Vec<Event>) {
        if let Some(url) = self.media_url.take() {
            events.push(Event::MediaUrlReleased { url, why });
        }
    }

    /// 收一条消息：事件 + 需要回发的消息（PONG 等）。
    pub fn on_message(&mut self, message: &CastMessage, now_ms: u64) -> Result<Outcome, Error> {
        self.received += 1;
        self.last_inbound_ms = Some(now_ms);
        // 应答关账：带 requestId 的必须是待决请求，否则明确拒绝（迟到/伪造一律不认）。
        let (payload, request_id) = decode_message(message)?;
        if let Some(request_id) = request_id {
            let index = self
                .pending
                .iter()
                .position(|p| p.request_id == request_id)
                .ok_or_else(|| {
                    invalid(format!("requestId {request_id} 不在待决请求里（F-16）：明确拒绝"))
                })?;
            self.pending.remove(index);
        }

        let mut outcome = Outcome::default();
        match payload {
            CastPayload::Connected => {
                if self.state != ControllerState::Connecting {
                    return Err(invalid("未在连接中就收到 CONNECTED（tp.connection）"));
                }
                self.state = ControllerState::Connected;
                outcome.events.push(Event::Connected);
            }
            CastPayload::Ping => {
                self.pings += 1;
                // F-09：收到 PING 必须回 PONG（不注入 requestId）。
                outcome.outbound.push(
                    CastPayload::Pong
                        .into_message(&self.sender_id, &message.source_id, None)?,
                );
            }
            CastPayload::Pong => {
                self.pongs += 1;
            }
            CastPayload::Close => {
                self.state = ControllerState::Ended;
                self.release_media_url("peer-close", &mut outcome.events);
            }
            CastPayload::ReceiverStatus(status) => {
                self.apply_receiver_status(&status, &mut outcome.events)?;
            }
            CastPayload::LaunchError { reason, app_id } => {
                // T43-02：明确错误，且不进入媒体会话。
                self.state = ControllerState::Connected;
                self.app_id = None;
                self.transport_id = None;
                self.session_id = None;
                outcome.events.push(Event::LaunchFailed {
                    reason: match app_id {
                        Some(app_id) => format!("{reason}（appId={app_id}）"),
                        None => reason,
                    },
                });
            }
            CastPayload::MediaStatus(status) => {
                self.apply_media_status(&status, &mut outcome.events);
                outcome.events.push(Event::MediaStatus(status));
            }
            CastPayload::LoadFailed { reason } => {
                self.release_media_url("load-failed", &mut outcome.events);
                outcome
                    .events
                    .push(Event::MediaLoadFailed { reason });
            }
            other => {
                return Err(invalid(format!(
                    "收到不适用于 sender 侧的消息类型 {:?}（命名空间 {}）",
                    other.type_name(),
                    other.namespace()
                )))
            }
        }
        Ok(outcome)
    }

    fn apply_receiver_status(
        &mut self,
        status: &ReceiverStatus,
        events: &mut Vec<Event>,
    ) -> Result<(), Error> {
        match status.transport_id() {
            Some(transport_id) => {
                let was_launched = self.state == ControllerState::Launching
                    || self.state == ControllerState::Launched
                    || self.state == ControllerState::MediaSession;
                self.transport_id = Some(transport_id.to_string());
                self.session_id = status.session_id().map(str::to_string);
                if let Some(app_id) = status.app_id() {
                    self.app_id = Some(app_id.to_string());
                }
                if !was_launched {
                    self.state = ControllerState::Launched;
                } else if self.state == ControllerState::Launching {
                    self.state = ControllerState::Launched;
                    events.push(Event::Launched {
                        app_id: status.app_id().unwrap_or_default().to_string(),
                        transport_id: Some(transport_id.to_string()),
                        session_id: status.session_id().map(str::to_string),
                    });
                } else if self.state == ControllerState::Launched {
                    events.push(Event::Launched {
                        app_id: status.app_id().unwrap_or_default().to_string(),
                        transport_id: Some(transport_id.to_string()),
                        session_id: status.session_id().map(str::to_string),
                    });
                }
            }
            None => {
                // 应用列表为空 = 接收端把应用停了（T43-03）。
                if matches!(self.state, ControllerState::Launched | ControllerState::MediaSession) {
                    self.state = ControllerState::Connected;
                    self.transport_id = None;
                    self.session_id = None;
                    self.media_session_id = None;
                    self.release_media_url("receiver-stopped", events);
                }
            }
        }
        Ok(())
    }

    fn apply_media_status(&mut self, status: &MediaStatus, events: &mut Vec<Event>) {
        self.state = ControllerState::MediaSession;
        self.media_session_id = status.media_session_id;
        let idle = status.player_state == PlayerState::Idle
            || status.idle_reason.as_deref().is_some_and(|reason| {
                matches!(reason, "FINISHED" | "CANCELLED" | "INTERRUPTED" | "ERROR")
            });
        if idle {
            self.media_session_id = None;
            self.release_media_url("media-idle", events);
        }
    }

    /// 时间推进：心跳发送、请求超时、连接失效。
    pub fn tick(&mut self, now_ms: u64) -> Result<Outcome, Error> {
        let mut outcome = Outcome::default();
        if self.state == ControllerState::Ended || self.state == ControllerState::Idle {
            return Ok(outcome);
        }
        // 请求超时（F-16 的 10 s 实现取值）。
        let mut timed_out = Vec::new();
        self.pending.retain(|pending| {
            if now_ms.saturating_sub(pending.sent_ms) > REQUEST_TIMEOUT_MS {
                timed_out.push((pending.request_id, pending.type_name));
                false
            } else {
                true
            }
        });
        for (request_id, type_name) in timed_out {
            self.timeouts += 1;
            outcome.events.push(Event::RequestTimedOut {
                request_id,
                type_name,
            });
        }
        // 心跳：到点发 PING；超过窗口没有任何入站消息 → 判定连接失效。
        if self.state == ControllerState::Connected
            || self.state == ControllerState::Launched
            || self.state == ControllerState::MediaSession
        {
            let due = match self.last_outbound_ms {
                Some(last) => now_ms.saturating_sub(last) >= HEARTBEAT_PING_MS,
                None => false,
            };
            if due {
                self.gate.ensure_open()?;
                let message =
                    CastPayload::Ping.into_message(&self.sender_id, RECEIVER_PLATFORM_ID, None)?;
                outcome.outbound.push(message);
                self.last_outbound_ms = Some(now_ms);
            }
            if let Some(last_inbound) = self.last_inbound_ms {
                if now_ms.saturating_sub(last_inbound) > HEARTBEAT_EXPIRY_MS {
                    self.state = ControllerState::Ended;
                    self.pending.clear();
                    self.release_media_url("channel-expired", &mut outcome.events);
                    outcome.events.push(Event::ChannelExpired);
                }
            }
        }
        Ok(outcome)
    }
}

fn message_type_of(message: &CastMessage) -> &'static str {
    // 只用于超时事件的可读标签：解析失败就退回命名空间。
    decode_message(message)
        .map(|(payload, _)| payload.type_name())
        .unwrap_or("unknown")
}

/// 媒体 URL 形状（本仓策略）：只接受 http(s)、无 userinfo、有主机名。
pub fn validate_media_url(url: &str) -> Result<(), Error> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| invalid(format!("媒体 URL 必须是 http(s)：{url:?}")))?;
    if rest.contains('@') {
        return Err(invalid("媒体 URL 不得带 userinfo"));
    }
    let host = rest.split(['/', '?']).next().unwrap_or("");
    if host.is_empty() {
        return Err(invalid("媒体 URL 缺主机名"));
    }
    Ok(())
}
