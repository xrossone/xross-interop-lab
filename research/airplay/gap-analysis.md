# AirPlay legacy-mirror 分模块 gap 分析（T31）

**日期**：2026-09-15　**角色**：T32/T33 独立实现的输入 gate（不做实现，只定事实边界与缺口）
**输入**：`specs-reviewed/m01-airplay-legacy-mirror.md`（含 T31 字段级事实表）、
`research/m01-airplay-legacy-mirror/dossier.md`、`docs/research/airplay-research.md`、
`provenance/airplay-inputs.json`、`evidence/airplay-corpus/corpus-plan.json`
**纪律**：分清"规范事实"与"第三方代码/常量"；**FairPlay 材料不进入任何一格**（永久 vendor-gated）。

## 0. 一句话结论

除 **key setup（FairPlay）** 一格被合法地永久阻塞、**音频**与**画质**两个现象未解之外，
其余模块的事实足以支撑 T32 的 headless 实现（discovery / control / session / 帧 framing / 回收）；
真机验收（A1–A10）需要用户 scope S1/S2。

## 1. 模块 gap 表

| 模块 | 已固化的事实 | 缺口 | 关闭方式 | 本仓任务 |
|---|---|---|---|---|
| discovery | `_airplay._tcp` + `_raop._tcp`；TXT 特性位决定模式；transient 广播 legacy 位 | TXT 全位图的逐位语义（只需"已实现位"即可广播） | 只广播已实现 feature（T32 实现决策），位图不追求全解 | T32 discovery.rs |
| control（RTSP） | 端口 5001；每请求新连接；`/info`、`/audioMode`、`/feedback`、SETUP(110/96)、`/fp-setup` 的存在与顺序 | 各响应的完整字段集与状态码矩阵 | 自制 fixture（`fx-airplay-*`）+ 未知字段留空标待固化 | T32 rtsp.rs |
| pairing（PIN） | 三步 binary plist 字段与尺寸（86B Step1、20B SHA1 proof、32B epk/16B authTag） | PIN 一次性语义（P-M01-4） | 真机实验（S2）+ UxPlay 代码复读 | T32 session.rs（状态机）|
| **key setup（FairPlay）** | 只有"`/fp-setup` 存在且必须在 SETUP 前完成"这一**流程事实** | 载荷、密钥派生、任何常量：**不获取、不推导** | **不可关闭**（vendor-gated）；只能改用外部合规 provider（UxPlay）或获得 Apple 授权 | **不实现**（keying seam 留空）|
| video | stream_type=110 TCP；128B 头（3 字段已固化）；AES-128-CTR；AvcC≈30B 旋转重发 | 128B 头其余字段；跨包 CTR 计数器连续性；多 slice 帧边界（P-M01-1） | 帧头字段：抓包或真机日志（S3/S2）；画质：自研管线 A/B | T33（T32 只做 control/帧 framing）|
| audio | stream_type=96；legacy ALAC AES-CBC；44100Hz；RTP UDP；`/audioMode default`；`/feedback` 2s | 无声根因（P-M01-2）；`elapsed_ms` 语义 | 真机诊断（S2） | T33 |
| timing | timebase 三域分离已在 interop-media 落地（T28）；AvcC 重发 → format_id 递增 | 抖动/漂移的实测数据 | 真机长跑（S2） | T33 + T28 复用 |
| teardown | `stop` 语义存在 | 重连资源回收的实测（P-M01-5，20 次重连） | 真机矩阵（S2） | T32-04（资源回收）+ T33 |

## 2. 与 T32 实现决策的对应

- **只广播已实现 feature**（T32-03）：discovery 的 TXT 位图由"本仓已实现能力"生成，
  绝不照抄 UxPlay 位图——否则会广告 HEVC 却没有 decoder。
- **认证前不分配视频大缓冲**（T32-02）：session 状态机把 `authorizing` 与 `streaming` 分开；
  未认证的 `SETUP` 请求返回错误且不建流。
- **每连接资源由 broker 记账**（T32-04）：连接/会话注册到计数表，断连即回收；
  100 次快速连接断开后计数必须回到 0。
- **消息 payload 来自自制 fixture**：`evidence/airplay-corpus/corpus-plan.json` 的 `fx-airplay-*`；
  真机矩阵 `matrix-airplay-devices` 在 S1/S2 前是 `blocked`。

## 3. 法律风险与替代路径（T31-02 语义）

| 风险 | 处理 |
|---|---|
| FairPlay 密钥材料来源不可说明 | 独立 permissive 产物在**密钥获取**环节标 `blocked`；实现以 keying seam 表达（trait），生产实现缺席即能力缺席 |
| 外部合规 provider（UxPlay，GPL） | 可继续研究与运行（**外部二进制**，独立发布单元）；接入需用户 S1 |
| UxPlay/shairplay-rust 表达 | 只行级事实引用（注明 commit+行）；不复制/翻译/链接（`provenance/airplay-inputs.json` 逐条登记） |
| R10 无许可文件 | 只事实核对，不复制任何文本进仓 |
| 自测 vs 真机（T31-03） | fixture 往返只证明 framing；**"可用"结论必须来自真机矩阵**（A1–A10） |

## 4. 进入 T32/T33 的合法输入（gate 结论）

**允许**：本文件、`specs-reviewed/m01`（含字段表）、`research/m01-*/dossier.md`、
`docs/research/airplay-research.md`、`evidence/airplay-corpus/corpus-plan.json` 中
`authored=self` 的 fixture（尚未生成，按计划生成）、R01/R02/R05/R08/R09/R10 的**行级事实引用**。

**禁止**：R06/PlayFair 与任何 FairPlay 常量/表；任何第三方的报文/代码/文本复制；
未固定 commit 的声明；把 fixture 自测写成真机结论。
