# 参考项目目录与使用边界

这是 **64 个仓库候选**，不是生产依赖列表。`page_review` 只表示阅读过公开页面或初步确认入口，**不是完整代码、安全、许可或真机审计**。所有项目 `production_approved=false`，当前分支不等于锁定的可复现版本。克隆后先生成本地 commit lock，再研究；不要为图省事执行 README 的安装脚本。

## 分组与克隆策略

`file-core`、`airplay`、`miracast`、`cast` 是主要研究组；`file-extra`、`media`、`media-extra` 是扩展；`platform` 是平台后端；`gated` 涉及额外许可/权限/来源障碍；`large` 不建议第一轮整仓下载。运行包内 `tools/clone_plan.py` 只生成命令，不联网、不执行。

“有 library”只说明技术形态；GPL library 仍需遵守 GPL，C ABI、Rust FFI 或 Tauri 不改变许可。MIT/Apache 标签也不替代文件级 provenance。旧 README 的“supported”只作上游声明。

## 仓库清单

### R01 · FDH2/UxPlay

- 源码：<https://github.com/FDH2/UxPlay>
- 形态 / 分组：C/C++ 接收应用，可改造 callback/RTP 输出；`airplay`。
- 许可初筛：GPL-3.0；依赖及文件级有混合许可。**未放行生产复用。**
- 用途：AirPlay 镜像接收、平台兼容基线、外部 provider POC。
- 首读范围：`README.md; lib/; renderers/; CMakeLists.txt; LICENSE/COPYING`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：非通用发送库；主分支实验能力与稳定发行版要分开；不代表完整 AP2/DRM 支持。

### R02 · metaneutrons/shairplay-rust

- 源码：<https://github.com/metaneutrons/shairplay-rust>
- 形态 / 分组：Rust library + example player；`airplay`。
- 许可初筛：LGPL-3.0-or-later（项目声明）。**未放行生产复用。**
- 用途：库接口、AP1/AP2 音频与实验视频路径对照。
- 首读范围：`Cargo.toml; Cargo.lock; src/raop/; src/net/; src/crypto/; AP2-STATUS.md; deny.toml`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：视频仍为 WIP；第三方安全/完整性尚未审计；不根据 stars 或 CI 判定可上线。

### R03 · FD-/RPiPlay

- 源码：<https://github.com/FD-/RPiPlay>
- 形态 / 分组：C/C++ 镜像接收应用；`airplay`。
- 许可初筛：GPL-3.0（项目标注）。**未放行生产复用。**
- 用途：UxPlay 沿革、接收/渲染分层对照。
- 首读范围：`README.md; lib/; renderers/`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：历史/设备特化实现，非首选新产品底座。

### R04 · jqssun/android-airplay-server

- 源码：<https://github.com/jqssun/android-airplay-server>
- 形态 / 分组：Android/Android TV 应用 + JNI/C 核心；`airplay`。
- 许可初筛：GPL-3.0（项目标注）。**未放行生产复用。**
- 用途：Android 平台接收器适配参考。
- 首读范围：`README.md; app/src/main/cpp/; Gradle 配置`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：JNI、系统音视频权限、引用 UxPlay 版本独立核对。

### R05 · juhovh/shairplay

- 源码：<https://github.com/juhovh/shairplay>
- 形态 / 分组：C library + app；`airplay`。
- 许可初筛：LGPL/GPL 混合；PlayFair 链路单独核查。**未放行生产复用。**
- 用途：经典 RAOP 接口和上游来源追踪。
- 首读范围：`README.md; LICENSE*; src/; lib/`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不能以单个 LGPL 头文件覆盖完整链接产物许可。

### R06 · EstebanKubata/playfair

- 源码：<https://github.com/EstebanKubata/playfair>
- 形态 / 分组：C 协议密码兼容组件；`airplay`。
- 许可初筛：GPL 及密码材料来源待文件级核查。**未放行生产复用。**
- 用途：只作为法律/来源和互操作研究对象。
- 首读范围：`README; license headers; implementation provenance`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：隔离研究；不把现成密钥/表格默认导入自有 MIT 实现。

### R07 · mikebrady/shairport-sync

- 源码：<https://github.com/mikebrady/shairport-sync>
- 形态 / 分组：C 音频接收应用/daemon；`airplay`。
- 许可初筛：混合许可，逐文件核对。**未放行生产复用。**
- 用途：AP1/AP2 音频、时钟同步、输出后端参考。
- 首读范围：`README.md; COPYING*; configuration and audio backends`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不是屏幕镜像实现；外部解码/时钟依赖另审。

### R08 · SteeBono/airplayreceiver

- 源码：<https://github.com/SteeBono/airplayreceiver>
- 形态 / 分组：C#/.NET receiver 项目；`airplay`。
- 许可初筛：根目录 MIT 声明；衍生代码/依赖待核。**未放行生产复用。**
- 用途：跨语言实现的协议和媒体结构对照。
- 首读范围：`README.md; LICENSE; receiver handlers; crypto dependency tree`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不能因 MIT 标签推断所有 FairPlay/历史移植代码来源已解决。

### R09 · YimingZhanshen/Airplay2OnWindows

- 源码：<https://github.com/YimingZhanshen/Airplay2OnWindows>
- 形态 / 分组：Windows C# 应用/fork；`airplay`。
- 许可初筛：根目录 MIT 声明；上游链待核。**未放行生产复用。**
- 用途：Windows 接收体验及对照测试。
- 首读范围：`README.md; LICENSE; upstream diffs`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不是官方 Windows AirPlay SDK；按 fork 全链审计。

### R10 · openairplay/airplay-spec

- 源码：<https://github.com/openairplay/airplay-spec>
- 形态 / 分组：非官方协议文档；`airplay`。
- 许可初筛：文档许可需逐项核查。**未放行生产复用。**
- 用途：字段、状态机、服务发现索引。
- 首读范围：`service discovery; mirroring; pairing chapters`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：逆向文档不是 Apple 保证；复制文档/图/fixture 也需许可判断。

### R11 · postlund/pyatv

- 源码：<https://github.com/postlund/pyatv>
- 形态 / 分组：Python library + CLI；`airplay`。
- 许可初筛：MIT（项目声明）。**未放行生产复用。**
- 用途：Apple TV/AirPlay 控制和发送能力参考。
- 首读范围：`docs; protocol modules; tests`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：控制/媒体 URL/音频不等于通用实时屏幕编码发送。

### R12 · philippe44/AirConnect

- 源码：<https://github.com/philippe44/AirConnect>
- 形态 / 分组：C/C++ 音频桥接应用；`airplay`。
- 许可初筛：多许可/上游依赖待核查。**未放行生产复用。**
- 用途：AirPlay→UPnP/Chromecast 音频桥接思路。
- 首读范围：`README.md; bridge modules; license files`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：音频桥不是完整跨协议屏幕桥；只在用户显式授权下中继。

### R13 · localsend/protocol

- 源码：<https://github.com/localsend/protocol>
- 形态 / 分组：REST 协议文档；`file-core`。
- 许可初筛：文档复制许可未完成审计。**未放行生产复用。**
- 用途：LocalSend v2.2 主规范；v3 另行成熟度判断。
- 首读范围：`README.md; README-zh-CN.md; CHANGELOG.md; v3/`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：其发现是 UDP multicast/HTTP，不是 mDNS；文档版本≠产品版本。

### R14 · localsend/localsend

- 源码：<https://github.com/localsend/localsend>
- 形态 / 分组：Dart/Flutter 完整应用；`file-core`。
- 许可初筛：Apache-2.0（项目声明）。**未放行生产复用。**
- 用途：库存客户端 E2E 和协议细节基线。
- 首读范围：`app protocol/service code; LICENSE; releases`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：Xross 已有实现优先复用；不是为 Rust 设计的稳定嵌入库。

### R15 · grishka/NearDrop

- 源码：<https://github.com/grishka/NearDrop>
- 形态 / 分组：Swift macOS 应用；`file-core`。
- 许可初筛：Unlicense（项目声明）。**未放行生产复用。**
- 用途：Quick Share LAN 互操作及 PROTOCOL.md。
- 首读范围：`PROTOCOL.md; README.md; discovery; transfer handlers`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：macOS BLE 广播受限；QR/可发现状态要实测；不要执行降低系统安全的安装建议。

### R16 · kyujin-cho/Bada

- 源码：<https://github.com/kyujin-cho/Bada>
- 形态 / 分组：Kotlin Android 应用 + core-protocol JVM 模块；`file-core`。
- 许可初筛：Apache-2.0（项目/模块声明）。**未放行生产复用。**
- 用途：无 GMS Quick Share LAN/P2P 收发参考。
- 首读范围：`core-protocol/; docs/architecture.md; README.md; Android transport modules`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：早期项目；其 NearDrop/Windows 互操作不能按已验证处理。

### R17 · google/nearby

- 源码：<https://github.com/google/nearby>
- 形态 / 分组：C++ 多平台库集合；`file-core`。
- 许可初筛：Apache-2.0。**未放行生产复用。**
- 用途：Nearby Connections/Presence、帧与平台链路。
- 首读范围：`connections/; internal/; sharing/（仅存在时）； proto; platform`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：官方仓库明确非正式支持产品；不是稳定完整 Quick Share 应用 SDK。

### R18 · google/ukey2

- 源码：<https://github.com/google/ukey2>
- 形态 / 分组：协议/实现/测试；`file-core`。
- 许可初筛：Apache-2.0（项目声明）。**未放行生产复用。**
- 用途：认证密钥交换规范与测试向量。
- 首读范围：`README.md; protocol docs; test vectors`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：复用前固定 suite/版本；不要自行简化承诺或确认步骤。

### R19 · Martichou/rquickshare

- 源码：<https://github.com/Martichou/rquickshare>
- 形态 / 分组：Rust core_lib + core_bin + Tauri app；`file-core`。
- 许可初筛：GPL-3.0（仓库声明）。**未放行生产复用。**
- 用途：现成 Rust Quick Share core/CLI 对照。
- 首读范围：`core_lib/Cargo.toml; core_lib/src/; Cargo.lock; frontend integration`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：core_lib 是可调用形态但仍有 GPL 义务；git 分支依赖/build script 要审计。

### R20 · vicr123/QNearbyShare

- 源码：<https://github.com/vicr123/QNearbyShare>
- 形态 / 分组：Qt/C++ Linux 应用；`file-extra`。
- 许可初筛：未完成文件级许可核验。**未放行生产复用。**
- 用途：Quick Share 第二接收器/实现对照。
- 首读范围：`README; LICENSE/COPYING; networking modules`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不作为默认许可安全来源；独立验证支持方向。

### R21 · seemoo-lab/opendrop

- 源码：<https://github.com/seemoo-lab/opendrop>
- 形态 / 分组：Python AirDrop 研究工具；`gated`。
- 许可初筛：GPL-3.0（项目声明）。**未放行生产复用。**
- 用途：AirDrop 发现、HTTPS/归档流程研究。
- 首读范围：`README.md; opendrop/; requirements; known limitations`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：现代系统、contacts-only 与 AWDL/硬件支持均需重证。

### R22 · seemoo-lab/owl

- 源码：<https://github.com/seemoo-lab/owl>
- 形态 / 分组：C AWDL 研究工具；`gated`。
- 许可初筛：GPL-3.0（项目声明）。**未放行生产复用。**
- 用途：Linux AWDL 链路研究。
- 首读范围：`README.md; src/; interface and packet paths`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：需适配网卡/驱动/权限；不是通用用户态 macOS/WiFi Direct 库。

### R23 · nearby-sharing/android

- 源码：<https://github.com/nearby-sharing/android>
- 形态 / 分组：Android 应用 + 协议组件；`file-extra`。
- 许可初筛：未完成文件级许可核验。**未放行生产复用。**
- 用途：Windows Nearby Sharing / MS-CDP 互操作参考。
- 首读范围：`README.md; protocol modules; platform transport; LICENSE`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不要混为 Google Quick Share；必须核对 CDP 会话和授权模型。

### R24 · KDE/kdeconnect-kde

- 源码：<https://github.com/KDE/kdeconnect-kde>
- 形态 / 分组：C++/Qt daemon + plugins + app；`file-extra`。
- 许可初筛：GPL/组件混合，逐文件核查。**未放行生产复用。**
- 用途：KDE Connect 配对与 share 插件。
- 首读范围：`core/; plugins/share/; network backend; licensing`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：只启用 share/必要 discovery；不能隐式开放命令执行或输入控制插件。

### R25 · GSConnect/gnome-shell-extension-gsconnect

- 源码：<https://github.com/GSConnect/gnome-shell-extension-gsconnect>
- 形态 / 分组：GNOME Shell 扩展/服务；`file-extra`。
- 许可初筛：GPL（版本与依赖待核）。**未放行生产复用。**
- 用途：KDE Connect 跨语言/平台互操作对照。
- 首读范围：`README.md; service/plugins/share.js（以本地树核对）； protocol code`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不是通用 Rust 库；避免引入整个 shell 扩展。

### R26 · schlagmichdoch/PairDrop

- 源码：<https://github.com/schlagmichdoch/PairDrop>
- 形态 / 分组：Web 前后端应用；`file-extra`。
- 许可初筛：GPL-3.0（项目声明）。**未放行生产复用。**
- 用途：浏览器附近发现/房间/文件传输 UX。
- 首读范围：`README.md; client/; server/; deployment config`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：应用信令不等于统一系统分享协议；浏览器须打开页面。

### R27 · SnapDrop/snapdrop

- 源码：<https://github.com/SnapDrop/snapdrop>
- 形态 / 分组：Web 应用；`file-extra`。
- 许可初筛：GPL-3.0（项目标注，文件级待核）。**未放行生产复用。**
- 用途：轻量网页传输基线。
- 首读范围：`README.md; client/; server/`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：自身 signaling schema，需要指定兼容部署版本。

### R28 · magic-wormhole/magic-wormhole

- 源码：<https://github.com/magic-wormhole/magic-wormhole>
- 形态 / 分组：Python library + CLI；`file-extra`。
- 许可初筛：MIT（项目声明）。**未放行生产复用。**
- 用途：短码配对/relay 文件互传。
- 首读范围：`docs; src/wormhole; tests; relay protocol`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：跨网但需要对应客户端/协议服务；不能替代系统 Quick Share 入口。

### R29 · magic-wormhole/magic-wormhole.rs

- 源码：<https://github.com/magic-wormhole/magic-wormhole.rs>
- 形态 / 分组：Rust library + CLI；`file-extra`。
- 许可初筛：未完成文件级许可核验。**未放行生产复用。**
- 用途：Rust Wormhole 适配候选。
- 首读范围：`Cargo.toml; src/; protocol tests; LICENSE*`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：协议版本、传输加密、服务依赖分别核对。

### R30 · schollz/croc

- 源码：<https://github.com/schollz/croc>
- 形态 / 分组：Go CLI/包；`file-extra`。
- 许可初筛：MIT（项目声明）。**未放行生产复用。**
- 用途：短码跨网文件传输与恢复参考。
- 首读范围：`README.md; src/; relay; cryptography tests`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：是独立产品协议；需说明 relay/版本互通，不把它改成 Xross 原生的替代品。

### R31 · syncthing/syncthing

- 源码：<https://github.com/syncthing/syncthing>
- 形态 / 分组：Go daemon + Web UI/协议实现；`file-extra`。
- 许可初筛：MPL-2.0（项目声明）。**未放行生产复用。**
- 用途：BEP/分块传输/兼容性测试资料。
- 首读范围：`lib/protocol/; lib/connections/; docs; LICENSE`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：同步是长期扩展；不得在本项目另造 Xross Sync/Shelf 主状态。

### R32 · albfan/miraclecast

- 源码：<https://github.com/albfan/miraclecast>
- 形态 / 分组：Linux C daemon + CLI；`miracast`。
- 许可初筛：LGPL-2.1 为主，含其他许可。**未放行生产复用。**
- 用途：Miracast sink 和无线协商对照。
- 首读范围：`README.md; COPYING; src/; res/（只读审查）`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：README 明确 source 未实现；旧脚本可能停止 NetworkManager，仅隔离机验证。

### R33 · GNOME/gnome-network-displays

- 源码：<https://github.com/GNOME/gnome-network-displays>
- 形态 / 分组：Linux C/GStreamer 发送应用；`miracast`。
- 许可初筛：GPL（逐文件确认版本）。**未放行生产复用。**
- 用途：Miracast source 与 NetworkManager 交互。
- 首读范围：`README.md; src/; meson.build; COPYING`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：实验性；GitHub 为镜像；WiFi P2P/编码设备决定可用性。

### R34 · ivygroup/miracast-sink

- 源码：<https://github.com/ivygroup/miracast-sink>
- 形态 / 分组：Android/AOSP 衍生接收项目；`miracast`。
- 许可初筛：Apache-2.0 衍生声明，逐文件审计。**未放行生产复用。**
- 用途：WFD RTSP/媒体拆分的历史参考。
- 首读范围：`native/wifi-display/; README; license headers`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：旧 API 和自带第三方组件不可默认适配当前安卓。

### R35 · FoxLost/universal-miracast-sink

- 源码：<https://github.com/FoxLost/universal-miracast-sink>
- 形态 / 分组：Android privileged/Magisk 项目；`gated`。
- 许可初筛：未发现可据以放行的统一授权；来源高风险。**未放行生产复用。**
- 用途：研究 Android WFD sink 系统约束。
- 首读范围：`README.md; root requirements; provenance only`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：涉及固件/反编译来源描述；不得进入 permissive 实现允许列表。

### R36 · weekdayjast/MiracastReceiver

- 源码：<https://github.com/weekdayjast/MiracastReceiver>
- 形态 / 分组：Android TV 接收应用；`miracast`。
- 许可初筛：未完成文件级许可核验。**未放行生产复用。**
- 用途：DLNA/WFD 接收平台 POC 对照。
- 首读范围：`README.md; manifests; WFD and DLNA modules`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：Miracast 路径可能需 root；各模式不要合并宣称。

### R37 · openharmony/castengine_wifi_display

- 源码：<https://github.com/openharmony/castengine_wifi_display>
- 形态 / 分组：OpenHarmony C++ 系统组件；`miracast`。
- 许可初筛：Apache-2.0（根许可确认）。**未放行生产复用。**
- 用途：开放 WFD/媒体分享组件的优先参考。
- 首读范围：`README_zh.md; interfaces/; frameworks/; services/; BUILD.gn`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：系统服务依赖和设备 API≠普通鸿蒙 App 可用；按文件/依赖复核。

### R38 · openharmony/castengine_cast_framework

- 源码：<https://github.com/openharmony/castengine_cast_framework>
- 形态 / 分组：OpenHarmony C++ 系统框架；`gated`。
- 许可初筛：根许可与依赖需本地核验。**未放行生产复用。**
- 用途：Cast 子系统管理/接口/实现关系。
- 首读范围：`README; interfaces/; services/; bundle.json`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：框架存在不代表 Huawei 商用 SDK 或手机互操作已开放。

### R39 · openharmony/castengine_cast_plus_stream

- 源码：<https://github.com/openharmony/castengine_cast_plus_stream>
- 形态 / 分组：OpenHarmony C++ 系统组件；`gated`。
- 许可初筛：根许可与依赖需本地核验。**未放行生产复用。**
- 用途：Cast+ stream 技术/系统依赖研究。
- 首读范围：`README; interfaces; transport; session state`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不能把 OpenHarmony 源码视为 Huawei Cast+ 合作授权。

### R40 · openharmony/communication_dsoftbus

- 源码：<https://github.com/openharmony/communication_dsoftbus>
- 形态 / 分组：OpenHarmony 分布式软总线组件；`gated`。
- 许可初筛：根许可与第三方依赖需本地核验。**未放行生产复用。**
- 用途：发现/传输/认证系统条件研究。
- 首读范围：`README; sdk/; core/; authentication boundary`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不是替换 Xross fabric 的建议；规模大、绑定 OS。

### R41 · platform/frameworks/av

- 源码：<https://android.googlesource.com/platform/frameworks/av>
- 形态 / 分组：AOSP 媒体系统源码；`large`。
- 许可初筛：Apache-2.0 为主/混合树。**未放行生产复用。**
- 用途：历史 Wi-Fi Display 源/汇实现和 Android 媒体接口。
- 首读范围：`media/libstagefright/wifi-display（对应历史 tag）； license headers`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：庞大且历史分支差异大；先 sparse/固定分支，不拉整个 Android 平台。

### R42 · chromium/openscreen

- 源码：<https://chromium.googlesource.com/openscreen>
- 形态 / 分组：C++ libcast + sender/receiver demos；`cast`。
- 许可初筛：BSD 风格/第三方混合，逐文件核查。**未放行生产复用。**
- 用途：Cast Streaming 双向/控制层、公开库首要参考。
- 首读范围：`cast/README.md; cast/sender/public/; cast/receiver/public/; cast/streaming/; standalone demos`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：自有 demo 闭环不等于官方 sender 认可通用 receiver；设备认证为独立 gate。

### R43 · googlecast/CastReceiver

- 源码：<https://github.com/googlecast/CastReceiver>
- 形态 / 分组：官方 Web Receiver 示例；`cast`。
- 许可初筛：Apache-2.0（项目声明）。**未放行生产复用。**
- 用途：CAF media URL/application receiver 语义。
- 首读范围：`README.md; js/; registration instructions`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：运行于合规 Cast 设备的 receiver app；不是 Windows Chromecast SDK。

### R44 · home-assistant-libs/pychromecast

- 源码：<https://github.com/home-assistant-libs/pychromecast>
- 形态 / 分组：Python library；`cast`。
- 许可初筛：MIT（项目声明）。**未放行生产复用。**
- 用途：Cast discovery、应用 launch、媒体控制 sender。
- 首读范围：`pychromecast/; controllers/; discovery; tests`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不是完整 screen sender 或通用 receiver。

### R45 · cast-web/protocol

- 源码：<https://github.com/cast-web/protocol>
- 形态 / 分组：CASTV2 实现/库；`cast`。
- 许可初筛：未完成文件级许可核验。**未放行生产复用。**
- 用途：TLS/protobuf channel 和认证限制交叉参考。
- 首读范围：`README.md; authentication; namespace handlers`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：逆向 channel 能通信≠通过官方认证；原库状态需核。

### R46 · pupnp/pupnp

- 源码：<https://github.com/pupnp/pupnp>
- 形态 / 分组：C UPnP SDK；`media`。
- 许可初筛：BSD-3-Clause（项目声明）。**未放行生产复用。**
- 用途：SSDP/UPnP SOAP/事件能力。
- 首读范围：`README.md; upnp/; test/; COPYING`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不是完整媒体解码器或 DMR；XML/URL安全单独做。

### R47 · plutinosoft/Platinum

- 源码：<https://github.com/plutinosoft/Platinum>
- 形态 / 分组：C++ UPnP AV SDK + tools；`media`。
- 许可初筛：GPL/商业授权路线，具体文件待核。**未放行生产复用。**
- 用途：DMC/DMR/DMS 三种角色参考。
- 首读范围：`README; license; Source/Devices/; Apps/`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不能当 permissive 小工具直接链接闭源产品。

### R48 · gerbera/gerbera

- 源码：<https://github.com/gerbera/gerbera>
- 形态 / 分组：C++ UPnP 媒体服务器；`media`。
- 许可初筛：GPL-2.0（项目标注，逐文件核查）。**未放行生产复用。**
- 用途：DMS 内容目录及 TV 兼容性对照。
- 首读范围：`README.md; src/; clients profiles`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：媒体服务器不是通用镜像接收器。

### R49 · GNOME/rygel

- 源码：<https://github.com/GNOME/rygel>
- 形态 / 分组：GNOME UPnP AV 服务/库；`media`。
- 许可初筛：LGPL-2.1（项目标注，逐文件核查）。**未放行生产复用。**
- 用途：DLNA renderer/server 模式参考。
- 首读范围：`README; src/; plugins; license`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：系统/GStreamer 依赖；API 和应用边界要核对。

### R50 · bluenviron/mediamtx

- 源码：<https://github.com/bluenviron/mediamtx>
- 形态 / 分组：Go 独立媒体路由服务；`media`。
- 许可初筛：MIT（项目声明）。**未放行生产复用。**
- 用途：RTSP/RTMP/SRT/WebRTC/HLS 互通实验基线。
- 首读范围：`README.md; config; internal/protocols; APIs`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：协议路由/重封装不等于任意转码；控制/认证需隔离。

### R51 · bluenviron/gortsplib

- 源码：<https://github.com/bluenviron/gortsplib>
- 形态 / 分组：Go RTSP library；`media`。
- 许可初筛：MIT（项目声明）。**未放行生产复用。**
- 用途：RTSP client/server、RTP/RTCP 测试基线。
- 首读范围：`README.md; examples/; pkg/; tests`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：非 Rust；可作独立对端，不要求改写成熟模块。

### R52 · GStreamer/gstreamer

- 源码：<https://github.com/GStreamer/gstreamer>
- 形态 / 分组：C 多媒体框架/插件；`large`。
- 许可初筛：LGPL 为主；插件/外部依赖各自许可。**未放行生产复用。**
- 用途：POC 解码/渲染/封装与跨平台 pipeline。
- 首读范围：`subprojects/; plugins and license files; versioned docs`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：发行时选定插件白名单；不是纯 Rust，允许作为可替换后端。

### R53 · GStreamer/gstreamer-rs

- 源码：<https://github.com/GStreamer/gstreamer-rs>
- 形态 / 分组：Rust bindings；`media`。
- 许可初筛：MIT/Apache-2.0 声明；下游 native 独立许可。**未放行生产复用。**
- 用途：Rust 调 GStreamer 的实现后端。
- 首读范围：`Cargo.toml; examples; gstreamer-app; video/audio bindings`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：绑定 Rust 不会改变 native 许可，也非纯 Rust 解码。

### R54 · webrtc-rs/webrtc

- 源码：<https://github.com/webrtc-rs/webrtc>
- 形态 / 分组：Rust WebRTC library；`media`。
- 许可初筛：MIT/Apache-2.0（项目声明）。**未放行生产复用。**
- 用途：可选标准实时媒体收发栈。
- 首读范围：`README; examples; media; peer_connection; Cargo.lock`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：ICE/DTLS/codec/平台成熟度需实测；不重复 Xross 已选栈。

### R55 · Haivision/srt

- 源码：<https://github.com/Haivision/srt>
- 形态 / 分组：C++ SRT library + tools；`media`。
- 许可初筛：MPL-2.0（项目声明）。**未放行生产复用。**
- 用途：SRT 低延迟可靠传输。
- 首读范围：`README; srtcore/; apps/; docs/`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：Rust FFI 边界、部署包和加密参数待核。

### R56 · Genymobile/scrcpy

- 源码：<https://github.com/Genymobile/scrcpy>
- 形态 / 分组：C 客户端 + Android server；`media-extra`。
- 许可初筛：Apache-2.0。**未放行生产复用。**
- 用途：Android ADB 屏幕/音频/控制实验。
- 首读范围：`README; doc/; app/; server/`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：需要调试和用户授权；不是系统投屏协议零安装兼容。

### R57 · LizardByte/Sunshine

- 源码：<https://github.com/LizardByte/Sunshine>
- 形态 / 分组：C++ GameStream host；`media-extra`。
- 许可初筛：GPL-3.0（项目声明）。**未放行生产复用。**
- 用途：高性能屏幕发送/硬件编码对照。
- 首读范围：`README; docs; src; protocol/session code`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：屏幕捕获与控制权限另授权；高复杂度后置。

### R58 · moonlight-stream/moonlight-common-c

- 源码：<https://github.com/moonlight-stream/moonlight-common-c>
- 形态 / 分组：C client protocol core；`media-extra`。
- 许可初筛：GPL-3.0（项目声明）。**未放行生产复用。**
- 用途：GameStream 接收端核心对照。
- 首读范围：`README; src; callbacks; platform code`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：没有 UI 不等于宽松授权；输入控制默认关闭。

### R59 · obsproject/obs-studio

- 源码：<https://github.com/obsproject/obs-studio>
- 形态 / 分组：C/C++ 应用 + libobs + plugins；`large`。
- 许可初筛：GPL-2.0-or-later 为主。**未放行生产复用。**
- 用途：外部媒体输入/输出验收对端。
- 首读范围：`README; plugins/; libobs/; docs`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不是默认嵌入闭源 Xross 的 SDK；插件许可独立审查。

### R60 · FFmpeg/FFmpeg

- 源码：<https://github.com/FFmpeg/FFmpeg>
- 形态 / 分组：C libraries + CLI；`large`。
- 许可初筛：LGPL/GPL，取决于配置/依赖。**未放行生产复用。**
- 用途：解码/封装/测试语料生成基线。
- 首读范围：`LICENSE.md; configure; selected libav*; tests/`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：禁止默认 enable-gpl/nonfree；编解码专利是另一问题。

### R61 · bluez/bluez

- 源码：<https://github.com/bluez/bluez>
- 形态 / 分组：Linux Bluetooth daemon + libraries；`platform`。
- 许可初筛：GPL/LGPL 混合。**未放行生产复用。**
- 用途：OBEX/OPP 与 Linux BLE 平台参考。
- 首读范围：`doc/; obexd/; profiles/; COPYING*`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：用系统 D-Bus 接口优先；不把整栈内嵌。

### R62 · keepsimple1/mdns-sd

- 源码：<https://github.com/keepsimple1/mdns-sd>
- 形态 / 分组：Rust library；`platform`。
- 许可初筛：Apache-2.0/MIT（具体 Cargo 声明本地确认）。**未放行生产复用。**
- 用途：mDNS provider 候选。
- 首读范围：`Cargo.toml; examples; service_daemon tests`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：不能据此覆盖 BLE/P2P；接口多播生命周期需要实测。

### R63 · deviceplug/btleplug

- 源码：<https://github.com/deviceplug/btleplug>
- 形态 / 分组：Rust BLE central library；`platform`。
- 许可初筛：MIT/Apache-2.0（项目声明）。**未放行生产复用。**
- 用途：跨平台扫描/连接能力候选。
- 首读范围：`README; platform modules; feature support table`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：central 能力不能推断任意 peripheral 广播或 Wi-Fi P2P。

### R64 · microsoft/windows-rs

- 源码：<https://github.com/microsoft/windows-rs>
- 形态 / 分组：Rust Windows API bindings；`platform`。
- 许可初筛：MIT/Apache-2.0（项目声明）。**未放行生产复用。**
- 用途：WinRT Miracast/Windows API 接入。
- 首读范围：`README; crates/libs/windows; Miracast generated API; samples`（路径只作定位建议，按固定 commit 的树确认）。
- 边界：绑定存在不代表当前进程身份/硬件/权限可以调用。

## 官方标准、平台 SDK 与政策入口

### S01 · GNU GPL FAQ：链接、aggregate、IPC 的解释

<https://www.gnu.org/licenses/gpl-faq.en.html>

法律解释来源；不是针对 Xross 的法律意见。

### S02 · GNU LGPLv3 正文

<https://www.gnu.org/licenses/lgpl.html>

库组合、重新链接、通知与安装信息等条件。

### S03 · 美国版权法 §101/§102

<https://www.copyright.gov/title17/92chap1.html>

思想/过程与受保护表达的区别；司法辖区不能泛化。

### S04 · Windows MiracastReceiver API

<https://learn.microsoft.com/en-us/uwp/api/windows.media.miracast.miracastreceiver>

系统 receiver 入口；需实际平台 probe。

### S05 · Windows MediaSourceCreated

<https://learn.microsoft.com/en-us/uwp/api/windows.media.miracast.miracastreceiversession.mediasourcecreated>

公开对象为 MediaSource；不是 raw packet 导出保证。

### S06 · MS-MICE overview

<https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-mice/ab6341b7-4fc7-41fd-a74d-3fe023455482>

Miracast over infrastructure；初始无线发现不可忽略。

### S07 · Google Cast getting started

<https://developers.google.com/cast/docs/get-started>

sender 与 receiver app 平台区别。

### S08 · Google Cast registration

<https://developers.google.com/cast/docs/registration>

应用 ID/开发设备注册≠通用硬件授权。

### S09 · Google Cast Web Receiver

<https://developers.google.com/cast/docs/web_receiver/basic>

CAF receiver app 运行条件。

### S10 · Huawei Cast+ codelab

<https://developer.huawei.com/consumer/cn/codelab/CastPlusKit/index.html>

合作/SDK权限门槛；文档年代和新鸿蒙需再核。

### S11 · Huawei Share Windows codelab

<https://developer.huawei.com/consumer/cn/codelab/ShareKit-Windows/>

旧 Windows/EMUI 前提，不自动外推现代设备。

### S12 · Huawei Share Linux codelab

<https://developer.huawei.com/consumer/en/codelab/ShareEngine-Linux/>

SDK平台条件和原生依赖，旧资料。

### S13 · Huawei Share Android codelab

<https://developer.huawei.com/consumer/cn/codelab/ShareKit-android/>

厂商 SDK 合作路径，非公开 wire 标准。

### S14 · Samsung China Quick Share

<https://www.samsung.com.cn/apps/quick-share/>

中国区互传联盟与分享方式，需地区/ROM矩阵。

### S15 · Android Quick Share

<https://www.android.com/quick-share/>

产品支持范围不是开放 SDK 保证。

### S16 · Cargo build scripts

<https://doc.rust-lang.org/cargo/reference/build-scripts.html>

构建阶段执行风险。

### S17 · Rust procedural macros

<https://doc.rust-lang.org/reference/procedural-macros.html>

编译期代码/文件访问风险。

### S18 · Tauri capabilities

<https://v2.tauri.app/security/capabilities/>

限制 frontend IPC；不是全进程安全沙箱。

### S19 · Tauri sidecar

<https://v2.tauri.app/develop/sidecar/>

目标三元组打包及执行权限。

### S20 · FFmpeg legal

<https://www.ffmpeg.org/legal.html>

构建选项与组合许可。

### S21 · Bluetooth OPP

<https://www.bluetooth.com/specifications/specs/object-push-profile-1-2-1/>

文件推送 profile；系统开放 API 待核。

### S22 · WHIP RFC9725

<https://www.rfc-editor.org/rfc/rfc9725.html>

WebRTC HTTP ingest 已成 RFC；不因此宣称 WHEP 也已标准化。

### S23 · GitHub license API 局限

<https://docs.github.com/en/rest/licenses/licenses>

根 LICENSE 检测不覆盖完整依赖。

### S24 · Wi-Fi Display 规范入口（经 MiracleCast 链接）

<https://www.wi-fi.org/file/wi-fi-display-technical-specification-v11>

可用性/获取条款需本地确认；本包未读取完整标准。

### S25 · WebDAV RFC4918

<https://www.rfc-editor.org/rfc/rfc4918>

稳定规范链接；仅作为后续研究起点，本轮未逐章重审。

### S26 · RTP RFC3550

<https://www.rfc-editor.org/rfc/rfc3550>

时序/RTCP 标准参考入口，本轮未逐章重审。

### S27 · HTTP semantics RFC9110

<https://www.rfc-editor.org/rfc/rfc9110>

Range、条件请求与缓存验证，后续实现依据。

### S28 · Microsoft Connected Devices Platform / MS-CDP

<https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-cdp/f5a15c56-ac3a-48f9-8c51-07b2eadbe9b4>

公开协议基础；NearShare上层语义和具体Windows互操作另验。

### S29 · Microsoft SMB2 / SMB3 规范

<https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-smb2/5606ad47-5ee0-437a-817e-70c366052962>

系统共享/成熟库接入的协议参考，不建议自行重写完整安全栈。

### S30 · SFTP filexfer draft history

<https://datatracker.ietf.org/doc/draft-ietf-secsh-filexfer/history/>

历史Internet-Draft并非最终通用RFC；固定实现版本与扩展，再与既有Xross SFTP核对。

### S31 · NDI SDK licensing

<https://docs.ndi.video/all/developing-with-ndi/sdk/licensing>

官方SDK使用/分发/标识条件；SDK可取得不代表开放wire协议或任意重许可。

## 没有默认克隆入口的候选

- **互传联盟 / MDFE**：本轮未找到可以确认完备性和公开许可的官方 wire spec + receiver library 组合；不能根据厂商宣传推导协议字节。研究与商务申请保留，不造一个虚构 SDK。
- **Huawei Share / Cast+**：通过官方申请取得 SDK、授权文本和支持矩阵后，记录 SDK 哈希及可分发权；不可把付费/受限 SDK 提交公开仓库。
- **NDI**：以合法 SDK 条款/平台支持为 gate；本包不声称它是开放协议。
- **OpenHarmony castengine_dlna**：由框架文档提及的候选关系，但这次未成功核验该独立仓库入口，不放进自动克隆清单。先追踪 framework 固定版本中的真实依赖。
- **Tizen/webOS/Roku/厂商电视控制 SDK**：辅助发现、app launch、媒体控制作为能力扩展；在模型/系统版本/API 授权验证前不宣称原生通用投屏。

## 必须形成的本地来源记录

每个 Rxx 最终记录完整 Git commit、获取时间、分支/tag（仅说明来源）、实际许可文件 SHA256、模块/依赖关系、构建步骤、所有能执行代码的构建入口、是否有生成或固件来源文件、引用过它的研究文档、实现允许列表。根许可证与输出二进制许可分别记录。不要删除不方便的来源历史。

依赖版本和证据必须绑定：`source_commit + protocol_profile + feature_set + platform + device/OS + test_run`。任何一项变化均不能自动继承“支持”结论。
