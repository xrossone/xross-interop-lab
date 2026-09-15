//! T4 · seam 模拟实证（阶段 2 goal T4）：外来 offer / 媒体会话 fixture 走完整 seam。
//!
//! 证据等级 = **simulated**：wire 由 testkit 确定性 fixture 造出（不实现任何真实协议栈），
//! 但**每一段都是真实代码路径**：发现 → registry/路由 → 准入（scoped grant）→ HostPorts →
//! interop-file 原子落盘 → 事件投影；媒体面走 会话 → canonical tracks → 本地 spool。
//!
//! 断言里最重的三条（ADR-003 的 seam 语义）：
//! - **默认拒绝**：没有 grant 的读写一律 auth-denied；Vault 等非 Entry scope 永不签发；
//! - **无 control 面全权**：standalone host 结构上只有 scoped HostPorts（本测试按其公开 API 调用）；
//! - **本地/不上 fabric**：媒体只写本地 spool，监听默认关闭（network-listen 被拒），
//!   media_open 在无渲染器时显式失败而不是开假窗口。

use interop_contract::capability::{Capability, EvidenceState, Role};
use interop_contract::error::ErrorCode;
use interop_contract::ids::{EndpointId, EntryId, OfferId, SessionId};
use interop_contract::media::{MediaDescriptor, MediaForm, SessionIntent, Track};
use interop_contract::offer::{OfferEntry, ShareOffer};
use interop_contract::U64;
use interop_file::integrity::sha256_hex;
use interop_file::path::sanitize;
use interop_file::spool::SpoolStore;
use interop_platform::discovery::{EndpointAddress, EndpointObservation};
use interop_policy::grant::{ResourceScope, ScopeKind};
use interop_runtime::endpoints::EndpointRegistry;
use interop_runtime::events::{EventBus, SubscribeResult};
use interop_runtime::host::{AdmissionRequest, ContentRequest, FakeHost};
use interop_runtime::limits::EventLimits;
use interop_runtime::routing::{
    plan_route, RoutePolicy, RoutePurpose, RouteRequest, SecurityRequirement,
};
use interop_runtime::session::{SessionRegistry, SessionState};
use interop_testkit::peer::Fragmenter;
use standalone_host::{default_issuable_scopes, StandaloneHost, StandalonePolicy};
use std::path::PathBuf;

const PAYLOAD: &[u8] = b"fake-foreign-file-content: 4KiB-ish payload for the seam simulation";

fn temp_root(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("interop-t4-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp root");
    d
}

fn peer_capability(role: Role) -> Capability {
    Capability {
        profile_id: "localsend.v2".into(),
        role,
        provider_id: "localsend-provider".into(),
        state: EvidenceState::Simulated,
        available: true,
        prerequisites: vec![],
        media_forms: vec![],
        evidence_ids: vec![],
    }
}

/// 模拟一个 F01 形状的外部 peer（fixture，不是真 wire）。
fn observe_peer(registry: &mut EndpointRegistry) -> EndpointId {
    registry
        .observe(EndpointObservation {
            endpoint_id: EndpointId::try_from("ep_ls_1").unwrap(),
            profile_id: "localsend.v2".into(),
            source: interop_platform::discovery::DiscoverySource::Fake,
            display_name: "Galaxy S26".into(),
            identity_claim: Some("ls-fingerprint-fixture".into()),
            addresses: vec![EndpointAddress {
                host: "192.168.1.9".into(),
                port: 53317,
                interface: "en0".into(),
                secure: true,
            }],
            capabilities: vec![peer_capability(Role::Send)],
            observed_at_ms: 1_000,
        })
        .expect("observe")
}

fn fixture_offer(endpoint: &EndpointId, display_name: &str, components: Option<Vec<String>>) -> ShareOffer {
    ShareOffer {
        offer_id: OfferId::try_from("ofr_fixture_1").unwrap(),
        endpoint_id: endpoint.clone(),
        profile_id: "localsend.v2".into(),
        entries: vec![OfferEntry {
            entry_id: EntryId::try_from("ent_1").unwrap(),
            display_name: display_name.into(),
            relative_components: components,
            media_type_hint: Some("application/octet-stream".into()),
            declared_size: U64(PAYLOAD.len() as u64),
            wire_payload_id: "wire-token-fixture".into(),
        }],
        total_bytes: Some(U64(PAYLOAD.len() as u64)),
        deadline: Some("2026-09-15T12:00:00Z".into()),
        requested_actions: vec!["save".into()],
        trust_context: "external-foreign".into(),
        attempt_id: "att_1".into(),
    }
}

fn host() -> StandaloneHost {
    StandaloneHost::new(StandalonePolicy::default(), FakeHost::new())
}

/// 文件面主干：外来 offer → 路由 → 准入 → scoped grant → 原子落盘 → 事件投影。
#[test]
fn t4_file_plane_seam_end_to_end() {
    let root = temp_root("file");
    let mut registry = EndpointRegistry::new(30_000);
    let endpoint = observe_peer(&mut registry);

    // 1) 路由：接收方向要求对端声明 send（方向不静默互换）
    let route = plan_route(
        &registry,
        &RoutePolicy::default(),
        &RouteRequest {
            endpoint_id: endpoint.clone(),
            profile_id: "localsend.v2".into(),
            purpose: RoutePurpose::FileReceive,
            min_security: SecurityRequirement::RequireSecure,
        },
        1_100,
    )
    .expect("fixture peer 可路由");
    assert!(route.secure, "仅安全候选");
    assert_eq!(route.subject, "localsend.v2@ep_ls_1");

    // 2) 准入：requested scope 只有 Entry → 签发 scoped grant
    let mut host = host();
    let offer = fixture_offer(&endpoint, "holiday.bin", None);
    offer.validate().expect("offer 形状合法");
    let entry = offer.entries[0].clone();
    let grants = host
        .admit(AdmissionRequest {
            offer: offer.clone(),
            requested_scopes: vec![ResourceScope::Entry {
                entry_id: entry.entry_id.clone(),
            }],
            now_ms: 1_100,
        })
        .expect("standalone policy 已启用 localsend.v2");
    assert_eq!(grants.len(), 1);
    let grant = grants[0].clone();
    assert_eq!(grant.subject, "localsend-provider");

    // 3) HostPorts：开 sink + 确定性分片写入（testkit 固定 seed）
    let handle = host
        .open_sink("localsend-provider", &grant, &entry, 1_100)
        .expect("open_sink");
    let mut fragmenter = Fragmenter::new(20_260_915);
    for chunk in fragmenter.fragment(PAYLOAD) {
        host.sink_append(&grant, handle, &chunk.bytes, 1_100)
            .expect("append");
    }

    // 4) 原子落盘（interop-file）：hash 校验 → no-follow → rename
    let mut spool = SpoolStore::open(&root).expect("spool root");
    let spool_handle = spool
        .begin(&entry, PAYLOAD.len() as u64)
        .expect("begin");
    for chunk in Fragmenter::new(7).fragment(PAYLOAD) {
        spool
            .write(spool_handle, chunk.offset, &chunk.bytes)
            .expect("write");
    }
    let final_rel = sanitize(entry.relative_components.as_deref(), &entry.display_name)
        .expect("安全路径");
    let receipt = spool
        .commit(spool_handle, &final_rel, Some(&sha256_hex(PAYLOAD)))
        .expect("commit");
    assert_eq!(receipt.sha256_hex, sha256_hex(PAYLOAD));
    assert_eq!(receipt.size, PAYLOAD.len() as u64);
    let landed = root.join("holiday.bin");
    assert_eq!(std::fs::read(&landed).expect("文件已落盘"), PAYLOAD);
    assert!(landed.starts_with(&root), "落盘必须在 spool root 内");
    assert_eq!(spool.published().len(), 1);

    // 5) host 侧 commit 记账（fake authority）+ 事件投影（in-process 与 IPC 同一序列源）
    host.sink_commit(&grant, 1_200).expect("host commit");
    let mut bus = EventBus::new("instance-fixture", EventLimits::default());
    let sid = SessionId::try_from("ses_att_1").unwrap();
    assert_eq!(bus.publish(Some(&sid), "transfer.started", "total"), 1);
    assert_eq!(bus.publish(Some(&sid), "transfer.progress", "bytes"), 2);
    let published_seq = bus.publish(
        Some(&sid),
        "transfer.published",
        &format!("sha256={}", receipt.sha256_hex),
    );
    assert_eq!(published_seq, 3);
    match bus.subscribe_after(0) {
        SubscribeResult::Events { events, next_cursor } => {
            assert_eq!(events.len(), 3, "无 gap：缓冲内连续");
            assert_eq!(next_cursor, 3);
            assert!(events[2].data.contains(&receipt.sha256_hex));
            assert_eq!(events[2].session_id.as_ref(), Some(&sid));
        }
        other => panic!("不应出现 gap：{other:?}"),
    }
}

/// 文件面负向：默认拒绝、预算、路径形状、symlink、越权 scope、无监听。
#[test]
fn t4_file_plane_denials_are_fail_closed() {
    let root = temp_root("denials");
    let mut registry = EndpointRegistry::new(30_000);
    let endpoint = observe_peer(&mut registry);
    let mut host = host();
    let offer = fixture_offer(&endpoint, "holiday.bin", None);
    let entry = offer.entries[0].clone();

    // a) 没有 grant：读/写一律 auth-denied（默认拒绝）
    let no_grant = host.content_read(
        "localsend-provider",
        &ContentRequest {
            session_id: SessionId::try_from("ses_att_1").unwrap(),
            scope: ResourceScope::Entry {
                entry_id: entry.entry_id.clone(),
            },
            direction: interop_policy::grant::Direction::Write,
            max_bytes: 1024,
        },
        1_100,
    );
    assert_eq!(
        no_grant.expect_err("无 grant 必须拒绝").code,
        ErrorCode::AuthDenied
    );

    // b) 越权 scope（Vault）整单拒绝：认证成功也不放大 scope
    let err = host
        .admit(AdmissionRequest {
            offer: offer.clone(),
            requested_scopes: vec![ResourceScope::Vault],
            now_ms: 1_100,
        })
        .expect_err("Vault 永不签发");
    assert_eq!(err.code, ErrorCode::AuthDenied);
    assert_eq!(
        default_issuable_scopes(),
        vec![ScopeKind::Entry],
        "standalone 可签发集合只有 Entry"
    );

    // c) 未知 profile 默认关闭
    let mut disabled = offer.clone();
    disabled.profile_id = "m05-wfd".into();
    assert_eq!(
        host.admit(AdmissionRequest {
            offer: disabled,
            requested_scopes: vec![ResourceScope::Entry {
                entry_id: entry.entry_id.clone(),
            }],
            now_ms: 1_100,
        })
        .expect_err("profile 未启用")
        .code,
        ErrorCode::PlatformUnavailable
    );

    // d) 路径形状：穿越 / 分隔符 / 控制字符 → invalid-frame，零写入
    //    （display_name 也是不受信任的路径段——域层与落盘层都要把关）
    for (name, components) in [
        ("..", None),
        ("a/b", None),
        ("bad\u{7}name", None),
        ("ok.bin", Some(vec!["..".to_string()])),
        ("ok.bin", Some(vec!["a/b".to_string()])),
    ] {
        let bad = fixture_offer(&endpoint, name, components);
        assert_eq!(
            bad.validate().expect_err("形状拒绝").code,
            ErrorCode::InvalidFrame,
            "display_name={name:?} 必须被拒"
        );
    }
    // Windows 保留名是平台规则，由落盘边界（interop-file）拒绝——域层放行、文件层必须拦
    let reserved = fixture_offer(&endpoint, "CON.txt", None);
    reserved.validate().expect("域形状本身合法");
    assert_eq!(
        sanitize(reserved.entries[0].relative_components.as_deref(), "CON.txt")
            .expect_err("文件层必须拒绝保留名")
            .code,
        ErrorCode::InvalidFrame
    );

    // e) 预算：超限 → resource-limit，且临时文件被清理、不产生发布物
    let mut spool = SpoolStore::open(&root).expect("spool root");
    let handle = spool.begin(&entry, 16).expect("begin");
    let err = spool
        .write(handle, 0, &[0u8; 32])
        .expect_err("超过预算");
    assert_eq!(err.code, ErrorCode::ResourceLimit);
    assert!(spool.published().is_empty());
    let leftovers: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "超预算必须删除临时文件，实际残留：{leftovers:?}"
    );

    // f) symlink 目标目录：commit 时 no-follow 检查拒绝（不跟随到 root 之外）
    let outside = temp_root("outside");
    let mut spool2 = SpoolStore::open(&root).expect("spool root");
    let handle2 = spool2.begin(&entry, PAYLOAD.len() as u64).expect("begin");
    spool2.write(handle2, 0, PAYLOAD).expect("write");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("sub")).expect("symlink fixture");
    let symlinked = sanitize(Some(&["sub".to_string()]), "escape.bin").expect("路径形状合法");
    let err = spool2
        .commit(handle2, &symlinked, None)
        .expect_err("symlink 目录必须拒绝");
    assert_eq!(err.code, ErrorCode::InvalidFrame);
    assert!(
        !outside.join("escape.bin").exists(),
        "不得写到 root 之外"
    );

    // g) 网络：监听默认关闭（不存在第二个 fabric / 后台 listener）
    let err = host
        .network_listen("localsend-provider", "localsend.v2", 1_100)
        .expect_err("监听必须显式启用");
    assert_eq!(err.code, ErrorCode::PermissionRequired);

    // h) 媒体：无渲染器时显式不可用（不开假窗口）
    let err = host
        .media_open("airplay-provider", "m01-airplay", 1_100)
        .expect_err("standalone 无渲染器");
    assert_eq!(err.code, ErrorCode::PlatformUnavailable);
}

/// 媒体面（最小形状）：会话 → canonical tracks → 本地 spool；不上 fabric、崩溃隔离。
#[test]
fn t4_media_plane_stays_local_and_isolates_crash() {
    let root = temp_root("media");
    let mut registry = EndpointRegistry::new(30_000);

    // 远端呈现者（镜像可渲染）与 URL-only TV（不能冒充 mirror）
    let renderer = registry
        .observe(EndpointObservation {
            endpoint_id: EndpointId::try_from("ep_tv_render").unwrap(),
            profile_id: "m01-airplay".into(),
            source: interop_platform::discovery::DiscoverySource::Fake,
            display_name: "Living Room TV".into(),
            identity_claim: None,
            addresses: vec![EndpointAddress {
                host: "10.0.0.20".into(),
                port: 7000,
                interface: "en0".into(),
                secure: true,
            }],
            capabilities: vec![Capability {
                profile_id: "m01-airplay".into(),
                role: Role::Renderer,
                provider_id: "airplay-provider".into(),
                state: EvidenceState::Simulated,
                available: true,
                prerequisites: vec![],
                media_forms: vec![MediaForm::EncodedStream, MediaForm::PcmStream],
                evidence_ids: vec![],
            }],
            observed_at_ms: 2_000,
        })
        .expect("observe renderer");
    let url_only = registry
        .observe(EndpointObservation {
            endpoint_id: EndpointId::try_from("ep_tv_url").unwrap(),
            profile_id: "m01-airplay".into(),
            source: interop_platform::discovery::DiscoverySource::Fake,
            display_name: "URL-only TV".into(),
            identity_claim: None,
            addresses: vec![EndpointAddress {
                host: "10.0.0.21".into(),
                port: 7000,
                interface: "en0".into(),
                secure: true,
            }],
            capabilities: vec![Capability {
                profile_id: "m01-airplay".into(),
                role: Role::Renderer,
                provider_id: "airplay-provider".into(),
                state: EvidenceState::Simulated,
                available: true,
                prerequisites: vec![],
                media_forms: vec![MediaForm::MediaResource],
                evidence_ids: vec![],
            }],
            observed_at_ms: 2_000,
        })
        .expect("observe url-only");

    let mirror = |endpoint: EndpointId| RouteRequest {
        endpoint_id: endpoint,
        profile_id: "m01-airplay".into(),
        purpose: RoutePurpose::Media(SessionIntent::Mirror),
        min_security: SecurityRequirement::RequireSecure,
    };
    assert!(
        plan_route(&registry, &RoutePolicy::default(), &mirror(renderer.clone()), 2_100).is_ok(),
        "声明 encoded/pcm 的 renderer 可接收镜像"
    );
    assert_eq!(
        plan_route(&registry, &RoutePolicy::default(), &mirror(url_only), 2_100)
            .expect_err("URL-only 不能冒充 mirror")
            .code,
        ErrorCode::UnsupportedFeature
    );

    // canonical tracks（音视频分轨；clock domain 单列）
    let sid = SessionId::try_from("ses_media_1").unwrap();
    let descriptor = MediaDescriptor {
        session_id: sid.clone(),
        intent: SessionIntent::Mirror,
        tracks: vec![
            Track {
                codec: "h264".into(),
                format_id: 1,
                timebase: "1/90000".into(),
                clock_domain: "clk_gen_1".into(),
                layout_or_dimensions: Some("1920x1080".into()),
                transport_security: "dtls-srtp".into(),
                codec_extradata_hash: None,
            },
            Track {
                codec: "aac".into(),
                format_id: 1,
                timebase: "1/44100".into(),
                clock_domain: "clk_gen_1".into(),
                layout_or_dimensions: Some("stereo".into()),
                transport_security: "dtls-srtp".into(),
                codec_extradata_hash: None,
            },
        ],
        source_form: MediaForm::EncodedStream,
        controls: vec!["volume".into()],
        permissions: vec![],
        owner_provider: "airplay-provider".into(),
    };
    assert_eq!(descriptor.tracks.len(), 2, "音视频分轨保留");

    // 本地录制落盘（file sink 形态）：只进本地 spool，没有任何对 peer 的发布路径
    let recording = b"fake-recording-bytes-locally-written";
    let entry = OfferEntry {
        entry_id: EntryId::try_from("ent_rec_1").unwrap(),
        display_name: "session-recording.bin".into(),
        relative_components: Some(vec!["recordings".to_string()]),
        media_type_hint: Some("application/octet-stream".into()),
        declared_size: U64(recording.len() as u64),
        wire_payload_id: "rec-fixture".into(),
    };
    let mut spool = SpoolStore::open(&root).expect("spool root");
    let handle = spool.begin(&entry, recording.len() as u64).expect("begin");
    spool.write(handle, 0, recording).expect("write");
    let rel = sanitize(entry.relative_components.as_deref(), &entry.display_name).unwrap();
    let receipt = spool.commit(handle, &rel, Some(&sha256_hex(recording))).expect("commit");
    let landed = root.join("recordings/session-recording.bin");
    assert!(landed.starts_with(&root), "录制只落本地 spool");
    assert_eq!(std::fs::read(&landed).unwrap(), recording);
    assert_eq!(receipt.size, recording.len() as u64);

    // 会话注册表：worker 崩溃只影响挂靠会话，host/其他会话不受影响
    let mut sessions = SessionRegistry::new();
    let crashed = SessionId::try_from("ses_media_1").unwrap();
    let healthy = SessionId::try_from("ses_media_2").unwrap();
    sessions.register_session(&crashed, "m01-airplay", "airplay-provider", 2_000);
    sessions.attach_worker(&crashed, "worker-airplay");
    sessions.set_session_deadline(&crashed, 10_000);
    sessions.register_session(&healthy, "localsend.v2", "localsend-provider", 2_000);
    sessions.attach_worker(&healthy, "worker-localsend");

    let affected = sessions.worker_crashed("worker-airplay", 2_500);
    assert_eq!(affected, vec![crashed.clone()]);
    assert_eq!(sessions.session_state(&crashed), SessionState::Failed);
    assert_eq!(
        sessions.session_state(&healthy),
        SessionState::Active,
        "其他 provider 的会话不受影响"
    );
    assert!(!sessions.cleanup_events().is_empty(), "崩溃必须留下清理记录");

    // 会话终止（取消幂等单清理）→ 资源回收
    let cancelled = sessions.cancel(&healthy, 2_600);
    assert_eq!(cancelled, interop_runtime::session::DecisionState::Cancelled);
    assert_eq!(sessions.session_state(&healthy), SessionState::Cancelled);
    let cleanups_after = sessions.cleanup_events().len();
    sessions.cancel(&healthy, 2_700);
    assert_eq!(
        sessions.cleanup_events().len(),
        cleanups_after,
        "重复取消不重复清理"
    );
    // 过期回收（absolute deadline）
    sessions.sweep(9_999);
    assert_eq!(sessions.session_state(&crashed), SessionState::Failed);
}
