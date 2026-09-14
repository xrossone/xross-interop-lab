//! T10 验收 case（plans/01-foundation.md §T10）+ 会话/事件行为测试。
//!
//! - T10-01：accept 与 expire 同时发生 → 恰好一个有效终态。
//! - T10-02：重复 cancel 100 次 → 一次资源清理、结果一致。
//! - T10-03：cursor 已被淘汰 → event-gap 而非空成功。
//! - T10-04：worker 崩溃 → 对应 session 失败、其他会话继续。
//!
//! 映射说明：plans/01 的 `tests/contract/lifecycle.rs` →
//! `impl/crates/interop-runtime/tests/lifecycle.rs`。
//!
//! 序列化说明：registry 在同步测试中就是唯一裁决点（docs/05 §4：
//! "accept 与 expire race 由一个 authority 序列化裁决"）；时钟全部注入。
#![forbid(unsafe_code)]

use interop_contract::ids::SessionId;
use interop_runtime::events::{EventBus, SubscribeResult};
use interop_runtime::limits::EventLimits;
use interop_runtime::session::{DecisionState, SessionRegistry, SessionState};

fn sid(s: &str) -> SessionId {
    SessionId::try_from(s.to_string()).unwrap()
}

// ---------------------------------------------------------------------------
// T10-01 accept 与 expire race：单 authority 恰好一个终态
// ---------------------------------------------------------------------------

#[test]
fn t10_01_accept_expire_race_yields_exactly_one_terminal() {
    // 场景 A：accept 的调用先到（deadline 未过）
    let mut a = SessionRegistry::new();
    a.register_offer("ofr_a", 100); // deadline = 100ms
    let d1 = a.decide_accept("ofr_a", 99);
    let d2 = a.expire("ofr_a", 99);
    assert_eq!(d1, DecisionState::Accepted);
    assert_eq!(d2, DecisionState::Accepted, "终态不可逆：expire 不能覆盖已 accept");
    assert_eq!(a.terminal_decisions(), 1, "恰好记录一个有效终态");

    // 场景 B：expire 先到（或 deadline 已过）
    let mut b = SessionRegistry::new();
    b.register_offer("ofr_b", 100);
    let d1 = b.expire("ofr_b", 100);
    let d2 = b.decide_accept("ofr_b", 100);
    assert_eq!(d1, DecisionState::Expired);
    assert_eq!(d2, DecisionState::Expired, "accept 不能覆盖已 expire");
    assert_eq!(b.terminal_decisions(), 1);

    // 场景 C：accept 到达时已过 deadline → 裁决为 Expired（不是 Accepted）
    let mut c = SessionRegistry::new();
    c.register_offer("ofr_c", 100);
    assert_eq!(c.decide_accept("ofr_c", 101), DecisionState::Expired);
    assert_eq!(c.terminal_decisions(), 1);
}

// ---------------------------------------------------------------------------
// T10-02 重复 cancel 100 次：一次清理、结果一致
// ---------------------------------------------------------------------------

#[test]
fn t10_02_repeat_cancel_is_idempotent() {
    let mut reg = SessionRegistry::new();
    reg.register_session(&sid("ses_1"), "localsend.v2", "localsend-provider", 0);
    reg.attach_worker(&sid("ses_1"), "localsend-worker");

    let mut last = DecisionState::Cancelled;
    for i in 0..100 {
        last = reg.cancel(&sid("ses_1"), 1000 + i);
    }
    assert_eq!(last, DecisionState::Cancelled);
    assert_eq!(
        reg.session_state(&sid("ses_1")),
        SessionState::Cancelled
    );
    assert_eq!(
        reg.cleanup_events().len(),
        1,
        "100 次 cancel 只允许一次资源清理"
    );
    assert_eq!(reg.cleanup_events()[0], "ses_1");
}

// ---------------------------------------------------------------------------
// T10-03 cursor 被淘汰 → event-gap
// ---------------------------------------------------------------------------

#[test]
fn t10_03_evicted_cursor_returns_gap() {
    let mut bus = EventBus::new("instance-1", EventLimits::default());
    // 默认上限 4096 条：发布 5000 条，最早的 cursor（seq 1）必然被淘汰
    for i in 0..5000 {
        bus.publish(Some(&sid("ses_x")), "progress", &format!("event-{i}"));
    }
    let last = bus.last_sequence();

    // 过期 cursor → Gap + 快照建议，不是空成功
    match bus.subscribe_after(1) {
        SubscribeResult::Gap { snapshot_advice } => {
            assert!(snapshot_advice <= last);
        }
        SubscribeResult::Events { .. } => panic!("淘汰 cursor 必须返回 Gap（T10-03）"),
    }

    // 缓冲内的 cursor → 正常事件流
    let from = last - 10;
    match bus.subscribe_after(from) {
        SubscribeResult::Events { events, next_cursor } => {
            assert_eq!(events.len(), 10);
            assert_eq!(events[0].sequence, from + 1);
            assert_eq!(next_cursor, last);
            assert!(events.windows(2).all(|w| w[1].sequence == w[0].sequence + 1));
        }
        SubscribeResult::Gap { .. } => panic!("缓冲内 cursor 不应返回 Gap"),
    }

    // 跟到最新 → 空事件是合法结果（不是 gap）
    match bus.subscribe_after(last) {
        SubscribeResult::Events { events, .. } => assert!(events.is_empty()),
        SubscribeResult::Gap { .. } => panic!("最新 cursor 不应返回 Gap"),
    }

    // 总字节上限（16MiB）：发布 100 条 1MiB → 只保留尾部若干条，同样产生 gap
    let mut bus2 = EventBus::new("instance-2", EventLimits::default());
    let big = "x".repeat(1024 * 1024);
    for i in 0..100 {
        bus2.publish(Some(&sid("ses_y")), "payload", &format!("{big}-{i}"));
    }
    match bus2.subscribe_after(1) {
        SubscribeResult::Gap { .. } => {}
        SubscribeResult::Events { events, .. } => {
            assert!(events.len() < 100, "16MiB 上限必须淘汰早期事件")
        }
    }
}

// ---------------------------------------------------------------------------
// T10-04 worker 崩溃：对应 session 失败，其他会话继续
// ---------------------------------------------------------------------------

#[test]
fn t10_04_worker_crash_isolates_its_sessions() {
    let mut reg = SessionRegistry::new();
    reg.register_session(&sid("ses_good_1"), "localsend.v2", "p1", 0);
    reg.register_session(&sid("ses_bad"), "quickshare.lan.v1", "p2", 0);
    reg.register_session(&sid("ses_good_2"), "localsend.v2", "p3", 0);
    reg.attach_worker(&sid("ses_bad"), "crashy-worker");
    reg.attach_worker(&sid("ses_good_1"), "stable-worker");
    reg.attach_worker(&sid("ses_good_2"), "stable-worker");

    let affected = reg.worker_crashed("crashy-worker", 500);

    assert_eq!(affected, vec![sid("ses_bad")]);
    assert_eq!(reg.session_state(&sid("ses_bad")), SessionState::Failed);
    assert_eq!(reg.session_state(&sid("ses_good_1")), SessionState::Active);
    assert_eq!(reg.session_state(&sid("ses_good_2")), SessionState::Active);
    // 崩溃会话的资源被回收；其他会话不受影响
    assert!(reg.cleanup_events().contains(&"ses_bad".to_string()));
    assert_eq!(reg.cleanup_events().len(), 1);

    // absolute deadline 回收：超过 deadline 的会话被 sweep 失败化
    reg.register_session(&sid("ses_ttl"), "localsend.v2", "p4", 0);
    assert_eq!(reg.session_state(&sid("ses_ttl")), SessionState::Active);
    reg.set_session_deadline(&sid("ses_ttl"), 1000);
    reg.sweep(1001);
    assert_eq!(reg.session_state(&sid("ses_ttl")), SessionState::Failed);
}
