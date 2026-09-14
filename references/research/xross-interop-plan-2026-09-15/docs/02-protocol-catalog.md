# 协议与互操作 profile 总目录

共 **38 个 profile**：17 个文件/同步方向，21 个媒体/屏幕/音频/控制方向。它们不是38套必须从零编写的协议栈：有的是系统API、应用协议、已有栈的不同角色或标准媒体载体。品牌UI、发现、传输、安全与媒体格式分别研究。

## 优先级含义

- P0：第一条可用纵向闭环。
- P1：紧接着交付、性价比高的原生入口或电视能力。
- P2：核心扩展，先把对应平台/方向probe通过。
- P3：后续标准或专业工作流，不阻塞Xross主体。
- Research：有明确调查问题/证据退出条件；可交付blocked dossier。
- Vendor：授权/SDK前置，不以逆向猜测代替合作条件。

同一个profile可能receiver先P0、sender后P2。所有条目当前状态都是catalogued，绝不表示Xross已支持。

## 总览

| ID | Profile | 角色 | 优先级 | 首要边界 |
|---|---|---|---|---|
| F01 | LocalSend v2.x | send, receive | P0 | 不要混入未确认的 v3；不可把 UDP discovery 改成仅 mDNS |
| F02 | Google Quick Share LAN/QR | send, receive | P1 | 发现机制和 stock Android 状态影响方向；global Quick Share不等于中国互传 |
| F03 | Quick Share BLE/P2P transport upgrade | send, receive | P2 | BLE central库不代表能发任意广播；macOS公开P2P限制；链路升级授权 |
| F04 | Apple AirDrop | send, receive | Research | AWDL、系统版本、可见模式、身份和许可；不承诺 contacts-only；不依赖私有密钥滥用 |
| F05 | Windows Nearby Sharing / NearShare | send, receive | P2 | 和Quick Share完全不同；身份/BT/LAN与Windows版本需固定 |
| F06 | 互传联盟 / MDFE | send, receive | Research | 本轮未找到完整公开wire spec+可复用library；必须先通过资料/授权 gate |
| F07 | Huawei Share / Share Engine | send, receive | Vendor | 旧EMUI文档不代表新鸿蒙；SDK申请、分发许可、硬件与平台支持 |
| F08 | KDE Connect Share | send, receive | P2 | 配对身份与Xross隔离；GPL组合边界；不要打开runcommand/input |
| F09 | Bluetooth OBEX / OPP | send, receive | P3 | 移动OS公开API和设备profile差异；不是所有iPhone支持OPP |
| F10 | Browser HTTPS / QR share | serve, upload, download | P1 | 浏览器安全上下文/TLS信任/CORS/CSRF；不能绕过确认 |
| F11 | WebDAV | client, server | P3 | 不是nearby协议；写/锁/属性语义复杂；必须沿用host ACL |
| F12 | SFTP | client, server | P3 | SFTP版本和host-key策略；server是新增外部网络暴露需专门授权 |
| F13 | SMB3 | client, server | Research | 签名/加密/认证与挂载权限；Windows服务冲突；无匿名全盘 |
| F14 | Magic Wormhole | send, receive | P3 | 对端需Wormhole；rendezvous/relay与版本不是Xross同一身份 |
| F15 | croc | send, receive | P3 | 独立产品协议，不是原生系统菜单；中继与升级耦合 |
| F16 | PairDrop / Snapdrop deployment | send, receive | P3 | 页面/服务器版本是互通契约；WebRTC标准本身不定义文件分享信令 |
| F17 | Syncthing BEP interoperability | peer | Research | continuous sync不是单次offer；双写/conflict/删除语义不同 |
| M01 | AirPlay legacy screen mirroring | receive, send | P0→P2 | FairPlay/来源；receiver≠sender；不声称DRM；codec/旋转依赖真机 |
| M02 | AirPlay classic audio / RAOP | receive, send | P2 | 音频加密与访问认证要分开；多房间不是自动能力 |
| M03 | AirPlay 2 audio | receive, send | P2 | HomeKit pairing/时钟/多房间状态；不把README自测当产品证明 |
| M04 | AirPlay media URL/HLS/photo | receive, send | P2 | YouTube受支持不代表全部网页视频/DRM；照片与视频不同模式 |
| M05 | Miracast / Wi-Fi Display | receive, send | P1→P2 | WiFiP2P/驱动/用户会话；Mac/普通手机App权限；source不从MiracleCast误推 |
| M06 | Miracast over infrastructure / MS-MICE | receive, send | P2 | 仍有无线初始发现约束；不是纯LAN万能替代 |
| M07 | DLNA / UPnP AV | renderer, controller, server | P1 | 非实时screen；电视codec/DLNA profiles；SSRF/XXE/事件回调风险 |
| M08 | Google Cast media control / URL sender | send, control | P1 | sender controller不自动拥有本地media服务；codec与app ID校验 |
| M09 | Google Cast real-time streaming source | send | P2 | 设备认证/receiver app/codec/capture；不是M08多一个URL字段 |
| M10 | Google Cast-compatible generic receiver | receive | Research | 自有demo证书成功不代表Pixel/Chrome承认；授权/设备认证是gate |
| M11 | Huawei Cast+ / Cast Engine | receive, send | Vendor | 商业权限/设备代际/SDK受支持平台 |
| M12 | OpenHarmony CastEngine integration | receive, send | Research | OpenHarmony系统集成与普通HarmonyOS App是不同部署；不自动互通华为商用 |
| M13 | RTSP / RTP / MPEG-TS | publish, subscribe, serve | P2 | RTSP1/2和ANNOUNCE/RECORD支持分开；RTP无加密时必须披露 |
| M14 | WebRTC / WHIP / WHEP | publish, subscribe | P2 | WebRTC没有单一信令；WHEP按具体规范/实现版本，不能冒称已同样成为RFC |
| M15 | SRT | caller, listener, rendezvous | P3 | 媒体容器/加密/流ID与会话权限独立；不是投屏菜单发现协议 |
| M16 | RTMP / RTMPS | publish, receive | P3 | RTMPS与明文显式选择；enhanced codecs不默认支持 |
| M17 | HLS delivery / playback | serve, play | P2 | 不是设备发现协议；buffered高延迟不当交互镜像baseline |
| M18 | scrcpy / authorized ADB | receive, control | P3 | 设备debugging授权和版本匹配；控制另授权 |
| M19 | GameStream / Sunshine / Moonlight | host, client | P3 | 输入、编码、配对与安全大范围；不阻塞文件/基础投屏 |
| M20 | NDI | send, receive | Vendor | 专有SDK与分发条件；本包没有已核可直接克隆实现 |
| M21 | DIAL / Tizen / webOS / Roku control | discover, launch, control | Research | 发现/launch app不等于media或screen流；不假定控制权限 |

## 各 profile 的进入条件与验证目标

### F01 · LocalSend v2.x

**实现路线：** 复用 Xross 现有实现；只抽边界和补规范/回归。

**不做的假设：** 不要混入未确认的 v3；不可把 UDP discovery 改成仅 mDNS。

**首个可判定的probe/验收：** 库存 macOS/Android/Windows 客户端各双向，拒绝、部分选择、取消、Unicode与同名处理。

**资料：** [R13](03-reference-projects.md), [R14](03-reference-projects.md)。

### F02 · Google Quick Share LAN/QR

**实现路线：** NearDrop/Bada/google Nearby 对照；独立 Rust core 或合法外部 provider。

**不做的假设：** 发现机制和 stock Android 状态影响方向；global Quick Share不等于中国互传。

**首个可判定的probe/验收：** Android→Mac 收件；Mac→Android二维码/显式可发现；对端确认码一致，10GiB流式受控传输。

**资料：** [R15](03-reference-projects.md), [R16](03-reference-projects.md), [R17](03-reference-projects.md), [R18](03-reference-projects.md), [R19](03-reference-projects.md), [R20](03-reference-projects.md)。

### F03 · Quick Share BLE/P2P transport upgrade

**实现路线：** F02 已稳定后增加平台 radio backend，保持同一 transfer。

**不做的假设：** BLE central库不代表能发任意广播；macOS公开P2P限制；链路升级授权。

**首个可判定的probe/验收：** Wi-Fi LAN→P2P阶段切换；失败回退不丢/重写文件；无线调度不影响当前联网。

**资料：** [R16](03-reference-projects.md), [R17](03-reference-projects.md), [R19](03-reference-projects.md), [R63](03-reference-projects.md)。

### F04 · Apple AirDrop

**实现路线：** OpenDrop/owl 只读研究；来源清楚的协议事实 +当前真机验证。

**不做的假设：** AWDL、系统版本、可见模式、身份和许可；不承诺 contacts-only；不依赖私有密钥滥用。

**首个可判定的probe/验收：** 获用户批准的可发现模式下明确版本双向；确认对话、重名与大小限制；不可用输出 blocker。

**资料：** [R21](03-reference-projects.md), [R22](03-reference-projects.md)。

### F05 · Windows Nearby Sharing / NearShare

**实现路线：** 读取 MS-CDP + Android 实现；独立适配到 Offer。

**不做的假设：** 和Quick Share完全不同；身份/BT/LAN与Windows版本需固定。

**首个可判定的probe/验收：** Windows原生分享→独立节点，反向也需独立证据；拒绝与不受信任对端。

**资料：** [R23](03-reference-projects.md), [S28](03-reference-projects.md)。

### F06 · 互传联盟 / MDFE

**实现路线：** 厂商公开资料 +受控设备观察 +合作申请。

**不做的假设：** 本轮未找到完整公开wire spec+可复用library；必须先通过资料/授权 gate。

**首个可判定的probe/验收：** 至少两品牌中国ROM相互基线成功，再研究Xross端；任何未知字段标未证实。

**资料：** [S14](03-reference-projects.md)。

### F07 · Huawei Share / Share Engine

**实现路线：** 合法SDK适配独立worker；开源实现仅另行许可通过后研究。

**不做的假设：** 旧EMUI文档不代表新鸿蒙；SDK申请、分发许可、硬件与平台支持。

**首个可判定的probe/验收：** 取得SDK哈希/授权；Huawei手机→Win/Linux测试，Mac与新鸿蒙分别probe。

**资料：** [S11](03-reference-projects.md), [S12](03-reference-projects.md), [S13](03-reference-projects.md)。

### F08 · KDE Connect Share

**实现路线：** 有限协议子集/合法daemon bridge；只share plugin。

**不做的假设：** 配对身份与Xross隔离；GPL组合边界；不要打开runcommand/input。

**首个可判定的probe/验收：** KDE库存客户端与Xross双向，pair/revoke，文本/URL/文件，不可调用其他插件。

**资料：** [R24](03-reference-projects.md), [R25](03-reference-projects.md)。

### F09 · Bluetooth OBEX / OPP

**实现路线：** 调用系统OBEX服务优先；Rust仅封装契约。

**不做的假设：** 移动OS公开API和设备profile差异；不是所有iPhone支持OPP。

**首个可判定的probe/验收：** Android↔Linux/受支持Windows的公开路径；拒绝认证/文件名攻击；低速显示真实。

**资料：** [R61](03-reference-projects.md), [S21](03-reference-projects.md)。

### F10 · Browser HTTPS / QR share

**实现路线：** 复用Xross现有HTTP gateway；临时资源grant和受限网页。

**不做的假设：** 浏览器安全上下文/TLS信任/CORS/CSRF；不能绕过确认。

**首个可判定的probe/验收：** 陌生浏览器扫码下载一文件；上传受限目录；过期和撤销后403/410；Range版本一致。

**资料：** [S27](03-reference-projects.md), [R13](03-reference-projects.md)。

### F11 · WebDAV

**实现路线：** 只读优先适配Xross Remote Files；成熟库选型单独审计。

**不做的假设：** 不是nearby协议；写/锁/属性语义复杂；必须沿用host ACL。

**首个可判定的probe/验收：** 库存WebDAV client只看授权root；PROPFIND深度/大目录/Range/凭据错误边界。

**资料：** [S25](03-reference-projects.md)。

### F12 · SFTP

**实现路线：** 复用Xross已有SSH/SFTP引擎，禁止再造凭据库。

**不做的假设：** SFTP版本和host-key策略；server是新增外部网络暴露需专门授权。

**首个可判定的probe/验收：** 已知host key双向文件流；拒绝改变host key；shell权限不能因SFTP自动启用。

**资料：** [S30](03-reference-projects.md)。

### F13 · SMB3

**实现路线：** 优先系统/成熟服务集成，不从零实现SMB安全栈。

**不做的假设：** 签名/加密/认证与挂载权限；Windows服务冲突；无匿名全盘。

**首个可判定的probe/验收：** 只授权一个share，验证authentication/signing与路径隔离；无SMB1 fallback。

**资料：** [S29](03-reference-projects.md)。

### F14 · Magic Wormhole

**实现路线：** 合法Rust/Python库或CLI provider。

**不做的假设：** 对端需Wormhole；rendezvous/relay与版本不是Xross同一身份。

**首个可判定的probe/验收：** 与库存CLI双方输入短码；错误码/中断/relay不可用；不记录PAKE秘密。

**资料：** [R28](03-reference-projects.md), [R29](03-reference-projects.md)。

### F15 · croc

**实现路线：** 独立Go CLI/provider做互通优先。

**不做的假设：** 独立产品协议，不是原生系统菜单；中继与升级耦合。

**首个可判定的probe/验收：** 库存croc双向，密码错/中断重试行为如实映射；显式服务配置。

**资料：** [R30](03-reference-projects.md)。

### F16 · PairDrop / Snapdrop deployment

**实现路线：** 可选网页/信令provider，不加入默认daemon。

**不做的假设：** 页面/服务器版本是互通契约；WebRTC标准本身不定义文件分享信令。

**首个可判定的probe/验收：** 指定版本服务器和浏览器双向；隔离房间/拒绝/过期；不广播所有Xross设备。

**资料：** [R26](03-reference-projects.md), [R27](03-reference-projects.md)。

### F17 · Syncthing BEP interoperability

**实现路线：** 仅建立协议/商业边界研究；复用成熟实现优于重做sync。

**不做的假设：** continuous sync不是单次offer；双写/conflict/删除语义不同。

**首个可判定的probe/验收：** 先dossier与scope裁决；不得把Syncthing数据库当Xross/Shelf authority。

**资料：** [R31](03-reference-projects.md)。

### M01 · AirPlay legacy screen mirroring

**实现路线：** 先外部UxPlay接收基线；Rust receiver独立合法路线；sender另开子任务。

**不做的假设：** FairPlay/来源；receiver≠sender；不声称DRM；codec/旋转依赖真机。

**首个可判定的probe/验收：** iPhone/iPad/Mac→PC受控画面声音；20次重连；反向投AppleTV另验收。

**资料：** [R01](03-reference-projects.md), [R02](03-reference-projects.md), [R03](03-reference-projects.md), [R04](03-reference-projects.md), [R08](03-reference-projects.md), [R09](03-reference-projects.md), [R10](03-reference-projects.md)。

### M02 · AirPlay classic audio / RAOP

**实现路线：** 库/sidecar互通；沿用平台音频后端。

**不做的假设：** 音频加密与访问认证要分开；多房间不是自动能力。

**首个可判定的probe/验收：** 库存sender→null/native audio；反向→已授权AirPlay sink，采样率切换。

**资料：** [R02](03-reference-projects.md), [R05](03-reference-projects.md), [R07](03-reference-projects.md), [R11](03-reference-projects.md), [R12](03-reference-projects.md)。

### M03 · AirPlay 2 audio

**实现路线：** shairplay-rust/Shairport Sync对照，buffered/realtime分profile。

**不做的假设：** HomeKit pairing/时钟/多房间状态；不把README自测当产品证明。

**首个可判定的probe/验收：** 具名设备persistent/transient各测；音画/多声道profile另验收。

**资料：** [R02](03-reference-projects.md), [R07](03-reference-projects.md), [R10](03-reference-projects.md)。

### M04 · AirPlay media URL/HLS/photo

**实现路线：** URL session + host FetchPolicy；兼容app逐项列。

**不做的假设：** YouTube受支持不代表全部网页视频/DRM；照片与视频不同模式。

**首个可判定的probe/验收：** 自有合法HLS/静态照片；重定向/语言轨/停止；恶意file://和私网跳转失败。

**资料：** [R01](03-reference-projects.md), [R10](03-reference-projects.md), [R11](03-reference-projects.md)。

### M05 · Miracast / Wi-Fi Display

**实现路线：** Windows native receiver优先；Linux source/sink与Rust WFD core分别probe。

**不做的假设：** WiFiP2P/驱动/用户会话；Mac/普通手机App权限；source不从MiracleCast误推。

**首个可判定的probe/验收：** Samsung/Huawei/Win+K→Windows native source；Linux收发分别；无网卡时清晰不可用。

**资料：** [R32](03-reference-projects.md), [R33](03-reference-projects.md), [R34](03-reference-projects.md), [R37](03-reference-projects.md), [R41](03-reference-projects.md), [S04](03-reference-projects.md), [S05](03-reference-projects.md), [S24](03-reference-projects.md)。

### M06 · Miracast over infrastructure / MS-MICE

**实现路线：** 在M05后增加Microsoft公开扩展适配。

**不做的假设：** 仍有无线初始发现约束；不是纯LAN万能替代。

**首个可判定的probe/验收：** 受支持Windows与sink采用基础设施路径；拒绝证书/路由失败按契约回退。

**资料：** [S06](03-reference-projects.md)。

### M07 · DLNA / UPnP AV

**实现路线：** SSDP+SOAP+event+AVTransport；DMC/DMR/DMS单独能力。

**不做的假设：** 非实时screen；电视codec/DLNA profiles；SSRF/XXE/事件回调风险。

**首个可判定的probe/验收：** DMC推文件到TV；DMR接库存app URL；DMS只发布授权目录，三种角色各测。

**资料：** [R46](03-reference-projects.md), [R47](03-reference-projects.md), [R48](03-reference-projects.md), [R49](03-reference-projects.md)。

### M08 · Google Cast media control / URL sender

**实现路线：** libcast/pychromecast对照；合法CAF/默认media receiver。

**不做的假设：** sender controller不自动拥有本地media服务；codec与app ID校验。

**首个可判定的probe/验收：** Xross选择本地文件→有限HTTP URL→Chromecast；load/play/pause/seek/stop。

**资料：** [R42](03-reference-projects.md), [R43](03-reference-projects.md), [R44](03-reference-projects.md), [R45](03-reference-projects.md), [S07](03-reference-projects.md), [S08](03-reference-projects.md), [S09](03-reference-projects.md)。

### M09 · Google Cast real-time streaming source

**实现路线：** Open Screen streaming sender基线；capture/encode独立。

**不做的假设：** 设备认证/receiver app/codec/capture；不是M08多一个URL字段。

**首个可判定的probe/验收：** 桌面测试图+声音→指定Cast receiver；分辨率与拥塞，浏览器回环不算TV通过。

**资料：** [R42](03-reference-projects.md)。

### M10 · Google Cast-compatible generic receiver

**实现路线：** Open Screen receiver demo+stock sender认证probe。

**不做的假设：** 自有demo证书成功不代表Pixel/Chrome承认；授权/设备认证是gate。

**首个可判定的probe/验收：** 先自有demo互通，再stock Pixel/Chrome→PC；分别记录认证、launch、媒体三个阶段。

**资料：** [R42](03-reference-projects.md), [R43](03-reference-projects.md), [R45](03-reference-projects.md)。

### M11 · Huawei Cast+ / Cast Engine

**实现路线：** 官方SDK可用且授权通过时wrapper；不伪造设备认证。

**不做的假设：** 商业权限/设备代际/SDK受支持平台。

**首个可判定的probe/验收：** SDK许可与运行matrix通过后华为手机→合法receiver；反向独立验证。

**资料：** [S10](03-reference-projects.md), [R39](03-reference-projects.md)。

### M12 · OpenHarmony CastEngine integration

**实现路线：** 开放系统源码的编译/依赖probe；native provider。

**不做的假设：** OpenHarmony系统集成与普通HarmonyOS App是不同部署；不自动互通华为商用。

**首个可判定的probe/验收：** 定制OpenHarmony设备闭环；再尝试普通App API，失败保留system-only标记。

**资料：** [R37](03-reference-projects.md), [R38](03-reference-projects.md), [R39](03-reference-projects.md), [R40](03-reference-projects.md)。

### M13 · RTSP / RTP / MPEG-TS

**实现路线：** 标准stream adapter；GStreamer/MediaMTX/gortsplib对照。

**不做的假设：** RTSP1/2和ANNOUNCE/RECORD支持分开；RTP无加密时必须披露。

**首个可判定的probe/验收：** VLC/GStreamer指定codec双向；丢包/RTCP/序号回绕/会话断开与恢复。

**资料：** [R50](03-reference-projects.md), [R51](03-reference-projects.md), [R52](03-reference-projects.md), [R53](03-reference-projects.md), [S26](03-reference-projects.md)。

### M14 · WebRTC / WHIP / WHEP

**实现路线：** 优先复用Xross选定WebRTC栈；WHIP按RFC9725。

**不做的假设：** WebRTC没有单一信令；WHEP按具体规范/实现版本，不能冒称已同样成为RFC。

**首个可判定的probe/验收：** 浏览器与daemon互通；WHIP POST/PATCH/DELETE；TURN失败/证书/ICE restart。

**资料：** [R50](03-reference-projects.md), [R54](03-reference-projects.md), [S22](03-reference-projects.md)。

### M15 · SRT

**实现路线：** libsrt合法FFI/provider；默认仅caller/listener。

**不做的假设：** 媒体容器/加密/流ID与会话权限独立；不是投屏菜单发现协议。

**首个可判定的probe/验收：** OBS/GStreamer对端MPEGTS；延迟窗口/丢包/口令错/重连。

**资料：** [R50](03-reference-projects.md), [R55](03-reference-projects.md)。

### M16 · RTMP / RTMPS

**实现路线：** 合法媒体服务provider优先。

**不做的假设：** RTMPS与明文显式选择；enhanced codecs不默认支持。

**首个可判定的probe/验收：** OBS发布到受限stream key，Xross输出到测试服务器；密钥错与停止清理。

**资料：** [R50](03-reference-projects.md), [R59](03-reference-projects.md), [R60](03-reference-projects.md)。

### M17 · HLS delivery / playback

**实现路线：** 媒体URL能力复用manifest/segment/token服务。

**不做的假设：** 不是设备发现协议；buffered高延迟不当交互镜像baseline。

**首个可判定的probe/验收：** 自有HLS播放器与服务；seek/live窗口/语言轨/子请求授权。

**资料：** [R50](03-reference-projects.md), [R52](03-reference-projects.md), [R60](03-reference-projects.md)。

### M18 · scrcpy / authorized ADB

**实现路线：** 独立scrcpy provider，不归入零安装原生投屏。

**不做的假设：** 设备debugging授权和版本匹配；控制另授权。

**首个可判定的probe/验收：** 用户确认ADB后显示Android画面；撤销调试立即断开；read-only profile无输入。

**资料：** [R56](03-reference-projects.md)。

### M19 · GameStream / Sunshine / Moonlight

**实现路线：** 高性能后续provider，GPL边界清楚。

**不做的假设：** 输入、编码、配对与安全大范围；不阻塞文件/基础投屏。

**首个可判定的probe/验收：** 库存Sunshine/Moonlight互通，视频与输入分别授权，游戏优化另里程碑。

**资料：** [R57](03-reference-projects.md), [R58](03-reference-projects.md)。

### M20 · NDI

**实现路线：** 先获取SDK/条款/再适配，不承诺纯Rust wire。

**不做的假设：** 专有SDK与分发条件；本包没有已核可直接克隆实现。

**首个可判定的probe/验收：** SDK许可、Linux/Mac/Win兼容、video/audio格式和网络范围证据。

**资料：** [S31](03-reference-projects.md)。

### M21 · DIAL / Tizen / webOS / Roku control

**实现路线：** 将具体厂商profile作为辅助控制，逐一API研究。

**不做的假设：** 发现/launch app不等于media或screen流；不假定控制权限。

**首个可判定的probe/验收：** 只在受支持型号launch指定测试app；未经pair不可调用控制；不伪装mirror支持。

**资料：** [R43](03-reference-projects.md), [S07](03-reference-projects.md)。

## 操作系统与 form factor 策略

| 平台 | 初始承诺范围 | 必须单独probe | 不可当作默认事实 |
|---|---|---|---|
| macOS | LAN文件协议；AirPlay接收provider；native媒体播放 | Bonjour/BLE广播限制；sandbox文件授权；native player | 公开API支持任意Wi-Fi Direct/Miracast/AWDL |
| Windows | LAN文件；AirPlay provider；Miracast系统接收probe | 应用identity、WinRT/UI线程、无线驱动、MediaSource生命周期 | 有API就能headless raw帧导出；所有PC均有兼容WiFi |
| Linux desktop/server | LAN文件、headless媒体流；可选WFD无线实验 | P2P网卡、NetworkManager/wpa、会话/声卡/显示后端 | 应自动停网络服务；容器可以完整模拟WiFi硬件 |
| Android/Android TV | 普通App LAN接收/发送、平台播放器 | mDNS权限、后台、JNI生命周期、电视安装/遥控器 | 普通App能注册完整WFD sink或常驻后台 |
| HarmonyOS phone/tablet/TV | LAN socket/发现/媒体API逐SDK验证 | 商用系统与OpenHarmony区别、系统版本、分发/权限 | 系统源码存在就能普通App调用；旧EMUI支持等于新鸿蒙支持 |
| iOS/iPadOS | 自有App内的受限LAN/分享/capture入口研究 | 后台、权限、系统分享扩展、录屏用户同意 | 任意daemon/动态下载sidecar/常驻receiver |
| Tizen/webOS/Roku TV | 先使用电视已有receiver作为对端 | App商店、Web/native SDK、网络/媒体限制 | 任意电视都能运行Rust/Tauri二进制 |
| Browser | QR/HTTP/WebRTC playground/client | TLS、安全上下文、用户交互、后台暂停 | raw UDP、WFD、任意BLE广告、system device identity |

## 系统服务的共存规则

不要多个adapter同时抢同一个端口、mDNS名字、音频设备或P2P group。平台radio服务采用lease；独占要求需要用户确认。既有系统receiver已运行时优先复用或让用户选择，不强行关闭。多个协议声明相同友好名称可以在UI聚合展示为“可能同一设备”，但身份与权限始终分别保留。
