# 安全模型与强制防线

## 1. 攻击者与资产

攻击者包括局域网未配对设备、已配对但恶意设备、恶意媒体URL服务器、投毒源码/依赖/工具输出、同机非授权客户端、恶意或被攻陷的provider。不能因为LAN/蓝牙靠近就视为可信。

需要保护：Xross的Vault和SSH凭据、设备身份密钥、未授权文件、屏幕/音频隐私、用户批准的目标、网络设置、主app可用性、构建/签名/云服务凭据。Interop自己的协议配对key与Xross Device Identity严格分开。

## 2. 权限图

- observer：读取最小能力/发现状态，无原始文件、无PIN、无approve。
- interactive-client：只能对自己被授权的scope提出/批准指定offer/media请求。
- provider：只使用当前session grants；不能调用管理账户、扫描目录、任意网络fetch或输入控制。
- platform-broker：极小特权面，按已定义operation处理network/radio/native capability；拒绝任意命令。
- host-authority：Xross现有权威或standalone本地策略，是grant的唯一来源。

grant绑定client/provider/session/entry/direction/目的/总字节/有效期/允许操作。复制token到另一session、从receive转send、从media转terminal全部失败。接收方可见名称只作UI文本。

## 3. 文件路径与发布

provider提交名称数组但不指定可信absolute destination。storage层逐component处理：拒绝空段、`.`、`..`、绝对/UNC/drive路径、NUL/control、路径分隔注入；对目标平台保留名/大小写/Unicode规范化冲突生成安全新名或拒绝并显示原名。长度按目标文件系统限制，不能假设所有系统相同。

使用已打开目录能力和no-follow等平台适当原语，防止检查后目录被换成symlink；不能只做字符串prefix检查。临时文件独占创建，不跟随现有link；写入受限，fsync策略明确；核对已批准大小/哈希后原子rename。跨filesystem移动单独实现可恢复copy+commit，不能假装rename原子。

默认禁止解压、自启动、自动打开可执行文件、自动导入证书/配置。目录/归档安全独立授权。转发未经扫描/确认的接收文件必须重新明确目标与用户意图。

## 4. 网络解析与资源

所有长度先检查、再分配；checked arithmetic；嵌套结构有深度/元素/总字节限额。读取header、body、握手、idle、write分别计时，其中slowloris使用absolute deadline而不是每字节刷新。

HTTP/RTSP拒绝重复或矛盾framing字段、未经支持的transfer encoding、超界端口和恶意URL；二进制plist/protobuf/TLV要求边界完整。UDP/RTP验证SSRC/session绑定，处理sequence回绕/丢包/乱序，不能由数据包随意创建无限会话。

错误日志转义控制符，保留可定位的类型和长度，不打印完整敏感包。PIN/公钥指纹可能具识别性，不随telemetry默认上报。

## 5. URL、媒体和外部程序

默认只允许host批准的http/https资源，按用途区分“允许TV访问我自己提供的局域网媒体”与“外来URL要求我访问内网”。不能一刀切禁止所有private IP而破坏DLNA；应绑定已批准peer/subnet/resource lease。重定向每跳复核scheme/host/IP/授权，DNS解析到连接之间固定并核验地址，限制redirect次数和字节/时限。HLS playlists、segments、subtitles、keys等子资源也走相同策略。

不得把token/Authorization自动转到重定向后的别域。拒绝file://、shell语法、UNC、device paths和未知schemes。native player选定固定可执行路径、参数结构化；不要shell拼接网络字段。必要的mpv/FFmpeg也要禁用不需要的协议/脚本/配置加载，并限制能访问的文件与网络。

decoder/thumbnailer/字幕parser独立预算/隔离；压缩流“解密成功”不代表内容可信。硬解码也需处理损坏码流与设备reset，不给主daemon无限分配图像尺寸。

## 6. Worker隔离不是一句spawn

每个平台交付一份实际policy和验证日志：可读/写哪些文件、可连哪些接口/地址、可创建哪些子进程、继承哪些句柄、CPU/内存限制。worker启动时清理环境和多余FD，指定独立state目录，不继承用户home访问能力作为理所当然。

至少测试worker主动尝试读主仓canary、虚拟SSH key、未授权文件、连接被禁地址、启动shell时被OS拒绝。使用**人工生成的假秘密**，不是用真实私钥做测漏。对做不到的平台标记process-only并限制feature/分发，不能用“纯Rust”代替sandbox。

设备级长驻协议密钥只给必要provider，尽可能host broker按用途操作。小心协议真的需要私钥运算时不能凭空声称provider永远看不到secret；必须明确可隔离程度和攻击后果。

## 7. 跨协议桥接

bridge有两个独立会话与两次授权。source加密在bridge终止，target重新加密；UI应提示bridge能处理明文，不能称“原生发送方到最终目标全程E2EE”。

内置origin/route ID与hop budget（初始3）防循环；文件回流去重不能泄露跨audience内容存在性。目标选择必须用户确认或已批准规则，不能让恶意源指定一个公网上传目的地。录制/反控不可从“允许播放”推导。

## 8. App/CLI安全

Tauri capability限定具体命令；远端页面/文件内容不以trusted HTML或JS插入主WebView；字符串按文本渲染。网页测试界面若允许外部浏览器访问，另行bearer/CSRF/origin策略，默认loopback且非共享cookie。禁止把所有plugin shell权限开放给frontend。[S18–S19](03-reference-projects.md)

CLI不在process args打印tokens；机器可读结果不带ANSI/PIN。`doctor`提供明确步骤但不自动降低系统保护或下载驱动。系统网络变更需用户许可和可验证rollback。

## 9. 安全发布阻断条件

未认证可读任意文件；认证旁路；无限内存/线程/连接；文件穿越/覆写；跨scope令牌可复用；已取消会话自动恢复；日志出现真实密钥；构建或runtime未知外连；第三方代码来源/许可不明确；声称sandbox但canary测试失败。以上任一项阻断受影响profile release，不必阻断完全独立且未受影响的其他profile。
