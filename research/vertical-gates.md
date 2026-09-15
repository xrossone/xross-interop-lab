# 首个真机 vertical 的 gate 与 scope 请求（T5）

**日期**：2026-09-15　**性质**：计划与来源审查，**不含任何实现**；本文件不产生真机证据。
**输入**：`research/m01-airplay-legacy-mirror/dossier.md`、`research/f02-quickshare-lan/dossier.md`、
`docs/research/airplay-research.md`、`decisions/source-allowlist.json`、`decisions/provider-adoption.json`、
`plans/03-casting.md`（T28–T31、T30 验收 case）、`plans/02-files.md`（T19–T21）。

## 0. 选择（本阶段代理用户决策，理由在下面）

**Vertical A = AirPlay 接收（T28 → T29 → T30 路线），先做；Vertical B = Quick Share LAN 接收（T19–T21）排后。**
理由（可复核）：
1. T30 用**外部引擎（UxPlay 二进制）**做接收，协议栈不由我们实现——不需要先固化任何未知 wire，
   也不触碰 FairPlay 材料（`vendor-gated` 永久锁定）。
2. Quick Share 的 wire 尚未固化（P-F02-1/2/3 全未关闭），且 **P-F02-1 需要抓包批准**；
   在 spec 固化前实现属计划明令禁止（"实现任务不得根据计划标题创造字节格式"）。
3. AirPlay 线的语料基础最好：真机 transient 链路已 device-verified（M01），参考实现与
   行为 oracle（UxPlay）都在手，模块 gap 已按 discovery/control/pairing/fp-setup/video/audio/
   timing/teardown 拆过（M01 dossier §probes）。
4. T13 已完成——T30 的 crash 隔离（T30-03）与"无残留监听"（T30-02）不再是未知数：
   机制已由 `T13-04`/`T4` 在真实进程上证明。

## 1. Vertical A（AirPlay 接收）gate

### 1.1 前置状态

| 前置 | 状态 | 说明 |
|---|---|---|
| T05 provider 路线（UxPlay=worker/外部二进制） | ✅ 已定 | `production_approved=false`：**放行权在用户** |
| T13 worker supervisor（hash pin/沙箱/无孤儿） | ✅ 已完成 | 本机 sandbox-exec 真实拒绝 canary（platform-sandbox） |
| T28 媒体形态、时钟与数据 frame | ⛔ 未开始 | plans/03；T30 的前置 |
| T29 null/file/native sink | ⛔ 未开始 | plans/03；T30 的前置 |
| T30 UxPlay provider 闭环 | ⛔ 未开始 | 前置 = T05+T13+T28+T29（T13 已关） |
| T31 规范/来源/兼容语料 gate | ⛔ 未开始 | 本文件给它的输入清单（§1.2–1.4） |

### 1.2 来源与法律盘点（全部 `production_approved=false`）

| ID | 来源 | 许可（intake 实测） | 在 AirPlay 线的用途 | 边界 |
|---|---|---|---|---|
| R01 | FDH2/UxPlay | GPL-3.0（依赖及文件级混合） | **行为 oracle + 外部 receiver 引擎**（T30 的引擎） | 只运行**未修改二进制**；不链接、不翻译、不复制表达；随产品分发须履行 GPL 源码 offer（独立发布单元） |
| R02 | metaneutrons/shairplay-rust | LGPL-3.0-or-later | 仅架构参考（用户实测质量不达标：无音频/视频缺陷） | 行级事实引用注明 commit；不链接、不移植 |
| R03 | FD-/RPiPlay | GPL-3.0 | 结构参考（不采用） | 同上 |
| R05 | juhovh/shairplay | LGPL/GPL 混合 | 结构参考（UxPlay 的上游） | 同上 |
| R06 | EstebanKubata/playfair | GPL 及**密码材料来源待核查** | **不使用** | FairPlay/设备认证材料 → T05-03 永久 vendor-gated；不获取、不使用、不绕过 |
| R07 | mikebrady/shairport-sync | 混合（per-file-needed） | 音频管线参考（如需要） | 逐文件审查后才可用 |
| R08/R09 | airplayreceiver / Airplay2OnWindows | MIT（根声明） | 实现结构参考 | 仅事实/结构；不复制表达 |
| R10 | openairplay/airplay-spec | **无许可文件（restricted）** | 「spec source」候选 | **只能事实引用**：不复制文本/文件进仓；引用要注明来源 URL 与 commit |
| R11 | postlund/pyatv | MIT | 控制/配对行为对照 | 事实参考 |
| R12 | philippe44/AirConnect | 多许可/上游待核 | 不采用（先审上游） | 待核 |

**第三角（规范来源）**：UxPlay=兼容 oracle、R02/R05=架构参考、**规范事实**来自 R10 与抓包
——后者需要用户批准（见 §3 S3），且 `captures/` 永不入库（脱敏摘录进 `evidence/`）。

### 1.3 模块 gap 与 probe 状态（来自 M01 dossier，未关闭项）

| 模块 | 已知 | 未关闭 |
|---|---|---|
| discovery | mDNS broadcast 形状、与系统 AirPlay Receiver 同名冲突 | 冲突时的用户可见行为（dossier 已记为 probe 项） |
| pairing | transient 直通已通；PIN 三步（binary plist + legacy SRP）**未实现** | **P-M01-3**（计划中）：`/pair-setup-pin` 独立实现 |
| fp-setup | `/fp-setup` 握手的通用路径（非 FairPlay 授权材料） | FairPlay 授权材料的**任何**使用都是禁止项（T31-02 语义） |
| video | SETUP(110) + TCP 帧、AvcC 重发（旋转→format_id 递增） | **P-M01-1**（可选诊断）：画质归因——**参考实现的问题不承诺修复** |
| audio | SETUP(96) + RTP | **P-M01-2**（可选诊断）：参考实现链路断点 |
| timing | clock domain/timebase 已在 contract 的 `Track` 表达 | 真机抖动/漂移测量（首个 vertical 的验收项） |
| teardown | stop 语义 | 重连矩阵 **P-M01-5**（10/30/60 分钟 + 20 次重连） |

**纪律（T31-01）**：视频与音频的能力**分开声明**——"视频能解密但音频未验证"时
`Capability{role, media_forms}` 必须如实拆开，不显示成一个"支持 AirPlay"。

**纪律（T31-02）**：FairPlay/密码材料来源说不清时 → 独立 permissive 产物标 `blocked`，
但**外部合规 provider（UxPlay 这类外部进程）仍可继续研究**。不写假 stub 让测试变绿。

**纪律（T31-03）**：自测（selftest 对打）**不替代** iPhone 真实互通；语料/矩阵必须有真机条目。

### 1.4 首个 vertical 的验收清单（真机项需用户在场）

| # | 项 | 证据方式 |
|---|---|---|
| A1 | iPhone 控制中心能看到具名接收端（名称/型号可见） | 屏幕记录 + 设备/OS 版本具名 |
| A2 | 连接建立（transient 直通；PIN 路径按 P-M01-3 结论单独标注） | 会话日志（`wire_phase` 投影） |
| A3 | 视频呈现（首帧证据；分辨率/旋转变化 → format_id 递增） | 落盘快照 + 事件序列 |
| A4 | 音频呈现（与视频能力分开声明） | 音频存在性检查（计数/回放记录） |
| A5 | 旋转 | 事件序列中的 format change |
| A6 | 断开/重连（含 20 次重连） | 次数与失败率 |
| A7 | 稳定性 10/30/60 分钟 | 长跑记录（掉帧/崩溃/内存趋势） |
| A8 | UxPlay A/B 对照（行为 oracle） | 同一 iPhone 同场景对照记录 |
| A9 | 畸形流量不带走 host | worker 隔离证据：注入畸形帧 → worker 退出/被回收，host 与其他会话不受影响（机制已由 T13/T4 证明） |
| A10 | **不上 fabric** | 全程无对 peer 的媒体发布路径；`network_listen` 默认拒绝（T4 已证） |

## 2. Vertical B（Quick Share LAN 接收）gate

**阻塞于 wire 未固化**（plans 纪律：probe 关闭前不创造字节格式）：

| probe | 问题 | 方法 | 需要用户 |
|---|---|---|---|
| P-F02-1 | stock Android Quick Share 在 LAN 用什么发现？ | **批准 lab 网段抓包**（脱敏）+ 对照 r15/r17 发现模块 | ✅ 抓包批准 |
| P-F02-2 | UKEY2 握手 → 会话密钥 → 传输加密的绑定链？ | source-review（R18 spec/docs + R19 wire 结构 + R15 接收路径） | 不需要 |
| P-F02-3 | 可见性模式（所有人/联系人/隐藏）矩阵 | 两台 Android 不同设置 × 本节点 | ✅ 真机 |

关闭 P-F02-1/2 后才动手写 `specs-reviewed/f02`，之后才谈 T19–T21。
抓包：`captures/` 永不入库；脱敏摘录进 `evidence/`（SEC-08）。

## 3. 需要用户新开 scope 的行（**这是本文件的重点**）

| # | scope 行 | 为什么需要你 | 批准后的第一步 |
|---|---|---|---|
| **S1** | 允许把**用户已构建的 UxPlay 二进制**接入为外部 receiver 引擎（worker 进程，不改编、不链接、不复制源码） | GPL 产物的运行与分发边界（T05 的组合审查未闭合） | 记录二进制来源与 hash，跑 T30-01（具名设备 + 首帧证据） |
| **S2** | 允许**真机互联测试**（用户自有 iPhone/iPad + 本机），含 10/30/60 分钟长跑与重连矩阵 | 需要你的设备和在场时间；证据要写设备/OS 版本 | 执行 §1.4 的 A1–A8 |
| **S3** | 允许 **lab 网段抓包 + 脱敏**（Quick Share 线 P-F02-1；AirPlay 线的兼容语料同理） | 抓包涉及你的网络与设备流量 | 抓包一次 LAN 发现过程 → 脱敏 → `evidence/` 记 hash |
| **S4** | xross-dev 侧 **interop bridge 落地窗口**（ADR-003 的产品集成侧） | 主仓当前焦点是付费后端，排期由你定 | 与主仓排期对齐后开 bridge 任务 |
| **S5** | **provider 放行**（`production_approved` 仍全 false） | T05 保留给用户的放行权；首个 vertical 的结论先进 lab 证据，不发布 | 逐 provider 走"批准 → 组合审查 → release manifest 放行" |

**S1 之外的 AirPlay 材料不需要你批准**：R10 无许可 → 只事实引用；R06/playfair 与 FairPlay
材料**永久不碰**（T05-03）；R02/R05/R08/R09 只做参考。

## 4. 阶段 3 的任务顺序建议

1. **T28**（媒体形态、时钟、数据 frame）→ **T29**（null/file/native sink）——T30 的硬前置。
2. **T30**（UxPlay provider 闭环 + §1.4 验收 A1–A10）——**在 S1 批准后**；S2 决定 A6–A8 的完成度。
3. **T31**（AirPlay 规范/来源/兼容语料 gate 的正规化：`research/airplay/gap-analysis.md`、
   `specs-reviewed/airplay-legacy-mirror.md`、`provenance/airplay-inputs.json`、`evidence/airplay-corpus/`）
   ——可以并行在 T28/T29 期间推进（纯研究，不需要新权限）。
4. **T19–T21**（Quick Share）——待 P-F02-1/2 关闭（S3）。

## 5. 明确不做（本阶段）

- 不实现 AirPlay/Miracast/Cast/QuickShare 真实协议栈（阶段 3 亦仅"外部引擎接入"起步）；
- 不获取/不使用/不绕过任何 FairPlay 或设备认证材料；
- 不执行第三方构建脚本；不把 `captures/`、第三方源码树或受版权文本入库；
- 不把 simulated/selftest 结果写成 device-verified。
