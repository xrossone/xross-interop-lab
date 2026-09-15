//! T11 验收测试（plans/01-foundation.md §T11）：多发现源与 endpoint registry。
//!
//! 覆盖验收 case T11-01..04，以及两条由 ADR-003 / 顶层设计简报固定的规格：
//! - **authority 层永不合并 identity**：同名同 IP、甚至同地址同声明不同来源，都是不同授权主体；
//! - **UI 聚合只是派生展示视图**（`presentation_groups`）：只读、可回退、不参与授权与路由。
//!
//! 发现源为 fake（确定性时钟 + 显式观察），不接真协议 mDNS/SSDP（阶段 3 才做）。

use interop_contract::capability::{Capability, EvidenceState, Role};
use interop_contract::error::ErrorCode;
use interop_contract::ids::EndpointId;
use interop_contract::media::{MediaForm, SessionIntent};
use interop_platform::discovery::{DiscoverySource, EndpointAddress, EndpointObservation};
use interop_runtime::endpoints::{presentation_groups, EndpointRegistry, UserAlias};
use interop_runtime::routing::{
    plan_route, RoutePolicy, RoutePurpose, RouteRequest, SecurityRequirement,
};
use interop_testkit::clock::FakeClock;

const TTL_MS: u64 = 30_000;
const GALAXY_IP: &str = "192.168.1.9";
const GALAXY_PORT: u16 = 53317;

fn cap(profile: &str, role: Role, forms: &[MediaForm]) -> Capability {
    Capability {
        profile_id: profile.into(),
        role,
        provider_id: "fake-provider".into(),
        state: EvidenceState::Simulated,
        available: true,
        prerequisites: vec![],
        media_forms: forms.to_vec(),
        evidence_ids: vec![],
    }
}

#[allow(clippy::too_many_arguments)]
fn obs(
    id: &str,
    profile: &str,
    name: &str,
    host: &str,
    port: u16,
    iface: &str,
    identity: Option<&str>,
    secure: bool,
    caps: Vec<Capability>,
    now_ms: u64,
) -> EndpointObservation {
    EndpointObservation {
        endpoint_id: EndpointId::try_from(id).expect("fixture id"),
        profile_id: profile.into(),
        source: DiscoverySource::Fake,
        display_name: name.into(),
        identity_claim: identity.map(Into::into),
        addresses: vec![EndpointAddress {
            host: host.into(),
            port,
            interface: iface.into(),
            secure,
        }],
        capabilities: caps,
        observed_at_ms: now_ms,
    }
}

/// 两台"同名同 IP"的对端：不同协议来源、不同 identity claim。
fn galaxy_pair() -> (EndpointObservation, EndpointObservation, EndpointObservation) {
    let a = obs(
        "ep_ls_1",
        "f01-localsend",
        "Galaxy S26",
        GALAXY_IP,
        GALAXY_PORT,
        "en0",
        Some("xr-dev-abc"),
        true,
        vec![cap("f01-localsend", Role::Receive, &[]), cap("f01-localsend", Role::Send, &[])],
        1_000,
    );
    let b = obs(
        "ep_qs_1",
        "f02-quickshare",
        "Galaxy S26",
        GALAXY_IP,
        GALAXY_PORT,
        "en0",
        Some("qs-endpoint-77"),
        true,
        vec![cap("f02-quickshare", Role::Receive, &[])],
        1_000,
    );
    // 同来源同地址、但 identity claim 不同：不得复用已建立的对端身份
    let c = obs(
        "ep_ls_2",
        "f01-localsend",
        "Galaxy S26",
        GALAXY_IP,
        GALAXY_PORT,
        "en0",
        Some("evil-claim"),
        true,
        vec![cap("f01-localsend", Role::Receive, &[])],
        1_000,
    );
    (a, b, c)
}

fn file_send(id: &str, profile: &str) -> RouteRequest {
    RouteRequest {
        endpoint_id: EndpointId::try_from(id).expect("fixture id"),
        profile_id: profile.into(),
        purpose: RoutePurpose::FileSend,
        min_security: SecurityRequirement::AllowPlaintext,
    }
}

fn policy() -> RoutePolicy {
    RoutePolicy {
        allow_plaintext_fallback: true,
        denied_endpoints: vec![],
        platform_blocked_profiles: vec![],
    }
}

/// T11-01：两个协议同名同 IP 但 identity 不同 → 两个独立授权主体。
#[test]
fn t11_01_same_name_and_ip_stay_two_subjects() {
    let mut clock = FakeClock::new(1_000);
    let mut reg = EndpointRegistry::new(TTL_MS);
    let (a, b, c) = galaxy_pair();
    reg.observe(a).expect("observe a");
    reg.observe(b).expect("observe b");
    reg.observe(c).expect("observe c");

    assert_eq!(reg.endpoints().len(), 3, "同名同 IP 不同来源/不同声明 = 三行，不合并");
    let rec_a = reg.get(&EndpointId::try_from("ep_ls_1").unwrap()).unwrap();
    let rec_b = reg.get(&EndpointId::try_from("ep_qs_1").unwrap()).unwrap();
    assert_eq!(rec_a.display_name, rec_b.display_name, "展示名相同（同名）");
    assert_eq!(
        rec_a.addresses[0].host, rec_b.addresses[0].host,
        "地址相同（同 IP）"
    );
    let subject_a = reg.get(&EndpointId::try_from("ep_ls_1").unwrap()).unwrap().subject();
    let subject_b = reg.get(&EndpointId::try_from("ep_qs_1").unwrap()).unwrap().subject();
    assert_ne!(subject_a, subject_b, "授权主体必须不同");

    // 重复观察同一 key（同 profile/地址/声明）→ 去重，不新增行、不做任何"同名合并"
    let repeat = obs(
        "ep_ls_1",
        "f01-localsend",
        "Galaxy S26",
        GALAXY_IP,
        GALAXY_PORT,
        "en0",
        Some("xr-dev-abc"),
        true,
        vec![cap("f01-localsend", Role::Receive, &[])],
        2_000,
    );
    reg.observe(repeat).expect("重复观察");
    assert_eq!(reg.endpoints().len(), 3, "重复观察只刷新 TTL/声明，不新增");
    assert_eq!(
        reg.get(&EndpointId::try_from("ep_ls_1").unwrap()).unwrap().last_seen_ms,
        2_000
    );

    // 独立授权：拒绝 B 不影响 A——即使两者地址字符串完全相同
    let mut pol = policy();
    pol.denied_endpoints.push(EndpointId::try_from("ep_qs_1").unwrap());
    clock.advance(10);
    let route_a = plan_route(&reg, &pol, &file_send("ep_ls_1", "f01-localsend"), clock.now_ms())
        .expect("A 可路由");
    assert_eq!(route_a.address.host, GALAXY_IP);
    assert_eq!(route_a.subject, subject_a);
    let err_b = plan_route(&reg, &pol, &file_send("ep_qs_1", "f02-quickshare"), clock.now_ms())
        .expect_err("B 被拒绝");
    assert_eq!(err_b.code, ErrorCode::AuthDenied, "拒绝而非借用同地址的 A");
}

/// T11-02：旧接口断开 → 地址候选过期不可发送。
#[test]
fn t11_02_interface_down_expires_candidates() {
    let clock = FakeClock::new(5_000);
    let mut reg = EndpointRegistry::new(TTL_MS);
    let (a, _, _) = galaxy_pair();
    reg.observe(a).expect("observe");
    let id = EndpointId::try_from("ep_ls_1").unwrap();

    assert_eq!(reg.sendable_addresses(&id, clock.now_ms()).len(), 1);
    let affected = reg.interface_down("en0", clock.now_ms());
    assert_eq!(affected, vec![id.clone()], "断开的接口上的候选全部失效");
    assert!(
        reg.sendable_addresses(&id, clock.now_ms()).is_empty(),
        "失效候选不可发送"
    );
    let err = plan_route(&reg, &policy(), &file_send("ep_ls_1", "f01-localsend"), clock.now_ms())
        .expect_err("无可用地址");
    assert_eq!(err.code, ErrorCode::DestinationUnavailable);

    // TTL 路径：重新观察后仅靠时间推进也过期
    let (a2, _, _) = galaxy_pair();
    let mut reg2 = EndpointRegistry::new(TTL_MS);
    reg2.observe(a2).expect("observe");
    let id2 = EndpointId::try_from("ep_ls_1").unwrap();
    assert_eq!(reg2.sendable_addresses(&id2, 6_000).len(), 1);
    assert!(reg2.sendable_addresses(&id2, 5_000 + TTL_MS + 1).is_empty(), "TTL 到期");
    let err2 = plan_route(
        &reg2,
        &policy(),
        &file_send("ep_ls_1", "f01-localsend"),
        5_000 + TTL_MS + 1,
    )
    .expect_err("过期后不可发送");
    assert_eq!(err2.code, ErrorCode::DestinationUnavailable);
}

/// T11-03：明文 fallback 违反用户策略 → auth-denied / 无可用路由。
#[test]
fn t11_03_plaintext_fallback_denied_by_policy() {
    let clock = FakeClock::new(1_000);
    let mut reg = EndpointRegistry::new(TTL_MS);
    reg.observe(obs(
        "ep_plain_1",
        "f02-quickshare",
        "Old TV",
        "10.0.0.5",
        9000,
        "en0",
        None,
        false, // 只有明文候选
        vec![cap("f02-quickshare", Role::Receive, &[])],
        1_000,
    ))
    .expect("observe");

    let strict = RoutePolicy {
        allow_plaintext_fallback: false,
        denied_endpoints: vec![],
        platform_blocked_profiles: vec![],
    };
    let err = plan_route(&reg, &strict, &file_send("ep_plain_1", "f02-quickshare"), clock.now_ms())
        .expect_err("用户策略禁止明文 fallback");
    assert_eq!(err.code, ErrorCode::AuthDenied);
    assert!(err.message.contains("明文"), "错误信息须指明明文策略：{}", err.message);

    // 用户放行明文 + 请求不要求安全 → 可路由（secure=false 如实标注）
    let route = plan_route(&reg, &policy(), &file_send("ep_plain_1", "f02-quickshare"), clock.now_ms())
        .expect("放行后可用");
    assert!(!route.secure, "不得把明文候选伪装成安全路由");

    // 请求侧要求安全 → 即使策略放行明文也不满足
    let mut req = file_send("ep_plain_1", "f02-quickshare");
    req.min_security = SecurityRequirement::RequireSecure;
    let err2 = plan_route(&reg, &policy(), &req, clock.now_ms()).expect_err("请求要求安全");
    assert_eq!(err2.code, ErrorCode::AuthDenied);

    // 有安全候选时"禁止明文 fallback"不等于拒绝：选安全候选
    let mut reg2 = EndpointRegistry::new(TTL_MS);
    let mut multi = obs(
        "ep_multi_1",
        "f02-quickshare",
        "New TV",
        "10.0.0.7",
        9000,
        "en0",
        None,
        true,
        vec![cap("f02-quickshare", Role::Receive, &[])],
        1_000,
    );
    multi.addresses.push(EndpointAddress {
        host: "10.0.0.8".into(),
        port: 9000,
        interface: "en0".into(),
        secure: false,
    });
    reg2.observe(multi).expect("observe mixed");
    let route2 = plan_route(
        &reg2,
        &strict,
        &file_send("ep_multi_1", "f02-quickshare"),
        clock.now_ms(),
    )
    .expect("有安全候选时不必拒绝");
    assert!(route2.secure, "必须选安全候选，不回落到明文");
    assert_eq!(route2.address.host, "10.0.0.7");
}

/// T11-04：只支持 URL 的 TV 请求 screen → unsupported-feature。
#[test]
fn t11_04_url_only_tv_rejects_screen() {
    let clock = FakeClock::new(1_000);
    let mut reg = EndpointRegistry::new(TTL_MS);
    reg.observe(obs(
        "ep_tv_1",
        "f-cast",
        "Living Room TV",
        "10.0.0.20",
        8009,
        "en0",
        None,
        true,
        vec![cap("f-cast", Role::Renderer, &[MediaForm::MediaResource])],
        1_000,
    ))
    .expect("observe tv");

    let screen = RouteRequest {
        endpoint_id: EndpointId::try_from("ep_tv_1").unwrap(),
        profile_id: "f-cast".into(),
        purpose: RoutePurpose::Media(SessionIntent::Screen),
        min_security: SecurityRequirement::RequireSecure,
    };
    let err = plan_route(&reg, &policy(), &screen, clock.now_ms()).expect_err("URL-only 不支持 screen");
    assert_eq!(err.code, ErrorCode::UnsupportedFeature);
    assert!(
        err.message.contains("native-presentation"),
        "错误信息须指明缺失的媒体形态，便于 UI 如实显示：{}",
        err.message
    );

    // 同一台 TV 的 URL cast 是支持的（拒绝 screen ≠ 拒绝该设备）
    let url = RouteRequest {
        purpose: RoutePurpose::Media(SessionIntent::MediaUrl),
        ..screen.clone()
    };
    let route = plan_route(&reg, &policy(), &url, clock.now_ms()).expect("URL cast 可路由");
    assert_eq!(route.profile_id, "f-cast");

    // 文件方向的能力未声明（TV 无 Receive）→ unsupported-feature，不假造成功
    let file = file_send("ep_tv_1", "f-cast");
    let err2 = plan_route(&reg, &policy(), &file, clock.now_ms()).expect_err("TV 未声明接收");
    assert_eq!(err2.code, ErrorCode::UnsupportedFeature);
}

/// 展示聚合：派生、只读、可回退，不参与授权。
#[test]
fn t11_05_presentation_groups_are_derived_not_merged() {
    let mut reg = EndpointRegistry::new(TTL_MS);
    let (a, b, c) = galaxy_pair();
    reg.observe(a).expect("a");
    reg.observe(b).expect("b");
    reg.observe(c).expect("c");

    let aliases = vec![
        UserAlias {
            alias: "Samsung S26".into(),
            endpoints: vec![
                EndpointId::try_from("ep_ls_1").unwrap(),
                EndpointId::try_from("ep_qs_1").unwrap(),
            ],
        },
        UserAlias {
            alias: "ghost".into(),
            endpoints: vec![EndpointId::try_from("ep_does_not_exist").unwrap()],
        },
    ];
    let groups = presentation_groups(&reg, &aliases);
    assert_eq!(groups.len(), 1, "只有含已知成员的 alias 成组；ghost 被丢弃");
    assert_eq!(groups[0].alias, "Samsung S26");
    assert_eq!(groups[0].members.len(), 2, "仅展示聚合，成员仍是两行");
    assert_eq!(reg.endpoints().len(), 3, "authority 视图不因聚合而合并");

    // 聚合不产生任何授权效果：被拒绝的成员仍然被拒绝
    let mut pol = policy();
    pol.denied_endpoints.push(EndpointId::try_from("ep_qs_1").unwrap());
    assert_eq!(
        plan_route(&reg, &pol, &file_send("ep_qs_1", "f02-quickshare"), 1_000)
            .expect_err("组内成员仍独立授权")
            .code,
        ErrorCode::AuthDenied
    );

    // 可回退：去掉 alias 立即回到逐源列表
    assert!(presentation_groups(&reg, &[]).is_empty());
    assert_eq!(reg.endpoints().len(), 3);
}

/// 不受信任的观察输入形状必须 fail-closed。
#[test]
fn t11_06_observation_shape_is_validated() {
    let mut reg = EndpointRegistry::new(TTL_MS);
    let good = obs(
        "ep_ok",
        "f01-localsend",
        "Peer",
        "10.0.0.1",
        1000,
        "en0",
        None,
        true,
        vec![cap("f01-localsend", Role::Receive, &[])],
        1_000,
    );
    reg.observe(good.clone()).expect("valid observation");

    let mut bad_port = good.clone();
    bad_port.addresses[0].port = 0;
    assert_eq!(
        reg.observe(bad_port).expect_err("port 0 非法").code,
        ErrorCode::InvalidFrame
    );

    let mut bad_host = good.clone();
    bad_host.addresses[0].host = String::new();
    assert_eq!(
        reg.observe(bad_host).expect_err("空 host 非法").code,
        ErrorCode::InvalidFrame
    );

    let mut bad_name = good.clone();
    bad_name.display_name = "evil\u{0}name".into();
    assert_eq!(
        reg.observe(bad_name).expect_err("NUL 显示名非法").code,
        ErrorCode::InvalidFrame
    );

    let mut bad_profile = good.clone();
    bad_profile.profile_id = String::new();
    assert_eq!(
        reg.observe(bad_profile).expect_err("空 profile 非法").code,
        ErrorCode::InvalidFrame
    );

    assert_eq!(reg.endpoints().len(), 1, "非法观察不产生记录");
}

/// 平台状态参与路由：blocked profile 明确不可用（CORE-09：不假造 stub 成功）。
#[test]
fn t11_07_platform_blocked_profile_is_unavailable() {
    let mut reg = EndpointRegistry::new(TTL_MS);
    reg.observe(obs(
        "ep_wfd_1",
        "m05-wfd",
        "Sink",
        "10.0.0.30",
        7236,
        "en0",
        None,
        true,
        vec![cap("m05-wfd", Role::Receive, &[])],
        1_000,
    ))
    .expect("observe");

    let pol = RoutePolicy {
        allow_plaintext_fallback: true,
        denied_endpoints: vec![],
        platform_blocked_profiles: vec!["m05-wfd".into()],
    };
    let err = plan_route(&reg, &pol, &file_send("ep_wfd_1", "m05-wfd"), 1_000)
        .expect_err("平台不支持");
    assert_eq!(err.code, ErrorCode::PlatformUnavailable);
}
