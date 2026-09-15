//! T45 验收：桌面 Cast receiver 的可达边界（语料 cast-018..023）。
//!
//! **核心断言是"产品路径恒拒绝"**：没有厂商签发的设备凭据材料，stock sender 不会接受我们
//! （F-26/F-28/F-33）；只有调用方显式构造 test-root 闸门时，自配对闭环才成立，且报告必须标注它不是
//! stock 兼容。

#![allow(clippy::result_large_err)]

use interop_contract::error::ErrorCode;
use proto_cast::receiver::{
    LaunchOutcome, ReceiverInfo, ReceiverSession, ReceiverState, KNOWN_TXT_KEYS, PORT_REAL_DEVICE,
    PORT_REFERENCE_RECEIVER, SERVICE_TYPE, STATUS_BUSY_JOIN, STATUS_IDLE,
};
use proto_cast::namespaces::CastPayload;

const DEMO_APP: &str = "DEMOAPP";

// ---------------------------------------------------------------- T45-01 / T45-03

#[test]
fn t45_01_vendor_trust_path_is_permanently_refused() {
    // 生产闸门：即便对端"信任我们"，也拒绝——因为真正的条件是证书链根到厂商信任的 CA。
    let mut session = ReceiverSession::new();
    assert_eq!(session.gate_name(), "vendor-required");
    session.note_sender_trust(true);
    let err = session.on_connect(Some(3)).expect_err("生产路径必须拒绝");
    assert_eq!(err.code, ErrorCode::VendorGated);
    assert!(
        err.message.contains("不获取、不伪造"),
        "拒绝理由必须写明材料边界：{}",
        err.message
    );
    // 拒绝后：不建会话、不持有 app、不发状态。
    assert_eq!(session.state(), ReceiverState::Idle);
    let acc = session.accounting();
    assert_eq!(acc.connected_senders, 0);
    assert_eq!(acc.statuses_sent, 0);
    assert!(!acc.holding_app);
    // 未连接时 LAUNCH 也被拒（状态机不放过）。
    assert_eq!(
        session.on_launch(DEMO_APP).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // 未连接时不应答心跳。
    assert_eq!(session.on_ping().unwrap_err().code, ErrorCode::InvalidFrame);
}

#[test]
fn t45_03_test_root_path_is_explicit_and_never_claims_stock() {
    // 演示路径必须显式命名，且只在对方信任同一测试根时成立。
    let mut session = ReceiverSession::test_root_for_demo(vec![DEMO_APP.to_string()]);
    assert_eq!(session.gate_name(), "test-root");
    assert_eq!(
        session.on_connect(Some(3)).unwrap_err().code,
        ErrorCode::VendorGated,
        "对方不信任测试根 → 同样拒绝"
    );
    session.note_sender_trust(true);
    assert_eq!(session.on_connect(Some(3)).expect("放行"), Some(CastPayload::Connected));
    let outcome = session.on_launch(DEMO_APP).expect("可拉起");
    match outcome {
        LaunchOutcome::Launched { app_id, transport_id, session_id } => {
            assert_eq!(app_id, DEMO_APP);
            assert!(!transport_id.is_empty() && !session_id.is_empty());
        }
        other => panic!("配置过的 app 应被拉起：{other:?}"),
    }
    // launch 成功后必须再广播一次 RECEIVER_STATUS，且带 transportId/sessionId。
    match session.status() {
        CastPayload::ReceiverStatus(status) => {
            assert!(status.transport_id().is_some(), "RECEIVER_STATUS 必须带 transportId");
            assert!(status.session_id().is_some());
            assert_eq!(status.app_id(), Some(DEMO_APP));
        }
        other => panic!("应为 RECEIVER_STATUS：{other:?}"),
    }
    assert!(session.accounting().statuses_sent >= 1);
    // test-root 闸门的名字就写在报告里——调用方无法把它误读成 stock 兼容。
    assert!(session.accounting().gate.contains("test-root"));
}

// ---------------------------------------------------------------- T45-02

#[test]
fn t45_02_discoverable_but_not_launchable_is_not_a_success() {
    // 可被发现（TXT 构造得出来）+ 可 CONNECT，但 LAUNCH 一个未配置的 app 必须是明确拒绝：
    // 不得记为 cast 成功（T45-02）。
    let mut session = ReceiverSession::test_root_for_demo(vec![DEMO_APP.to_string()]);
    session.note_sender_trust(true);
    let _ = session.on_connect(Some(3)).expect("连接");
    let info = ReceiverInfo::from_state(&session, "abcd1234", "客厅", "xross-demo");
    assert_eq!(info.to_txt().len(), KNOWN_TXT_KEYS.len(), "TXT 只有登记过的 6 个键");
    assert_eq!(info.status, STATUS_IDLE, "没有 app 在跑 → idle");

    match session.on_launch("SOME-OTHER-APP").expect("拒绝不是错误") {
        LaunchOutcome::Refused { code, reason } => {
            assert_eq!(code, ErrorCode::DestinationUnavailable);
            assert!(reason.contains("不内置、不申请"), "理由要写明 app id 策略：{reason}");
        }
        other => panic!("未配置的 app 不得被拉起：{other:?}"),
    }
    // 仍是 idle、没有 app：不得因为"被发现了"就写成功。
    assert_eq!(session.state(), ReceiverState::Connected);
    assert!(!session.accounting().holding_app);
    let after = ReceiverInfo::from_state(&session, "abcd1234", "客厅", "xross-demo");
    assert_eq!(after.status, STATUS_IDLE);
    assert_eq!(session.accounting().refused_launches, 1);
}

// ---------------------------------------------------------------- 语料 cast-018 / cast-022

#[test]
fn connect_replies_connected_only_when_a_protocol_version_was_given() {
    let mut session = ReceiverSession::test_root_for_demo(vec![DEMO_APP.to_string()]);
    session.note_sender_trust(true);
    assert_eq!(
        session.on_connect(None).expect("放行但无版本"),
        None,
        "未提供协议版本 → 不回 CONNECTED（来源行为）"
    );
    assert_eq!(session.on_connect(Some(0)).expect("放行"), Some(CastPayload::Connected));
    assert_eq!(session.accounting().connected_senders, 2);
}

#[test]
fn txt_construction_and_parsing_follow_the_source_keys() {
    let mut session = ReceiverSession::test_root_for_demo(vec![DEMO_APP.to_string()]);
    session.note_sender_trust(true);
    let _ = session.on_connect(Some(3)).expect("连接");
    let _ = session.on_launch(DEMO_APP).expect("拉起");
    let busy = ReceiverInfo::from_state(&session, "abcd1234", "客厅", "xross-demo");
    assert_eq!(busy.status, STATUS_BUSY_JOIN, "有 app 在跑 → busy");

    let txt = busy.to_txt();
    let keys: Vec<&str> = txt.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, vec!["id", "ve", "ca", "st", "fn", "md"]);
    assert_eq!(ReceiverInfo::from_txt(&txt).expect("往返"), busy);

    // 未登记的键（来源里不存在的 rs/bs/nf/ic/cd/rm）必须拒绝。
    for unknown in ["rs", "bs", "nf", "ic", "cd", "rm"] {
        let mut pairs = txt.clone();
        pairs.push((unknown.to_string(), "x".to_string()));
        assert_eq!(
            ReceiverInfo::from_txt(&pairs).unwrap_err().code,
            ErrorCode::InvalidFrame,
            "{unknown} 必须拒绝"
        );
    }
    // st 超出 0/1、数字键不是整数、缺 id 都拒绝。
    let mut bad = txt.clone();
    bad[3] = ("st".to_string(), "2".to_string());
    assert_eq!(ReceiverInfo::from_txt(&bad).unwrap_err().code, ErrorCode::InvalidFrame);
    let mut bad = txt.clone();
    bad[1] = ("ve".to_string(), "two".to_string());
    assert_eq!(ReceiverInfo::from_txt(&bad).unwrap_err().code, ErrorCode::InvalidFrame);
    let no_id: Vec<(String, String)> = txt.into_iter().filter(|(k, _)| k != "id").collect();
    assert_eq!(ReceiverInfo::from_txt(&no_id).unwrap_err().code, ErrorCode::InvalidFrame);

    // 两个端口取值分别引用、不合并。
    assert_eq!(SERVICE_TYPE, "_googlecast._tcp");
    assert_eq!(PORT_REAL_DEVICE, 8009);
    assert_eq!(PORT_REFERENCE_RECEIVER, 8010);
}

#[test]
fn session_releases_everything_on_close() {
    let mut session = ReceiverSession::test_root_for_demo(vec![DEMO_APP.to_string()]);
    session.note_sender_trust(true);
    let _ = session.on_connect(Some(3)).expect("连接");
    let _ = session.on_launch(DEMO_APP).expect("拉起");
    assert!(session.accounting().holding_app);
    let _ = session.on_ping().expect("应答应答");
    session.on_close();
    assert_eq!(session.state(), ReceiverState::Closed);
    let acc = session.accounting();
    assert!(!acc.holding_app, "关闭后不得继续持有 app");
    assert!(acc.close_seen);
    assert_eq!(session.on_ping().unwrap_err().code, ErrorCode::InvalidFrame);
    assert_eq!(
        session.on_launch(DEMO_APP).unwrap_err().code,
        ErrorCode::InvalidFrame,
        "关闭后不得再拉起"
    );
}
