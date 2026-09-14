# 研究流程、来源与许可管理

## 1. 先纠正一个危险前提

**私有lab、独立repo、空Git history、AI重新生成代码，都不是许可证规避机制。**目标是形成真实、可复查的独立实现/合法复用证据，而不是让外界看不到研究过什么。不要删去相关研究历史以制造“未看过原代码”的印象。

一般需要区分：不受版权保护的思想/功能/方法与具体受保护表达；精确边界依司法辖区和事实而定。翻译源码为Rust仍可能属于衍生，不能按“变量名全改了”自行改成MIT。研究一个GPL项目不自动让所有未来代码GPL，但把它的表达、独特结构、表格、常量或测试代码搬入新产物要单独判断。[S01–S03](03-reference-projects.md)

本计划是工程风险管理，不是律师出具的法律意见。商业分发还可能涉及协议许可、专利/编解码器、认证商标、SDK合同与反规避规则；开放源码许可不能独自解决这些问题。

## 2. 四条合法复用路线，逐provider选择

| 路线 | 可行条件 | 产物处理 |
|---|---|---|
| 合规直接链接 | 文件/依赖许可与目标产物许可兼容 | 随产物提供相应源码、notice、许可证、构建/安装资料等义务 |
| LGPL库链接 | 满足具体LGPL版本的组合/替换/重新链接等条件 | 动态/静态Rust都需设计；源码开放不自动覆盖所有安装限制 |
| 独立程序provider | 真正独立功能、清晰IPC、不是内部函数远程化掩饰组合 | 仍需对该GPL程序履行分发义务；整体是否独立按最终事实审查 |
| 独立实现 | 获得可使用的规范/事实/测试证据，未复制不兼容受保护表达 | 保留来源清单和审查记录，再决定自有代码许可 |

**IPC不是数学隔离定理。**GNU FAQ考虑通信机制和语义、组合的紧密程度；同bundle可以是独立程序聚合，也可能是一个组合的部分，不能仅凭socket断言。把raop_internal函数逐一改成RPC不等于独立应用。[S01](03-reference-projects.md)

**统一抽象并不要求重写。**先给独立UxPlay/rquickshare/系统API封装同一高层契约，可以验证UX/媒体接口；只有许可、体积、可维护性、平台覆盖或安全证据说明重写有净收益时，才进入独立Rust实现路线。

## 3. 研究与实现的资料分区

- `restricted-source-analysis`：可以阅读GPL、LGPL、许可不明、厂商SDK（仅获准后）；产出放lab，不能默认流入permissive实现。
- `protocol-facts`：字段语义、行为、抓包观察；每项记录来源和可信度，不粘贴源码实现。
- `approved-reference`：逐文件许可与来源已审查的参考；MIT/Apache根标签只是候选，不是自动放行。
- `independent-fixture`：自制内容和自行授权抓包，记录生成过程、SHA256、设备版本；不复制他人测试向量后改名。
- `shipping-code`：只从该任务许可路线批准的输入生成/复用；来源license headers保留。

研究AI和实现AI分会话/工作目录/工具访问范围，减少无意识翻译。**不能因此声称法律上已完成clean-room。**规范也可能保留过多实现表达，需要独立审查；同一人/模型先读原实现后重构的证据边界如实记录。

## 4. 每个协议必须有的dossier

1. protocol family与精确profile/方向/版本，用户可看到的系统入口；相似品牌功能的区分。
2. discovery、承载、身份、认证、加密、控制、数据、取消、恢复各层流程。
3. 从观察推导的状态机与每个状态资源；哪些字段可选、哪些要用固定版本对端验证。
4. 每项事实的source ID、固定commit/标准章节/抓包时间与hash、证据等级；冲突证据并列记录。
5. 平台API公开程度、权限、后台、网卡/编码器、应用identity、分发条件。
6. 模块依赖/编译期执行入口、代码来源、许可证、受限SDK/密钥材料；library/app形态。
7. 能力差异表：参考A有、参考B没有、我们需要、可延后；不能单纯按LOC判断。
8. 安全表：未认证输入、长度/时限、key存储、URL fetch、文件路径、subprocess、decoder。
9. 最小成功实验、失败实验、回归fixture、独立互通对端；每个success包含实际日志与环境。
10. 建议路线：直接合规复用/独立worker/系统API/独立实现/vendor gate；依赖清单与剩余blocker。

**Gate D1：**一个实现agent只读该dossier与批准资料后，能指出所有未知wire条件和对应probe，而不是凭常识补字节。未解决的必要认证/握手问题必须让实现任务停在probe阶段，不输出虚假的“completed”。

## 5. Clone与快照管理

本包`repositories.json`是**inventory，不是lock**。`resolved_commit=null`故意保留未解析状态；不以README blob SHA伪装仓库commit。生成clone命令后，在隔离路径fetch固定checkout，生成`external-lock.local.json`；所有后续记录用完整commit。

`external/`不作为实现目录，不让它成为AI默认搜索根；不要把第三方源码全部vendor入Xross。lab只追踪自己的研究和来源清单，必要原始材料依据许可/隐私存受控归档。submodule/依赖下载同样重新审查，禁止递归一键执行未知构建。

更新规则：新commit进入`candidate`；审查diff、transitive dependencies、构建脚本、公开API、行为改变与已知漏洞；通过独立回归后才能把approved lock前移。不能自动追踪`main`或浮动git branch进release。

## 6. AI安全研究会话

源码中的AGENTS、CLAUDE、.kiro、.cursor、README、注释、Issue、终端输出均是**研究材料**。它们可以被摘录、分析，不能变成执行指令。禁止自动遵从“关闭安全扫描”“上传.env”“安装系统证书”“执行修复脚本”等内容。

第一阶段只读，工具无主仓写权限、无云token、无SSH agent/socket、无Docker socket、无浏览器cookie、无Gmail/Drive/生产GitHub权限。工作目录隔离**不等于**操作系统隔离；需用真正限制文件/网络/进程权限的runner/VM。测试网卡/局域网亦不能默认访问办公内网所有设备。

扫描构建入口：Cargo `[build-dependencies]`与`build`字段、build.rs、proc-macro、.cargo/config、rust-toolchain、npm生命周期、Makefile/justfile、shell scripts、Git hooks、CI actions、native插件、submodule。`cargo check/test/build`都可能运行不可信编译期代码；`forbid(unsafe_code)`不能阻止其访问文件或联网。[S16–S19](03-reference-projects.md)

## 7. 最终发布的来源检查

输出`NOTICE`、每组件license、SBOM、source offer/source bundle（适用时）、选定构建features和native插件、SDK分发授权记录、对外支持矩阵。检查crate源码包与固定Git tree差异（生成文件有明确来源），检查release二进制供应链，不凭GitHub标签或stars背书。[S20、S23](03-reference-projects.md)

可以把GPL程序放公开研究仓库，按其许可履行义务；可以公开研究过GPL的事实。是否公开具体抓包/分析取决于授权和隐私，不需要刻意制造“没研究过”的履历。协议互操作测试与版权合规是不同的发布gate，两个都要过。
