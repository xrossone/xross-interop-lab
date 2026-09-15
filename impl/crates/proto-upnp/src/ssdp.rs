//! SSDP 报文（plans/03 §T41；字段表 F-01..F-04）。
//!
//! 来源：R46 pupnp（BSD-3-Clause）`upnp/src/inc/ssdplib.h:81`（端口 1900）、
//! `upnp/src/ssdp/ssdp_ctrlpt.c:404-429,476-501`（M-SEARCH 形状与 `MAN: "ssdp:discover"`、`MX`）、
//! `upnp/src/ssdp/ssdp_device.c:688-691`（`ssdp:alive`/`ssdp:byebye`）、
//! `upnp/src/ssdp/ssdp_server.c:632,706`（USN 形状与只接受 NOTIFY/M-SEARCH）。
//!
//! 严格立场：`MAN` 缺失或值不符、`MX` 超出上限、方法未知、`ST`/`NT`/`USN` 为空 → 一律拒绝；
//! 报文体积与头数有预算（本仓策略值），超限即拒（不做"截断后尽力解析"）。

use interop_contract::error::{Error, ErrorCode};

/// SSDP 端口（F-01）。
pub const SSDP_PORT: u16 = 1900;
/// IPv4 组播地址（F-01；由调用方填入报文）。
pub const SSDP_MULTICAST_V4: &str = "239.255.255.250";
/// `MAN` 头的期望值（F-02）。
pub const M_SEARCH_MAN: &str = "\"ssdp:discover\"";
/// 单个 SSDP 报文的字节上限（本仓策略值）。
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;
/// 头数量上限（本仓策略值）。
pub const MAX_HEADERS: usize = 48;
/// `MX` 的上限（F-02 的语义是"秒"；超过 5 秒在本仓被拒，另见字段表）。
pub const MAX_MX: u8 = 5;

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("discovered")
}

fn limit(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourceLimit, why.into()).with_phase("discovered")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsdpMethod {
    MSearch,
    Notify,
}

/// 解析后的 M-SEARCH 请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub method: SsdpMethod,
    pub st: String,
    pub mx: u8,
    pub man_is_discover: bool,
    pub host: Option<String>,
}

/// 解析后的 NOTIFY（存活/失效通告）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyMessage {
    pub nt: String,
    pub nts: String,
    pub usn: String,
    pub location: Option<String>,
    pub max_age_secs: Option<u64>,
    pub host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsdpMessage {
    Search(SearchRequest),
    Notify(NotifyMessage),
}

impl SsdpMessage {
    /// 该消息的 USN（通告/响应都有；请求没有）。
    pub fn usn(&self) -> Option<&str> {
        match self {
            Self::Notify(n) => Some(&n.usn),
            Self::Search(_) => None,
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(limit(format!(
                "SSDP 报文 {} 字节超过策略上限 {}",
                bytes.len(),
                MAX_MESSAGE_BYTES
            )));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("SSDP 报文不是合法 UTF-8"))?;
        let mut lines = text.split("\r\n");
        let start = lines.next().ok_or_else(|| invalid("空报文"))?;
        let mut parts = start.split_whitespace();
        let method = match parts.next() {
            Some("M-SEARCH") => SsdpMethod::MSearch,
            Some("NOTIFY") => SsdpMethod::Notify,
            Some(other) => {
                return Err(invalid(format!(
                    "只接受 M-SEARCH/NOTIFY（收到 {other:?}）——F-02"
                )))
            }
            None => return Err(invalid("缺少起始行")),
        };
        if parts.next() != Some("*") {
            return Err(invalid("SSDP 起始行的目标必须是 *"));
        }
        if parts.next() != Some("HTTP/1.1") {
            return Err(Error::new(
                ErrorCode::VersionUnsupported,
                "SSDP 只支持 HTTP/1.1",
            )
            .with_phase("discovered"));
        }

        let mut headers: Vec<(String, String)> = Vec::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if headers.len() >= MAX_HEADERS {
                return Err(limit(format!("SSDP 头数超过策略上限 {MAX_HEADERS}")));
            }
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| invalid(format!("非法头行：{line:?}")))?;
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
        let get = |k: &str| -> Option<&str> {
            headers
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.as_str())
        };

        match method {
            SsdpMethod::MSearch => {
                let man = get("man").ok_or_else(|| invalid("M-SEARCH 缺 MAN 头（F-02）"))?;
                if man != M_SEARCH_MAN {
                    return Err(invalid(format!(
                        "MAN 必须是 {M_SEARCH_MAN}（收到 {man:?}）"
                    )));
                }
                let mx_raw = get("mx").ok_or_else(|| invalid("M-SEARCH 缺 MX 头（F-02）"))?;
                let mx: i64 = mx_raw
                    .parse()
                    .map_err(|_| invalid(format!("MX 不是整数：{mx_raw:?}")))?;
                if mx < 1 || mx > MAX_MX as i64 {
                    return Err(invalid(format!(
                        "MX 必须在 1..={MAX_MX}（收到 {mx}；上限是本仓策略值）"
                    )));
                }
                let st = get("st").ok_or_else(|| invalid("M-SEARCH 缺 ST 头"))?;
                if st.is_empty() {
                    return Err(invalid("ST 不得为空"));
                }
                Ok(Self::Search(SearchRequest {
                    method,
                    st: st.to_string(),
                    mx: mx as u8,
                    man_is_discover: true,
                    host: get("host").map(str::to_string),
                }))
            }
            SsdpMethod::Notify => {
                let nt = get("nt").ok_or_else(|| invalid("NOTIFY 缺 NT 头"))?;
                let nts = get("nts").ok_or_else(|| invalid("NOTIFY 缺 NTS 头（F-03）"))?;
                let usn = get("usn").ok_or_else(|| invalid("NOTIFY 缺 USN 头（F-04）"))?;
                if nt.is_empty() || usn.is_empty() {
                    return Err(invalid("NT/USN 不得为空"));
                }
                if !matches!(nts, "ssdp:alive" | "ssdp:byebye" | "ssdp:update") {
                    return Err(invalid(format!(
                        "未知 NTS {nts:?}（F-03 只定义 alive/byebye/update）"
                    )));
                }
                let max_age_secs = match get("cache-control") {
                    Some(v) => {
                        let v = v.trim();
                        let n = v.strip_prefix("max-age=").ok_or_else(|| {
                            invalid(format!("CACHE-CONTROL 只支持 max-age=<秒>（收到 {v:?}）"))
                        })?;
                        Some(
                            n.trim()
                                .parse::<u64>()
                                .map_err(|_| invalid(format!("max-age 不是整数：{n:?}")))?,
                        )
                    }
                    None => None,
                };
                Ok(Self::Notify(NotifyMessage {
                    nt: nt.to_string(),
                    nts: nts.to_string(),
                    usn: usn.to_string(),
                    location: get("location").map(str::to_string),
                    max_age_secs,
                    host: get("host").map(str::to_string),
                }))
            }
        }
    }
}

/// 构造 M-SEARCH（我们作为控制点去发现设备）。
pub fn build_search(st: &str, mx: u8) -> Result<Vec<u8>, Error> {
    if st.is_empty() {
        return Err(invalid("ST 不得为空"));
    }
    if !(1..=MAX_MX).contains(&mx) {
        return Err(invalid(format!("MX 必须在 1..={MAX_MX}（本仓策略值）")));
    }
    Ok(format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {SSDP_MULTICAST_V4}:{SSDP_PORT}\r\nMAN: {M_SEARCH_MAN}\r\nMX: {mx}\r\nST: {st}\r\n\r\n"
    )
    .into_bytes())
}

/// 构造搜索响应（我们作为设备侧回答控制点）。
pub fn build_search_response(st: &str, usn: &str, location: &str, max_age_secs: u64) -> Vec<u8> {
    format!(
        "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age={max_age_secs}\r\nST: {st}\r\nUSN: {usn}\r\nLOCATION: {location}\r\n\r\n"
    )
    .into_bytes()
}

/// 构造存活通告（设备侧）。
pub fn build_alive(nt: &str, usn: &str, location: &str, max_age_secs: u64) -> Vec<u8> {
    format!(
        "NOTIFY * HTTP/1.1\r\nHOST: {SSDP_MULTICAST_V4}:{SSDP_PORT}\r\nNT: {nt}\r\nNTS: ssdp:alive\r\nUSN: {usn}\r\nLOCATION: {location}\r\nCACHE-CONTROL: max-age={max_age_secs}\r\n\r\n"
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_parse_roundtrip() {
        let bytes = build_search("ssdp:all", 3).expect("build");
        match SsdpMessage::parse(&bytes).expect("parse") {
            SsdpMessage::Search(r) => {
                assert_eq!(r.mx, 3);
                assert_eq!(r.st, "ssdp:all");
            }
            other => panic!("expected search, got {other:?}"),
        }
        let alive = build_alive("upnp:rootdevice", "uuid:x::upnp:rootdevice", "http://192.0.2.1/d.xml", 1800);
        match SsdpMessage::parse(&alive).expect("parse") {
            SsdpMessage::Notify(n) => {
                assert_eq!(n.nts, "ssdp:alive");
                assert_eq!(n.max_age_secs, Some(1800));
            }
            other => panic!("expected notify, got {other:?}"),
        }
    }

    #[test]
    fn bad_cache_control_is_rejected() {
        let msg = "NOTIFY * HTTP/1.1\r\nNT: upnp:rootdevice\r\nNTS: ssdp:alive\r\nUSN: uuid:a::upnp:rootdevice\r\nCACHE-CONTROL: no-cache\r\n\r\n";
        assert_eq!(
            SsdpMessage::parse(msg.as_bytes()).expect_err("只支持 max-age").code,
            ErrorCode::InvalidFrame
        );
    }
}
