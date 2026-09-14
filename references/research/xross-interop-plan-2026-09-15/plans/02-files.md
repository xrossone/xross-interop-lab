# 文件互操作实现 Implementation Plan

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

## T15 · 冻结现有LocalSend行为并决定抽取方式

**前置：** T03, T04, T05, T14。**按所选profile才需要：** 无。

**对应需求：** FILE-10, INT-03, INT-04。

**Files：**
- `lab/research/localsend/reuse-map.md`
- `tests/devices/localsend-baseline.md`
- `adapters/xross-host/localsend-map.md`

**Consumes：** 主仓当前LocalSend及库存客户端、官方v2.2文档。

**Produces：** 复用边界、已有通过用例、明确缺口；不重复重写。

**实现决策：** 先在主仓当前commit录双向baseline；比较listener lifecycle、fingerprint、offer/session、TLS、文件commit。若现有code可抽到library，做adapter；否则保持host-owned LocalSend，Interop只桥接事件直到有迁移证据。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T15-01",
    "scenario": "原有Android→Xross成功但新adapter失败",
    "expected": "阻断迁移"
  },
  {
    "id": "T15-02",
    "scenario": "已有库不依赖UI",
    "expected": "优先直接适配而非另写状态机"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `执行integration-map中实际主仓LocalSend测试命令；记录库存客户端版本`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R13](../docs/03-reference-projects.md), [R14](../docs/03-reference-projects.md)。

## T16 · 接入LocalSend正确发现机制

**前置：** T11, T15。**按所选profile才需要：** 无。

**对应需求：** CORE-04, CORE-05, INT-04。

**Files：**
- `crates/proto-localsend/src/discovery.rs`
- `tests/contract/localsend_discovery.rs`

**Consumes：** 已通过复用决策的LocalSend模块与EndpointObservation。

**Produces：** 选定接口的multicast/HTTP registration adapter。

**实现决策：** 按固定版本规范接入multicast announce/response和HTTP registration；保留port可配置。fingerprint用于协议范围识别，不认定Xross身份。避免announce互相无限触发与网络切换旧地址残留。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T16-01",
    "scenario": "收到announce=false",
    "expected": "不触发广播风暴"
  },
  {
    "id": "T16-02",
    "scenario": "自己的fingerprint",
    "expected": "不显示自己"
  },
  {
    "id": "T16-03",
    "scenario": "multicast不可用但明确可达HTTP地址",
    "expected": "使用合法fallback"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-localsend discovery`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R13](../docs/03-reference-projects.md), [R14](../docs/03-reference-projects.md)。

## T17 · 接入LocalSend接收与安全落盘

**前置：** T08, T09, T10, T16。**按所选profile才需要：** 无。

**对应需求：** FILE-01, FILE-02, FILE-03, FILE-04, FILE-05, FILE-06, FILE-09, INT-04。

**Files：**
- `crates/proto-localsend/src/receive.rs`
- `tests/contract/localsend_receive.rs`
- `tests/devices/localsend-receive.md`

**Consumes：** 现有LocalSend接收器、OfferAuthority、ContentWriteLease。

**Produces：** 库存sender→统一offer/transfer/history的接收闭环。

**实现决策：** 把准备/批准/文件token绑定映射到host authority；验证token只能上传被批准的entry。支持协议允许的选择范围，wire cancellation及时释放spool。收完整并验证后才publish receipt。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T17-01",
    "scenario": "未批准entry被上传",
    "expected": "auth-denied"
  },
  {
    "id": "T17-02",
    "scenario": "上传token换session使用",
    "expected": "auth-denied"
  },
  {
    "id": "T17-03",
    "scenario": "同名3文件",
    "expected": "不覆盖既有文件且返回独立receipts"
  },
  {
    "id": "T17-04",
    "scenario": "取消中断上传",
    "expected": "无最终文件、临时资源按策略回收"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-localsend receive; 库存Android与Windows sender人工E2E`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R13](../docs/03-reference-projects.md), [R14](../docs/03-reference-projects.md)。

## T18 · 接入LocalSend发送与重试语义

**前置：** T08, T10, T16, T17。**按所选profile才需要：** 无。

**对应需求：** FILE-01, FILE-03, FILE-04, FILE-06, FILE-07, FILE-08, INT-04。

**Files：**
- `crates/proto-localsend/src/send.rs`
- `tests/contract/localsend_send.rs`
- `tests/devices/localsend-send.md`

**Consumes：** ContentReadLease/Endpoint registry/库存receiver。

**Produces：** 本地CLI→库存LocalSend双向完成。

**实现决策：** 发送方只有用户所选files的read leases；接收拒绝映射auth-denied；部分接受按真实reply处理。不假设原生transfer的resume能映射LocalSend，断线按已验证wire能力选择重新attempt。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T18-01",
    "scenario": "对端拒绝全部",
    "expected": "不读取/发送payload"
  },
  {
    "id": "T18-02",
    "scenario": "源文件中途变化",
    "expected": "source-changed"
  },
  {
    "id": "T18-03",
    "scenario": "网络断开无resume支持",
    "expected": "retry新attempt，不显示透明续传"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-localsend send; 库存macOS/Android receiver人工E2E`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R13](../docs/03-reference-projects.md), [R14](../docs/03-reference-projects.md)。

## T19 · 建立Quick Share wire与来源gate

**前置：** T02, T04, T05, T14。**按所选profile才需要：** 无。

**对应需求：** CORE-02, SEC-02, SEC-06, APP-08。

**Files：**
- `lab/research/quickshare/dossier.md`
- `lab/specs-reviewed/quickshare-lan.md`
- `lab/evidence/quickshare/`
- `lab/provenance/quickshare-inputs.json`

**Consumes：** 固定NearDrop/Bada/Nearby/UKEY2源码和库存Android环境。

**Produces：** LAN/QR profile的独立事实规范、测试向量、来源允许列表。

**实现决策：** 分别记录endpoint discovery、UKEY2、channel framing、paired-key验证、introduction、payload与cancel。为LAN接收和反向发送列不同发现前提。GPL分析只在restricted区；不得把其Rust函数翻译后当MIT新实现。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T19-01",
    "scenario": "只有对端被discover但握手失败",
    "expected": "状态不能记transfer成功"
  },
  {
    "id": "T19-02",
    "scenario": "二维码打开可发现不等于认证",
    "expected": "仍需确认/校验完整会话"
  },
  {
    "id": "T19-03",
    "scenario": "文档与两实现相冲突",
    "expected": "用具名真机实验裁决并记录"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `依据dossier模板审核并运行只读reference比较；无未证实必需字段才能进入T20`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R15](../docs/03-reference-projects.md), [R16](../docs/03-reference-projects.md), [R17](../docs/03-reference-projects.md), [R18](../docs/03-reference-projects.md), [R19](../docs/03-reference-projects.md), [R20](../docs/03-reference-projects.md)。

## T20 · 实现Quick Share受限安全会话核心

**前置：** T06, T07, T10, T14, T19。**按所选profile才需要：** 无。

**对应需求：** CORE-06, SEC-01, SEC-03, SEC-06。

**Files：**
- `crates/proto-quickshare/src/framing.rs`
- `crates/proto-quickshare/src/handshake.rs`
- `crates/proto-quickshare/src/session.rs`
- `tests/contract/quickshare_crypto.rs`

**Consumes：** 已放行spec/独立fixture和成熟密码原语crate。

**Produces：** 可离线测试的handshake/state/framing，不含UI/文件系统。

**实现决策：** 严格按已确认suite实现状态转换与key derivation，使用成熟crypto实现，不自写椭圆曲线/AES。验证每步transcript/commitment/确认码；拒绝重放、截断、未知帧类型和不合法长度。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T20-01",
    "scenario": "确认码/transcript不匹配",
    "expected": "pairing-failed、不交付payload"
  },
  {
    "id": "T20-02",
    "scenario": "重放旧handshake frame",
    "expected": "拒绝"
  },
  {
    "id": "T20-03",
    "scenario": "长度溢出/截断protobuf",
    "expected": "invalid-frame，无panic"
  },
  {
    "id": "T20-04",
    "scenario": "TCP任意分片",
    "expected": "与完整输入同结果"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-quickshare handshake; cargo test -p proto-quickshare framing`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R17](../docs/03-reference-projects.md), [R18](../docs/03-reference-projects.md)。

## T21 · Quick Share LAN接收闭环

**前置：** T08, T11, T19, T20。**按所选profile才需要：** 无。

**对应需求：** FILE-01, FILE-02, FILE-03, FILE-04, FILE-05, FILE-06, FILE-09。

**Files：**
- `crates/proto-quickshare/src/receive.rs`
- `crates/proto-quickshare/src/discovery.rs`
- `tests/devices/quickshare-receive.md`

**Consumes：** 握手core、approved discovery facts、host grants。

**Produces：** Android原生Quick Share→macOS/Linux独立receiver。

**实现决策：** 先选同LAN、发送端手动分享作为最小路径；会话元数据转Offer，host批准后stream写lease。校验wire payloadID与entryID绑定、块offset、长度和terminal通知；对方claim的MIME只作提示。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T21-01",
    "scenario": "两个payloadID混入同entry",
    "expected": "拒绝且不发布"
  },
  {
    "id": "T21-02",
    "scenario": "用户拒绝",
    "expected": "对端显示拒绝/失败、无最终文件"
  },
  {
    "id": "T21-03",
    "scenario": "10GiB合成文件/配额允许",
    "expected": "常数级内存流式传输"
  },
  {
    "id": "T21-04",
    "scenario": "发送方伪造filename穿越",
    "expected": "FILE-05规则拒绝"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-quickshare receive; Android原生QuickShare→Mac真机运行并记录hash`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R15](../docs/03-reference-projects.md), [R16](../docs/03-reference-projects.md), [R17](../docs/03-reference-projects.md)。

## T22 · Quick Share LAN/QR发送闭环

**前置：** T08, T11, T20, T21。**按所选profile才需要：** 无。

**对应需求：** CORE-05, FILE-01, FILE-03, FILE-04, FILE-08。

**Files：**
- `crates/proto-quickshare/src/send.rs`
- `crates/proto-quickshare/src/qr.rs`
- `tests/devices/quickshare-send.md`

**Consumes：** 已验证receiver profile与发送发现方式。

**Produces：** Mac/Linux→原生Android；二维码/手动可发现明确显示。

**实现决策：** 先实现已验证QR或用户显式打开可见状态，不要求macOS发不可用BLE广告。只选合法endpoint和read leases；错误确认码/远端拒绝不发送数据；不要把Google账户同联系人功能假装已实现。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T22-01",
    "scenario": "Android未可发现",
    "expected": "提示正确入口/二维码而非伪造设备"
  },
  {
    "id": "T22-02",
    "scenario": "远端确认码错",
    "expected": "pairing-failed"
  },
  {
    "id": "T22-03",
    "scenario": "取消发送",
    "expected": "停流并清理两端会话可见状态"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-quickshare send; 原生Android接收真机测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R15](../docs/03-reference-projects.md), [R16](../docs/03-reference-projects.md), [R17](../docs/03-reference-projects.md)。

## T23 · Quick Share BLE/P2P能力probe与可选升级

**前置：** T12, T20, T21, T22。**按所选profile才需要：** 无。

**对应需求：** CORE-09, SEC-09, APP-08。

**Files：**
- `crates/interop-platform/src/quickshare_radio.rs`
- `lab/research/quickshare/radio-matrix.md`
- `tests/devices/quickshare-p2p.md`

**Consumes：** 已验证LAN transfer和radio lease。

**Produces：** 每OS的BLE bootstrap/P2P能力声明与受控fallback。

**实现决策：** 先在Android/Linux/Windows指定设备probe，macOS不能公开实现的标不可用。先证明广播与组连接，再接会话升级；验证升级与原握手身份绑定。不能关闭用户网络或自动请求管理员。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T23-01",
    "scenario": "BLE可扫描但无法所需广告",
    "expected": "只声明scan不声明bootstrap"
  },
  {
    "id": "T23-02",
    "scenario": "P2P升级失败",
    "expected": "保持合法LAN路径或明确失败，无数据串session"
  },
  {
    "id": "T23-03",
    "scenario": "P2P组结束",
    "expected": "网络状态恢复证据"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `平台fake测试+指定网卡真机probe；记录前后网络状态`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**Gate / 阻塞处理：** 没有公开API/硬件支持时交付blocked profile；LAN能力继续发布。

**参考：** [R16](../docs/03-reference-projects.md), [R17](../docs/03-reference-projects.md), [R19](../docs/03-reference-projects.md), [R63](../docs/03-reference-projects.md)。

## T24 · 浏览器HTTPS/QR临时文件网关

**前置：** T07, T08, T09, T10。**按所选profile才需要：** 无。

**对应需求：** FILE-01, FILE-05, FILE-08, SEC-05, SEC-08, APP-04, INT-03。

**Files：**
- `crates/interop-file/src/http_gateway.rs`
- `apps/browser-share/`
- `tests/e2e/browser_share.rs`

**Consumes：** host内容read/write lease与现有Xross HTTP gateway映射。

**Produces：** 有限时间/文件/方法的浏览器上传下载入口。

**实现决策：** 优先复用主仓已有range/version/lease逻辑，standalone实现同ports。下载URL仅作用单资源；上传要求明确授权、origin/CSRF防护和quota。TLS信任不合格时清晰说明实验条件，不关闭证书校验。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T24-01",
    "scenario": "token换资源ID",
    "expected": "403"
  },
  {
    "id": "T24-02",
    "scenario": "URL过期或stop",
    "expected": "410/403"
  },
  {
    "id": "T24-03",
    "scenario": "Range时源版本改变",
    "expected": "source-changed而非拼接新旧文件"
  },
  {
    "id": "T24-04",
    "scenario": "跨域网页伪造上传/审批",
    "expected": "被拒绝"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p interop-file http_gateway; 浏览器E2E`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R13](../docs/03-reference-projects.md), [S27](../docs/03-reference-projects.md)。

## T25 · Windows NearShare协议研究与adapter

**前置：** T04, T05, T08, T14。**按所选profile才需要：** 无。

**对应需求：** CORE-02, FILE-01, FILE-02, FILE-05, SEC-01。

**Files：**
- `lab/research/nearshare/dossier.md`
- `crates/proto-nearshare/`
- `tests/devices/nearshare.md`

**Consumes：** MS-CDP资料与固定Android实现，独立运行Windows原生对端。

**Produces：** 经认证的NearShare→Offer adapter或有证据blocker。

**实现决策：** 先确认公开规范/上层NearShare消息、发现介质、配对、证书和Windows版本，输出reviewed spec后实现。测试direction分别记录；所有received files仍通过同一个storage lease。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T25-01",
    "scenario": "Windows Share入口发现不到Xross",
    "expected": "先记录discovery blocker，不写文件解析伪装完成"
  },
  {
    "id": "T25-02",
    "scenario": "未配对发送文件",
    "expected": "需要审批/身份规则"
  },
  {
    "id": "T25-03",
    "scenario": "完成后hash不一致",
    "expected": "integrity-failed"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `来源gate后cargo test -p proto-nearshare；Windows原生Share双向设备测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R23](../docs/03-reference-projects.md), [S28](../docs/03-reference-projects.md)。

## T26 · KDE Connect仅share子集

**前置：** T04, T05, T07, T08, T14。**按所选profile才需要：** 无。

**对应需求：** FILE-01, FILE-02, SEC-01, SEC-04。

**Files：**
- `lab/research/kdeconnect/dossier.md`
- `workers/kdeconnect/`
- `tests/devices/kdeconnect-share.md`

**Consumes：** 库存KDE Connect和选择的合法实现路线。

**Produces：** 只包含pair/discovery/share的provider。

**实现决策：** 先external daemon或来源审查后独立实现；协议pairing identity单独store；capability whitelist只share/必需metadata。远端其他plugin requests默认unsupported，不将host全部插件接上。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T26-01",
    "scenario": "share配对后请求runcommand",
    "expected": "unsupported-feature/auth-denied"
  },
  {
    "id": "T26-02",
    "scenario": "撤销配对后新文件",
    "expected": "要求重新批准"
  },
  {
    "id": "T26-03",
    "scenario": "文本URL与文件",
    "expected": "分别映射合适offer"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `独立provider tests + 库存KDE/GSConnect双向share测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R24](../docs/03-reference-projects.md), [R25](../docs/03-reference-projects.md)。

## T27 · Bluetooth OPP系统接口接收/发送

**前置：** T04, T05, T08, T12。**按所选profile才需要：** 无。

**对应需求：** CORE-09, FILE-01, FILE-02, FILE-05。

**Files：**
- `lab/research/obex/dossier.md`
- `workers/obex/`
- `tests/devices/obex.md`

**Consumes：** 所选OS的公开OBEX服务和指定设备。

**Produces：** OPP能力probe与有限文件share provider。

**实现决策：** Linux优先使用系统BlueZ/obex服务接口；别把蓝牙配对当任意文件接受。Windows/mobile对公开API和系统profile逐项probe；不可用的平台显示对应限制。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T27-01",
    "scenario": "未批准OBEX push",
    "expected": "无最终文件"
  },
  {
    "id": "T27-02",
    "scenario": "低速/未知总大小",
    "expected": "显示真实bytes不假百分比"
  },
  {
    "id": "T27-03",
    "scenario": "蓝牙中断",
    "expected": "明确失败/重试新attempt"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `platform接口测试+Android↔受支持PC真机OPP测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R61](../docs/03-reference-projects.md), [S21](../docs/03-reference-projects.md)。
