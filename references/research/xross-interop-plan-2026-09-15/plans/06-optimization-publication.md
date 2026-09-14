# 性能与公开交付 Implementation Plan

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

## T78 · 性能、背压与radio共存优化

**前置：** T14, T28, T29, T53。**按所选profile才需要：** T23, T33, T38, T39。

**对应需求：** FILE-03, MEDIA-03, MEDIA-07, MEDIA-10, APP-05。

**Files：**
- `benchmarks/`
- `docs/performance-profiles.md`
- `lab/evidence/performance/`
- `tests/contract/backpressure.rs`

**Consumes：** 真机baseline、目标硬件与已稳定profile。

**Produces：** 按profile的实测预算与可重复对照，不按LOC/token估算性能。

**实现决策：** 先分阶段计时/内存/CPU/queue/音画drift；用同设备参考receiver进行A/B。文件流不为提高吞吐绕过hash/磁盘提交；视频拥塞不无限缓冲。无线操作对现有LAN任务的影响纳入指标。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T78-01",
    "scenario": "慢磁盘/consumer",
    "expected": "有界背压、无OOM"
  },
  {
    "id": "T78-02",
    "scenario": "持续30分钟",
    "expected": "记录内存趋势/时钟偏移并有阈值依据"
  },
  {
    "id": "T78-03",
    "scenario": "sender+receiver不同codec",
    "expected": "不隐藏转码CPU/延迟"
  },
  {
    "id": "T78-04",
    "scenario": "无测量依据",
    "expected": "UI不显示虚假毫秒精度"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `运行明确设备/config的基准用例并生成原始指标CSV与summary`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

## T79 · 按测量需要升级共享内存/native-view输出

**前置：** T13, T28, T29, T47, T78。**按所选profile才需要：** 无。

**对应需求：** MEDIA-02, MEDIA-07, SEC-03, SEC-04, APP-02。

**Files：**
- `crates/interop-ipc/src/shared_ring.rs`
- `workers/presentation/native_view/`
- `tests/security/shared_ring.rs`

**Consumes：** 证明拷贝/数据通道是瓶颈的profiling证据。

**Produces：** 可选ring/texture/native-view fast path，保留基线fallback。

**实现决策：** 只在真实瓶颈存在时做；验证producer index/slot length/generation、对端进程身份、FD/handle授权与释放。shared memory不存长期秘密；畸形元信息不能让consumer越界读写或use-after-release。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T79-01",
    "scenario": "producer伪造slot length",
    "expected": "消费端拒绝"
  },
  {
    "id": "T79-02",
    "scenario": "旧generation handle重复使用",
    "expected": "拒绝"
  },
  {
    "id": "T79-03",
    "scenario": "窗口销毁后texture回调",
    "expected": "安全取消不崩溃"
  },
  {
    "id": "T79-04",
    "scenario": "无法跨平台实现",
    "expected": "保留binary pipe/native-window不阻断功能"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `共享内存属性/fuzz/OS句柄测试 + 基线/fast-path一致性回归`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

## T80 · 公开实现仓库与Xross依赖化交付

**前置：** T05, T50, T51, T54, T55, T56。**按所选profile才需要：** 无。

**对应需求：** SEC-06, SEC-07, SEC-10, INT-01, INT-06, INT-08。

**Files：**
- `README.md`
- `NOTICE`
- `docs/provenance-summary.md`
- `docs/public-api.md`
- `packaging/source-bundles/`
- `releases/publication-review.json`

**Consumes：** 有来源证据的实现、合规产物和稳定host契约。

**Produces：** 可公开的库/worker/CLI与Xross版本化依赖，不人工复制lab。

**实现决策：** 保留真实provenance；审查公开docs/captures是否含第三方受限内容或用户隐私。决定各component许可，不把混合tree一律MIT。主产品用版本/commit依赖或独立worker manifest，提供升级兼容说明与安全报告入口。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T80-01",
    "scenario": "公开包含受限SDK/原始私有抓包",
    "expected": "发布拒绝"
  },
  {
    "id": "T80-02",
    "scenario": "文档声称独立实现但来源为GPL翻译",
    "expected": "纠正路线/许可，不抹历史"
  },
  {
    "id": "T80-03",
    "scenario": "Xross升级interop后旧provider不兼容",
    "expected": "version gate阻断而不是运行失控"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `公开包license/privacy/provenance review+完整首批qualified矩阵回归`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S01](../docs/03-reference-projects.md), [S02](../docs/03-reference-projects.md), [S03](../docs/03-reference-projects.md), [S23](../docs/03-reference-projects.md)。
