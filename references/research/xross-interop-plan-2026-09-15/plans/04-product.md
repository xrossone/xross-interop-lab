# Playground、XROSS集成与发布 Implementation Plan

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

## T46 · Tauri文件/发现控制台

**前置：** T09, T10, T11, T17。**按所选profile才需要：** 无。

**对应需求：** APP-02, APP-03, APP-04, FILE-09。

**Files：**
- `apps/playground/src-tauri/src/bridge.rs`
- `apps/playground/src/routes/devices/`
- `apps/playground/src/routes/transfers/`
- `apps/playground/tests/control.spec.ts`

**Consumes：** 已鉴权IPC client、fake/真实provider事件。

**Produces：** 只使用相同API的Devices/Offers/Transfers UI。

**实现决策：** frontend只发typed intents；native side持有scoped client token。文件选择转read lease，目录选择转root grant；不开放通用shell/fs。处理watch gap/instance变化/worker断开；所有远端名称text渲染。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T46-01",
    "scenario": "显示名含<script>或控制符",
    "expected": "不执行/破坏界面"
  },
  {
    "id": "T46-02",
    "scenario": "窗口关闭但用户选择后台文件接收",
    "expected": "daemon任务继续"
  },
  {
    "id": "T46-03",
    "scenario": "无approval scope客户端点击接受",
    "expected": "明确auth-denied"
  },
  {
    "id": "T46-04",
    "scenario": "事件gap",
    "expected": "重新snapshot而非保留错误百分比"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `pnpm --dir apps/playground test; pnpm --dir apps/playground exec playwright test`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S18](../docs/03-reference-projects.md), [S19](../docs/03-reference-projects.md)。

## T47 · Tauri媒体与诊断测试台

**前置：** T28, T29, T30, T46。**按所选profile才需要：** 无。

**对应需求：** MEDIA-02, MEDIA-04, MEDIA-06, APP-02, APP-03, APP-04, APP-05。

**Files：**
- `apps/playground/src/routes/media/`
- `apps/playground/src/routes/diagnostics/`
- `apps/playground/src-tauri/src/presentation.rs`
- `apps/playground/tests/media.spec.ts`

**Consumes：** MediaDescriptor/native presentation参考、stats与evidence exporter。

**Produces：** 可选择sink/预览/停止/记录指标的媒体面板。

**实现决策：** 先显示独立native window生命周期，不强迫嵌入WebView。controls来自capability；native-only禁用record/export。stats明确测量方法，表格区分networkRTT与端到端。原始trace只在lab受控打开。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T47-01",
    "scenario": "native-only session",
    "expected": "record/forward按钮禁用且API同样拒绝"
  },
  {
    "id": "T47-02",
    "scenario": "关闭媒体页默认stop",
    "expected": "没有孤儿音频播放"
  },
  {
    "id": "T47-03",
    "scenario": "停止录制权限未授予",
    "expected": "禁止创建录制文件"
  },
  {
    "id": "T47-04",
    "scenario": "诊断导出",
    "expected": "不含token/PIN/私有文件内容"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `Playwright mock媒体事件测试 + 本机native window生命周期E2E`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S05](../docs/03-reference-projects.md), [S18](../docs/03-reference-projects.md), [S19](../docs/03-reference-projects.md)。

## T48 · 接入真实桌面/窗口/系统音频capture

**前置：** T07, T12, T28, T29。**按所选profile才需要：** T35, T39, T44。

**对应需求：** MEDIA-01, MEDIA-05, MEDIA-08, SEC-01, SEC-04。

**Files：**
- `crates/interop-platform/src/capture.rs`
- `workers/capture/`
- `tests/devices/screen-capture.md`

**Consumes：** 已验证test-pattern sender、公开capture API与host授权。

**Produces：** monitor/window/audio source ports，用户可撤销。

**实现决策：** 逐OS查询合法capture API和权限，首次只选一个窗口/屏幕；系统音频另授权。Wayland走portal，macOS/Windows按当前SDK文档；不要用私有注入作为默认捕获。pipeline记录codec转换/编码器预算。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T48-01",
    "scenario": "用户拒绝屏幕权限",
    "expected": "permission-required而非空黑帧成功"
  },
  {
    "id": "T48-02",
    "scenario": "共享时用户撤销",
    "expected": "立即停止capture与远端stream"
  },
  {
    "id": "T48-03",
    "scenario": "选择窗口A",
    "expected": "不意外泄露屏幕B"
  },
  {
    "id": "T48-04",
    "scenario": "声卡变化",
    "expected": "重新协商格式，不变速播放"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `platform capture fake tests + 每个计划发布OS的权限/撤销真机测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R33](../docs/03-reference-projects.md), [R56](../docs/03-reference-projects.md), [R57](../docs/03-reference-projects.md)。

## T49 · 显式跨协议转发与Media bridge

**前置：** T07, T08, T10, T24, T28。**按所选profile才需要：** 无。

**对应需求：** CORE-05, MEDIA-08, FILE-08, SEC-01, INT-07。

**Files：**
- `crates/interop-runtime/src/bridge.rs`
- `tests/contract/bridge.rs`
- `apps/playground/src/routes/bridge/`

**Consumes：** 来源session、目标endpoint能力与两端grants。

**Produces：** 有目的/有hop限制/重新授权的bridge任务。

**实现决策：** 建立source→bridge→target两个会话，不沿用source外部身份获得target权限。文件完整落盘后转发为初始策略；流式中继只有明确协议/预算才支持。媒体codec交集优先remux，需要转码明确审批。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T49-01",
    "scenario": "原始source指定公网上传地址",
    "expected": "需新目标授权"
  },
  {
    "id": "T49-02",
    "scenario": "相同route循环回来",
    "expected": "hop/route检测阻断"
  },
  {
    "id": "T49-03",
    "scenario": "没有record/export能力的NativePresentation",
    "expected": "不可桥接"
  },
  {
    "id": "T49-04",
    "scenario": "桥接成功",
    "expected": "UI说明在本机解密/重加密，不称原始端到端E2EE"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p interop-runtime bridge`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

## T50 · XROSS文件/Shelf真实集成

**前置：** T03, T07, T08, T15, T17, T18。**按所选profile才需要：** T21, T22。

**对应需求：** INT-01, INT-02, INT-03, INT-04, FILE-10。

**Files：**
- `adapters/xross-host/src/files.rs`
- `adapters/xross-host/src/offers.rs`
- `tests/contract/xross_host.rs`
- `lab/evidence/xross-file-integration/`

**Consumes：** 当前主仓已核对接口、至少LocalSend和QuickShare接收闭环。

**Produces：** 从原生外部分享进入Xross既有offer/history/content流程。

**实现决策：** 主仓只新增薄adapter和feature开关。把external principal以原有权限表达接入，不登录第二账号；bytes由外部wire处理但发布和存储遵循host。audience不能因跨协议桥接放大；保留file clipboard fast-path语义。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T50-01",
    "scenario": "外部接收文件",
    "expected": "主transfer中心一条任务、一个内容receipt"
  },
  {
    "id": "T50-02",
    "scenario": "相同内容但不共享audience",
    "expected": "不泄露存在性或合并权限"
  },
  {
    "id": "T50-03",
    "scenario": "禁用interop",
    "expected": "原有Xross file/clipboard/LocalSend行为不变"
  },
  {
    "id": "T50-04",
    "scenario": "provider重启",
    "expected": "不实例化第二Iroh/profile"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `主仓既有回归 + adapters/xross-host契约测试；记录双方commit与实际命令`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

## T51 · XROSS Media Graph/native presentation集成

**前置：** T03, T07, T28, T29, T30。**按所选profile才需要：** T36。

**对应需求：** INT-01, INT-02, INT-05, MEDIA-02, MEDIA-03, MEDIA-08。

**Files：**
- `adapters/xross-host/src/media.rs`
- `tests/contract/xross_media.rs`
- `lab/evidence/xross-media-integration/`

**Consumes：** 主仓真实media ports，encoded与native-only两种provider。

**Produces：** 统一Xross媒体入口，不强制重复录屏/转码。

**实现决策：** 先接AirPlay compressed输出和Windows native-only source两个形态，证明抽象没有只适合一种协议。将media permission映射现有host权限；virtual camera/OBS若还没稳定，只保留可选sink而不阻塞基本播放。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T51-01",
    "scenario": "AirPlay encoded source",
    "expected": "host decoder播放并保留clock"
  },
  {
    "id": "T51-02",
    "scenario": "Windows native-only",
    "expected": "host显示合法native窗口且不提供虚假export"
  },
  {
    "id": "T51-03",
    "scenario": "停止会话",
    "expected": "Xross侧player/source/worker均回收"
  },
  {
    "id": "T51-04",
    "scenario": "一个provider异常",
    "expected": "其他Remote Audio/文件会话不受影响"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `主仓媒体契约回归 + 两种source form的桌面E2E`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S05](../docs/03-reference-projects.md)。

## T52 · 针对首批协议的fuzz与对抗测试

**前置：** T08, T09, T13, T14, T28。**按所选profile才需要：** T20, T32, T37。

**对应需求：** SEC-03, SEC-04, SEC-05, SEC-08, FILE-05, MEDIA-07。

**Files：**
- `tests/fuzz/`
- `tests/security/`
- `policies/limits.json`
- `lab/evidence/security/`

**Consumes：** 首批parser/worker/lease实现和固定fixtures。

**Produces：** 可复现crash corpus、修复回归与安全gate。

**实现决策：** RTSP/HTTP/TLV/plist/protobuf/IPC/metadata分别target；覆盖任意分片、重复长度、超深结构、整数溢出、恶意URI、symlink race、token跨scope。fuzz只在隔离runner跑，时间/seed/corpus版本保留；找到crash先最小化再修复。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T52-01",
    "scenario": "任一可复现panic/OOM/任意文件读取",
    "expected": "该profile release阻断"
  },
  {
    "id": "T52-02",
    "scenario": "fuzz无crash",
    "expected": "只声称此范围测试通过，不声称安全证明"
  },
  {
    "id": "T52-03",
    "scenario": "日志注入可伪造审批事件",
    "expected": "测试失败并转义修复"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `按各target执行cargo-fuzz/属性测试与OS隔离E2E；记录实际执行次数和持续时间`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S16](../docs/03-reference-projects.md), [S17](../docs/03-reference-projects.md)。

## T53 · 建立具名真机兼容性矩阵

**前置：** T14。**按所选profile才需要：** T17, T18, T21, T22, T30, T36, T41, T43。

**对应需求：** CORE-03, APP-05, APP-08, MEDIA-05。

**Files：**
- `tests/devices/matrix.json`
- `lab/evidence/device-runs/`
- `docs/compatibility.md`

**Consumes：** 可运行首批providers、真实iPhone/Android/Windows/TV。

**Produces：** 每个profile/role/platform/device组合的实测证据。

**实现决策：** 使用测试计划文档中的最小设备矩阵与合法自制内容。分别记录发现、配对、接受、媒体/字节、停止、重连；不能把同品牌某机型通过泛化所有ROM。无需等全部候选通过，只qualify本次release范围。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T53-01",
    "scenario": "只有仿真测试",
    "expected": "不能进入device-verified列表"
  },
  {
    "id": "T53-02",
    "scenario": "同型号OS更新",
    "expected": "旧结果保留并新增待复测行"
  },
  {
    "id": "T53-03",
    "scenario": "失败涉及特定codec",
    "expected": "只关闭该profile变体，不假称全协议失败或成功"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `执行docs/10-validation-and-device-lab.md中的具名用例，生成实际run manifests`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

## T54 · 构建清晰许可的发布产物

**前置：** T05, T13, T46, T47。**按所选profile才需要：** 无。

**对应需求：** SEC-06, SEC-07, SEC-10, APP-07, INT-08。

**Files：**
- `packaging/provider-manifest.json`
- `packaging/NOTICE`
- `packaging/licenses/`
- `packaging/sbom/`
- `tests/packaging/`

**Consumes：** 已批准provider路线、目标OS/arch/features。

**Produces：** 独立可追溯的core/worker/playground产物和源码义务资料。

**实现决策：** 每产物列transitive依赖/feature/native codecs、GPL/LGPL/SDK义务；独立GPLworker不可被默认静态连进permissive core。不以根repo LICENSE判定产物。进行签名/校验，禁止运行时从陌生URL下载可执行provider。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T54-01",
    "scenario": "release含未批准GPL dependency",
    "expected": "构建gate拒绝"
  },
  {
    "id": "T54-02",
    "scenario": "关闭profile但仍携带其native library",
    "expected": "SBOM/打包测试发现"
  },
  {
    "id": "T54-03",
    "scenario": "worker hash与manifest不符",
    "expected": "拒绝启动"
  },
  {
    "id": "T54-04",
    "scenario": "来源包/notice缺项",
    "expected": "不发布受影响产物"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `产物SBOM/license检查+各OS安装包清单比较；使用真实产物hash`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S01](../docs/03-reference-projects.md), [S02](../docs/03-reference-projects.md), [S19](../docs/03-reference-projects.md), [S20](../docs/03-reference-projects.md), [S23](../docs/03-reference-projects.md)。

## T55 · CI、schema兼容、升级与回滚

**前置：** T09, T10, T13, T50, T51, T54。**按所选profile才需要：** 无。

**对应需求：** CORE-06, CORE-07, APP-06, APP-07, INT-06。

**Files：**
- `.github/workflows/ci.yml`
- `tests/upgrade/`
- `packaging/state-migrations.md`
- `docs/operations.md`

**Consumes：** contract版本、scope store、worker manifests。

**Produces：** 跨版本测试/资源回收/升级回滚策略。

**实现决策：** CI按批准feature matrix运行，不自动all-features商用打包；PR权限最小/禁生产secrets。N/N−1 control与worker兼容，状态迁移先备份再原子切换，明确不可恢复session不自动假resume。保留旧产物回滚路由。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T55-01",
    "scenario": "旧UI收到未知可选字段",
    "expected": "可忽略"
  },
  {
    "id": "T55-02",
    "scenario": "旧worker缺关键feature",
    "expected": "拒绝而非降级授权"
  },
  {
    "id": "T55-03",
    "scenario": "升级中杀进程",
    "expected": "配对store可恢复且无半写"
  },
  {
    "id": "T55-04",
    "scenario": "回滚",
    "expected": "不遗留旧监听或宽权限token"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `CI矩阵+N/N−1 golden/upgrade tests；主仓相关integration回归`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S16](../docs/03-reference-projects.md), [S17](../docs/03-reference-projects.md), [S19](../docs/03-reference-projects.md)。

## T56 · 首批profile的release gate与能力发布

**前置：** T50, T51, T52, T53, T54, T55。**按所选profile才需要：** 无。

**对应需求：** CORE-03, APP-05, APP-08, INT-08。

**Files：**
- `releases/<version>/qualification.json`
- `docs/compatibility.md`
- `docs/known-limitations.md`

**Consumes：** 每个目标profile证据、安全与许可决议。

**Produces：** 限范围可用release，受限/实验能力明确隐藏或标注。

**实现决策：** 按profile检查证据、oncall/维护责任、依赖更新策略、安装/回滚和授权。主产品只加载此次qualified provider。单个Huawei/Cast receiver gate失败不阻断LocalSend/AirPlay/QuickShare等独立能力。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T56-01",
    "scenario": "缺device evidence但目录有实现",
    "expected": "只能experimental不进普通支持列表"
  },
  {
    "id": "T56-02",
    "scenario": "未来protocol backlog未做",
    "expected": "不阻断本次已限定release"
  },
  {
    "id": "T56-03",
    "scenario": "任何安全发布阻断项",
    "expected": "移除受影响profile后重验产物"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `逐项执行release checklist；所有实际结论写qualification.json，不以计划本身当证据`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。
