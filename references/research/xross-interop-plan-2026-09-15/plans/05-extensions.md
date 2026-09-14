# 扩展协议与厂商路线 Implementation Plan

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

## T57 · RTSP/RTP标准媒体适配

**前置：** T04, T05, T28, T29。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-03, MEDIA-05, MEDIA-07。

**Files：**
- `crates/proto-rtsp/`
- `tests/devices/rtsp.md`

**Consumes：** 独立RTSP1/2/发布与拉取profile、固定codec/容器。

**Produces：** 可用标准网络media sink/source，和mirror区别明确。

**实现决策：** 先订阅/服务一个合法H264流，再按对端支持实现ANNOUNCE/RECORD发布。保留RTP/RTCP时钟、sequence回绕和丢包处理；认证无默认公网匿名listener。wire/parser来自已审查路线。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T57-01",
    "scenario": "RTSP2对RTSP1-only",
    "expected": "明确不支持"
  },
  {
    "id": "T57-02",
    "scenario": "RTP乱序/丢包",
    "expected": "有界缓冲并按keyframe恢复"
  },
  {
    "id": "T57-03",
    "scenario": "坏SSRC/session",
    "expected": "不串流"
  },
  {
    "id": "T57-04",
    "scenario": "对端断开",
    "expected": "media ended与资源回收"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-rtsp; 固定VLC/GStreamer/MediaMTX对端测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R50](../docs/03-reference-projects.md), [R51](../docs/03-reference-projects.md), [R52](../docs/03-reference-projects.md), [R53](../docs/03-reference-projects.md), [S26](../docs/03-reference-projects.md)。

## T58 · WebRTC、WHIP与明确版本WHEP适配

**前置：** T04, T05, T07, T28, T29。**按所选profile才需要：** 无。

**对应需求：** MEDIA-03, MEDIA-05, MEDIA-07, SEC-05。

**Files：**
- `crates/proto-webrtc/`
- `tests/devices/webrtc.md`
- `lab/research/webrtc/signaling-profiles.md`

**Consumes：** 主仓既有WebRTC选型或已审查栈、WHIP RFC9725。

**Produces：** 与浏览器/OBS兼容的明确HTTP信令profile。

**实现决策：** 复用ICE/DTLS/SRTP实现，WHIP按标准定义POST session/Location/PATCH/DELETE与鉴权。WHEP固定实际文档/实现版本并记录成熟度，不假设WebRTC自带通用信令。TURN/credentials由host有限配置。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T58-01",
    "scenario": "没有bearer的受限WHIP POST",
    "expected": "拒绝"
  },
  {
    "id": "T58-02",
    "scenario": "其他session URL DELETE",
    "expected": "拒绝"
  },
  {
    "id": "T58-03",
    "scenario": "ICE restart/网络变更",
    "expected": "成功恢复或明确终止"
  },
  {
    "id": "T58-04",
    "scenario": "不受支持WHEP版本",
    "expected": "明确不匹配"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p proto-webrtc; 浏览器+指定媒体服务双向测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R50](../docs/03-reference-projects.md), [R54](../docs/03-reference-projects.md), [S22](../docs/03-reference-projects.md)。

## T59 · HLS媒体资源与播放拓展

**前置：** T24, T28, T29。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-06, MEDIA-09。

**Files：**
- `crates/interop-media/src/hls.rs`
- `tests/e2e/hls.rs`

**Consumes：** 有限resource URL与FetchPolicy、合法自制HLS。

**Produces：** playlist/segments/字幕的受限发布与播放。

**实现决策：** 主manifest和所有子资源走同授权/URL策略；区分VOD/live和seek能力；语言轨等只显示已解析支持项。segment窗口回收有上限，禁止无限缓存。不可把HLS高buffer模式宣传为低延迟镜像。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T59-01",
    "scenario": "子playlist重定向到未授权地址",
    "expected": "拒绝"
  },
  {
    "id": "T59-02",
    "scenario": "token到期后segment请求",
    "expected": "拒绝"
  },
  {
    "id": "T59-03",
    "scenario": "live窗口之外seek",
    "expected": "不支持/范围错误"
  },
  {
    "id": "T59-04",
    "scenario": "多语言轨选择",
    "expected": "每次取资源仍守scope"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `cargo test -p interop-media hls; 浏览器/native player合法样例E2E`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R50](../docs/03-reference-projects.md), [R52](../docs/03-reference-projects.md), [R60](../docs/03-reference-projects.md), [S27](../docs/03-reference-projects.md)。

## T60 · WebDAV只读先行适配

**前置：** T03, T04, T05, T07, T08, T24。**按所选profile才需要：** 无。

**对应需求：** FILE-05, FILE-08, INT-01, INT-03。

**Files：**
- `lab/research/webdav/library-selection.md`
- `workers/webdav/`
- `tests/devices/webdav.md`

**Consumes：** Xross Remote Files已存在读/Range接口和许可合格库。

**Produces：** 只读WebDAV client/server scope；写能力后续独立授权。

**实现决策：** 先核对成熟库feature与license，映射PROPFIND/GET/HEAD到scoped host entries；深度/条目/属性响应预算固定。写/LOCK/rename不能凭函数存在就放行，单独增加conformance用例再开放。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T60-01",
    "scenario": "Depth=infinity超预算",
    "expected": "有界拒绝"
  },
  {
    "id": "T60-02",
    "scenario": "未授权目录PROPFIND",
    "expected": "无存在性泄露"
  },
  {
    "id": "T60-03",
    "scenario": "Range读源变更",
    "expected": "失败不拼接"
  },
  {
    "id": "T60-04",
    "scenario": "只读share PUT",
    "expected": "拒绝"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `库原生测试+Xross契约测试+库存WebDAV客户端读/拒绝写验证`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S25](../docs/03-reference-projects.md), [S27](../docs/03-reference-projects.md)。

## T61 · SFTP复用现有SSH能力

**前置：** T03, T04, T05, T07, T08。**按所选profile才需要：** 无。

**对应需求：** FILE-05, FILE-08, INT-01, INT-02。

**Files：**
- `adapters/xross-host/src/sftp.rs`
- `lab/research/sftp/interop-profile.md`
- `tests/devices/sftp.md`

**Consumes：** 现有Xross SSH/SFTP引擎和CredentialRef授权。

**Produces：** 不另造SSH/Vault的SFTP服务适配。

**实现决策：** client方向优先复用现有连接/host key校验；server新增外部暴露需独立policy与目录权限，禁止隐式shell。不要把SFTP认证成功授予Xross账户/好友身份。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T61-01",
    "scenario": "host key变化",
    "expected": "拒绝而非自动接受"
  },
  {
    "id": "T61-02",
    "scenario": "只有SFTP scope请求PTY",
    "expected": "auth-denied"
  },
  {
    "id": "T61-03",
    "scenario": "路径越界read/write",
    "expected": "被host lease拒绝"
  },
  {
    "id": "T61-04",
    "scenario": "取消上传",
    "expected": "不发布半文件"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `现有Xross SSH/SFTP回归+库存SFTP客户端/服务器interop`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S30](../docs/03-reference-projects.md)。

## T62 · SMB3系统/成熟服务方案可行性

**前置：** T03, T04, T05, T07, T08。**按所选profile才需要：** 无。

**对应需求：** CORE-09, FILE-05, FILE-08, SEC-01。

**Files：**
- `lab/research/smb/decision.md`
- `workers/smb/`
- `tests/devices/smb.md`

**Consumes：** 所选平台系统服务/成熟SMB实现的官方资料。

**Produces：** client/server分开、仅授权share的provider或blocker。

**实现决策：** 先调研系统共享是否可受控集成，必要library另入source intake。禁SMB1降级/匿名全盘；记录auth/sign/encryption与credential边界。server装服务/开端口不得自动做。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T62-01",
    "scenario": "没有签名/加密满足要求",
    "expected": "禁止静默降级"
  },
  {
    "id": "T62-02",
    "scenario": "重复/冲突share名称",
    "expected": "不修改系统现有share"
  },
  {
    "id": "T62-03",
    "scenario": "越权路径/凭据错误",
    "expected": "拒绝且不泄露metadata"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `平台probe+库存SMB3客户端受限share互通`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**Gate / 阻塞处理：** 没有适合的许可/公开API时只交决议，不从零写完整SMB安全栈。

**参考：** [S29](../docs/03-reference-projects.md)。

## T63 · Magic Wormhole兼容provider

**前置：** T04, T05, T07, T08, T13。**按所选profile才需要：** 无。

**对应需求：** FILE-01, FILE-02, FILE-05, FILE-07, SEC-08。

**Files：**
- `workers/wormhole/`
- `lab/research/wormhole/profile.md`
- `tests/devices/wormhole.md`

**Consumes：** 固定协议/库/relay配置、source intake。

**Produces：** 独立CLI兼容收发，与Xross身份分开。

**实现决策：** 先用合法参考CLI在独立worker接受host file leases；短码输入不记录到日志，rendezvous/relay由用户配置。只实现固定compatible version，取消和重试按协议真实能力映射。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T63-01",
    "scenario": "短码不一致",
    "expected": "pairing失败无payload"
  },
  {
    "id": "T63-02",
    "scenario": "中继不可用",
    "expected": "明确network failure"
  },
  {
    "id": "T63-03",
    "scenario": "用户取消",
    "expected": "rendezvous/session/临时文件回收"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `库存Wormhole CLI双向+中继失效/取消测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R28](../docs/03-reference-projects.md), [R29](../docs/03-reference-projects.md)。

## T64 · croc兼容provider

**前置：** T04, T05, T07, T08, T13。**按所选profile才需要：** 无。

**对应需求：** FILE-01, FILE-02, FILE-03, FILE-07, SEC-08。

**Files：**
- `workers/croc/`
- `lab/research/croc/profile.md`
- `tests/devices/croc.md`

**Consumes：** 固定croc版本与合法CLI/provider。

**Produces：** croc↔Xross文件互通实验能力。

**实现决策：** 先独立程序封装不复制其整个应用进core；把code/relay配置视为secret/连接条件。验证CLI输出稳定性，必要时补结构化接口并保留来源许可。版本握手不匹配明确拒绝。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T64-01",
    "scenario": "code泄露到普通日志",
    "expected": "测试失败"
  },
  {
    "id": "T64-02",
    "scenario": "对端拒绝",
    "expected": "本地任务不继续读文件"
  },
  {
    "id": "T64-03",
    "scenario": "partial transfer重启",
    "expected": "只按已验证croc语义恢复，不继承Xross假resume"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `库存croc双向文件与中断回归`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R30](../docs/03-reference-projects.md)。

## T65 · PairDrop/Snapdrop网页部署适配

**前置：** T04, T05, T07, T08, T58。**按所选profile才需要：** 无。

**对应需求：** FILE-01, FILE-02, SEC-01, APP-04。

**Files：**
- `lab/research/browser-interop/profile.md`
- `workers/browser-signaling/`
- `tests/e2e/pairdrop.md`

**Consumes：** 指定版本信令服务器/页面及许可路线。

**Produces：** 明确限定部署的网页互通provider。

**实现决策：** 研究的是应用信令与传输元数据，不把WebRTC当完整分享协议。隔离房间/peer可见性、上传批准、session tokens；域名/服务器绑定，禁止陌生signaling列出全部Xross设备。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T65-01",
    "scenario": "加入不同房间",
    "expected": "不看到对方文件/身份"
  },
  {
    "id": "T65-02",
    "scenario": "恶意服务器要求额外目录",
    "expected": "无权读取"
  },
  {
    "id": "T65-03",
    "scenario": "浏览器关闭",
    "expected": "会话终止/重连规则明确"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `指定PairDrop/Snapdrop版本部署+浏览器E2E`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R26](../docs/03-reference-projects.md), [R27](../docs/03-reference-projects.md)。

## T66 · Syncthing BEP/同步边界决议

**前置：** T03, T04, T05, T14。**按所选profile才需要：** 无。

**对应需求：** INT-01, INT-03, INT-08, SEC-01。

**Files：**
- `lab/research/syncthing/dossier.md`
- `lab/decisions/sync-boundary.md`
- `lab/pocs/syncthing-provider/`

**Consumes：** Xross现有Sync/Shelf权威、Syncthing协议/许可。

**Produces：** 是否提供互通gateway的清晰决议和最小只读/单向POC。

**实现决策：** 先评估对等持续同步语义、folder授权、版本向量、删除传播和冲突。首个可行实验只在空临时目录单向传测试文件；不能把两个sync引擎同时写同一生产目录。扩展实现必须单独scope批准。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T66-01",
    "scenario": "收到远端删除",
    "expected": "未授权双向同步时不删本地文件"
  },
  {
    "id": "T66-02",
    "scenario": "Syncthing identity与Xross身份混用",
    "expected": "设计测试拒绝"
  },
  {
    "id": "T66-03",
    "scenario": "双向引擎同目录导致循环",
    "expected": "scope不批准"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `协议dossier审查+独立临时目录单向POC`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**Gate / 阻塞处理：** 这是可选同步互通路线，不是本项目必须重写的新Xross Sync。

**参考：** [R31](../docs/03-reference-projects.md)。

## T67 · AirDrop平台与独立实现gate

**前置：** T02, T04, T05, T12, T14。**按所选profile才需要：** 无。

**对应需求：** CORE-03, CORE-09, FILE-01, FILE-02, SEC-06, SEC-10。

**Files：**
- `lab/research/airdrop/dossier.md`
- `lab/pocs/airdrop/`
- `tests/devices/airdrop.md`

**Consumes：** 用户授权测试设备、固定研究实现、AWDL平台证据。

**Produces：** 现代系统可行性和可发布路线；失败有具体阶段blocker。

**实现决策：** 先库存设备互传基线，再公开/合法实验profile；分discover、TLS/identity、offer、archive/stream四关。原生contacts可见模式与用户授权单独记录，不能借用私有身份材料或隐藏可见限制。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T67-01",
    "scenario": "只有老iOS成功",
    "expected": "不外推当前iOS"
  },
  {
    "id": "T67-02",
    "scenario": "AWDL链路失败",
    "expected": "在传输层记录blocker"
  },
  {
    "id": "T67-03",
    "scenario": "归档路径穿越",
    "expected": "storage拒绝"
  },
  {
    "id": "T67-04",
    "scenario": "对端未同意",
    "expected": "不传文件"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `指定Apple设备版本的同意可发现模式下逐阶段probe`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R21](../docs/03-reference-projects.md), [R22](../docs/03-reference-projects.md)。

## T68 · 互传联盟MDFE研究与厂商路径

**前置：** T02, T04, T05, T12。**按所选profile才需要：** 无。

**对应需求：** CORE-02, CORE-03, SEC-06, SEC-10。

**Files：**
- `lab/research/mdfe/dossier.md`
- `lab/research/mdfe/vendor-contacts.md`
- `tests/devices/mdfe.md`

**Consumes：** 两家中国ROM真机、厂商公开资料/可取得SDK。

**Produces：** 是否有可合法实现spec/API的结论与最小双向实验计划。

**实现决策：** 先确认具体系统菜单与版本属于联盟profile而非Google Quick Share；搜官方开发者入口/申请资料，记录发现与链路，不猜加密材料。缺公开spec则保持research，不写凭空Rust兼容栈。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T68-01",
    "scenario": "三星国际版QuickShare成功",
    "expected": "不当作中国互传通过"
  },
  {
    "id": "T68-02",
    "scenario": "只有厂商宣传互通",
    "expected": "仍无Xross实现证据"
  },
  {
    "id": "T68-03",
    "scenario": "授权资料不可公开",
    "expected": "原始内容留受控lab，公开实现不直接引用"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `厂家原生↔原生基线+资料gate；合法wire事实充分后另行落实对应adapter任务`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S14](../docs/03-reference-projects.md)。

## T69 · Huawei Share SDK provider

**前置：** T04, T05, T07, T08, T12, T13。**按所选profile才需要：** 无。

**对应需求：** CORE-09, FILE-01, FILE-02, SEC-04, SEC-07, SEC-10。

**Files：**
- `lab/research/huawei-share/sdk-matrix.md`
- `workers/huawei-share/`
- `tests/devices/huawei-share.md`

**Consumes：** 正式取得的SDK/条款/版本/设备支持说明。

**Produces：** SDK worker与同一Offer接口；支持平台实测矩阵。

**实现决策：** 验证EMUI/HarmonyOS代际、OS/网卡依赖和可分发条件。SDK binary hash锁定，不传云凭据/文件目录。先Win/Linux官方supported路径probe，再考虑其他平台，不能自动宣称Mac支持。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T69-01",
    "scenario": "SDK缺失/无分发权",
    "expected": "vendor-gated"
  },
  {
    "id": "T69-02",
    "scenario": "新鸿蒙无法发现旧SDK",
    "expected": "标具体不兼容"
  },
  {
    "id": "T69-03",
    "scenario": "SDK请求任意path",
    "expected": "只通过已批准临时file lease适配"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `官方SDK sample+provider contract tests+指定Huawei设备双向测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S11](../docs/03-reference-projects.md), [S12](../docs/03-reference-projects.md), [S13](../docs/03-reference-projects.md)。

## T70 · Huawei Cast+与OpenHarmony系统provider

**前置：** T04, T05, T12, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** CORE-03, CORE-09, MEDIA-01, MEDIA-02, SEC-10。

**Files：**
- `lab/research/huawei-cast/architecture-map.md`
- `workers/huawei-cast/`
- `workers/openharmony-cast/`
- `tests/devices/huawei-cast.md`

**Consumes：** 官方SDK合法资料或固定OpenHarmony系统源码。

**Produces：** 明确分开commercial SDK/system component/public app三种可行性。

**实现决策：** 先映射Cast framework/WFD/Cast+stream/softbus实际依赖。OpenHarmony自制系统闭环不外推商用Huawei设备；普通App可用性单独编译/权限probe。可行时只接已有media source contract。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T70-01",
    "scenario": "系统privileged应用成功",
    "expected": "普通App能力仍未验证"
  },
  {
    "id": "T70-02",
    "scenario": "Cast+SDK许可不允许分发",
    "expected": "不进入release bundle"
  },
  {
    "id": "T70-03",
    "scenario": "native source只能播放",
    "expected": "不广告录制/转发"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `指定SDK/System build的最小sample+普通App权限probe+设备对端实验`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R37](../docs/03-reference-projects.md), [R38](../docs/03-reference-projects.md), [R39](../docs/03-reference-projects.md), [R40](../docs/03-reference-projects.md), [S10](../docs/03-reference-projects.md)。

## T71 · scrcpy授权ADB provider

**前置：** T04, T05, T07, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-06, MEDIA-08, SEC-01, SEC-04。

**Files：**
- `workers/scrcpy/`
- `tests/devices/scrcpy.md`

**Consumes：** 用户显式启用/授权ADB的测试Android设备。

**Produces：** 有标注的ADB显示provider，view-only默认。

**实现决策：** 保持固定client/server版本，用稳定数据接口或合法patch；不自动打开ADB或批准调试指纹。视频/音频和control权限分开。只把它归类ADB连接工具，不宣称Smart View兼容。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T71-01",
    "scenario": "用户未确认ADB",
    "expected": "不连接"
  },
  {
    "id": "T71-02",
    "scenario": "view-only尝试输入",
    "expected": "auth-denied"
  },
  {
    "id": "T71-03",
    "scenario": "撤销ADB授权",
    "expected": "终止且不自动重授信"
  },
  {
    "id": "T71-04",
    "scenario": "设备断开",
    "expected": "不残留server进程/任务"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `库存scrcpy基线+Xross provider view/control权限设备测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R56](../docs/03-reference-projects.md)。

## T72 · Sunshine/Moonlight GameStream provider

**前置：** T04, T05, T07, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** MEDIA-01, MEDIA-03, MEDIA-05, MEDIA-06, SEC-01, SEC-10。

**Files：**
- `workers/gamestream/`
- `tests/devices/gamestream.md`

**Consumes：** 许可审查后的host/client组件、测试设备。

**Produces：** 可选低延迟画面provider，输入与媒体权限分开。

**实现决策：** 不把GPL core直接link到不兼容产物；先各自库存client/host互通再adapter。只在用户选择时允许capture/launch/input；codec/控制器/多显示器是分别profile，不先追全部游戏优化。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T72-01",
    "scenario": "只看权限请求launch app",
    "expected": "拒绝"
  },
  {
    "id": "T72-02",
    "scenario": "配对撤销",
    "expected": "不能恢复旧session"
  },
  {
    "id": "T72-03",
    "scenario": "网络丢包",
    "expected": "记录实际延迟/恢复，不假装lossless"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `库存Sunshine与Moonlight互操作+媒体/输入权限回归`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R57](../docs/03-reference-projects.md), [R58](../docs/03-reference-projects.md)。

## T73 · 标准媒体SRT适配

**前置：** T04, T05, T07, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** MEDIA-03, MEDIA-05, MEDIA-07, SEC-05。

**Files：**
- `workers/srt/`
- `tests/devices/srt.md`

**Consumes：** 许可允许的libsrt/provider和指定MPEGTS/codec组合。

**Produces：** caller/listener初版，rendezvous另标可选。

**实现决策：** 把SRT连接角色和媒体方向分开；listen须host批准接口/port，stream ID不直接决定任意文件路径。口令/加密要求fail closed。容器codec交集由media模块表达。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T73-01",
    "scenario": "口令错误",
    "expected": "拒绝且不播放垃圾数据"
  },
  {
    "id": "T73-02",
    "scenario": "慢receiver",
    "expected": "有界队列/超时"
  },
  {
    "id": "T73-03",
    "scenario": "恶意stream ID含路径",
    "expected": "不触发本地文件创建"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `libsrt原生测试+OBS/GStreamer库存对端测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R50](../docs/03-reference-projects.md), [R55](../docs/03-reference-projects.md)。

## T74 · 标准媒体RTMP/RTMPS适配

**前置：** T04, T05, T07, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** MEDIA-03, MEDIA-05, SEC-05, SEC-08。

**Files：**
- `workers/rtmp/`
- `tests/devices/rtmp.md`

**Consumes：** 审查后的媒体服务和有限stream key。

**Produces：** 发布/接收profile，TLS与codec支持显式。

**实现决策：** 优先worker/成熟库，控制stream key作用域/生命周期；默认RTMPS，明文实验需用户明确知情且限制接口。不要自动支持enhanced-RTMP全部codec，逐组合记录。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T74-01",
    "scenario": "错误key发布",
    "expected": "无媒体会话"
  },
  {
    "id": "T74-02",
    "scenario": "对端TLS验证失败",
    "expected": "不降级明文"
  },
  {
    "id": "T74-03",
    "scenario": "日志含stream key",
    "expected": "失败"
  },
  {
    "id": "T74-04",
    "scenario": "停止后新包",
    "expected": "丢弃并结束"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `OBS↔测试服务自制媒体流+key/TLS错误测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R50](../docs/03-reference-projects.md), [R59](../docs/03-reference-projects.md), [R60](../docs/03-reference-projects.md)。

## T75 · NDI合法SDK接入gate

**前置：** T04, T05, T12, T13, T28, T29。**按所选profile才需要：** 无。

**对应需求：** CORE-03, MEDIA-02, SEC-07, SEC-10。

**Files：**
- `lab/research/ndi/sdk-decision.md`
- `workers/ndi/`
- `tests/devices/ndi.md`

**Consumes：** 正式SDK与授权/分发/平台文档。

**Produces：** 是否采用SDK以及支持格式/方向的有证据决议。

**实现决策：** 把SDK作为独立native依赖审查，确认可发布平台、架构、商标/条款、网络发现范围。无合法SDK资料则不编写假兼容。取得后映射video/audio/metadata与媒体clock，不获得隐式输入权限。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T75-01",
    "scenario": "许可或SDK版本未固定",
    "expected": "vendor-gated"
  },
  {
    "id": "T75-02",
    "scenario": "仅特定pixelformat可用",
    "expected": "仅该profile"
  },
  {
    "id": "T75-03",
    "scenario": "SDK升级",
    "expected": "重新核对hash/ABI/许可与设备回归"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `合法SDK samples + media contract测试；无SDK时只交gate文档`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [S31](../docs/03-reference-projects.md)。

## T76 · DIAL与厂商电视控制子profile

**前置：** T04, T05, T07, T11, T24。**按所选profile才需要：** 无。

**对应需求：** CORE-02, CORE-05, SEC-01, MEDIA-06。

**Files：**
- `lab/research/tv-control/dossiers/`
- `workers/tv-control/`
- `tests/devices/tv-control.md`

**Consumes：** 具体TV型号的官方公开API/SDK授权。

**Produces：** 仅discover/launch/control能力，与media transport拆分。

**实现决策：** 按DIAL/Tizen/webOS/Roku分别dossier，先只launch用户选定测试app；pair/pin/token存在时必须正常完成。不要把能开TV app显示成screen mirror support。新的库先进入intake，不能因需要功能直接npm安装执行。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T76-01",
    "scenario": "未pair请求控制",
    "expected": "拒绝"
  },
  {
    "id": "T76-02",
    "scenario": "电视支持launch但不支持媒体流",
    "expected": "capabilities只control"
  },
  {
    "id": "T76-03",
    "scenario": "用户未选目标app",
    "expected": "不执行任意launch"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `具名TV API sample+权限/动作allowlist测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R43](../docs/03-reference-projects.md), [S07](../docs/03-reference-projects.md)。

## T77 · 移动端与TV packaging能力验证

**前置：** T12, T13, T28, T29, T54。**按所选profile才需要：** 无。

**对应需求：** CORE-09, APP-06, APP-07, INT-05, INT-08。

**Files：**
- `lab/research/platforms/android-tv.md`
- `lab/research/platforms/harmonyos.md`
- `lab/research/platforms/ios.md`
- `adapters/mobile-host/`
- `tests/devices/mobile-lifecycle.md`

**Consumes：** 已稳定LAN/file/media契约和平台SDK实际可用条件。

**Produces：** 不用桌面spawn假设的embedding/native lifecycle方案。

**实现决策：** 先AndroidTV LAN接收+native播放，再Harmony公共socket/mDNS/media API，iOS后台/extension按实际限制。Tizen/webOS只在SDK允许时接，不能假定Tauri能装所有TV。系统receiver已有时不重复注册冲突服务。

**验收case（需要实现为测试）：**

```json
[
  {
    "id": "T77-01",
    "scenario": "App退后台被系统暂停",
    "expected": "明确不可用/恢复，不声称always-on"
  },
  {
    "id": "T77-02",
    "scenario": "电视用遥控器审批",
    "expected": "完整可操作无鼠标依赖"
  },
  {
    "id": "T77-03",
    "scenario": "系统不允许spawn sidecar",
    "expected": "使用合法embedding或标blocked"
  },
  {
    "id": "T77-04",
    "scenario": "商店规则不允许特定组件",
    "expected": "对应发行版不包含该feature"
  }
]
```

- [ ] **1. 固定输入。**确认上述来源/契约和真实commit；只允许本task获批资料进入上下文。
- [ ] **2. 写出失败测试或受控probe。**逐个实现上述case；确认当前缺失功能导致预期失败，不能跳过失败测试。设备任务先记录基线失败阶段。
- [ ] **3. 实现本task最小闭环。**严格执行上面的实现决策，不扩展其他profile/隐式权限。
- [ ] **4. 运行并保留验证证据。** `各平台SDK构建与指定设备前台/后台/息屏/卸载测试`。测试方法/实际命令、设备条件、结果和hash写入run manifest。
- [ ] **5. 独立复核与提交。**复核正常和负向case、来源、依赖与资源回收；提交仅包含本task相关实现/测试/文档。不要把未取得真机证据的状态写成release-qualified。

**参考：** [R04](../docs/03-reference-projects.md), [R36](../docs/03-reference-projects.md), [R37](../docs/03-reference-projects.md), [S10](../docs/03-reference-projects.md), [S11](../docs/03-reference-projects.md), [S12](../docs/03-reference-projects.md), [S13](../docs/03-reference-projects.md)。
