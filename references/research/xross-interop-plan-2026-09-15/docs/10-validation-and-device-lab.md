# 验收体系与真机实验室

## 1. 五层验证

| 层 | 测什么 | 不能证明什么 |
|---|---|---|
| L0 静态来源/构建 | manifest、许可、依赖、编译入口、固定版本 | 不能证明没有恶意代码或协议正确 |
| L1 单元/属性/fuzz | codec、parser、state、size/time bounds、grant | 不能证明真机发现/驱动/商店可用 |
| L2 仿真/参考对端 | 正常/异常会话、文件字节、时钟、控制 | 自有两端同时犯错仍可对通 |
| L3 真机 | 指定型号/OS/ROM/App、网卡、网络、用户动作 | 一台设备通过不代表所有同品牌设备 |
| L4 发布qualification | 安全+许可+打包+升级+性能+产品回归 | 有范围的工程验收，不是绝对安全证明 |

任何测试结果必须引用source commit和config。库“production ready”、coverage70%、大量stars或CI绿灯都不是Xross自己的L3/L4证据。

## 2. 最小设备矩阵

| 角色 | 最少准备 | 记录项 |
|---|---|---|
| Apple sender | 用户现有iPhone；可用时加iPad/Mac | 型号、完整OS build、所用系统入口/应用版本 |
| Android sender/receiver | 一台有Google Quick Share的设备；一台Smart View/Miracast设备 | Google服务存在性、ROM区域、OneUI/系统版本、可见模式 |
| China alliance | 两个不同品牌中国ROM设备，仅执行MDFE任务时需要 | 入口品牌、联盟版本、蓝牙/WiFi状态 |
| Huawei | 用户可用鸿蒙手机/平板；新旧系统分别记录 | HarmonyOS/EMUI精确版本，不能统称“华为已支持” |
| Desktop receiver | macOS、Windows11、Linux各一 | 架构、GPU/codec、网卡/驱动、图形/无界面会话 |
| Miracast sink | 一个已知可用TV/显示器与Windows原生接收基线 | 型号/固件/P2P能力/HDCP模式（不绕过） |
| Cast/DLNA TV | 合法Chromecast/GoogleTV与DLNA renderer | app ID/认证条件/支持格式/实际URL访问路径 |
| 无线实验 | 可独立于工作网络使用的测试网卡/路由 | subnet/VLAN、是否client isolation、多播是否转发、频段/频道 |

这些是测试条件，不是让用户立即购买全部设备。缺设备则保留unverified，不使用模拟结果替代。

## 3. 文件验收数据集

全部自制、不含用户秘密：0byte文件、1byte、二进制全字节模式、4KiB边界、1MiB、100MiB、桌面10GiB流式文件；一批1000个小文件；Unicode/emoji/组合字符/RTL显示、同名及大小写冲突。按平台能力/磁盘预算选择实际规模，并在发布profile列清。

每个文件测试：send→approve→transfer→commit→hash；拒绝、取消、超时、远端退出、接收进程kill、系统重启、磁盘满、只读目录、目标文件已有、源文件变更、反复重试。对不支持resume的协议，预期是明确失败+新attempt，而不是把“不能续传”记成代码bug再伪造能力。

目录默认不自动解包。归档测试使用嵌套、软链接、压缩炸弹的**合成安全fixture**，仅用于验证拒绝/预算，不对真实系统路径做攻击演示。

## 4. 媒体验收数据集

自制test-pattern含递增frame counter和可见时钟；合成音轨含已知间隔脉冲用于音画对齐。初始720p/1080p、30/60fps按真实codec协商范围；44100/48000Hz mono/stereo。HEVC/HDR/4K/5.1/7.1是额外profile，不用一个H264成功结果覆盖。

每轮分开测：discovery、连接、用户确认、first video、first audio、A/V offset、rotation/resize、暂停（若有）、音量、stop、对端断网、worker kill、睡眠唤醒、网络切换、20次重连、30分钟连续播放。对headless做无DISPLAY/无声卡的null/file模式。

测量端到端必须记录方法：同屏可见计数器拍摄、硬件采集或对时测试，而不是把RTT当视频延迟。reference A/B须同手机/网络/codec/输出模式；两套缓冲策略不同则注明不可直接比较。

## 5. 安全负向矩阵

| 类别 | 必测输入/情景 | 硬性断言 |
|---|---|---|
| 控制framing | 0长度、超长、负/溢出、非法UTF8、分片粘包 | 分配前拒绝；无panic、无限等待 |
| 认证 | 错PIN、错确认码、重放、cross-session token | 不发送/落盘、不升级身份 |
| 文件 | traversal、UNC、symlink race、重复块、伪大小 | 无授权root外读写，无覆盖 |
| 媒体 | 巨大dimensions、坏codec config、坏PTS、乱序/丢包 | bounded资源、合法discontinuity/结束 |
| URL | file/UNC、自跳转、跨域凭据、DNS变换、HLS子URL | 每个请求按scope重验，秘密不跨域 |
| 发现 | 同名伪设备、重复announce、多个网卡反射 | 不合并可信身份，不广播风暴 |
| 本地IPC | 未认证client、错scope、媒体channel token复用 | 无管理/文件/媒体越权 |
| Worker | 读fake SSH canary、任意shell、禁网目的地、crash loop | OS隔离真实可测，否则明确process-only |
| UI | HTML/JS/ANSI/RTL日志注入、重复event/gap | 无代码执行/伪审批、状态可恢复 |
| 来源 | 新build.rs/proc macro/git依赖、产物hash变化 | release gate阻断并复核 |

## 6. 性能预算的决定方式

功能正确与权限边界是硬性；性能值先测量再冻结到发布硬件profile，不按代码行数推断。

初始优化目标（本项目目标，不是已达到结果）：文件进程内存不随文件总大小线性增长；队列遵守64MiB默认budget；媒体不以无界buffer换取“无丢帧”；多次重连后session/FD/端口回到稳定基线；同类reference条件下，adapter/IPC新增延迟单独测量。若要冻结数值SLO，将设备/codec/网络/测量方式与阈值一起写release profile，不发布孤立的“低于50ms”。

音频buffered profile可能本来高延迟，不能和interactive mirroring放同一条KPI。高分辨率/转码开销必须UI可见；设备无硬解码时能诚实降级或拒绝。

## 7. 真机执行表模板

1. 固定本地实现与参考commit、二进制hash、OS/app版本、配置与网络图。
2. 先用库存客户端/系统receiver做原生对原生基线，证明测试环境不是坏的。
3. 换成Xross provider，只改变一个变量；记录每个阶段，不只“works”。
4. 跑正常和对应负向/取消/重连用例；不拿真实个人文件/凭据做测试。
5. 导出脱敏run manifest、日志hash、样例输出hash、测量CSV；raw captures受控保存。
6. 结果分pass/fail/blocked/not-run；fail说明最早不符合阶段，blocked说明缺少的权限/硬件/授权。
7. 复测只能新增run，不覆写旧证据；稳定同profile多轮通过后才提升成熟度。

## 8. Release checklist

- [ ] 当前release的profile/role/platform范围显式列出。
- [ ] 全部normal与security-negative tests已执行，失败无隐瞒。
- [ ] 至少一套库存真实对端互通，具体版本清楚。
- [ ] 文件与secret权限、sandbox实际等级和URL策略通过。
- [ ] 许可/SDK/源码/notice/编解码配置审查通过。
- [ ] CLI、Tauri和Xross真实host表现一致，旧功能无回归。
- [ ] 安装/卸载/升级/回滚/worker hash校验完成。
- [ ] 用户能看到限制、需要权限、无resume/无raw-export等事实。
- [ ] 维护人/故障报告/依赖升级和漏洞修复入口明确。
