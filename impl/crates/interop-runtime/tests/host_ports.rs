//! T07 验收 case（plans/01-foundation.md §T07）+ HostPorts 行为测试。
//!
//! - T07-01：receive grant 用于 read arbitrary file → auth-denied。
//! - T07-02：offer 批准 60 秒后到期 → offer-expired 且不落最终文件。
//! - T07-03：QuickShare 认证成功后请求 Vault scope → auth-denied。
//!
//! 映射说明：plans/01 的 `tests/contract/host_ports.rs` →
//! `impl/crates/interop-runtime/tests/host_ports.rs`（cargo tests 目标；
//! 被测面是 interop-policy 的 grant 与 interop-runtime/standalone-host 的 ports）。
//!
//! 时钟：全部用注入的单调毫秒（fake clock），不读真实时钟（docs/05 §1）。
#![forbid(unsafe_code)]

use interop_contract::error::ErrorCode;
use interop_contract::ids::{EntryId, LeaseId, OfferId, SessionId};
use interop_contract::offer::{OfferEntry, ShareOffer};
use interop_contract::U64;
use interop_policy::grant::{Direction, Grant, ResourceScope};
use interop_runtime::host::{AdmissionRequest, ContentRequest, FakeHost, SinkHandle};
use standalone_host::{StandalonePolicy, StandaloneHost};

fn sid(s: &str) -> SessionId {
    SessionId::try_from(s.to_string()).unwrap()
}
fn eid(s: &str) -> EntryId {
    EntryId::try_from(s.to_string()).unwrap()
}
fn lid(s: &str) -> LeaseId {
    LeaseId::try_from(s.to_string()).unwrap()
}

fn entry(name: &str) -> OfferEntry {
    OfferEntry {
        entry_id: eid(name),
        display_name: name.into(),
        relative_components: Some(vec![name.into()]),
        media_type_hint: None,
        declared_size: U64(100),
        wire_payload_id: format!("wire-{name}"),
    }
}

fn offer(entries: Vec<OfferEntry>) -> ShareOffer {
    ShareOffer {
        offer_id: OfferId::try_from("ofr_t07".to_string()).unwrap(),
        endpoint_id: interop_contract::ids::EndpointId::try_from("ep_t07".to_string()).unwrap(),
        profile_id: "quickshare.lan.v1".into(),
        entries,
        total_bytes: Some(U64(100)),
        deadline: None,
        requested_actions: vec!["save".into()],
        trust_context: "untrusted-peer".into(),
        attempt_id: "att_1".into(),
    }
}

// ---------------------------------------------------------------------------
// T07-01 receive grant 不得读任意条目
// ---------------------------------------------------------------------------

#[test]
fn t07_01_receive_grant_cannot_read_foreign_entry() {
    // 接收会话：scope 只绑定 ent_a 的 Write
    let grant = Grant {
        grant_id: lid("g1"),
        subject: "quickshare-provider".into(),
        session_id: sid("ses_recv"),
        scope: ResourceScope::Entry {
            entry_id: eid("ent_a"),
        },
        direction: Direction::Write,
        purpose: "save".into(),
        byte_budget: Some(100),
        expires_at_ms: 1_000,
        consumed_bytes: 0,
    };

    // 同 scope 同方向 → 允许
    assert!(grant
        .authorize("quickshare-provider", &sid("ses_recv"), &ResourceScope::Entry { entry_id: eid("ent_a") }, Direction::Write, 10, 500)
        .is_ok());

    // ① 方向反用（Write grant 拿去 Read）
    let err = grant
        .authorize("quickshare-provider", &sid("ses_recv"), &ResourceScope::Entry { entry_id: eid("ent_a") }, Direction::Read, 10, 500)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthDenied);

    // ② 换一个 entry（"任意文件"尝试）
    let err = grant
        .authorize("quickshare-provider", &sid("ses_recv"), &ResourceScope::Entry { entry_id: eid("ent_other") }, Direction::Write, 10, 500)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthDenied);

    // ③ 换 subject（另一个 provider 拿同一 grant）
    let err = grant
        .authorize("evil-provider", &sid("ses_recv"), &ResourceScope::Entry { entry_id: eid("ent_a") }, Direction::Write, 10, 500)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthDenied);

    // ④ 换 session（复制 token 到另一会话）
    let err = grant
        .authorize("quickshare-provider", &sid("ses_other"), &ResourceScope::Entry { entry_id: eid("ent_a") }, Direction::Write, 10, 500)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthDenied);
}

#[test]
fn t07_01_fake_host_read_requires_exact_grant() {
    let mut host = FakeHost::new();
    let grant = Grant {
        grant_id: lid("g1"),
        subject: "p1".into(),
        session_id: sid("ses_recv"),
        scope: ResourceScope::Entry { entry_id: eid("ent_a") },
        direction: Direction::Write,
        purpose: "save".into(),
        byte_budget: Some(100),
        expires_at_ms: 1_000,
        consumed_bytes: 0,
    };
    host.issue_grant(grant.clone());

    // p1 的接收 grant 去读 ent_b → auth-denied（且 provider 请求根本不携带路径：
    // ContentRequest 只接受 entry_id，任意路径在类型层面不可表达）
    let err = host
        .content_read(
            "p1",
            &ContentRequest {
                session_id: sid("ses_recv"),
                scope: ResourceScope::Entry { entry_id: eid("ent_b") },
                direction: Direction::Read,
                max_bytes: 100,
            },
            500,
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthDenied);
}

// ---------------------------------------------------------------------------
// T07-02 批准 60 秒后到期 → offer-expired 且不落最终文件
// ---------------------------------------------------------------------------

#[test]
fn t07_02_expired_offer_cannot_commit() {
    let mut host = StandaloneHost::new(StandalonePolicy::default(), FakeHost::new());
    let o = offer(vec![entry("doc.pdf")]);

    // 批准（auto-accept 场景；批准即签发 scoped grants，预算 100B）
    let admission = host.admit(AdmissionRequest {
        offer: o,
        requested_scopes: vec![],
        now_ms: 0,
    });
    let grants = admission.expect("本测试场景应批准");
    assert_eq!(grants.len(), 1);

    // 59 秒：写成功（进受限 spool，不是最终文件）
    host.open_sink("quickshare-provider", &grants[0], &entry("doc.pdf"), 59_000)
        .expect("有效期内可写");
    host.sink_append(&grants[0], SinkHandle(0), b"hello".as_slice(), 59_000)
        .expect("追加");

    // 61 秒：commit → offer-expired
    let err = host
        .sink_commit(&grants[0], 61_000)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::OfferExpired);

    // 不落最终文件
    assert_eq!(
        host.published_count(),
        0,
        "过期 commit 不得产生任何最终文件"
    );
    // 且 spool 里那条受限写入也随会话作废（abort 语义）
    assert!(
        !host.has_committed(&grants[0]),
        "过期会话的 spool 不得进入已发布状态"
    );
}

// ---------------------------------------------------------------------------
// T07-03 QuickShare 认证成功 ≠ 可访问 Vault scope
// ---------------------------------------------------------------------------

#[test]
fn t07_03_authenticated_peer_cannot_request_vault_scope() {
    let mut host = StandaloneHost::new(StandalonePolicy::default(), FakeHost::new());
    // 请求列表里混入 vault scope（模拟 QuickShare 配对成功后的越权请求）
    let admission = host.admit(AdmissionRequest {
        offer: ShareOffer {
            trust_context: "quickshare-paired-and-trusted".into(),
            ..offer(vec![entry("doc.pdf")])
        },
        requested_scopes: vec![ResourceScope::Vault],
        now_ms: 0,
    });
    // 只要请求里包含 standalone 策略之外（非 allowlist）的 scope，整体拒绝：
    // 认证/配对不放大 scope（SEC-01）
    let err = admission.unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthDenied);
}

#[test]
fn t07_03_vault_scope_never_issuable_by_standalone() {
    // 即使绕过 admission 直接构造 GrantStore 请求，vault scope 也不在可签发集合
    let mut host = FakeHost::new();
    host.set_issuable_scopes(standalone_host::default_issuable_scopes());
    let err = host
        .issue_scoped(
            "quickshare-provider",
            &sid("ses_x"),
            &ResourceScope::Vault,
            Direction::Read,
            Some(1024),
            1_000,
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthDenied);
}

// ---------------------------------------------------------------------------
// 预算与其他默认拒绝
// ---------------------------------------------------------------------------

#[test]
fn byte_budget_exceeded_is_resource_limit() {
    let grant = Grant {
        grant_id: lid("g2"),
        subject: "p1".into(),
        session_id: sid("s"),
        scope: ResourceScope::Entry { entry_id: eid("e") },
        direction: Direction::Write,
        purpose: "save".into(),
        byte_budget: Some(100),
        expires_at_ms: 1_000,
        consumed_bytes: 0,
    };
    let err = grant
        .authorize("p1", &sid("s"), &ResourceScope::Entry { entry_id: eid("e") }, Direction::Write, 101, 0)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ResourceLimit);
    // 预算是累计的：两次 60B 第二次应失败
    let mut store_grant = grant.clone();
    assert!(store_grant
        .authorize("p1", &sid("s"), &ResourceScope::Entry { entry_id: eid("e") }, Direction::Write, 60, 0)
        .is_ok());
    store_grant.consume(60);
    let err = store_grant
        .authorize("p1", &sid("s"), &ResourceScope::Entry { entry_id: eid("e") }, Direction::Write, 60, 0)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ResourceLimit);
}

#[test]
fn media_host_and_network_ports_deny_by_default_in_standalone() {
    let mut host = StandaloneHost::new(StandalonePolicy::default(), FakeHost::new());
    // media：mock host 先行（T03-02），返回 platform-unavailable，不假装能渲染
    let err = host
        .media_open("p1", "airplay.legacy-mirror", 0)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PlatformUnavailable);
    // network listener：默认拒绝自动监听（docs/00 §3.10），需显式 profile 启用
    let err = host.network_listen("p1", "localsend.v2", 0).unwrap_err();
    assert_eq!(err.code, ErrorCode::PermissionRequired);
}
