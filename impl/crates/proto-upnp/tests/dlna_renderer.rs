//! T42-01..04 验收测试（plans/03-casting.md §T42；字段行见 `specs-reviewed/m07-dlna-upnp-av.md` F-15..F-22）。
//!
//! - T42-01 `SetAVTransportURI` 为 `file://` → 拒绝（且不留播放入口）；
//! - T42-02 重复 `Stop` → 幂等结束；
//! - T42-03 live 流 `Seek` 不支持 → 710；
//! - T42-04 外来 event 订阅 callback 越界 → 拒绝。
//!
//! 外加：AVTransport/RenderingControl 动作经 SOAP 层的往返、状态变量取值、以及"渲染器控制路径
//! 不做网络 I/O"的纪律（后者由 `tools/test_dlna_gate.py` 机器保证）。全部字节与地址自制。

use proto_upnp::dmc::DescriptionFetchPolicy;
use proto_upnp::dmr::{
    Dmr, DmrFault, TransportState, TransportStatus, UriDecision, ERR_CANT_PROCESS,
    ERR_ILLEGAL_SEEK_TARGET, ERR_INVALID_ARGUMENT, ERR_PLAY_SPEED_NOT_SUPPORTED,
    ERR_SEEK_MODE_NOT_SUPPORTED, ERR_TRANSITION_NOT_AVAILABLE, MAX_SUBSCRIPTION_TIMEOUT_SECS,
    SUPPORTED_PLAY_SPEED, SUPPORTED_SEEK_MODE,
};
use proto_upnp::soap::{
    SoapMessage, AV_TRANSPORT, AV_TRANSPORT_V2, CHANNEL_MASTER, RENDERING_CONTROL,
};

const GOOD_URI: &str = "http://192.0.2.40:8000/media/clip.mp4";
const CALLBACK: &str = "http://192.0.2.50:9999/event";

fn renderer() -> Dmr {
    Dmr::new(DescriptionFetchPolicy::default())
}

// ------------------------------------------------------------------ T42-01

#[test]
fn t42_01_file_scheme_is_refused_without_playback_entry() {
    let mut dmr = renderer();
    let fault = dmr
        .set_uri("file:///etc/passwd", None)
        .expect_err("file:// 必须拒绝（T42-01）");
    assert_eq!(fault.code, ERR_INVALID_ARGUMENT);
    assert!(fault.description.contains("T42-01"), "{fault:?}");
    assert_eq!(dmr.state(), TransportState::NoMediaPresent, "拒绝后不得留下播放入口");
    assert_eq!(dmr.current_uri(), None);
    assert!(dmr.media().is_none());
    // 没有媒体时 Play 也不可用。
    assert_eq!(dmr.play(SUPPORTED_PLAY_SPEED).unwrap_err().code, ERR_TRANSITION_NOT_AVAILABLE);

    // 其它被拒形态：内网/元数据/带 userinfo、非 http scheme。
    for bad in [
        "http://127.0.0.1:8000/a.mp4",
        "http://169.254.169.254/latest/meta-data/",
        "http://user:pass@192.0.2.40/a.mp4",
        "ftp://192.0.2.40/a.mp4",
        "rtsp://192.0.2.40/a.mp4",
    ] {
        let fault = dmr.set_uri(bad, None).expect_err("必须拒绝");
        assert_eq!(fault.code, ERR_INVALID_ARGUMENT, "{bad}");
        assert_eq!(dmr.state(), TransportState::NoMediaPresent, "{bad}");
    }

    // 合法 URI：先要用户同意（T42 实现决策），同意后才可播放。
    let decision = dmr.set_uri(GOOD_URI, None).expect("合法 URI 可接受");
    assert_eq!(decision, UriDecision::NeedsUserConsent);
    assert_eq!(dmr.state(), TransportState::Stopped, "接受后进入 STOPPED");
    assert_eq!(dmr.current_uri(), Some(GOOD_URI));
    let fault = dmr.play(SUPPORTED_PLAY_SPEED).expect_err("未授权时不得播放");
    assert_eq!(fault.code, ERR_TRANSITION_NOT_AVAILABLE);
    dmr.grant_consent(GOOD_URI).expect("可授权");
    dmr.play(SUPPORTED_PLAY_SPEED).expect("授权后可播放");
    assert_eq!(dmr.state(), TransportState::Playing);

    // 重新设置同一 URI 不再要求同意。
    assert_eq!(dmr.set_uri(GOOD_URI, None).expect("可接受"), UriDecision::Accepted);
}

// ------------------------------------------------------------------ T42-02

#[test]
fn t42_02_stop_is_idempotent() {
    let mut dmr = renderer();
    // NO_MEDIA_PRESENT 下 Stop：成功且状态不变。
    dmr.stop().expect("空状态下 Stop 也要成功（T42-02）");
    assert_eq!(dmr.state(), TransportState::NoMediaPresent);

    dmr.set_uri(GOOD_URI, None).expect("可接受");
    dmr.grant_consent(GOOD_URI).expect("可授权");
    assert_eq!(dmr.state(), TransportState::Stopped);
    // STOPPED 下重复 Stop。
    dmr.stop().expect("重复 Stop 幂等");
    dmr.stop().expect("再重复 Stop 幂等");
    assert_eq!(dmr.state(), TransportState::Stopped);

    // 播放中 Stop → STOPPED，位置归零；随后再 Stop 仍成功。
    dmr.play(SUPPORTED_PLAY_SPEED).expect("可播放");
    dmr.on_playback_position(42);
    assert_eq!(dmr.position_secs(), 42);
    dmr.stop().expect("停止");
    assert_eq!(dmr.state(), TransportState::Stopped);
    assert_eq!(dmr.position_secs(), 0, "停止要清位置");
    dmr.stop().expect("停止后的 Stop 幂等");
    assert_eq!(dmr.state(), TransportState::Stopped);
}

// ------------------------------------------------------------------ T42-03

#[test]
fn t42_03_live_seek_is_explicitly_unsupported() {
    let mut dmr = renderer();
    dmr.set_uri(GOOD_URI, None).expect("可接受");
    dmr.grant_consent(GOOD_URI).expect("可授权");
    dmr.play(SUPPORTED_PLAY_SPEED).expect("可播放");

    // 时长未知 = 直播流 → 明确 710，不改位置。
    let fault = dmr.seek(SUPPORTED_SEEK_MODE, "0:01:30").expect_err("直播流不得定位");
    assert_eq!(fault.code, ERR_SEEK_MODE_NOT_SUPPORTED, "T42-03：710");
    assert!(fault.description.contains("直播流"), "{fault:?}");
    assert_eq!(dmr.position_secs(), 0, "被拒的 Seek 不得改位置");
    assert_eq!(dmr.state(), TransportState::Playing);

    // 点播（已知时长）：合法定位生效。
    dmr.set_media_duration(Some(600));
    dmr.seek(SUPPORTED_SEEK_MODE, "0:01:30").expect("点播可定位");
    assert_eq!(dmr.position_secs(), 90);
    // `AbsTime`/`TrackDuration` 用 H:MM:SS（F-19）。
    let reply = dmr.dispatch(&SoapMessage::request(AV_TRANSPORT, "GetPositionInfo", &[("InstanceID", "0")]).unwrap());
    assert!(reply.is_ok());
    assert_eq!(reply.args.get("RelTime").map(String::as_str), Some("0:01:30"));
    assert_eq!(reply.args.get("TrackDuration").map(String::as_str), Some("0:10:00"));

    // 未实现的单位 → 710；非法目标 → 711；超时长 → 711。
    let fault = dmr.seek("ABS_TIME", "0:00:10").expect_err("未实现单位");
    assert_eq!(fault.code, ERR_SEEK_MODE_NOT_SUPPORTED);
    let fault = dmr.seek(SUPPORTED_SEEK_MODE, "ninety seconds").expect_err("目标非法");
    assert_eq!(fault.code, ERR_ILLEGAL_SEEK_TARGET);
    let fault = dmr.seek(SUPPORTED_SEEK_MODE, "0:20:00").expect_err("超出时长");
    assert_eq!(fault.code, ERR_ILLEGAL_SEEK_TARGET);
    assert_eq!(dmr.position_secs(), 90, "被拒的 Seek 不改位置");

    // Pause 后仍可定位（PAUSED_PLAYBACK 允许 Seek，F-19 的 CurrentTransportActions）。
    dmr.pause().expect("可暂停");
    assert_eq!(dmr.state(), TransportState::PausedPlayback);
    assert!(dmr.current_transport_actions().contains(&"Seek"));
    dmr.seek(SUPPORTED_SEEK_MODE, "0:00:30").expect("暂停态可定位");
    assert_eq!(dmr.position_secs(), 30);
    // 无媒体时 Seek → 701。
    let mut empty = renderer();
    assert_eq!(
        empty.seek(SUPPORTED_SEEK_MODE, "0:00:01").unwrap_err().code,
        ERR_TRANSITION_NOT_AVAILABLE
    );
}

// ------------------------------------------------------------------ T42-04

#[test]
fn t42_04_out_of_range_callback_is_refused() {
    let mut dmr = renderer();
    // 越界回调：本地路径、loopback、链路本地/元数据、多地址形式。
    for bad in [
        "file:///tmp/events",
        "http://127.0.0.1:9999/event",
        "http://169.254.169.254/event",
        "http://user:pw@192.0.2.50/event",
        "<http://192.0.2.50/event><http://192.0.2.51/event>",
    ] {
        let fault = dmr.subscribe(bad, 300, None).expect_err("越界回调必须拒绝（T42-04）");
        assert_eq!(fault.code, ERR_INVALID_ARGUMENT, "{bad}");
        assert!(fault.description.contains("T42-04"), "{fault:?}");
        assert!(dmr.subscriptions().is_empty(), "拒绝时不得建立订阅：{bad}");
    }
    // 超时越界（本仓策略上限）。
    assert_eq!(
        dmr.subscribe(CALLBACK, MAX_SUBSCRIPTION_TIMEOUT_SECS + 1, None)
            .unwrap_err()
            .code,
        ERR_INVALID_ARGUMENT
    );
    assert_eq!(dmr.subscribe(CALLBACK, 0, None).unwrap_err().code, ERR_INVALID_ARGUMENT);

    // 合法订阅：建立、续订、退订。
    let subscription = dmr.subscribe(CALLBACK, 300, None).expect("合法订阅");
    assert!(subscription.sid.starts_with("uuid:xross-gena-"));
    assert_eq!(dmr.subscriptions().len(), 1);
    let renewed = dmr.subscribe(CALLBACK, 600, Some(&subscription.sid)).expect("续订");
    assert_eq!(renewed.timeout_secs, 600);
    assert_eq!(dmr.subscriptions().len(), 1, "续订不新增订阅");
    assert_eq!(
        dmr.unsubscribe("uuid:xross-gena-deadbeef").unwrap_err().code,
        ERR_INVALID_ARGUMENT,
        "未知 SID 不得静默成功"
    );
    dmr.unsubscribe(&subscription.sid).expect("退订");
    assert!(dmr.subscriptions().is_empty());

    // 订阅数上限：明确拒绝，不上抛无界列表（F-10 只说有上限）。
    let mut many = renderer();
    for index in 0..proto_upnp::dmr::DEFAULT_MAX_SUBSCRIPTIONS {
        many.subscribe(&format!("http://192.0.2.{index}:9999/event"), 300, None)
            .expect("在上限内");
    }
    let fault = many
        .subscribe("http://192.0.2.200:9999/event", 300, None)
        .expect_err("超上限必须拒绝");
    assert_eq!(fault.code, ERR_CANT_PROCESS);
    assert_eq!(many.subscriptions().len(), proto_upnp::dmr::DEFAULT_MAX_SUBSCRIPTIONS);
}

// ------------------------------------------------------------------ SOAP 层往返

#[test]
fn soap_dispatch_roundtrips_and_faults() {
    let mut dmr = renderer();

    // SetAVTransportURI（AVTransport:1 与 :2 两个已登记版本串都可）
    for service in [AV_TRANSPORT, AV_TRANSPORT_V2] {
        let request = SoapMessage::request(
            service,
            "SetAVTransportURI",
            &[
                ("InstanceID", "0"),
                ("CurrentURI", GOOD_URI),
                ("CurrentURIMetaData", ""),
            ],
        )
        .expect("构造");
        let replayed = SoapMessage::parse(&request.encode()).expect("回读");
        let reply = dmr.dispatch(&replayed);
        assert!(reply.is_ok(), "{service} {reply:?}");
    }
    dmr.grant_consent(GOOD_URI).expect("授权");
    let play = SoapMessage::request(AV_TRANSPORT, "Play", &[("InstanceID", "0"), ("Speed", "1")]).unwrap();
    assert!(dmr.dispatch(&SoapMessage::parse(&play.encode()).unwrap()).is_ok());
    assert_eq!(dmr.state(), TransportState::Playing);

    // GetTransportInfo 的状态变量取值来自 F-15。
    let info = dmr.dispatch(&SoapMessage::request(AV_TRANSPORT, "GetTransportInfo", &[("InstanceID", "0")]).unwrap());
    assert_eq!(info.args.get("CurrentTransportState").map(String::as_str), Some("PLAYING"));
    assert_eq!(info.args.get("CurrentTransportStatus").map(String::as_str), Some(TransportStatus::Ok.as_str()));
    assert_eq!(info.args.get("CurrentSpeed").map(String::as_str), Some("1"));

    // RenderingControl：音量范围（F-18）与 Channel=Master。
    let set_volume = SoapMessage::request(
        RENDERING_CONTROL,
        "SetVolume",
        &[("InstanceID", "0"), ("Channel", CHANNEL_MASTER), ("DesiredVolume", "80")],
    )
    .unwrap();
    assert!(dmr.dispatch(&SoapMessage::parse(&set_volume.encode()).unwrap()).is_ok());
    assert_eq!(dmr.volume(), 80);
    let over = SoapMessage::request(
        RENDERING_CONTROL,
        "SetVolume",
        &[("InstanceID", "0"), ("Channel", CHANNEL_MASTER), ("DesiredVolume", "101")],
    )
    .unwrap();
    let reply = dmr.dispatch(&over);
    assert_eq!(reply.fault.as_ref().map(|f| f.code), Some(ERR_INVALID_ARGUMENT));
    assert_eq!(dmr.volume(), 80, "越界音量不得生效");

    // Channel 只认 Master（本仓策略）。
    assert!(SoapMessage::request(
        RENDERING_CONTROL,
        "SetVolume",
        &[("InstanceID", "0"), ("Channel", "LF"), ("DesiredVolume", "10")],
    )
    .is_err());

    // 服务类型与动作不匹配：Play 发到 RenderingControl。
    let wrong = SoapMessage::request(RENDERING_CONTROL, "Play", &[("InstanceID", "0")]);
    assert!(wrong.is_err(), "动作与服务类型必须匹配");

    // Fault 编码/解析往返（错误码透传给控制器）。
    let fault = DmrFault {
        code: ERR_SEEK_MODE_NOT_SUPPORTED,
        description: "直播流不支持定位".into(),
    };
    let bytes = fault.to_soap().encode();
    let parsed = SoapMessage::parse(&bytes).expect("Fault 可回读");
    assert_eq!(parsed.fault.as_ref().map(|(code, _)| *code), Some(ERR_SEEK_MODE_NOT_SUPPORTED));
    assert!(parsed.fault.as_ref().unwrap().1.contains("直播流"));

    // 播放速度只认 1（F-17）。
    let slow = SoapMessage::request(AV_TRANSPORT, "Play", &[("InstanceID", "0"), ("Speed", "0.5")]).unwrap();
    let reply = dmr.dispatch(&slow);
    assert_eq!(reply.fault.as_ref().map(|f| f.code), Some(ERR_PLAY_SPEED_NOT_SUPPORTED));

    // 播放器反馈：结束 → STOPPED（F-15）。
    dmr.on_playback_ended();
    assert_eq!(dmr.state(), TransportState::Stopped);
}
