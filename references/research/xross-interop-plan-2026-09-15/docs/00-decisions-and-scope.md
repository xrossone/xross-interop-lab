# Xross Interop：目标、边界与已定决策

**文档日期：2026-09-15。状态：可执行的研究/开发设计，不是已实现功能清单。**

## 1. 用户目标

在目标设备运行一个可嵌入、可无界面运行的互操作层，使系统原生或已存在的软件入口可以向它分享文件、音频、媒体 URL、屏幕；同时让它尽可能作为发送方接入其他设备。近期重点是手机/电脑 → 电视/电脑；Android/鸿蒙平板接收是逐平台机会，不是所有任务的先决条件。

产品必须同时支持三种形态：独立 CLI/headless 节点；Tauri Playground 测试台；XROSS 正式客户端/daemon 的互操作 provider。研究工作独立于当前 XROSS 的其他并行开发。

“尽可能多”落实为**完整候选目录 + 每个方向的可行性结论 + 可逐个交付的 provider**，不是先承诺全部兼容。品牌词不是协议：Smart View 不另建一套重复 WFD 栈，Huawei Share 与 Quick Share 不合并，Google Cast sender 和 receiver 分别排期。

## 2. 三个仓库/工作区的职责

| 名称 | 默认可见性 | 存放什么 | 禁止什么 |
|---|---|---|---|
| `xross-interop-lab` | private | 来源清单、研究笔记、设备记录、原始/脱敏抓包、实验 POC、第三方 provider wrapper | 不将所有内容误标 MIT；不存真实账户密钥/私有用户文件 |
| `xross-interop` | private 起步，可经审核后公开 | 自有契约/核心/经过来源审查的协议模块、CLI、SDK、测试与 Playground | 不承接未经审核的 GPL 语义翻译、受限 SDK、未清理敏感抓包 |
| `xross-dev` | 当前产品仓库 | XROSS 身份、设备、账号、Shelf、传输/媒体产品、Native UI 与集成 adapter | 不复制整个 lab，不新增第二套账户/设备网络/主传输数据库 |

这是一种管理和依赖边界，**不是“把研究藏起来就没有版权问题”**。即使研究仓库私有、新实现没有旧 Git history，复制或衍生的表达也不会自动消失。保留完整且真实的来源记录；公开范围由隐私、许可和商业需要决定，不以掩盖来源为目的。[S01–S03](03-reference-projects.md)

可以把 `xross-interop` 未来并入 Xross 公共 monorepo，也可以保持独立库；本计划不要求现在就确定 public GitHub 组织、商标或最终 crate 名。只依赖稳定 API/版本，不用人工反复复制源码到主仓。

## 3. 已冻结的架构选择

1. **Headless-first，但不假装所有平台 API 都 headless。**网络接收/文件操作可以后台运行；音视频播放、屏幕捕获、Windows 原生 MediaSource 可能需要用户会话或 UI owner。
2. **统一领域对象，不统一外部线协议。**内部统一 ExternalEndpoint、Offer、Transfer、MediaSession、Track、Grant；AirPlay、WFD、Quick Share 各有独立状态机和验证器。
3. **统一 API 不要求统一代码语言。**系统 WinRT provider、合法第三方 sidecar、自有 Rust provider 都可以实现同一契约。纯 Rust 是自有协议核心的目标，不是重写 GStreamer/硬解码器/成熟密码原语的理由。
4. **统一 API 不要求统一进程。**许可边界、安全边界、UI线程/硬件边界分别决定部署；可能一个 crate 对应 worker，也可能多个独立库共用一个经过审查的 worker。
5. **控制面与数据面分开。**JSON-RPC 只传状态/授权/句柄引用；文件块和音视频不走 WebView base64 事件。
6. **接收与发送独立声明。**收到/发现 Chromecast 不表示能成为 Chromecast；能接收 AirPlay 镜像不表示能把桌面作为 AirPlay source 发出。
7. **能力分层。**有源码、能编译、模拟器对通、真机发现、真机正常传输、长期稳定、可合法分发是不同 gate。
8. **XROSS 是 host，而不是该库的必备依赖。**库不登录 WorkOS，不持有 Vault，不创建 Iroh 网络；集成 host 通过 ports 提供既有能力。
9. **广播内容最小化。**默认不广播账号邮箱、好友关系、文件目录、设备公钥或原始 hostname；让用户选择可见名称。
10. **安全默认关闭自动接收、跨协议桥接、输入反控和动态下载执行。**用户显式开启某个 profile 才启动对应监听。

## 4. 证据等级

- `catalogued`：已记录可研究入口。
- `source-reviewed`：固定 commit 的相关代码已审查；仍可能不能运行。
- `build-verified`：指定平台构建完成，并保留日志/哈希。
- `simulated`：自有/参考程序模拟对端成功。
- `device-verified`：与具名型号、OS、App 版本真机互通。
- `release-qualified`：安全、许可、安装升级、性能、混合版本及维护责任通过验收。
- `blocked`：说明证据、障碍及重新评估条件；不是悄悄放空。

本包唯一来自用户的运行事实：**本地 UxPlay 已成功接收 iPhone 镜像并打开窗口**。没有用户给出的具体 commit/手机版本，先记录 `user-observed/unpinned`，不能代替可复现测试。其他项目的 README 声明不升级成 Xross 真机证据。

## 5. 与前面讨论相比必须修正的点

- MiracleCast README 明确写 Display-Source 尚未实现；采用它做 sink 参考，sender 看 GNOME Network Displays/OpenHarmony/AOSP 等独立路径。[R32–R37](03-reference-projects.md)
- Windows Miracast API 公开 `MediaSourceCreated`，先验收系统媒体源播放；原始帧/转发/录制单独 probe，不保证可得。[S04–S06](03-reference-projects.md)
- Google **Open Screen/libcast** 确实包含开源 sender/receiver/streaming 组件。真正未知的是 stock sender 对通用 receiver 的认证与兼容，不是“没有开源 receiver 代码”。[R42–R45](03-reference-projects.md)
- LocalSend 默认使用 UDP multicast/HTTP discovery，不应被统一实现误改为只有 mDNS。[R13](03-reference-projects.md)
- `rquickshare` 有 `core_lib` 和独立 `core_bin`；可作为 library 的技术结构存在，但 GPL 不因此消失。[R19](03-reference-projects.md)
- OpenHarmony 系统源码存在不证明普通 HarmonyOS 第三方 App 有同样权限；Huawei 旧 EMUI SDK 文档也不证明所有新鸿蒙兼容。[R37–R40、S10–S13](03-reference-projects.md)
- “看过 GPL + 换语言 + 新 session + 新 Git repo”不是自动 clean-room 认证；“用了 socket”也不是自动许可隔离认证。

## 6. 首个有用版本与完整愿景

**首个可演示切片：**CLI 能查询 provider、接收/拒绝一份文件；复用现有 LocalSend 双向互通；外部 UxPlay provider 能把镜像送到受控播放器；Tauri 只消费相同控制 API；主 XROSS 用 mock host adapter 通过至少一条契约回归。

**第二个真正差异化切片：**Android 原生 Quick Share → Xross/macOS，先 LAN/二维码可发现；接收后进入现有 Xross Transfer/Shelf 产品流程，发送方向独立验收。

**完整愿景：**协议目录中的每个候选都得到可执行实现或可复查的 blocker dossier；允许协议持续扩展，不让一个厂商认证障碍阻塞其余可交付能力。

## 7. 本计划不承诺的事情

不承诺完整 Apple TV/Chromecast 替代；不承诺 DRM/HDCP 受保护内容录制；不承诺所有网卡/ROM 均支持 WFD；不承诺 Windows 服务会话可直接展示 UI；不承诺更少 LOC 自动更快或更安全；不使用未经授权的设备认证材料。所有商用兼容宣传必须来自 `release-qualified` 矩阵。
