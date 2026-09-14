# 功能规格：Xross Interop v0.x

**性质：以下 MUST/默认值是本项目设计要求，不是外部协议标准的原文。**外部 wire 细节由固定来源 dossier 决定，不得把这些内部字段伪装成 AirPlay/Quick Share 标准。

## 1. 角色与主要场景

- **收件者**：只在 Mac/Windows/Linux/受支持 TV 上运行 Xross，发送方使用系统原生分享或投屏入口。
- **发送者**：在 Xross 选择文件/窗口/音源和附近目标，路由到双方共同支持且允许的 profile。
- **Headless 管理者**：通过 CLI 查看待处理请求，配置有限自动接收规则，把内容写入指定存储或转到经授权输出。
- **集成开发者**：用 Rust library 或本地 IPC 接入自己的 UI/播放器；无 Xross 账号也能运行。
- **研究者**：在隔离 lab 复现实验、抓包、编写协议事实和一致性测试，不把试验结论当产品能力。

## 2. 核心需求（CORE）

| ID | 要求 | 可验证结果 |
|---|---|---|
| CORE-01 | 库、daemon、CLI 共用领域契约 | 同一个 fixture 经 in-process 与 IPC 得到相同状态投影 |
| CORE-02 | 每个 profile 分开记录 sender/receiver/control/server/client 角色 | `capabilities` 不用单个 `supports_airplay=true` |
| CORE-03 | 能力包含平台、最低已验证版本、权限、媒体形态、来源/测试证据 | 缺少证据时返回 `unverified`，不显示绿色支持 |
| CORE-04 | 发现只产生 scoped EndpointObservation | 同名/同 IP 不自动合并为可信 Xross Device |
| CORE-05 | 协议路由只选择满足用途与安全要求的能力 | 收发方向错误、降级加密、缺 codec 不静默 fallback |
| CORE-06 | 所有异步任务可取消、可查询，重启有明确恢复语义 | 重复取消幂等；连接断开不留下无限 pending task |
| CORE-07 | 事件有递增序号、快照和 replay-gap 标记 | 客户端断连重订阅可以发现缺口并刷新快照 |
| CORE-08 | 纯 core 不访问账号/全盘/进程启动/平台 UI | crate graph 检查阻断禁止依赖 |
| CORE-09 | 平台 probe 只检查并报告，不自动改系统配置 | 缺无线显示组件/权限时只返回 remedy，不运行 sudo |
| CORE-10 | 所有 provider 失败独立降级 | WFD 网卡异常不影响 LocalSend 文件接收 |

## 3. 文件分享需求（FILE）

| ID | 要求 | 验收边界 |
|---|---|---|
| FILE-01 | 文件、多个文件、文本、URL；目录按协议能力显式映射 | 不支持目录时提示“打包后发送”并让用户选择 |
| FILE-02 | offer 先有元信息、后授权、再接收内容 | 未批准不能 materialize 到最终路径；允许必要但有上限的前置网络缓存 |
| FILE-03 | 按会话和文件显示进度/速度/失败原因 | 对端只给粗进度就标粗粒度，不能伪造精确完成 |
| FILE-04 | 可选部分接受、取消、重复请求去重 | 协议不支持部分接受时整个 offer 决策，不伪造双方一致 |
| FILE-05 | 安全目录落盘、hash、原子发布 | 路径穿越/符号链接/同名覆盖/大小谎报失败；不执行接收内容 |
| FILE-06 | 区分接收完成、本地持久化、远端确认、integrity verified | 文件只有本地 SHA256 时不声称对端提供端到端 hash |
| FILE-07 | 明确支持或拒绝断点续传 | 无协议续传则重启新 attempt，保留用户选择的重试策略 |
| FILE-08 | 只通过 scoped lease 读取发送内容 | provider 不拿源文件任意路径，也不扫 Downloads/SSH 目录 |
| FILE-09 | headless 人工审批与有限规则 | 默认无客户端审批则 60 秒超时拒绝；显式 `accept` 后继续 |
| FILE-10 | Xross native / LocalSend / 新协议共享产品状态 | 不生成第二套历史、下载中心和 Shelf 内容 ID |

## 4. 媒体与投屏需求（MEDIA）

| ID | 要求 | 验收边界 |
|---|---|---|
| MEDIA-01 | 镜像、音频、媒体 URL 是不同 SessionIntent | URL cast 不能作为完整 screen mirror 成功 |
| MEDIA-02 | 支持 encoded packets、PCM、native media source、URL 四类 output | 原生媒体源只保证可呈现时不显示“录制/转发可用” |
| MEDIA-03 | 音视频分轨，保留 clock domain、timebase、PTS/DTS | 缺 DTS 为 `null`；重新连接/旋转显式 discontinuity |
| MEDIA-04 | null/file/native player/standard stream sink 可选择 | headless 缺声卡也可接收/保存；屏幕接收不强制打开窗口 |
| MEDIA-05 | 首批只保证明确的 H.264 + 已验证音频组合 | HEVC/HDR/4K/多声道逐 codec/profile/设备证明，不 blanket advertise |
| MEDIA-06 | 播放、暂停、音量、seek、stop、反控按能力授权 | live mirror 无 seek 就禁用；UIBC/鼠标输入是独立权限 |
| MEDIA-07 | 有界缓冲、背压与合法丢帧策略 | 不丢随机 H264 delta 包后继续当正常帧；请求/等待关键帧恢复 |
| MEDIA-08 | 录制/截图/跨设备转发显式同意和合法能力检查 | 禁止后台静默录制、DRM/HDCP 绕过 |
| MEDIA-09 | 播放 URL 通过 SSRF/授权策略，凭据不跨域泄露 | 重定向、DNS 重绑定、内网地址、HLS 子资源均再检查 |
| MEDIA-10 | 多会话策略清晰、不能抢占默认音频输出 | 初期 1 个 active render session；另一个请求提示忙或用户切换 |

## 5. 安全与来源需求（SEC）

| ID | 要求 | 验收边界 |
|---|---|---|
| SEC-01 | 外部设备默认不可信，第三方加密不升级 Xross 身份 | Quick Share pair 成功后仍不能访问 Shell/Vault |
| SEC-02 | 构建/运行/AI 研究三种隔离 | 第三方指令、build.rs/proc macros、安装脚本不能接触真实开发凭据 |
| SEC-03 | 协议解析前配置资源上限与绝对 deadline | slowloris、长度溢出、重复 Content-Length、压缩炸弹受控拒绝 |
| SEC-04 | workers 按威胁/许可边界隔离 | 无 sandbox 的 worker 显示 `isolation=process-only`，不能宣传沙箱安全 |
| SEC-05 | 本地 IPC 有 peer/令牌验证、scope、速率限制 | 浏览器网页或别的无授权进程不能批准/读取文件 |
| SEC-06 | 所有引用来源固定 commit/许可/使用方式 | AI 翻译/文档复制/测试向量也必须有 provenance |
| SEC-07 | 依赖与二进制产物可追踪且更新经审查 | 新 dependency/feature/native plugin 默认阻断发布 |
| SEC-08 | 日志/抓包/诊断脱敏 | key、token、PIN、Authorization、真实内容不出现在正常诊断包 |
| SEC-09 | 网络 profile 显式启用，可见范围可选 | 不默认监听所有公网/VPN接口；无线资源冲突可解释 |
| SEC-10 | 私有研究与公开发布不混同 | 公布前审查版权、第三方许可、商标/认证、隐私；无“改 Git 即干净” |

## 6. CLI / Playground / 运维需求（APP）

| ID | 要求 | 验收边界 |
|---|---|---|
| APP-01 | CLI 支持 JSON 查询、watch、accept/reject、send、cast、doctor | 非交互调用不用解析彩色日志；stderr 仅诊断 |
| APP-02 | Tauri 是测试台，不是协议宿主权威 | 关闭窗口不影响已授权 headless 文件任务；媒体按显式 close policy |
| APP-03 | discovery、offers、session、trace 四个核心视图 | 状态均来自 daemon 事件，不由前端推测网络状态 |
| APP-04 | 无 shell wildcard 和远程内容 privileged WebView | CSP 本地、Tauri capability 最小，外部 URL 不注入主页面 |
| APP-05 | 每个运行输出证据 manifest | commit/config/device/build/test versions + result + hashes |
| APP-06 | 可部署/升级/回滚，不遗留监听/系统改动 | worker 崩溃有限重启；不重写配对数据 schema 至无法回滚 |
| APP-07 | 安装包 feature/plugin 白名单、许可 notice/SBOM | 禁用 profile 不带入对应 GPL/厂商 SDK 到不合适产物 |
| APP-08 | 自动化测试与真机流程分开 | CI pass 不等于 Pixel/Samsung/Huawei/TV 已通过 |

## 7. XROSS 集成需求（INT）

| ID | 要求 | 验收边界 |
|---|---|---|
| INT-01 | 先审查主仓真实公共契约，再写 adapter | 本包的 proposed ports 不被冒称为当前已存在 API |
| INT-02 | 保留唯一 Xross 设备身份/账号/Fabric endpoint | standalone mode 不能在 integrated mode 自动再次启动 Iroh |
| INT-03 | 文件进入现有 Offer/Transfer/Shelf 的合适位置 | 只有内容 offer 投影，媒体/终端/设备状态不硬塞 Shelf |
| INT-04 | 现有 LocalSend 优先复用，迁移有回归门槛 | 旧双向互通不能因抽象统一退化 |
| INT-05 | 媒体 provider 和 Media Graph 解耦 | native-only source 能显示，不强行导出帧或重复录屏 |
| INT-06 | API版本、mixed-version、取消/重启保持兼容 | daemon N 与UI N−1 支持范围有测试，不能忽略未知枚举 |
| INT-07 | bridge 是显式新会话，重新授权目标 | source→bridge→target不是无条件端到端加密；不继承好友权限 |
| INT-08 | 主产品发布不被全部候选阻塞 | profile feature gates；实验/受限实现可不打包 |

## 8. 初始运行默认值

以下是**可配置产品资源预算**，不是声称兼容协议的固定参数。平台真机证明后可调，但必须通过测试修改。

- 外部 discovery 默认 off；首启 UI/CLI 选择接口与 profile 后开启。daemon control 从不监听公网。
- 1 个 active video render，4 个 concurrent file transfers，4 个 pending offers；超额回复 busy 而不是无限排队。
- offer 元信息最大 1 MiB、最多 10,000 entries，同时应用两项限制；超出需显式较高预算或拒绝。
- 交互审批 deadline 60 秒；协议要求更短时用其上限，且 UI 显示实际 deadline。
- IPC control frame 最大 256 KiB；单条媒体 data frame 最大 16 MiB；每个 worker 总队列默认 64 MiB，达到限制背压/关闭对应流。
- 未认证会话 handshake 总预算默认 15 秒；**不照搬到每个协议**，其合法流可能需要更长时要 dossier + 测试说明。空闲/请求/上传分别计时。
- 文件大小以可信 host quota/剩余磁盘/批准预算控制，不以平台 memory 配额代替。使用流式写入，禁止全文件读入内存。
- 不自动接受归档/目录解压；保留为文件。需要解包时独立 sandbox、文件数/总展开大小/深度预算。
- 自动重启 worker 最多 3 次/5分钟，指数退避；超过转故障，不无限 crash loop。
- headless 只启用 null/file sink；播放设备选择必须显式，音量不超过 host 当前允许上限。

## 9. 核心场景验收

**U1 外部文件进入 Xross**：Android 系统 Quick Share 发送 3 个文件 → provider 只报告 offer → Xross UI 接受其中支持的选择范围 → ContentSinkLease 写临时区 → 完整验证后发布 → 历史只记一个 transfer；拒绝后没有最终文件。

**U2 iPhone 镜像到桌面**：用户开启 receiver → iPhone 原生镜像发现 → 用户确认/PIN → compressed 或 native source →播放器呈现 →旋转产生 format change → stop 回收全部缓冲。用户已测 UxPlay 的现象应被重现为记录完整的 lab run。

**U3 Xross 发送给非 Xross 电视**：选择 Media URL/本地文件 → DMC/Cast URL sender 筛选支持的 sink → 建有限资源 URL lease → TV 拉取 → stop 撤销令牌。找不到 codec 不无声转码，显示选择转码或换设备。

**U4 无界面 node**：进程无 DISPLAY、无默认声卡 →接收 LocalSend/Quick Share →第二个 CLI 审批 →保存文件；AirPlay 音视频可保存/丢弃供验证，而不因创建窗口失败崩溃。

**U5 平台受阻**：Mac 请求 WFD receiver，probe 没有受支持实现 →返回 `platform-unavailable` 与说明。不能仅显示在线再永久加载，也不能悄悄执行 root helper。
