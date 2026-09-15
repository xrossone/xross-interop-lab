//! 接收侧（sink）会话状态机（字段表 F-02..F-16、F-26）。
//!
//! **只固定消息顺序，不固定状态名**（F-26：四个来源各有一套状态命名，没有共同事实）。
//! 本模块的状态枚举是我们自己的记账，用来执行顺序约束与给出可读状态。
//!
//! 时间由调用方传入（`now_ms`）：本层不拥有时钟，也不做真实等待——
//! keep-alive 的 25 s 与会话超时的 30 s 是 F-27 的**来源取值**（不是规范常量），
//! 在这里只作为比较用的常量。

use crate::ie::DeviceInfoSubelement;
use crate::messages::{
    build_request, build_response, check_cseq, classify, parse_parameter_body,
    parse_parameter_names, Request, Response,
    Role, WfdMessage, METHOD_SETUP, PARAM_AUDIO_CODECS, PARAM_CLIENT_RTP_PORTS,
    PARAM_CONTENT_PROTECTION, PARAM_IDR_REQUEST, PARAM_PRESENTATION_URL, PARAM_VIDEO_FORMATS,
    WFD_METHOD_SET,
};
use crate::negotiate::{
    parse_audio_codecs, parse_video_formats, select_common, select_common_audio, AudioCodecSet,
    ContentProtection, RtpPorts, Transport, VideoFormatSet,
};
use crate::rtp::{parse_header, StreamAccount};
use crate::{CONTROL_URI, STREAM_PATH};
use interop_contract::error::{Error, ErrorCode};

/// keep-alive 间隔（F-27：R33 `wfd-client.c:433` 的取值，**不是规范常量**）。
pub const KEEPALIVE_INTERVAL_MS: u64 = 25_000;
/// 会话超时（F-27：R33 `wfd-client.c:426` 的取值，**不是规范常量**）。
pub const SESSION_TIMEOUT_MS: u64 = 30_000;

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn unsupported(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, msg)
}

/// 本端能**收下并处理**的媒体（不是"能显示"）。
///
/// 默认值是空的：本 crate 没有 TS 解复用、没有解码器、没有渲染（F-28 之外），
/// 因此生产路径只能构造出 `none()`，广告出去就是 `none`——"能力广告由实现清单推导"的纪律
/// 与 T32/T33 一致：广告未实现的能力必须是测试失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedMediaSupport {
    video: VideoFormatSet,
    audio: AudioCodecSet,
}

impl ReceivedMediaSupport {
    /// 生产路径：什么都不支持（本仓没有媒体引擎）。
    pub fn none() -> Self {
        Self {
            video: VideoFormatSet::none(),
            audio: AudioCodecSet::none(),
        }
    }

    /// **仅供测试与演示**：假设本端支持某些格式，用来演练协商逻辑。
    ///
    /// 这个名字是刻意的：它**不**主张本仓具备解码/显示能力，也不得被当作生产广告来源。
    pub fn hypothetical(video: VideoFormatSet, audio: AudioCodecSet) -> Self {
        Self { video, audio }
    }

    pub fn video(&self) -> &VideoFormatSet {
        &self.video
    }

    pub fn audio(&self) -> &AudioCodecSet {
        &self.audio
    }

    pub fn is_empty(&self) -> bool {
        self.video.is_empty() && self.audio.is_empty()
    }

    fn video_wire(&self) -> String {
        self.video.to_wire()
    }

    fn audio_wire(&self) -> String {
        self.audio.to_wire()
    }
}

/// 协商完成后的流参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiatedStream {
    pub video: crate::negotiate::VideoFormatDescriptor,
    pub audio: crate::negotiate::AudioCodecDescriptor,
    pub presentation_url: String,
    /// M4 里对端声明的端口；未声明就是 `None`——不填占位值（port0 = 0 本身是畸形，F-13）。
    pub rtp_ports: Option<RtpPorts>,
    pub transport: Option<Transport>,
    pub content_protection: ContentProtection,
}

/// 会话状态（本仓记账用；F-26 说明状态名不构成规范事实）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkState {
    Idle,
    Options,
    ParamsAnswered,
    ParamsSet,
    SetupTriggered,
    SetupCreated,
    Playing,
    Ended,
}

impl SinkState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Options => "options",
            Self::ParamsAnswered => "params-answered",
            Self::ParamsSet => "params-set",
            Self::SetupTriggered => "setup-triggered",
            Self::SetupCreated => "setup-created",
            Self::Playing => "playing",
            Self::Ended => "ended",
        }
    }
}

/// 会话结束原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndReason {
    Teardown,
    KeepaliveTimeout,
    SetupFailed(String),
}

/// 会话统计（演示与证据用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStats {
    pub state: SinkState,
    pub messages_in: u32,
    pub messages_out: u32,
    pub player_available: bool,
    pub keyframe_requests: u32,
    pub packets: u64,
    pub payload_bytes: u64,
    pub lost: u32,
    pub reordered: u32,
    pub duplicates: u32,
    pub ssrc_changes: u32,
    pub payload_type_anomalies: u32,
    pub keepalive_suppressed: bool,
    pub used_port_fallback: bool,
}

struct Pending {
    message: WfdMessage,
    cseq: u64,
}

/// 接收侧会话。
pub struct SinkSession {
    support: ReceivedMediaSupport,
    local_rtp_port: u16,
    local_device: DeviceInfoSubelement,
    state: SinkState,
    end_reason: Option<EndReason>,
    m1_answered: bool,
    m2_sent: bool,
    m3_answered: bool,
    cseq_out: u64,
    last_cseq_in: u64,
    pending: Vec<Pending>,
    session_id: Option<String>,
    negotiated: Option<NegotiatedStream>,
    remote_ports: Option<RtpPorts>,
    suppress_keepalive: bool,
    used_port_fallback: bool,
    last_inbound_ms: Option<u64>,
    playing_since_ms: Option<u64>,
    last_keepalive_sent_ms: Option<u64>,
    account: StreamAccount,
    messages_in: u32,
    messages_out: u32,
}

impl SinkSession {
    /// `local_rtp_port`：本端愿意收流的 UDP 端口（本层不开 socket，端口由调用方决定）。
    pub fn new(support: ReceivedMediaSupport, local_rtp_port: u16) -> Result<Self, Error> {
        if local_rtp_port == 0 {
            // F-13：来源把 port0 = 0 判为畸形，我们不该广告一个畸形值。
            return Err(invalid("local_rtp_port 不得为 0（F-13）"));
        }
        Ok(Self {
            support,
            local_rtp_port,
            local_device: DeviceInfoSubelement::new(crate::CONTROL_PORT, 0x00c8),
            state: SinkState::Idle,
            end_reason: None,
            m1_answered: false,
            m2_sent: false,
            m3_answered: false,
            cseq_out: 0,
            last_cseq_in: 0,
            pending: Vec::new(),
            session_id: None,
            negotiated: None,
            remote_ports: None,
            suppress_keepalive: false,
            used_port_fallback: false,
            last_inbound_ms: None,
            playing_since_ms: None,
            last_keepalive_sent_ms: None,
            account: StreamAccount::new(),
            messages_in: 0,
            messages_out: 0,
        })
    }

    pub fn state(&self) -> SinkState {
        self.state
    }

    pub fn end_reason(&self) -> Option<&EndReason> {
        self.end_reason.as_ref()
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn negotiated(&self) -> Option<&NegotiatedStream> {
        self.negotiated.as_ref()
    }

    pub fn remote_ports(&self) -> Option<RtpPorts> {
        self.remote_ports
    }

    pub fn device_subelement(&self) -> &DeviceInfoSubelement {
        &self.local_device
    }

    pub fn account(&self) -> &StreamAccount {
        &self.account
    }

    pub fn stats(&self) -> SessionStats {
        SessionStats {
            state: self.state,
            messages_in: self.messages_in,
            messages_out: self.messages_out,
            // 本 crate 没有解码/渲染：这一位恒为 false，避免"能协商"被读成"能显示"。
            player_available: false,
            keyframe_requests: self.account.keyframe_requests,
            packets: self.account.packets,
            payload_bytes: self.account.payload_bytes,
            lost: self.account.lost,
            reordered: self.account.reordered,
            duplicates: self.account.duplicates,
            ssrc_changes: self.account.ssrc_changes,
            payload_type_anomalies: self.account.payload_type_anomalies,
            keepalive_suppressed: self.suppress_keepalive,
            used_port_fallback: self.used_port_fallback,
        }
    }

    fn next_cseq(&mut self) -> u64 {
        self.cseq_out += 1;
        self.cseq_out
    }

    fn build_out(&mut self, message: WfdMessage, extra: &[(&str, &str)], body: &str) -> Vec<u8> {
        let cseq = self.next_cseq();
        let uri = match message {
            WfdMessage::M1Options | WfdMessage::M2Options => "*".to_string(),
            WfdMessage::M6Setup => self
                .negotiated
                .as_ref()
                .map(|n| n.presentation_url.clone())
                // 还没协商到 URL 时用控制 URI 兜底（不会走到：SETUP 前必须有 M4）。
                .unwrap_or_else(|| CONTROL_URI.to_string()),
            // M13/M16 走控制 URI（F-10/F-11 的字面量），不是 streamid URL。
            _ => CONTROL_URI.to_string(),
        };
        let session = match message {
            WfdMessage::M1Options | WfdMessage::M2Options | WfdMessage::M6Setup => None,
            _ => self.session_id.clone(),
        };
        let bytes = build_request(
            message.method(),
            &uri,
            cseq,
            session.as_deref(),
            extra,
            body,
        );
        self.pending.push(Pending { message, cseq });
        self.messages_out += 1;
        bytes
    }

    /// 本端能力广告的参数体（M3 应答）：**由实现清单推导**，空则写 `none`。
    pub fn advertisement_body(&self) -> String {
        format!(
            "{PARAM_VIDEO_FORMATS}: {}\r\n{PARAM_AUDIO_CODECS}: {}\r\n\
             {PARAM_CLIENT_RTP_PORTS}: RTP/AVP/UDP;unicast {} 0 mode=play\r\n\
             {PARAM_CONTENT_PROTECTION}: none\r\n",
            self.support.video_wire(),
            self.support.audio_wire(),
            self.local_rtp_port
        )
    }

    /// 发起 M2（`OPTIONS *`）。
    pub fn options(&mut self) -> Result<Vec<u8>, Error> {
        if self.state == SinkState::Ended {
            return Err(invalid("会话已结束"));
        }
        if self.m2_sent {
            return Err(invalid("M2 已发送过：重复 OPTIONS 明确拒绝"));
        }
        self.m2_sent = true;
        if self.state == SinkState::Idle {
            self.state = SinkState::Options;
        }
        Ok(self.build_out(
            WfdMessage::M2Options,
            &[("Require", WFD_METHOD_SET)],
            "",
        ))
    }

    /// 处理一条 source 发来的请求。
    pub fn on_request(&mut self, req: &Request, now_ms: u64) -> Result<Response, Error> {
        if self.state == SinkState::Ended {
            return Err(invalid("会话已结束：不再接受控制消息"));
        }
        let message = classify(req, Role::Source)?;
        if message.originator() != Role::Source {
            return Err(invalid(format!(
                "M{} 由 sink 发起，不该出现在入站方向（F-02..F-11）",
                message.number()
            )));
        }
        let cseq = req.cseq()?;
        check_cseq(self.last_cseq_in + 1, cseq)?;
        self.last_cseq_in = cseq;
        self.messages_in += 1;
        self.last_inbound_ms = Some(now_ms);

        match message {
            WfdMessage::M1Options => {
                if self.m1_answered {
                    return Err(invalid("M1 重复：明确拒绝（不重放应答）"));
                }
                self.m1_answered = true;
                if self.state == SinkState::Idle {
                    self.state = SinkState::Options;
                }
                let public = WfdMessage::M2Options.public_methods().join(", ");
                Ok(Response {
                    status: 200,
                    reason: "OK".into(),
                    headers: vec![("Public".into(), public)],
                    body: Vec::new(),
                })
            }
            WfdMessage::M3GetParameter => {
                if !self.m1_answered && !self.m2_sent {
                    return Err(invalid("未完成 OPTIONS 就收到 M3（顺序约束 F-26）"));
                }
                let names = parse_parameter_names(req.body_str()?)?;
                if names.is_empty() {
                    return Err(invalid("M3 正文为空：没有要查询的参数"));
                }
                self.m3_answered = true;
                self.state = SinkState::ParamsAnswered;
                Ok(Response {
                    status: 200,
                    reason: "OK".into(),
                    headers: Vec::new(),
                    body: self.advertisement_body().into_bytes(),
                })
            }
            WfdMessage::M4SetParameter => {
                if self.state != SinkState::ParamsAnswered {
                    return Err(invalid("M4 必须在 M3 之后（顺序约束 F-26）"));
                }
                let params = parse_parameter_body(req.body_str()?)?;
                let get = |name: &str| {
                    params
                        .iter()
                        .find(|(k, _)| k == name)
                        .map(|(_, v)| v.clone())
                };
                let video_raw = get(PARAM_VIDEO_FORMATS)
                    .ok_or_else(|| invalid("M4 缺 wfd_video_formats（F-05）"))?;
                let audio_raw = get(PARAM_AUDIO_CODECS)
                    .ok_or_else(|| invalid("M4 缺 wfd_audio_codecs（F-05）"))?;
                let url = get(PARAM_PRESENTATION_URL)
                    .ok_or_else(|| invalid("M4 缺 wfd_presentation_URL（F-05）"))?;
                let url = url.trim_end_matches(" none").trim().to_string();
                validate_presentation_url(&url)?;

                let remote_ports = match get(PARAM_CLIENT_RTP_PORTS) {
                    Some(raw) => Some(RtpPorts::parse(&raw)?),
                    None => None,
                };
                if let Some(ports) = remote_ports {
                    // F-14：RTCP 端口无效时来源会禁用 keep-alive——照实记账。
                    self.suppress_keepalive = ports.suppress_keepalive();
                }
                let protection = match get(PARAM_CONTENT_PROTECTION) {
                    Some(raw) => ContentProtection::parse(&raw)?,
                    None => ContentProtection::None,
                };
                if protection.requires_hdcp() {
                    // F-22：HDCP 握手属设备/厂商材料 → 明确拒绝，绝不静默忽略后继续。
                    return Err(unsupported(
                        "对端要求内容保护（HDCP）：本仓不实现 HDCP 握手，明确拒绝（F-22）",
                    ));
                }
                let remote_video = parse_video_formats(&video_raw)?;
                let remote_audio = parse_audio_codecs(&audio_raw)?;
                let video = select_common(self.support.video(), &remote_video)?;
                let audio = select_common_audio(self.support.audio(), &remote_audio)?;
                self.remote_ports = remote_ports;
                self.negotiated = Some(NegotiatedStream {
                    video,
                    audio,
                    presentation_url: url,
                    rtp_ports: remote_ports,
                    transport: None,
                    content_protection: protection,
                });
                self.state = SinkState::ParamsSet;
                Ok(Response {
                    status: 200,
                    reason: "OK".into(),
                    headers: Vec::new(),
                    body: Vec::new(),
                })
            }
            WfdMessage::M5TriggerSetup => {
                if self.state != SinkState::ParamsSet {
                    return Err(invalid("M5 触发必须在 M4 之后（顺序约束 F-26）"));
                }
                self.state = SinkState::SetupTriggered;
                Ok(Response {
                    status: 200,
                    reason: "OK".into(),
                    headers: Vec::new(),
                    body: Vec::new(),
                })
            }
            WfdMessage::M16KeepAlive => {
                if self.session_id.is_none() {
                    return Err(invalid("M16 带 Session 头但尚未建立会话（F-07/F-11）"));
                }
                Ok(Response {
                    status: 200,
                    reason: "OK".into(),
                    headers: Vec::new(),
                    body: Vec::new(),
                })
            }
            WfdMessage::M8Teardown => {
                self.state = SinkState::Ended;
                self.end_reason = Some(EndReason::Teardown);
                Ok(Response {
                    status: 200,
                    reason: "OK".into(),
                    headers: Vec::new(),
                    body: Vec::new(),
                })
            }
            other => Err(unsupported(format!(
                "未经登记的入站消息 M{}（F-26 只固定顺序，不猜语义）",
                other.number()
            ))),
        }
    }

    /// 把一条应答交给会话（按 `CSeq` 找到对应的待决请求）。
    pub fn on_response(&mut self, resp: &Response, now_ms: u64) -> Result<(), Error> {
        let cseq = resp.cseq()?;
        let index = self
            .pending
            .iter()
            .position(|p| p.cseq == cseq)
            .ok_or_else(|| invalid(format!("CSeq {cseq} 没有对应的待决请求：明确拒绝")))?;
        let pending = self.pending.remove(index);
        if !resp.ok() {
            let reason = EndReason::SetupFailed(format!(
                "M{} 应答 {} {}",
                pending.message.number(),
                resp.status,
                resp.reason
            ));
            self.state = SinkState::Ended;
            self.end_reason = Some(reason.clone());
            return Err(unsupported(format!("远端拒绝：{reason:?}")));
        }
        match pending.message {
            WfdMessage::M2Options => {
                // 应答必须声明 WFD 方法集（F-02）。
                if !resp
                    .header("Public")
                    .map(|v| v.split(',').any(|t| t.trim() == WFD_METHOD_SET))
                    .unwrap_or(false)
                {
                    return Err(invalid("M2 应答的 Public 头不含 org.wfa.wfd1.0（F-02）"));
                }
                if self.state == SinkState::Options {
                    self.state = SinkState::ParamsAnswered;
                }
            }
            WfdMessage::M6Setup => {
                let session = resp
                    .session()
                    .ok_or_else(|| invalid("SETUP 应答缺 Session 头（F-07）"))?;
                if session.is_empty() {
                    return Err(invalid("SETUP 应答的 Session 头为空（F-07）"));
                }
                self.session_id = Some(session.to_string());
                if let Some(raw) = resp.header("Transport") {
                    let (transport, fallback) = Transport::parse(raw)?;
                    self.used_port_fallback = fallback;
                    if let Some(negotiated) = self.negotiated.as_mut() {
                        negotiated.transport = Some(transport);
                    }
                }
                self.state = SinkState::SetupCreated;
            }
            WfdMessage::M7Play => {
                self.state = SinkState::Playing;
                // keep-alive 以进入 PLAYING 的时刻为基准（F-27 的间隔按来源取值比较）。
                self.playing_since_ms = Some(now_ms);
            }
            WfdMessage::M8Teardown => {
                self.state = SinkState::Ended;
                self.end_reason = Some(EndReason::Teardown);
            }
            other => {
                return Err(unsupported(format!(
                    "M{} 的应答处理未登记",
                    other.number()
                )))
            }
        }
        Ok(())
    }

    /// 发送 M6（`SETUP` + `Transport`）。
    pub fn setup(&mut self) -> Result<Vec<u8>, Error> {
        if self.state != SinkState::SetupTriggered {
            return Err(invalid("SETUP 必须在 M5 触发之后（顺序约束 F-26）"));
        }
        let transport = Transport::UdpUnicast {
            client_port: self.local_rtp_port,
            rtcp_client_port: None,
        };
        let value = transport.to_wire();
        if let Some(negotiated) = self.negotiated.as_mut() {
            negotiated.transport = Some(transport);
        }
        Ok(self.build_out(WfdMessage::M6Setup, &[("Transport", &value)], ""))
    }

    /// 发送 M7（`PLAY`）。
    pub fn play(&mut self) -> Result<Vec<u8>, Error> {
        if self.state != SinkState::SetupCreated || self.session_id.is_none() {
            return Err(invalid("PLAY 必须在 SETUP 应答（含 Session）之后（F-07/F-08）"));
        }
        Ok(self.build_out(WfdMessage::M7Play, &[], ""))
    }

    /// 发送 M8（`TEARDOWN`）。
    pub fn teardown(&mut self) -> Result<Vec<u8>, Error> {
        if self.state == SinkState::Ended {
            return Err(invalid("会话已结束"));
        }
        Ok(self.build_out(WfdMessage::M8Teardown, &[], ""))
    }

    /// 发送 M13（`SET_PARAMETER` + `wfd_idr_request`）。
    pub fn request_keyframe(&mut self) -> Result<Vec<u8>, Error> {
        if self.state != SinkState::Playing {
            return Err(invalid("关键帧请求只在 PLAYING 之后发（F-10/F-26）"));
        }
        Ok(self.build_out(
            WfdMessage::M13IdrRequest,
            &[],
            &format!("{PARAM_IDR_REQUEST}\r\n"),
        ))
    }

    /// 到点则生成 M16（keep-alive）；被 F-14 抑制时返回 `None`。
    pub fn keepalive_due(&mut self, now_ms: u64) -> Result<Option<Vec<u8>>, Error> {
        if self.state != SinkState::Playing {
            return Ok(None);
        }
        if self.suppress_keepalive {
            return Ok(None);
        }
        let reference = match self.last_keepalive_sent_ms.or(self.playing_since_ms) {
            Some(reference) => reference,
            None => return Ok(None),
        };
        let due = now_ms.saturating_sub(reference) >= KEEPALIVE_INTERVAL_MS;
        if !due {
            return Ok(None);
        }
        self.last_keepalive_sent_ms = Some(now_ms);
        Ok(Some(self.build_out(WfdMessage::M16KeepAlive, &[], "")))
    }

    /// 超时检查（F-27 的 30 s 取值）：超时 → 结束会话并返回原因。
    pub fn tick(&mut self, now_ms: u64) -> Option<&'static str> {
        if self.state != SinkState::Playing {
            return None;
        }
        if let Some(last) = self.last_inbound_ms {
            if now_ms.saturating_sub(last) > SESSION_TIMEOUT_MS {
                self.state = SinkState::Ended;
                self.end_reason = Some(EndReason::KeepaliveTimeout);
                return Some("keepalive-timeout");
            }
        }
        None
    }

    /// 收一个 RTP 包：记账；本仓策略判定需要关键帧时返回 M13 字节（F-29）。
    pub fn on_rtp(&mut self, packet: &[u8], now_ms: u64) -> Result<Option<Vec<u8>>, Error> {
        if self.state != SinkState::Playing {
            return Err(invalid("未进入 PLAYING 就收到媒体包（F-26）"));
        }
        let header = parse_header(packet)?;
        let want_keyframe = self.account.on_packet(&header, now_ms);
        if want_keyframe {
            return self.request_keyframe().map(Some);
        }
        Ok(None)
    }
}

/// presentation URL 形状校验（F-12）：`rtsp://host[:port]/wfd1.0/streamid=0`。
pub fn validate_presentation_url(url: &str) -> Result<(), Error> {
    let rest = url
        .strip_prefix("rtsp://")
        .ok_or_else(|| invalid(format!("presentation URL 必须是 rtsp scheme（F-12）：{url:?}")))?;
    if rest.contains('@') {
        // 带 userinfo 的 URL 一律拒绝（与 T41 的抓取策略同一条纪律）。
        return Err(invalid("presentation URL 不得带 userinfo（F-12）"));
    }
    let (authority, path) = rest
        .split_once('/')
        .ok_or_else(|| invalid(format!("presentation URL 缺路径（F-12）：{url:?}")))?;
    if authority.is_empty() {
        return Err(invalid("presentation URL 缺 host（F-12）"));
    }
    if let Some((_host, port)) = authority.rsplit_once(':') {
        port.parse::<u16>()
            .map_err(|_| invalid(format!("presentation URL 端口非法（F-12）：{port:?}")))?;
    }
    if path != STREAM_PATH.trim_start_matches('/') {
        return Err(invalid(format!(
            "presentation URL 路径必须为 {STREAM_PATH}（F-12）：{url:?}"
        )));
    }
    Ok(())
}

/// 生成一个 M3 应答（供演示直接展示广告内容，不走完整状态机）。
pub fn build_m3_response(cseq: u64, body: &str) -> Vec<u8> {
    build_response(200, "OK", cseq, &[], body)
}

/// 演示/测试用：构造一条 M6 的字节（用于断言方法名与 `Transport`）。
pub fn encode_setup(url: &str, cseq: u64, transport: &Transport) -> Vec<u8> {
    let value = transport.to_wire();
    build_request(METHOD_SETUP, url, cseq, None, &[("Transport", &value)], "")
}
