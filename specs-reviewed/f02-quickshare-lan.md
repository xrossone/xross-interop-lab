# Reviewed Wire Spec — quickshare.lan.v1（F02）

**状态**：P-F02-2（UKEY2 绑定链）**source-reviewed 完成**（2026-09-15，T19）；
P-F02-1（LAN 发现载体）与 P-F02-3（可见性矩阵）**blocked**——需用户抓包/真机。

**本文件现在是什么**：一张**字段级事实表**——每行给出「事实 / 来源（commit+文件+行）/ 状态 / 是否允许进入实现」。
它不是对 Android 真实行为的断言：所有行都停在 source-reviewed，**没有一行是 device-verified**。

**红线**：状态为 `blocked` 或 `待固化` 的行，**不得**写进 `impl/`（T20 只实现 `impl-allowed=yes` 的行）。

## A. 字段级事实表

图例：`impl-allowed=yes` 表示该行已被来源固化到可以写实现（仍不是真机证据）；`no` 表示实现必须停在这条线之前。

| # | 层 | 事实 | 来源（commit+位置） | 状态 | impl-allowed |
|---|---|---|---|---|---|
| F-01 | discovery | 服务端在 LAN 广播 mDNS 服务类型 `_FC9F5ED42C8A._tcp.`；名称是 10 字节 URL-safe base64：`0x23`（"PCP"）+ 4 字节 endpoint id + 3 字节 service id `FC 9F 5E` + 2 个零字节 | R15 8f1fdc7 `PROTOCOL.md:27-36` | source-reviewed | no |
| F-02 | discovery | TXT 键 `n` = base64(endpoint info)：1 字节位域（3 位 version / 1 位 visibility（0=可见）/ 3 位 device type / 1 位保留）+ 16 字节设备识别材料 + 1 字节长度的 UTF-8 设备名（仅在可见时）+ TLV（1: QR 数据，2: vendor id） | R15 `PROTOCOL.md:38-51` | source-reviewed | no |
| F-03 | discovery | service id 来源：`SHA256("NearbySharing")` = `fc9f5ed42c8a5e9e94684076ef3bf938a809c60ad354992b0435aebbdc58b97b`（前 6 字节即 `FC9F5ED42C8A`） | R15 `PROTOCOL.md:60` | source-reviewed；**本仓可用命令自证**（headless 可核实） | yes |
| F-04 | discovery | Android 只在收到 BLE 广播（service UUID `fe2c`，service data 前缀 `fc 12 8e 01 42 00…`）后才广播 mDNS；macOS 无 API 发送该 service data | R15 `PROTOCOL.md:53-58` | source-reviewed | no |
| F-05 | framing | TCP 上是 4 字节**大端**长度前缀 + 该长度的消息体；读入前先做长度上限检查（R15 用 `SANE_FRAME_LENGTH = 5*1024*1024` 作为本实现的保守上限——**这是实现选择，不是协议常量**） | R15 `NearbyShare/NearbyConnection.swift:18,117-121` | source-reviewed（上限值待真机确认） | yes |
| F-06 | ukey2 | 外层封装 `Ukey2Message{ message_type=1, message_data=2 }`；`Type`：`UNKNOWN_DO_NOT_USE=0, ALERT=1, CLIENT_INIT=2, SERVER_INIT=3, CLIENT_FINISH=4` | R18 10fc737 `src/main/proto/ukey.proto`；README §Message Framing | source-reviewed（规范） | yes |
| F-07 | ukey2 | 错误以 `Ukey2Alert{ type=1, error_message=2 }` 返回，码表：`BAD_MESSAGE=1, BAD_MESSAGE_TYPE=2, INCORRECT_MESSAGE=3, BAD_MESSAGE_DATA=4, BAD_VERSION=100, BAD_RANDOM=101, BAD_HANDSHAKE_CIPHER=102, BAD_NEXT_PROTOCOL=103, BAD_PUBLIC_KEY=104, INTERNAL_ERROR=200`；**所有 alert 都是致命**（收到即关闭连接） | R18 `ukey.proto`；README §Alerts | source-reviewed（规范） | yes |
| F-08 | ukey2 | `Ukey2ClientInit{ version=1, random=2, cipher_commitments=3[CipherCommitment{handshake_cipher=1, commitment=2}], next_protocol=4 }`；`version` 必须为 1；`random` 恰好 32 字节；每个 cipher 只允许一条 commitment，顺序即客户端偏好 | R18 README §The ClientInit Message；`src/main/cpp/src/securegcm/ukey2_handshake.cc:330-335`（`kNonceLengthInBytes = 32`） | source-reviewed（规范+实现一致） | yes |
| F-09 | ukey2 | `Ukey2ServerInit{ version=1, random=2, handshake_cipher=3, public_key=4 }`；`random` 恰好 32 字节 | R18 README；`ukey2_handshake.cc:428-433` | source-reviewed | yes |
| F-10 | ukey2 | `Ukey2ClientFinished{ public_key=1 }` | R18 README；`ukey.proto` | source-reviewed | yes |
| F-11 | ukey2 | commitment = `SHA-512(serialized Ukey2Message(CLIENT_FINISH, ClientFinished))`——即**含外层 Ukey2Message 封装、不含 TCP 4 字节长度前缀**的字节 | R18 `ukey2_handshake.cc:583-612`（`CryptoOps::Sha512`）；R15 `PROTOCOL.md:141` | source-reviewed（规范与两实现一致） | yes |
| F-12 | ukey2 | **冲突行（cipher 选择规则）**：规范概览说「服务端选其同样支持的最高 enum 值」；规范详述与 C++ 实现是「按客户端给出的顺序选**第一个**可接受的」。裁决（source 级）：以实现为准（客户端顺序优先）；**真机实验（具名）**：Pixel/Samsung stock 与 R15 对跑时观察服务端是否选择首项——P-F02-2 关闭条件之一 | R18 README §Handshake Ciphersuites vs §Interpreting ClientInit + `ukey2_handshake.cc:341-370` | source-reviewed；**待真机裁决** | yes |
| F-13 | ukey2 | 握手 cipher 枚举：`P256_SHA512=100`、`CURVE25519_SHA512=200`；Nearby Share/Quick Share 用 P-256（R15 侧 `EC256r1`） | R18 README；R15 `NearbyShare/NearbyConnection.swift:346-353` | source-reviewed | yes |
| F-14 | ukey2 | DH 共享值 `DHS = SHA-256(ECDH 共享秘密字节)`（`KeyAgreementSha256`）；R15 对 P-256 点乘结果的 x 坐标做 SHA-256 | R18 `src/securemessage/src/securemessage/crypto_ops.cc:221-240`；R15 `NearbyConnection.swift:355-358` | source-reviewed（规范未指定，两实现一致） | yes |
| F-15 | ukey2 | **冲突行（HKDF 哈希）**：规范说 HKDF 用「握手 cipher 的哈希」（P256_SHA512 → SHA-512）；C++ 实现 `CryptoOps::Hkdf` 实际是 **HKDF-SHA256**（`HkdfSha256Extract/Expand`），R15 同样用 HKDF-SHA256。裁决（source 级）：互通事实以实现为准（HKDF-SHA256）；规范文本差异记录在案。**真机实验（具名）**：与 stock Android 完成一次握手并比对 4 位 PIN——P-F02-2 关闭条件之二 | R18 README §Deriving… vs `crypto_ops.cc:57-109`；R15 `NearbyConnection.swift:341-374` | source-reviewed；**待真机裁决** | yes |
| F-16 | ukey2 | 认证串与 next secret：`AUTH = HKDF(ikm=DHS, salt="UKEY2 v1 auth", info=M1‖M2)`；`NEXT = HKDF(ikm=DHS, salt="UKEY2 v1 next", info=M1‖M2)`；`M1/M2` = 双方完整序列化（含 Ukey2Message 封装、不含 TCP 长度前缀）的前两条消息；UKEY2 的 `L_auth/L_next` 由**下一协议**决定（UKEY2 本身不规定长度） | R18 README §Deriving…；`ukey2_handshake.cc:32,35,210-265`；R15 `NearbyConnection.swift:365-374` | source-reviewed（三方一致） | yes |
| F-17 | auth | 4 位确认码：对 auth 串逐字节做 `hash = (hash + int8(byte)·mult) mod 9973, mult = mult·31 mod 9973`，输出 `%04d` | R15 `NearbyConnection.swift:307-319`（PROTOCOL.md 指向 Chromium `nearby_sharing_service_impl.cc`） | source-reviewed（单一实现来源） | yes |
| F-18 | ukey2 | `next_protocol` 取值为 `"AES_256_CBC-HMAC_SHA256"`（客户端设置、服务端校验，不匹配即拒绝） | R15 `NearbyShare/OutboundNearbyConnection.swift:173`、`InboundNearbyConnection.swift:187` | source-reviewed | yes |
| F-19 | keys | 握手后密钥链：`D2D_client = HKDF-SHA256(ikm=NEXT, salt=SHA256("D2D"), info="client")`、`D2D_server = …info="server"`；随后四把密钥以 `salt=SHA256("SecureMessage")`、`info="ENC:2"`（加密）/`"SIG:1"`（HMAC）派生 | R15 `NearbyConnection.swift:380-395`；R18 `src/main/cpp/src/securegcm/d2d_crypto_ops.cc:120-150` | source-reviewed（两来源一致；T21 已实现） | yes |
| F-20 | transport | 加密层 `SecureMessage{ header_and_body=1, signature=2 }`；`Header{ signature_scheme=1(HMAC_SHA256=1), encryption_scheme=2(AES_256_CBC=2), iv=5, public_metadata=6 }`；D2D 消息含独立递增 sequence number（首条为 1） | R18 `securemessage.proto`；R15 `PROTOCOL.md:175-183` | source-reviewed | no |
| F-21 | control | 连接握手顺序：连接请求 → UKEY2 ClientInit → ServerInit → ClientFinish → 双方 connection response；此后全部加密 | R15 `PROTOCOL.md:87-109,127-171` | source-reviewed | yes |
| F-22 | control | paired-key encryption/result 帧、introduction（文件清单 + payload_id）、response（`ACCEPT`/`REJECT`/`NOT_ENOUGH_SPACE`）、payload transfer（id/type/totalSize/chunk/offset/flags(LAST_CHUNK bit0)）、disconnection | R15 `PROTOCOL.md:191-227` | source-reviewed | no |
| F-23 | control | keep-alive：Android 每 10 秒发一次 offline frame `KEEP_ALIVE` 并期待对端同样发送，否则断开 | R15 `PROTOCOL.md:228-230` | source-reviewed | no |
| F-24 | discovery/QR | QR URL `https://quickshare.google/qrcode#key=…`；由 `key` 派生 `advertisingToken = HKDF-SHA256(ikm=key, salt="", info="advertisingContext", 16B)` 与 `nameEncryptionKey = HKDF-SHA256(…, info="encryptionKey", 16B)`；可见时 TLV 直接放 token，隐藏时放 AES-GCM(12B IV ‖ 密文 ‖ 16B tag, AAD=token) | R15 `PROTOCOL.md:62-83` | source-reviewed | no |
| F-25 | transport | `SecureMessage{header_and_body=1, signature=2}`、`HeaderAndBody{header=1, body=2}`、`Header{signature_scheme=1(HMAC_SHA256=1), encryption_scheme=2(AES_256_CBC=2), iv=5, public_metadata=6}` | R18 `src/main/proto/securemessage.proto:24-69` | source-reviewed（规范） | yes |
| F-26 | transport | `public_metadata` = `GcmMetadata{type=1, version=2}`，D2D 消息用 `type=DEVICE_TO_DEVICE_MESSAGE(13)`、`version=1` | R18 `src/main/proto/securegcm.proto:250-285`；R15 `PROTOCOL.md:177` | source-reviewed（两来源一致） | yes |
| F-27 | control | `OfflineFrame{version=1(V1=1), v1=2}`、`V1Frame{type=1(PAYLOAD_TRANSFER=3), payload_transfer=4}`、`PayloadTransferFrame{packet_type=1(DATA=1,CONTROL=2,PAYLOAD_ACK=3), payload_header=2, payload_chunk=3, control_message=4}`、`PayloadHeader{id=1,type=2(BYTES=1,FILE=2,STREAM=3),total_size=3,is_sensitive=4,file_name=5,parent_folder=6}`、`PayloadChunk{flags=1(LAST_CHUNK=0x1),offset=2,body=3,index=4}` | R17 2ea517e `connections/implementation/proto/offline_wire_formats.proto:26-215` | source-reviewed（官方 proto） | yes |
| F-28 | control | introduction 的 `FileMetadata{name=1,type=2,payload_id=3,size=4,mime_type=5,id=6,parent_folder=7,is_sensitive_content=9}` 与 `ConnectionResponseFrame.Status{ACCEPT=1,REJECT=2,NOT_ENOUGH_SPACE=3,...}` | R15 `NearbyShare/ProtobufSource/wire_format.proto:30-73,248-267`（该文件由 R15 汇集自 Chromium，见 provenance 的许可注记） | source-reviewed（仅取字段号事实） | yes |
| F-29 | transport | D2D 消息 `DeviceToDeviceMessage{message=1, sequence_number=2}`，序号必须递增（首条为 1，双方各自独立计数） | R18 `src/main/proto/device_to_device_messages.proto:18-27`；R15 `PROTOCOL.md:183` | source-reviewed（T21 采用严格 +1 策略） | yes |

| F-30 | control | **分层模型与编号冲突的裁决**：外层（Nearby Connections 层）用 R17 的 `V1Frame.FrameType`（`PAYLOAD_TRANSFER=3`、`KEEP_ALIVE=5`、`DISCONNECTION=6`、`PAIRED_KEY_ENCRYPTION=7`），内层（Nearby Share 层，装在 BYTES payload 里）用 R15 自己的枚举（`INTRODUCTION=1`、`RESPONSE=2`、`PAIRED_KEY_ENCRYPTION=3`、`PAIRED_KEY_RESULT=4`）——**同一字段号在两层含义不同**。依据：PROTOCOL.md 明确写 paired-key 帧是**包在 payload 层里**发出的，而 payload transfer 帧本身是外层帧 | R17 `connections/implementation/proto/offline_wire_formats.proto:41-53,54-70`；R15 `NearbyShare/ProtobufSource/wire_format.proto:189-212`；R15 `PROTOCOL.md:185-206` | source-reviewed（**待真机复核**：外层 keep-alive 的 type 值需与 P-F02-2 的同一次对跑确认） | yes |
| F-31 | control | keep-alive：外层 `V1Frame.type=KEEP_ALIVE(5)` + `KeepAliveFrame{ack=1(bool), seq_num=2(uint32)}`；Android 每 **10 秒**发一次并期待对端同样发送，否则过一段时间断开（"a while" 没有给数值） | R17 `offline_wire_formats.proto:45,61,444-449`；R15 `PROTOCOL.md:228-230` | source-reviewed（10 s 是**来源取值**，不是规范常量；超时阈值属**本仓策略**） | yes |
| F-32 | control | paired-key encryption 帧（Nearby Share 层，装在 BYTES payload 里）：`PairedKeyEncryptionFrame{signed_data=1, secret_id_hash=2, optional_signed_data=3, qr_code_handshake_data=4}`；**双方互发**；参考实现填随机字节（`secretIDHash` 6 B、`signedData` 72 B）且明说"要拿到里面的内容得跟 Google 服务器说话" → **材料不可离线推导**：本仓只做帧与状态机，材料由调用方提供 | R15 `wire_format.proto:322-340`；R15 `PROTOCOL.md:204-206` | source-reviewed（材料语义**未固化**：不得声称已实现配对或 PIN 免确认） | yes |
| F-33 | control | paired-key result 帧：`PairedKeyResultFrame{status=1(enum UNKNOWN=0/SUCCESS=1/FAIL=2/UNABLE=3), os_type=2}`；参考实现双方都发 `UNABLE`，其后流程照常（PIN 仍由用户核对） | R15 `wire_format.proto:342-356`；R15 `PROTOCOL.md:208-212` | source-reviewed（真机实际取值待 P-F02-2） | yes |

## B. 与本仓实现的关系（T20 范围）

- **T20 已实现**：F-05（TCP framing 结构 + 本仓保守上限）、F-06..F-11、F-12（实现侧规则）、
  F-13（仅 P256_SHA512）、F-14、F-15（HKDF-SHA256）、F-16、F-17（标注为兼容启发式）、F-18、F-21。
- **T21 已实现（headless 部分）**：F-19/F-25/F-26/F-29（D2D 密钥链 + SecureMessage 信封 + 序号）、
  F-22/F-27/F-28（introduction 的报价投影、response 状态、payload 分块与 LAST_CHUNK 语义）；
  落盘走 `interop-file` 的预算/顺序写/原子发布，文件名穿越由 FILE-05 规则拒绝。
- **T21+ 已实现（headless 部分，2026-09-15）**：F-30/F-31（外层 keep-alive 帧与 10 s 节奏、超时策略）、
  F-32/F-33（paired-key encryption/result 帧编解码与交换状态机，材料由调用方提供）——
  见 `impl/crates/proto-quickshare/src/control.rs`。
- **不实现**：F-01/F-02/F-04/F-24（发现与 QR，P-F02-1/3 未关闭）、`PAYLOAD_ACK`（未实现，显式返回）、
  `DisconnectionFrame`（外层 6：帧结构已登记，本切片不做带宽升级路径）、
  paired-key 的**材料语义**（F-32：不可离线推导，不做配对存储、不据 `SUCCESS` 免 PIN——见能力表）。
- 因此 `impl/crates/proto-quickshare` 里**没有**任何 mDNS/BLE/QR/GMS 代码——这是机器检查项（`tools/test_quickshare_gate.py` QS-02）。

## C. 能力分声明（禁止一个布尔值概括）

| 能力 | 状态 | 障碍 / 重评条件 |
|---|---|---|
| LAN 发现（广播/监听 mDNS `_FC9F5ED42C8A._tcp.`） | `blocked` | P-F02-1：需用户批准网段抓包对照 R15/R17；重评条件＝抓包与源码对照一致 |
| UKEY2 握手（P-256 / HKDF-SHA256 / SHA-512 commitment） | `source-reviewed`（实现见 T20） | 互通真机验证未做；F-12/F-15 冲突需具名真机实验 |
| 传输加密（SecureMessage AES-256-CBC + HMAC-SHA256） | `source-reviewed`（实现见 T21） | 真机互通未验（P-F02-2） |
| payload/文件接收闭环 | `source-reviewed`（实现见 T21） | 真机互通未验；反向发送仍需 F-24 |
| keep-alive 帧与节奏（F-31） | `source-reviewed`（实现见 T21+） | 真机是否每 10 s 发、断开阈值多少待对跑（P-F02-2）；超时阈值是本仓策略 |
| paired-key 交换（F-32/F-33：帧 + 状态机） | `source-reviewed`（实现见 T21+） | **材料不可离线推导**：不做配对存储、不据 `SUCCESS` 免 PIN；真机实际 status 待 P-F02-2 |
| 反向发送（Mac→Android） | `not-implemented` | 需要 QR/显式可发现（F-24）与 P-F02-3 |
| 可见性模式矩阵（所有人/联系人/隐藏） | `blocked` | P-F02-3：两台 Android × 不同设置的真机矩阵 |
| 4 位确认码一致性 | `source-reviewed`（启发式实现） | 真机比对（P-F02-2 关闭条件之二） |

## D. 语料规则（`evidence/quickshare/`）

1. **自制优先**：所有 fixture 由本仓脚本按上表字段号构造（含负向：篡改 commitment、重放、截断、超长、
   任意分片），并在 corpus-plan 中登记期望结果与错误码。
2. **真机语料后置**：抓包只能由用户执行；pcap 不入库（脱敏摘录才进 `evidence/`）。
3. **禁止**：把 R16 的表达、R19 的代码、任何 GMS 服务端依赖混入语料或实现。
4. 每个 fixture 必须能回答「它验证了表里的哪一行」——无出处的字节不得入库。

## E. 平台/许可

- 实现语言 Rust，第三方仅用成熟密码原语（P-256 ECDH、SHA-2、HKDF）——**不自写曲线/AES/HMAC 内部**。
- 不依赖 Google 账号或 GMS 服务端（本 profile 的 LAN 形态可无账号互通；账号相关字段不在本 profile）。

## F. 兼容矩阵

| 对端 | 状态 |
|---|---|
| stock Android（Pixel/Samsung）→ 本节点 | **not-run**（P-F02-1/2/3 关闭后按 catalog 验收） |
| Mac（R15 行为对照）→ 本节点 | not-run；R15 仅作事实来源，不作为对照运行时（不构建第三方代码） |

## G. 安全审查要点

- **发现 ≠ 认证**（T19-02）：QR/可见性只影响能否被发现；会话仍必须走完 UKEY2 并以确认码核对，
  否则不得交付任何 payload。实现里以「payload gate」强制（T20-04 断言）。
- **握手失败 ≠ 传输成功**（T19-01）：任何 alert、commitment 不符、确认码不符都必须停在
  `PairingFailed`/`Alert`，会话状态不得被记为 transfer 成功。
- 明文/未认证阶段不得建立任何文件写入路径；payload 只能由 T21 的加密层驱动。
- 抓包脱敏（SEC-08）：pcap 不入库。

## H. 决策

路线：**independent implementation**（Rust core），事实来自 R15/R17/R18/R19（R19 仅结构）。
冲突以「实现侧 + 具名真机实验」裁决（F-12/F-15）。重评条件：P-F02-1/2/3 全部关闭。
