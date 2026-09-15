# Reviewed Wire Spec — dlna.upnp-av（M07）

**状态**：字段级事实表已建立（2026-09-15）。**P-M07-1（真实 TV protocolInfo/时序矩阵）blocked**：
需库存 TV 与用户在场。**P-M07-2（解析预算）由本仓策略关闭**：预算值是我们自己的安全策略（不是协议事实），
定义在 `impl/crates/proto-upnp/src/xml.rs` 并有测试。

**红线**：状态 `blocked`/`待固化` 的行不得写进实现；本文件不复制第三方表达，只记字段名/常量/结构事实。

## A. 字段级事实表

| # | 层 | 事实 | 来源（commit+位置） | 状态 | impl-allowed |
|---|---|---|---|---|---|
| F-01 | discovery | SSDP 是 HTTP-over-UDP，端口 1900（`SSDP_PORT 1900`）；IPv4 组播地址由实现填入（`239.255.255.250`） | R46 2dfc275 `upnp/src/inc/ssdplib.h:81`；`upnp/src/ssdp/ssdp_server.c:986` | source-reviewed | yes |
| F-02 | discovery | 搜索请求形状：`M-SEARCH * HTTP/1.1`，`MAN: "ssdp:discover"`，`MX: <秒>`，`ST: <目标>`；服务端只接受 `NOTIFY`/`M-SEARCH` 两种方法 | R46 `upnp/src/ssdp/ssdp_ctrlpt.c:404-429,476-501`；`upnp/src/ssdp/ssdp_server.c:706` | source-reviewed | yes |
| F-03 | discovery | 存活/失效通告 `NTS: ssdp:alive` / `ssdp:byebye`；控制点按 NTS 更新设备存活 | R46 `upnp/src/ssdp/ssdp_device.c:688-691`；`upnp/src/ssdp/ssdp_ctrlpt.c:214-216` | source-reviewed | yes |
| F-04 | discovery | 通告/响应携带 `LOCATION`（设备描述 URL）、`ST`/`NT`、`USN`、`CACHE-CONTROL`；`ST: ssdp:all` 表示全部设备与服务；USN 形如 `<uuid>::upnp:rootdevice` | R46 `upnp/src/inc/ssdplib.h:289`；`upnp/src/ssdp/ssdp_server.c:632` | source-reviewed | yes |
| F-05 | control | 控制 = SOAP over HTTP POST；请求头 `SOAPACTION`（M-POST 变体为 `MAN` + `<nn>-SOAPACTION`）；服务端从该头取动作名 | R46 `upnp/src/soap/soap_device.c:538-602` | source-reviewed | yes |
| F-06 | control | AVTransport:1 服务类型字符串 `urn:schemas-upnp-org:service:AVTransport:1`；动作与参数：`SetAVTransportURI{InstanceID, CurrentURI, CurrentURIMetaData}`、`Play{InstanceID, Speed}`、`Seek{InstanceID, Unit, Target}`、`Stop{InstanceID}`、`GetTransportInfo{InstanceID}` | R49 dc8a47d `src/librygel-renderer/rygel-av-transport.vala:37`；`data/xml/AVTransport2.xml.in:10-33`（SetAVTransportURI）及 Play/Seek action 块 | source-reviewed | yes |
| F-07 | control | ContentDirectory:1 动作 `Browse{ObjectID, BrowseFlag, Filter, StartingIndex, RequestedCount, SortCriteria}` → `{Result, NumberReturned, TotalMatches, UpdateID}` | R49 `data/xml/ContentDirectory.xml.in`（Browse action 块） | source-reviewed | yes |
| F-08 | control | ContentDirectory 错误码：`NO_SUCH_OBJECT=701`、`CANT_PROCESS=720`、`INVALID_ARGS=402`（经 SOAP Fault 承载） | R49 `src/librygel-server/rygel-content-directory.vala:33,47,49,261-308` | source-reviewed | yes |
| F-09 | payload | 媒体按 URL 拉取：renderer 声明 `protocolInfo` 形如 `http-get:<附加>:<MIME>:<附加>`，协议集合含 `http-get` 与 `rtsp` | R49 `src/librygel-renderer-gst/rygel-playbin-player.vala:45,47` | source-reviewed | yes |
| F-10 | events | GENA：`SUBSCRIBE`/`UNSUBSCRIBE`，回调地址在 `CALLBACK` 头，订阅标识 `SID`，通知含 `SEQ`；服务端对订阅数有上限 | R46 `upnp/src/gena/gena_device.c:289-333,520-604,1511` | source-reviewed | yes |
| F-11 | device desc | 设备/服务描述是 XML（`<scpd>` + `specVersion`/`actionList`/`serviceStateTable`）；描述 URL 来自 SSDP 的 `LOCATION` | R49 `data/xml/AVTransport2.xml.in:1-8`；R46 的 LOCATION（F-04） | source-reviewed | yes |
| F-12 | xml | 描述/控制消息是任意 XML：DTD/外部实体必须禁（XXE）；深度/体积/元素数/属性数预算属**本仓策略**（P-M07-2），不是协议常量 | 本仓策略（`impl/crates/proto-upnp/src/xml.rs` 常量与测试）；威胁面参考 R46 `upnp/src/gena/gena_device.c:1511`（订阅洪泛） | source-reviewed（策略由本仓定义） | yes |
| F-15 | control | AVTransport `TransportState` 允许值：`STOPPED`/`PAUSED_PLAYBACK`/`PAUSED_RECORDING`/`PLAYING`/`RECORDING`/`TRANSITIONING`/`NO_MEDIA_PRESENT`；`TransportStatus`：`OK`/`ERROR_OCCURRED` | R49 dc8a47d `data/xml/AVTransport2.xml.in:445-451,459-460` | source-reviewed | yes |
| F-16 | control | AVTransport 错误码语义：`701` = Transition not available、`710` = Seek mode not supported、`711` = Illegal seek target、`712` = Play mode not supported | R49 `src/librygel-renderer/rygel-av-transport.vala:481,499,529,535,554,560,573,584,598,610,659` | source-reviewed | yes |
| F-17 | control | `Seek` 的 `Unit` 允许值：`ABS_TIME`/`REL_TIME`/`TRACK_NR`/`ABS_COUNT`/`REL_COUNT`/`X_DLNA_REL_BYTE`；`TransportPlaySpeed` 是自由字符串（默认 `1`，无允许值列表） | R49 `data/xml/AVTransport2.xml.in:716-725,595-598` | source-reviewed（本仓只实现 `REL_TIME`，其余 → 710） | yes |
| F-18 | control | RenderingControl 动作集：`ListPresets`/`SelectPreset`/`GetMute`/`SetMute`/`GetVolume`/`SetVolume`；`Volume` 为 `ui2`、范围 0..100、步长 1 | R49 `data/xml/RenderingControl2.xml.in:10,26,42,63,143-149` | source-reviewed | yes |
| F-19 | control | AVTransport 状态变量：`CurrentTransportActions`(string)、`CurrentTrackURI`、`CurrentMediaDuration`、`NumberOfTracks`(ui4, 0..512) | R49 `data/xml/AVTransport2.xml.in:475-477,632-635,656-668` | source-reviewed | yes |
| F-20 | control | 服务类型串：AVTransport 声明 `:2` 与 `:1` 两个版本；RenderingControl 为 `:2` | R49 `src/librygel-renderer/rygel-av-transport.vala:35,37`、`src/librygel-renderer/rygel-rendering-control.vala:30` | source-reviewed | yes |
| F-21 | policy | renderer 侧对外来 `CurrentURI` 的策略（只接受 http(s)；`file://` 与内网/元数据目标拒绝；改动 URI 需用户同意）与 GENA 回调地址策略（同一类检查）属**本仓策略**——字段表只固定字段形状（F-04/F-10），没有规定策略阈值 | 本仓策略（引用 F-04、F-10 与 T42-01/T42-04） | source-reviewed（策略由本仓定义） | yes |
| F-22 | events | **GENA 回调连接的建立不实现**（范围声明）：订阅校验与通知构造都在本仓，但**不自己建连接**——通知字节交给调用方提供的传输（路由/NAT/多接口行为不归本仓）；`notify` 事件只做形状与转义规则（见 F-23..F-29） | 本仓范围声明（F-10 只固定头与上限；F-23..F-29 固定通知字节） | source-reviewed（本仓范围：构造在本仓、连接不在本仓） | yes |
| F-23 | events | NOTIFY 请求形状（**设备侧**）：请求行 `NOTIFY <回调路径> HTTP/1.1`；头 `Content-Type: text/xml; charset="utf-8"`、`Content-Length: <正文字节数 + 2>`（**+2 是来源的既有行为**：正文以 `\n\n` 结尾）、`NT: upnp:event`、`NTS: upnp:propchange`，随后每条订阅追加 `SID: uuid:<...>` 与 `SEQ: <n>` | R46 `upnp/src/gena/gena_device.c:440-482`（头）、`:304-314`（SID/SEQ）、`:1569-1571`（SID 形式 `uuid:%s`） | source-reviewed（T42+ gate；**只构造不建连**） | yes |
| F-24 | events | propertyset 正文：`<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">\n` + 每个变量 `<e:property>\n<NAME>VALUE</NAME>\n</e:property>\n` + `</e:propertyset>\n\n`；**不发送 XML 声明**——来源把 `XML_VERSION` 宏留着但因为"与其他 UPnP 厂商不互操作"而明确不发送 | R46 `upnp/src/inc/gena.h:58-63`（宏与注释）、`upnp/src/gena/gena_device.c:110-165`（构造与注释） | source-reviewed（T42+ gate；**本仓按来源不写声明**） | yes |
| F-25 | events | **值的转义由构造值的一方负责**：来源的 propertyset 构造把 `values[counter]` **原样** `sprintf` 进正文（不做任何转义）⇒ 生产值的一方必须先转义，否则正文非法。LastChange 的值就是一份**内嵌 XML 文档**，因此它必须整体被转义后放进 `<LastChange>…</LastChange>` | R46 `upnp/src/gena/gena_device.c:148-158`（原样写入） | source-reviewed（T42+ gate） | yes |
| F-26 | events | LastChange 的内嵌文档形状（**服务侧**）：`<Event xmlns="<服务命名空间>"><InstanceID val="0">` + 每个变量 `<VAR val="<已转义值>"/>` + `</InstanceID></Event>`；带通道的变量为 `<VAR val=".." channel=".."/>`；AVTransport 命名空间 `urn:schemas-upnp-org:metadata-1-0/AVT/`、RenderingControl 为 `urn:schemas-upnp-org:metadata-1-0/RCS/` | R49 `src/librygel-renderer/rygel-changelog.vala:78-118`（`log`/`log_with_channel`/`finish`）、`rygel-av-transport.vala:40`、`rygel-rendering-control.vala:33` | source-reviewed（T42+ gate） | yes |
| F-27 | events | LastChange 是**合并后**发出的：来源用一个 **150 ms** 的延迟窗口把短时间内的多次变量变化合并成一条通知（`Timeout.add(150, …)`） | R49 `src/librygel-renderer/rygel-changelog.vala:63-76` | source-reviewed（**150 ms 是来源实现取值**，不是规范常量；本仓投递由调用方驱动、窗口可配并如实标注） | yes |
| F-28 | events | SEQ 规则：新订阅的 `SEQ` **从 0 开始**（初始事件），每次投递后 +1；来源在自增后若为负则**回绕到 1**（即 0 只用于初始事件） | R46 `upnp/src/gena/gena_device.c:1491`（初始 0）、`:409-412`（自增与回绕） | source-reviewed（T42+ gate） | yes |
| F-29 | events | 控制点侧对通知的校验：`NT` 必须是 `upnp:event`、`NTS` 必须是 `upnp:propchange`（大小写不敏感的 `memptr_cmp`），SUBSCRIBE 请求里用 `NT: upnp:event` | R46 `upnp/src/gena/gena_ctrlpt.c:819-820`、`:408,426` | source-reviewed（T42+ gate） | yes |
| F-13 | 能力 | `protocolInfo` 的具体 MIME 矩阵与品牌差异（三星/LG/Sony 各代）**未固化** | P-M07-1 未关闭 | blocked（impl 不准入） | no |
| F-14 | 时序 | `SetAVTransportURI` 之后 `Play` 的时序容忍度（品牌差异）**未固化** | P-M07-1 未关闭 | blocked（impl 不准入） | no |

## B. 与本仓实现的关系（T41 headless 切片）

- **实现**：F-01..F-12 中不依赖真机的部分——SSDP 编解码与按 USN 去重、**加固 XML 子集解析**
  （禁 DTD/实体、深度/体积/元素数预算）、SOAP 动作名与参数校验、DMC（发现 → 描述抓取策略 →
  protocolInfo 能力检查 → SetAVTransportURI/Play/Stop）、URL lease 撤销、受限 ContentDirectory
  （objectID 授权、分页上限、701/402）。
- **不实现**：F-13/F-14（真实 TV 矩阵与时序）——不得据此写"支持某品牌 TV"；**GENA 不建立回调连接**（通知构造已实现，见 T42+），
  不实现回调投递；媒体字节传输属 T24/T42 的 read lease 范围。

## C. 能力分声明

| 能力 | 状态 | 障碍 / 重评条件 |
|---|---|---|
| SSDP 发现（解析请求/通告，构造响应） | `source-reviewed`（T41 实现） | 真机发现矩阵未做（P-M07-1） |
| DMC 控制（AVTransport 动作） | `source-reviewed`（T41 实现） | 真机 TV 时序与 protocolInfo 未验（P-M07-1） |
| 受限 DMS（ContentDirectory Browse） | `source-reviewed`（T41 实现） | 真机拉流未验（T42/T24） |
| DLNA renderer（接收 AVTransport/RenderingControl 动作） | `source-reviewed`（T42 实现） | 真机控制器（库存 TV/手机 App）未验（P-M07-1） |
| **屏幕镜像（screen mirroring）** | `not-implemented` | DLNA 是"媒体 URL 推送"，**不假装镜像**（T41-02） |
| GENA 订阅校验（头/回调策略/退订/上限） | `source-reviewed`（T42 实现） | 真机控制器订阅行为未验（P-M07-1） |
| GENA 通知**构造与调度**（NOTIFY 字节、propertyset、LastChange 转义、SEQ 规则） | `source-reviewed`（实现见 T42+） | 字节形状按 F-23..F-29；**真机控制器是否接受**待 P-M07-1 |
| GENA 回调**连接**（自己发起 HTTP） | `not-implemented` | 范围声明（F-22）：通知交给调用方传输，本仓不建连接 |
| 真实 TV 兼容矩阵 | `blocked` | P-M07-1：需库存 TV 与用户在场 |

## D. 语料规则（`evidence/dlna/`）

1. 全部 fixture 自制（SSDP 报文、SOAP 信封、设备描述 XML 由测试构造）；
2. 负向必须含：DTD/XXE、超深 XML、超长字段、SOAPACTION 与服务类型不符、未知动作、越权 objectID、
   指向内网/元数据地址的描述 URL、未知 codec；
3. 真机语料（抓包/真机 log）后置、由用户执行；pcap 不入库。

## E. 平台/许可

- 事实来源：R46 pupnp（BSD-3-Clause）、R49 rygel（LGPL-2.1）——**只取字段名/常量/结构事实**，不复制表达；
  R47 Platinum（GPL/商业双轨）、R48 gerbera（GPL-2.0）本轮**未使用**。
- 实现不链接任何第三方 UPnP 栈；XML 解析为自研最小子集（禁 DTD/实体）。

## F. 兼容矩阵

| 对端 | 状态 |
|---|---|
| 库存 TV → 本节点（DMS/DMR 侧） | **not-run**（需 P-M07-1 与真机） |
| 本节点 → 真实 TV 推流（DMC 侧） | not-run（同上） |

## G. 安全审查要点

- 设备描述 URL 抓取必须过策略：只允许 http、无 userinfo、无 fragment，目标不得是 loopback/link-local/
  组播/云元数据地址（T41-01）；本轮不实现重定向跟随。
- XML 禁 DTD/外部实体（XXE）；深度/体积/元素数/属性数均有上限（P-M07-2 预算）。
- SOAPACTION 的服务类型必须与本节点提供的服务一致；未知动作 → `unsupported-feature`（不做"尽力解析"）。
- 媒体 URL lease 必须可撤销：Stop/取消/超时都撤销（T41-04），不允许"停止后 URL 仍可取"。
- ContentDirectory 只暴露批准目录；越权 objectID → 701（T41-03）。

## H. 决策

路线：**independent implementation**（Rust core），事实来自 R46/R49（只取字段）。重评条件：P-M07-1 关闭
（真机矩阵）后按 catalog 执行 device 级验证。
