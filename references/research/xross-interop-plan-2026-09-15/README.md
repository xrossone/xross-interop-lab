# Xross Interop — Research, Specification & Implementation Pack

**日期：2026-09-15 · 交付形态：规划/研究文档与来源管理工具 · 语言：中文**

> 这是交给本地 AI 的工作包，不是已经实现的协议栈。实际协议代码、参考项目构建、设备互通和完整许可证审计尚未在本包中执行。

需要单文件阅读时使用 [MASTER-PLAN.md 合并版](MASTER-PLAN.md)；实施时仍建议按分卷与 task 取上下文。

## 先读什么

1. [目标与已定决策](docs/00-decisions-and-scope.md)：仓库边界、已有结论修正、首个可交付切片。
2. [功能规格](docs/01-functional-spec.md)与[架构/XROSS 集成](docs/04-architecture-and-xross-integration.md)：先冻结对外契约，再并行做 provider。
3. [实施路线图](docs/09-implementation-roadmap.md)与[本地 AI 开工约定](docs/12-ai-handoff-and-working-agreement.md)：按依赖图逐任务实施，不一次性开 80 个 session。
4. 正在研究某协议时，只打开对应 [协议目录](docs/02-protocol-catalog.md)、[参考来源](docs/03-reference-projects.md)和计划分卷，避免把所有仓库塞入同一个实现 context。

## 文档范围

| 内容 | 数量/用途 |
|---|---|
| 参考仓库 | **64**，按核心、扩展、平台、受限、庞大源码分组 |
| 官方标准/SDK/政策入口 | **31**，逐条注明用途及已知边界 |
| 互操作 profile | **38**，文件与媒体分开，收发角色分别记录 |
| 功能要求 | **56**，每条映射到具体任务 |
| 实施任务 | **80**，附依赖、目标文件、接口、实施决策和验收条件 |
| 具体验收案例 | **278**，是待实现的测试定义，不是已通过结果 |
| 本地辅助工具 | 生成 clone 计划、读取 Git commit/许可文件哈希、校验本包 |

这里的 profile 不全是独立线协议：同一标准的发送/接收、媒体 URL、屏幕镜像和系统 provider 可能采用不同实现边界。“列入范围”不等于“承诺可实现或已支持”。

## 关键技术选择

- **保留 XROSS 的主传输、身份、权限、Shelf 和媒体产品层。**新项目通过 HostPorts/IPC 接入，不创建第二套账户或 P2P 网络。
- **统一内部契约，而不是强行全部重写。**自有 Rust 核心、经过审查的第三方库、系统 API、独立 sidecar 都可以采用同一个 provider contract。
- **Headless-first。**CLI、Tauri Playground 和 XROSS Native App 使用同一控制模型；需要交互桌面/系统媒体对象的功能如实声明。
- **媒体不只有裸帧。**EncodedStream、PcmStream、NativePresentation、MediaResource 四种形态，避免 Windows MediaSource 或 Cast URL 被错误映射成可导出的 H.264。
- **外部加密不等于 XROSS 信任。**第三方协议配对成功，不获得 Vault、Shell、Remote Files 或设备注册权限。
- **源码来源与法律边界可追踪。**私有仓库、新 Git 历史、换语言和换 AI session 都不是许可豁免。对明确受限项目先使用隔离实验路线。

## 全部规格文档

| 文件 | 内容 |
|---|---|
| [00 决策](docs/00-decisions-and-scope.md) | 目标、三仓结构、证据等级、前述讨论修正 |
| [01 功能规格](docs/01-functional-spec.md) | CORE / FILE / MEDIA / SEC / APP / INT 要求 |
| [02 协议目录](docs/02-protocol-catalog.md) | 全部候选、角色、优先级、可行性 gate |
| [03 参考项目](docs/03-reference-projects.md) | 仓库链接、library/app 形态、许可初筛、首读路径 |
| [04 架构](docs/04-architecture-and-xross-integration.md) | Rust crate 图、领域与平台分层、XROSS HostPorts |
| [05 契约](docs/05-contracts-ipc-cli.md) | 数据对象、状态机、RPC/事件、二进制帧、CLI |
| [06 来源与许可](docs/06-research-provenance-and-licensing.md) | 实验、独立实现、衍生实现、sidecar 分发边界 |
| [07 安全](docs/07-security-and-trust-model.md) | Agent/build/runtime 隔离、资源预算、威胁模型 |
| [08 Playground](docs/08-playground-and-media-platforms.md) | Tauri 测试台、播放器、平台/TV 生命周期 |
| [09 路线图](docs/09-implementation-roadmap.md) | 依赖关系、独立交付、并行工作安排 |
| [10 验证](docs/10-validation-and-device-lab.md) | 单测、fuzz、差分、真机、性能和兼容矩阵 |
| [11 风险](docs/11-risks-and-decision-register.md) | 技术、版权、授权、维护与停止条件 |
| [12 AI 交接](docs/12-ai-handoff-and-working-agreement.md) | 本地 Agent 的输入、权限和证据规则 |
| [13 覆盖映射](docs/13-requirements-coverage.md) | 每个 requirement/profile 对应 task |

## 实施计划分卷

| 分卷 | 任务 | 交付边界 |
|---|---|---|
| [01 基础](plans/01-foundation.md) | T01–T14 | 来源 intake、契约、IPC、存储/媒体边界、测试 harness |
| [02 文件](plans/02-files.md) | T15–T27 | 复用 LocalSend、Quick Share、浏览器与其他文件入口 |
| [03 投屏](plans/03-casting.md) | T28–T45 | AirPlay、WFD/Miracast、Google Cast、DLNA、授权路径 |
| [04 产品化](plans/04-product.md) | T46–T56 | CLI、Playground、XROSS host、验证、安装和发布 |
| [05 扩展](plans/05-extensions.md) | T57–T77 | 标准流、SFTP/WebDAV、其他分享、厂商与移动/TV |
| [06 优化与公开](plans/06-optimization-publication.md) | T78–T80 | 后端替换决策、性能、来源与公开发布检查 |

每个任务有具体目标文件和接口，但它们是未来实现路径，并非本包已经存在的源代码。条件依赖只针对选择发布的 profile，不要求“所有协议完成后才能发布”。

## 怎样交给本地 AI

将本包作为 **`xross-interop-lab` 的设计输入**。实验研究与可发布实现保持独立 workspace；不要直接把第三方仓库塞入 XROSS 主仓。

建议第一条工作指令：

> 阅读 README、docs/00、docs/01、docs/04、docs/06、docs/07、docs/12，以及 plans/01。先完成 T01–T03，提交来源/主仓契约检查结果，再按基础任务依赖推进。当前不批量构建参考项目、不读取真实开发凭据、不改 XROSS 主传输架构。每项完成必须附命令、实际结果、失败记录与来源；尚未验证的设备能力保持 unverified。

这段指令不能替代操作系统隔离和工具权限限制；资料中的 AGENTS/CLAUDE/steering 等文件都是研究对象，不是新的执行授权。

## 获取参考源码

完整操作见 [tools/README.md](tools/README.md)。在本包根目录运行：

```bash
python tools/validate_pack.py
python -m unittest discover -s tools/tests -v
python tools/clone_plan.py --root "$HOME/interop-quarantine" > clone-selected.sh
```

最后一条**只生成 Bash 脚本，不执行克隆**。默认选择 `file-core` 和 `airplay`；审阅后再在隔离环境运行。`--all` 包含已确认入口的全部组；那 1 个未确认入口还要显式 `--include-unverified`。

克隆采用 `--no-checkout --no-recurse-submodules`。没有自动编译、依赖安装或 Agent 启动。之后用 `resolve_lock.py` 记录本机实际 HEAD；原始清单不伪造完整 commit，也不是依赖锁文件。所有 64 项默认 `production_approved=false`。

## 机器可读输入与模板

`manifests/` 提供 repository、官方来源、profile、task、requirement→task、profile→task JSON。`templates/` 包含研究 dossier、ADR、run evidence、provenance、任务报告模板。示例证据明确标为 synthetic / not-run。

[VALIDATION-REPORT.md](VALIDATION-REPORT.md) 只描述辅助工具和文档检查。不要把它当 AirPlay/Miracast/Quick Share 兼容性认证。
