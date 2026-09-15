//! DLNA renderer（DMR）接收侧（plans/03 T42）。事实行见 `specs-reviewed/m07-dlna-upnp-av.md` 的 F-15..F-22。
//!
//! 四条 T42 约束在代码里的落点：
//!
//! - **T42-01**（`SetAVTransportURI` 为 `file://` → 拒绝）：[`Dmr::set_uri`] 只接受 http(s)，
//!   并对内网/元数据目标走策略拒绝；**拒绝时不留任何播放入口**；
//! - **T42-02**（重复 `Stop` → 幂等结束）：[`Dmr::stop`] 在 `STOPPED`/`NO_MEDIA_PRESENT` 下也返回成功；
//! - **T42-03**（live 流 `Seek` 不支持 → 710）：[`Dmr::seek`] 对无时长媒体与未实现单位都返回 710；
//! - **T42-04**（外来订阅 callback 越界 → 拒绝）：[`Dmr::subscribe`] 对回调地址做形状与策略检查，
//!   拒绝时不建立订阅。
//!
//! **本层不做网络 I/O**：策略检查只做形状/范围判断，控制路径不 fetch、不建立回调连接（F-22）。
//! 播放进度由本机播放器经 [`Dmr::on_playback_position`] 反馈，不在控制线程里执行。

use crate::dmc::DescriptionFetchPolicy;
use crate::soap::{
    SoapAction, SoapMessage, AV_TRANSPORT, CHANNEL_MASTER, RENDERING_CONTROL,
};
use interop_contract::error::ErrorCode;
use std::collections::BTreeMap;

/// 错误码（F-16：R49 `src/librygel-renderer/rygel-av-transport.vala:481,499,529,535,554,560,573,584,598,610,659`；
/// `402` 取自 R49 各服务通用的参数错误用法，见 F-08）。
pub const ERR_INVALID_ARGUMENT: u32 = 402;
pub const ERR_TRANSITION_NOT_AVAILABLE: u32 = 701;
pub const ERR_SEEK_MODE_NOT_SUPPORTED: u32 = 710;
pub const ERR_ILLEGAL_SEEK_TARGET: u32 = 711;
pub const ERR_PLAY_MODE_NOT_SUPPORTED: u32 = 712;
pub const ERR_PLAY_SPEED_NOT_SUPPORTED: u32 = 717;
pub const ERR_INVALID_INSTANCE_ID: u32 = 718;
/// 订阅数超上限时复用 ContentDirectory 的 `CANT_PROCESS`（F-08）：UPnP 没有为这种情形固定错误码，
/// 这是**本仓策略**的复用，描述里会写明原因。
pub const ERR_CANT_PROCESS: u32 = 720;

/// 本仓唯一实现的 `Seek` 单位（F-17：来源列出六种允许值，本仓只实现 `REL_TIME`）。
pub const SUPPORTED_SEEK_MODE: &str = "REL_TIME";

/// 订阅数上限（**本仓策略**；F-10 只说明服务端有上限，没有给数值）。
pub const DEFAULT_MAX_SUBSCRIPTIONS: usize = 8;
/// 订阅超时上限（秒，**本仓策略**）。
pub const MAX_SUBSCRIPTION_TIMEOUT_SECS: u32 = 1800;

/// 统一播放速度只认 `1`（F-17：`TransportPlaySpeed` 是自由字符串、默认 `1`；其余 → 717）。
pub const SUPPORTED_PLAY_SPEED: &str = "1";

/// DLNA 的 fault（UPnPError 包装）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmrFault {
    pub code: u32,
    pub description: String,
}

impl DmrFault {
    fn new(code: u32, description: impl Into<String>) -> Self {
        Self {
            code,
            description: description.into(),
        }
    }

    /// 转成 SOAP Fault 消息（由调用方通过数据面回给控制器）。
    pub fn to_soap(&self) -> SoapMessage {
        SoapMessage::fault(self.code, &self.description)
    }
}

/// AVTransport 状态（F-15：来源固定七个允许值；本仓不使用录音相关的两个）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    NoMediaPresent,
    Stopped,
    Playing,
    PausedPlayback,
    Transitioning,
}

impl TransportState {
    /// 线上取值逐字对应 F-15 的 `allowedValue`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoMediaPresent => "NO_MEDIA_PRESENT",
            Self::Stopped => "STOPPED",
            Self::Playing => "PLAYING",
            Self::PausedPlayback => "PAUSED_PLAYBACK",
            Self::Transitioning => "TRANSITIONING",
        }
    }
}

/// `TransportStatus`（F-15）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportStatus {
    Ok,
    ErrorOccurred,
}

impl TransportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::ErrorOccurred => "ERROR_OCCURRED",
        }
    }
}

/// 已接收的媒体（F-19：`CurrentTrackURI`/`CurrentMediaDuration`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaEntry {
    pub uri: String,
    pub title: Option<String>,
    /// `None` 表示时长未知 → 视为直播流（F-21 的策略据此拒绝 `Seek`）。
    pub duration_secs: Option<u64>,
}

impl MediaEntry {
    pub fn is_live(&self) -> bool {
        self.duration_secs.is_none()
    }
}

/// `GetPositionInfo` 的结果（F-19）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionInfo {
    pub track: u32,
    pub rel_time_secs: u64,
    pub duration_secs: Option<u64>,
}

/// 一条 GENA 订阅（F-10）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub sid: String,
    pub callback: String,
    pub timeout_secs: u32,
    pub seq: u32,
}

/// `SetAVTransportURI` 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UriDecision {
    /// 已接受，但**必须等用户同意**才能播放（T42-01：先过 policy/用户同意）。
    NeedsUserConsent,
    /// 已接受且此前已同意（例如同一 URL 重新设置）。
    Accepted,
}

/// 一次 SOAP 动作的处理结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DmrReply {
    /// `Some` 表示返回 SOAP Fault。
    pub fault: Option<DmrFault>,
    /// 成功时的返回参数（如 `CurrentTransportState`）。
    pub args: BTreeMap<String, String>,
}

impl DmrReply {
    fn ok(args: &[(&str, String)]) -> Self {
        Self {
            fault: None,
            args: args
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }

    fn fault(fault: DmrFault) -> Self {
        Self {
            fault: Some(fault),
            args: BTreeMap::new(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.fault.is_none()
    }
}

/// 渲染器（接收侧）。
pub struct Dmr {
    policy: DescriptionFetchPolicy,
    max_subscriptions: usize,
    state: TransportState,
    status: TransportStatus,
    media: Option<MediaEntry>,
    consented_uri: Option<String>,
    position_secs: u64,
    volume: u8,
    muted: bool,
    subscriptions: Vec<Subscription>,
    next_sid: u32,
}

impl Dmr {
    pub fn new(policy: DescriptionFetchPolicy) -> Self {
        Self {
            policy,
            max_subscriptions: DEFAULT_MAX_SUBSCRIPTIONS,
            state: TransportState::NoMediaPresent,
            status: TransportStatus::Ok,
            media: None,
            consented_uri: None,
            position_secs: 0,
            volume: 50,
            muted: false,
            subscriptions: Vec::new(),
            next_sid: 1,
        }
    }

    pub fn state(&self) -> TransportState {
        self.state
    }

    pub fn transport_status(&self) -> TransportStatus {
        self.status
    }

    pub fn current_uri(&self) -> Option<&str> {
        self.media.as_ref().map(|media| media.uri.as_str())
    }

    pub fn media(&self) -> Option<&MediaEntry> {
        self.media.as_ref()
    }

    pub fn position_secs(&self) -> u64 {
        self.position_secs
    }

    /// 音量 0..100（F-18 的 `Volume` 范围）。
    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn subscriptions(&self) -> &[Subscription] {
        &self.subscriptions
    }

    /// `CurrentTransportActions`（F-19）——由当前状态推导，不是常量表。
    pub fn current_transport_actions(&self) -> Vec<&'static str> {
        match self.state {
            TransportState::NoMediaPresent => vec!["Stop"],
            TransportState::Stopped => vec!["Play", "Stop"],
            TransportState::Playing => vec!["Pause", "Stop", "Seek"],
            TransportState::PausedPlayback => vec!["Play", "Stop", "Seek"],
            TransportState::Transitioning => vec!["Stop"],
        }
    }

    // ---------------------------------------------------------------- AVTransport

    /// `SetAVTransportURI`（T42-01）。
    pub fn set_uri(&mut self, uri: &str, _metadata: Option<&str>) -> Result<UriDecision, DmrFault> {
        if uri.is_empty() {
            return Err(DmrFault::new(ERR_INVALID_ARGUMENT, "CurrentURI 不得为空"));
        }
        if uri.len() > self.policy.max_url_bytes {
            return Err(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                format!(
                    "CurrentURI {} 字节超过策略上限 {}",
                    uri.len(),
                    self.policy.max_url_bytes
                ),
            ));
        }
        let scheme_ok = uri.starts_with("http://") || uri.starts_with("https://");
        if !scheme_ok {
            // T42-01：只接受 http(s)。file:// 等本地路径明确拒绝，且不留播放入口。
            return Err(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                format!("只接受 http(s) 媒体 URL（T42-01）：{uri:?} 不是可拉取的网络资源"),
            ));
        }
        // 内网/元数据/带 userinfo 的目标走既有抓取策略（默认拒绝 → permission-required）。
        if let Err(error) = self.policy.check(uri) {
            let reason = match error.code {
                ErrorCode::PermissionRequired => error.message,
                _ => format!("URL 形状非法：{}", error.message),
            };
            return Err(DmrFault::new(ERR_INVALID_ARGUMENT, reason));
        }
        let needs_consent = self.consented_uri.as_deref() != Some(uri);
        self.media = Some(MediaEntry {
            uri: uri.to_string(),
            title: None,
            duration_secs: None,
        });
        self.position_secs = 0;
        self.state = TransportState::Stopped;
        if needs_consent {
            Ok(UriDecision::NeedsUserConsent)
        } else {
            Ok(UriDecision::Accepted)
        }
    }

    /// 用户同意某个 URI 之后才允许 `Play`（T42 实现决策：先过 policy/用户同意）。
    pub fn grant_consent(&mut self, uri: &str) -> Result<(), DmrFault> {
        if self.current_uri() != Some(uri) {
            return Err(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                "只能对当前已设置的 URI 授权",
            ));
        }
        self.consented_uri = Some(uri.to_string());
        Ok(())
    }

    /// 播放器上报媒体时长（用于区分点播与直播；F-21 的 `Seek` 策略据此判断）。
    pub fn set_media_duration(&mut self, duration_secs: Option<u64>) {
        if let Some(media) = self.media.as_mut() {
            media.duration_secs = duration_secs;
        }
    }

    /// `Play`（F-17：速度只认 `1`）。
    pub fn play(&mut self, speed: &str) -> Result<(), DmrFault> {
        if speed != SUPPORTED_PLAY_SPEED {
            return Err(DmrFault::new(
                ERR_PLAY_SPEED_NOT_SUPPORTED,
                format!("只支持 Speed={SUPPORTED_PLAY_SPEED}（F-17），收到 {speed:?}"),
            ));
        }
        match self.state {
            TransportState::Stopped | TransportState::PausedPlayback => {}
            TransportState::Playing => return Ok(()),
            TransportState::NoMediaPresent => {
                return Err(DmrFault::new(
                    ERR_TRANSITION_NOT_AVAILABLE,
                    "没有媒体：先 SetAVTransportURI（F-16 的 701）",
                ))
            }
            TransportState::Transitioning => {
                return Err(DmrFault::new(
                    ERR_TRANSITION_NOT_AVAILABLE,
                    "状态迁移中，稍后重试（F-16 的 701）",
                ))
            }
        }
        let uri = self
            .media
            .as_ref()
            .map(|media| media.uri.clone())
            .ok_or_else(|| DmrFault::new(ERR_TRANSITION_NOT_AVAILABLE, "没有媒体"))?;
        if self.consented_uri.as_deref() != Some(uri.as_str()) {
            return Err(DmrFault::new(
                ERR_TRANSITION_NOT_AVAILABLE,
                "该 URI 尚未获得用户同意：播放前需要授权（T42 实现决策）",
            ));
        }
        self.state = TransportState::Playing;
        Ok(())
    }

    /// `Pause`（F-15：目标状态 `PAUSED_PLAYBACK`）。
    pub fn pause(&mut self) -> Result<(), DmrFault> {
        match self.state {
            TransportState::Playing => {
                self.state = TransportState::PausedPlayback;
                Ok(())
            }
            TransportState::PausedPlayback => Ok(()),
            _ => Err(DmrFault::new(
                ERR_TRANSITION_NOT_AVAILABLE,
                format!("当前状态 {} 不能暂停（F-16 的 701）", self.state.as_str()),
            )),
        }
    }

    /// `Stop`（T42-02：幂等——已经是停止态也返回成功）。
    pub fn stop(&mut self) -> Result<(), DmrFault> {
        match self.state {
            TransportState::Stopped => Ok(()),
            TransportState::NoMediaPresent => Ok(()),
            _ => {
                self.state = TransportState::Stopped;
                self.position_secs = 0;
                Ok(())
            }
        }
    }

    /// `Seek`（T42-03：live 流与未实现单位 → 710；目标非法 → 711）。
    pub fn seek(&mut self, unit: &str, target: &str) -> Result<(), DmrFault> {
        if unit != SUPPORTED_SEEK_MODE {
            return Err(DmrFault::new(
                ERR_SEEK_MODE_NOT_SUPPORTED,
                format!("只实现 Unit={SUPPORTED_SEEK_MODE}（F-17 的六种允许值里的这一种），收到 {unit:?}"),
            ));
        }
        let media = self.media.as_ref().ok_or_else(|| {
            DmrFault::new(ERR_TRANSITION_NOT_AVAILABLE, "没有媒体：无法定位（F-16 的 701）")
        })?;
        if media.is_live() {
            // T42-03：直播流没有可定位的时间轴 → 明确不支持，不静默忽略。
            return Err(DmrFault::new(
                ERR_SEEK_MODE_NOT_SUPPORTED,
                "直播流不支持定位（时长未知）：明确不支持而不是忽略（T42-03）",
            ));
        }
        if !matches!(self.state, TransportState::Playing | TransportState::PausedPlayback) {
            return Err(DmrFault::new(
                ERR_TRANSITION_NOT_AVAILABLE,
                format!("当前状态 {} 不能定位（F-16 的 701）", self.state.as_str()),
            ));
        }
        let (hours, minutes, seconds) = parse_rel_time(target)
            .ok_or_else(|| DmrFault::new(ERR_ILLEGAL_SEEK_TARGET, format!("目标不是 H:MM:SS：{target:?}")))?;
        let target_secs = hours * 3600 + minutes * 60 + seconds;
        if let Some(duration) = media.duration_secs {
            if target_secs > duration {
                return Err(DmrFault::new(
                    ERR_ILLEGAL_SEEK_TARGET,
                    format!("目标 {target_secs}s 超过时长 {duration}s（F-16 的 711）"),
                ));
            }
        }
        self.position_secs = target_secs;
        Ok(())
    }

    /// 播放器反馈进度（native player → controller）。
    pub fn on_playback_position(&mut self, secs: u64) {
        self.position_secs = secs;
    }

    /// 播放器反馈播放结束 → 回到 `STOPPED`（F-15）。
    pub fn on_playback_ended(&mut self) {
        if self.media.is_some() {
            self.state = TransportState::Stopped;
            self.position_secs = 0;
        }
    }

    // ---------------------------------------------------------------- RenderingControl

    pub fn get_volume(&self) -> u8 {
        self.volume
    }

    /// `SetVolume`（F-18：`Volume` 为 ui2、范围 0..100）。
    pub fn set_volume(&mut self, volume: i64) -> Result<(), DmrFault> {
        if !(0..=100).contains(&volume) {
            return Err(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                format!("DesiredVolume 必须在 0..100（F-18），收到 {volume}"),
            ));
        }
        self.volume = volume as u8;
        Ok(())
    }

    pub fn get_mute(&self) -> bool {
        self.muted
    }

    pub fn set_mute(&mut self, muted: bool) -> Result<(), DmrFault> {
        self.muted = muted;
        Ok(())
    }

    // ---------------------------------------------------------------- GENA（只做校验与拒绝）

    /// `SUBSCRIBE`（T42-04：回调越界 → 拒绝；F-22：只建立订阅记录，不投递事件）。
    pub fn subscribe(
        &mut self,
        callback: &str,
        timeout_secs: u32,
        renewal_sid: Option<&str>,
    ) -> Result<Subscription, DmrFault> {
        if callback.is_empty() {
            return Err(DmrFault::new(ERR_INVALID_ARGUMENT, "CALLBACK 不得为空"));
        }
        // T42-04：回调必须是单个 http(s) 地址——多播/多地址/本地路径一律拒绝。
        if callback.contains('<') || callback.contains('>') {
            return Err(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                "CALLBACK 只接受单个地址（尖括号多地址形式一律拒绝，T42-04）",
            ));
        }
        if let Err(error) = self.policy.check(callback) {
            return Err(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                format!("回调地址被策略拒绝（T42-04）：{}", error.message),
            ));
        }
        if timeout_secs == 0 || timeout_secs > MAX_SUBSCRIPTION_TIMEOUT_SECS {
            return Err(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                format!("TIMEOUT 必须在 1..{MAX_SUBSCRIPTION_TIMEOUT_SECS} 秒（本仓策略）"),
            ));
        }
        if let Some(sid) = renewal_sid {
            let existing = self
                .subscriptions
                .iter_mut()
                .find(|subscription| subscription.sid == sid)
                .ok_or_else(|| {
                    DmrFault::new(ERR_INVALID_ARGUMENT, format!("未知 SID {sid:?}：续订不建立新订阅"))
                })?;
            existing.timeout_secs = timeout_secs;
            return Ok(existing.clone());
        }
        if self.subscriptions.len() >= self.max_subscriptions {
            return Err(DmrFault::new(
                ERR_CANT_PROCESS,
                format!(
                    "订阅数达到上限 {}（本仓策略；F-10 只说明服务端有上限）",
                    self.max_subscriptions
                ),
            ));
        }
        let sid = format!("uuid:xross-gena-{:08x}", self.next_sid);
        self.next_sid += 1;
        let subscription = Subscription {
            sid,
            callback: callback.to_string(),
            timeout_secs,
            seq: 0,
        };
        self.subscriptions.push(subscription.clone());
        Ok(subscription)
    }

    /// `UNSUBSCRIBE`（F-10）。
    pub fn unsubscribe(&mut self, sid: &str) -> Result<(), DmrFault> {
        let before = self.subscriptions.len();
        self.subscriptions.retain(|subscription| subscription.sid != sid);
        if self.subscriptions.len() == before {
            return Err(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                format!("未知 SID {sid:?}（F-10：退订必须带已建立的 SID）"),
            ));
        }
        Ok(())
    }

    // ---------------------------------------------------------------- SOAP 入口

    /// 处理一条解析好的 SOAP 动作消息（未知动作在解析期就被拒；这里只做分发）。
    pub fn dispatch(&mut self, message: &SoapMessage) -> DmrReply {
        let Some(action) = message.action else {
            return DmrReply::fault(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                "不是动作消息（Fault 或空消息）",
            ));
        };
        let arg = |name: &str| message.arg(name).unwrap_or("").to_string();
        match action {
            SoapAction::SetAvTransportUri => {
                match self.set_uri(&arg("CurrentURI"), message.arg("CurrentURIMetaData")) {
                    Ok(_) => DmrReply::ok(&[]),
                    Err(fault) => DmrReply::fault(fault),
                }
            }
            SoapAction::Play => match self.play(&arg("Speed")) {
                Ok(()) => DmrReply::ok(&[]),
                Err(fault) => DmrReply::fault(fault),
            },
            SoapAction::Pause => match self.pause() {
                Ok(()) => DmrReply::ok(&[]),
                Err(fault) => DmrReply::fault(fault),
            },
            SoapAction::Stop => match self.stop() {
                Ok(()) => DmrReply::ok(&[]),
                Err(fault) => DmrReply::fault(fault),
            },
            SoapAction::Seek => match self.seek(&arg("Unit"), &arg("Target")) {
                Ok(()) => DmrReply::ok(&[]),
                Err(fault) => DmrReply::fault(fault),
            },
            SoapAction::GetTransportInfo => DmrReply::ok(&[
                ("CurrentTransportState", self.state.as_str().to_string()),
                ("CurrentTransportStatus", self.status.as_str().to_string()),
                (
                    "CurrentSpeed",
                    if self.state == TransportState::Playing {
                        SUPPORTED_PLAY_SPEED.to_string()
                    } else {
                        "0".to_string()
                    },
                ),
            ]),
            SoapAction::GetMediaInfo => DmrReply::ok(&[
                (
                    "CurrentURI",
                    self.current_uri().unwrap_or("").to_string(),
                ),
                (
                    "CurrentMediaDuration",
                    match self.media.as_ref().and_then(|media| media.duration_secs) {
                        Some(duration) => format_rel_time(duration),
                        None => "00:00:00".to_string(),
                    },
                ),
                (
                    "NumberOfTracks",
                    if self.media.is_some() { "1" } else { "0" }.to_string(),
                ),
            ]),
            SoapAction::GetPositionInfo => {
                let position = self.position_secs;
                let duration = self.media.as_ref().and_then(|media| media.duration_secs);
                DmrReply::ok(&[
                    ("Track", if self.media.is_some() { "1" } else { "0" }.to_string()),
                    ("RelTime", format_rel_time(position)),
                    (
                        "AbsTime",
                        format_rel_time(duration.unwrap_or(position)),
                    ),
                    (
                        "TrackDuration",
                        duration.map(format_rel_time).unwrap_or_else(|| "00:00:00".to_string()),
                    ),
                ])
            }
            SoapAction::GetVolume => {
                DmrReply::ok(&[("CurrentVolume", self.volume.to_string())])
            }
            SoapAction::SetVolume => {
                let desired = arg("DesiredVolume");
                match desired.parse::<i64>() {
                    Ok(volume) => match self.set_volume(volume) {
                        Ok(()) => DmrReply::ok(&[("CurrentVolume", self.volume.to_string())]),
                        Err(fault) => DmrReply::fault(fault),
                    },
                    Err(_) => DmrReply::fault(DmrFault::new(
                        ERR_INVALID_ARGUMENT,
                        format!("DesiredVolume 不是整数：{desired:?}"),
                    )),
                }
            }
            SoapAction::GetMute => DmrReply::ok(&[(
                "CurrentMute",
                if self.muted { "1" } else { "0" }.to_string(),
            )]),
            SoapAction::SetMute => {
                let desired = arg("DesiredMute");
                let muted = match desired.as_str() {
                    "1" | "true" | "True" => true,
                    "0" | "false" | "False" => false,
                    other => {
                        return DmrReply::fault(DmrFault::new(
                            ERR_INVALID_ARGUMENT,
                            format!("DesiredMute 只认 0/1：{other:?}"),
                        ))
                    }
                };
                match self.set_mute(muted) {
                    Ok(()) => DmrReply::ok(&[(
                        "CurrentMute",
                        if self.muted { "1" } else { "0" }.to_string(),
                    )]),
                    Err(fault) => DmrReply::fault(fault),
                }
            }
            SoapAction::Browse => DmrReply::fault(DmrFault::new(
                ERR_INVALID_ARGUMENT,
                "renderer 不提供 ContentDirectory（Browse 属 DMS 侧）",
            )),
        }
    }

    /// 本渲染器声明的服务类型（供设备描述使用）。
    pub fn declared_services(&self) -> [&'static str; 2] {
        [AV_TRANSPORT, RENDERING_CONTROL]
    }

    /// `Channel` 只认 `Master`（F-18 + 本仓策略）——供演示与测试引用。
    pub fn channel_master(&self) -> &'static str {
        CHANNEL_MASTER
    }
}

/// `H:MM:SS` 解析（`REL_TIME` 的目标格式；来源用法见 F-17）。
fn parse_rel_time(raw: &str) -> Option<(u64, u64, u64)> {
    let mut parts = raw.split(':');
    let hours = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let seconds = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() || minutes > 59 || seconds > 59 {
        return None;
    }
    Some((hours, minutes, seconds))
}

fn format_rel_time(total_secs: u64) -> String {
    format!(
        "{}:{:02}:{:02}",
        total_secs / 3600,
        (total_secs % 3600) / 60,
        total_secs % 60
    )
}
