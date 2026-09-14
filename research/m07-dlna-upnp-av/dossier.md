```json xross-dossier
{
  "profile_id": "dlna.upnp-av",
  "catalog_id": "M07",
  "priority": "P1",
  "status": "catalogued",
  "capability_status": "not-implemented",
  "directions": {
    "receive": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r46-pupnp", "page-r47-platinum", "page-r49-rygel"],
      "notes": "DMR（renderer）：库存 app 推 URL/文件到本节点；sink 能力声明与 AVTransport 处理"
    },
    "send": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r46-pupnp", "page-r48-gerbera", "page-r47-platinum"],
      "notes": "DMC（controller）：把 URL/文件推到 TV；DMS（server）：只发布授权目录，独立开关"
    },
    "control": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r46-pupnp"],
      "notes": "SSDP 发现 + SOAP 控制 + GENA 事件回调三件套"
    }
  },
  "sources": [
    {"id": "R46", "name": "pupnp/pupnp", "commit": "2dfc2751cae1", "license": "BSD-3-Clause", "usage": "SSDP/SOAP/GENA 能力参考（XML/URL 安全单独做）"},
    {"id": "R47", "name": "plutinosoft/Platinum", "commit": "2674fa6b6f86", "license": "GPL/商业双轨（待核）", "usage": "DMC/DMR/DMS 三角色结构参考"},
    {"id": "R48", "name": "gerbera/gerbera", "commit": "1dc78d2230df", "license": "GPL-2.0", "usage": "DMS 内容目录与 TV 兼容 profiles 对照"},
    {"id": "R49", "name": "GNOME/rygel", "commit": "dc8a47dcbb7c", "license": "LGPL-2.1 为主", "usage": "DMR/DMS 的 GNOME 实现参考"}
  ],
  "unknown_fields": [
    {"field": "目标 TV 集合的 DLNA profile/codec 能力声明（protocolInfo 字符串矩阵）", "probe": "P-M07-1"},
    {"field": "SSDP/GENA 事件回调地址（CALLBACK 头）在 NAT/多接口环境的处理", "probe": "P-M07-2"},
    {"field": "SOAP ActionXML 的具体 schema 校验边界（XXE/深度/大小预算参数）", "probe": "P-M07-2"},
    {"field": "各 TV 品牌对 SetAVTransportURI 后 play 的时序容忍度", "probe": "P-M07-1"}
  ],
  "probes": [
    {"id": "P-M07-1", "question": "目标 TV 的 codec/protocolInfo 矩阵与控制时序？", "method": "lab TV（具名型号）DMC 推文件 + DMR 接库存 app URL（catalog 验收）", "status": "planned"},
    {"id": "P-M07-2", "question": "SSDP/GENA/SOAP 的安全预算参数怎么定？", "method": "source-review R46 解析器 + docs/07 §4 预算推导；恶意 SSDP/SOAP fixture fuzz", "status": "planned"}
  ]
}
```

# Protocol Dossier — dlna.upnp-av（M07）

## A. Scope

- **Profile**：DLNA / UPnP AV；三角色**分别声明**（CORE-02）：DMR（receive）、DMC（send/control）、DMS（server，只发布授权目录）。
- **系统入口**：电视/播放器的"DLNA 渲染"选择；手机相册 app 的"投放到 DLNA"。
- **场景**：U3（DMC 推文件/URL 到非 Xross 电视）；DMR 接库存 app；**排除**：实时屏幕镜像（非 WFD，不混同）、自动全盘共享为 DMS。
- **品牌辨析**：DLNA 认证已死但实现普遍存活；UPnP AV ≠ Cast ≠ AirPlay URL（M04/M08）。

## B. Sources and provenance

见机器头，commit 与 lock 一致（本组未深度 source-review → catalogued）。R47 双轨许可待核：在许可定论前只作结构参考，不复制表达。

## C. Wire layers

| 层 | 已知（catalogued） | 待证 |
|---|---|---|
| discovery | SSDP：UDP 239.255.255.250:1900，M-SEARCH/NOTIFY；设备/服务描述 XML（HTTP 取回） | 多接口/速率预算（P-M07-2） |
| 控制 | SOAP over HTTP POST（AVTransport/RenderingControl/ConnectionManager） | ActionXML schema 边界（P-M07-2） |
| 事件 | GENA：SUBSCRIBE + CALLBACK 回调 URL | NAT/多接口行为（P-M07-2） |
| payload | HTTP URL 拉取（TV 主动 GET 本节点 HTTP 资源）——media URL lease 模式（docs/04 §7 MediaResource） | range/格式细节（P-M07-1） |
| codec | 由对端 protocolInfo 声明决定；不无声转码（U3：找不到 codec 显式提示） | TV 矩阵（P-M07-1） |

## D. Message/state map

DMC 方向：discover(sink) → SetAVTransportURI → Play → （pause/seek/stop 按能力授权，MEDIA-06）→ 撤销令牌。DMR 方向：advertising → 接 SetAVTransportURI（offer 投影）→ 用户/策略裁决 → Play → 事件通知。字段级待 P-M07-2。

## E. Platform contract

- 无特殊平台权限（普通 UDP/HTTP 监听）；本地网络权限（macOS TCC）适用。
- DMS 需要目录读取授权 → 只走 scoped lease（FILE-08），不扫全盘。

## F. Compatibility matrix

| 对端 | 状态 |
|---|---|
| lab TV（具名型号待登记）DMC 推文件 | **not-run**（P-M07-1） |
| 库存 app → 本节点 DMR | **not-run** |

## G. Security review

docs/07/catalog 点名的风险面：
- **SSRF**：本节点生成的 media URL 与对端回调 URL 都要过 URL 策略（scheme/host/IP/授权，重定向逐跳复核——docs/07 §5；"允许 TV 访问我提供的 LAN URL"与"外来 URL 要求我访问内网"分开）。
- **XXE/XML 炸弹**：设备描述/SOAP 解析禁 DTD、深度/大小/时限预算（P-M07-2 定参）。
- **事件回调**：CALLBACK 头任意 URL → 只允许已批准 peer/subnet。
- SOAP/SSDP 均未认证输入：长度/深度/deadline 全预算化。

## H. Decision

**路线：independent implementation（Rust UPnP AV 栈），R46（BSD-3）为可合规复用候选库。** 三角色独立 feature gate（INT-08）。R47/R48/R48 只作事实/结构参考（GPL 边界 T05 正式化）。重评条件：P-M07-2 完成安全参数后可进实现排期。

## I. Implementation input release

- 允许：本 dossier、（P-M07-2 后的）specs-reviewed/m07、R46 库（合规复用评估后）。
- 禁止：R47/R48 表达复制（GPL 待核/确认）；真实家庭网络拓扑入库。
- 未关闭阻塞：P-M07-1/2。
- 验收 case：catalog M07（DMC 推文件到 TV；DMR 接库存 URL；DMS 只发布授权目录，三角色各测）。
- 负责 task：T15+ casting 分卷。
