//! SOAP 控制消息（plans/03 §T41；字段表 F-05..F-08）。
//!
//! 来源：R46 pupnp `upnp/src/soap/soap_device.c:538-602`（`SOAPACTION` 头、M-POST 变体）、
//! R49 rygel `src/librygel-renderer/rygel-av-transport.vala:37`（AVTransport:1 服务类型）、
//! `data/xml/AVTransport2.xml.in`（动作与参数名）、`data/xml/ContentDirectory.xml.in`（Browse 参数）、
//! `src/librygel-server/rygel-content-directory.vala:33,47,49`（701/720/402 错误码）。
//!
//! 严格立场：**不做"尽力解析"** —— 未知动作、服务类型不符、缺必需参数、未知参数、`InstanceID != 0`
//! 都明确拒绝；XML 走 `crate::xml` 的加固解析器（禁 DTD/实体 + 预算）。

use interop_contract::error::{Error, ErrorCode};
use std::collections::BTreeMap;

use crate::xml::{parse_document, Element, XmlBudget};

/// SOAP 信封命名空间。
pub const SOAP_ENV_NS: &str = "http://schemas.xmlsoap.org/soap/envelope/";
/// AVTransport:1 服务类型（F-06）。
pub const AV_TRANSPORT: &str = "urn:schemas-upnp-org:service:AVTransport:1";
/// ContentDirectory:1 服务类型（F-07）。
pub const CONTENT_DIRECTORY: &str = "urn:schemas-upnp-org:service:ContentDirectory:1";
/// `CurrentURIMetaData` 的字节上限（本仓策略值；DIDL-Lite 元数据通常几 KB）。
pub const MAX_URI_METADATA_BYTES: usize = 32 * 1024;

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("negotiating")
}

fn unsupported(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, why.into()).with_phase("negotiating")
}

/// 支持的动作（其余一律拒绝）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoapAction {
    SetAvTransportUri,
    Play,
    Stop,
    Seek,
    GetTransportInfo,
    GetMediaInfo,
    Browse,
}

impl SoapAction {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::SetAvTransportUri => "SetAVTransportURI",
            Self::Play => "Play",
            Self::Stop => "Stop",
            Self::Seek => "Seek",
            Self::GetTransportInfo => "GetTransportInfo",
            Self::GetMediaInfo => "GetMediaInfo",
            Self::Browse => "Browse",
        }
    }

    fn spec(self) -> ActionSpec {
        match self {
            Self::SetAvTransportUri => ActionSpec {
                service: AV_TRANSPORT,
                required: &["InstanceID", "CurrentURI"],
                optional: &["CurrentURIMetaData"],
            },
            Self::Play => ActionSpec {
                service: AV_TRANSPORT,
                required: &["InstanceID", "Speed"],
                optional: &[],
            },
            Self::Stop => ActionSpec {
                service: AV_TRANSPORT,
                required: &["InstanceID"],
                optional: &[],
            },
            Self::Seek => ActionSpec {
                service: AV_TRANSPORT,
                required: &["InstanceID", "Unit", "Target"],
                optional: &[],
            },
            Self::GetTransportInfo | Self::GetMediaInfo => ActionSpec {
                service: AV_TRANSPORT,
                required: &["InstanceID"],
                optional: &[],
            },
            Self::Browse => ActionSpec {
                service: CONTENT_DIRECTORY,
                required: &["ObjectID", "BrowseFlag", "RequestedCount"],
                optional: &["Filter", "StartingIndex", "SortCriteria"],
            },
        }
    }

    pub fn from_wire(name: &str) -> Result<Self, Error> {
        match name {
            "SetAVTransportURI" => Ok(Self::SetAvTransportUri),
            "Play" => Ok(Self::Play),
            "Stop" => Ok(Self::Stop),
            "Seek" => Ok(Self::Seek),
            "GetTransportInfo" => Ok(Self::GetTransportInfo),
            "GetMediaInfo" => Ok(Self::GetMediaInfo),
            "Browse" => Ok(Self::Browse),
            other => Err(unsupported(format!(
                "未知 SOAP 动作 {other:?}：只实现 AVTransport/ContentDirectory 的固定集合（不做尽力解析）"
            ))),
        }
    }

    pub fn service(self) -> &'static str {
        self.spec().service
    }
}

struct ActionSpec {
    service: &'static str,
    required: &'static [&'static str],
    optional: &'static [&'static str],
}

/// 一条 SOAP 消息（请求或 Fault）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoapMessage {
    pub service_type: String,
    pub action: Option<SoapAction>,
    pub args: BTreeMap<String, String>,
    /// Fault 消息：(错误码, 描述)
    pub fault: Option<(u32, String)>,
}

impl SoapMessage {
    /// 构造请求（构造期就校验动作/参数，避免生成无法解析的消息）。
    pub fn request(
        service: &str,
        action: &str,
        args: &[(&str, &str)],
    ) -> Result<Self, Error> {
        let action = SoapAction::from_wire(action)?;
        let spec = action.spec();
        if spec.service != service {
            return Err(unsupported(format!(
                "动作 {} 属于 {}，不属于 {service}",
                action.as_wire(),
                spec.service
            )));
        }
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        for (k, v) in args {
            if !spec.required.contains(k) && !spec.optional.contains(k) {
                return Err(invalid(format!("动作 {} 不接受参数 {k:?}", action.as_wire())));
            }
            map.insert((*k).to_string(), (*v).to_string());
        }
        validate_args(action, &map)?;
        Ok(Self {
            service_type: service.to_string(),
            action: Some(action),
            args: map,
            fault: None,
        })
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::parse_raw(bytes)
    }

    /// 解析（含加固 XML + 动作/参数校验）。
    pub fn parse_raw(bytes: &[u8]) -> Result<Self, Error> {
        let doc = parse_document(bytes, XmlBudget::default())?;
        if doc.local_name() != "Envelope" {
            return Err(invalid("SOAP 根元素必须是 Envelope"));
        }
        let ns = doc
            .attrs
            .get("xmlns:s")
            .or_else(|| doc.attrs.get("xmlns:soap"))
            .or_else(|| doc.attrs.get("xmlns"))
            .ok_or_else(|| invalid("Envelope 缺少 SOAP 命名空间声明"))?;
        if ns != SOAP_ENV_NS {
            return Err(invalid(format!("SOAP 命名空间必须是 {SOAP_ENV_NS}")));
        }
        let body = doc
            .child("s:Body")
            .or_else(|| doc.child("soap:Body"))
            .or_else(|| doc.child("Body"))
            .ok_or_else(|| invalid("SOAP 缺 Body"))?;

        // Fault
        if let Some(fault) = body.child("s:Fault").or_else(|| body.child("Fault")) {
            let (code, description) = parse_fault(fault)?;
            return Ok(Self {
                service_type: String::new(),
                action: None,
                args: BTreeMap::new(),
                fault: Some((code, description)),
            });
        }

        let action_el = body
            .children
            .first()
            .ok_or_else(|| invalid("SOAP Body 为空"))?;
        if body.children.len() != 1 {
            return Err(invalid("SOAP Body 必须恰好含一个动作元素"));
        }
        let action = SoapAction::from_wire(action_el.local_name())?;
        let service_type = action_el
            .attrs
            .iter()
            .find(|(k, _)| k.starts_with("xmlns:"))
            .map(|(_, v)| v.clone())
            .ok_or_else(|| invalid("动作元素缺少服务类型命名空间声明（xmlns:u）"))?;
        if service_type != action.service() {
            return Err(unsupported(format!(
                "动作 {} 声明的服务类型 {service_type:?} 与期望 {} 不符",
                action.as_wire(),
                action.service()
            )));
        }

        let mut args = BTreeMap::new();
        for child in &action_el.children {
            let name = child.local_name().to_string();
            args.insert(name, child.text.clone());
        }
        validate_args(action, &args)?;
        Ok(Self {
            service_type,
            action: Some(action),
            args,
            fault: None,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        if let Some((code, description)) = &self.fault {
            return format!(
                "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"{SOAP_ENV_NS}\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\n\
                 <s:Body>\n<s:Fault>\n<faultcode>s:Client</faultcode>\n<faultstring>UPnPError</faultstring>\n\
                 <detail>\n<UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\">\n<errorCode>{code}</errorCode>\n\
                 <errorDescription>{}</errorDescription>\n</UPnPError>\n</detail>\n</s:Fault>\n</s:Body>\n</s:Envelope>\n",
                escape(description)
            )
            .into_bytes();
        }
        let action = self.action.expect("非 Fault 必有动作");
        let mut out = format!(
            "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"{SOAP_ENV_NS}\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\n<s:Body>\n<u:{} xmlns:u=\"{}\">\n",
            action.as_wire(),
            self.service_type
        );
        for (k, v) in &self.args {
            out.push_str(&format!("<{k}>{}</{k}>\n", escape(v)));
        }
        out.push_str(&format!(
            "</u:{}>\n</s:Body>\n</s:Envelope>\n",
            action.as_wire()
        ));
        out.into_bytes()
    }

    /// `SOAPACTION` 头的值（F-05）。
    pub fn soapaction_header(&self) -> String {
        format!("\"{PROTO}#{ACT}\"", PROTO = self.service_type, ACT = self.action_name())
    }

    pub fn action_name(&self) -> &'static str {
        match self.action {
            Some(a) => a.as_wire(),
            None => "Fault",
        }
    }

    pub fn arg(&self, name: &str) -> Option<&str> {
        self.args.get(name).map(String::as_str)
    }

    /// 构造 SOAP Fault（ContentDirectory 错误码经此承载，F-08）。
    pub fn fault(code: u32, description: &str) -> Self {
        Self {
            service_type: String::new(),
            action: None,
            args: BTreeMap::new(),
            fault: Some((code, description.to_string())),
        }
    }
}

fn validate_args(action: SoapAction, args: &BTreeMap<String, String>) -> Result<(), Error> {
    let spec = action.spec();
    for req in spec.required {
        if !args.contains_key(*req) {
            return Err(invalid(format!(
                "动作 {} 缺必需参数 {req}",
                action.as_wire()
            )));
        }
    }
    for k in args.keys() {
        if !spec.required.contains(&k.as_str()) && !spec.optional.contains(&k.as_str()) {
            return Err(invalid(format!("动作 {} 不接受参数 {k:?}", action.as_wire())));
        }
    }
    if let Some(id) = args.get("InstanceID") {
        if id != "0" {
            return Err(unsupported(format!(
                "只支持 InstanceID=0（收到 {id:?}）：多实例未实现"
            )));
        }
    }
    if let Some(meta) = args.get("CurrentURIMetaData") {
        if meta.len() > MAX_URI_METADATA_BYTES {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!(
                    "CurrentURIMetaData {} 字节超过策略上限 {MAX_URI_METADATA_BYTES}",
                    meta.len()
                ),
            )
            .with_phase("negotiating"));
        }
    }
    if let Some(uri) = args.get("CurrentURI") {
        if uri.is_empty() {
            return Err(invalid("CurrentURI 不得为空（停止播放用 Stop，不是空 URI）"));
        }
    }
    Ok(())
}

fn parse_fault(fault: &Element) -> Result<(u32, String), Error> {
    let detail = fault.child("detail").ok_or_else(|| invalid("Fault 缺 detail"))?;
    let upnp_error = detail
        .children
        .iter()
        .find(|c| c.local_name() == "UPnPError")
        .ok_or_else(|| invalid("Fault 缺 UPnPError"))?;
    let code_text = upnp_error
        .child("errorCode")
        .ok_or_else(|| invalid("UPnPError 缺 errorCode"))?
        .text
        .trim()
        .to_string();
    let code: u32 = code_text
        .parse()
        .map_err(|_| invalid(format!("errorCode 不是整数：{code_text:?}")))?;
    let description = upnp_error
        .child("errorDescription")
        .map(|e| e.text.trim().to_string())
        .unwrap_or_default();
    Ok((code, description))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_encode_parse_roundtrip() {
        let msg = SoapMessage::request(
            CONTENT_DIRECTORY,
            "Browse",
            &[
                ("ObjectID", "0"),
                ("BrowseFlag", "BrowseDirectChildren"),
                ("RequestedCount", "10"),
            ],
        )
        .expect("build");
        let parsed = SoapMessage::parse(&msg.encode()).expect("parse");
        assert_eq!(parsed.action, Some(SoapAction::Browse));
        assert_eq!(parsed.arg("ObjectID"), Some("0"));
    }

    #[test]
    fn wrong_service_for_action_is_rejected() {
        assert!(SoapMessage::request(CONTENT_DIRECTORY, "Play", &[("InstanceID", "0"), ("Speed", "1")]).is_err());
        assert!(SoapMessage::request(AV_TRANSPORT, "Browse", &[("ObjectID", "0"), ("BrowseFlag", "x"), ("RequestedCount", "1")]).is_err());
    }
}
