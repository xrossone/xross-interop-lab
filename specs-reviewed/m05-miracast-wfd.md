# Reviewed Wire Spec — wfd.miracast（M05）

**状态**：字段级事实表已建立（2026-09-15，T37 gate）。**P-M05-2（WFD IE 容器与 M0–M16 实测序列）与
P-M05-3（真实设备 codec 矩阵）blocked**：前者需要隔离 Linux 机跑 sink 抓序列，后者需要库存 TV/手机。
**P-M05-1（Windows 平台入口）**未 probe（本机 macOS，无公开 WFD API）。

**红线**：状态 `blocked`/`待固化` 的行不得写进实现；本文件不复制第三方表达，只记字段名/常量/结构事实。
**特别地：WFD IE 的 OUI 与 OUI type 字节在四个来源里都不存在（F-24），因此在 P-M05-2 关闭前，
本仓实现不得构造完整 WFD IE**——只能处理子元素（subelement）字节。

## A. 字段级事实表

| # | 层 | 事实 | 来源（commit+位置） | 状态 | impl-allowed |
|---|---|---|---|---|---|
| F-01 | control | 控制面是 RTSP，默认控制端口 **7236** | R32 0b7f1f1f `src/ctl/sinkctl.c:73`（`DEFAULT_RSTP_PORT = 7236`）、`src/ctl/ctl-sink.c:588`（`htons(7236)`）；R37 13ce3d3e `services/common/const_def.h:89`（`DEFAULT_WFD_CTRLPORT = 7236`）；R34 214d77ae `native/wifi-display/source/WifiDisplaySource.h:38` | source-reviewed | yes |
| F-02 | control | M1：`OPTIONS * RTSP/1.0` + `Require: org.wfa.wfd1.0`；应答 `Public: org.wfa.wfd1.0, <方法列表>` | R32 `src/ctl/ctl-sink.c:61-67,42-44`；R34 `WifiDisplaySource.cpp:451-455,961-962`；R37 `services/impl/wfd/wfd_message.cpp:32-36`、`wfd_message.h:51-53` | source-reviewed | yes |
| F-03 | control | M2：sink→source 的 `OPTIONS`（source 侧作为 RTSP server 接收并应答） | R37 `services/source/impl/wfd/wfd_source/wfd_source_session.cpp:433`（`// M2`）、`services/sink/impl/wfd/wfd_sink/wfd_sink_session.h:80`（`SendM2Request(); // M2/OPTIONS`）；R34 `sink/WifiDisplaySink.cpp:245-249` | source-reviewed | yes |
| F-04 | control | M3：`GET_PARAMETER`，询问对端能力；必需参数 `wfd_client_rtp_ports`/`wfd_audio_codecs`/`wfd_video_formats`，另含 `wfd_content_protection`/`wfd_uibc_capability` 等 | R37 `services/impl/wfd/wfd_message.h:82-87`；R33 521caad `src/wfd/wfd-client.c:517`、`src/wfd/wfd-params.c:6-8`（必需）与 `:12-26`（可选）；R34 `WifiDisplaySource.cpp:475-478,480` | source-reviewed | yes |
| F-05 | control | M4：`SET_PARAMETER`，下发选定参数（`wfd_video_formats`/`wfd_audio_codecs`/`wfd_presentation_URL`/`wfd_client_rtp_ports`） | R33 `src/wfd/wfd-client.c:251,257-265`；R34 `WifiDisplaySource.cpp:540-554`；R32 `src/ctl/ctl-sink.c:261`（`/* M4 (or any other) can pass presentation URLs */`）、`:318`（`/* M4 again */`） | source-reviewed | yes |
| F-06 | control | M5：`SET_PARAMETER` + `wfd_trigger_method: SETUP`（触发对端发 SETUP） | R34 `WifiDisplaySource.cpp:578`；R33 `src/wfd/wfd-client.c:650`；R32 `src/ctl/ctl-sink.c:327-332` | source-reviewed | yes |
| F-07 | control | M6：`SETUP <presentation URL>`，携带 `Transport`；应答携带 `Session`（形如 `id;timeout=`，取分号前子串）与 `server_port` | R34 `sink/WifiDisplaySink.cpp:578-589`、`source/WifiDisplaySource.cpp:996-1051`；R37 `wfd_message.h:181`；R32 `src/ctl/ctl-sink.c:338-347`、`Session` 取 `:178-191` 并回显 `:200` | source-reviewed | yes |
| F-08 | control | M7：`PLAY <URL>`（带 `Session`）；`CSeq` 逐条递增 | R34 `sink/WifiDisplaySink.cpp:613`；R37 `wfd_message.h:203`；R32 `src/ctl/ctl-sink.c:193-200`；`src/shared/rtsp.c:1271`（`CSeq`） | source-reviewed | yes |
| F-09 | control | M8：`TEARDOWN`；AOSP 侧在 `AWAITING_CLIENT_TEARDOWN` 状态接收 | R34 `WifiDisplaySource.cpp:922-935`（方法分派）、`WifiDisplaySource.h:55-65`（状态）；R37 `wfd_message.h:204`；**反例**：R32 sink 侧未实现 TEARDOWN（全仓无该字面量） | source-reviewed | yes |
| F-10 | control | M13：`SET_PARAMETER` + `wfd_idr_request`（请求关键帧），无正文参数 | R37 `services/sink/impl/wfd/wfd_sink/wfd_sink_session.h:83`（`SendIDRRequest(); // M13/SET_PARAMETER wfd-idr-request`）、`wfd_message.h:206-213`；R34 `native/wifi-display/ANetworkSession.cpp:380`、`WifiDisplaySource.cpp:1332`；R33 `src/wfd/wfd-client.c:475` | source-reviewed | yes |
| F-11 | control | M16：`GET_PARAMETER` keep-alive，带 `Session` 头（URI 非 `*`） | R37 `wfd_message.h:229-235`、`wfd_sink_session.h:86`（`// ... M16/GET_PARAMETER keep-alive`）；R34 `WifiDisplaySource.cpp:612`（`sendM16`）；R33 `src/wfd/wfd-client.c:384`（`GET_PARAMETER rtsp://localhost/wfd1.0`） | source-reviewed | yes |
| F-12 | control | presentation URL 形状 `rtsp://<host>:<port>/wfd1.0/streamid=0`；M4 里写作 `.../streamid=0 none`（尾部 `none` 为 3D/显示模式占位）；source 侧校验路径后缀 `/wfd1.0/streamid=0` | R34 `WifiDisplaySource.cpp:547,1078`、`WifiDisplaySink.cpp:648-649`；R33 `src/wfd/wfd-client.c:237`；R37 `services/impl/wfd/wfd_message.cpp:579`、`wfd_session_def.h:77`（`rtsp://localhost/wfd1.0`） | source-reviewed | yes |
| F-13 | payload | `wfd_client_rtp_ports` 值形状 `RTP/AVP/UDP;unicast <port0> <port1> mode=play`；source 侧校验 `port0 != 0`、`port0 <= 65535`、`port1 == 0`，否则 `ERROR_MALFORMED` | R34 `WifiDisplaySource.cpp:704-715`；R32 `src/ctl/ctl-sink.c:124`；R33 `src/wfd/wfd-params.c:202-248`（要求 profile 与 `mode=play`） | source-reviewed | yes |
| F-14 | payload | RTCP 端口 quirk：`port1` 为 0 或等于 `port0` 时视为**无效**，取 `port0 + 1` 为 RTCP 端口，并置 `WFD_QUIRK_NO_KEEPALIVE`（不打 keep-alive） | R33 `src/wfd/wfd-params.c:202-248`、`src/wfd/wfd-media-factory.c:544-548` | source-reviewed | yes |
| F-15 | payload | `Transport` 形状：UDP 单播 `RTP/AVP/UDP;unicast;client_port=<p>`（RTCP 可写 `-<p2>` 或省略）；TCP 交错 `RTP/AVP/TCP;interleaved=0-1` | R34 `sink/WifiDisplaySink.cpp:583-589`、`source/WifiDisplaySource.cpp:996-1046`；R32 `src/ctl/ctl-sink.c:346-347`；R37 `services/protocol/rtsp/src/rtsp_request.cpp:122-126` | source-reviewed | yes |
| F-16 | payload | 缺 `client_port` 时的回退：老 LG dongle 的 `Transport` 只有 `RTP/AVP/UDP;unicast`，此时按固定 `19000` 收流 | R34 `WifiDisplaySource.cpp:1049-1051` | source-reviewed | yes |
| F-17 | payload | `wfd_video_formats` 值 = `native SP preferred-display-mode SP 描述符列表`；描述符以 `,` 分隔，字段序为 `profile level cea_sup vesa_sup hh_sup latency min_slice_size slice_params frame_rate_ctrl`，随后两个占位字段（来源注释标注为最大宽/高，取 `none` 表示未指定） | R33 `src/wfd/wfd-params.c:270-283`（native 与 preferred-display-mode 的拆分）、`src/wfd/wfd-video-codec.c:264-318`（字段序与切片字段位布局）、`:452-459`（序列化格式串，11 个可变字段 + 两个 `none`）；R34 `sink/WifiDisplaySink.cpp:515`（13 段字面量） | source-reviewed | yes |
| F-18 | payload | H.264 profile 位图：bit0 CBP、bit1 CHP…；level 位图：bit0=3.1、bit1=3.2、bit2=4.0、bit3=4.1、bit4=4.2、bit5=5.0、bit6=5.1、bit7=5.2（来源注释标注为 WFD 规范 Table 38/39） | R37 `services/impl/wfd/wfd_session_def.h:231-250`；R33 `src/wfd/wfd-video-codec.h:13-17`（`BASE = 0x01`、`HIGH = 0x02`） | source-reviewed | yes |
| F-19 | payload | 分辨率表由 `native` 字段选择：低 3 位为表（CEA=0b000、VESA=0b001、HH=0b010），高位为表内索引 | R33 `src/wfd/wfd-video-codec.c:313`（`resolution_table_lookup (native & 0x7, native >> 3)`）、`:87-91`；R37 `wfd_session_def.h:225-229` | source-reviewed | yes |
| F-20 | payload | `wfd_audio_codecs` 值 = 以 `,` 分隔的 `CODEC MODES LATENCY`，`CODEC ∈ {LPCM, AAC, AC3}`、`MODES` 为 8 位十六进制位图、末字段 2 位十六进制（R33 解析为 `× 5 ms` 延迟） | R33 `src/wfd/wfd-audio-codec.c:91-118,146`；R34 `WifiDisplaySource.cpp:549-551`、`:649-650`（语法注释）；R37 `wfd_session_def.h:49-51,62-64` | source-reviewed | yes |
| F-21 | payload | 音频 mode 位语义：AAC bit0 = 48 kHz/16 bit/2ch；LPCM bit0 = 44.1 kHz/16 bit/2ch、bit1 = 48 kHz/16 bit/2ch | R34 `WifiDisplaySource.cpp:730-734`（`modes & 1`、`modes & 2`）；R37 `wfd_session_def.h:258-268` | source-reviewed | yes |
| F-22 | payload | 内容保护：取值 `none` 或以 `HDCP2.0 `/`HDCP2.1 ` 开头的串，可带 `port=<n>` 属性；**HDCP 密钥交换/握手属设备与厂商材料，本仓不实现** | R34 `WifiDisplaySource.cpp:753-780`（检测、`ParsedMessage::GetInt32Attribute(..., "port", ...)`）、`:1465-1494`（IHDCP 调用）；R32 `src/ctl/ctl-sink.c:90`（`wfd_content_protection: none`）；R37 `services/sink/impl/scene/wfd/include/sink_def.h:39`（默认 `none`；source 只记录不强制，`wfd_source_session.cpp:628-631`） | source-reviewed（拒绝实现握手） | yes（只做解析与拒绝） |
| F-23 | discovery | WFD 子元素（subelement）：id `0x00` = 设备信息子元素，长度 6，载荷 = 设备信息(2) + 控制端口(2) + 最大吞吐(2)；R33 的 9 字节字面量 `00 00 06 00 90 1c 44 00 c8` 中 `0x1c44`=7236、`0x00c8`=200，与 R37 的 `SetCtrlPort(...)`/`SetMaxThroughput(0x00c8)` 一致 | R33 `src/nd-wfd-p2p-sink.c:546-547`；R37 `services/source/impl/scene/wfd/wfd_source_scene.cpp:672-674`、`services/sink/impl/scene/wfd/wfd_sink_scene.cpp:1395-1397` | source-reviewed | yes |
| F-24 | discovery | **WFD IE 容器（OUI / OUI type 字节）与设备信息位图语义在四个来源里均未出现**：R33 只塞子元素 blob；R37/R34 把 IE 交给平台 API（`Wifi::WifiP2pWfdInfo` / `WifiP2pWfdInfo`）；R32 只透传 wpa_supplicant 的 `wfd_subelems` 且自带 TODO 说明未解析 | R33 `src/nd-wfd-p2p-sink.c:547`；R37 `services/source/impl/scene/wfd/wfd_source_scene.cpp:125-126`；R34 `app/src/com/ivygroup/wfdplayer/WfdSinkController.java:79-83`；R32 `src/wifi/wifid-supplicant.c:889,905,900-904` | **blocked**（不得构造完整 IE） | **no** |
| F-25 | discovery | P2P（Wi-Fi Direct）组形成与 IE 播发依赖平台：NetworkManager P2P（`nm_client_add_and_activate_connection2` + `NM_SETTING_WIFI_P2P_WFD_IES`）、HarmonyOS Wi-Fi kit、Android `WifiP2pManager`；macOS 无公开 WFD API | R33 `src/nd-wfd-p2p-sink.c:556-558,577-584`、`src/nd-wfd-p2p-provider.c:183-284`；R37/R34（同 F-24 的平台 API 行）；M05 dossier 的平台结论 | **blocked**（P-M05-1） | **no** |
| F-26 | control | 会话状态名各实现自定、**不构成规范事实**：source 侧 AOSP `INITIALIZED/AWAITING_CLIENT_CONNECTION/AWAITING_CLIENT_SETUP/AWAITING_CLIENT_PLAY/ABOUT_TO_PLAY/PLAYING/AWAITING_CLIENT_TEARDOWN/STOPPING/STOPPED`、R37 `M0..M8(+M6_SENT/M6_DONE/M7_WAIT/M7_SENT/M7_DONE)`、R33 `INIT_STATE_M0..M5 + INIT_STATE_DONE`；sink 侧 R37 `INIT/READY/PLAYING/STOPPING`、R34 `UNDEFINED/CONNECTING/CONNECTED/PAUSED/PLAYING`。**共同可固定的只有消息顺序（M1→M2→M3→M4→M5→SETUP→PLAY→…→TEARDOWN）** | R34 `WifiDisplaySource.h:55-65`、`sink/WifiDisplaySink.h:47-53`；R37 `wfd_source_session.h:40`、`wfd_sink_session.h:92`；R33 `src/wfd/wfd-client.c:10-17` | source-reviewed（只固定顺序，不固定状态名） | yes |
| F-27 | control | keep-alive 时序取值（**来自实现取值，不是规范常量**）：RTSP 会话超时 30 s、每 25 s 发一次 `GET_PARAMETER`；F-14 的 quirk 命中时禁用 | R33 `src/wfd/wfd-client.c:426,433`、`src/wfd/wfd-media-factory.c:544-548` | source-reviewed（本仓取同值并标注来源） | yes |
| F-28 | payload | 媒体封装 = **MPEG-2 TS over RTP/UDP**：RTP 固定头 12 字节（版本位 `0x80`、`byte1` 低 7 位 = payload type **33**、M 位 `0x80`、随后 seq/ts/SSRC）；接收侧以 `rtpmp2tdepay ! tsdemux` 解复用 | R34 `native/wifi-display/source/Sender.cpp:295-315`；R32 `res/gstplayer:78`、`res/miracle-gst:78`；R33 `src/wfd/wfd-media-factory.c:461-470`（`rtpmp2tpay`，`ssrc=1`） | source-reviewed | yes |
| F-29 | policy | 丢包后请求关键帧（M13）的**触发阈值与策略属本仓策略**：来源只证明该消息存在（F-10），不规定何时发 | 本仓策略（`impl/crates/proto-wfd/src/rtp.rs` 常量与测试）；消息事实见 F-10 | source-reviewed（策略由本仓定义） | yes |
| F-30 | compat | 真实设备 codec/时序矩阵（Samsung/Huawei/Windows+K → sink、TV 端 sink 行为）**未固化**；WFD IE 播发与 M0–M16 完整实测序列同样未固化 | P-M05-2/P-M05-3 未关闭 | **blocked**（impl 不准入） | **no** |

## B. 与本仓实现的关系（T37 headless 切片）

- **实现**：F-01..F-23、F-26..F-29 中不依赖真机/平台 API 的部分——
  1. **消息层**：M1..M8 与 M13/M16 的方法与方向表（F-02..F-11）、严格 `CSeq` 与 `Session` 语义、
     presentation URL 校验（F-12）、`Transport` 解析（F-15/F-16）、`wfd_client_rtp_ports` 解析（F-13）
     与 RTCP quirk（F-14）；
  2. **协商层**：`wfd_video_formats`/`wfd_audio_codecs` 描述符解析与序列化（F-17..F-21）、
     交集选择（无可交集 → 明确拒绝，不静默降级）；
  3. **子元素层**：subelement id `0x00` 的编解码（F-23），**不含 IE 容器**（F-24 blocked）；
  4. **媒体边界**：RTP 固定头解析与序号/丢包记账（F-28），丢包按本仓策略触发 M13（F-29）；
     无 TS 解复用、无解码、无渲染。
- **不实现**：F-24/F-25（WFD IE 容器与 P2P 组形成——不得臆造 OUI）、F-22 的 HDCP 握手（设备/厂商材料）、
  F-30（真实设备矩阵）、UIBC 输入回传、任何平台 API 调用（不含 NetworkManager/wpa_supplicant/WinRT/P2P）。

## C. 能力分声明

| 能力 | 状态 | 障碍 / 重评条件 |
|---|---|---|
| WFD RTSP 控制面（消息与方向、状态顺序、keep-alive） | `source-reviewed`（T37 实现） | 真机序列未验（P-M05-2） |
| 媒体协商（video/audio/ports 描述符） | `source-reviewed`（T37 实现） | 真实设备 codec 矩阵未验（P-M05-3） |
| 子元素编解码（id 0x00） | `source-reviewed`（T37 实现） | 容器 OUI 未固化（F-24） |
| RTP 接收记账（序号/丢包/关键帧请求） | `source-reviewed`（T37 实现） | 无 TS 解复用/解码；真机收流未验 |
| **完整 WFD IE 构造/解析** | `blocked` | F-24：OUI 与 OUI type 在四个来源里都不存在；需 P-M05-2 抓包关闭 |
| **P2P 组形成 / IE 播发** | `blocked` | F-25 + P-M05-1：macOS 无公开 API；需 Windows 或 Linux 隔离机 probe |
| **HDCP 内容保护** | `not-implemented` | F-22：密钥交换属设备/厂商材料，本仓不实现（不由本仓放行） |
| **TS 解复用 / 解码 / 画面呈现** | `not-implemented` | 属媒体引擎范围；本切片只到 RTP 记账 |
| 真实设备兼容矩阵 | `blocked` | P-M05-3：需库存 TV/手机与用户在场 |

## D. 语料规则（`evidence/wfd/`）

1. 全部 fixture 自制（RTSP 请求/应答、M3/M4 参数体、子元素字节由测试构造），**不含抓包与第三方字节**；
2. 负向必须含：方法/方向错配、`CSeq` 跳号、未知参数、`wfd_client_rtp_ports` 畸形、RTCP quirk 命中、
   描述符字段缺失/超长、profile/level 越界、`wfd_content_protection` 非 `none`（→ 明确拒绝而非忽略）、
   RTP 序号跳变；
3. 真机语料（抓包/真机 log）后置、由用户执行；pcap 不入库；
4. 语料不得包含任何 OUI 猜测值（F-24）。

## E. 平台/许可

- 事实来源：R32 miraclecast（LGPL-2.1 为主）、R33 gnome-network-displays（GPL）、
  R34 miracast-sink（Apache-2.0 衍生声明）、R37 castengine_wifi_display（Apache-2.0）——
  **只取字段名/常量/结构事实**，不复制表达、不翻译、不链接；逐项登记见 `provenance/m05-inputs.json`。
- R35（universal-miracast-sink）**gated**：涉及固件/反编译来源描述，只登记 provenance，不进实现允许列表；
  R36/R38/R39 本轮未使用。
- 实现不链接任何第三方 WFD/RTSP 库；RTSP 框架为自研最小子集（沿用 `proto-airplay` 的解析纪律）。

## F. 决策

**路线**：接收侧（sink）控制面与媒体协商作为独立 headless 实现（本切片）；平台入口按 dossier
（Windows = 系统 MiracastReceiver API，Linux = 独立 WFD core 候选，macOS = 无 API，如实
`platform-unavailable`）。重评条件：P-M05-1/2/3 关闭。
