//! RTSP 消息层（字段表 F-01..F-16）：方法与 M 编号、方向、`CSeq`/`Session` 语义、参数体。
//!
//! 解析纪律沿用 T32 的"先检查长度再分配"：缺 `Content-Length` 时正文必须为空、重复
//! `Content-Length` 直接拒绝（走私风险）、超限在分配前拒绝。上限值属**本仓策略**。
//!
//! **方向不靠字节猜**：M1 与 M2 在线上同形（都是 `OPTIONS *` + `Require: org.wfa.wfd1.0`，
//! 见 F-02/F-03），区分它们靠的是"谁发的"。因此分类函数要求调用方给出 sender 角色，
//! 而不是从报文里推断——推断会把两个来源的事实混成一个。

use interop_contract::error::{Error, ErrorCode};

/// 头块上限（本仓策略；WFD 控制消息不该接近这个量级）。
pub const MAX_HEADER_BYTES: usize = 8 * 1024;
/// body 上限（本仓策略）：M3/M4 的参数文本很小。
pub const MAX_BODY_BYTES: usize = 64 * 1024;
/// 单条消息头数量上限（本仓策略）。
pub const MAX_HEADERS: usize = 48;

/// RTSP 方法名（F-02..F-11）。
pub const METHOD_OPTIONS: &str = "OPTIONS";
pub const METHOD_GET_PARAMETER: &str = "GET_PARAMETER";
pub const METHOD_SET_PARAMETER: &str = "SET_PARAMETER";
pub const METHOD_SETUP: &str = "SETUP";
pub const METHOD_PLAY: &str = "PLAY";
pub const METHOD_PAUSE: &str = "PAUSE";
pub const METHOD_TEARDOWN: &str = "TEARDOWN";

/// WFD 方法集标识（F-02：`Require`/`Public` 里出现的值）。
pub const WFD_METHOD_SET: &str = "org.wfa.wfd1.0";

/// 参数名（F-04/F-05；拼写逐字取自来源，不改写）。
pub const PARAM_VIDEO_FORMATS: &str = "wfd_video_formats";
pub const PARAM_AUDIO_CODECS: &str = "wfd_audio_codecs";
pub const PARAM_CLIENT_RTP_PORTS: &str = "wfd_client_rtp_ports";
pub const PARAM_PRESENTATION_URL: &str = "wfd_presentation_URL";
pub const PARAM_CONTENT_PROTECTION: &str = "wfd_content_protection";
pub const PARAM_TRIGGER_METHOD: &str = "wfd_trigger_method";
pub const PARAM_IDR_REQUEST: &str = "wfd_idr_request";
pub const PARAM_UIBC_CAPABILITY: &str = "wfd_uibc_capability";
pub const PARAM_STANDBY_RESUME: &str = "wfd_standby_resume_capability";
pub const PARAM_COUPLED_SINK: &str = "wfd_coupled_sink";

/// M3 询问的必需参数（F-04：R33 `wfd-params.c:6-8` 把这三个列为必需）。
pub const REQUIRED_QUERY_PARAMS: [&str; 3] = [
    PARAM_CLIENT_RTP_PORTS,
    PARAM_AUDIO_CODECS,
    PARAM_VIDEO_FORMATS,
];

/// 消息发起方角色。M1 与 M2 同形，只有方向不同（F-02/F-03）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Source,
    Sink,
}

/// 字段表登记的消息（F-02..F-11）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WfdMessage {
    /// M1：source→sink 的 `OPTIONS *`（F-02）。
    M1Options,
    /// M2：sink→source 的 `OPTIONS *`（F-03，与 M1 同形）。
    M2Options,
    /// M3：`GET_PARAMETER` 询问能力（F-04）。
    M3GetParameter,
    /// M4：`SET_PARAMETER` 下发选定参数（F-05）。
    M4SetParameter,
    /// M5：`SET_PARAMETER` + `wfd_trigger_method: SETUP`（F-06）。
    M5TriggerSetup,
    /// M6：`SETUP <presentation URL>` + `Transport`（F-07）。
    M6Setup,
    /// M7：`PLAY` + `Session`（F-08）。
    M7Play,
    /// M8：`TEARDOWN`（F-09）。
    M8Teardown,
    /// M13：`SET_PARAMETER` + `wfd_idr_request`（F-10）。
    M13IdrRequest,
    /// M16：`GET_PARAMETER` + `Session`（keep-alive，F-11）。
    M16KeepAlive,
}

impl WfdMessage {
    /// 编号（F-02..F-11；编号到方法的映射见 R37 `wfd_sink_session.h:80-86`）。
    pub fn number(self) -> u8 {
        match self {
            Self::M1Options => 1,
            Self::M2Options => 2,
            Self::M3GetParameter => 3,
            Self::M4SetParameter => 4,
            Self::M5TriggerSetup => 5,
            Self::M6Setup => 6,
            Self::M7Play => 7,
            Self::M8Teardown => 8,
            Self::M13IdrRequest => 13,
            Self::M16KeepAlive => 16,
        }
    }

    pub fn method(self) -> &'static str {
        match self {
            Self::M1Options | Self::M2Options => METHOD_OPTIONS,
            Self::M3GetParameter | Self::M16KeepAlive => METHOD_GET_PARAMETER,
            Self::M4SetParameter | Self::M5TriggerSetup | Self::M13IdrRequest => {
                METHOD_SET_PARAMETER
            }
            Self::M6Setup => METHOD_SETUP,
            Self::M7Play => METHOD_PLAY,
            Self::M8Teardown => METHOD_TEARDOWN,
        }
    }

    /// 谁发起这条消息（F-02..F-11 的方向列）。
    pub fn originator(self) -> Role {
        match self {
            Self::M1Options | Self::M3GetParameter | Self::M4SetParameter | Self::M5TriggerSetup
            | Self::M16KeepAlive => Role::Source,
            Self::M2Options | Self::M6Setup | Self::M7Play | Self::M8Teardown
            | Self::M13IdrRequest => Role::Sink,
        }
    }

    /// `Public` 头里该角色声明的方法集合（F-02：R32 的 sink 与 R34/R37 的 source 取值不同）。
    pub fn public_methods(self) -> &'static [&'static str] {
        match self.originator() {
            // R32 `ctl-sink.c:42-44`（sink 只声明这三项）
            Role::Sink => &[WFD_METHOD_SET, METHOD_GET_PARAMETER, METHOD_SET_PARAMETER],
            // R34 `WifiDisplaySource.cpp:961-962`
            Role::Source => &[
                WFD_METHOD_SET,
                METHOD_SETUP,
                METHOD_TEARDOWN,
                METHOD_PLAY,
                METHOD_PAUSE,
                METHOD_GET_PARAMETER,
                METHOD_SET_PARAMETER,
            ],
        }
    }
}

/// 头列表形状（名字, 值），顺序保留。
pub type Headers = Vec<(String, String)>;

/// 解析出的 RTSP 请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub uri: String,
    pub headers: Headers,
    pub body: Vec<u8>,
}

/// 解析出的 RTSP 应答。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub reason: String,
    pub headers: Headers,
    pub body: Vec<u8>,
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

fn limit(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourceLimit, msg)
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn parse_head(buf: &[u8]) -> Result<(String, Headers, usize), Error> {
    let header_end = find_header_end(buf).ok_or_else(|| {
        if buf.len() > MAX_HEADER_BYTES {
            limit("头块超过上限")
        } else {
            invalid("未找到头块结束（CRLFCRLF）")
        }
    })?;
    if header_end > MAX_HEADER_BYTES {
        return Err(limit("头块超过上限"));
    }
    let head = std::str::from_utf8(&buf[..header_end]).map_err(|_| invalid("头块不是合法 UTF-8"))?;
    let mut lines = head.split("\r\n");
    let start_line = lines.next().ok_or_else(|| invalid("空消息"))?.to_string();
    let mut headers: Headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if headers.len() >= MAX_HEADERS {
            return Err(limit("头数量超过上限"));
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| invalid(format!("头行缺少冒号：{line:?}")))?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    Ok((start_line, headers, header_end + 4))
}

/// 按头里的 `Content-Length` 取正文（缺省 = 正文必须为空）。
fn take_body<'a>(buf: &'a [u8], offset: usize, headers: &Headers) -> Result<(&'a [u8], usize), Error> {
    let mut declared: Option<u64> = None;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("Content-Length") {
            if declared.is_some() {
                return Err(invalid("重复 Content-Length（走私风险）：明确拒绝"));
            }
            let n: u64 = value
                .trim()
                .parse()
                .map_err(|_| invalid(format!("Content-Length 非法：{value:?}")))?;
            declared = Some(n);
        }
    }
    let len = declared.unwrap_or(0);
    if len as usize > MAX_BODY_BYTES {
        return Err(limit("body 超过上限（分配前拒绝）"));
    }
    let len = len as usize;
    if buf.len() < offset + len {
        return Err(invalid("body 截断"));
    }
    Ok((&buf[offset..offset + len], offset + len))
}

impl Request {
    /// 解析一个请求，返回 `(request, consumed)`。
    pub fn parse(buf: &[u8]) -> Result<(Self, usize), Error> {
        let (start_line, headers, offset) = parse_head(buf)?;
        let mut parts = start_line.split_whitespace();
        let method = parts.next().ok_or_else(|| invalid("缺 method"))?.to_string();
        let uri = parts.next().ok_or_else(|| invalid("缺 URI"))?.to_string();
        let version = parts.next().ok_or_else(|| invalid("缺 RTSP 版本"))?;
        if version != "RTSP/1.0" {
            return Err(invalid(format!("不支持的 RTSP 版本 {version:?}")));
        }
        if !matches!(
            method.as_str(),
            METHOD_OPTIONS
                | METHOD_GET_PARAMETER
                | METHOD_SET_PARAMETER
                | METHOD_SETUP
                | METHOD_PLAY
                | METHOD_PAUSE
                | METHOD_TEARDOWN
        ) {
            return Err(Error::new(
                ErrorCode::UnsupportedMethod,
                format!("未知 WFD RTSP 方法 {method:?}：fail-closed（R32 方法集之外一律不认）"),
            ));
        }
        let (body, consumed) = take_body(buf, offset, &headers)?;
        Ok((
            Self {
                method,
                uri,
                headers,
                body: body.to_vec(),
            },
            consumed,
        ))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn cseq(&self) -> Result<u64, Error> {
        let raw = self
            .header("CSeq")
            .ok_or_else(|| invalid("缺 CSeq"))?
            .trim();
        raw.parse()
            .map_err(|_| invalid(format!("CSeq 非法：{raw:?}")))
    }

    /// `Session` 头取分号前的子串（F-07：R32 `ctl-sink.c:178-191` 的写法）。
    pub fn session(&self) -> Option<&str> {
        let raw = self.header("Session")?;
        Some(raw.split(';').next().unwrap_or(raw).trim())
    }

    /// `Require` 里是否含 WFD 方法集（F-02）。
    pub fn require_is_wfd(&self) -> bool {
        self.header("Require")
            .map(|v| v.split(',').any(|t| t.trim() == WFD_METHOD_SET))
            .unwrap_or(false)
    }

    /// `Public` 头的方法列表（M1/M2 应答）。
    pub fn public_methods(&self) -> Vec<&str> {
        self.header("Public")
            .map(|v| v.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()).collect())
            .unwrap_or_default()
    }

    pub fn body_str(&self) -> Result<&str, Error> {
        std::str::from_utf8(&self.body).map_err(|_| invalid("body 不是合法 UTF-8"))
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header("Content-Type")
    }

    /// `Transport` 头（F-15）。
    pub fn transport(&self) -> Option<&str> {
        self.header("Transport")
    }
}

impl Response {
    pub fn parse(buf: &[u8]) -> Result<(Self, usize), Error> {
        let (start_line, headers, offset) = parse_head(buf)?;
        let mut parts = start_line.splitn(3, ' ');
        let version = parts.next().ok_or_else(|| invalid("缺 RTSP 版本"))?;
        if version != "RTSP/1.0" {
            return Err(invalid(format!("不支持的 RTSP 版本 {version:?}")));
        }
        let status: u16 = parts
            .next()
            .ok_or_else(|| invalid("缺状态码"))?
            .parse()
            .map_err(|_| invalid("状态码非法"))?;
        let reason = parts.next().unwrap_or("").to_string();
        let (body, consumed) = take_body(buf, offset, &headers)?;
        Ok((
            Self {
                status,
                reason,
                headers,
                body: body.to_vec(),
            },
            consumed,
        ))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn cseq(&self) -> Result<u64, Error> {
        let raw = self.header("CSeq").ok_or_else(|| invalid("缺 CSeq"))?.trim();
        raw.parse()
            .map_err(|_| invalid(format!("CSeq 非法：{raw:?}")))
    }

    pub fn session(&self) -> Option<&str> {
        let raw = self.header("Session")?;
        Some(raw.split(';').next().unwrap_or(raw).trim())
    }

    pub fn ok(&self) -> bool {
        self.status == 200
    }

    pub fn body_str(&self) -> Result<&str, Error> {
        std::str::from_utf8(&self.body).map_err(|_| invalid("body 不是合法 UTF-8"))
    }
}

/// 构造一个请求（我们发出的消息：`Content-Length` 只在有正文时出现）。
pub fn build_request(
    method: &str,
    uri: &str,
    cseq: u64,
    session: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: &str,
) -> Vec<u8> {
    let mut out = format!("{method} {uri} RTSP/1.0\r\nCSeq: {cseq}\r\n");
    if let Some(session) = session {
        out.push_str(&format!("Session: {session}\r\n"));
    }
    for (name, value) in extra_headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        out.push_str(&format!(
            "Content-Type: text/parameters\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    out.push_str("\r\n");
    out.push_str(body);
    out.into_bytes()
}

/// 构造一个应答。
pub fn build_response(
    status: u16,
    reason: &str,
    cseq: u64,
    extra_headers: &[(&str, &str)],
    body: &str,
) -> Vec<u8> {
    let mut out = format!("RTSP/1.0 {status} {reason}\r\nCSeq: {cseq}\r\n");
    for (name, value) in extra_headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        out.push_str(&format!(
            "Content-Type: text/parameters\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    out.push_str("\r\n");
    out.push_str(body);
    out.into_bytes()
}

/// 解析 `GET_PARAMETER` 查询正文：每行是**参数名**（可带尾随冒号，值必须为空）。
///
/// F-04 只固定"询问哪些参数"，没有固定正文字节形状；M4 的取值形状才是 `名字: 值`
/// （见 [`parse_parameter_body`]）。两种形状分开解析，避免把查询行当成"空值参数"放过去。
pub fn parse_parameter_names(body: &str) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    for line in body.split("\r\n").flat_map(|l| l.split('\n')) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let name = line.strip_suffix(':').unwrap_or(line).trim();
        if name.is_empty() {
            return Err(invalid("查询行缺少参数名"));
        }
        if name.contains(':') {
            return Err(invalid(format!(
                "查询行不应带取值（GET_PARAMETER 只列参数名）：{line:?}"
            )));
        }
        out.push(name.to_string());
    }
    Ok(out)
}

/// 解析 `SET_PARAMETER` 取值正文：每行 `名字: 值`（F-04/F-05）。
pub fn parse_parameter_body(body: &str) -> Result<Vec<(String, String)>, Error> {
    let mut out = Vec::new();
    for line in body.split("\r\n").flat_map(|l| l.split('\n')) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| invalid(format!("参数行缺少冒号：{line:?}")))?;
        out.push((name.trim().to_string(), value.trim().to_string()));
    }
    Ok(out)
}

/// 分类一条**入站**请求：`sender` 是发出这条消息的角色（不由字节推断，见模块文档）。
pub fn classify(req: &Request, sender: Role) -> Result<WfdMessage, Error> {
    match req.method.as_str() {
        METHOD_OPTIONS => {
            if !req.require_is_wfd() {
                return Err(invalid("OPTIONS 缺 `Require: org.wfa.wfd1.0`"));
            }
            Ok(match sender {
                Role::Source => WfdMessage::M1Options,
                Role::Sink => WfdMessage::M2Options,
            })
        }
        METHOD_GET_PARAMETER => {
            if req.session().is_some() {
                Ok(WfdMessage::M16KeepAlive)
            } else {
                Ok(WfdMessage::M3GetParameter)
            }
        }
        METHOD_SET_PARAMETER => {
            let body = req.body_str()?;
            // F-10：M13 的正文就是裸参数名 `wfd_idr_request`（R34 `ANetworkSession.cpp:380` 按字面量匹配），
            // 不带冒号——所以要在严格的 `名字: 值` 解析之前判定。
            if body
                .split("\r\n")
                .flat_map(|l| l.split('\n'))
                .any(|l| l.trim() == PARAM_IDR_REQUEST)
            {
                return Ok(WfdMessage::M13IdrRequest);
            }
            let params = parse_parameter_body(body)?;
            let has = |name: &str| params.iter().any(|(k, _)| k == name);
            if has(PARAM_TRIGGER_METHOD) {
                let value = params
                    .iter()
                    .find(|(k, _)| k == PARAM_TRIGGER_METHOD)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                if value != METHOD_SETUP {
                    return Err(invalid(format!(
                        "wfd_trigger_method 只认 {METHOD_SETUP:?}（F-06），收到 {value:?}"
                    )));
                }
                Ok(WfdMessage::M5TriggerSetup)
            } else {
                Ok(WfdMessage::M4SetParameter)
            }
        }
        METHOD_SETUP => {
            if req.transport().is_none() {
                return Err(invalid("SETUP 缺 Transport 头（F-07）"));
            }
            Ok(WfdMessage::M6Setup)
        }
        METHOD_PLAY => Ok(WfdMessage::M7Play),
        METHOD_PAUSE => Ok(WfdMessage::M7Play),
        METHOD_TEARDOWN => Ok(WfdMessage::M8Teardown),
        other => Err(Error::new(
            ErrorCode::UnsupportedMethod,
            format!("未登记的 WFD 消息：{other}"),
        )),
    }
}

/// 校验一条请求的 `CSeq` 是否按序递增（F-08：`CSeq` 逐条递增）。
pub fn check_cseq(expected: u64, actual: u64) -> Result<(), Error> {
    if actual < expected {
        return Err(invalid(format!(
            "CSeq 回退或重复：期望 >= {expected}，收到 {actual}"
        )));
    }
    Ok(())
}
