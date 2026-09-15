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
  crates/interop-testkit/       # fake clock/确定性分片/fixture 登记/run manifest
  crates/interop-media/         # 媒体形态/三时钟域/format tracker/有界帧队列 + null/file sink（T28/T29）
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
- **headless demo**：`apps/interop-demo`（`xinterop-demo [run|discovery|session|sink|media|qshare|mirror|dlna] [--json]`）
  把上述各层跑成真实状态输出：注册表不合并 identity、SETUP 503/401 与 131072 B 分配时机、sink 统计与
  显式 resample、XMD1 往返与负向、UKEY2 握手与分片等价；输出恒带 `evidence_level=simulated` + blocked +
  待用户手动清单（见 [evidence/2026-09-15-t5-demo-cli](evidence/2026-09-15-t5-demo-cli/demo-report.txt)）。
- **验证**（阶段 3 收尾时）：`cargo test --manifest-path impl/Cargo.toml --workspace` **113 tests 全绿**；
  clippy 0 warnings；`python3 -m unittest discover -s tools` **39 tests OK**（阶段 4 首批后为
  **163 tests / lab 46 tests**，见下）。每 task 的 run manifest 见
  [evidence/index.json](evidence/index.json)。

**阶段 4 首批（2026-09-15 稍后追加，三项已完成）**

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

**未开始 / 待批准**：T30（UxPlay provider 闭环，需 scope S1/S2）、T33/T34（AirPlay 音视频接收真机）、
T22（Quick Share 发送闭环，需 QR/可发现路径）、keep-alive 与 paired-key 帧、Quick Share 发现源
（待 P-F02-1/2/3 关闭与 S3 抓包批准）、T42（DLNA renderer 侧真实媒体拉取，需真电视矩阵 P-M07-1）、
T37（Wi-Fi Display/Miracast 纯状态机与媒体协商，未开始）、xross-dev 侧 bridge 窗口（S4）、
provider 放行（S5）；native 窗口 sink 与 Tauri demo 壳留桌面会话。
阶段 3 的 goal prompt 见 [docs/goal-phase-3.md](docs/goal-phase-3.md)，执行结果见
[evidence/2026-09-15-phase-3-final-report.md](evidence/2026-09-15-phase-3-final-report.md)；阶段 2 见
[evidence/2026-09-15-phase-2-final-report.md](evidence/2026-09-15-phase-2-final-report.md)。

Agent 工作规则、禁止事项与汇报格式见 [AGENTS.md](AGENTS.md)；基础阶段执行提示词见
[docs/goal-phase-1.md](docs/goal-phase-1.md)（已执行完毕，期末报告见
[evidence/2026-09-15-phase-1-final-report.md](evidence/2026-09-15-phase-1-final-report.md)）。
