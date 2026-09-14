//! 单连接 JSON-RPC 状态机：PreHello → Ready（→ Closed）。
//!
//! 方法 allowlist：hello（仅 PreHello）/ ping / capabilities.list /
//! offers.list / offers.decide（需 scope）。认证前任何业务方法 →
//! `auth-denied`（T09-03）；hello 的 major 与本端不兼容 →
//! `version-unsupported` 且连接进入 Closed（T09-04）。

use interop_contract::error::{Error, ErrorCode};
use crate::auth::TokenStore;
use serde_json::{json, Value};

pub const API_MAJOR: u32 = 0;
pub const API_MINOR: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnState {
    PreHello,
    Ready,
    Closed,
}

/// 一条控制连接的协议状态。
#[derive(Debug)]
pub struct IpcConnection {
    state: ConnState,
    instance_id: String,
    granted_scopes: Vec<String>,
}

impl IpcConnection {
    pub fn new(instance_id: &str) -> Self {
        Self {
            state: ConnState::PreHello,
            instance_id: instance_id.to_string(),
            granted_scopes: Vec::new(),
        }
    }

    pub fn is_closed(&self) -> bool {
        self.state == ConnState::Closed
    }

    /// 处理一个已分帧的 payload，返回 JSON-RPC 响应字节。
    /// 返回 `Err` 时调用方必须关闭连接（帧级/协议级违规）。
    pub fn handle_payload(&mut self, payload: &[u8], tokens: &TokenStore) -> Result<Vec<u8>, Error> {
        if self.state == ConnState::Closed {
            return Err(Error::new(ErrorCode::AuthDenied, "连接已关闭"));
        }
        let text = std::str::from_utf8(payload)
            .map_err(|_| Error::new(ErrorCode::InvalidFrame, "payload 不是合法 UTF-8"))?;
        let msg: Value = serde_json::from_str(text)
            .map_err(|_| Error::new(ErrorCode::InvalidFrame, "payload 不是合法 JSON"))?;
        if !msg.is_object() {
            // 批量 JSON-RPC（数组）与其他非对象一律拒绝：初版禁批量
            return Err(Error::new(ErrorCode::InvalidFrame, "JSON-RPC 批量/非对象被禁止"));
        }
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        match self.dispatch(method, &params, tokens) {
            Ok(result) => Ok(json!({"jsonrpc":"2.0","id":id,"result":result}).to_string().into_bytes()),
            Err(e) => {
                if e.code == ErrorCode::VersionUnsupported {
                    self.state = ConnState::Closed;
                }
                Ok(json!({
                    "jsonrpc":"2.0","id":id,
                    "error":{"code": -32000, "message": e.message,
                             "data": {"code": e.code, "retryable": e.retryable}}
                })
                .to_string()
                .into_bytes())
            }
        }
    }

    fn dispatch(&mut self, method: &str, params: &Value, tokens: &TokenStore) -> Result<Value, Error> {
        match method {
            "hello" => self.hello(params, tokens),
            _ if self.state != ConnState::Ready => Err(Error::new(
                ErrorCode::AuthDenied,
                "hello 未完成：认证前不接受任何业务方法",
            )
            .with_phase("authenticating")),
            "ping" => Ok(json!({"pong": true, "instance_id": self.instance_id})),
            "capabilities.list" => Ok(json!({"capabilities": []})),
            "offers.list" => {
                self.require_scope("offers.list")?;
                Ok(json!({"offers": []}))
            }
            "offers.decide" => {
                self.require_scope("offers.decide")?;
                Ok(json!({"decided": true}))
            }
            other => Err(Error::new(
                ErrorCode::UnsupportedMethod,
                format!("未知方法 {other:?}"),
            )),
        }
    }

    fn require_scope(&self, scope: &str) -> Result<(), Error> {
        if self.granted_scopes.iter().any(|s| s == scope) {
            Ok(())
        } else {
            Err(Error::new(ErrorCode::AuthDenied, format!("缺少 scope {scope:?}")))
        }
    }

    fn hello(&mut self, params: &Value, tokens: &TokenStore) -> Result<Value, Error> {
        if self.state != ConnState::PreHello {
            return Err(Error::new(ErrorCode::AuthDenied, "hello 只能调用一次"));
        }
        let major = params.get("api_major").and_then(|v| v.as_u64()).unwrap_or(u64::MAX) as u32;
        let minor = params.get("api_minor").and_then(|v| v.as_u64()).unwrap_or(u64::MAX) as u32;
        // 版本协商：0.x 要求 minor 精确相等；major 不兼容 → 拒绝并关闭（T09-04）
        let compatible = major == API_MAJOR && (major != 0 || minor == API_MINOR);
        if !compatible {
            return Err(Error::new(
                ErrorCode::VersionUnsupported,
                format!("客户端 API {major}.{minor} 与本端 {API_MAJOR}.{API_MINOR} 不兼容"),
            ));
        }
        let token = params.get("token").and_then(|v| v.as_str()).unwrap_or("");
        let requested: Vec<String> = params
            .get("requested_scopes")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let Some(scoped) = tokens.verify(token) else {
            return Err(Error::new(ErrorCode::AuthDenied, "token 无效"));
        };
        // 授予 = token 自身 scopes ∩ 本次请求（不放大）
        self.granted_scopes = requested
            .into_iter()
            .filter(|s| scoped.scopes.contains(s))
            .collect();
        self.state = ConnState::Ready;
        Ok(json!({
            "version": format!("interop.api/{API_MAJOR}.{API_MINOR}"),
            "granted_scopes": self.granted_scopes,
            "limits": {"max_frame": 262144},
            "instance_id": self.instance_id,
        }))
    }
}
