//! T32 验收（plans/03-casting.md §T32）：AirPlay legacy receiver 的控制链核心。
//!
//! - T32-01 缺失/重复 body 长度 → 明确拒绝（RTSP framing 严格）；
//! - T32-02 未完成认证请求 media → 不开始流（且认证前不分配视频缓冲）；
//! - T32-03 广告 HEVC 但没有 decoder/接收实现 → 构建/能力测试失败；
//! - T32-04 快速连接断开 100 次 → session/port 资源全部回收。
//!
//! 消息 payload 全部自制（字段出自 `specs-reviewed/m01` 的字段级事实表）；
//! **FairPlay 相关不实现**：`UnavailableKeying` 是默认，能力随之缺席（keying seam）。

use interop_contract::error::ErrorCode;
use proto_airplay::capability::{
    advertised_features, assert_advertised_matches_implementation, AdvertisedFeature,
    ImplementationInventory,
};
use proto_airplay::discovery::{mdns_service_types, ServiceAdvertisement};
use proto_airplay::keying::{FakeKeying, UnavailableKeying};
use proto_airplay::rtsp::{RtspRequest, RtspStatus};
use proto_airplay::session::{AirPlayReceiver, SessionState};

fn request(method: &str, uri: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut out = format!("{method} {uri} RTSP/1.0\r\n").into_bytes();
    for (k, v) in headers {
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    out
}

/// T32-01：缺失/重复 body 长度 → 明确拒绝。
#[test]
fn t32_01_missing_or_duplicate_body_length_is_rejected() {
    // POST 没有 Content-Length：body 长度未知 → 拒绝（不猜、不读到 EOF）
    let no_length = request("POST", "/pair-setup-pin", &[("CSeq", "1")], b"plist");
    assert_eq!(
        RtspRequest::parse(&no_length).expect_err("缺 body 长度必须拒绝").code,
        ErrorCode::InvalidFrame
    );

    // 重复 Content-Length：走私风险 → 拒绝
    let duplicate = request(
        "POST",
        "/pair-setup-pin",
        &[("CSeq", "1"), ("Content-Length", "5"), ("Content-Length", "6")],
        b"plist",
    );
    assert_eq!(
        RtspRequest::parse(&duplicate).expect_err("重复长度必须拒绝").code,
        ErrorCode::InvalidFrame
    );

    // 非法/超限长度：非法 → invalid-frame；超限 → 分配前 resource-limit
    let bad = request("POST", "/pair-setup-pin", &[("Content-Length", "abc")], b"");
    assert_eq!(RtspRequest::parse(&bad).expect_err("非法长度").code, ErrorCode::InvalidFrame);
    let huge = request("POST", "/pair-setup-pin", &[("Content-Length", "999999")], b"");
    assert_eq!(
        RtspRequest::parse(&huge).expect_err("超限长度").code,
        ErrorCode::ResourceLimit,
        "长度检查先于分配"
    );

    // body 截断（声明 10 字节只有 3 字节）
    let truncated = request("POST", "/pair-setup-pin", &[("Content-Length", "10")], b"abc");
    assert_eq!(RtspRequest::parse(&truncated).expect_err("截断").code, ErrorCode::InvalidFrame);

    // 合法 POST + 合法 body 往返
    let ok = request("POST", "/pair-setup-pin", &[("CSeq", "7"), ("Content-Length", "5")], b"plist");
    let (parsed, used) = RtspRequest::parse(&ok).expect("合法请求");
    assert_eq!(used, ok.len());
    assert_eq!(parsed.method, "POST");
    assert_eq!(parsed.uri, "/pair-setup-pin");
    assert_eq!(parsed.body, b"plist");
    assert_eq!(parsed.cseq(), Some(7));

    // GET /info 无 body：允许（没有长度字段也不拒绝）
    let info = request("GET", "/info", &[("CSeq", "1")], b"");
    let (parsed, _) = RtspRequest::parse(&info).expect("GET 无 body 合法");
    assert!(parsed.body.is_empty());
}

/// T32-02：未认证请求 media → 不开始流；认证前不分配视频缓冲。
#[test]
fn t32_02_media_before_auth_does_not_start_stream() {
    let mut rx = AirPlayReceiver::new(4, Box::new(FakeKeying::deterministic()));
    let sid = rx.accept_connection(1_000).expect("连接");
    assert_eq!(rx.state(&sid), Some(SessionState::Unauthenticated));

    let (setup, _) = RtspRequest::parse(&request(
        "SETUP",
        "rtsp://192.168.1.10/stream",
        &[("CSeq", "2"), ("Content-Length", "0")],
        b"",
    ))
    .expect("请求形状合法");
    let resp = rx.handle(&sid, &setup, 1_100).expect("handle");
    assert_eq!(resp.status, RtspStatus::Unauthorized, "未认证的 SETUP 必须 401");
    assert!(!rx.has_stream(&sid), "不得开始流");
    assert_eq!(rx.allocated_video_bytes(&sid), 0, "认证前不得分配视频缓冲");
    assert_eq!(rx.state(&sid), Some(SessionState::Unauthenticated), "状态不得推进");

    // 走配对（fake 密钥）后 SETUP 才允许
    let (pair, _) = RtspRequest::parse(&request(
        "POST",
        "/pair-setup-pin",
        &[("CSeq", "3"), ("Content-Length", "5")],
        b"plist",
    ))
    .expect("请求形状合法");
    let resp = rx.handle(&sid, &pair, 1_200).expect("handle");
    assert_eq!(resp.status, RtspStatus::Ok);
    assert_eq!(rx.state(&sid), Some(SessionState::Authenticated));

    let resp = rx.handle(&sid, &setup, 1_300).expect("handle");
    assert_eq!(resp.status, RtspStatus::Ok, "认证后 SETUP 成功");
    assert!(rx.has_stream(&sid), "此时才建流");
    assert!(rx.allocated_video_bytes(&sid) > 0, "建流后才分配缓冲");

    // 没有 keying 提供者（FairPlay 位置留空）→ 认证成功也不得建流
    let mut bare = AirPlayReceiver::new(4, Box::new(UnavailableKeying));
    let sid2 = bare.accept_connection(2_000).expect("连接");
    bare.handle(&sid2, &pair, 2_100).expect("配对路径可达");
    let resp = bare.handle(&sid2, &setup, 2_200).expect("handle");
    assert_eq!(resp.status, RtspStatus::ServiceUnavailable, "密钥不可得 → 不建流");
    assert!(!bare.has_stream(&sid2));
    assert_eq!(bare.allocated_video_bytes(&sid2), 0);
}

/// T32-03：广告 HEVC 但没有 decoder/接收实现 → 构建/能力测试失败。
#[test]
fn t32_03_advertising_unimplemented_codecs_fails_capability_check() {
    let current = ImplementationInventory::current();
    assert!(!current.hevc_decode, "本仓尚未实现 HEVC 解码（T33 之前）");
    let advertised = advertised_features(&current);
    assert!(
        !advertised.contains(&AdvertisedFeature::VideoReceiverHevc),
        "没有 HEVC 实现就不得广告 HEVC：{advertised:?}"
    );
    assert!(
        !advertised.contains(&AdvertisedFeature::VideoReceiverH264),
        "视频接收链路（T33）未实现前不得广告"
    );
    assert_advertised_matches_implementation(&advertised, &current).expect("当前清单自洽");

    // 人为把 HEVC 塞进广告（模拟"照抄别人的位图"）→ 能力一致性检查必须失败
    let mut lying = advertised.clone();
    lying.insert(AdvertisedFeature::VideoReceiverHevc);
    let err = assert_advertised_matches_implementation(&lying, &current)
        .expect_err("广告了未实现能力必须失败");
    assert_eq!(err.code, ErrorCode::UnsupportedFeature);

    // discovery 侧同样只广告已实现能力
    let adv = ServiceAdvertisement::for_receiver(&current, 5001, "Xross Interop (MacBook)");
    assert!(!adv.features.contains(&AdvertisedFeature::VideoReceiverHevc));
    let types = mdns_service_types();
    assert_eq!(types, vec!["_airplay._tcp", "_raop._tcp"], "两种服务同时广播（字段表）");
}

/// T32-04：快速连接断开 100 次 → session/port 资源回收。
#[test]
fn t32_04_hundred_fast_disconnects_recycle_resources() {
    let mut rx = AirPlayReceiver::new(8, Box::new(FakeKeying::deterministic()));
    for i in 0..100u64 {
        let sid = rx.accept_connection(i * 10).expect("连接");
        // 一半连接连 /info 都发了就断，一半直接断
        if i % 2 == 0 {
            let (info, _) =
                RtspRequest::parse(&request("GET", "/info", &[("CSeq", "1")], b"")).expect("info");
            rx.handle(&sid, &info, i * 10 + 1).expect("info");
        }
        rx.disconnect(&sid, i * 10 + 2);
    }
    assert_eq!(rx.active_sessions(), 0, "100 次快速断开后不得残留 session");
    assert_eq!(rx.reserved_ports(), 0, "端口/资源必须回收");
    assert_eq!(rx.tracked_sessions(), 0, "注册表不得泄漏条目");

    // 建流后的 TEARDOWN 同样回收端口
    let sid = rx.accept_connection(10_000).expect("连接");
    let (pair, _) = RtspRequest::parse(&request(
        "POST",
        "/pair-setup-pin",
        &[("CSeq", "1"), ("Content-Length", "5")],
        b"plist",
    ))
    .expect("pair");
    rx.handle(&sid, &pair, 10_001).expect("handle");
    let (setup, _) = RtspRequest::parse(&request(
        "SETUP",
        "rtsp://192.168.1.10/stream",
        &[("CSeq", "2"), ("Content-Length", "0")],
        b"",
    ))
    .expect("setup");
    rx.handle(&sid, &setup, 10_002).expect("handle");
    assert_eq!(rx.reserved_ports(), 1, "建流占用一个流端口");
    let (teardown, _) = RtspRequest::parse(&request(
        "TEARDOWN",
        "rtsp://192.168.1.10/stream",
        &[("CSeq", "3"), ("Content-Length", "0")],
        b"",
    ))
    .expect("teardown");
    rx.handle(&sid, &teardown, 10_003).expect("handle");
    assert_eq!(rx.reserved_ports(), 0, "TEARDOWN 后回收");
    assert_eq!(rx.state(&sid), Some(SessionState::Ended));

    // 上限：明确 busy（不静默超发），且 TEARDOWN 会释放名额
    let mut one = AirPlayReceiver::new(1, Box::new(FakeKeying::deterministic()));
    let first = one.accept_connection(20_000).expect("第一条");
    assert_eq!(
        one.accept_connection(20_001).expect_err("上限后必须 busy").code,
        ErrorCode::Busy
    );
    let (td, _) = RtspRequest::parse(&request(
        "TEARDOWN",
        "rtsp://192.168.1.10/stream",
        &[("CSeq", "1"), ("Content-Length", "0")],
        b"",
    ))
    .expect("teardown");
    one.handle(&first, &td, 20_002).expect("handle");
    one.accept_connection(20_003).expect("TEARDOWN 释放名额后可再接受连接");
}
