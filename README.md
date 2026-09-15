# xross-interop-lab

私有研究仓库（private lab）。存放互操作协议的来源清单、研究档案、实验证据与决策记录，
以及实现 workspace（`impl/`）。**本仓库不公开、不发布任何产物**（见 AGENTS.md）。

## 仓库边界

| 仓库 | 可见性 | 职责 |
|---|---|---|
| `xross-interop-lab`（本仓库） | private | 来源清单、研究笔记/dossier、设备记录、脱敏抓包、决策与证据，**以及实现 workspace（`impl/`）** |
| `xross-dev` | 产品仓库 | XROSS 身份、设备、Shelf、传输/媒体产品与集成 adapter |

设计包原规划的独立实现仓 `xross-interop` **暂不创建**：实现代码先在本仓库 `impl/` 内演进；
未来是否开源由用户决定，届时把许可清理过的 `impl/` 子集复制成新仓库。
私有仓库与新 Git 历史都不是版权豁免：所有第三方源码使用必须保留真实 provenance，规则见 [AGENTS.md](AGENTS.md)。

## 目录结构

```
impl/                           # 实现 workspace（plans/01 代码路径映射到此处）
  crates/interop-contract/      # 领域契约：ids/capability/offer/media/error + golden + JSON Schema
  crates/interop-policy/        # scoped grant（默认拒绝跨 scope）
  crates/interop-file/          # 受限流式存储：路径形状/预算/原子 commit/hash
  crates/interop-runtime/       # HostPorts + fake host（T07）；
                                # 会话注册表/事件/取消（T10）
  crates/interop-ipc/           # 本地控制面：长度前缀 JSON-RPC + hello 认证
  crates/interop-testkit/       # fake clock/确定性分片/fixture 登记/run manifest/对抗性输入扫描（T46）
  crates/interop-media/         # 媒体形态/三时钟域/format tracker/有界帧队列 + null/file sink（T28/T29）
  crates/interop-hardening/     # 只放测试：48 个线上解析入口的对抗性扫描 + 分配预算（无运行时依赖，T46）
  crates/proto-airplay/         # AirPlay legacy 控制链：严格 RTSP/能力门禁/Bonjour 类型/keying seam（T32）
  crates/proto-quickshare/      # Quick Share/UKEY2：TCP framing/握手状态机/payload gate（T19/T20）
  adapters/standalone-host/     # standalone 显式 policy + 最小权限 host
  apps/interopd/                # headless daemon 骨架（UDS 0600）
  apps/interop-demo/            # headless demo CLI：发现/会话/sink/XMD1/Quick Share（T5）
  schemas/interop-api.schema.json
references/repositories.json    # 来源清单（64 项，均未放行）
references/sources.lock.json    # 本机实际 HEAD + 许可文件 hash（2026-09-15）
references/external/<slug>      # → /Volumes/Portable2TB/ExtDev/others/<slug> 的 symlink（不入库）
references/research/xross-interop-plan-2026-09-15/  # 设计输入包（勿修改）
docs/goal-phase-1.md            # 基础阶段 goal prompt（已完成）
docs/goal-phase-2.md            # 阶段 2 goal prompt（已完成）
docs/goal-phase-3.md            # 阶段 3 goal prompt（投屏 + 近场传送协议核心，headless 优先）
docs/research/airplay-research.md  # AirPlay 真机 POC 记录（UxPlay/shairplay-rust）
research/                       # 来源 intake + 逐 profile dossier（T02/T04 产出）
specs-reviewed/                 # 审查后的 wire spec（F01/M01 实质；其余 review-pending）
decisions/                      # source-allowlist / xross-contract-baseline / provider-adoption / ADR（见 decisions/README.md 台账）
provenance/                     # approved-inputs / review-log
evidence/                       # run manifest（run.schema.json 校验）+ intake 扫描
tools/                          # 仓库检查脚本与测试（python3 -m unittest discover -s tools）
                                # + capture_redact.py：pcap → 白名单式脱敏摘录（S3 用，自测同目录）
```

参考源码放在仓库外 `/Volumes/Portable2TB/ExtDev/others/`，以 slug 名浅克隆 default branch，
共 63 项约 1.6 GB；R41（AOSP frameworks/av）因入口未核验且为巨仓而跳过。本仓库不 vendor 第三方源码树。

## 证据等级

`catalogued → source-reviewed → build-verified → simulated → device-verified → release-qualified`；
受阻任务标 `blocked` 并写明障碍与重评条件。README 声明、模拟自通、未固定 commit 一律不升级证据等级。

## 当前状态（2026-09-15，阶段 3：投屏 + 近场传送协议核心，headless 部分完成）

**阶段 2（2026-09-15，全部完成）**

- **T0 执行与信任边界（ADR-003，accepted）**：产品集成 = xrossd 内第一方 interop bridge
  （协议名到此为止）；协议执行 = 隔离低权限 worker（supervisor 管理，只经 scoped 能力面，
  禁用 `control.v1` 全权）；provider 部署类 A/B/C（LocalSend=A grandfathered）。
  见 [decisions/adr-003-execution-and-trust-boundary.md](decisions/adr-003-execution-and-trust-boundary.md)
  （台账 [decisions/README.md](decisions/README.md)）；`decisions/provider-adoption.json` 增
  `deployment_class`/`source_role`；恢复 ADR 纪律测试（ADR-01..04）。用户采纳原话记录于
  [provenance/review-log.md](provenance/review-log.md)。
- **T11 endpoint registry**：`interop-platform` 新 crate（观察形状校验、明文候选如实标注、
  确定性 fake 源）+ registry/路由——去重键含来源与 identity claim（**同名/同 IP/同地址都不合并**）、
  地址候选带 TTL 与接口（接口断开即失效）、用户 alias 只生成**派生展示分组**（不参与授权）、
  路由检查 purpose/方向、能力与媒体形态、安全策略（默认禁明文回落）、平台状态（拒绝而非降级）。
- **T12 只读 platform probe + radio lease**：`xinterop doctor [--json]`（白名单只读命令、不经 shell、
  只保留接口名）；radio arbiter 共享/独占 lease（pending 期间零网络变更、批准后一次 apply、
  释放/退出按 rollback 恢复）。真机只读输出见 [research/platform-probes.md](research/platform-probes.md)
  （macOS：P2P/WFD 无公开 API → `unavailable`；媒体输出 `not-run`——不伪装可用）。
- **T13 worker supervisor + 真实隔离**：binary hash pin、固定参数、环境清空 + 白名单、parent pipe
  bootstrap、有限重启（4 次/5 分钟）、关闭无孤儿；canary 探针在**本机真实 OS 沙箱**
  （macOS sandbox-exec）下被内核拒绝（`permission denied`）→ 等级 `platform-sandbox`；无沙箱时
  如实报 `process-only` 并限制可发布 profile。真实子进程 worker 在 `impl/workers/mock-provider`。
- **T4 seam 模拟实证（`simulated`）**：外来 offer → registry/路由 → Entry scoped grant → HostPorts →
  interop-file 原子落盘 → EventBus 投影；负向 = 无 grant 拒绝、Vault 越权、预算、路径形状、
  symlink no-follow、监听默认关闭、无渲染器不开假窗口；媒体面只落本地 spool（不上 fabric）+
  worker 崩溃只影响挂靠会话。**simulated 不冒充真机**。
- **T5 首个真机 vertical gate**：[research/vertical-gates.md](research/vertical-gates.md)——
  AirPlay 优先（T28→T29→T30 路线）、Quick Share 待 F02 wire；R01–R12 用途/边界盘点、
  A1–A10 真机验收清单、**5 条待用户批准的 scope 行（S1–S5）**。
- **测试面**：`cargo test --manifest-path impl/Cargo.toml --workspace` **69 tests 全绿**
  （46 → 69）；clippy 0 warnings；`python3 -m unittest discover -s tools` **28 tests OK**（含 ADR 纪律）。

**基础阶段（T01–T14，2026-09-15 完成）**

- **T02 来源 intake**：file-core+airplay 19 项只读扫描（[research/source-intake.md](research/source-intake.md)、
  [decisions/source-allowlist.json](decisions/source-allowlist.json)）；restricted：R10/R13/R16（许可不明）；
  7 个 foreign AGENTS/CLAUDE/.kiro 全部登记为 `untrusted-material`；全部 `production_approved=false`。
- **T03 Xross 集成映射**：[research/xross-integration-map.md](research/xross-integration-map.md) 锚定
  xross-dev `099b6c72`（该仓为活跃工作区，测试按"基线是 HEAD 祖先"校验）；唯一 seam =
  XrossHostAdapter（gRPC/UDS）；**媒体接口 absent → mock host 先行**；LocalSend reuse-first。
- **T04 dossier**：F01/F02/M01/M05/M07/M08 六份（机器可读证据等级/方向/probe，由
  [tools/test_dossiers.py](tools/test_dossiers.py) 强制）；M01 折算 airplay-research.md：
  transient 链路 device-verified 但 `capability_status=blocked`（画质/音频/PIN 三障碍）。
- **T05 许可路线**：[decisions/provider-adoption.json](decisions/provider-adoption.json)——
  UxPlay→worker(外部二进制 oracle/fallback)、shairplay-rust→仅参考（用户实测质量不达标，不链接）、
  LocalSend→reuse（主仓 adapter；用户自有 localsend-rs 属另一项目不引入）、
  QuickShare 参考→independent、GStreamer→worker、WinRT→independent、**FairPlay→永久 vendor-gated**；
  全部 `production_approved=false`（放行权保留给用户）。
- **T01/T06–T10/T14 实现**：契约/策略/存储/IPC/会话/测试基建，见
  [evidence/2026-09-15-phase-1-final-report.md](evidence/2026-09-15-phase-1-final-report.md)。

**阶段 3 实现（headless 优先，2026-09-15）**

- **T28 媒体形态/时钟/数据帧**：`crates/interop-media`（Encoded/Pcm/NativeOnly/Resource 四类 source、
  三时钟域分离、format 变化必须 decoder reset、有界帧队列）+ `interop-ipc::media_frame` 的 36 字节
  XMD1 头（未知 critical flag 拒绝、>16MiB 分配前 `resource-limit`）；native 只传 opaque id。
- **T29 sink**：null sink（无 DISPLAY/音频设备也能统计）、file sink（自有内容落盘 + 帧索引）、
  44.1k→48k 显式 resample（147/160，时长不变）；native 窗口 sink **明确返回 `platform-unavailable`**
  （不以屏幕录制假装 raw output），留待桌面会话手动验证。
- **T31 AirPlay gate**：[specs-reviewed/m01](specs-reviewed/m01-airplay-legacy-mirror.md) 的字段级事实表
  （每行带来源）、能力分声明（视频/音频/PIN 分开，全 blocked）、FairPlay 永久 vendor-gated、
  [evidence/airplay-corpus](evidence/airplay-corpus/corpus-plan.json) 语料计划 + `tools/test_airplay_gate.py`。
- **T32 AirPlay 控制链**：`crates/proto-airplay` —— 严格 RTSP framing（缺失/重复 `Content-Length` 拒绝、
  长度检查先于分配）、能力广告**由实现清单推导**（广告未实现 codec = 能力测试失败）、
  两种 Bonjour 服务类型、keying seam（默认 `UnavailableKeying` → SETUP 503、认证前零分配）、
  100 次快速断开无 session/端口残留。
- **T19/T20 Quick Share / UKEY2**：[specs-reviewed/f02](specs-reviewed/f02-quickshare-lan.md) 的 F-01..F-24
  字段表（每行来源 + 状态 + 是否准入实现）+ 两处**规范/实现冲突的具名裁决**（F-12 cipher 选择、F-15 HKDF 哈希）；
  `crates/proto-quickshare` 实现 TCP 4 字节大端 framing、UKEY2 三消息状态机（真实 P-256 ECDH /
  SHA-512 commitment / HKDF-SHA256）、Alert 码表、重放拒绝、payload gate（未核对 4 位确认码不放行）。
  **发现/QR/传输加密/payload 层未实现**（P-F02-1/3 未关闭），gate 测试机器保证实现里没有发现代码。
- **headless demo**：`apps/interop-demo`（`xinterop-demo [run|discovery|session|sink|media|qshare|mirror|airplay|dlna|wfd|cast] [--json]`）
  把上述各层跑成真实状态输出：注册表不合并 identity、SETUP 503/401 与 131072 B 分配时机、sink 统计与
  显式 resample、XMD1 往返与负向、UKEY2 握手与分片等价；输出恒带 `evidence_level=simulated` + blocked +
  待用户手动清单（见 [evidence/2026-09-15-t5-demo-cli](evidence/2026-09-15-t5-demo-cli/demo-report.txt)）。
- **验证**（阶段 3 收尾时）：`cargo test --manifest-path impl/Cargo.toml --workspace` **113 tests 全绿**；
  clippy 0 warnings；`python3 -m unittest discover -s tools` **39 tests OK**（阶段 4 后为
  **273 tests / lab 100 tests**，见下）。每 task 的 run manifest 见
  [evidence/index.json](evidence/index.json)。

**阶段 4（2026-09-15 稍后追加；协议切片全部完成，另加一次输入加固）**

- **T21 Quick Share 传输闭环（headless 部分）**：`crates/proto-quickshare` 新增
  `secure_message`（D2D 密钥链 + SecureMessage AES-256-CBC/HMAC-SHA256 + 严格 +1 序号，跳号/重放即
  `integrity-failed`）、`payload`（OfflineFrame/V1Frame/PayloadTransferFrame 编解码 + 常数内存组装器，
  10 GiB 合成流有测试）、`receive`（introduction 报价投影 → 用户裁决 → 流式落盘 → 原子发布；
  payloadID↔entryID 绑定、预算、CANCEL 清理；落盘经 `interop-file`，文件名穿越由 FILE-05 拒绝）。
  证据：[evidence/2026-09-15-t21-quickshare-transport](evidence/2026-09-15-t21-quickshare-transport/run-manifest.json)；
  demo 的 `qshare` 段现在能一次跑完 握手→加密→分块→落盘 hash 对照。**发现与真机仍未做**（P-F02-1/3）。

- **T33 AirPlay 镜像接收路径（headless 部分）**：`crates/proto-airplay` 新增 `mirror`（格式协商→encoded AU
  直通 sink；背压超预算进入**关键帧恢复**；未配置/无 reset 的帧被拒）、`audio`（只收 PCM 块，44.1k→48k
  **显式重采样**、ALAC 需 decoder → 拒绝）、`timing`（按连接的 clock generation + 漂移观测）。
  **不含 decoder**：能力广告保持为空（能路由 ≠ 能显示）。顺带修掉 `interop-media` sink 侧复判 format
  变化的接口缺口。证据：[evidence/2026-09-15-t33-airplay-mirror](evidence/2026-09-15-t33-airplay-mirror/run-manifest.json)；
  demo 新增 `mirror` 段（`xinterop-demo mirror`）。

- **T41 DLNA/UPnP AV（headless 切片）**：`crates/proto-upnp` 新增 `xml`（加固子集：无 DTD/ENTITY/CDATA
  → 无 XXE；深度/体积/元素/属性预算是**本仓策略值**而非协议常量）、`ssdp`（严格校验 MAN/MX/ST/NTS，
  USN 去重 + max-age）、`soap`（动作与参数白名单，`SOAPACTION` 绑定，只认 InstanceID=0）、
  `dmc`（描述抓取策略：仅 http、拒 loopback/链路本地/云元数据/带 userinfo → `permission-required`；
  注册表按对方声明的 `protocolInfo` 判 push 能力；URL lease 在 Stop/取消/超时时可撤销）、
  `dms`（受限根 + 显式注册子项才算授权；701/402 语义；分页上限）。
  **不做真电视矩阵、不做 GENA 事件投递、不做媒体字节传输，也不提供屏幕镜像替代**（P-M07-1 未关闭）。
  证据：[evidence/2026-09-15-t41-dlna-control](evidence/2026-09-15-t41-dlna-control/run-manifest.json)；
  demo 新增 `dlna` 段（`xinterop-demo dlna`，见 D-10）。

- **T37 WFD/Miracast（headless 切片）**：先做 gate——`specs-reviewed/m05` 重写为 F-01..F-32 字段表
  （M1..M8/M13/M16 的方法与**方向**、媒体协商描述符、子元素、RTP 封装，每行带来源 commit+文件+行），
  blocked = F-24（WFD IE 容器 OUI）、F-25（P2P 组形成/平台 API）、F-30（真机矩阵）；
  `provenance/m05-inputs.json`（R32/R33/R34/R37 facts-only，R35/R36 不进实现）+ 语料 18 条
  （规则明确禁止任何 OUI 猜测值）+ `tools/test_wfd_gate.py`（9 例）。
  实现 `crates/proto-wfd`：消息层（含走私/长度纪律）、协商层（`wfd_video_formats` 三段与 13 字段描述符、
  `wfd_audio_codecs` 三元组与 mode 位、端口/传输形状与 RTCP quirk）、子元素编解码、
  RTP 固定头记账与按**本仓策略**的关键帧请求；**完整 WFD IE 的构造与解析一律 `unsupported-feature`**
  （OUI 不臆造），HDCP 只解析并拒绝，`player_available` 恒 false（能协商 ≠ 能显示）。
  证据：[evidence/2026-09-15-t37-wfd-control](evidence/2026-09-15-t37-wfd-control/run-manifest.json)；
  demo 新增 `wfd` 段（`xinterop-demo wfd`，见 D-11）。测试逼出四处实现缺陷（M3 查询正文是**裸参数名**、
  M13 正文是裸 `wfd_idr_request`、keep-alive 基准、M13/M16 应用控制 URI），详见 run manifest 的 red→green 记录。

- **T36 AirPlay 音频路径控制请求（headless 切片）**：`specs-reviewed/m01` 增 T36 增量（实测来源）——
  `/audioMode`（`POST`，binary plist 体含 `mode`，实测取值 `default`）与 `/feedback`（实测每 2 秒一次，
  体含 `elapsed_ms` 且单调增）。实现 `crates/proto-airplay/src/audio_control.rs`：**只做形状校验与观测记账**——
  方法/路径/Content-Type/键名逐项校验，`/feedback` 记录到达间隔、单调性与对实测 2 s 的偏离；
  `mode` **只声称实测到的 `default`**。**不实现心跳语义、不据 `elapsed_ms` 推任何时钟**：实测记录原文写的是
  「疑为请求处理耗时字段被复用的痕迹，**待考**」，语义钉死之前本仓只回答"观测到了什么"
  （lab gate 机器检查实现里不出现 `clock`/`playback_position` 之类推断）；plist 体按**不透明**处理
  （允许的来源清单里没有 bplist 规格）。demo `airplay` 段新增子块（D-18）。
  证据：[evidence/2026-09-15-t36-airplay-audio-control](evidence/2026-09-15-t36-airplay-audio-control/run-manifest.json)。

- **T42+ DLNA GENA 通知构造与调度（headless 切片）**：`specs-reviewed/m07` 增 F-23..F-29（R46 pupnp 设备侧 +
  R49 rygel 服务侧行级来源）——NOTIFY 字节（`NT: upnp:event`/`NTS: upnp:propchange`/`SID`/`SEQ`，
  且 **`Content-Length` 是正文字节 + 2**，因为正文以 `\n\n` 结尾）、propertyset 正文（`<e:property>` 每变量一条，
  **不发送 XML 声明**——来源把宏留着并注释「与其他 UPnP 厂商不互操作」）、LastChange 的内嵌文档
  （`<Event xmlns="…AVT/|…RCS/"><InstanceID val="0">` + `<VAR val="…"/>`，带通道的变量多一个 `channel`）。
  两条容易做错的来源事实被固化进实现与测试：① propertyset 构造把值**原样**写入 ⇒ **转义责任在产値的一方**，
  而 LastChange 的值正是一份内嵌 XML ⇒ 必须先整体转义；② `SEQ` **从 0 开始**（初始事件）后严格 +1。
  合并窗口 **150 ms 是来源实现取值**（rygel），本仓可配并如实标注。实现 `crates/proto-upnp/src/gena.rs`：
  只产出通知**字节**与记账，**不建立任何回调连接**——字节交给调用方实现的 `NotifyTransport`
  （lab gate 机器检查模块内无网络客户端）。demo `dlna` 段新增 GENA 子块（D-17）。
  证据：[evidence/2026-09-15-t42p-dlna-gena-notify](evidence/2026-09-15-t42p-dlna-gena-notify/run-manifest.json)。

- **T22headless Quick Share 断开与确认帧（headless 切片）**：`specs-reviewed/f02` 增 F-34..F-40（R17 官方 proto
  + 参考实现 + NearDrop 行级来源）——`DisconnectionFrame`（外层 `DISCONNECTION(6)`/字段 7，`request_safe_to_disconnect=1`、
  `ack_safe_to_disconnect=2`）与 `PAYLOAD_ACK`（`packet_type=3`，只带 `payload_header{id,total_size=-1}`）。
  三处关键事实：① **同一帧在参考实现里有两种合法字节形态**——R17 的构造器总是显式写两个 bool（false 也写），
  NearDrop 发的是**空正文**，两者字节不同 ⇒ 解码必须保留字段存在性（本仓用 `Option<bool>` + `has_request()/has_ack()`）；
  ② `PAYLOAD_ACK` 的启用条件**明确排除 BYTES 载荷**（协商类载荷不发 ack），且只有末块才发；发送侧对未知 payload
  与对本端 incoming payload 的 ack 一律**忽略**；③ keep-alive 参数其实是**握手协商字段**（`keep_alive_interval_millis=8`/
  `keep_alive_timeout_millis=9`，proto 无默认值）——**修正了此前"来源只说 a while 没数值"的表述**，本仓不实现连接
  握手，故只做校验 + 回退并如实标注来源。另：已废弃的 `ControlMessage.PAYLOAD_RECEIVED_ACK` 路径收到即**明确拒绝**
  （不静默忽略）。实现见 `crates/proto-quickshare/src/control.rs`；demo `qshare` 段新增三块（D-16）。
  证据：[evidence/2026-09-15-t22h-quickshare-disconnect-ack](evidence/2026-09-15-t22h-quickshare-disconnect-ack/run-manifest.json)。
  safe-to-disconnect 的**带宽升级路径本身仍不实现**（只做成帧与决策）。

- **T45 Cast receiver 可行性 gate（headless 切片）**：`specs-reviewed/m08` 增 F-26..F-35（全部 R42
  行级来源）——**认证在消息层，不在 TLS**（sender 恒定跳过 TLS 证书校验，真正的检查是把 `AuthResponse`
  的证书链走到信任库；默认只信厂商根，不可信链 → `kCastV2CertNotSignedByTrustedCa=66`）；sender 不出示
  证书、receiver 出示 TLS 证书 + `AuthResponse`；参考实现的 test-root 路径（`--generate-credentials`
  自签根，3 天寿命）与 receiver 侧消息流（CONNECTED 仅在 sender 给了协议版本时回、LAUNCH 未知 app id →
  `kItemNotFound`、进程内 app 注册、TXT `id`/`ve`/`ca`/`st`/`fn`/`md`、参考 receiver 用 8010 而真实设备
  8009）；`research/cast/receiver-gate.md` 给出判定：**stock sender 连上本机 receiver = blocked**，
  可自动化的是"可被发现 + 控制面 + LAUNCH 白名单 + test-root 自配对闭环"。实现
  `crates/proto-cast/src/receiver.rs`：生产闸门 `VendorTrustRequired` **恒拒绝且拒绝后不留半开会话**、
  演示闸门 `TestRootTrust` 只在对方**显式**信任同一测试根时放行、**不内置任何 app id**、
  CLOSE 释放全部记账；demo 的 `cast` 段新增 receiver 子块（D-15）。
  证据：[evidence/2026-09-15-t45-cast-receiver-gate](evidence/2026-09-15-t45-cast-receiver-gate/run-manifest.json)。
  gate 机器纪律在本 task 里第二次挡住越界：生产闸门原名 `VendorDeviceAuth` 被禁词检查抓到 → 改名。

- **T34 AirPlay 音频 profile 与配对登记（headless 切片）**：`specs-reviewed/m01` 增 T34 字段表
  （行级来源：UxPlay@d9791de / shairplay-rust@2fb72b3）——AP1 实时音频的请求键与响应形状、
  `ct` 四值表（1=PCM、2=ALAC spf=352、4=AAC-LC spf=1024、8=AAC-ELD spf=480）、采样率 44100
  为**来源取值**；AP2 的**两套编号**（打包 `audioFormat` 与 RTP SSRC 魔数，参考实现注释明说
  "not RTP SSRC values"）、流类型 96/103/110/120/130、配对端点集与**不是 TLV8**（UxPlay 全仓
  grep 零命中；`/pair-setup` 收发裸 32 字节）、`OneTimePairingRequired` = statusFlags bit 9。
  实现 `crates/proto-airplay/src/{audio_profile,pairstore}.rs`：profile 解析（未知值一律拒绝、PCM
  的 spf 不编造、两套编号互不回退、SSRC 从 `packet[8:12]` 取且只有非 0 变化才切换）与配对
  **登记层**——结论枚举只有 `Unknown`/`KnownButUnverified`（**`Authenticated` 被 lab gate 机器禁止**），
  `/fp-setup` 恒 vendor-gated，配对握手明确不实现（无 SRP/X25519/Ed25519）。
  证据：[evidence/2026-09-15-t34-airplay-audio-store](evidence/2026-09-15-t34-airplay-audio-store/run-manifest.json)；
  demo 新增 `airplay` 段（`xinterop-demo airplay`，见 D-14）。

- **T21+ Quick Share 控制帧（keep-alive 与 paired-key，headless 切片）**：`specs-reviewed/f02` 补
  F-30..F-33 —— F-30 裁决**两层编号冲突**（外层 Nearby Connections 与内层 Nearby Share 用各自的
  `V1Frame.FrameType` 编号，同号不同义；内层帧包在 BYTES payload 里，依据 R15
  `NearbyConnection.swift:207-226` + PROTOCOL.md:204-206），F-31 keep-alive（10 s 为**来源取值**、
  超时与判死基准为**本仓策略**）、F-32/F-33 paired-key 帧（材料**不可离线推导** → 只做帧与状态机，
  材料由调用方给）。实现 `crates/proto-quickshare/src/control.rs`：外层 KEEP_ALIVE(5) 与内层
  PAIRED_KEY_ENCRYPTION(3)/RESULT(4) 分列编解码、10 s 节奏 tracker（收到即回 ack、判死后停收发）、
  BYTES 载荷装载/解出（`kind` 非 BYTES 或 offset≠0 一律拒绝）、交换状态机（对端 encryption → 回
  `UNABLE`；**默认策略即使对端报 `SUCCESS` 也仍要求 4 位确认码**，只有调用方显式
  `allowing_skip_confirmation()` 才跳过）。控制帧层**不含任何密码学原语**（lab gate 机器检查）。
  证据：[evidence/2026-09-15-t21b-quickshare-control](evidence/2026-09-15-t21b-quickshare-control/run-manifest.json)；
  demo 的 `qshare` 段新增控制帧块（`xinterop-demo qshare`，见 D-13）。qs-020 的"对端静默"要求抓出一处
  **真实缺陷**：判死基准原为 `max(last_sent, last_received)`，自己继续发心跳就把判死无限推后——只发不听
  的连接永远不会被判死；现在基准是"最后一次**收到**的对端帧，从未收到时用首个心跳"。

- **T43 Google Cast 媒体 URL sender（headless 切片）**：gate 重写 `specs-reviewed/m08` 为 F-01..F-25 字段表
  （CASTV2 信封字段号与 required 声明、4 字节长度前缀与 64 KiB 上限、namespace 集合、四个命名空间的
  载荷键、requestId/超时、DeviceAuth 消息与发送端强制认证点、mDNS 服务类型与 TXT 键），
  blocked = P-M08-1（真机认证强制点）/P-M08-2（组件边界）/P-M08-3（TXT 矩阵）+ F-25（媒体字节服务）；
  **两条硬边界**：认证握手不实现（凭据材料不可得 → 生产闸门恒拒绝）、app ID 不申请不伪造。
  实现 `crates/proto-cast`：信封层、载荷层（未知命名空间/类型拒绝、取值白名单）、
  sender 控制会话（连接→拉起→媒体会话→收尾；三条 URL 释放路径）、TXT 只解析不组播。
  证据：[evidence/2026-09-15-t43-cast-sender](evidence/2026-09-15-t43-cast-sender/run-manifest.json)；
  demo 新增 `cast` 段（`xinterop-demo cast`，见 D-12）。T43-01 抓出一处真实缺陷：闸门检查原先在
  状态变更之后，认证失败会留下停在 `connecting` 的半个会话。

- **T42 DLNA renderer 接收侧（headless 切片）**：M07 字段表补 F-15..F-22（AVTransport 状态值与
  `TransportState` 允许值、错误码 701/710/711/712/717/718 的语义、`Seek` 单位表、RenderingControl
  动作集与 Volume 0..100、状态变量、服务类型串 `:1`/`:2`），能力表把 GENA 拆成"订阅校验（已实现）/
  事件投递（未实现）"。实现 `crates/proto-upnp/src/dmr.rs`：AVTransport 状态机 + RenderingControl +
  媒体 URL 策略与用户同意（`file://`、内网/元数据目标一律拒绝且不留播放入口）、幂等 `Stop`、
  直播流 `Seek` 明确 710、GENA 订阅校验（越界回调拒绝）——**控制路径不做任何网络 I/O**（lab gate 机器检查），
  GENA 只做校验与拒绝、不建立回调连接。证据：
  [evidence/2026-09-15-t42-dlna-renderer](evidence/2026-09-15-t42-dlna-renderer/run-manifest.json)；
  demo 的 `dlna` 段新增 renderer 子块（D-10 扩展）。

- **T46 不可信输入加固（headless，2026-09-15 追加）**：所有吃线上字节的解析入口现在有**确定性对抗性扫描**。
  `interop-testkit::adversarial`（零依赖：复用既有 xorshift64\* + FNV-1a 派生 seed，固定 seed ⇒ 固定字节集，
  逐入口记 sha256 digest）＋ **测试专用 crate** `interop-hardening`（`[dependencies]` 为空，只做扫描，
  也正因为要独占进程的 `GlobalAlloc` 才单列一个 crate）。48 个入口 × 227 用例 = **10896 个自撰输入**，
  断言四件事：任意字节不 panic（dev profile 的算术溢出检查会抓长度+偏移回绕）、`Ok` 时消费长度落在 `(0, len]`、
  **基线帧必须被自己的解码器接受**（编码器/解码器脱节先炸）、未实现的入口**一个都不接受**
  （`ALWAYS_REJECTS`：WFD IE 容器=OUI 不存在、AirPlay 配对端点=交换不实现）。
  分配预算用进程内计数分配器逐输入测量**峰值存活**（不是累计——累计会把 `Vec`/`String` 的 2 倍扩容算成放大），
  上限 = `max(64 KiB, 8×输入, 流式声明上限, 结构预算)`；大输入按**调用方上限**截断（AirPlay 正文 256 KiB、
  WFD 正文 64 KiB、IP1 帧 256 KiB、SSDP 报文 8 KiB…），炸弹极值停在 16 MiB（4 GiB 的失败模式是进程 abort，
  抓不住也不该抓）。扫描抓出**两处真实缺陷**（red→green 记录在 run manifest）：① WFD 协商参数
  「先全量收集再判段数」——1 MiB 合法 token 串换来 13.1 MB 分配，修法 `take(4)`/`take(5)` 与
  `MAX_VIDEO_DESCRIPTOR_TOKENS = 32`（接受/拒绝判定一字未改）；② 参数正文解析「行数无上限 + 错误消息整行回显」
  ——1 MiB 定形正文换来 14.2 MB，修法 `MAX_PARAMETER_LINES = 64`（**本仓策略值**，已登记为 m05 F-33）、
  控制字符拒绝、`preview()` 128 字节截断。**不作越界声明**：这些断言证明的是"不崩、记账不越界、
  分配不被声明长度牵着走、未实现的入口不接受"，不是协议正确性，更不是互操作证据。
  `tools/test_hardening_gate.py` 把覆盖关系变成机器检查（每个线上解析入口要么有用例标签、要么在**带理由**的
  排除名单里且名单不得过期；无第三方模糊器依赖；`unsafe` 只许计数分配器那一处）。
  证据：[evidence/2026-09-15-t46-input-hardening](evidence/2026-09-15-t46-input-hardening/run-manifest.json)、
  语料计划 [evidence/input-hardening-corpus](evidence/input-hardening-corpus/corpus-plan.json)。

**S3 抓包已批准（2026-09-15，用户配合真机）**：步骤见
[research/capture-runbook.md](research/capture-runbook.md)——①先在 Ubuntu 上用 `btmon` 定**发现介质**
（BLE vs mDNS，这一步没有结论后面就是猜）；②Samsung → ROG(Quick Share for Windows) 走 Mac 热点抓 `bridge100`
拿传输字节；③iPhone/iPad → 库存 Apple TV 抓 `audioFormat`/`ct`/`spf`（**不需要 S1**）。
脱敏管道已就绪：`tools/capture_redact.py`（白名单式：只留服务类型/TXT 键名/协议常量/端口/时序/TCP 首批载荷形状，
设备名与人名一律假名化），原始 pcap 不入库。

**未开始 / 待批准**：T30（UxPlay provider 闭环，需 scope S1/S2）、T33（AirPlay 音视频接收真机）、
T22（Quick Share 发送闭环，需 QR/可发现路径）、Quick Share 发现源
（待 P-F02-1/2/3 关闭与 S3 抓包批准）、T42 的媒体字节服务与 native player 接入（T24/host gateway）、
T38/T39（WFD 真机序列与 IE 播发、平台入口 probe，需 P-M05-1/2/3）、T44（Cast 实时 streaming，在 T43 之后）、
xross-dev 侧 bridge 窗口（S4）、
provider 放行（S5）；native 窗口 sink 与 Tauri demo 壳留桌面会话。
阶段 3 的 goal prompt 见 [docs/goal-phase-3.md](docs/goal-phase-3.md)，执行结果见
[evidence/2026-09-15-phase-3-final-report.md](evidence/2026-09-15-phase-3-final-report.md)；阶段 2 见
[evidence/2026-09-15-phase-2-final-report.md](evidence/2026-09-15-phase-2-final-report.md)。

Agent 工作规则、禁止事项与汇报格式见 [AGENTS.md](AGENTS.md)；基础阶段执行提示词见
[docs/goal-phase-1.md](docs/goal-phase-1.md)（已执行完毕，期末报告见
[evidence/2026-09-15-phase-1-final-report.md](evidence/2026-09-15-phase-1-final-report.md)）。
