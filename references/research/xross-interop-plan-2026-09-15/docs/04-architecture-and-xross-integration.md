# 架构：统一能力，不合并所有协议和信任

## 1. 系统边界

```text
                         外部设备 / 库存客户端
       LocalSend  Quick Share  AirPlay  WFD  Cast  DLNA  ...
              │        │          │      │     │
       ┌──────┴────────┴──────────┴──────┴─────┴───────────┐
       │ Protocol Providers                                │
       │ Rust core / approved library / standalone worker   │
       │ Windows native media provider / vendor SDK worker   │
       └─────────┬──────────────────────────────┬───────────┘
                 │ offers + file byte leases    │ media source descriptors
       ┌─────────▼──────────────────────────────▼───────────┐
       │ Interop Runtime                                     │
       │ capabilities · policy · session registry · events    │
       │ resource budgets · radio leases · diagnostics        │
       └─────────┬──────────────────────────────┬───────────┘
                 │ HostPorts / scoped storage  │ Media ports
          ┌──────▼────────┐            ┌───────▼──────────┐
          │ StandaloneHost │            │ XrossHostAdapter │
          │ private spool  │            │ existing authority│
          │ local policy   │            │ existing services │
          └───────────────┘            └──────────────────┘
                 ▲                             ▲
          CLI / Tauri Playground       Xross Native UI / xrossd
```

箭头是**数据/调用边界**，不代表一定跨进程。Rust API与IPC是同一语义契约的两种carrier。主Xross只依赖契约+adapter，不依赖lab源码树。

## 2. 推荐仓库树（计划创建，不是声称当前已存在）

```text
xross-interop-lab/
  README.md
  references/repositories.json
  references/sources.lock.json        # 本地解析后生成，不能伪造SHA
  external/                          # ignored；third-party quarantine
  research/<profile>/                # source analyses / open questions
  captures/private/                  # ignored；加密保管原始敏感抓包
  evidence/<run-id>/                  # 脱敏结果、hash与环境
  specs-reviewed/                    # 来源审查后可给实现者的事实规范
  pocs/<provider>/                    # throwaway / third-party-labelled
  decisions/                         # gate decisions 与法律/权限结论

xross-interop/
  crates/interop-contract/            # 数据类型、错误、能力、JSON契约
  crates/interop-runtime/             # 会话/事件/路由/预算；不持有Xross账号
  crates/interop-policy/              # 策略、授权、token/lease约束
  crates/interop-file/                # spool、safe path、stream、integrity
  crates/interop-media/               # frame/timebase/sink/contracts
  crates/interop-platform/            # typed discovery/radio/platform ports
  crates/interop-ipc/                 # 本地control/data plane codecs
  crates/proto-localsend/             # 经复用决策后的adapter
  crates/proto-quickshare/            # source-audited实现
  crates/proto-airplay/               # source-audited实现；允许另许可产物
  crates/proto-wfd/                   # 平台无关WFD parser/state machine
  crates/proto-upnp/                  # 三角色分别feature gate
  crates/proto-cast/                  # sender/streaming/receiver明确分开
  crates/interop-testkit/             # fixtures、reference drivers、虚拟时钟
  adapters/standalone-host/           # 独立本地store/最小权限
  adapters/xross-host/                # 契约映射；主仓接口确认后实现
  apps/interopd/                      # supervisor/control API
  apps/interop-cli/                   # binary: xinterop
  apps/playground/src-tauri/          # native bridge，不包含协议实现
  apps/playground/src/                # Svelte/TypeScript控制UI
  workers/<provider>/                # 按许可/权限/UI owner边界构建
  tests/contract/ tests/e2e/ tests/fuzz/ tests/devices/
  policies/                          # scoped sample policies
  schemas/                           # exported JSON Schema
  docs/                              # public-safe docs，逐项来源
```

**不一次创建所有空crates。**按任务首次产生可验收能力时创建，避免“架构很大但没有闭环”。`proto-*` 只要没有通过来源gate，就只存在于lab，不得靠同名路径伪装为可发布实现。

## 3. 依赖规则

`interop-contract` 不依赖Tokio、Xross、GUI、codec、Bluetooth、平台SDK；只允许基本序列化/标识类型等小型依赖。`proto-*-core` 可用bytes/成熟密码原语/标准parser，但不直接访问全盘、启动进程、登录账户或输出UI。

`interop-runtime` 编排纯状态机与ports。平台socket/BLE/mDNS provider负责系统I/O。native media provider可以独立实现高层`MediaSession`而不经过自有WFD parser：**Windows系统已经做的栈不要求再转回Rust重复协商。**

所有可选协议默认feature关闭；“all-features用于CI”与“release要打包哪些feature”是两个配置。包含GPL实现的linked产物不作为MIT通用daemon分发。单独out-of-process provider有独立LICENSE/NOTICE/SBOM及审查。

## 4. HostPorts——避免第二套Xross

以下是拟议的功能边界，不是对当前Xross API的引用：

| Port | 输入/输出语义 | Standalone实现 | Integrated实现 |
|---|---|---|---|
| AdmissionPort | 外部offer/session请求 → allow/deny/pending及预算 | 本地人工/有限policy | Xross现有身份与权限机制，仅创建external主体 |
| ContentReadPort | 一个被批准的entry/variant → bounded read lease | 用户显式CLI选择文件 | Xross现有内容/Remote Files authority |
| ContentWritePort | offer entry →受限sink lease、commit/abort | 独立private spool | Xross既有transfer/materialization入口 |
| HistoryPort | normalized task events | 本地轻量日志/DB | 现有transfer center/history |
| ContentPublishPort |完成内容及来源→publish receipt |存储路径/opaque receipt |Shelf/catalog映射；不扩大audience |
| MediaHostPort |source descriptor→sink能力/呈现会话 |null/file/native player |Xross Media Graph或系统native-only provider |
| IdentityLinkPort |外部endpoint→可选验证关联 |明确用户alias |Xross已有verified binding；不靠IP自动关联 |
| NetworkPolicyPort |listener/egress/radio申请→lease |本地接口规则 |主产品现有网络/配额/策略 |

**文件字节并非全部经过 Xross native transfer wire。**外部协议仍自己执行它的分帧/确认/安全传输；复用的是内容存储、审批、lease、历史、UI、整合语义。不能因为内部有resume，就对不支持resume的对端宣称断点续传。

## 5. Peer与Discovery模型

每条观察包括`protocol_id`、`profile_id`、`provider_id`、`interface_id`、`observed_address`、`opaque_protocol_identity`、`display_name`、`expires_at`、`advertised_capabilities`及`evidence_level`。

`EndpointId` 由本地注册表生成，不直接等于MAC/IP/device name。观察expired即从路由候选移除，但历史保留脱敏来源。跨协议合并只允许：用户显式alias（仅UI用途）或可验证身份绑定（额外授权仍独立）。同IP下可能有多个设备/NAT，随机MAC会变，同名TV很常见，因此它们只能作为弱显示线索。

mDNS、SSDP、LocalSend multicast、BLE、Wi-Fi P2P都是不同DiscoveryProvider。统一事件接口，但不伪造共同底层。广播策略控制每个profile启停、接口选择、TTL、可见名称、速率；网络切换撤销旧地址再发布新观察。

## 6. RadioLease

Wi-Fi Direct可能要求创建组、改变频道或独占适配器。请求必须声明`resource`、`shared/exclusive`、`estimated_disruption`、`owner_session`、`deadline`、`rollback_action_id`。只读probe先判断可用性，不执行系统变更。

默认拒绝会断开当前网络的隐式操作；交互确认后只在指定测试网卡上运行。worker意外退出由platform broker回收lease；rollback必须在真实硬件上验证。Bluetooth discovery不等于授权，RSSI不当作身份强度。

## 7. 媒体图与形态

- `EncodedStream`：codec configuration + access units/packets + timebase/clock mapping。可接适配解码器或重封装。
- `PcmStream`：显式sample rate/channels/layout/format；不能按错误采样率播放。
- `NativePresentation`：owner process内的系统媒体源/渲染对象；跨IPC只给opaque presentation ID与控制能力，不传原生裸指针。
- `MediaResource`：URL/resource lease；客户端/TV自行取内容，遵循fetch和credential policy。

sink可以是null、file、native renderer、network exporter、Xross media adapter。transcoder是单独资源预算服务，不自动为了“都统一成H264”而启动。NativePresentation无export权限/能力时，只能播放，不能暗中屏幕录制来绕开边界。

## 8. 进程拓扑

**开发最小模式：**可信CLI→standalone runtime；一个合法外部provider→native player。一个进程可以覆盖已审查的纯mock/LocalSend；不是第一天强制运行十个daemon。

**产品模式：**小型interop supervisor +若干worker。按解析暴露、native dependency、GPL许可、厂商SDK、特权radio、图形会话决定worker分组。`airplay-worker`仅拿必要配对store、受限network和媒体输出，不拿Xross Vault/device private key。

同用户、同权限的独立进程只提供崩溃隔离，**不自动防止读取用户目录**。Linux可验证namespaces/seccomp/限制文件描述符等具体策略；Windows与macOS分别验证实际隔离机制。无法达到目标时报告`process-only`，不声称安全sandbox。移动端可能必须使用系统service/isolate/extension而不是spawn二进制。

## 9. XROSS真实接口审查的最低要求

本轮只读取了`xrossone/xross-dev`的`dev`分支README和north-star部分内容。README描述单daemon、headless、typed服务；north-star明确旧MVP是参考而非当前限制。**没有完整审查当前Transfer/Shelf/Media/LocalControl的真实代码。**

集成第一个任务在本地完成：固定Xross commit；读取当前AGENTS（用户自己的可信仓库指令）、handbook/status、ADR、crate/public interfaces；找出Offer/Transfer/ContentLease/LocalSend/MediaSession对应真实类型；输出`integration-map.md`，每行写真实路径、类型、调用端、权限、生命周期、迁移影响。不能用本包拟议名称直接新建一套并存模型。

版本接入采用一个方向：`xross-dev adapter → versioned interop contract`。不让interop core反向import主仓private内部路径。先用mock host契约测试；再给一个现有Native UI入口接真实provider；形成垂直测试后才扩展。
