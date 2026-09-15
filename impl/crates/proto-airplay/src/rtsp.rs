//! RTSP framing（plans/03 T32-01）：严格解析，缺失/重复 body 长度一律拒绝。
//!
//! 规则（对齐 docs/07 §4 的"先检查后分配"与 UxPlay GHSA-479c-ww7g-wgp8 的教训）：
//! - `POST`/`PUT` **必须**带 `Content-Length`（不读到 EOF 猜长度）；
//! - **重复 `Content-Length`** → `invalid-frame`（走私风险，不做"取第一个"的宽容）；
//! - 长度非法 → `invalid-frame`；超过上限 → `resource-limit`（**分配前**）；
//! - body 截断 → `invalid-frame`；头块超限 → `resource-limit`。
//!
//! 本层只做形状与长度，不解 plist、不碰密钥。

use interop_contract::error::{Error, ErrorCode};

/// 头块上限（8 KiB）——RTSP 控制消息不该接近这个量级。
pub const MAX_HEADER_BYTES: usize = 8 * 1024;
/// body 上限（256 KiB）——控制面 plist 的量级；媒体走数据面。
pub const MAX_BODY_BYTES: usize = 256 * 1024;
/// 单请求头数量上限（防御性）。
pub const MAX_HEADERS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtspRequest {
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RtspRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn cseq(&self) -> Option<u64> {
        self.header("CSeq")?.trim().parse().ok()
    }

    /// 解析一个请求，返回 `(request, consumed)`。
    pub fn parse(buf: &[u8]) -> Result<(Self, usize), Error> {
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
        let head = std::str::from_utf8(&buf[..header_end])
            .map_err(|_| invalid("头块不是合法 UTF-8"))?;
        let mut lines = head.split("\r\n");
        let request_line = lines.next().ok_or_else(|| invalid("空请求"))?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().ok_or_else(|| invalid("缺 method"))?.to_string();
        let uri = parts.next().ok_or_else(|| invalid("缺 URI"))?.to_string();
        let version = parts.next().ok_or_else(|| invalid("缺 RTSP 版本"))?;
        if version != "RTSP/1.0" {
            return Err(invalid(format!("不支持的 RTSP 版本 {version:?}")));
        }
        if !matches!(method.as_str(), "GET" | "POST" | "SETUP" | "PLAY" | "RECORD" | "TEARDOWN" | "OPTIONS") {
            return Err(invalid(format!("未知 RTSP 方法 {method:?}：fail-closed")));
        }

        let mut headers: Vec<(String, String)> = Vec::new();
        let mut content_length: Option<u64> = None;
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| invalid(format!("头行缺少冒号：{line:?}")))?;
            let name = name.trim().to_string();
            let value = value.trim().to_string();
            if name.eq_ignore_ascii_case("Content-Length") {
                if content_length.is_some() {
                    return Err(invalid("重复 Content-Length（走私风险）：明确拒绝"));
                }
                let parsed: u64 = value
                    .parse()
                    .map_err(|_| invalid(format!("Content-Length 非法：{value:?}")))?;
                if parsed > MAX_BODY_BYTES as u64 {
                    return Err(limit(format!(
                        "Content-Length {parsed} 超过上限 {MAX_BODY_BYTES}（分配前拒绝）"
                    )));
                }
                content_length = Some(parsed);
            }
            if headers.len() >= MAX_HEADERS {
                return Err(limit("头数量超过上限"));
            }
            headers.push((name, value));
        }

        let body_start = header_end + 4;
        let length = match (content_length, method.as_str()) {
            (Some(n), _) => n as usize,
            (None, "POST") | (None, "PUT") => {
                return Err(invalid("POST/PUT 必须带 Content-Length（不读到 EOF 猜长度）"));
            }
            (None, _) => 0,
        };
        let total = body_start + length;
        if buf.len() < total {
            return Err(invalid(format!(
                "body 截断：声明 {length} 字节，实际只有 {}",
                buf.len() - body_start
            )));
        }
        Ok((
            Self {
                method,
                uri,
                headers,
                body: buf[body_start..total].to_vec(),
            },
            total,
        ))
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("negotiating")
}

fn limit(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourceLimit, why.into()).with_phase("negotiating")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtspStatus {
    Ok,
    BadRequest,
    Unauthorized,
    NotFound,
    ServiceUnavailable,
}

impl RtspStatus {
    pub fn code(&self) -> u16 {
        match self {
            Self::Ok => 200,
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::NotFound => 404,
            Self::ServiceUnavailable => 503,
        }
    }

    pub fn reason(&self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::BadRequest => "Bad Request",
            Self::Unauthorized => "Unauthorized",
            Self::NotFound => "Not Found",
            Self::ServiceUnavailable => "Service Unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtspResponse {
    pub status: RtspStatus,
    pub cseq: Option<u64>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RtspResponse {
    pub fn new(status: RtspStatus, cseq: Option<u64>) -> Self {
        Self {
            status,
            cseq,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn with_body(mut self, content_type: &str, body: Vec<u8>) -> Self {
        self.headers
            .push(("Content-Type".into(), content_type.into()));
        self.body = body;
        self
    }

    /// 编码（`Content-Length` 永远由我们写死，不继承请求里的值）。
    pub fn encode(&self) -> Vec<u8> {
        let mut out = format!(
            "RTSP/1.0 {} {}\r\n",
            self.status.code(),
            self.status.reason()
        )
        .into_bytes();
        if let Some(cseq) = self.cseq {
            out.extend_from_slice(format!("CSeq: {cseq}\r\n").as_bytes());
        }
        for (k, v) in &self.headers {
            out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
        }
        out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", self.body.len()).as_bytes());
        out.extend_from_slice(&self.body);
        out
    }
}
