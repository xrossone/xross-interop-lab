//! T34 验收：AP1/AP2 音频 profile 解析 + 配对登记层（语料 fx-airplay-*）。
//!
//! 全部字节与取值自制（字段号/常量取自 `specs-reviewed/m01` 的 T34 字段表）。
//! **没有真机、没有解密、没有任何厂商材料**：这里验证的是"解析与登记自洽"。

#![allow(clippy::result_large_err)]

use interop_contract::error::ErrorCode;
use proto_airplay::audio_profile::{
    is_aac_eld_no_data, parse_packed_audio_format, AudioProfile, AudioSsrc, CompressionType,
    ProfileCodec, RealtimeAudioRequest, SsrcTracker, StreamResponse, StreamRouting, StreamType,
    AAC_ELD_NO_DATA_MARKER, AP1_AUDIO_SAMPLE_RATE_HZ,
};
use proto_airplay::pairstore::{
    EndpointPolicy, PairStore, PairingEndpoint, PairingVerdict, MAX_DEVICE_ID_BYTES,
    MAX_DISPLAY_NAME_BYTES, MAX_PAIRED_DEVICES, PUBLIC_KEY_BYTES,
    STATUS_FLAG_ONE_TIME_PAIRING_REQUIRED,
};

fn key(byte: u8) -> [u8; PUBLIC_KEY_BYTES] {
    [byte; PUBLIC_KEY_BYTES]
}

// ---------------------------------------------------------------- AP1 实时音频（fx-airplay-setup-audio-ap1）

#[test]
fn ap1_compression_types_map_to_sourced_profiles() {
    // ct 四值表：1=PCM、2=ALAC、4=AAC-LC、8=AAC-ELD（来源 caps 注释）。
    let table = [
        (1u64, CompressionType::LinearPcm, None, true),
        (2, CompressionType::Alac, Some(352), false),
        (4, CompressionType::AacLc, Some(1024), false),
        (8, CompressionType::AacEld, Some(480), false),
    ];
    for (wire, expected, spf, decodable) in table {
        let ct = CompressionType::from_wire(wire).expect("登记过的 ct");
        assert_eq!(ct, expected);
        assert_eq!(ct.as_wire(), wire);
        assert_eq!(ct.samples_per_frame(), spf, "spf 为来源取值");
        assert_eq!(ct.decodable_here(), decodable, "只有 PCM 能在本仓直接路由");
    }
    // PCM 没有 spf：不得替它编一个（来源 caps 未给）。
    assert!(CompressionType::LinearPcm.samples_per_frame().is_none());
}

#[test]
fn ap1_realtime_request_requires_all_four_keys() {
    let full = [
        ("controlPort", 5002u64),
        ("ct", 2),
        ("spf", 352),
        ("audioFormat", 0x0004_0000),
    ];
    let req = RealtimeAudioRequest::validate(&full).expect("四键齐全");
    assert_eq!(req.control_port, 5002);
    assert_eq!(req.compression, CompressionType::Alac);
    assert_eq!(req.packed_audio_format, 0x0004_0000);
    assert!(req.spf_matches_source(), "352 与来源取值一致");
    assert!(!req.decodable_here(), "ALAC 需要 decoder");
    assert_eq!(AP1_AUDIO_SAMPLE_RATE_HZ, 44_100);

    // 逐项缺失都必须被拒（来源把四者都按必需字段读取）。
    for missing in ["controlPort", "ct", "spf", "audioFormat"] {
        let keys: Vec<(&str, u64)> = full
            .iter()
            .copied()
            .filter(|(k, _)| *k != missing)
            .collect();
        assert_eq!(
            RealtimeAudioRequest::validate(&keys).unwrap_err().code,
            ErrorCode::InvalidFrame,
            "缺 {missing} 必须被拒"
        );
    }

    // spf 与来源值不同不是错误，只是记录；controlPort 0 合法（来源：为 0 则不激活重传）。
    let spf_odd = [("controlPort", 0u64), ("ct", 8), ("spf", 480), ("audioFormat", 0)];
    let req = RealtimeAudioRequest::validate(&spf_odd).expect("合法");
    assert_eq!(req.control_port, 0);
    assert!(req.spf_matches_source(), "AAC-ELD 的 480 是来源默认值");
    let other = [("controlPort", 1u64), ("ct", 2), ("spf", 480), ("audioFormat", 0)];
    assert!(!RealtimeAudioRequest::validate(&other).unwrap().spf_matches_source());
}

#[test]
fn ap1_unknown_ct_and_bad_shapes_are_refused() {
    // 未知 ct 不得回退到默认 profile。
    for bad in [0u64, 3, 5, 16] {
        let keys = [("controlPort", 1u64), ("ct", bad), ("spf", 352), ("audioFormat", 0)];
        assert_eq!(
            RealtimeAudioRequest::validate(&keys).unwrap_err().code,
            ErrorCode::UnsupportedProfile,
            "ct={bad} 必须拒绝"
        );
    }
    // controlPort 超出 u16。
    let big = [("controlPort", 70_000u64), ("ct", 2), ("spf", 352), ("audioFormat", 0)];
    assert_eq!(
        RealtimeAudioRequest::validate(&big).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
}

#[test]
fn stream_response_shapes_match_the_fact_table() {
    // 96：dataPort/controlPort/type（顺序也照字段表）。
    let audio = StreamResponse::build(StreamType::RealtimeAudio, 6000, Some(6001)).expect("可构造");
    assert_eq!(
        audio.keys(),
        &[("dataPort", 6000), ("controlPort", 6001), ("type", 96)]
    );
    // 110：**没有** controlPort。
    let video = StreamResponse::build(StreamType::MirrorVideo, 7000, None).expect("可构造");
    assert_eq!(video.keys(), &[("dataPort", 7000), ("type", 110)]);
    assert!(!video.has_key("controlPort"), "110 的响应不得出现 controlPort");
    // 音频响应缺 controlPort → 拒绝（字段表要求三键）。
    assert_eq!(
        StreamResponse::build(StreamType::RealtimeAudio, 6000, None)
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        StreamType::MirrorVideo.response_keys(),
        &["dataPort", "type"],
        "响应键表来自字段表"
    );
}

// ---------------------------------------------------------------- 流类型（fx-airplay-stream-type-unsupported）

#[test]
fn stream_types_are_sourced_and_routing_is_honest() {
    let table = [
        (96u64, StreamType::RealtimeAudio),
        (103, StreamType::BufferedAudio),
        (110, StreamType::MirrorVideo),
        (120, StreamType::AppleMusicVideo),
        (130, StreamType::RemoteControlData),
    ];
    for (wire, expected) in table {
        assert_eq!(StreamType::from_wire(wire).expect("登记过的类型"), expected);
        assert_eq!(expected.as_wire(), wire);
    }
    assert_eq!(
        StreamType::from_wire(99).unwrap_err().code,
        ErrorCode::UnsupportedProfile,
        "未登记的流类型不得猜测"
    );
    assert_eq!(StreamType::RealtimeAudio.routing(), StreamRouting::Routable);
    assert_eq!(StreamType::MirrorVideo.routing(), StreamRouting::Routable);
    for unsupported in [
        StreamType::BufferedAudio,
        StreamType::AppleMusicVideo,
        StreamType::RemoteControlData,
    ] {
        match unsupported.routing() {
            StreamRouting::NotImplemented(reason) => assert!(!reason.is_empty()),
            other => panic!("{unsupported:?} 必须是 NotImplemented：{other:?}"),
        }
        // 不实现的流类型既不能构造响应，也要给 unsupported-feature。
        assert_eq!(
            StreamResponse::build(unsupported, 1, Some(2)).unwrap_err().code,
            ErrorCode::UnsupportedFeature
        );
    }
}

// ---------------------------------------------------------------- AP2 两套编号（fx-airplay-audio-format-packed / ssrc-magic）

#[test]
fn ap2_packed_audio_format_is_not_the_ssrc_space() {
    let table = [
        (0x0004_0000u64, 44_100u32, 16u16),
        (0x0008_0000, 44_100, 24),
        (0x0010_0000, 48_000, 16),
        (0x0020_0000, 48_000, 24),
    ];
    for (packed, rate, depth) in table {
        let profile = parse_packed_audio_format(packed).expect("登记过的打包值");
        assert_eq!(
            profile,
            AudioProfile {
                codec: ProfileCodec::Alac,
                sample_rate_hz: rate,
                bit_depth: depth,
                channels: 2,
            }
        );
    }
    // 未知值一律拒绝（含看似"合理"的邻近值）——不得回退到默认 profile。
    for bad in [0x0000_0000u64, 0x000C_0000, 0x0040_0000, 0x0004_0001] {
        assert_eq!(
            parse_packed_audio_format(bad).unwrap_err().code,
            ErrorCode::UnsupportedProfile,
            "0x{bad:08X} 必须拒绝"
        );
    }
    // 两套编号不得互相回退：SSRC 魔数不能当打包值解析，反之亦然。
    assert!(
        parse_packed_audio_format(0x0000_FACE).is_err(),
        "SSRC 魔数不是合法的打包 audioFormat"
    );
    assert_eq!(
        AudioSsrc::from_u32(0x0004_0000),
        AudioSsrc::None,
        "打包值不是合法的 SSRC 魔数"
    );
}

#[test]
fn ap2_ssrc_magic_values_and_change_detection() {
    let table = [
        (0x0000_FACEu32, AudioSsrc::Alac44100S16Stereo, 44_100u32, 16u16, 2u8),
        (0x1500_0000, AudioSsrc::Alac48000S24Stereo, 48_000, 24, 2),
        (0x1600_0000, AudioSsrc::Aac44100F24Stereo, 44_100, 24, 2),
        (0x1700_0000, AudioSsrc::Aac48000F24Stereo, 48_000, 24, 2),
        (0x2700_0000, AudioSsrc::Aac48000F24Surround51, 48_000, 24, 6),
        (0x2800_0000, AudioSsrc::Aac48000F24Surround71, 48_000, 24, 8),
    ];
    for (wire, expected, rate, depth, channels) in table {
        let ssrc = AudioSsrc::from_u32(wire);
        assert_eq!(ssrc, expected);
        assert_eq!(ssrc.as_u32(), wire);
        let profile = ssrc.profile().expect("有 profile");
        assert_eq!(
            (profile.sample_rate_hz, profile.bit_depth, profile.channels),
            (rate, depth, channels)
        );
    }
    // 0 与未知值都是 None（参考实现：不报错、也不当格式变化）。
    assert_eq!(AudioSsrc::from_u32(0), AudioSsrc::None);
    assert_eq!(AudioSsrc::from_u32(0x1234_5678), AudioSsrc::None);
    assert!(AudioSsrc::None.profile().is_none());

    // 从解密后的 RTP 头取 SSRC（packet[8:12]）。
    let mut packet = vec![0u8; 12];
    packet[8..12].copy_from_slice(&0x0000_FACEu32.to_be_bytes());
    assert_eq!(
        AudioSsrc::from_rtp_header(&packet),
        Some(AudioSsrc::Alac44100S16Stereo)
    );
    assert!(AudioSsrc::from_rtp_header(&packet[..8]).is_none(), "头不完整 → None");

    // 格式变化只在"非 None 且与上次不同"时触发。
    let mut tracker = SsrcTracker::new();
    assert!(tracker.observe(AudioSsrc::None).is_none(), "0 不触发");
    let first = tracker.observe(AudioSsrc::Alac44100S16Stereo).expect("首次非 0 触发");
    assert_eq!(first.codec, ProfileCodec::Alac);
    assert!(
        tracker.observe(AudioSsrc::Alac44100S16Stereo).is_none(),
        "同值不重复触发"
    );
    let switch = tracker.observe(AudioSsrc::Aac48000F24Stereo).expect("换格式触发");
    assert_eq!(switch.codec, ProfileCodec::Aac);
    assert_eq!(switch.sample_rate_hz, 48_000);
    assert!(tracker.observe(AudioSsrc::None).is_none(), "0 不重置格式");
    assert_eq!(tracker.current(), Some(AudioSsrc::Aac48000F24Stereo));
}

#[test]
fn aac_eld_no_data_marker_is_exact() {
    assert!(is_aac_eld_no_data(&AAC_ELD_NO_DATA_MARKER));
    assert!(!is_aac_eld_no_data(&[0x00, 0x68, 0x34]));
    assert!(!is_aac_eld_no_data(&[0x00, 0x68, 0x34, 0x01]));
    assert!(!is_aac_eld_no_data(&[]));
}

// ---------------------------------------------------------------- 配对登记层（fx-airplay-pairstore-*）

#[test]
fn pairstore_registers_queries_and_forgets() {
    let mut store = PairStore::new();
    assert!(!store.has_any_pairing());
    assert_eq!(store.verdict("dev-a"), PairingVerdict::Unknown);
    // 未配对 → 广告 OneTimePairingRequired（bit 9）。
    assert_eq!(store.status_flags(), STATUS_FLAG_ONE_TIME_PAIRING_REQUIRED);

    store
        .put("dev-a", key(0xAA), Some("客厅 iPad".to_string()), 1_000)
        .expect("可登记");
    assert!(store.has_any_pairing());
    assert_eq!(store.status_flags(), 0, "已有配对 → 不再广告 bit 9");
    assert_eq!(store.verdict("dev-a"), PairingVerdict::KnownButUnverified);
    let entry = store.get("dev-a").expect("可查询");
    assert_eq!(entry.controller_public_key, key(0xAA));
    assert_eq!(entry.added_at_ms, 1_000);
    assert_eq!(entry.last_seen_ms, 1_000);

    // touch 只改 last_seen，不改 added_at。
    assert!(store.touch("dev-a", 5_000));
    assert!(!store.touch("dev-zzz", 5_000));
    let entry = store.get("dev-a").expect("可查询");
    assert_eq!(
        (entry.added_at_ms, entry.last_seen_ms),
        (1_000, 5_000),
        "重新登记前 added_at 不变"
    );

    // 重新登记（重新配对）保留 added_at。
    store
        .put("dev-a", key(0xAB), None, 9_000)
        .expect("可覆盖");
    let entry = store.get("dev-a").expect("可查询");
    assert_eq!(entry.controller_public_key, key(0xAB));
    assert_eq!(entry.added_at_ms, 1_000);
    assert_eq!(store.len(), 1);

    // 遗忘（来源的注册表没有删除路径 → 本仓策略明确提供，并记在字段表）。
    assert!(store.remove("dev-a"));
    assert!(!store.remove("dev-a"));
    assert!(!store.has_any_pairing());
    assert_eq!(store.status_flags(), STATUS_FLAG_ONE_TIME_PAIRING_REQUIRED);
}

#[test]
fn pairstore_bounds_and_serialization() {
    let mut store = PairStore::with_capacity(2);
    store.put("a", key(1), None, 1).expect("第 1 条");
    store.put("b", key(2), None, 2).expect("第 2 条");
    // 满了明确拒绝（不静默淘汰）。
    assert_eq!(
        store.put("c", key(3), None, 3).unwrap_err().code,
        ErrorCode::ResourceLimit
    );
    // 同 id 覆盖不受容量限制。
    store.put("b", key(9), None, 4).expect("覆盖");
    // 空 id / 超长 id / 超长显示名。
    assert_eq!(
        store.put("", key(1), None, 5).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    let long_id = "x".repeat(MAX_DEVICE_ID_BYTES + 1);
    assert_eq!(
        store.put(&long_id, key(1), None, 5).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    let long_name = "名".repeat(MAX_DISPLAY_NAME_BYTES + 1);
    assert_eq!(
        store.put("d", key(1), Some(long_name), 5).unwrap_err().code,
        ErrorCode::ResourceLimit
    );

    // 身份种子：本层不生成，只保管宿主给的。
    assert!(store.identity_seed().is_none());
    store.set_identity_seed(key(0x55));
    assert_eq!(store.identity_seed(), Some(key(0x55)));

    // JSON 往返（宿主决定落盘；本层不做 I/O）。
    let json = store.to_json().expect("可序列化");
    let restored = PairStore::from_json(&json).expect("可反序列化");
    assert_eq!(restored.len(), store.len());
    assert_eq!(restored.get("b").expect("有 b").controller_public_key, key(9));
    assert_eq!(restored.identity_seed(), Some(key(0x55)));
    assert_eq!(restored.verdict("b"), PairingVerdict::KnownButUnverified);

    // 超上限的存档必须被拒（不静默截断）。
    let mut big = PairStore::with_capacity(1);
    for i in 0..(MAX_PAIRED_DEVICES + 1) {
        if big.put(&format!("d{i}"), key(1), None, 1).is_err() {
            break;
        }
    }
    let mut json_value: serde_json::Value =
        serde_json::from_str(&big.to_json().expect("可序列化")).expect("json");
    let devices = json_value
        .get_mut("devices")
        .and_then(|v| v.as_object_mut())
        .expect("devices");
    for i in 0..(MAX_PAIRED_DEVICES + 5) {
        devices.insert(
            format!("x{i}"),
            serde_json::json!({
                "device_id": format!("x{i}"),
                "controller_public_key": vec![7u8; PUBLIC_KEY_BYTES],
                "display_name": null,
                "added_at_ms": 1,
                "last_seen_ms": 1,
            }),
        );
    }
    assert_eq!(
        PairStore::from_json(&json_value.to_string())
            .unwrap_err()
            .code,
        ErrorCode::ResourceLimit
    );
}

#[test]
fn pairstore_never_claims_authentication() {
    let mut store = PairStore::new();
    store.put("dev-a", key(0xAA), None, 1).expect("可登记");
    // 枚举本身没有 Authenticated 变体；已知设备只能得到 known-but-unverified。
    let verdict = store.verdict("dev-a");
    assert_eq!(verdict, PairingVerdict::KnownButUnverified);
    assert_ne!(verdict, PairingVerdict::Unknown);
    // 没有任何 API 会因"登记过"而返回成功放行：可用的结论只有这两种。
    let known = store.get("dev-a").is_some();
    assert!(known, "登记事实存在……");
    // ……但登记不是认证：本仓不提供签名校验，keying seam 仍是唯一授权点。
    assert_eq!(
        proto_airplay::keying::KeyingProvider::name(&proto_airplay::keying::UnavailableKeying),
        "unavailable(vendor-gated)"
    );
    assert!(matches!(
        store.verdict("dev-a"),
        PairingVerdict::KnownButUnverified
    ));
}

#[test]
fn pairing_endpoints_have_honest_policies() {
    assert_eq!(PairingEndpoint::all().len(), 5);
    for endpoint in PairingEndpoint::all() {
        assert_eq!(PairingEndpoint::from_path(endpoint.path()), Some(*endpoint));
    }
    assert!(PairingEndpoint::from_path("/pair-add").is_none(), "未登记路径 → None");

    // /fp-setup：永久 vendor-gated，且不解析载荷。
    assert_eq!(
        PairingEndpoint::FpSetup.policy(),
        EndpointPolicy::VendorGated {
            reason: "FairPlay/设备认证材料永久 vendor-gated：不实现、不解析载荷、不绕过"
        }
    );
    assert_eq!(
        PairingEndpoint::FpSetup.check_request(&["data"]).unwrap_err().code,
        ErrorCode::VendorGated
    );

    // 其余四个：形状校验后明确 unsupported-feature（本仓没有 SRP/X25519/Ed25519）。
    for endpoint in [
        PairingEndpoint::PairPinStart,
        PairingEndpoint::PairSetupPin,
        PairingEndpoint::PairSetup,
        PairingEndpoint::PairVerify,
    ] {
        assert!(matches!(
            endpoint.policy(),
            EndpointPolicy::ShapeCheckOnly { .. }
        ));
        assert_eq!(
            endpoint.check_request(&[]).unwrap_err().code,
            ErrorCode::UnsupportedFeature,
            "{endpoint:?} 必须明确不实现"
        );
    }
    // 字段表未登记的键 → invalid-frame（先于"不实现"的判断）。
    assert_eq!(
        PairingEndpoint::PairSetupPin
            .check_request(&["srp_a"])
            .unwrap_err()
            .code,
        ErrorCode::InvalidFrame
    );
    // 登记过的键 → 走到"不实现"这一层。
    assert_eq!(
        PairingEndpoint::PairSetupPin
            .check_request(&["method", "user"])
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedFeature
    );
}
