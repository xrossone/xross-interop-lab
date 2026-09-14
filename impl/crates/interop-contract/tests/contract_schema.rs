//! 契约 golden 与 JSON Schema 一致性（plans/01 T06「公共schema与Rust序列化golden一致」）。
//!
//! golden 更新：`GOLDEN_UPDATE=1 cargo test -p interop-contract --test contract_schema`
#![forbid(unsafe_code)]

use interop_contract::capability::{Capability, EvidenceState, Role};
use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::{EndpointId, EntryId, OfferId};
use interop_contract::media::{MediaDescriptor, MediaForm, SessionIntent, Track};
use interop_contract::offer::{OfferEntry, ShareOffer};
use interop_contract::U64;
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn sample_capability() -> Capability {
    Capability {
        profile_id: "localsend.v2".into(),
        role: Role::Receive,
        provider_id: "localsend-core".into(),
        state: EvidenceState::SourceReviewed,
        available: true,
        prerequisites: vec!["selected-lan-interface".into()],
        media_forms: vec![],
        evidence_ids: vec!["run_example".into()],
    }
}

fn sample_offer() -> ShareOffer {
    ShareOffer {
        offer_id: OfferId::try_from("ofr_fixture".to_string()).unwrap(),
        endpoint_id: EndpointId::try_from("ep_fixture".to_string()).unwrap(),
        profile_id: "localsend.v2".into(),
        entries: vec![OfferEntry {
            entry_id: EntryId::try_from("ent_fixture".to_string()).unwrap(),
            display_name: "demo.bin".into(),
            relative_components: Some(vec!["docs".into(), "demo.bin".into()]),
            media_type_hint: Some("application/octet-stream".into()),
            declared_size: U64(9_007_199_254_740_993),
            wire_payload_id: "wp_1".into(),
        }],
        total_bytes: Some(U64(9_007_199_254_740_993)),
        deadline: None,
        requested_actions: vec!["save".into()],
        trust_context: "untrusted".into(),
        attempt_id: "att_fixture".into(),
    }
}

fn sample_error() -> Error {
    Error {
        code: ErrorCode::UnsupportedFeature,
        message: "media form not supported".into(),
        retryable: false,
        phase: Some("negotiating".into()),
        profile_id: Some("localsend.v2".into()),
        evidence_id: None,
        remedy: None,
    }
}

fn sample_media() -> MediaDescriptor {
    MediaDescriptor {
        session_id: interop_contract::ids::SessionId::try_from("ses_fixture".to_string()).unwrap(),
        intent: SessionIntent::Mirror,
        tracks: vec![Track {
            codec: "h264".into(),
            format_id: 1,
            timebase: "1/90000".into(),
            clock_domain: "sender-rtp".into(),
            layout_or_dimensions: Some("1920x1080".into()),
            transport_security: "aes-128-ctr".into(),
            codec_extradata_hash: None,
        }],
        source_form: MediaForm::EncodedStream,
        controls: vec!["play".into(), "stop".into()],
        permissions: vec![],
        owner_provider: "airplay-worker".into(),
    }
}

fn samples() -> Vec<(&'static str, String)> {
    vec![
        (
            "capability.json",
            serde_json::to_string_pretty(&sample_capability()).unwrap(),
        ),
        (
            "offer.json",
            serde_json::to_string_pretty(&sample_offer()).unwrap(),
        ),
        ("error.json", serde_json::to_string_pretty(&sample_error()).unwrap()),
        (
            "media_descriptor.json",
            serde_json::to_string_pretty(&sample_media()).unwrap(),
        ),
    ]
}

#[test]
fn golden_matches_rust_serialization() {
    for (name, actual) in samples() {
        let path = golden_dir().join(name);
        if std::env::var("GOLDEN_UPDATE").is_ok() {
            std::fs::create_dir_all(golden_dir()).unwrap();
            std::fs::write(&path, actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("golden {name} 缺失: {e}"));
        assert_eq!(actual, expected, "golden {name} 与 Rust 序列化不一致");
    }
}

#[test]
fn golden_roundtrips_to_same_value() {
    for (name, _) in samples() {
        let raw = std::fs::read_to_string(golden_dir().join(name)).unwrap();
        match name {
            "capability.json" => assert_eq!(
                serde_json::from_str::<Capability>(&raw).unwrap(),
                sample_capability()
            ),
            "offer.json" => {
                let v: ShareOffer = serde_json::from_str(&raw).unwrap();
                assert_eq!(v, sample_offer());
            }
            "error.json" => assert_eq!(
                serde_json::from_str::<Error>(&raw).unwrap(),
                sample_error()
            ),
            "media_descriptor.json" => assert_eq!(
                serde_json::from_str::<MediaDescriptor>(&raw).unwrap(),
                sample_media()
            ),
            _ => panic!("unhandled golden {name}"),
        }
    }
}

/// 公共 schema 必须与 Rust 序列化的字段集一致（防两处漂移）。
#[test]
fn schema_covers_serialized_fields() {
    let schema_raw = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/interop-api.schema.json"),
    )
    .expect("schemas/interop-api.schema.json 存在");
    let schema: serde_json::Value = serde_json::from_str(&schema_raw).unwrap();

    let check = |def_name: &str, instance: serde_json::Value| {
        let defs = schema
            .get("$defs")
            .or_else(|| schema.get("definitions"))
            .expect("schema 需要含 $defs 或 definitions");
        let def = &defs[def_name];
        assert!(
            def.is_object(),
            "schema 缺少 {def_name} 定义"
        );
        let props = def["properties"].as_object().expect("properties object");
        let fields = instance.as_object().expect("object instance").keys();
        for f in fields {
            assert!(
                props.contains_key(f),
                "{def_name} 缺少字段 {f}（schema 与 Rust 序列化漂移）"
            );
        }
    };

    check(
        "capability",
        serde_json::to_value(sample_capability()).unwrap(),
    );
    check("share_offer", serde_json::to_value(sample_offer()).unwrap());
    check("error", serde_json::to_value(sample_error()).unwrap());
    check(
        "media_descriptor",
        serde_json::to_value(sample_media()).unwrap(),
    );

    // 大整数字段在 schema 中必须是 string 编码（直接或经 u64_wire $ref）
    let defs = schema
        .get("$defs")
        .or_else(|| schema.get("definitions"))
        .unwrap();
    let declared = &defs["offer_entry"]["properties"]["declared_size"];
    let is_string = declared.get("type").and_then(|t| t.as_str()) == Some("string")
        || declared
            .get("$ref")
            .and_then(|r| r.as_str())
            .map(|r| r.ends_with("/u64_wire"))
            .unwrap_or(false);
    assert!(
        is_string,
        "大整数在 schema 中必须是 string（或 $ref u64_wire）"
    );
}
