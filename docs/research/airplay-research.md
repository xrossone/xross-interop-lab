# Xross AirPlay 接收端调研记录

> **2026-09-15 决议更新（后补，不改动下方原始记录）**：用户基于本 POC 实测（shairplay-rust 无音频、
> 视频多缺陷、质量远不如 UxPlay，14 stars 成熟度不足）决议：**shairplay-rust 仅作协议事实参考，
> 不链接、不复用**；AirPlay 接收实现路线改为**独立实现 + UxPlay 外部 oracle/fallback**。
> 下文 §1"首选技术路线"的表述以本更新为准。详见 decisions/provider-adoption.json、
> research/m01-airplay-legacy-mirror/dossier.md、specs-reviewed/m01。

- 时间:2026-09-14 ~ 2026-09-15
- 来源:ChatGPT 调研对话(导出件 `~/Downloads/airplay-chat.md`)+ 本地 AI 审计与真机 POC
- 设备:iPad Pro 12.9 (iPad13,8, AirPlay/960.13.1) ↔ MacBook Pro (macOS 26.6, arm64, 有线网络, hostname `mbp-eth`)
- 决策框架(来自 ChatGPT,已采纳):
  - **Gate 1 仓库可信** ✅ 已过(2026-09-15 供应链审计 + 断网容器测试)
  - **Gate 2 真机兼容性** 🔄 进行中:transient 模式视频链路已通,画质/音频有问题(见下)
  - **Gate 3 receiver 安全加固**(fuzz / 资源限制 / RSA reachability)—— RSA trace 已完成,其余未开始
  - **Gate 4 商业分发 review**(LGPL packaging / FairPlay / airport.key)—— 未开始,不阻塞 POC

---

## 1. 仓库清单(全部在 `~/Dev/others`,即 `/Volumes/Portable2TB/ExtDev/others`)

| 目录 | 上游 | 角色 | 状态 |
|---|---|---|---|
| `shairplay-rust` | metaneutrons/shairplay-rust | **首选技术路线**(LGPL-3.0-or-later,纯 Rust) | main pinned `2fb72b3`(=v0.10.0),另有本地分支 `fix/pair-setup-pin`(见 §6) |
| `UxPlay` | FDH2/UxPlay | 参考 oracle + fallback(GPLv3, C) | HEAD `44024fe`(2026-09-11,含 GHSA-479c-ww7g-wgp8 修复) |
| `RPiPlay` | FD-/RPiPlay | 历史参考(2022 年停更,勿直接暴露网络) | 只读 |
| `android-airplay-server` | jqssun | Android 移植样板 | 构建会拉 4 个 submodule 并 patch UxPlay submodule |
| `airplayreceiver` | SteeBono(C#, MIT) | 协议参考 | FairPlay 查找表以资源形式内嵌 |
| `Airplay2OnWindows` | YimingZhanshen(C#, MIT) | Windows 参考 | **不要用它的 CI 产物**(FFmpeg/fdk-aac 未 pin),自己编译 |

结论:shairplay-rust 升级为 Xross AirPlay 首选 candidate;UxPlay 定位为 reference implementation + oracle + emergency fallback。

## 2. shairplay-rust 安全审计结论(2026-09-15,完整)

**结论:未发现恶意代码 / prompt injection / 供应链红旗。可以编译、运行、研究。**

已核查项:
- `.kiro/steering/` 五个 AI 指令文件逐字读:正常工程规范,无注入;全仓库无隐形 Unicode
- 无 `build.rs`、无 `.cargo/config.toml`、`rust-toolchain.toml` pin 1.98.0(与工具链一致)
- Cargo.lock 291 个包**全部 crates.io**,无 git 依赖;`deny.toml` 设 `unknown-registry/git = deny`
- 库代码零 `std::process::Command`;env 只读 `CI`、`SHAIRPLAY_NO_PTP`;唯一主动外连是 DACP 回连 mDNS 发现的发送端(协议必需)
- `scripts/release/tests/bin/curl|sleep` 是 bash 测试桩(离线模拟 crates.io index),无害
- CI:actions 全部 pin commit SHA、最小权限、gitleaks、拒绝 AI 署名 trailer;发布走 release-please + Trusted Publishing
- git 历史:原 shairplay(2012, juhovh)完整历史保留(LGPL 出处有利);历史最大 blob 是 FairPlay 查找表/测试数据,无内嵌二进制
- 已知例外:`deny.toml` ignore `RUSTSEC-2023-0071`(rsa 0.9 Marvin timing)。**RSA reachability trace 已完成**:`rsa` crate 只被 `src/crypto/rsa.rs` 引用;全部 3 处 `RsaKey` 均来自内置公开 `airport.key`(server.rs:21、handlers_ap1.rs:499、测试);无任何随机 RSA keygen;私有操作仅 legacy `sign_challenge`(PKCS#1 v1.5)和 `rsaaeskey` RSA-OAEP 解密 → Marvin 最多恢复一把全球公开的 key,实际风险可忽略。且 `config.rs:18` 默认广播 `et=0`(无加密),RSA/FairPlay 模式是显式配置项
- `airport.key` = 领域公开的 AirPort Express RSA key(非机密)

**断网容器验证**:rust:1.98 镜像(Dockerfile 在 `/tmp/shairplay-audit/`),`cargo fetch --locked`(联网)→ `cargo test --workspace --all-features --locked --offline`(断网),229 测试全过。注意:断网构建通过只证明"依赖缓存后可离线构建",不严格证明没有 build script 尝试联网后忽略错误——需结合静态检查(本仓库静态检查干净)。Docker volume `shairplay-cargo`(registry 缓存)保留可复用。

**DeepWiki**:`https://deepwiki.com/metaneutrons/shairplay-rust`(2026-09-14 索引,恰为 pinned commit 2fb72b)。抽查行号级引用全部准确,可当学习辅助;AI 生成,上游推进后会过时。

## 3. 构建与运行手册

### 3.1 shairplay example player(核心验证工具)

```bash
# 构建(保持 audit clone 干净,target 放隔离目录;当前在 fix/pair-setup-pin 分支)
cd ~/Dev/others/shairplay-rust
CARGO_TARGET_DIR=/tmp/shairplay-audit/target \
  cargo build --release --locked --features video --example player

# 运行(当前采用的完整参数)
RUST_LOG=debug nohup /tmp/shairplay-audit/target/release/examples/player \
  --name "shairplay-rust POC" \
  --persist /tmp/shairplay-audit/state.json \
  --transient --resample \
  > /tmp/shairplay-audit/player.log 2>&1 &

# 停止
pkill -f 'examples/player'
```

参数语义(examples/player/main.rs):
- `--name` :mDNS 显示名
- `--persist <file>`:设备身份(MAC/seed/配对)持久化,**必加**,否则每次随机身份,iPad 侧缓存全部失效
- `--pin <code>`:默认 `3939`(PIN-required HomeKit 配对);`--transient`:PIN-less transient 配对(不弹码,直连,但连接稍慢)
- `--resample`:音频采样率不匹配时启用 rubato 重采样(44100→48000 必需,见 §5.3)
- `RUST_LOG=debug`:RTSP/会话级日志;协议帧级细节需库内 trace

日志:`/tmp/shairplay-audit/player.log`(tracing,ANSI 色,`sed 's/\x1b\[[0-9;]*m//g'` 去色)
端口:RTSP 5001;视频 TCP 动态端口;mDNS 广播 `_airplay._tcp` + `_raop._tcp`

验证广播可见:`(dns-sd -B _airplay._tcp local > /tmp/x 2>&1 &); sleep 5; pkill -f 'dns-sd -B'; cat /tmp/x`
⚠️ 本机 macOS 自带 AirPlay Receiver 也在广播(显示为 "MacBook Pro"),测试别选错设备。

### 3.2 UxPlay(brew 无 formula,必须源码编译)

```bash
# 依赖(一次性;新版 brew gstreamer formula 已含全部插件,无需单独装 plugins-*)
brew install gstreamer pkg-config cmake libplist

# ⚠️⚠️ 关键坑:macOS + GStreamer ≥1.22 默认走 gst_macos_main wrapper(CMake 选项 GST_MACOS)。
# 从后台/agent shell 启动会静默卡死在 NSApplication 事件循环(不监听端口、无输出),
# 必须从用户自己的 Terminal.app 启动。症状定位手段:sample <pid> 看是否卡在 mach_msg_tramp/NSApplication。
# (wrapper ON + 用户 Terminal 启动 = 视频正常;wrapper OFF = 任何方式启动都行但 GStreamer-GL 报
#  "An NSApplication needs to be running on the main thread",镜像一启动视频管线即死 → 打勾后消失)

cmake -S ~/Dev/others/UxPlay -B /tmp/shairplay-audit/uxplay-build \
  -DCMAKE_BUILD_TYPE=Release -DGST_MACOS=ON   # 注意 cmake 缓存会记住旧值,显式传
cmake --build /tmp/shairplay-audit/uxplay-build -j

# 运行(必须从用户 Terminal.app)
/tmp/shairplay-audit/uxplay-build/uxplay -n "UxPlay baseline"
# 调试版:加 -d;音视频录制:-mp4 [fn];无视频纯音频:-vs 0
```

有用选项(`uxplay -h`):`-as <audiosink>`(默认 autoaudiosink/osxaudiosink)、`-vsync [x]|no`(音画同步)、`-vol <0..1>`(初始音量)、`-db l[:h]`(音量衰减映射)、`-s wxh[@r]`(请求分辨率)、`-h265`。

### 3.3 差分测试方法(当前 manually)

同一 iPad,先投 UxPlay 再投 shairplay,对比发现速度/首帧/音画/旋转/重连。后续可把两边 mDNS TXT、RTSP 交互、SETUP 参数做成 fixture 做自动化差分。

## 4. 真机 POC 结果汇总(2026-09-15)

### 4.1 UxPlay baseline

- ✅ **屏幕镜像视频成功**(用户自建 Terminal 启动,GST_MACOS wrapper ON)
- ❌ **音频无声**,但 Audio Metadata(album/artist/title/track length)正常打印 → 说明音频会话的控制面已建立,卡在解码→输出链路
- 已排除:GStreamer 组件缺失。`gst-inspect-1.0` 确认 osxaudiosink/autoaudiosink/avdec_aac/avdec_alac/aacparse 均 OK;`fdkaacdec` 缺失,但 avdec_aac(ffmpeg)可解 AAC-ELD 兜底
- 待试:①连接状态下按 iPad 音量键检查 **AirPlay 音量条**(与本机音量独立,反复重连后可能停在 0);②`-d` 重启看音频管线错误;③`-vsync no` / `-vol 1.0`
- 附:wrapper OFF 时镜像"打勾后消失"的根因 = GStreamer-GL 需要 NSApplication 主线程,视频管线即死(日志可见 WARNING + `raop_rtp_mirror->running is no longer true`)

### 4.2 shairplay-rust

时间线(全部有 debug 日志佐证,日志在 `/tmp/shairplay-audit/player.log`):

1. **PIN 模式(paired HomeKit)**:iPad → `/info` 200 → `/pair-pin-start` 200 → 弹码框 → 用户输 3939 → iPad `POST /pair-setup-pin`(86 字节)→ **v0.10.0 无此路由,404** → iPad 报 "unable to connect"
2. 打了路由别名补丁(§6)后:86 字节 body 走进 `handle_pair_setup`,但 TLV 解析后 fallback 到 AP1 handler 返回空 200 → iPad **无限重试 M1**(每次新 TCP 连接、同样的 86 字节),永远不进后续步骤
3. 对照 UxPlay 源码确认:**Apple `/pair-setup-pin` 是独立协议**(见 §5.1),不是 HomeKit TLV —— 需要完整移植,不是别名能解决的
4. **`--transient`(PIN-less)模式:✅ 镜像直连成功,无配对弹窗**,视频窗口正常打开(1442x1080 → 1920x1080 随旋转变化)——Gate 2 链路性胜利
5. ❌ 画质扭曲(可辨认但局部花):加了同帧聚合补丁(§6)后仍扭曲
6. ❌ 无声:日志 `⚠️ Source 44100Hz ≠ device 48000Hz — use --resample to convert`,加 `--resample` 后仍无声

## 5. 协议知识(本次实测+源码考证,写 Xross 时直接用)

### 5.1 Apple `/pair-setup-pin`(PIN 配对)= binary plist 三步 legacy SRP【未实现,移植材料齐全】

参考实现:`UxPlay/lib/raop_handlers.h:275-430`(`raop_handler_pairsetup_pin`)+ `UxPlay/lib/srp.c`(头部注明 "modified (2023) by fduncanh for use with Apple's pair-setup-pin protocol")。**不是** HomeKit TLV!

- 请求体是 **binary plist**(`Content-Type` 含 `apple-binary-plist`),响应 `application/x-apple-binary-plist`
- **Step 1**(实测 86 字节):dict `{method: "pin"(string), user: "<device_id>"}`;服务端用 **4 位 PIN 作为 SRP password** 起 session(`srp_new_user(session, pairing, user, pin, &salt, &pk)`),回 `{pk: <data>, salt: <data>}`
- **Step 2**:dict `{pk: <client pubkey>, proof: <client M>}`;校验 proof(注意 UxPlay 的 srp.c 是 SHA1 系,**proof 20 字节**,响应也 20 字节);回 `{proof: <20B>}`
- **Step 3**:dict `{epk: <32B ed25519>, authTag: <16B GCM>}`;`srp_confirm_pair_setup` 后 `pairing_session_set_setup_status`,回 `{epk, authTag}`
- ⚠️ **iPad 每个 RTSP 请求都换新 TCP 连接**(实测:/info、pair-pin-start、每次 pair-setup-pin 全是不同源端口)→ SRP 会话状态必须跨连接存活,不能像现在这样放 per-connection。UxPlay 用单条长连接 + conn->session 存;Xross 侧建议按 session/全局单槽(单发送端假设)存 pending pairing
- PIN 生效语义:`snprintf(pin, 6, "%04u", raop->pin % 10000); if (raop->pin < 10000) raop->pin = 0;` —— UxPlay 的 PIN 是一次性的?**(待考)**,shairplay 目前 PIN 是持久的

### 5.2 配对模式与 feature 位

- `--pin`(默认):PIN-required 模式,iPad 走 `/pair-pin-start` + `/pair-setup-pin`
- `--transient`:PIN-less,shairport-sync 式 transient;**video 模式此时广播 UxPlay 兼容 legacy 位**(`src/net/features.rs` 有 `video_receiver_uses_uxplay_features` 测试钉死),iPad 完全跳过配对直接 `/fp-setup`
- legacy video 流程(实测走通):`/fp-setup` → SETUP(stream_type=110, video)→ SETUP(stream_type=96, audio)→ RTP/镜像

### 5.3 shairplay 视频/音频管线(实测观察)

- 视频:stream_type=110,TCP,128 字节帧头(payload_len u32 LE + type u16 + timestamp u64 @ bytes 8-15),AES-128-CTR 解密,**密钥从 audio aeskey 三段推导**("Video key: Stage 3 derivation from aeskey_audio");`PacketKind::{AvcC,HvcC,Payload,Plist,Other}` 分发,库只交付 NAL,解码渲染归宿主
- AvcC 配置约 30 字节,实测收到多次(旋转/切换时重发);窗口分辨率 1442x1080 ↔ 1920x1080
- 音频:stream_type=96,**Legacy ALAC(AES-CBC via ekey),44100Hz**;iPad 同时开 RTP UDP(use_udp=true, control_rport);`/audioMode mode="default"`;`/feedback` 每 2 秒一次(心跳,elapsed_ms 单调增——注意这是请求处理耗时字段被复用的痕迹,待考)
- 例程音频:cpal 输出 48000Hz;**源 44100 ≠ 48000 且无 `--resample` 时不播放**(只打 warning)——这是第一轮无声的直接原因;加 `--resample` 后仍无声,见 §7 假设

## 6. 本地补丁(分支 `fix/pair-setup-pin`,基于 2fb72b3)

```bash
cd ~/Dev/others/shairplay-rust && git diff main --stat
```

1. `src/raop/rtsp.rs`:注册 `POST /pair-setup-pin` 路由 → 别名到现有 pair-setup handler。
   保留理由:协议上 Step1-3 与 HomeKit TLV 不同,**别名本身不解决 PIN 配对**,但让请求不再 404,便于后续挂真正的 handler。若要还原:`git checkout main -- src/raop/rtsp.rs`
2. `examples/player/video_display.rs`:按 timestamp 聚合同帧多包(`pending_ts`/`pending`),时间戳推进或收到新 AvcC 时 `flush_pending()` 整帧解码;IDR 前拼 SPS/PPS 逻辑保持。
   结果:**仍扭曲** → 扭曲根因另有其一(见 §7)

未提交(commit 由后续决定,方便继续改);要回干净审计态:`git stash` 或 `git checkout main`。

## 7. 遗留问题与下一步(按优先级)

### P1 — shairplay 视频扭曲(链路已通,图像不对)
已做:同帧聚合。仍扭曲,假设按概率排序:
1. **openh264 解码器用法**:openh264 crate 0.9 的 `decode()` 可能只适合"一次一个完整 access unit";多 NAL 拼接后是否需要补 AUD(`09 F0`)?可先试在每帧前插 AUD
2. **多 slice 帧**:一帧多个 slice 分散在多包,4 字节长度前缀解析在包边界处的 `break` 会静默截断(聚合后此问题已缓解,但可在 `length_to_annexb` 加日志确认没有 bad length)
3. **解密只对 Payload 类包,时间戳相同但跨包的 CTR 计数器连续性**——若库内按包重置 IV 偏移会有规律性花屏(可对照 UxPlay 同场景:UxPlay 清晰则库嫌疑大,反之 example 嫌疑大)
4. **openh264 不支持该 profile/level**:用 ffplay/ffmpeg 替代 example 渲染器做 A/B(把 NAL 落盘成 .h264 文件 → `ffplay -f h264`),一锤定音区分"数据坏"vs"解码器弱"
5. H.265:HvcC 分支存在,但实测收到 AvcC,排除

建议动作:先做 4(落盘 .h264 用 ffplay 放),10 分钟内可分辨数据/解码器责任,再决定往库查还是往 example 查。

### P2 — shairplay 音频无声
- 日志确认 ALAC 流已建立、RTP UDP 已启动;`--resample` 已加仍无声
- 假设:①example 的 ring→cpal 路径在 resample 分支有 bug;②ALAC 解码输出根本没进 ring(可在 `AudioSession` 回调加计数日志);③AES-CBC ekey 解密失败被静默吞(日志级别不够)
- 建议动作:在 main.rs 音频回调加每秒计数日志,和 `RUST_LOG=shairplay=trace` 看 RTP/解密层

### P3 — 实现 `/pair-setup-pin`(给上游的正式贡献,也是 PIN 一次配对体验的前提)
- 按 §5.1 移植:binary plist 解析(plist crate 已有)+ legacy SRP-SHA1 服务端(num-bigint 已在依赖,需按 UxPlay srp.c 移植 k/M 计算细节)+ 跨连接会话状态
- 注意 iPad 每请求换连接的实测事实,状态不能放 per-connection
- 上游提 PR 前先查 issues 是否已有讨论;commit 遵循 Conventional Commits(上游 CI 会拒绝 AI 署名)

### P4 — UxPlay 音频无声(影响 oracle 对音频的参考价值)
- 先查 AirPlay 音量条 → `-d` 看管线 → `-as osxaudiosink` 强制 → 仍不行记录为 macOS 已知问题,音频 oracle 改用"iPad 本机扬声器"主观对比

### P5 — Gate 3 其余项(fuzz `fuzz/` 四个 target、资源上限清单、`diagnostic-headers` feature 可用于抓包对齐)

## 8. 参考链接

- DeepWiki: https://deepwiki.com/metaneutrons/shairplay-rust
- RustSec RUSTSEC-2023-0071: https://rustsec.org/advisories/RUSTSEC-2023-0071.html
- UxPlay GHSA-479c-ww7g-wgp8(1.73.7 修复,认证前可达)
- GNU LGPL-3.0 与静态链接义务: https://www.gnu.org/licenses/lgpl.html
- 本机相关路径速查:
  - player 二进制:`/tmp/shairplay-audit/target/release/examples/player`
  - player 日志:`/tmp/shairplay-audit/player.log`;状态:`/tmp/shairplay-audit/state.json`
  - uxplay 二进制:`/tmp/shairplay-audit/uxplay-build/uxplay`
  - Docker 审计 Dockerfile:`/tmp/shairplay-audit/Dockerfile`(volume `shairplay-cargo` 保留)
  - ⚠️ `/tmp` 会在重启后清空:二进制可随时按 §3 重编,`state.json` 丢了只需 iPad 重新配对
