//! T09 验收 case（plans/01-foundation.md §T09）+ framing/auth 行为测试。
//!
//! - T09-01：每字节分片 + 两帧粘包 → 返回两个准确请求的响应。
//! - T09-02：长度 0xffffffff → 分配前拒绝。
//! - T09-03：无 hello 直接 offers.decide → auth-denied。
//! - T09-04：major 不兼容 → 版本拒绝并关闭。
//!
//! 映射说明：plans/01 的 `tests/contract/ipc.rs` →
//! `impl/crates/interop-ipc/tests/ipc.rs`。
//!
//! Golden（docs/05 §6）：`{"jsonrpc":"2.0","id":1,"method":"ping"}` 的字节数
//! 由测试计算，不盲写常量；frame 前缀 == 实际 payload 长度。
#![forbid(unsafe_code)]

use interop_contract::error::ErrorCode;
use interop_ipc::auth::TokenStore;
use interop_ipc::frame::{encode_frame, FrameReader};
use interop_ipc::server::IpcConnection;
use std::io::Read;

/// 每次只交出 n 字节的 reader（模拟最恶劣的分片）。
struct ChunkedReader<'a> {
    data: &'a [u8],
    pos: usize,
    chunk: usize,
    max_requested: usize,
}

impl<'a> ChunkedReader<'a> {
    fn byte_by_byte(data: &'a [u8]) -> Self {
        Self { data, pos: 0, chunk: 1, max_requested: 0 }
    }
    /// 只提供 prefix 字节，之后 EOF。
    fn truncating(data: &'a [u8], prefix: usize) -> Self {
        Self { data, pos: 0, chunk: data.len().min(prefix), max_requested: 0 }
    }
}

impl Read for ChunkedReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.max_requested = self.max_requested.max(buf.len());
        let n = buf.len().min(self.chunk).min(self.data.len() - self.pos);
        if n == 0 {
            return Ok(0); // EOF
        }
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        // 截断模式：只给前 chunk 字节
        if self.chunk != 1 && self.pos >= self.chunk {
            self.pos = self.data.len();
        }
        Ok(n)
    }
}

fn hello_payload() -> Vec<u8> {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "hello",
        "params": {
            "api_major": 0, "api_minor": 1,
            "client_id": "cli_test",
            "token": "test-token-1",
            "requested_scopes": ["offers.decide"]
        }
    })
    .to_string()
    .into_bytes()
}

fn ping_payload() -> Vec<u8> {
    serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "ping"})
        .to_string()
        .into_bytes()
}

fn decide_payload() -> Vec<u8> {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "offers.decide",
        "params": {"offer_id": "ofr_x", "decision": "accept"}
    })
    .to_string()
    .into_bytes()
}

fn tokens() -> TokenStore {
    let mut t = TokenStore::new();
    t.issue_with("test-token-1", "cli_test", vec!["offers.decide".to_string()]);
    t
}

fn error_code_of(response: &[u8]) -> String {
    let v: serde_json::Value = serde_json::from_slice(response).unwrap();
    v["error"]["data"]["code"]
        .as_str()
        .or_else(|| v["error"]["code"].as_str())
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// Golden：docs/05 §6
// ---------------------------------------------------------------------------

#[test]
fn golden_ping_frame_prefix_equals_length() {
    let payload = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    // 测试计算长度，不盲写常量（docs/05 §6 要求"执行时由测试计算"）
    let mut frame = Vec::new();
    encode_frame(payload, &mut frame);
    assert_eq!(
        &frame[..4],
        &(payload.len() as u32).to_be_bytes(),
        "frame 前缀必须等于 payload 实际字节数"
    );
    assert_eq!(&frame[4..], payload);
}

// ---------------------------------------------------------------------------
// T09-01 每字节分片 + 两帧粘包
// ---------------------------------------------------------------------------

#[test]
fn t09_01_byte_fragments_and_coalesced_frames() {
    let mut stream = Vec::new();
    encode_frame(&hello_payload(), &mut stream);
    encode_frame(&ping_payload(), &mut stream); // 两帧粘在一起

    // 每字节一个 read() 调用喂入
    let mut reader = FrameReader::new(ChunkedReader::byte_by_byte(&stream));
    let mut conn = IpcConnection::new("instance-1");

    let f1 = reader.read_frame().unwrap().unwrap();
    let r1 = conn.handle_payload(&f1, &tokens()).unwrap();
    let v1: serde_json::Value = serde_json::from_slice(&r1).unwrap();
    assert!(v1["result"]["version"]
        .as_str()
        .unwrap()
        .starts_with("interop.api/0."));

    let f2 = reader.read_frame().unwrap().unwrap();
    let r2 = conn.handle_payload(&f2, &tokens()).unwrap();
    let v2: serde_json::Value = serde_json::from_slice(&r2).unwrap();
    assert_eq!(v2["result"]["pong"], serde_json::json!(true));
}

// ---------------------------------------------------------------------------
// T09-02 长度 0xffffffff：分配前拒绝
// ---------------------------------------------------------------------------

#[test]
fn t09_02_oversize_length_rejected_before_allocation() {
    let raw = 0xffff_ffffu32.to_be_bytes();
    // 只提供 4 字节头（身体永远不会来）：拒绝必须发生在读身体/分配之前
    let mut reader = FrameReader::new(ChunkedReader::truncating(&raw, 4));
    let err = reader.read_frame().unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    // 超限（>256KiB）与 0 长度同样在分配前拒绝
    let oversized = 262_145u32.to_be_bytes().to_vec();
    let mut r2 = FrameReader::new(ChunkedReader::truncating(&oversized, 4));
    assert_eq!(r2.read_frame().unwrap_err().code, ErrorCode::InvalidFrame);
    let zero = 0u32.to_be_bytes().to_vec();
    let mut r3 = FrameReader::new(ChunkedReader::truncating(&zero, 4));
    assert_eq!(r3.read_frame().unwrap_err().code, ErrorCode::InvalidFrame);
}

// ---------------------------------------------------------------------------
// T09-03 无 hello 调 offers.decide → auth-denied
// ---------------------------------------------------------------------------

#[test]
fn t09_03_offers_decide_without_hello_is_auth_denied() {
    let mut conn = IpcConnection::new("instance-1");
    let resp = conn.handle_payload(&decide_payload(), &tokens()).unwrap();
    assert_eq!(error_code_of(&resp), "auth-denied");
    // 未认证状态下 ping 同样拒绝
    let resp2 = conn.handle_payload(&ping_payload(), &tokens()).unwrap();
    assert_eq!(error_code_of(&resp2), "auth-denied");
}

// ---------------------------------------------------------------------------
// T09-04 major 不兼容 → 版本拒绝并关闭
// ---------------------------------------------------------------------------

#[test]
fn t09_04_incompatible_major_rejects_and_closes() {
    let mut conn = IpcConnection::new("instance-1");
    let bad_hello = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "hello",
        "params": {
            "api_major": 1, "api_minor": 0,
            "client_id": "cli_x", "token": "test-token-1",
            "requested_scopes": []
        }
    })
    .to_string()
    .into_bytes();
    let resp = conn.handle_payload(&bad_hello, &tokens()).unwrap();
    assert_eq!(error_code_of(&resp), "version-unsupported");
    assert!(conn.is_closed(), "版本拒绝后连接必须关闭");

    // 关闭后任何帧都拒绝服务
    let err = conn.handle_payload(&ping_payload(), &tokens()).unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthDenied);
}

// ---------------------------------------------------------------------------
// 其他 fail-closed 行为
// ---------------------------------------------------------------------------

#[test]
fn non_json_and_batch_frames_rejected() {
    let mut conn = IpcConnection::new("instance-1");
    // 非 JSON
    let err = conn
        .handle_payload(b"not json at all", &tokens())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    // 批量 JSON-RPC（初版禁止）
    let batch = serde_json::json!([{"jsonrpc":"2.0","id":1,"method":"ping"}])
        .to_string()
        .into_bytes();
    let err = conn.handle_payload(&batch, &tokens()).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    // 坏 UTF-8（在 frame 层面表现为 payload 无法成为合法 JSON）
    let err = conn
        .handle_payload(&[0xFF, 0xFE, 0x00, 0x01], &tokens())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);
}

#[test]
fn hello_with_wrong_token_is_denied_and_unknown_method_rejected() {
    let mut conn = IpcConnection::new("instance-1");
    let bad_token = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "hello",
        "params": {"api_major": 0, "api_minor": 1, "client_id": "cli",
                    "token": "wrong", "requested_scopes": []}
    })
    .to_string()
    .into_bytes();
    assert_eq!(error_code_of(&conn.handle_payload(&bad_token, &tokens()).unwrap()), "auth-denied");

    // 正确 hello 后：未知方法 → unsupported-method
    let good = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "hello",
        "params": {"api_major": 0, "api_minor": 1, "client_id": "cli_test",
                    "token": "test-token-1", "requested_scopes": ["offers.decide"]}
    })
    .to_string()
    .into_bytes();
    conn.handle_payload(&good, &tokens()).unwrap();
    let unknown = serde_json::json!({"jsonrpc":"2.0","id":9,"method":"shell.exec"})
        .to_string()
        .into_bytes();
    assert_eq!(
        error_code_of(&conn.handle_payload(&unknown, &tokens()).unwrap()),
        "unsupported-method"
    );
    // scope 不足（未申请 offers.decide 的另一个 token）
    let mut t2 = TokenStore::new();
    t2.issue_with("limited", "cli_lim", vec![]);
    let hello2 = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "hello",
        "params": {"api_major": 0, "api_minor": 1, "client_id": "cli_lim",
                    "token": "limited", "requested_scopes": ["offers.decide"]}
    })
    .to_string()
    .into_bytes();
    // hello 只在 PreHello 有效：申请超出 token 自身 scope 的授权 → 授予结果为空
    let mut conn2 = IpcConnection::new("instance-1");
    conn2.handle_payload(&hello2, &t2).unwrap();
    let err = conn2.handle_payload(&decide_payload(), &t2).unwrap();
    assert_eq!(error_code_of(&err), "auth-denied");
}
