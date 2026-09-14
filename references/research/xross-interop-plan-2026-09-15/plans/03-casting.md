# 媒体、AirPlay、Miracast与Cast Implementation Plan

> **For agentic workers:** Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` when available. 无这些工具时按相同的逐任务测试/评审/提交过程执行；禁止虚构子agent或测试证据。

**Goal:** 在明确能力/来源/平台边界内交付本分册的可测试provider与契约。

**Architecture:** Headless-first；独立协议状态机通过统一HostPorts/Media/Offer契约集成。Rust核心、系统API与合法worker共存；生产许可和安全边界独立审核。

**Tech Stack:** Rust stable（本地首次锁版本）、Tokio、serde、scoped IPC、平台原生媒体；可选Tauri 2/Svelte。第三方库按来源gate选择，不能自动安装浮动依赖。

**Spec:** [功能规格](../docs/01-functional-spec.md)、[架构](../docs/04-architecture-and-xross-integration.md)、[契约](../docs/05-contracts-ipc-cli.md)、[研究与许可](../docs/06-research-provenance-and-licensing.md)。

## Global Constraints

- Headless-first；UI不是任务/权限权威。
- 所有外部profile默认关闭；发送与接收分别验收。
- 保留Xross唯一身份/Fabric/现有内容权威。
- 源码/SDK/依赖与fixture必须有可复查来源；私有仓库不豁免义务。
- 不支持的系统权限、编码或认证明确返回不可用；不假造stub成功。
- 计划中的命令和crate是待实现接口，不是本包已存在的协议软件。
- `lab/`表示`xross-interop-lab/`根，其余相对代码路径属于`xross-interop/`；集成主仓路径以T03真实映射为准。
- `<profile>`/`<run-id>`/`<directory>`是实际运行参数，必须在证据中解析成真实值，不是让AI自行猜协议。

## 执行方式

先完成task的依赖；每次只取一个task相关spec/source集合。下面每个JSON块是**具体验收场景与断言需求**，不是已运行测试，也不是用文字替代真实测试。实现者应在列出的test文件内把它变成可运行单元/集成/设备测试。对未知私有协议，先完成明确probe和审查后的wire spec，再编写消息codec，不能根据计划标题创造字节格式。

## T28 · 实现媒体形态、时钟与数据frame

**前置：** T06, T09, T10, T14。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-02, MEDIA-03, MEDIA-07, SEC-03。

**Files：**
- `crates/interop-media/src/descriptor.rs`
- `crates/interop-media/src/clock.rs`
- `crates/interop-ipc/src/media_frame.rs`
- `tests/contract/media.rs`

**Consumes：** MediaDescriptor与36byte XMD1数据frame定义。

**Produces：** encoded/PCM/native/resource四类typed source与有界binary通道。

**实现决策：** 实现frame尺寸/序号/format变更验证和有界队列；区分远端timebase、本地monotonic、wall time。没有PTS/DTS明确标志。native presentation只发opaque owner reference，不能序列化裸pointer。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T28-01",
    "scenario": "声明payload大于16MiB",
    "expected": "分配前resource-limit"
  },
  {
    "id": "T28-02",
    "scenario": "timebase denominator=0",
    "expected": "invalid-frame"
  },
  {
    "id": "T28-03",
    "scenario": "format_id变化无decoder reset",
    "expected": "测试必须失败"
  },
  {
    "id": "T28-04",
    "scenario": "native-only source请求export",
    "expected": "unsupported-feature"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p interop-media; cargo test -p interop-ipc media_frame`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S05](../docs/03-reference-projects.md), [S26](../docs/03-reference-projects.md)。

## T29 · 实现null/file/native播放器sink

**前置：** T05, T13, T28。**按所选profile才需要：** 无。

**对应需求：** MEDIA-04, MEDIA-05, MEDIA-07, APP-02。

**Files：**
- `crates/interop-media/src/sinks.rs`
- `workers/presentation/`
- `tests/e2e/media_sinks.rs`

**Consumes：** 自制H264/PCM fixture、已审查GStreamer/native后端。

**Produces：** 无GUI可接收，图形会话可受控呈现的player。

**实现决策：** 先null sink统计frame/timebase，再file sink保存自有测试内容，最后native window。固定允许codec和插件，声卡/显示缺失不影响null/file。播放和文件输出共用同一source不重复解码不必然保证，按实际graph决定。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T29-01",
    "scenario": "无DISPLAY/无音频设备运行null",
    "expected": "成功统计帧且不初始化UI"
  },
  {
    "id": "T29-02",
    "scenario": "44.1k PCM输出48k设备",
    "expected": "显式resample节点，不变速"
  },
  {
    "id": "T29-03",
    "scenario": "损坏或巨大dimensions",
    "expected": "受控失败，不拖垮supervisor"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p interop-media sinks; 在无UI容器和桌面分别执行自制媒体fixture`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R50](../docs/03-reference-projects.md), [R52](../docs/03-reference-projects.md), [R53](../docs/03-reference-projects.md), [R60](../docs/03-reference-projects.md), [S20](../docs/03-reference-projects.md)。

## T30 · UxPlay外部receiver provider闭环

**前置：** T05, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-02, MEDIA-04, SEC-04, APP-05。

**Files：**
- `lab/pocs/uxplay-provider/`
- `workers/uxplay/`
- `tests/devices/airplay-uxplay.md`

**Consumes：** 锁定UxPlay、自有内容iPhone、独立player和worker contract。

**Produces：** 可启动/停止、可查询、可呈现的外部AirPlay provider。

**实现决策：** 先用库存binary正常窗口重现用户成功，再选择经文档验证的RTP/callback路径输出到sink。不臆造UxPlay现成JSON IPC；需要修改时修改在GPL-labelled wrapper产物中。生命周期和PIN事件不靠无限正则猜日志，必要时做明确接口patch。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T30-01",
    "scenario": "iPhone控制中心镜像",
    "expected": "有具名设备版本和首帧证据"
  },
  {
    "id": "T30-02",
    "scenario": "拒绝PIN/停止",
    "expected": "无持续媒体和残留监听"
  },
  {
    "id": "T30-03",
    "scenario": "worker crash",
    "expected": "Xross/LocalSend不退出"
  },
  {
    "id": "T30-04",
    "scenario": "RTP输出无法实现目标",
    "expected": "保留native-window形态，raw能力不标成功"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `锁定UxPlay在隔离环境构建/运行；执行tests/devices/airplay-uxplay.md步骤`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**Gate / 阻塞处理：** GPL wrapper与组合分发方案需T05批准；本阶段不要求独立Rust协议已完成。

**参考：** [R01](../docs/03-reference-projects.md)。

## T31 · AirPlay独立实现前的规范/来源/兼容语料gate

**前置：** T04, T05, T14, T30。**按所选profile才需要：** 无。

**对应需求：** SEC-06, SEC-10, MEDIA-05, APP-08。

**Files：**
- `lab/research/airplay/gap-analysis.md`
- `lab/specs-reviewed/airplay-legacy-mirror.md`
- `lab/provenance/airplay-inputs.json`
- `lab/evidence/airplay-corpus/`

**Consumes：** 至少一个真机接收基线、多个参考实现和合法资料。

**Produces：** 分模块gap、独立fixture、可用密码兼容路线和明确法律风险。

**实现决策：** 按discovery/control/pairing/key setup/video/audio/timing/teardown拆；分清规范事实与第三方代码/常量。密码材料或受保护表达没有可用许可就不能硬写假stub通过测试；记录合法library/worker替代路径。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T31-01",
    "scenario": "视频能解密但音频未验证",
    "expected": "两个能力分开"
  },
  {
    "id": "T31-02",
    "scenario": "FairPlay材料来源不能说明",
    "expected": "独立permissive产物blocked，外部合规provider仍可研究"
  },
  {
    "id": "T31-03",
    "scenario": "只比较selftest",
    "expected": "不替代iPhone真实互通"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `按dossier和provenance模板进行审查；所有必要wire字段标注可复查来源`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R01](../docs/03-reference-projects.md), [R02](../docs/03-reference-projects.md), [R03](../docs/03-reference-projects.md), [R05](../docs/03-reference-projects.md), [R06](../docs/03-reference-projects.md), [R08](../docs/03-reference-projects.md), [R09](../docs/03-reference-projects.md), [R10](../docs/03-reference-projects.md)。

## T32 · AirPlay Rust discovery/control/session核心

**前置：** T06, T07, T11, T14, T31。**按所选profile才需要：** 无。

**对应需求：** CORE-02, CORE-06, SEC-01, SEC-03, SEC-06。

**Files：**
- `crates/proto-airplay/src/discovery.rs`
- `crates/proto-airplay/src/rtsp.rs`
- `crates/proto-airplay/src/session.rs`
- `tests/contract/airplay_control.rs`

**Consumes：** 获批AirPlay事实规范、许可兼容crypto provider。

**Produces：** 可测试的legacy receiver控制链、准确广告能力。

**实现决策：** 先发现和info/控制帧，严格schema/framing；再配对/密钥session；只广播已实现feature。每连接资源和端口由broker管理，认证前不得分配视频大缓冲。所有message payload来自独立/合法fixture。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T32-01",
    "scenario": "缺失/重复body长度",
    "expected": "明确拒绝"
  },
  {
    "id": "T32-02",
    "scenario": "未完成所需会话认证请求media",
    "expected": "不开始流"
  },
  {
    "id": "T32-03",
    "scenario": "广告HEVC但没有decoder/接收实现",
    "expected": "构建/能力测试失败"
  },
  {
    "id": "T32-04",
    "scenario": "快速连接断开100次",
    "expected": "session/port资源回收"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-airplay control`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**Gate / 阻塞处理：** T31未批准的资料不能进入这个crate；允许保持外部provider而非强行重写。

**参考：** [R10](../docs/03-reference-projects.md)。

## T33 · AirPlay legacy镜像音视频接收

**前置：** T28, T29, T32。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-03, MEDIA-05, MEDIA-07, MEDIA-10, APP-08。

**Files：**
- `crates/proto-airplay/src/mirror.rs`
- `crates/proto-airplay/src/audio.rs`
- `crates/proto-airplay/src/timing.rs`
- `tests/devices/airplay-rust.md`

**Consumes：** 已授权控制session与bounded media sink。

**Produces：** iPhone/iPad/Mac→Rust receiver的分轨输出。

**实现决策：** 保持codec config与frames、音频clock、format变化和discontinuity；第一目标H264+一个真机验证音频profile。以UxPlay行为作对照不是逐函数翻译。buffer超过预算按关键帧策略恢复，媒体停止清理cipher/session。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T33-01",
    "scenario": "横竖屏往返",
    "expected": "format更新、画面恢复、无越界"
  },
  {
    "id": "T33-02",
    "scenario": "连续30分钟自有测试音画 =>记录drift/丢帧/内存趋势",
    "expected": "连续30分钟自有测试音画 =>记录drift/丢帧/内存趋势"
  },
  {
    "id": "T33-03",
    "scenario": "20次断开重连",
    "expected": "每次独立key/clock generation且资源无持续增长"
  },
  {
    "id": "T33-04",
    "scenario": "HEVC不支持",
    "expected": "不广告该能力"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-airplay mirror; 与固定UxPlay基线按相同设备/网络执行真机回归`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R01](../docs/03-reference-projects.md), [R02](../docs/03-reference-projects.md), [R10](../docs/03-reference-projects.md)。

## T34 · AP1/AP2音频与持久配对扩展

**前置：** T28, T29, T31, T32。**按所选profile才需要：** 无。

**对应需求：** MEDIA-03, MEDIA-05, MEDIA-06, SEC-01, SEC-08。

**Files：**
- `crates/proto-airplay/src/audio_profiles.rs`
- `crates/proto-airplay/src/pair_store.rs`
- `tests/devices/airplay-audio.md`

**Consumes：** 音频profile独立dossier，legal provider和clock映射。

**Produces：** AP1/AP2各方向的明确能力，buffered与realtime分别声明。

**实现决策：** 先AP1 null/native音频，再AP2配对/持久store/缓冲/实时链路；使用host受控secret store，不能重用Xross设备key。多房间/5.1/7.1/输出混音每项独立profile，通过前不广告。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T34-01",
    "scenario": "持久pairing重启",
    "expected": "正确恢复或明确要求重配，不随机伪装同身份"
  },
  {
    "id": "T34-02",
    "scenario": "pairing撤销",
    "expected": "旧会话不能重新获取媒体权限"
  },
  {
    "id": "T34-03",
    "scenario": "音频capture/播放rate变化",
    "expected": "显式resample与clock重新映射"
  },
  {
    "id": "T34-04",
    "scenario": "媒体控制不支持",
    "expected": "UI不显示可操作seek"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-airplay audio_profiles; 指定sender/speaker设备矩阵`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R02](../docs/03-reference-projects.md), [R07](../docs/03-reference-projects.md), [R10](../docs/03-reference-projects.md), [R11](../docs/03-reference-projects.md)。

## T35 · AirPlay发送端分媒体URL/音频/镜像实现

**前置：** T04, T05, T24, T28, T31。**按所选profile才需要：** 无。

**对应需求：** CORE-02, MEDIA-01, MEDIA-05, MEDIA-06, MEDIA-09。

**Files：**
- `lab/specs-reviewed/airplay-sender.md`
- `crates/proto-airplay/src/sender/`
- `tests/devices/airplay-sender.md`

**Consumes：** 真实AirPlay sink、sender独立规范与受控内容源。

**Produces：** URL/音频先行，mirror source单独成功后开放。

**实现决策：** 不能把receiver状态机反转当sender；先验证media URL/音频已支持路径，再用test-pattern编码镜像。捕获真实屏幕由T48负责。每profile保留设备配对、codec、授权和端到端延迟证据。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T35-01",
    "scenario": "能发送歌曲但不能镜像",
    "expected": "只audio sender支持"
  },
  {
    "id": "T35-02",
    "scenario": "TV要求配对但未完成",
    "expected": "pairing-failed"
  },
  {
    "id": "T35-03",
    "scenario": "停止URL会话",
    "expected": "撤销host资源URL lease"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-airplay sender; 原生接收器逐profile设备测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R10](../docs/03-reference-projects.md), [R11](../docs/03-reference-projects.md), [R12](../docs/03-reference-projects.md)。

## T36 · Windows Miracast native receiver可行性切片

**前置：** T04, T05, T12, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** CORE-09, MEDIA-01, MEDIA-02, MEDIA-04, APP-08。

**Files：**
- `workers/windows-miracast/`
- `lab/research/wfd/windows-native-probe.md`
- `tests/devices/windows-miracast.md`

**Consumes：** 兼容无线网卡/Windows用户会话/WinRT API。

**Produces：** 能从Smart View/Win+K接收的native presentation或准确blocker。

**实现决策：** 先最小本地native程序查询status，按官方API先订阅MediaSourceCreated再start。记录package identity/apartment/UI线程/capability需求。owner持有MediaSource并播放；之后才probe raw/record/export，不伪造成功。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T36-01",
    "scenario": "系统报告无线不支持",
    "expected": "hardware-unavailable"
  },
  {
    "id": "T36-02",
    "scenario": "MediaSourceCreated成功但raw导出不可用",
    "expected": "presentation=true/export=false"
  },
  {
    "id": "T36-03",
    "scenario": "用户会话关闭",
    "expected": "会话正确结束、无session0黑窗"
  },
  {
    "id": "T36-04",
    "scenario": "同机Wireless Display冲突",
    "expected": "清楚提示不强停系统服务"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `构建Windows专属worker并执行真机probe；Cargo/WinRT编译只是build-verified`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R64](../docs/03-reference-projects.md), [S04](../docs/03-reference-projects.md), [S05](../docs/03-reference-projects.md)。

## T37 · WFD纯状态机与媒体协商

**前置：** T04, T05, T06, T14, T28。**按所选profile才需要：** 无。

**对应需求：** CORE-02, CORE-06, MEDIA-01, MEDIA-05, SEC-03。

**Files：**
- `lab/specs-reviewed/wfd.md`
- `crates/proto-wfd/src/rtsp.rs`
- `crates/proto-wfd/src/capabilities.rs`
- `crates/proto-wfd/src/state.rs`
- `tests/contract/wfd.rs`

**Consumes：** 获批WFD事实/规范、角色分离的states与fixtures。

**Produces：** 不碰网卡的source/sink协商core与有界RTSP解析。

**实现决策：** 先M1/M7相关角色/请求响应事实核对，再写状态机和格式选择；双方codec交集明确。UIBC/content protection是可选且单独权限；不实现的选项不广告。测试端口参数与合法转移。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T37-01",
    "scenario": "source/sink角色收到非法顺序",
    "expected": "protocol error不跳状态"
  },
  {
    "id": "T37-02",
    "scenario": "无共同codec",
    "expected": "unsupported-feature"
  },
  {
    "id": "T37-03",
    "scenario": "对端要求未支持content protection",
    "expected": "明确拒绝而非绕过"
  },
  {
    "id": "T37-04",
    "scenario": "重复PLAY/TEARDOWN",
    "expected": "幂等或按规范拒绝，无泄漏"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-wfd`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R32](../docs/03-reference-projects.md), [R33](../docs/03-reference-projects.md), [R34](../docs/03-reference-projects.md), [R37](../docs/03-reference-projects.md), [R41](../docs/03-reference-projects.md), [S24](../docs/03-reference-projects.md)。

## T38 · Linux Miracast sink平台闭环

**前置：** T12, T13, T29, T37。**按所选profile才需要：** 无。

**对应需求：** CORE-09, MEDIA-01, MEDIA-03, SEC-04, SEC-09。

**Files：**
- `workers/linux-wfd/src/sink.rs`
- `lab/research/wfd/linux-radio.md`
- `tests/devices/linux-wfd-sink.md`

**Consumes：** 指定备用P2P网卡、已审批radio lease和WFD core。

**Produces：** Android/Windows→Linux sink，必要平台依赖有回滚。

**实现决策：** 使用独立实验网卡和系统服务整合；不得照抄README在用户主机停NetworkManager。先发现/P2P组，再RTSP/RTP/TS/decoder；每阶段独立日志。root broker仅有限网络操作。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T38-01",
    "scenario": "P2P group建立失败",
    "expected": "不启动媒体播放假进度"
  },
  {
    "id": "T38-02",
    "scenario": "worker退出",
    "expected": "组/端口/临时网络配置回收"
  },
  {
    "id": "T38-03",
    "scenario": "其他LAN传输并行",
    "expected": "不被无线试验隐式断开"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `WFD core测试 + Android/Win+K→指定Linux网卡真机测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**Gate / 阻塞处理：** 无合适网卡/公开服务接口时blocked硬件profile，不影响LAN协议。

**参考：** [R32](../docs/03-reference-projects.md), [R37](../docs/03-reference-projects.md)。

## T39 · Linux Miracast source平台闭环

**前置：** T12, T13, T28, T37。**按所选profile才需要：** 无。

**对应需求：** CORE-02, MEDIA-01, MEDIA-05, MEDIA-07。

**Files：**
- `workers/linux-wfd/src/source.rs`
- `tests/devices/linux-wfd-source.md`

**Consumes：** GNOME source对照、受支持TV与test-pattern encoder。

**Produces：** Linux→TV Miracast source的具名配置。

**实现决策：** 先用自制图样与合成音频，证明source无线/协商/媒体输出；再接捕获port。TS/RTP保持时间信息，codec参数交集由core决定。与MiracleCast sink验证不能代替真实TV。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T39-01",
    "scenario": "TV只支持某H264 profile =>选择交集或明确拒绝",
    "expected": "TV只支持某H264 profile =>选择交集或明确拒绝"
  },
  {
    "id": "T39-02",
    "scenario": "丢包/带宽降低",
    "expected": "不无限积累输出queue"
  },
  {
    "id": "T39-03",
    "scenario": "用户停止",
    "expected": "capture/encoder/network全部结束"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-wfd source; 具名TV真机播放与重连测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R33](../docs/03-reference-projects.md), [R37](../docs/03-reference-projects.md), [R41](../docs/03-reference-projects.md)。

## T40 · MS-MICE基础设施传输probe

**前置：** T12, T36, T37。**按所选profile才需要：** 无。

**对应需求：** CORE-09, MEDIA-01, SEC-09。

**Files：**
- `lab/research/wfd/ms-mice.md`
- `crates/proto-wfd/src/mice.rs`
- `tests/devices/ms-mice.md`

**Consumes：** Microsoft公开协议与WFD基线。

**Produces：** 仅对已验证Windows/receiver组合声明infra扩展。

**实现决策：** 分离初始发现与后续LAN路径，验证名字解析/端口/TLS等规范前提。保留普通P2P能力但不自动降级安全。实际flow符合文档才实现Rust或native provider额外控制。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T40-01",
    "scenario": "没有初始发现条件",
    "expected": "不能声称纯网线万能MICE"
  },
  {
    "id": "T40-02",
    "scenario": "LAN路径失败",
    "expected": "经策略允许才回退P2P"
  },
  {
    "id": "T40-03",
    "scenario": "停止infra session",
    "expected": "资源/解析记录回收"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `基于固定文档的frame tests + 支持MICE设备路径抓包对照`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S06](../docs/03-reference-projects.md)。

## T41 · DLNA controller与受限media server

**前置：** T04, T05, T11, T24, T28。**按所选profile才需要：** 无。

**对应需求：** CORE-02, MEDIA-01, MEDIA-06, MEDIA-09, FILE-08。

**Files：**
- `crates/proto-upnp/src/ssdp.rs`
- `crates/proto-upnp/src/dmc.rs`
- `crates/proto-upnp/src/dms.rs`
- `tests/devices/dlna-controller.md`

**Consumes：** UPnP AV事实规范、授权resource URL、库存TV。

**Produces：** Xross推媒体到TV与有限ContentDirectory，角色分开。

**实现决策：** SSDP/XML/SOAP parser禁外部实体并限深度/体积；DMC发现renderer、查支持格式、设置AVTransport URL并控制。DMS只公开批准目录/资源，不自动扫描全盘；GENA回调验证地址scope。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T41-01",
    "scenario": "设备description指向恶意内网目标",
    "expected": "fetch policy拒绝"
  },
  {
    "id": "T41-02",
    "scenario": "未知/不支持codec",
    "expected": "禁止假装screen mirror"
  },
  {
    "id": "T41-03",
    "scenario": "ContentDirectory请求越权objectID",
    "expected": "拒绝"
  },
  {
    "id": "T41-04",
    "scenario": "stop播放",
    "expected": "URL lease按策略撤销"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-upnp; Xross→真实TV播放自有文件/seek/stop`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R46](../docs/03-reference-projects.md), [R47](../docs/03-reference-projects.md), [R48](../docs/03-reference-projects.md), [R49](../docs/03-reference-projects.md), [S27](../docs/03-reference-projects.md)。

## T42 · DLNA renderer接收媒体URL

**前置：** T29, T41。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-02, MEDIA-06, MEDIA-09。

**Files：**
- `crates/proto-upnp/src/dmr.rs`
- `tests/devices/dlna-renderer.md`

**Consumes：** SOAP/eventcore与native player/FetchPolicy。

**Produces：** 库存DMC/app→Xross媒体URL接收与播放。

**实现决策：** 实现声明的AVTransport/RenderingControl动作和状态事件；接收到URL先走policy/用户同意，native播放状态反馈给controller。未知动作明确fault；不在UI线程直接执行网络fetch。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T42-01",
    "scenario": "SetAVTransportURI为file://",
    "expected": "拒绝"
  },
  {
    "id": "T42-02",
    "scenario": "重复Stop",
    "expected": "幂等结束"
  },
  {
    "id": "T42-03",
    "scenario": "live流Seek不支持",
    "expected": "返回明确不支持状态"
  },
  {
    "id": "T42-04",
    "scenario": "外来event订阅callback越界",
    "expected": "拒绝"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-upnp dmr; 库存控制器→Xross设备测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R46](../docs/03-reference-projects.md), [R47](../docs/03-reference-projects.md), [R49](../docs/03-reference-projects.md)。

## T43 · Google Cast媒体URL sender

**前置：** T04, T05, T11, T24, T28。**按所选profile才需要：** 无。

**对应需求：** CORE-02, MEDIA-01, MEDIA-06, MEDIA-09。

**Files：**
- `lab/specs-reviewed/cast-sender.md`
- `crates/proto-cast/src/controller.rs`
- `tests/devices/cast-url.md`

**Consumes：** 合法receiver设备/app ID、Cast控制资料、bounded resource URL。

**Produces：** Xross→Chromecast媒体load/control，非屏幕sender。

**实现决策：** 按认证/heartbeat/channel/app launch/media session分层；使用真实设备证书验证策略，不全局关闭TLS。媒体URL可达性来自host gateway lease，seek/volume状态由receiver回复。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T43-01",
    "scenario": "TLS/auth失败",
    "expected": "不启动播放"
  },
  {
    "id": "T43-02",
    "scenario": "app launch失败",
    "expected": "明确错误不是media loading"
  },
  {
    "id": "T43-03",
    "scenario": "停止或会话被receiver终止",
    "expected": "清理URL与控制状态"
  },
  {
    "id": "T43-04",
    "scenario": "只URL可用",
    "expected": "screen能力保持false"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-cast controller; 真实Chromecast/GoogleTV指定app播放自有媒体`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R42](../docs/03-reference-projects.md), [R43](../docs/03-reference-projects.md), [R44](../docs/03-reference-projects.md), [R45](../docs/03-reference-projects.md), [S07](../docs/03-reference-projects.md), [S08](../docs/03-reference-projects.md), [S09](../docs/03-reference-projects.md)。

## T44 · Cast实时streaming sender

**前置：** T28, T29, T43。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-03, MEDIA-05, MEDIA-07。

**Files：**
- `lab/research/cast/streaming-source.md`
- `workers/cast-streaming/`
- `tests/devices/cast-streaming.md`

**Consumes：** Open Screen streaming API/测试receiver、合法实际Cast设备。

**Produces：** test-pattern→Cast实时sender；capture后接入。

**实现决策：** 先合规libcast provider建立双端demo，不重写成熟streaming以求形式纯Rust。分别核验官方receiver app和设备认证后再声明stock支持。记录拥塞反馈、帧时间、keyframe恢复和codec。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T44-01",
    "scenario": "自有sender/receiver demo成功",
    "expected": "仅simulated/reference-pair证据"
  },
  {
    "id": "T44-02",
    "scenario": "真实TV可播但音频失败",
    "expected": "仅video profile"
  },
  {
    "id": "T44-03",
    "scenario": "拥塞积压",
    "expected": "bounded queue/合理请求关键帧"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `libcast固定版本demo测试 + 实际Cast receiver独立qualify`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R42](../docs/03-reference-projects.md)。

## T45 · 通用Google Cast receiver可行性gate

**前置：** T04, T05, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** CORE-02, CORE-03, MEDIA-02, APP-08, SEC-10。

**Files：**
- `lab/research/cast/receiver-gate.md`
- `lab/pocs/cast-receiver/`
- `tests/devices/cast-receiver-auth.md`

**Consumes：** Open Screen receiver demo、stock Pixel/Chrome、授权说明。

**Produces：** 分别认证/launch/media成功的报告或明确blocked。

**实现决策：** 先用测试credentials运行自有demo闭环，再stock sender探测；记录卡在discovery、device authentication、app namespace还是streaming。不得获取/伪造他人受限设备私钥来填补gate。公开CAF sample不被当桌面接收SDK。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T45-01",
    "scenario": "自有测试root受信但Pixel不认可",
    "expected": "stock receiver=blocked"
  },
  {
    "id": "T45-02",
    "scenario": "只能被discover不能launch",
    "expected": "不记录cast成功"
  },
  {
    "id": "T45-03",
    "scenario": "认证/授权路径不明",
    "expected": "停止产品集成但保留合法demo"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `固定Open Screen构建/测试 + Pixel/Chrome具名版本的分阶段设备probe`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**Gate / 阻塞处理：** 只有合法认证和真实stock互操作证据通过后才产生可shipping receiver provider。

**参考：** [R42](../docs/03-reference-projects.md), [R43](../docs/03-reference-projects.md), [R45](../docs/03-reference-projects.md), [S07](../docs/03-reference-projects.md), [S08](../docs/03-reference-projects.md), [S09](../docs/03-reference-projects.md)。
