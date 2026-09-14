# Tauri Playground、headless运行与媒体平台

## 1. Playground的职责

Playground不是另一个产品主app，不拥有账号/设备信任/协议实现。它是可拆卸测试consumer：启动或连接interopd，显示发现/offer/媒体会话和诊断，帮助真机验证。建议Tauri 2 + Svelte + TypeScript，版本在首次实现时锁定；工具链不是本包虚构的最新版本号。

四个导航域：**Devices**（按profile/role/接口筛选，不乱合并身份）、**Transfers**（待审批/进行中/历史）、**Media**（会话/预览/输出）、**Diagnostics**（probe/事件/证据）。研究人员可展开wire phase，但普通模式只显示可理解阶段。

## 2. 文件流程UI

incoming卡片显示protocol、未验证友好名称、文件数/声明大小、信任状态、审批剩余时间、目标根目录及风险提示。显示名采用文本节点；emoji/control/RTL处理避免覆盖真实来源。接受/拒绝发RPC intent，最终状态由daemon事件驱动。

多文件中断不得显示整个批次成功；各entry单独状态、aggregate明确partial。取消按钮可重复点击且只发幂等请求。outgoing picker由native side创建ContentReadLease；frontend不传任意本机路径指挥worker读取。

## 3. 媒体预览实现选择

**POC首选：**被审查的GStreamer/native backend在独立窗口播放。它避免WebView codec差异，也能验证真实音画同步。Tauri窗口只显示session控制与stats。此时UX标`external native preview`，不是嵌入完成。

**可嵌入阶段：**若平台安全支持共享texture/原生子view且生命周期通过测试，可提供native-view sink。跨平台不硬承诺一种handle。macOS CoreMedia/VideoToolbox/AVFoundation、Windows Media Foundation/WinRT media、Linux GStreamer/PipeWire/桌面surface分别adapter。

**WebView预览：**使用本地WebRTC或适合浏览器的MSE/URL，仅在codec/低延迟预算符合时。不要每帧toDataURL或IPC base64；不要为预览先H264→JPEG→JS→Canvas高成本重复转换。HLS适合媒体播放/兼容性演示，不作为低延迟screen的默认基线。

**Windows Miracast：**`MediaSourceCreated`产生owner持有的source。先用合法线程/apartment/用户会话创建独立player验证；raw extraction、recording、forwarding分别probe，失败只保留presentation能力。[S04–S05](03-reference-projects.md)

## 4. 音频

音频sink明确选择：null、PCM文件、系统输出、指定设备、未来Xross audio graph。描述sample rate/sample format/channels/layout，native device实际格式可能不同。resampling只在明确转换节点发生，保留原始clock mapping。设备拔插/默认设备切换不重新解释旧PCM数据。

默认测试不把所有声音自动路由到系统高音量；第一次play需用户选择。多sender混音是后续功能，需要gain/headroom/limiter及每源静音策略，不在接收初版偷偷实现。

## 5. Screen source / sender

sender端的捕获是一层独立权限：Windows capture、macOS ScreenCaptureKit/系统授权、Linux Wayland portal/PipeWire、Android MediaProjection、HarmonyOS对应公开API。具体版本/API在platform probe文档固定。

第一轮sender使用自有test-pattern+合成音频，先证明协议，不把屏幕capture问题混入握手调试。通过后再加monitor/window capture、系统音频；用户停止共享必须同时终止捕获和对端stream。

## 6. Headless含义

`interopd --no-ui`必须可以管理文件、发现和网络媒体轨，不初始化声卡/显示。需要播放时可由同用户图形会话中的`presentation-worker`接管；服务器端用null/file/standard stream exporter。

**不保证所有协议在无用户会话运行。**Windows系统Miracast receiver和需要蓝牙授权/屏幕capture的路径可能不支持service/session0。报告`requires-interactive-session`，不要自动以高权限会话规避。手机端是embedding/system lifecycle模式，不要求spawn桌面sidecar。

## 7. 可测量的界面指标

显示discovery time、pairing/consent time、first-media time、输入输出codec、dimensions/fps、bytes、queue age/depth、dropped packets/frames、discontinuities、A/V offset、transport security、source exportability、actual provider/build hash。

延迟必须区分网络RTT、buffer age、decode time、present time和实测端到端。无法量到端到端就不显示假精度“延迟12ms”。用同屏计时器/相机自制图样进行对照，记录测量方法。

## 8. Tauri测试

mock daemon事件驱动UI测试；恶意名称按纯文本显示；旧event sequence/gap刷新；worker退出只对应卡片失败；窗口关闭后的文件继续或取消按照显式选项；媒体默认停止capture和播放，用户明确选择后台播放则按平台许可继续。

打包不同OS/arch sidecar名称符合Tauri target配置；签名和版本独立核对。frontend没有通用shell、任意fs、任意HTTP代理；native Rust也不是无条件可信，须在发布包的dependency/worker审查中覆盖。[S18–S19](03-reference-projects.md)
