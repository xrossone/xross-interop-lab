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
| F-13 | 能力 | `protocolInfo` 的具体 MIME 矩阵与品牌差异（三星/LG/Sony 各代）**未固化** | P-M07-1 未关闭 | blocked（impl 不准入） | no |
| F-14 | 时序 | `SetAVTransportURI` 之后 `Play` 的时序容忍度（品牌差异）**未固化** | P-M07-1 未关闭 | blocked（impl 不准入） | no |

## B. 与本仓实现的关系（T41 headless 切片）

- **实现**：F-01..F-12 中不依赖真机的部分——SSDP 编解码与按 USN 去重、**加固 XML 子集解析**
  （禁 DTD/实体、深度/体积/元素数预算）、SOAP 动作名与参数校验、DMC（发现 → 描述抓取策略 →
  protocolInfo 能力检查 → SetAVTransportURI/Play/Stop）、URL lease 撤销、受限 ContentDirectory
  （objectID 授权、分页上限、701/402）。
- **不实现**：F-13/F-14（真实 TV 矩阵与时序）——不得据此写"支持某品牌 TV"；GENA 只做到解析与预算，
  不实现回调投递；媒体字节传输属 T24/T42 的 read lease 范围。

## C. 能力分声明

| 能力 | 状态 | 障碍 / 重评条件 |
|---|---|---|
| SSDP 发现（解析请求/通告，构造响应） | `source-reviewed`（T41 实现） | 真机发现矩阵未做（P-M07-1） |
| DMC 控制（AVTransport 动作） | `source-reviewed`（T41 实现） | 真机 TV 时序与 protocolInfo 未验（P-M07-1） |
| 受限 DMS（ContentDirectory Browse） | `source-reviewed`（T41 实现） | 真机拉流未验（T42/T24） |
| **屏幕镜像（screen mirroring）** | `not-implemented` | DLNA 是"媒体 URL 推送"，**不假装镜像**（T41-02） |
| GENA 事件回调投递 | `not-implemented` | 只做解析/预算；NAT 与多接口行为待真机复核 |
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
