//! `interop-hardening`：不可信输入的对抗性扫描（只放测试，无运行时产物）。
//!
//! 语料生成器在 [`interop_testkit::adversarial`]；本 crate 只负责把这些解码器**凑到一个测试
//! 二进制里**，并持有计数分配器（`tests/allocation_budget.rs`）。
//!
//! 边界（不许越界声明）：这里证明的是"任意字节不 panic、消费记账不越界、分配不被声明长度
//! 牵着走"，**不是**"协议实现正确"，更不是任何真机/互操作证据。

#![forbid(unsafe_code)]

/// 扫描目标的标签清单（与 `tests/decode_campaign.rs` 的用例一一对应）。
///
/// 存在的意义是让 lab gate 能机器检查"每个吃不可信字节的解码入口都有扫描用例"：
/// gate 解析本数组 + 各 crate 的 `pub fn decode*/parse*` 列表做差集。
pub const SCAN_TARGETS: &[&str] = &[
    "quickshare/wire/decode_ukey2_message",
    "quickshare/wire/decode_client_init",
    "quickshare/wire/decode_server_init",
    "quickshare/wire/decode_client_finished",
    "quickshare/wire/decode_generic_public_key",
    "quickshare/wire/decode_secure_message",
    "quickshare/wire/decode_header_and_body",
    "quickshare/wire/decode_header",
    "quickshare/wire/decode_device_to_device",
    "quickshare/control/keepalive_frame_decode",
    "quickshare/control/decode_keepalive_offline",
    "quickshare/control/decode_payload_ack",
    "quickshare/control/paired_key_encryption/decode_inner",
    "quickshare/control/paired_key_result/decode_inner",
    "quickshare/control/disconnection/decode_offline",
    "quickshare/payload/decode_payload_transfer_frame",
    "quickshare/framing/push",
    "airplay/rtsp/parse",
    "airplay/audio_control/check_shape",
    "airplay/pairstore/check_request",
    "upnp/xml/parse_document",
    "upnp/ssdp/parse",
    "upnp/soap/parse_raw",
    "upnp/dmc/description_fetch_policy",
    "upnp/soap/action_from_wire",
    "upnp/dms/browse_flag_from_wire",
    "cast/castv2/decode_frame",
    "cast/castv2/decode_body",
    "cast/namespaces/decode_message",
    "cast/namespaces/decode",
    "cast/discovery/from_txt",
    "cast/receiver/from_txt",
    "wfd/messages/request_parse",
    "wfd/messages/response_parse",
    "wfd/messages/parse_parameter_names",
    "wfd/messages/parse_parameter_body",
    "wfd/ie/subelement_parse",
    "wfd/ie/container_parse",
    "wfd/rtp/parse_header",
    "wfd/negotiate/video_formats",
    "wfd/negotiate/audio_codecs",
    "wfd/negotiate/rtp_ports",
    "wfd/negotiate/transport",
    "wfd/negotiate/content_protection",
    "ipc/media_frame/decode",
    "ipc/frame/read_frame",
    "contract/capability/api_version",
    "contract/media/from_wire",
];
