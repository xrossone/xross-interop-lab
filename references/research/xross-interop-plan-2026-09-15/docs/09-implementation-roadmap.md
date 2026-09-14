# 实施路线、并行边界与第一批执行任务

## 1. 不采用“大一统重写后一次合并”

这是一个有80个独立任务的长期互操作项目，不是80个任务都必须完成才能看见第一条文件或画面。按纵向切片验收：发现→授权→传输/播放→停止→证据→Xross薄集成。

可行性问题（未知wire、SDK权限、网卡能力）以**有证据的go/no-go决议**结项；不能给所有问题预设“AI一定能写出来”。局部blocked不阻塞其他profile，但对外宣传不包含blocked项。

## 2. 里程碑

| 里程碑 | 任务组合 | 可独立交付的结果 | 离开gate条件 |
|---|---|---|---|
| G0 来源与边界 | T01–T05，按首批profile处理 | clean工作区、固定来源、当前Xross映射、研究dossier | 不执行陌生源码；自有实现/worker路线明确 |
| G1 headless契约 | T06–T14 | mock discovery/offer、CLI IPC、safe spool、events、worker lifecycle | 正/负契约测试可运行；不是一堆空trait |
| G2 第一条真实文件 | T15–T18 + T46 + T50最小部分 | LocalSend库存双向→统一存储/审批/历史 | 已有Xross行为不退化；文件hash/取消/拒绝通过 |
| G3 第一条真实媒体 | T28–T30 + T47 + T51最小部分 | iPhone→UxPlay provider→native/null/file sink | 同设备可复现、停止/崩溃独立、不碰Xross秘密 |
| G4 Android原生文件入口 | T19–T22 + T50扩展 | Quick Share LAN/QR双向，独立Rust候选 | 当前Android/ROM具名测试；来源与确认码/路径安全通过 |
| G5 电视与Windows覆盖 | T36、T41–T43 | Windows Miracast native接收；DLNA与Cast媒体发送 | 角色/媒体形态不混淆；TV真机与URL安全通过 |
| G6 Rust媒体与无线扩展 | T31–T40、T44、T48，按gate | 可来源审查的AirPlay Rust、Linux WFD等 | 与库存参考对照，不把代码量作为完成证据 |
| G7 产品化 | T49–T56，对当前选择范围 | XROSS集成、打包、回滚、公开支持矩阵 | 安全、许可、devices、版本兼容四项并过 |
| G8 广泛扩展 | T25–T27、T45、T57–T77 | 标准流、其他share、TV/mobile/vendor研究 | 每profile独立交付；供应商未许可则不发 |
| G9 性能与独立发布 | T78–T80 | 测量驱动优化、公开core/合法workers | 真实provenance、稳定API、可追踪升级 |

G2与G3可并行；G5的Windows probe可在G1之后开始，不要求Rust WFD已实现。G7针对**指定profile集合**执行，不能要求G8/所有vendor成功。

## 3. 第一次给本地AI的scope

仅执行：**T01、T02（file-core/airplay必要来源）、T03、T04（F01/M01/F02）、T05、T06–T10、T14。**

交付：目录、source lock、来源/风险分析、当前主仓映射、可运行mock headless API与存储负向测试。不要同时写AirPlay、Miracast、Google Cast三套完整协议；不要自动clone后`cargo run --all-features`。

第二批再开两个互不冲突的分支：`feat/localsend-host-adapter`和`poc/uxplay-provider`；共享契约已冻结，任何更改由一个integration owner裁决。

## 4. 并行工作单元

| 工作流 | 可独立做什么 | 不允许并行独裁改变什么 |
|---|---|---|
| 协议研究agent | 独立profile dossier、规范/源码比较、真机probe计划 | 不能修改共同identity/Grant/schema |
| 平台agent | Windows native/WiFi/AndroidTV权限probe | 不自动更改生产机网络；不替所有profile作支持结论 |
| core agent | 契约、file leases、IPC、event semantics | 不依靠某协议特有字段把core绑死 |
| provider agent | 一种profile/方向的状态机与port实现 | 不导入主仓private internals或开新账号 |
| UI agent | mock事件驱动UI/用户审批 | 不在frontend拥有token/privatekey或推测传输成功 |
| integration owner | schema/跨模块测试/主仓契约映射 | 不因赶工跳过profile来源或负向验收 |

每个任务分支有明确触碰文件；共同`interop-contract`变化要短ADR+version fixture更新，不用几个AI互相改同一个大型types.rs。先集成一个有用能力再扩大抽象，避免上次主项目“功能快于基础稳定”的问题重现。

## 5. 采用现成实现还是重写：决策算法

依次判断：系统API能满足所需output吗？有许可合适的library吗？独立worker能否满足契约/体积/安全/体验？仍不满足且来源独立实现可行，才选纯Rust协议实现。

对当前方向：LocalSend优先复用；Quick Share优先已审查的permissive资料独立Rust或合法worker；AirPlay先UxPlay外部基线再评估独立实现；Windows WFD先系统API；Linux WFD受无线条件影响；Cast先media sender，generic receiver是单独认证gate。

**重写的验证目标是减少实际耦合和维护风险，不是达成“1–2万行全变Rust”的数字。**自行重写不自动解决已知crypto设计风险、设备兼容、DRM、法务或系统权限。

## 6. 研究上下文管理

先生成每repo模块图与有来源的dossier，不把64个repo全喂进同一个高权限coding session。实现任务只读当前profile获批规范、接口、fixtures。源码token用实际所选tokenizer按固定文件集合计算；LOC不能可靠证明fit/理解质量，也不是许可证明。

## 7. 分册

- [基础与研究T01–T14](../plans/01-foundation.md)
- [文件T15–T27](../plans/02-files.md)
- [投屏与媒体T28–T45](../plans/03-casting.md)
- [产品/Tauri/XrossT46–T56](../plans/04-product.md)
- [扩展T57–T77](../plans/05-extensions.md)
- [性能/公开T78–T80](../plans/06-optimization-publication.md)

完整机器可读backlog在[任务JSON](../manifests/tasks.json)，每task包含依赖、需求、文件、接口输入/输出、实现决策和具体验收case。
