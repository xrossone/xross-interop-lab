# 阶段 3 期末报告（投屏 + 近场传送协议核心，headless 优先）

**日期**：2026-09-15　**范围**：`docs/goal-phase-3.md` 的 T1–T6　**格式**：docs/12 六项汇报格式
**结论**：阶段 3 完成定义 4 项全部满足；阻塞项与「待用户手动测试」清单见 §5。

---

## 1. 当前 task 与 commit，改动文件列表

各 task 单独提交，全部已 push origin（github.com/xrossone/xross-interop-lab）：

| task | commit | 规模 | 主要内容 |
|---|---|---|---|
| T28 媒体形态/时钟/frame | `88c6873` | 14 files, +1078 | 新 crate `interop-media`（descriptor/clock/stream）+ `interop-ipc` 的 XMD1 36 字节 frame |
| T29 null/file sink | `ff65784` | 5 files, +621 | `sinks.rs`（null/file 统计与落盘、显式 resample、native 明确不可用）+ 9 tests |
| T31 AirPlay gate | `1abc3f0` | 7 files, +588 | 字段级事实表（17 行）、能力分声明、FairPlay vendor-gated、语料计划、gate 测试 |
| T32 AirPlay 控制链 | `b56c591` | 12 files, +1094 | 新 crate `proto-airplay`（rtsp/capability/discovery/keying/session）+ 4 验收 case |
| T5 headless demo | `f296a2d` | 13 files, +2569 | 新 app `interop-demo`（4 段 + 诚实标注 + blocked/manual 清单）+ 7 tests |
| T19/T20 Quick Share | `b27827a` | 18 files, +3135/−13 | 新 crate `proto-quickshare` + F-01..F-24 字段表 + 两处冲突具名裁决 + 7 gate tests + 9 验收 tests |
| T5+ demo 增 Quick Share 段 | `ec6a669` | 9 files, +613/−11 | demo 增加 `qshare` 段（真实握手/分片等价/gate 语义）+ D-08 |

合计（`11f1379..HEAD`）：**58 files changed, +9687/−13**；`impl/` 增至 **10 crate + 1 adapter + 3 app + 1 worker**
（新增 `interop-media`、`proto-airplay`、`proto-quickshare`、`apps/interop-demo`）。

## 2. 实际验证命令及 exit code

| 命令 | exit | 结果 |
|---|---|---|
| `cargo test --manifest-path impl/Cargo.toml --workspace` | 0 | **113 tests 全绿**（阶段 2 的 69 → 113：T28 11、T29 9、T32 6、T5 demo 8、T19/T20 15，其余为单测） |
| `cargo clippy --manifest-path impl/Cargo.toml --workspace --all-targets` | 0 | 0 warnings |
| `python3 -m unittest discover -s tools -p 'test_*.py'` | 0 | **39 tests OK**（28 → 39：airplay gate 4 + quickshare gate 7） |
| `cargo run -p interop-demo -- run --json` | 0 | 真实状态输出（`ok=true`，5 个 section）；文本与 JSON 存 `evidence/2026-09-15-t5-demo-cli/` |
| `cargo test -p proto-quickshare` | 0 | 15 tests（真实 P-256 ECDH / SHA-512 commitment / HKDF-SHA256；分片等价 byte-identical） |
| `cargo test -p proto-airplay` | 0 | 6 tests（T32-01..04 + TXT RDATA 编码） |
| （说明） | — | 每个 task 的 red→green 过程（含真实失败：RtspStatus 位置、p256 CtOption、plan 包名解析、gate 测试误报）见各 run manifest 的 `commands` 数组 |

**pass**：全部上表；**fail**：无遗留；**blocked**：§5；**not-run**：真机互通（AirPlay ↔ iPhone、
Quick Share ↔ Android）、抓包、可见性矩阵、native 窗口、Tauri GUI。

## 3. 使用的 source/fixture IDs 与新引入依赖

- **本阶段读取的第三方源码（只读，不构建）**：
  - AirPlay 线：`specs-reviewed/m01` 字段表引用的 `R01`（UxPlay，compat-oracle）、`R02`（shairplay-rust）、
    `R05`、`R10`（restricted，仅事实核对）；`provenance/airplay-inputs.json` 9 条，全部 `allowed_in_crate=false` 者不进入实现。
  - Quick Share 线：`R15`（NearDrop，Unlicense，facts-source）、`R17`（nearby，Apache-2.0）、
    `R18`（ukey2，Apache-2.0，规范+proto 为主）、`R16`（Bada，restricted，仅核对）、
    `R19`（rquickshare，GPL，**未使用**）；登记于 `provenance/quickshare-inputs.json`。
  - 本阶段**未执行**任何第三方构建脚本、未运行第三方二进制、未读取任何凭据。
- **Fixture**：全部自制。XMD1/媒体帧为测试内构造；AirPlay 报文按字段表造；
  Quick Share 的 UKEY2 消息由 `crates/proto-quickshare` 的编码器按 R18 字段号生成，
  负向用 `tamper_client_finished_public_key` / `rebuild_client_init`（同一个编码器，不手写字节）。
  确定性密钥标量仅作测试注入（`with_fixed_entropy`/`from_scalar_bytes`），生产路径走系统熵。
- **新引入依赖**：`p256 0.13`、`hkdf 0.12`、`getrandom 0.2`（Quick Share 握手；均为成熟 RustCrypto/系统熵原语，
  本地 registry 已有，无 git 依赖）；`sha2 0.10` 沿用。**未引入** protobuf 代码生成、网络栈、tokio、GPL/LGPL 库。
- **两处规范/实现冲突（本阶段显式固化）**：F-12（cipher 选择规则）、F-15（HKDF 哈希）；
  见 `specs-reviewed/f02-quickshare-lan.md` 与 `crates/proto-quickshare/tests/quickshare_crypto.rs` 的断言。

## 4. 功能达到的证据层级

- **build-verified**：T28（媒体形态/时钟/帧）、T29（null/file sink）、T32（AirPlay 控制链状态机与资源记账）、
  T20（UKEY2 握手/framing/payload gate）全部验收 case —— 真实密码原语、真实字节流、真实文件落盘。
- **source-reviewed**：T31（AirPlay 规范/来源/语料 gate）、T19（Quick Share 字段表与来源 gate，
  P-F02-2 关闭到 source-reviewed）。
- **simulated**：T5 demo 的全部输出（真实 crate 执行 + 自制 wire，未连任何设备）。
- **device-verified**：**本阶段没有新增**。AirPlay 真机 POC 的记录仍是阶段 1 的
  `docs/research/airplay-research.md`（transient 链路，画质/音频/PIN 受阻）。
- **明确未声称**：AirPlay 音视频接收能力（能力广告为空即证据）、Quick Share 发现与传输闭环、
  任何 provider 可用性、与任何真实设备的互通。T32 的能力门禁让"广告 HEVC 却无 decoder"成为**测试失败**，
  T19 的 gate 测试让"实现里出现未固化发现代码"成为**测试失败**。

## 5. 尚未确认的 wire/平台条件与下一次 probe（blocked 清单）

| 项 | 障碍 | 下一次 probe | 重评条件 |
|---|---|---|---|
| AirPlay 音视频接收（T33/T34） | 无 decoder/sink 实现；FairPlay 材料永久 vendor-gated | S1 批准后接 UxPlay 作外部引擎（worker，class B） | scope **S1** + 一次真机镜像成功 |
| AirPlay PIN 配对一致性 | `/pair-setup-pin` 的 PIN 语义未固化（P-M01-3） | 与 iPhone 实测 PIN 码 | S2 真机 |
| AirPlay `features` TXT 位串 | 真实位值未固化（字段表只固化"位决定协商"） | 抓取真实接收端广播对照 | S3 抓包 |
| Quick Share 发现（LAN/QR） | P-F02-1/3 未关闭；发现载体只有逆向文档 | 用户批准网段抓包 + 两台 Android 可见性矩阵 | 抓包与源码一致 |
| Quick Share 传输闭环（T21/T22） | 传输加密与 payload 层未实现；序号/重放窗口的真机行为未知 | 先 T21（SecureMessage AES-256-CBC+HMAC，字段已固化 F-19/F-20） | 与 Android 完成一次文件传输 |
| Quick Share 确认码一致性 | 现为 R15 兼容启发式；F-12/F-15 冲突未裁决 | 与 stock Android 成功握手比对 4 位码 | PIN 一致 |
| xross-dev `client.v1` 落地程度 | 基线锚 `099b6c72`，主仓 HEAD 已前移 `9d44800c` | 首次对接前复核映射 | S4 |
| provider 放行 | `production_approved` 全 false | 逐 provider 批准 → 组合审查 → release manifest | S5 |

**待用户手动测试清单（本阶段产出）**

1. **真机投屏矩阵**（iPhone/iPad → 本机：发现/连接/视频/音频/旋转/重连/10-30-60 分钟）——需 S1/S2；
   `xinterop-demo qshare|session` 可在连接前后给出本机侧状态快照。
2. **Quick Share 抓包（P-F02-1）与两台 Android 可见性矩阵（P-F02-3）**——需 S3；pcap 不入库，脱敏摘录进 evidence。
3. **与 stock Android 完成一次 Quick Share 握手并对 4 位确认码**——裁决 F-12/F-15 的唯一手段。
4. **Tauri demo 壳的 GUI 交互验证**（窗口/渲染/点击）——当前 demo 是 headless CLI，GUI 未做。
5. **native 视频窗口 sink**（桌面会话内验证；无 GUI 自动化，且不以屏幕录制假装 raw output）。

## 6. 主仓回归/接口影响与 contract owner 裁决项

- **xross-dev 未被修改**（全程只读）。基线仍锚 `099b6c72`；`tools/test_xross_baseline.py` 持续提示 HEAD 前移，
  首次实现对接前需复核映射。
- **接口影响**：无。所有产物在 lab 内；interop 契约仍是 `interop.api/0.1`（`interop-ipc` 的 XMD1 是数据面
  本地帧，未触碰 `xross.*` 命名空间）。
- **需用户裁决**：§5 的 S1/S3/S5（以及 S2/S4）范围请求；F-12/F-15 若真机结果与我们的实现侧裁决相反，
  需要一次 ADR 记录裁决与迁移（当前实现已把两个变体都留在代码里，迁移是改一个默认分支）。
- **ADR 状态**：ADR-003（执行与信任边界）仍是唯一 accepted 的 ADR；本阶段新增的协议实现都遵守
  其"协议执行在隔离 worker、产品侧才有 bridge"的边界（本轮实现均在 lab 内库层，未起进程/未联网）。

## 下一阶段建议（阶段 4 首批）

1. **T21 Quick Share 传输闭环**（SecureMessage AES-256-CBC + HMAC-SHA256、D2D 序号、introduction/response、
   payload 分块）——字段已固化（F-19/F-20/F-22），**不需要新权限**即可 headless 做完。
2. **T33 AirPlay 参考接收路径**（把已实现的 RTSP/session 与 `interop-media` 的 sink 接起来，
   用自制 fixture 驱动解码前的数据面）——仍不需真机；真机验收挂 S1/S2。
3. **T30 UxPlay 外部引擎接入准备**（worker 侧封装、二进制 hash pin、能力对照脚本）——待 S1 批准后执行。
4. 有环境时补：Linux/Windows 的 probe 字段、`client.v1` 落地复核、native sink 的桌面会话验证。

**不是完成报告**：以上均为待办；阶段 3 的"完成"只指 §2 的测试面、§4 的证据层级与 §5 的清单落库。


---

## 补充（2026-09-15 稍后）：T21 Quick Share 传输闭环（headless 部分）已落地

**commit** `592acc9`（实现 + 证据）与 `8a7897e`（demo `qshare` 段接入传输链）。

- **实现**：`crates/proto-quickshare` 新增 `secure_message.rs`（D2D 密钥链 F-19、SecureMessage 信封
  AES-256-CBC + HMAC-SHA256、`DeviceToDeviceMessage` 严格 +1 序号）、`payload.rs`
  （OfflineFrame/V1Frame/PayloadTransferFrame 编解码 + 常数内存组装器）、`receive.rs`
  （introduction → 报价 → 用户裁决 → 流式落盘 → 原子发布；payloadID↔entryID 绑定、预算、
  CANCEL 清理；文件名穿越由 `interop-file` 的 FILE-05 规则拒绝）。
- **验收**：T21-01..04 全部有可运行测试（两个 payloadID 混入 → 拒绝且不发布；用户拒绝 → REJECT 且无文件；
  10 GiB 合成流常数内存；伪造文件名穿越 → 拒绝），另加 SecureMessage 正负向（往返/篡改/重放/跳号）。
- **字段表新增**：F-25..F-29（来源 R18 `securemessage.proto`/`securegcm.proto`/
  `device_to_device_messages.proto`、R17 `offline_wire_formats.proto`、R15 汇集自 Chromium 的
  `wire_format.proto`——许可边界已登记在 `provenance/quickshare-inputs.json`）。
- **测试面**：impl 113 → **126**（proto-quickshare 28）；clippy 0；lab 39 OK。
- **仍未做**：Quick Share 发现（P-F02-1/3 未关闭）、keep-alive 与 paired-key 帧、与 Android 的真机传输。
  因此 §5 的「传输闭环」一行从"未实现"改为"headless 已实现、真机未验"。


## 补充 2（2026-09-15）：T33 AirPlay 镜像接收路径（headless 部分）已落地

**commit** `1831556`（实现 + demo `mirror` 段 + 证据）。

- **实现**：`proto-airplay` 新增 `mirror.rs`（格式协商 → encoded access unit 直通 sink；缓冲超预算进入
  **关键帧恢复**；未配置就送帧、缺 discontinuity 的 format 变化、超大尺寸/超大 payload 全部拒绝）、
  `audio.rs`（只收 PCM 块；44.1k→48k 显式重采样、时长不变；ALAC 等编码 → `unsupported-feature`）、
  `timing.rs`（**按连接**发放 clock generation + 漂移观测，无 PTS 不伪造时间）。
- **验收**：T33-01（横竖屏往返 format 更新/画面恢复/越界拒绝）、T33-02（30 分钟合成音画：drift max ≤2ms、
  0 丢帧、缓冲高水位有界且后半程不增长）、T33-03（20 次断开重连：20 个 generation、keying 调用 20 次、
  每轮资源归零）、T33-04（HEVC 不广告；能力广告仍为空）全部有可运行测试。
- **发现的接口缺口（已修）**：`interop-media` 的 sink 侧用 `discontinuity=false` 复判 format 变化，
  导致协议层已校验通过的旋转在 sink 里被拒；现在 sink 收到 `on_config` 即视为被要求重置解码器，
  判定归协议层（T29 既有断言不回归）。
- **测试面**：impl 126 → **141**；clippy 0；lab 39 OK。
- **边界**：本 task **不含 decoder**，encoded AU/PCM 只被路由到 null sink 统计 —— 不声称"能看到画面"。
  T33-02 是**合成时间轴**（非 30 分钟墙钟），内存结论以缓冲高水位为代理；真机镜像、RSS 趋势与画面呈现
  仍在 §5 的待用户手动清单。


---

## 补充 3（2026-09-15）：T41 DLNA/UPnP AV headless 切片已落地

**commit** `6cbcc12`（gate + `proto-upnp` 实现 + 证据）与 `c09a57f`（demo `dlna` 段 + D-10 + 证据刷新）。

- **先关门禁**：重写 `specs-reviewed/m07-dlna-upnp-av.md` 为 F-01..F-14 字段级事实表（每行带来源），
  新增 `provenance/m07-inputs.json`（R46 pupnp BSD-3、R49 rygel LGPL-2.1 均按 facts-only 登记；
  R47/R48 排除）与 `evidence/dlna/corpus-plan.json`（dl-001..dl-012），纪律测试
  `tools/test_dlna_gate.py`（7 例：逐行来源、blocked 行不得进实现、实现里不得出现品牌名或
  屏幕镜像宣称；对"不提供镜像替代"这类**如实否认**不误报）。F-13/F-14（真实电视矩阵）标 blocked（P-M07-1）。
- **实现**：`crates/proto-upnp` 五个模块——`xml`（加固子集：拒绝 DTD/ENTITY/CDATA，深度/体积/元素/
  属性预算**明写为本仓策略值**，不冒充协议常量）、`ssdp`（M-SEARCH/NOTIFY 校验、USN 去重、max-age）、
  `soap`（动作与参数白名单、`SOAPACTION` 绑定、InstanceID 只认 0、metadata 预算、Fault 解析）、
  `dmc`（抓取策略：仅 http，拒 loopback/链路本地/云元数据/userinfo/fragment → `permission-required`；
  注册表按对方**声明的** `protocolInfo` 判 push 能力，未声明 codec/协议一律拒绝；URL lease 可撤销 + 过期清扫）、
  `dms`（受限根 + 显式注册子项授权、701/720/402、分页上限 256）。
- **明确不做**：真实电视矩阵与 GENA 事件投递、媒体字节传输、任何屏幕镜像能力或替代——capability
  分声明里"屏幕镜像"恒为 not-implemented，demo 的 `blocked` 清单也逐条列出。
- **验收**：T41-01（抓取策略 + 注册表拒绝）、T41-02（codec/protocolInfo 能力判定，不因对方是投屏设备
  就假装能收屏）、T41-03（DMS 授权与 701/402）、T41-04（lease 撤销/清扫/取消）全部有可运行测试，
  demo D-10 端到端复述同一批结论。
- **测试面**：impl 141 → **163**（proto-upnp 22 + demo 10）；clippy 0；lab 39 → **46**（dlna gate 7）。
- **边界**：无网络 I/O（SSDP/HTTP 报文只在内存里编解码）、无真机、无凭据；真电视与 renderer 侧
  媒体拉取（T42）仍需 P-M07-1 关闭。


---

## 补充 4（2026-09-15）：T37 WFD/Miracast 接收侧控制面与媒体协商（headless 切片）已落地

**commit** `dfcb0b5`（gate）、`cf11c38`（实现 + 证据）、`1e06d70`（demo `wfd` 段 + D-11）。

- **先关门禁**：重写 `specs-reviewed/m05-miracast-wfd.md` 为 F-01..F-32 字段级事实表（每行带来源
  commit + 文件 + 行），覆盖控制面消息与**方向**（M1 与 M2 在线上同形、只有方向不同）、媒体协商描述符
  字段序与位布局、子元素、RTP 封装；blocked 行 = F-24（WFD IE 容器 OUI/OUI type）、F-25（P2P 组形成与
  平台 API）、F-30（真实设备矩阵），另有 F-31（厂商参数拼写分歧）与 F-32（profile/level 按位图解释的裁决）。
  新增 `provenance/m05-inputs.json`（R32/R33/R34/R37 facts-only 放行并写明各自许可边界；R35 固件/反编译来源、
  R36 许可未审计 → 不进实现）与 `evidence/wfd/corpus-plan.json`（wfd-001..018，**规则禁止任何 OUI 猜测值**）。
- **实现** `crates/proto-wfd`：`messages`（方法集与 M 编号/方向、CSeq 递增、Session 取分号前子串、
  M3 查询列参数名 vs M4 取值 `名字: 值` 的两种正文形状、重复/缺 Content-Length 与超限在分配前拒绝）、
  `negotiate`（三段结构与 13 字段描述符、native 拆表/索引、切片参数位布局、H.264 profile/level 位图、
  音频三元组与 mode 位、`wfd_client_rtp_ports` 校验与 F-14 的 RTCP quirk、Transport 三形状与 19000 回退、
  内容保护解析）、`ie`（子元素 0x00 九字节；**容器构造/解析一律 `unsupported-feature`**）、
  `rtp`（固定头解析与序号记账）、`session`（接收侧状态机：只固定顺序不固定状态名——F-26 说明四个来源
  各有一套状态命名、没有共同事实）。
- **验收**：T37-01..04 共 17 例。**测试逼出四处实现缺陷**，且都是来源字面量决定的：① M3 的
  `GET_PARAMETER` 正文是裸参数名而不是 `名字: 值`；② M13 正文同样是裸 `wfd_idr_request`；
  ③ keep-alive 基准错（首个间隔未满就发 M16）；④ M13/M16 应走控制 URI `rtsp://localhost/wfd1.0`
  而不是 streamid URL。若无测试，这四处会以"看起来能跑"的方式与真实对端不通。
- **诚实边界**：完整 WFD IE 不构造（OUI 未固化，拒绝理由指向 P-M05-2）、无 P2P/平台 API 调用、
  无 HDCP 握手（只解析取值并拒绝）、无 TS 解复用与解码 → `SessionStats.player_available` 恒 false，
  生产路径的能力广告是 `none`（协商必然以 `unsupported-feature` 结束，不假装能显示）。
- **测试面**：impl 163 → **181**（proto-wfd 17 + demo D-11）；clippy 0；lab 46 → **55**（wfd gate 9 例）。


---

## 补充 5（2026-09-15）：T43 Google Cast 媒体 URL sender（headless 切片）已落地

**commit** `3e76cb9`（gate）、`5365c7c`（实现 + 证据）、`f696b26`（demo `cast` 段 + D-12）。

- **先关门禁**：重写 `specs-reviewed/m08-cast-media-control.md` 为 F-01..F-25 字段级事实表（每行带来源
  commit + 文件 + 行）：CASTV2 信封字段号 1..9 与 required 声明、四个协议版本、4 字节长度前缀与
  **64 KiB 正文上限**、分块字段在参考实现里的实现缺席、namespace 字面量集合、connection/heartbeat/
  receiver/media 四组载荷键、requestId 与 10 s 超时、DeviceAuth 消息与**发送端强制认证点**、
  mDNS 服务类型与 TXT 键。blocked = P-M08-1/2/3 与 F-25（媒体字节服务）。
  新增 `provenance/m08-inputs.json`（R42 openscreen / R43 CastReceiver / R44 pychromecast facts-only；
  R45 restricted 不进实现）与 `evidence/cast/corpus-plan.json`（cast-001..017，规则禁止证书/密钥/真实 app ID）。
- **两条硬边界写进字段表、能力表与纪律测试**：① 设备认证握手不实现（缺凭据材料 → `ChannelGate`
  生产实现恒 `vendor-gated`，一条命令都发不出去）；② app ID 不申请不伪造（只允许拉起调用方显式配置的 ID）。
- **实现** `crates/proto-cast`：`castv2`（信封编解码 + required + 上限 + 分块拒绝 + 自带最小 protobuf 子集）、
  `namespaces`（四组载荷的类型字面量与 JSON 键；LOAD 不写 duration、SEEK 只认 `PLAYBACK_START`、
  `playerState`/`streamType` 白名单；未知命名空间/类型 → `unsupported-feature`）、
  `controller`（连接→拉起 app→媒体会话→收尾；requestId 单调关联与超时、心跳 10 s/10 s 与 20 s 失效、
  LAUNCH_ERROR 与 MEDIA_STATUS 分流、三条 URL 释放路径）、`discovery`（TXT 只解析不组播）。
- **验收**：T43-01..04 共 12 例。**T43-01 抓出一处真实缺陷**：`connect()` 原先先把状态改成 `connecting`
  再由命令出口检查闸门 —— 认证失败会留下一个既没发出命令、又不在 idle 的**半个会话**；现在闸门检查
  前移到任何状态变更之前（`load` 的闸门检查也提到状态检查之前，因为缺通道是更根本的前提）。
- **诚实边界**：无 TLS 传输层、无真实设备、无媒体字节服务、无分块、**不提供屏幕镜像**
  （`screen_capability()` 恒 false —— Cast 在这里只是"换一个 URL 给接收端拉"）。
- **测试面**：impl 181 → **194**（proto-cast 12 + demo D-12）；clippy 0；lab 55 → **64**（cast gate 9 例）。
