# Reviewed Wire Spec — cast.media-control（M08 / T43）

**状态**：字段级事实表已建立（2026-09-15，T43 gate）。**P-M08-1（真实设备对第三方 sender 的认证强制点）
blocked**：需要库存 Chromecast/Google TV 与用户在场。P-M08-2（openscreen cast/ 组件边界）与
P-M08-3（mDNS TXT 字段矩阵）同样未关闭。

**红线**：状态 `blocked` 的行不得写进实现；本文件不复制第三方表达，只记字段名/常量/结构事实。
**认证（DeviceAuth）与 app ID 是两条硬边界**：本仓不实现认证握手（缺设备证书材料），也不伪造/申请 app ID
（注册体系属 Google Cast SDK Developer Console，注册 ≠ 通用硬件授权）。

## A. 字段级事实表

| # | 层 | 事实 | 来源（commit+位置） | 状态 | impl-allowed |
|---|---|---|---|---|---|
| F-01 | channel | CASTV2 信封 `CastMessage` 字段号：`protocol_version`=1(required)、`source_id`=2(required)、`destination_id`=3(required)、`namespace`=4(required)、`payload_type`=5(required)、`payload_utf8`=6、`payload_binary`=7、`continued`=8、`remaining_length`=9 | R42 2963401 `cast/common/channel/proto/cast_channel.proto:20,36,37,42,51,58-70` | source-reviewed | yes |
| F-02 | channel | `ProtocolVersion` 枚举：`CASTV2_1_0`=0、`CASTV2_1_1`=1（分块，注释标 deprecated）、`CASTV2_1_2`=2（重做分块）、`CASTV2_1_3`=3（二进制载荷走 utf8） | R42 `cast/common/channel/proto/cast_channel.proto:12-19` | source-reviewed | yes |
| F-03 | channel | `PayloadType` 枚举：`STRING`=0、`BINARY`=1；两个载荷字段"二选一" | R42 `cast/common/channel/proto/cast_channel.proto:44-51` | source-reviewed | yes |
| F-04 | channel | 帧 = 4 字节长度前缀（`kHeaderSize = sizeof(uint32_t)`）+ 正文；**正文上限 64 KiB**（`kMaxBodySize = 65536`），序列化与反序列化两侧都拒绝超限 | R42 `cast/common/channel/message_framer.cc:20-23,29-31,46-48` | source-reviewed | yes |
| F-05 | channel | 分块字段（`continued`/`remaining_length`）在参考实现里**没有实现路径**（只出现在 proto 与调试串）；本仓同样不实现：收到 `continued = true` 明确拒绝 | R42 `cast/common/channel/proto/cast_channel.proto:58-70`、`cast/common/channel/message_util.cc:168` | source-reviewed（实现侧缺席） | yes（只做拒绝） |
| F-06 | channel | 特殊端点 id：`sender-0`、`receiver-0`、通配 `*` | R42 `cast/common/channel/proto/cast_channel.proto:26-33`（注释）；R44 5cfdb607 `pychromecast/const.py:83`（`PLATFORM_DESTINATION_ID = "receiver-0"`） | source-reviewed | yes |
| F-07 | channel | namespace 字面量：`urn:x-cast:com.google.cast.media`、`.receiver`、`.tp.connection`、`.tp.heartbeat`、`.tp.deviceauth`、`.broadcast`、`.setup`、`.receiver.discovery` | R42 `cast/common/channel/message_util.h:20-37`；R44 `pychromecast/socket_client.py:47`、`controllers/heartbeat.py:16`、`controllers/receiver.py:28`、`controllers/media.py:385` | source-reviewed | yes |
| F-08 | channel | 载荷 JSON 的类型键是 `type`；connection 命名空间类型 `CONNECT`/`CLOSE`/`CONNECTED`；CONNECT 携带 `origin`（对象）、`connType`、`userAgent`、`senderInfo`（含 `sdkType`/`version`/`browserVersion`/`platform` 等键） | R42 `cast/common/channel/message_util.h:58,69-71,76-84`、`message_util.cc:124,142`；R44 `socket_client.py:49-50,1013-1030,1036-1042` | source-reviewed | yes |
| F-09 | channel | heartbeat 类型 `PING`/`PONG`；**周期只在 R44 给出取值**（`HB_PING_TIME = 10`、`HB_PONG_TIME = 10`，超时判定为两者之和），R42 无周期常量且文档写明控制层 PING/PONG 已弃用 → 取值属**实现取值，不是协议常量** | R44 `pychromecast/controllers/heartbeat.py:18-22,87-90`；R42 `cast/protocol/streaming_session_protocol.md:664-666` | source-reviewed（取值来源单一并已标注） | yes |
| F-10 | receiver | `receiver` 命名空间类型：`LAUNCH`{appId}、`GET_STATUS`、`STOP`、`SET_VOLUME`{volume{level 或 muted}}、`RECEIVER_STATUS`{status{applications[]{appId,displayName,namespaces[],sessionId,transportId,statusText,iconUrl,controlType},volume{level,muted},isActiveInput,isStandBy}}、`LAUNCH_ERROR`{reason,appId,requestId} | R44 `controllers/receiver.py:25-35,118-119,149-151,223,238-242,252-265,277-300`；R42 `cast/receiver/application_agent.cc:141-156,226-293`（接收侧只认 GET_APP_AVAILABILITY/GET_STATUS/LAUNCH/STOP） | source-reviewed | yes |
| F-11 | media | `media` 命名空间类型：`LOAD`、`PLAY`、`PAUSE`、`STOP`、`SEEK`、`GET_STATUS`、`MEDIA_STATUS`、`LOAD_FAILED`，另有 `QUEUE_INSERT`/`QUEUE_UPDATE`/`QUEUE_NEXT`/`QUEUE_PREV`/`SET_PLAYBACK_RATE`/`EDIT_TRACKS_INFO`（本仓不实现队列与倍速） | R44 `controllers/media.py:32-45` | source-reviewed | yes（队列/倍速除外） |
| F-12 | media | `LOAD` 载荷：`{media:{contentId,streamType,contentType,metadata{metadataType,title,thumb,images[]{url}}}, type:"LOAD", autoplay, currentTime?, customData:{}, activeTrackIds?}`；**`duration` 不在 LOAD 里写**（只从状态读） | R44 `controllers/media.py:47-51,476-498,536-546` | source-reviewed | yes |
| F-13 | media | `SEEK` 载荷：`{type:"SEEK", currentTime, resumeState:"PLAYBACK_START"}`；`relativeTime` 在本组来源里不存在 | R44 `controllers/media.py:657-664`、`controllers/plex.py:32,316,324` | source-reviewed | yes |
| F-14 | media | `MEDIA_STATUS` 载荷键：`media{contentId,contentType,duration,streamType,metadata}`、`mediaSessionId`、`playerState`、`idleReason`、`currentTime`、`volume{level,muted}` | R44 `controllers/media.py:313-334` | source-reviewed | yes |
| F-15 | media | `playerState` 取值 `PLAYING`/`BUFFERING`/`PAUSED`/`IDLE`/`UNKNOWN`；`streamType` 取值 `UNKNOWN`/`BUFFERED`/`LIVE` | R44 `controllers/media.py:22-30` | source-reviewed | yes |
| F-16 | channel | `requestId`：每条连接一个单调计数器，逐条注入载荷；响应按 `requestId` 关联回调；默认请求超时 10 s（库级默认值） | R44 `socket_client.py:204,515-519,884-886,926-927,664-665`、`const.py:13` | source-reviewed（默认值为实现取值） | yes |
| F-17 | app | app ID 属注册体系：默认媒体接收器 `CC1AD845`、待机/backdrop `E8C28D3C`；自定义 receiver 必须在 Google Cast SDK Developer Console 注册、登记 sender 与设备后才可被拉起 | R44 `config.py:10-12`、`__init__.py:39`；R43 ddfb06c7 `README.md:5,12,15,18,20,21` | source-reviewed（**本仓不申请、不伪造 app ID**） | yes（只做校验：非本节点配置的 app ID 不得发送） |
| F-18 | auth | DeviceAuth 消息：`AuthChallenge`(1-3)、`AuthResponse`(1-7，含 `signature`/`client_auth_certificate`/`intermediate_certificate`/`signature_algorithm`/`sender_nonce`/`hash_algorithm`/`crl`)、`AuthError`(1，枚举 INTERNAL_ERROR/NO_TLS/SIGNATURE_ALGORITHM_UNAVAILABLE)、`DeviceAuthMessage`(challenge=1/response=2/error=3)；**发送端实现强制认证**（非认证首包 → `kCastV2AuthenticationError` 并断链），接收端参考实现不拒绝未认证 sender | R42 `cast/common/channel/proto/cast_channel.proto:73-118`、`cast/sender/channel/sender_socket_factory.cc:175-178,181-202`、`cast/receiver/application_agent.cc:126-129`；R42 `cast/protocol/streaming_session_protocol.md:703-706` | source-reviewed（**认证握手不实现**：缺设备证书材料） | yes（只做 seam 与拒绝） |
| F-19 | discovery | 发现：服务类型 `_googlecast._tcp`、域 `local`；TXT 键 `id`/`ve`/`ca`/`st`/`fn`/`md`；`st` 取值 0/1（idle/busy）；`ca` 位掩码；连接端口 8009 | R42 `cast/common/public/receiver_info.h:19-28,31-51`；R44 `discovery.py:215-217,243`、`socket_client.py:324` | source-reviewed | yes（只做解析，不做组播） |
| F-20 | compat | 真实 Chromecast/Google TV 对第三方 sender 的实际认证强制点（哪些默认接受、哪些要求 DeviceAuth）**未固化** | P-M08-1 未关闭 | **blocked**（impl 不准入） | **no** |
| F-21 | scope | openscreen `cast/` 内 sender/streaming/receiver 与本 profile 的边界（M09 分界）**未固化** | P-M08-2 未关闭 | **blocked** | **no** |
| F-22 | discovery | mDNS TXT 字段矩阵（各代设备实际携带的键与取值）**未固化** | P-M08-3 未关闭 | **blocked** | **no** |
| F-23 | app | app 注册与签发：注册需要开发者账号与设备登记，且"注册 ≠ 通用硬件授权"（S08） | R43 `README.md:12,15,18,20`；S08（设计包编目） | source-reviewed（**不实现注册**） | **no**（登记在案） |
| F-24 | policy | 媒体 URL 由本节点签发（短时/单资源/可撤销），可达性属 host gateway lease（T24）；**本仓不拥有 lease**，只在停止/被终止时报告"必须释放" | 本仓策略（T43-03）；契约见 `docs/` 的 media URL 模型 | source-reviewed（策略由本仓定义） | yes |
| F-25 | media | 媒体字节服务（HTTP 拉取路径）与实时 streaming sender 不在本 task（T44 与 host gateway） | P-M08-1/T44 未开始 | **blocked**（本切片不实现） | **no** |
| F-26 | receiver | **认证在消息层，不在 TLS**：sender 侧恒定跳过 TLS 证书校验（`unsafely_skip_certificate_validation=true`，POSIX 即 `SSL_VERIFY_NONE`），真正的检查是把 `AuthResponse` 的证书链走到信任库（`VerifyDeviceCert`）；不可信链 → `kCastV2CertNotSignedByTrustedCa = 66` | R42 `platform/base/tls_connect_options.h:15`、`cast/sender/channel/sender_socket_factory.cc:69,96-101`、`platform/impl/tls_connection_factory_posix.cc:97-99`、`cast/sender/channel/cast_auth_util.cc:380-382`、`platform/base/error.h:147`、`cast/common/certificate/boringssl_trust_store.cc:493-500` | source-reviewed（T45 gate） | yes（只做状态与拒绝，不做握手） |
| F-27 | receiver | 谁出示什么：**sender 不出示证书**；receiver 出示 TLS 服务端证书 + `AuthResponse{client_auth_certificate, intermediates}`，签名输入是 `sender_nonce` 拼接 `receiver_tls_cert_der`，用设备私钥签；参考实现的独立 receiver 用**自签** "Test Device TLS" 证书 | R42 `cast/receiver/channel/device_auth_namespace_handler.cc:108-113,124-137`、`cast/receiver/channel/static_credentials.cc:82-86`、`cast/sender/channel/cast_auth_util.cc:364-375` | source-reviewed（T45 gate） | yes（只做消息形状与拒绝） |
| F-28 | receiver | **没有绕过 Google 根的生产路径**：默认 sender 只信任 Google 根（`CastTrustStore::Create`）；非 Google 证书仅在调用方**显式**配置信任库时可用（独立 sender 用 `--developer-certificate`；Chrome 需 `--cast-developer-certificate-path`，文档注明在 Chrome stable 可用）；CRL 校验默认可选（`kCrlOptional`）；自签 TLS 证书最长 4 天 | R42 `cast/sender/channel/sender_socket_factory.cc:31-36`、`cast/standalone_sender/main.cc:250-258`、`cast/docs/USING.md:29-39,94,154-161`、`cast/sender/channel/cast_auth_util.cc:36,316` | source-reviewed（T45 gate） | yes（**只做门禁与拒绝**：不获取、不伪造任何设备证书） |
| F-29 | receiver | 参考实现的**测试根**路径：独立 receiver 的 `--generate-credentials` 生成自签根（CN "Cast Root CA"、带 CA 标志、寿命 3 天）并写 `generated_root_cast_receiver.{key,crt}`，供 sender 显式信任 | R42 `cast/standalone_receiver/main.cc:58-62,153,233-236`、`cast/receiver/channel/static_credentials.cc:25-29,128-135,191-211`、`cast/docs/USING.md:29-39` | source-reviewed（**只用于本仓自配对 demo**：命名 test-root，不得冒充厂商信任） | yes（仅 test-root 路径） |
| F-30 | receiver | receiver 侧消息流：CONNECT → CONNECTED（**仅当 sender 给了协议版本才回**）、`sender-0`/`receiver-0` 平台 ID、`RECEIVER_STATUS`（含 transportId/sessionId）、LAUNCH 未知 app id → `kItemNotFound`、成功后再广播一次 `RECEIVER_STATUS`；app 按 app id 在**进程内注册** | R42 `cast/common/channel/message_util.h:40-41,69,71`、`cast/common/channel/connection_namespace_handler.cc:156-232,224-229,285-301`、`cast/receiver/application_agent.cc:246,268-291,324-339,387-436` | source-reviewed（T45 gate） | yes（只做状态机与形状） |
| F-31 | receiver | receiver 广播的 TXT：`id`/`ve`/`ca`/`st`/`fn`/`md`（`ve` 默认 2；`st` 0=idle、1=busy-join；`ca` 是能力位掩码）；参考实现的独立 receiver 用 **8010** 端口（真实设备用 8009——两个取值分别引用、不合并） | R42 `cast/common/public/receiver_info.h:19-28,44,47-53,81`、`cast/common/public/receiver_info.cc:83-103`、`cast/standalone_receiver/cast_service.cc:24,76-91` | source-reviewed | yes（只构造/解析 TXT，不组播） |
| F-32 | receiver | openscreen 的 receiver **没有** CAF/托管 app 加载路径（全仓 `CAF` 零命中、无 URL 加载字段）：app 是进程内对象；streaming 相关 app id 是**编译期常量**表 | R42 `cast/receiver/application_agent.cc:52-61,270-274`、`cast/common/public/cast_streaming_app_ids.h:16-29`、全仓 grep `CAF` 零命中 | source-reviewed（**本仓不内置任何 app id**：只用调用方配置的） | yes（只做校验） |
| F-33 | receiver | 参考实现的 receiver **不校验** sender 的证书（`cast/receiver/` 下无 `VerifyDeviceCert`）：认证是 sender 侧的前置条件——"能否被 stock sender 连上"由 **sender 的信任库**决定 | R42 `cast/receiver/`（grep 零命中）、`cast/sender/channel/sender_socket_factory.cc:181-202` | source-reviewed（T45-01 的关键依据） | yes（据此判定 stock sender 默认拒绝） |
| F-34 | receiver | **T45 结论（结论行，不是实现许可）**：桌面 receiver 的可达边界 = ① 可被发现（TXT 广播）② CONNECT/CONNECTED/心跳/RECEIVER_STATUS 可实现 ③ **LAUNCH 之后能否被 stock sender 接受，取决于它的信任库里有没有我们的证书** ⇒ stock sender（Pixel/Chrome 默认信任）**不可达**；本仓不得声称 stock receiver 可用 | F-26..F-33 + T45-01/02/03（本表）；R42 `cast/docs/USING.md:154-161` | **blocked**（stock 兼容不可达；写成能力即为越界） | **no** |
| F-35 | receiver | test-root 演示路径（**仅调用方显式构造时**）：自配对闭环 = 本仓 sender 与 receiver 在同一信任根下对跑；实现里不含任何证书/密钥材料，门禁对象名带 `test-root`，报告必须标注"非 stock 兼容" | F-29 + T45-01/03；R42 `cast/standalone_receiver/main.cc:58-62`、`cast/standalone_sender/main.cc:250-258` | source-reviewed（本仓策略：演示路径与产品路径分开命名） | yes（仅 test-root 门禁后） |

## B. 与本仓实现的关系（T43 headless 切片）

- **实现**：F-01..F-19、F-24 中不依赖真机与证书材料的部分——
  1. **信封层（`castv2.rs`）**：`CastMessage` 字段号 1..9 的编解码、required 字段强制、
     STRING/BINARY 载荷二选一、4 字节长度前缀与 64 KiB 上限（分配前拒绝）、
     `continued = true` → `unsupported-feature`（F-05）、协议版本 0..3 白名单；
  2. **命名空间层（`namespaces.rs`）**：namespace 与类型字面量、connection/heartbeat/receiver/media
     四组消息的载荷编解码（JSON 键逐条对应 F-08/F-10..F-15），未知类型 → 明确拒绝；
  3. **控制会话（`controller.rs`）**：连接 → （认证 seam）→ CONNECT/CONNECTED → LAUNCH →
     LOAD → PLAY/PAUSE/SEEK/STOP → CLOSE；`requestId` 单调注入与关联、超时、
     heartbeat 到点发送与过期判定；LAUNCH_ERROR 与 MEDIA_STATUS 的分流处理；
  4. **发现解析（`discovery.rs`）**：`_googlecast._tcp` 的 TXT 键值解析为 `ReceiverInfo`（不组播）。
- **不实现**：F-18 的认证握手（缺设备证书材料 → seam 恒 `vendor-gated`，未认证不得发送任何命令）、
  F-23 的 app 注册与 app ID 签发（未配置即拒绝）、F-20/F-21/F-22（真机与规范边界）、
  F-25（媒体字节服务/实时 streaming）、队列与倍速播放（F-11 的 QUEUE_*/SET_PLAYBACK_RATE）。

## C. 能力分声明

| 能力 | 状态 | 障碍 / 重评条件 |
|---|---|---|
| CASTV2 信封与命名空间消息（编解码） | `source-reviewed`（T43 实现） | 真机通道未验（P-M08-1） |
| sender 控制会话（launch/load/play/pause/seek/stop） | `source-reviewed`（T43 实现） | 真机命令集与接受条件未验（P-M08-1） |
| TXT 解析（`_googlecast._tcp`） | `source-reviewed`（T43 实现） | 字段矩阵未验（P-M08-3） |
| **设备认证（DeviceAuth）** | `not-implemented` | F-18：缺设备证书材料；seam 恒拒绝，未认证不发送命令 |
| **app 注册 / app ID 签发** | `not-implemented` | F-23：注册体系在 Google 侧；本仓只用已配置的 app ID |
| **屏幕镜像（screen mirroring）** | `not-implemented` | 本 profile 只做"媒体 URL"（T43-04）：screen 能力恒 false，不假装镜像 |
| 媒体字节服务 / 实时 streaming | `blocked` | F-25 + P-M08-1：本切片不服务媒体字节（T44 与 host gateway） |
| 真实设备兼容矩阵 | `blocked` | P-M08-1：需库存 Chromecast/Google TV 与用户在场 |
| **receiver 侧控制面（T45：CONNECT/CONNECTED、心跳、RECEIVER_STATUS 记账、LAUNCH 白名单与拒绝）** | `source-reviewed`（T45 实现，**仅在 test-root 门禁后可用**） | F-30/F-34：实现存在但默认门禁恒拒绝；stock sender 兼容仍 blocked |
| **stock sender 连上本机 receiver** | `blocked` | F-28/F-33：认证在消息层、默认 sender 只信厂商根；本仓不获取/不伪造证书材料；重评条件 = P-M08-1（真机认证强制点）+ P-M08-4（Chrome 开发者证书参数在用户机器上的实际行为） |
| receiver 的 TXT 广播（`id`/`ve`/`ca`/`st`/`fn`/`md`，只构造不组播） | `source-reviewed`（T45 实现） | F-31；字段矩阵未验（P-M08-3） |
| **CAF / 托管 receiver app 加载** | `blocked`（**来源未固化**） | R42 全仓无 CAF 与 URL 加载路径（F-32）→ 不能据此声称"托管 app 可行"；重评条件 = P-M08-2（sender/streaming/receiver 的 profile 边界）与另找合规来源 |

## D. 语料规则（`evidence/cast/`）

1. 全部 fixture 自制（CastMessage 字节、namespace 载荷 JSON、TXT 键值由测试构造），**不含抓包**；
2. 负向必须含：required 字段缺失、`payload_type` 与实际载荷不符、正文超 64 KiB、`continued = true`、
   未知 `protocol_version`、未知 namespace/类型、`requestId` 不匹配、LAUNCH_ERROR 后的 LOAD、
   未认证就发命令、未配置 app ID、TXT 缺 `id`/`fn`、状态里出现未知 `playerState`；
3. 真机语料（抓包/log）后置、由用户执行；pcap 不入库；
4. 语料不得包含真实设备证书、注册材料或任何密钥。

## E. 平台/许可

- 事实来源：R42 openscreen（BSD 风格，逐文件核——该 checkout 有 fork 提交，F-18 里的
  `cast/sender/...` 属 fork 新增，仅作"发送端强制认证"的事实）、R43 CastReceiver（Apache-2.0）、
  R44 pychromecast（MIT）——**只取字段名/常量/结构事实**，不复制表达、不翻译、不链接。
- R45（cast-web/protocol，restricted，文件级许可未核清）**未使用**；S07/S08/S09 为文档事实。
- 本实现不链接任何 Cast SDK；TLS 由宿主提供（本切片不含传输层）。

## F. 决策

**路线**：sender 侧"媒体 URL"控制面作为独立 headless 实现（本切片），认证作为 seam
（生产缺席 = 能力缺席，与 AirPlay keying seam 同一纪律）。屏幕镜像由 Miracast/AirPlay 承担，
Cast 只做媒体 URL 推送。重评条件：P-M08-1/2/3 关闭。
