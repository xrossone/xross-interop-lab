# T03 Xross 集成映射（xross-dev 当前 checkout，只读）

**仓库**：`/Volumes/Portable2TB/ExtDev/xross-dev`（用户活跃工作区）
**基线 commit**：`099b6c72`（branch `content-plane-0044`；扫描期间 HEAD 曾自 `4dcebc20` 前移——该仓在活跃开发中，集成实现前需复核）
**工具链**：xross-dev pin `1.97.1`；interop impl/ pin `1.98.0`（互不依赖，仅经 wire/JSON 契约对接）
**机器可读基线**：[decisions/xross-contract-baseline.json](../decisions/xross-contract-baseline.json)
（30 个 anchor 路径由 `tools/test_xross_baseline.py` 强制存在性校验）
**方法**：只读（git 元数据 + 文件读取），未修改 xross-dev 任何文件。

## 0. 总体架构事实（决定集成方式）

- 分层目录即依赖方向：`contract ← kernel ← runtime ← service ← adapter ← app`，由 `scripts/check-boundaries.ts` 强制；`[workspace] members = ["crates/*/*"]`（92 crates）。
- 服务经 `crates/contract/xross-service-host` 的 trait **编译期注册**进 `ServiceRegistry`，无动态插件。
- 本地客户端唯一正门：daemon `xrossd` 的控制面（**gRPC over Unix socket**，`control.proto` 135 RPC，UDS 0600 + peer-UID + bearer `ControlToken` 三层认证）。
- 对外协议（LocalSend）在 `adapter` 层，fabric 永不感知（ADR 0001）。
- 工作区有大量 vendored submodule（`vendors/{localsend-rs,irpc,iroh-relay,monio,file-search,wol-rust}`），workspace `exclude`。

## 1. 逐子系统映射（path / symbol / owner）

### LocalSend —— 状态：稳定，复用优先（T03-01）

| 项 | 值 |
|---|---|
| 协议实现 | `xross-adapter-localsend`（门面 `src/lib.rs`；协议本体在 vendored `vendors/localsend-rs`） |
| 接收 | `src/server.rs`：axum+rustls，路由 `/api/localsend/v2/{info,register,prepare-upload,upload,cancel}`（server.rs:799-809） |
| 发现 | `src/discovery.rs`（MulticastDiscovery）+ `src/scan.rs`（LanScanner TCP 探测） |
| 发送 | `src/send.rs`（LanSender/LanUpload/SendProgress） |
| 同意 | `src/consent.rs`（PendingRequest/TransferDecision；400=永不接受 vs 403=人拒绝） |
| composition root | `crates/app/xrossd/src/localsend.rs`（ReceiveServer 组装 :1457，announce 周期 :76） |
| 控制面 | `control.proto:195-241`（GetLocalSendStatus/SetLocalSendConfig/SetLocalSendEnabled/RestartLocalSend） |
| 回归测试 | `crates/adapter/xross-adapter-localsend/tests/`：over_the_wire / receive / scan / send / tls / web_share（6 个） |
| 结论 | **不写替代品**。interop LocalSend provider 复用该 adapter 模式；mock 闭环用自有 fake peer；真机回归以主仓 6 测试为门槛（INT-04）。**用户 2026-09-15 澄清**：`~/Dev/localsend-rs` 属另一项目，不作为本项目输入（已移出复用候选）。 |

### Transfer —— 状态：稳定

- wire：`crates/contract/xross-transfer-contract/src/protocol.rs` — `SERVICE_ID="xross.transfer.v1"`；`TransferOffer/V1`、`OfferedPayload::{File,Tree}`；`TransferEvent{Started{verified_bytes,total_bytes}, Progress, Published, Failed, Cancelled}`——**resume 语义 = `Started.verified_bytes`**；本地动词 `SEND_START/SEND_FOLLOW/SEND_CANCEL/RECEIVE_WATCH/RECEIVE_CANCEL/RECEIVE_PIN`（protocol.rs:29-163）；wire 冻结测试 `tests/wire.rs`。
- 行为：`crates/service/xross-svc-transfer` — `ConsentTable`（consent.rs:232，`admit` :299 / `decide` :389，PENDING_LEASE=300s）、`publish.rs` `Staging→Published`、`receive.rs` `TransferReceiver`、`watch.rs` `ReceiveWatcher/WatchTable`。
- lease 状态机：`crates/runtime/xross-node-runtime/src/lease.rs` — `Lease/LeaseState{Open,Completed,Cancelled,TimedOut,Failed}`、`LeaseStore`（operations.json，TTL 24h，MAX 1000）；取消 = terminalize + 关闭该 operation 全部 channel。
- **interop 规则**：外部 provider offer 一律进 `ConsentTable`（`ask_about_foreign_offer` 模式，xrossd/src/localsend.rs:32-36）；进度投影到 `TransferEvent`；不建第二套历史（FILE-10）。

### Shelf / Offer —— 状态：稳定（注意"Offer"位置）

- `crates/contract/xross-shelf-contract/src/lib.rs` — `SERVICE_ID="xross.shelf.v1"`；11 本地动词（WATCH/ADD/…/SHARE/AUDIENCE/PULL）；`CAPABILITY` 与 `SHARE_CAPABILITY` 分离（ADR 0114）。
- 模型：`crates/service/xross-svc-shelf/src/model.rs` — `Shelf/ShelfEntry/ShelfView/Materialization`；audience 语义在 `crates/contract/xross-contract-core/src/audience.rs` + `ShelfVisibility`（model.rs:52）；peer 侧 `host.rs` `SharedShelves` trait / `ShelfPeerService`；Loro 复制经 `xross-replica-loro`。
- **关键发现**：主仓没有泛化 "Offer" 类型——offer/accept/reject 语义在 **transfer**（`TransferOffer` + `ConsentTable`）；Shelf 的对应动作是 `PULL`。interop 的 `ShareOffer` 投影到 TransferOffer/ConsentTable（INT-03），保留既有 audience 语义。

### Content leases / content authority —— 状态：稳定（概念需对齐）

- **不存在 `ContentLease`/`LeaseId` 类型**。三个实际概念：
  1. 操作 lease（写授权）：`node-runtime/lease.rs`（`authorization_binding` 绑定 owner+资源，`service_metadata`+`checkpoint_revision` 存进度；ADR 0004）
  2. 本地 HTTP 读 lease：`crates/adapter/xross-adapter-files-http/src/lease.rs` — `HttpLease/HttpLeaseStore/LeaseScope`（scoped/过期/单 entry）
  3. 内容同一性：`crates/contract/xross-content` — `ContentHash`（`xr2:`）、`BlobDigest`（`b3:`），冻结域
- materialize/publish：`svc-transfer/publish.rs` + `crates/platform/xross-safe-fs`（pinned root 安全落盘）；blob 读授权在 `crates/kernel/xross-blob-bridge`（per operation/channel）。
- **interop 规则**：ContentWritePort/ContentReadPort 映射到 LeaseStore + publish/safe-fs + HttpLease 模式；**不新造 ContentLease 抽象**（映射表见 baseline `host_port_mapping`）。

### LocalControl —— 状态：稳定

- `crates/contract/xross-control-api`：transport.rs（`ControlAddress::default_for_state_dir`、`peer_uid` 校验 :366）、auth.rs（`ControlToken`）、methods.rs（~45 个 `xross.control.v1:*` 能力常量，`MethodSpec` 表编译期强制，:552/:572）、credentials.rs（`CredentialStore`）。
- 事件：`crates/app/xrossd/src/events.rs` `EventBus`（publish 带序号 :68、`replay_from` :108）；`Subscribe` 流（control.proto:841）；transfer 另有 `RECEIVE_WATCH` 专用流（快照+回放）。
- 客户端：`xs` CLI、TS `packages/sdk`、native `libxross_client`。
- **interop 规则**：interopd 作为 `ControlServiceClient` 客户端接入；T09 的 JSON-RPC 是 interop **内部**契约，与主仓 gRPC 不同 carrier，经唯一 adapter 转换。

### 媒体接口 —— 状态：**absent**（T03-02）

- 全仓无 `MediaSession`/`MediaGraph`/投屏/播放器子系统（grep 为空）；north-star :241 明确划掉 "media codecs"（narrowed 2026-08-19）。
- 相邻物：`xross-adapter-files-http`（M6：播放器可 seek 的本地 URL，读侧 seam `client_port::RemoteReadClient` trait，lib.rs:44-52）、`xross-capture-contract`（截屏能力）、`xross-crossflow`（活跃开发）、`xross-displays`。
- **处理**：mock host 先行；媒体集成标 `blocked-dependency`；interop 首批媒体呈现走 interop 独立 native player（UxPlay 模式），待主仓媒体接口出现后再映射 `MediaHostPort`。

### Profile runtime / provider 位 —— 状态：seam 稳定，无插件系统

- 主仓 "profile" = 账号 profile（control.proto ListProfiles :951）。无 provider/protocol 插件、无 feature-gated 动态加载。
- 可类比机制：`xross-service-host` trait + `ServiceRegistry::register_*`（registry.rs:40-166）；capability head 模型（`xross-capability-registry` + `xross-capability-port` FFI + `platform-bridge` 双向流 + `xross-platform-head` supervisor `--exit-with-pid`）。
- **interop 规则**：interop provider 不伪装成主仓服务；interopd 独立进程，经唯一 adapter 驱动主仓。

### 身份 / 设备 / Fabric —— 状态：冻结级

- `crates/kernel/xross-fabric`：`DeviceIdentity::load_or_create`（identity.rs:69）、`Fabric::bind_with`（endpoint.rs:103）、pairing permits、`PeerAuthorizer`。
- `crates/service/xross-svc-devices`：`DeviceDirectory`（directory.rs:279，`apply_lan`/`reconcile_fabric`）、`route.rs choose_route`（LocalSend vs fabric 选路）。
- **interop 规则**（INT-02）：interop 不创建第二套身份；`EndpointId` 只是本地观察标识，用户显式 alias 才关联 DeviceDirectory。

### proto 面

`control/v1/control.proto`（本地集成）、`fabric/v1/wire.proto`（peer 对等）、`platform/v1/bridge.proto`（native head）。Rust 侧 build.rs + prost/tonic，不经 buf。

## 2. HostPorts ↔ xross-dev 真实类型对照

| HostPort（docs/04 拟议） | xross-dev 真实入口 | 稳定性 |
|---|---|---|
| AdmissionPort | `xross-svc-transfer::consent::ConsentTable.admit/decide` | stable |
| ContentReadPort | `xross-service-host` BlobReadAccess/FetchService + files-http `HttpLease` 模式 | stable |
| ContentWritePort | `node-runtime LeaseStore` + `svc-transfer publish` + `xross-safe-fs` | stable |
| HistoryPort | `TransferEvent` + `RECEIVE_WATCH`（WatchTable） | stable |
| MediaHostPort | **无**（media absent） | absent → mock host 先行 |
| IdentityLinkPort | `DeviceDirectory`（用户显式 alias） | stable |
| NetworkPolicyPort | xrossd localsend 组装点 + control.proto LocalSend 开关 | stable |

## 3. 唯一 adapter seam

**XrossHostAdapter：以 `ControlServiceClient`（gRPC/UDS + ControlToken）为 xrossd 的唯一接入点。**

理由：主仓唯一受支持的本地正门是 daemon 控制面；interop 侧只实现这一个 adapter trait，主仓升级只影响该 adapter；**不建平行模型**——Offer 用 TransferOffer+ConsentTable，lease 用 LeaseStore/HttpLease，身份用 DeviceDirectory，均不做第二套（docs/04 §9 "不能用拟议名称新建并存模型"）。

## 4. localsend-rs 复用面评估（goal 要求）

- 主仓已 vendored `CrossCopy/localsend-rs` 并有成熟 adapter + 6 回归测试：**LocalSend 协议层零重复实现**。
- interop 基础阶段的 LocalSend 工作 = (a) mock fake peer（T14 testkit）做契约回归；(b) 经 XrossHostAdapter 的 offer/审批对接设计；(c) 真机互通门槛沿用主仓测试 + 后续 T15+ provider 接入。
- 结论：**不引入任何独立 localsend 库**——复用面 = 主仓 adapter；用户自有 `~/Dev/localsend-rs` 属另一项目（用户 2026-09-15 声明），不作为输入。

## 5. 范围冲突记录（T03-03）

| id | 冲突 | 裁决 |
|---|---|---|
| SC-1 | xross-dev north-star 划掉 media codecs/投屏（2026-08-19）；用户 interop 目标要求 AirPlay/Miracast/Cast | **keep-in-scope**（按用户目标）：媒体协议保留在 interop 计划，先 lab/standalone 闭环；不因主仓 MVP 收窄砍协议 |
| SC-2 | 设计包拟议本地 IPC = 长度前缀+JSON-RPC；主仓真实控制面 = gRPC/UDS | **keep-in-scope**：T09 按设计包实现 interop 内部契约；对接经 adapter 说 gRPC |
| SC-3 | 设计包拟议 ContentLease 在主仓无同名物 | **keep-in-scope**：interop 保留拟议抽象供 standalone；映射表记录真实对应 |

## 6. 验证

`python3 -m unittest discover -s tools -p 'test_*.py'`：10 tests OK（exit 0），含 baseline commit 历史锚定校验、30 anchors 存在性、T03-01/02/03 语义断言。抽查过的符号：ConsentTable（consent.rs:232）、peer_uid（transport.rs:181/187）、6 个 localsend 测试文件、MediaSession grep 为空、vendored 与用户副本 origin 一致。
