//! T06 验收 case（plans/01-foundation.md §T06）+ 语义测试。
//!
//! - T06-01：size=9007199254740993 JSON 往返 → 保持精确字符串。
//! - T06-02：未知 critical media form → unsupported-feature（fail closed）。
//! - T06-03：role=receive 查询 send 路由 → 不得匹配。
//!
//! 映射说明：plans/01 的 `tests/contract/schema.rs` →
//! `impl/crates/interop-contract/tests/contract_schema.rs`（cargo 只自动发现
//! tests/*.rs 顶层目标）。
#![forbid(unsafe_code)]

use interop_contract::capability::{ApiVersion, Capability, EvidenceState, Role};
use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::OfferId;
use interop_contract::media::MediaForm;
use interop_contract::offer::{OfferEntry, ShareOffer};
use interop_contract::U64;

// ---------------------------------------------------------------------------
// T06-01 大整数精度
// ---------------------------------------------------------------------------

#[test]
fn t06_01_u64_beyond_f53_roundtrips_exact() {
    let size: u64 = 9_007_199_254_740_993; // 2^53 + 1：f64 必丢精度的值
    let json = serde_json::to_string(&U64(size)).expect("serialize");
    assert_eq!(
        json, "\"9007199254740993\"",
        "大整数必须编码为十进制字符串（docs/05 §1）"
    );
    let back: U64 = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.0, size, "JSON 往返后必须保持精确值");
}

#[test]
fn t06_01_offer_total_bytes_roundtrip_in_offer() {
    let offer = sample_offer();
    let json = serde_json::to_string(&offer).expect("serialize");
    let back: ShareOffer = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, offer);
    assert!(
        json.contains("\"9007199254740993\""),
        "字段在完整结构内也必须是字符串形式"
    );
}

#[test]
fn t06_01_json_number_for_bigint_is_rejected() {
    // 严格 string-only：数字形式会漂移精度语义，直接拒绝
    let err = serde_json::from_str::<U64>("9007199254740993");
    assert!(err.is_err(), "大整数不接受 JSON number 形式");
}

// ---------------------------------------------------------------------------
// T06-02 未知 critical 枚举 fail-closed
// ---------------------------------------------------------------------------

#[test]
fn t06_02_unknown_media_form_is_unsupported_feature() {
    let err = MediaForm::from_wire("holographic-stream").expect_err("未知 form 必须失败");
    assert_eq!(err.code, ErrorCode::UnsupportedFeature);
    // 已知值正常
    assert_eq!(
        MediaForm::from_wire("encoded-stream"),
        Ok(MediaForm::EncodedStream)
    );
    assert_eq!(
        MediaForm::from_wire("media-resource"),
        Ok(MediaForm::MediaResource)
    );
}

#[test]
fn t06_02_unknown_media_form_in_json_rejected_not_defaulted() {
    let json = r#"{"profile_id":"p","role":"receive","media_forms":["holographic-stream"]}"#;
    let res = serde_json::from_str::<Capability>(json);
    assert!(res.is_err(), "未知 critical media form 在反序列化时必须拒绝");
}

#[test]
fn t06_02_unknown_error_code_rejected() {
    let json = r#"{"code":"warp-field-failure","message":"x","retryable":false}"#;
    assert!(
        serde_json::from_str::<Error>(json).is_err(),
        "未知错误码必须拒绝，不得映射 default"
    );
}

// ---------------------------------------------------------------------------
// T06-03 role 匹配隔离
// ---------------------------------------------------------------------------

#[test]
fn t06_03_receive_query_must_not_match_send_capability() {
    let cap = Capability {
        profile_id: "localsend.v2".into(),
        role: Role::Receive,
        provider_id: "localsend-core".into(),
        state: EvidenceState::SourceReviewed,
        available: true,
        prerequisites: vec![],
        media_forms: vec![],
        evidence_ids: vec![],
    };
    assert!(
        !cap.matches("localsend.v2", Role::Send),
        "role=receive 的能力不得匹配 send 路由查询（T06-03）"
    );
    assert!(cap.matches("localsend.v2", Role::Receive));
    assert!(!cap.matches("quickshare.lan.v1", Role::Receive));
}

// ---------------------------------------------------------------------------
// 版本协商（能力版本协商）
// ---------------------------------------------------------------------------

#[test]
fn version_negotiation_matrix() {
    let v01 = ApiVersion::new(0, 1);
    assert_eq!(ApiVersion::parse("interop.api/0.1"), Ok(v01));
    // 0.x：minor 必须精确相等（0.x 不稳定，无向后兼容承诺）
    assert_eq!(ApiVersion::negotiate(&v01, &ApiVersion::new(0, 1)), Some(v01));
    assert_eq!(ApiVersion::negotiate(&v01, &ApiVersion::new(0, 2)), None);
    assert_eq!(ApiVersion::negotiate(&v01, &ApiVersion::new(1, 0)), None);
    // stable 后：minor 取较小者
    let a = ApiVersion::new(1, 3);
    let b = ApiVersion::new(1, 1);
    assert_eq!(ApiVersion::negotiate(&a, &b), Some(ApiVersion::new(1, 1)));
    assert_eq!(ApiVersion::negotiate(&a, &ApiVersion::new(2, 0)), None);
}

// ---------------------------------------------------------------------------
// ShareOffer 域校验
// ---------------------------------------------------------------------------

#[test]
fn offer_rejects_traversal_components() {
    let mut offer = sample_offer();
    offer.entries[0].relative_components = Some(vec!["..".into(), "etc".into()]);
    let err = offer.validate().expect_err("路径穿越组件必须被拒");
    assert_eq!(err.code, ErrorCode::InvalidFrame);
}

#[test]
fn offer_rejects_bad_id_shape() {
    let id = OfferId::try_from("ofr/../../etc".to_string());
    assert!(id.is_err(), "ID 不得包含路径分隔符");
}

fn sample_offer() -> ShareOffer {
    ShareOffer {
        offer_id: OfferId::try_from("ofr_fixture".to_string()).expect("valid"),
        endpoint_id: interop_contract::ids::EndpointId::try_from("ep_1".to_string())
            .expect("valid"),
        profile_id: "localsend.v2".into(),
        entries: vec![OfferEntry {
            entry_id: interop_contract::ids::EntryId::try_from("ent_1".to_string())
                .expect("valid"),
            display_name: "report.pdf".into(),
            relative_components: Some(vec!["docs".into(), "report.pdf".into()]),
            media_type_hint: Some("application/pdf".into()),
            declared_size: U64(9_007_199_254_740_993),
            wire_payload_id: "wire-1".into(),
        }],
        total_bytes: Some(U64(9_007_199_254_740_993)),
        deadline: None,
        requested_actions: vec!["save".into()],
        trust_context: "untrusted".into(),
        attempt_id: "att_1".into(),
    }
}
