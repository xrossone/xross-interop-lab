//! 命名空间与载荷（字段表 F-07..F-15）：connection / heartbeat / receiver / media 四组消息的编解码。
//!
//! JSON 解析用通用库（serde_json）——它不是 Cast 协议栈，只是文本格式解析；协议语义（哪些类型、
//! 哪些键、哪些取值合法）全部由本文件按字段表逐条校验：未知类型 `unsupported-feature`、
//! 已知类型但取值非法 `invalid-frame`，不做"尽力解析"。
//!
//! 字段表里的 `deviceauth` 命名空间**故意不在这里定义**（F-18：认证握手不实现）。
//! 本模块只登记可用于媒体控制与连接的四个命名空间。

use crate::castv2::{CastMessage, Payload};
use interop_contract::error::{Error, ErrorCode};
use serde_json::{json, Map, Value};

/// 命名空间字面量（F-07）。
pub const NS_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
pub const NS_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
pub const NS_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
pub const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";

/// 已登记的命名空间（其余值一律拒绝）。
pub const KNOWN_NAMESPACES: [&str; 4] = [NS_CONNECTION, NS_HEARTBEAT, NS_RECEIVER, NS_MEDIA];

/// 载荷的类型键（F-08：`type`）。
pub const TYPE_KEY: &str = "type";
/// 请求关联键（F-16）。
pub const REQUEST_ID_KEY: &str = "requestId";

// connection（F-08）
pub const TYPE_CONNECT: &str = "CONNECT";
pub const TYPE_CLOSE: &str = "CLOSE";
pub const TYPE_CONNECTED: &str = "CONNECTED";

// heartbeat（F-09）
pub const TYPE_PING: &str = "PING";
pub const TYPE_PONG: &str = "PONG";

// receiver（F-10）
pub const TYPE_LAUNCH: &str = "LAUNCH";
pub const TYPE_GET_STATUS: &str = "GET_STATUS";
pub const TYPE_STOP: &str = "STOP";
pub const TYPE_SET_VOLUME: &str = "SET_VOLUME";
pub const TYPE_RECEIVER_STATUS: &str = "RECEIVER_STATUS";
pub const TYPE_LAUNCH_ERROR: &str = "LAUNCH_ERROR";

// media（F-11）
pub const TYPE_LOAD: &str = "LOAD";
pub const TYPE_PLAY: &str = "PLAY";
pub const TYPE_PAUSE: &str = "PAUSE";
pub const TYPE_SEEK: &str = "SEEK";
pub const TYPE_MEDIA_STATUS: &str = "MEDIA_STATUS";
pub const TYPE_LOAD_FAILED: &str = "LOAD_FAILED";

/// SEEK 只认这一个 resumeState 取值（F-13：`PLAYBACK_START`）。
pub const RESUME_STATE_PLAYBACK_START: &str = "PLAYBACK_START";

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn unsupported(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, msg)
}

/// 播放器状态（F-15）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    Playing,
    Buffering,
    Paused,
    Idle,
    Unknown,
}

impl PlayerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Playing => "PLAYING",
            Self::Buffering => "BUFFERING",
            Self::Paused => "PAUSED",
            Self::Idle => "IDLE",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, Error> {
        match raw {
            "PLAYING" => Ok(Self::Playing),
            "BUFFERING" => Ok(Self::Buffering),
            "PAUSED" => Ok(Self::Paused),
            "IDLE" => Ok(Self::Idle),
            "UNKNOWN" => Ok(Self::Unknown),
            other => Err(invalid(format!("未知 playerState {other:?}（F-15）"))),
        }
    }
}

/// 媒体流类型（F-15）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    Unknown,
    Buffered,
    Live,
}

impl StreamType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::Buffered => "BUFFERED",
            Self::Live => "LIVE",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, Error> {
        match raw {
            "UNKNOWN" => Ok(Self::Unknown),
            "BUFFERED" => Ok(Self::Buffered),
            "LIVE" => Ok(Self::Live),
            other => Err(invalid(format!("未知 streamType {other:?}（F-15）"))),
        }
    }
}

/// 音量（F-10/F-14）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Volume {
    pub level: Option<f64>,
    pub muted: Option<bool>,
}

/// 通用媒体元数据（F-12 的 `metadata` 键）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GenericMediaMetadata {
    pub metadata_type: i64,
    pub title: Option<String>,
    pub subtitle: Option<String>,
}

/// 媒体信息（F-12 的 `media` 对象 / F-14 的状态回包）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaInformation {
    pub content_id: String,
    pub content_type: String,
    pub stream_type: StreamType,
    pub duration: Option<f64>,
    pub metadata: Option<GenericMediaMetadata>,
}

/// LOAD 请求（F-12）。
#[derive(Debug, Clone, PartialEq)]
pub struct LoadRequest {
    pub content_id: String,
    pub content_type: String,
    pub stream_type: StreamType,
    pub title: Option<String>,
    pub autoplay: bool,
    pub current_time: Option<f64>,
}

/// MEDIA_STATUS（F-14）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaStatus {
    pub media_session_id: Option<i64>,
    pub player_state: PlayerState,
    pub idle_reason: Option<String>,
    pub current_time: Option<f64>,
    pub media: Option<MediaInformation>,
    pub volume: Option<Volume>,
}

/// RECEIVER_STATUS 里的应用条目（F-10）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplicationStatus {
    pub app_id: String,
    pub display_name: Option<String>,
    pub session_id: Option<String>,
    pub transport_id: Option<String>,
    pub namespaces: Vec<String>,
    pub status_text: Option<String>,
}

/// RECEIVER_STATUS（F-10）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReceiverStatus {
    pub applications: Vec<ApplicationStatus>,
    pub volume: Option<Volume>,
    pub is_stand_by: Option<bool>,
}

impl ReceiverStatus {
    /// 返回第一个应用的 `transportId`（launch 之后媒体命令发往它）。没有应用则为 `None`。
    pub fn transport_id(&self) -> Option<&str> {
        self.applications
            .iter()
            .find_map(|app| app.transport_id.as_deref())
    }

    pub fn session_id(&self) -> Option<&str> {
        self.applications.iter().find_map(|app| app.session_id.as_deref())
    }

    pub fn app_id(&self) -> Option<&str> {
        self.applications.first().map(|app| app.app_id.as_str())
    }
}

/// 已登记的载荷。
#[derive(Debug, Clone, PartialEq)]
pub enum CastPayload {
    /// `CONNECT`（F-08）。
    Connect {
        user_agent: String,
        sender_info: SenderInfo,
    },
    /// `CONNECTED`（F-08：连接命名空间的应答）。
    Connected,
    /// `CLOSE`（F-08）。
    Close,
    Ping,
    Pong,
    Launch {
        app_id: String,
    },
    ReceiverGetStatus,
    ReceiverStop,
    SetVolume {
        volume: Volume,
    },
    ReceiverStatus(ReceiverStatus),
    LaunchError {
        reason: String,
        app_id: Option<String>,
    },
    Load(LoadRequest),
    Play,
    Pause,
    MediaStop,
    Seek {
        current_time: f64,
        resume_state: &'static str,
    },
    MediaGetStatus,
    MediaStatus(Box<MediaStatus>),
    LoadFailed {
        reason: String,
    },
}

/// CONNECT 里的发送端信息（F-08 的 `senderInfo` 键）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderInfo {
    pub sdk_type: i64,
    pub version: String,
    pub browser_version: String,
    pub platform: i64,
    pub connection_type: i64,
}

impl SenderInfo {
    fn to_json(&self) -> Value {
        json!({
            "sdkType": self.sdk_type,
            "version": self.version,
            "browserVersion": self.browser_version,
            "platform": self.platform,
            "connectionType": self.connection_type,
        })
    }

    fn from_json(value: &Value) -> Result<Self, Error> {
        let obj = value
            .as_object()
            .ok_or_else(|| invalid("senderInfo 必须是对象（F-08）"))?;
        Ok(Self {
            sdk_type: obj.get("sdkType").and_then(Value::as_i64).unwrap_or(0),
            version: obj
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            browser_version: obj
                .get("browserVersion")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            platform: obj.get("platform").and_then(Value::as_i64).unwrap_or(0),
            connection_type: obj
                .get("connectionType")
                .and_then(Value::as_i64)
                .unwrap_or(0),
        })
    }
}

impl CastPayload {
    pub fn namespace(&self) -> &'static str {
        match self {
            Self::Connect { .. } | Self::Connected | Self::Close => NS_CONNECTION,
            Self::Ping | Self::Pong => NS_HEARTBEAT,
            Self::Launch { .. }
            | Self::ReceiverGetStatus
            | Self::ReceiverStop
            | Self::SetVolume { .. }
            | Self::ReceiverStatus(_)
            | Self::LaunchError { .. } => NS_RECEIVER,
            Self::Load(_)
            | Self::Play
            | Self::Pause
            | Self::MediaStop
            | Self::Seek { .. }
            | Self::MediaGetStatus
            | Self::MediaStatus(_)
            | Self::LoadFailed { .. } => NS_MEDIA,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Connect { .. } => TYPE_CONNECT,
            Self::Connected => TYPE_CONNECTED,
            Self::Close => TYPE_CLOSE,
            Self::Ping => TYPE_PING,
            Self::Pong => TYPE_PONG,
            Self::Launch { .. } => TYPE_LAUNCH,
            Self::ReceiverGetStatus => TYPE_GET_STATUS,
            Self::ReceiverStop => TYPE_STOP,
            Self::SetVolume { .. } => TYPE_SET_VOLUME,
            Self::ReceiverStatus(_) => TYPE_RECEIVER_STATUS,
            Self::LaunchError { .. } => TYPE_LAUNCH_ERROR,
            Self::Load(_) => TYPE_LOAD,
            Self::Play => TYPE_PLAY,
            Self::Pause => TYPE_PAUSE,
            Self::MediaStop => TYPE_STOP,
            Self::Seek { .. } => TYPE_SEEK,
            Self::MediaGetStatus => TYPE_GET_STATUS,
            Self::MediaStatus(_) => TYPE_MEDIA_STATUS,
            Self::LoadFailed { .. } => TYPE_LOAD_FAILED,
        }
    }

    /// 编码为 JSON 文本；`request_id` 由调用方给（F-16：逐条单调注入）。
    pub fn encode(&self, request_id: Option<u64>) -> Result<String, Error> {
        let mut map = Map::new();
        map.insert(TYPE_KEY.to_string(), json!(self.type_name()));
        match self {
            Self::Connect {
                user_agent,
                sender_info,
            } => {
                // F-08：origin 是对象；connType / userAgent / senderInfo 逐键就位。
                map.insert("origin".into(), json!({}));
                map.insert("connType".into(), json!(0));
                map.insert("userAgent".into(), json!(user_agent));
                map.insert("senderInfo".into(), sender_info.to_json());
            }
            Self::Connected | Self::Close => {
                map.insert("origin".into(), json!({}));
            }
            Self::Ping | Self::Pong => {}
            Self::Launch { app_id } => {
                if app_id.is_empty() {
                    return Err(invalid("LAUNCH 的 appId 不得为空（F-10）"));
                }
                map.insert("appId".into(), json!(app_id));
            }
            Self::ReceiverGetStatus | Self::MediaGetStatus | Self::Play | Self::Pause
            | Self::MediaStop | Self::ReceiverStop => {}
            Self::SetVolume { volume } => {
                map.insert("volume".into(), volume_json(volume));
            }
            Self::ReceiverStatus(status) => {
                map.insert("status".into(), receiver_status_json(status));
            }
            Self::LaunchError { reason, app_id } => {
                map.insert("reason".into(), json!(reason));
                if let Some(app_id) = app_id {
                    map.insert("appId".into(), json!(app_id));
                }
            }
            Self::Load(load) => {
                map.insert("media".into(), load_media_json(load));
                map.insert("autoplay".into(), json!(load.autoplay));
                if let Some(current_time) = load.current_time {
                    map.insert("currentTime".into(), json!(current_time));
                }
                // F-12：customData 固定空对象；duration 不在 LOAD 里写。
                map.insert("customData".into(), json!({}));
            }
            Self::Seek {
                current_time,
                resume_state,
            } => {
                map.insert("currentTime".into(), json!(current_time));
                map.insert("resumeState".into(), json!(resume_state));
            }
            Self::MediaStatus(status) => {
                map = media_status_map(status)?;
            }
            Self::LoadFailed { reason } => {
                map.insert("reason".into(), json!(reason));
            }
        }
        if let Some(request_id) = request_id {
            map.insert(REQUEST_ID_KEY.into(), json!(request_id));
        }
        serde_json::to_string(&Value::Object(map))
            .map_err(|e| invalid(format!("载荷不可序列化：{e}")))
    }

    /// 从命名空间 + JSON 文本解码（严格：未知命名空间/类型一律拒绝）。
    pub fn decode(namespace: &str, text: &str) -> Result<(Self, Option<u64>), Error> {
        if !KNOWN_NAMESPACES.contains(&namespace) {
            return Err(unsupported(format!(
                "未登记的 Cast 命名空间 {namespace:?}（F-07 只登记四个）"
            )));
        }
        let value: Value =
            serde_json::from_str(text).map_err(|e| invalid(format!("载荷不是合法 JSON：{e}")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| invalid("载荷必须是 JSON 对象（F-08）"))?;
        let type_name = obj
            .get(TYPE_KEY)
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(format!("载荷缺 {TYPE_KEY} 键（F-08）")))?;
        let request_id = obj.get(REQUEST_ID_KEY).and_then(Value::as_u64);
        let payload = match (namespace, type_name) {
            (NS_CONNECTION, TYPE_CONNECT) => Self::Connect {
                user_agent: obj
                    .get("userAgent")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                sender_info: obj
                    .get("senderInfo")
                    .map(SenderInfo::from_json)
                    .transpose()?
                    .unwrap_or(SenderInfo {
                        sdk_type: 0,
                        version: String::new(),
                        browser_version: String::new(),
                        platform: 0,
                        connection_type: 0,
                    }),
            },
            (NS_CONNECTION, TYPE_CONNECTED) => Self::Connected,
            (NS_CONNECTION, TYPE_CLOSE) => Self::Close,
            (NS_HEARTBEAT, TYPE_PING) => Self::Ping,
            (NS_HEARTBEAT, TYPE_PONG) => Self::Pong,
            (NS_RECEIVER, TYPE_LAUNCH) => Self::Launch {
                app_id: obj
                    .get("appId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("LAUNCH 缺 appId（F-10）"))?
                    .to_string(),
            },
            (NS_RECEIVER, TYPE_GET_STATUS) => Self::ReceiverGetStatus,
            (NS_RECEIVER, TYPE_STOP) => Self::ReceiverStop,
            (NS_RECEIVER, TYPE_SET_VOLUME) => Self::SetVolume {
                volume: obj
                    .get("volume")
                    .map(volume_from_json)
                    .transpose()?
                    .unwrap_or_default(),
            },
            (NS_RECEIVER, TYPE_RECEIVER_STATUS) => Self::ReceiverStatus(
                obj.get("status")
                    .map(receiver_status_from_json)
                    .transpose()?
                    .unwrap_or_default(),
            ),
            (NS_RECEIVER, TYPE_LAUNCH_ERROR) => Self::LaunchError {
                reason: obj
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("LAUNCH_ERROR 缺 reason（F-10）"))?
                    .to_string(),
                app_id: obj.get("appId").and_then(Value::as_str).map(str::to_string),
            },
            (NS_MEDIA, TYPE_LOAD) => Self::Load(load_from_json(obj)?),
            (NS_MEDIA, TYPE_PLAY) => Self::Play,
            (NS_MEDIA, TYPE_PAUSE) => Self::Pause,
            (NS_MEDIA, TYPE_STOP) => Self::MediaStop,
            (NS_MEDIA, TYPE_SEEK) => {
                let resume_state = obj
                    .get("resumeState")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("SEEK 缺 resumeState（F-13）"))?;
                if resume_state != RESUME_STATE_PLAYBACK_START {
                    return Err(invalid(format!(
                        "SEEK 只认 resumeState = {RESUME_STATE_PLAYBACK_START}（F-13），收到 {resume_state:?}"
                    )));
                }
                Self::Seek {
                    current_time: obj
                        .get("currentTime")
                        .and_then(Value::as_f64)
                        .ok_or_else(|| invalid("SEEK 缺 currentTime（F-13）"))?,
                    resume_state: RESUME_STATE_PLAYBACK_START,
                }
            }
            (NS_MEDIA, TYPE_GET_STATUS) => Self::MediaGetStatus,
            (NS_MEDIA, TYPE_MEDIA_STATUS) => Self::MediaStatus(Box::new(media_status_from_json(obj)?)),
            (NS_MEDIA, TYPE_LOAD_FAILED) => Self::LoadFailed {
                reason: obj
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
            _ => {
                return Err(unsupported(format!(
                    "命名空间 {namespace} 下的未登记类型 {type_name:?}（字段表 F-08..F-15 之外一律不认）"
                )))
            }
        };
        Ok((payload, request_id))
    }

    /// 包装成一条发出用的 `CastMessage`。
    pub fn into_message(
        self,
        source_id: &str,
        destination_id: &str,
        request_id: Option<u64>,
    ) -> Result<CastMessage, Error> {
        let text = self.encode(request_id)?;
        Ok(CastMessage {
            protocol_version: 0,
            source_id: source_id.to_string(),
            destination_id: destination_id.to_string(),
            namespace: self.namespace().to_string(),
            payload: Payload::Utf8(text),
        })
    }
}

fn volume_json(volume: &Volume) -> Value {
    let mut map = Map::new();
    if let Some(level) = volume.level {
        map.insert("level".into(), json!(level));
    }
    if let Some(muted) = volume.muted {
        map.insert("muted".into(), json!(muted));
    }
    Value::Object(map)
}

fn volume_from_json(value: &Value) -> Result<Volume, Error> {
    if value.is_null() {
        return Ok(Volume::default());
    }
    let obj = value
        .as_object()
        .ok_or_else(|| invalid("volume 必须是对象（F-10/F-14）"))?;
    let level = match obj.get("level") {
        Some(Value::Null) | None => None,
        Some(other) => Some(
            other
                .as_f64()
                .ok_or_else(|| invalid("volume.level 必须是数字"))?,
        ),
    };
    let muted = match obj.get("muted") {
        Some(Value::Null) | None => None,
        Some(other) => Some(
            other
                .as_bool()
                .ok_or_else(|| invalid("volume.muted 必须是布尔"))?,
        ),
    };
    Ok(Volume { level, muted })
}

fn load_media_json(load: &LoadRequest) -> Value {
    let mut media = Map::new();
    media.insert("contentId".into(), json!(load.content_id));
    media.insert("contentType".into(), json!(load.content_type));
    media.insert("streamType".into(), json!(load.stream_type.as_str()));
    let mut metadata = Map::new();
    metadata.insert("metadataType".into(), json!(0));
    if let Some(title) = &load.title {
        metadata.insert("title".into(), json!(title));
    }
    media.insert("metadata".into(), Value::Object(metadata));
    Value::Object(media)
}

fn load_from_json(obj: &Map<String, Value>) -> Result<LoadRequest, Error> {
    let media = obj
        .get("media")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("LOAD 缺 media 对象（F-12）"))?;
    let content_id = media
        .get("contentId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("LOAD 的 media 缺 contentId（F-12）"))?;
    if content_id.is_empty() {
        return Err(invalid("LOAD 的 contentId 不得为空（F-12）"));
    }
    let content_type = media
        .get("contentType")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("LOAD 的 media 缺 contentType（F-12）"))?;
    if content_type.is_empty() {
        return Err(invalid("LOAD 的 contentType 不得为空（F-12）"));
    }
    let stream_type = StreamType::parse(
        media
            .get("streamType")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("LOAD 的 media 缺 streamType（F-12）"))?,
    )?;
    let autoplay = match obj.get("autoplay") {
        Some(Value::Null) | None => false,
        Some(other) => other
            .as_bool()
            .ok_or_else(|| invalid("autoplay 必须是布尔"))?,
    };
    let current_time = match obj.get("currentTime") {
        Some(Value::Null) | None => None,
        Some(other) => Some(
            other
                .as_f64()
                .ok_or_else(|| invalid("currentTime 必须是数字"))?,
        ),
    };
    if let Some(custom) = obj.get("customData") {
        if !custom.is_object() {
            return Err(invalid("customData 必须是对象（F-12）"));
        }
    }
    Ok(LoadRequest {
        content_id: content_id.to_string(),
        content_type: content_type.to_string(),
        stream_type,
        title: media
            .get("metadata")
            .and_then(Value::as_object)
            .and_then(|m| m.get("title"))
            .and_then(Value::as_str)
            .map(str::to_string),
        autoplay,
        current_time,
    })
}

fn media_status_map(status: &MediaStatus) -> Result<Map<String, Value>, Error> {
    let mut map = Map::new();
    map.insert(TYPE_KEY.into(), json!(TYPE_MEDIA_STATUS));
    map.insert("playerState".into(), json!(status.player_state.as_str()));
    if let Some(id) = status.media_session_id {
        map.insert("mediaSessionId".into(), json!(id));
    }
    if let Some(reason) = &status.idle_reason {
        map.insert("idleReason".into(), json!(reason));
    }
    if let Some(current_time) = status.current_time {
        map.insert("currentTime".into(), json!(current_time));
    }
    if let Some(media) = &status.media {
        map.insert(
            "media".into(),
            json!({
                "contentId": media.content_id,
                "contentType": media.content_type,
                "streamType": media.stream_type.as_str(),
                "duration": media.duration,
            }),
        );
    }
    if let Some(volume) = &status.volume {
        map.insert("volume".into(), volume_json(volume));
    }
    Ok(map)
}

fn media_status_from_json(obj: &Map<String, Value>) -> Result<MediaStatus, Error> {
    let player_state = PlayerState::parse(
        obj.get("playerState")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("MEDIA_STATUS 缺 playerState（F-14）"))?,
    )?;
    let media = match obj.get("media") {
        Some(Value::Null) | None => None,
        Some(Value::Object(media)) => Some(MediaInformation {
            content_id: media
                .get("contentId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            content_type: media
                .get("contentType")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            stream_type: StreamType::parse(
                media
                    .get("streamType")
                    .and_then(Value::as_str)
                    .unwrap_or("UNKNOWN"),
            )?,
            duration: media.get("duration").and_then(Value::as_f64),
            metadata: None,
        }),
        Some(_) => return Err(invalid("MEDIA_STATUS 的 media 必须是对象（F-14）")),
    };
    Ok(MediaStatus {
        media_session_id: obj.get("mediaSessionId").and_then(Value::as_i64),
        player_state,
        idle_reason: obj
            .get("idleReason")
            .and_then(Value::as_str)
            .map(str::to_string),
        current_time: obj.get("currentTime").and_then(Value::as_f64),
        media,
        volume: obj.get("volume").map(volume_from_json).transpose()?,
    })
}

fn receiver_status_json(status: &ReceiverStatus) -> Value {
    let applications: Vec<Value> = status
        .applications
        .iter()
        .map(|app| {
            json!({
                "appId": app.app_id,
                "displayName": app.display_name,
                "sessionId": app.session_id,
                "transportId": app.transport_id,
                "namespaces": app.namespaces.iter().map(|n| json!({ "name": n })).collect::<Vec<_>>(),
                "statusText": app.status_text,
            })
        })
        .collect();
    json!({
        "applications": applications,
        "volume": status.volume.as_ref().map(volume_json),
        "isStandBy": status.is_stand_by,
    })
}

fn receiver_status_from_json(value: &Value) -> Result<ReceiverStatus, Error> {
    let obj = value
        .as_object()
        .ok_or_else(|| invalid("RECEIVER_STATUS 的 status 必须是对象（F-10）"))?;
    let mut applications = Vec::new();
    if let Some(list) = obj.get("applications") {
        let list = list
            .as_array()
            .ok_or_else(|| invalid("RECEIVER_STATUS 的 applications 必须是数组（F-10）"))?;
        for app in list {
            let app = app
                .as_object()
                .ok_or_else(|| invalid("applications 条目必须是对象（F-10）"))?;
            let app_id = app
                .get("appId")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("applications 条目缺 appId（F-10）"))?;
            let mut namespaces = Vec::new();
            if let Some(list) = app.get("namespaces").and_then(Value::as_array) {
                for entry in list {
                    if let Some(name) = entry.get("name").and_then(Value::as_str) {
                        namespaces.push(name.to_string());
                    }
                }
            }
            applications.push(ApplicationStatus {
                app_id: app_id.to_string(),
                display_name: app
                    .get("displayName")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                session_id: app.get("sessionId").and_then(Value::as_str).map(str::to_string),
                transport_id: app
                    .get("transportId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                namespaces,
                status_text: app
                    .get("statusText")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
    }
    Ok(ReceiverStatus {
        applications,
        volume: obj.get("volume").map(volume_from_json).transpose()?,
        is_stand_by: obj.get("isStandBy").and_then(Value::as_bool),
    })
}

/// 从一条收到的 `CastMessage` 解出载荷（BINARY 载荷在控制面一律拒绝）。
pub fn decode_message(message: &CastMessage) -> Result<(CastPayload, Option<u64>), Error> {
    let text = message
        .payload
        .as_utf8()
        .ok_or_else(|| unsupported("控制面消息必须是 STRING 载荷（F-03）：BINARY 一律拒绝"))?;
    CastPayload::decode(&message.namespace, text)
}
