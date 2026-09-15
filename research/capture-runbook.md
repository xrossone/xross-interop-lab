# 抓包 runbook（S3：lab 网段抓包 + 脱敏）

> **抓包由用户执行**（自有设备、自有网段），本仓负责脱敏、固化字段行与记 hash。
> 原始 pcap **永不入库**（`.gitignore` 已覆盖 `/captures/` 与 `*.pcap`），只有脱敏摘录进 `evidence/`。
> 脱敏工具已就绪：`tools/capture_redact.py`（stdlib only，白名单式保留；自测 `tools/test_capture_redact.py`）。

**批准状态**：S3 = **已批准（2026-09-15，用户主动配合真机）**。S1/S2 未批准前不运行第三方接收端
（UxPlay 等）；但 AirPlay 的抓包**不需要** S1，因为可以走库存 Apple TV（见 §3）。

**可用设备（2026-09-15 用户告知）**：Samsung Android（快速分享 Quick Share）、iPhone + iPad + Mac、
ASUS ROG（Windows，可装 Quick Share for Windows）、Apple TV（**尚未安装，需要时装**）、Ubuntu Linux。

**哪一步用哪台设备**（一句话）：§1 与 §2 用 **Samsung 安卓**（Quick Share 是安卓/Google 侧协议，
iPhone 不参与）；§3 用 **iPhone/iPad**（Apple TV 作接收端，**不需要 S1**）；§4 用 Samsung + ROG。
**一条线只动一台发送端**，其余设备保持安静。

**优先级**：§1（定性发现介质）→ §2（传输字节）→ §3（AirPlay，Apple TV 在场时顺手做）→ §4（WFD，最难，可暂缓）。

## 0. 预检（1 分钟）

```bash
cd /Volumes/Portable2TB/ExtDev/xross-interop-lab
mkdir -p captures
networksetup -listallhardwareports | grep -A2 Wi-Fi    # 取 Wi-Fi 的 Device（通常 en0）

# 10 秒预检：手机打开一次分享面板，看 Mac 上有没有 mDNS
sudo tcpdump -i en0 -n -c 20 'udp port 5353'
```

- 有包 → §1 照做。
- **0 包** → 多半是 AP 客户端隔离（或两设备不同 SSID/频段）。换同一 SSID 重试；仍 0 包就直接看 §1 的 BLE 侧。

## 1. 先定性：发现走的是 BLE 还是 LAN？（最高优先，约 10 分钟）

Nearby Connections 有 BLE 广播、蓝牙、Wi-Fi LAN(mDNS)、Wi-Fi Direct 等多条介质，**用哪条由实现自选**。
所以第一步不是"抓更多包"，而是**定性介质**——这一步没有结论，后面的抓包方向就是猜的。

**Ubuntu 侧（BLE 观察）**：

```bash
# 需要蓝牙适配器；btmon 抓的是 HCI 层，包含广播数据
sudo btmon -w captures/qs-ble-01.btsnoop
# 或者只要"看得见哪些设备/服务"，用：
sudo bluetoothctl --timeout 45 scan on
```

**Mac 侧（LAN 观察）**，与上面**同时**进行：

```bash
sudo tcpdump -i en0 -U -w captures/qs-01-discovery.pcap 'udp port 5353 or udp port 1900'
# 另开一个窗口做差分旁证：
dns-sd -B _services._dns-sd._udp local.
```

**手机动作（用 Samsung 安卓；iPhone 在这条线上没有任何角色，见 §3）**
（按顺序，每步停 5–10 秒，便于按时间轴切段）：

0. **让 iPhone/iPad 保持安静**：不要打开屏幕镜像列表、不要打开分享面板。
   否则 mDNS 上会同时出现 AirPlay 记录，**差分就被污染了**——一次只让一个协议说话。
   顺序比时刻重要，不用秒表；真要记时刻，每步后跑一次 `date +%H:%M:%S` 记下来即可。

1. 打开 Wi-Fi 设置页（刷新一次 mDNS）。
2. 进"快速分享"设置页 → 可见性改成 **"所有人 / 附近的所有人"** → 停 10 秒。
3. 改回 **"仅联系人"** → 停 10 秒。（**差分**就看这两段）
4. 回桌面 → 选一个文件 → 分享 → 快速分享，停在"选择设备"列表 20–30 秒别动。
5. 结束两个抓取（Ctrl-C）。

**三种结论都对 P-F02-1 有用，务必如实记下是哪一种**：

| 观察 | 结论 |
|---|---|
| 只有 BLE 有流量 | 发现走 **BLE 广播**：mDNS 只是可选通道，f02 的发现行要按 BLE 广告固化 |
| 只有 mDNS/SSDP 有 | 发现走 **Wi-Fi LAN**：可以按 mDNS 记录固化 |
| 两条都有 | 两条通道并存，**分别**固化，且要记录哪条在什么可见性档位下出现 |

BLE 侧我们**只记"有没有、哪种广告类型、服务 UUID 与长度"**，不反推、不伪造任何 UUID 常量；
抓包里若出现形如 `_<12 位十六进制>._tcp` 的服务名，那只是**假设**，验证前不进 spec。

## 1b. 决定性实验：三种可见性档位（**下一步优先做**）

首次抓包（2026-09-16）已经把"服务类型 / 实例名布局 / 端口 / TXT 键"对照完成（见
`evidence/2026-09-16-s3-quickshare-discovery/` 与 f02 的 F-41），剩下唯一的卡点是 **`n` 的位域读法**：

- 观测：`n` = 23 字符 base64url → 17 字节 = 1 位域（`0x32`）+ 16 字节识别材料，**没有设备名字段**；
- F-02 声明位域含 3 位 version / 1 位 visibility（0=可见）/ 3 位 device type / 1 位保留，
  且设备名"仅在可见时"出现；
- 于是有两种读法、各自与一条观测冲突：MSB 先（version=1, visibility=1, device_type=1）与"名字字段缺席"相符
  但与"用户开着所有人"冲突；LSB 先（version=2, visibility=0=可见, device_type=1）与用户设置相符
  但与"可见时带名字"冲突。

**抓法（约 3 分钟）**：

```bash
sudo tcpdump -i en0 -U -w captures/qs-03-visibility.pcap 'udp port 5353 or udp port 1900'
```

手机按顺序做，**每档之间停 10 秒并在终端跑一次 `date +%H:%M:%S` 记时刻**：

1. 快速分享可见性 = **所有人** → 等 30 秒；
2. 切 **仅联系人** → 等 30 秒；
3. 切 **隐藏 / 关闭** → 等 30 秒；
4. 结束抓取。

判定规则（我会按这个读）：

| 观察 | 结论 |
|---|---|
| 哪一档下 `n` 的首字节变化 → 变化的那一位就是 visibility 位，且能定出极性 |
| 哪一档下 `decoded_len` 变大（17 → 17+1+名字长度）→ 这就是"仅可见时带设备名"成立的那一档 |
| 三档都不带名字 → F-02 的"仅在可见时"这句需要修正（记为来源修正，不是我们的实现问题） |

**同一批顺手做（可选，20 分钟）**：让 ROG 装上 Quick Share for Windows 并在同一网段打开，再抓一次
同一服务类型——两台设备互为"第二个广播者"时，就能对照 **device_type 位**与两台设备各自的 `n`/`f`/`IPv4` 取值，
这是把 F-02 的位域彻底钉死的最快路径（也是第一次真正的双向发现观测）。

## 2. 传输字节：**Samsung（发）→ ASUS ROG（收）**（约 20 分钟）

unicast TCP 在别人的 AP 上看不见（监听模式与中间人都不做）。干净办法：让两台设备都挂在 Mac 的热点上。

1. **ROG 上装 Google「Quick Share」for Windows 并登录**（与手机同一个 Google 账号；Samsung 快速分享
   与 Google Quick Share 互通）。
2. Mac：系统设置 → 通用 → 共享 → **互联网共享**（来源：以太网或 Wi-Fi；目标端口：Wi-Fi）→ 打开。
   ⚠️ 这是真实系统网络变更，做完立刻关掉。
3. 手机与 ROG 都连上这个热点。
4. 抓网桥：

```bash
sudo tcpdump -i bridge100 -U -w captures/qs-02-transfer.pcap 'tcp or udp'
```

5. 手机 → 分享一个 **2–5 MB** 的文件给 ROG。**传到一半停 2 秒再继续**（便于区分分块、确认与重传）。
6. 结束后关掉互联网共享，恢复原网络。

> 如果 `bridge100` 上一个包都没有：这**不是失败**——它说明这次传输没走 Wi-Fi LAN
> （很可能走了 BLE/蓝牙/Wi-Fi Direct），那本身就是 P-F02-1 要的结论，记下来。

## 2b. 两台设备同时广播（ROG + 手机，**最优先**，约 10 分钟，零网络改动）

目的：让 `_FC9F5ED42C8A._tcp` 上**同时出现两个广播者**（手机 + ROG），从而对照 F-02 位域里的
`device_type` 位与两台各自的 `n`/`f`/`IPv4` 取值——这是把位域语义钉死最快的路径（也顺带回答
「`_quickshare._tcp` 为什么只被查询、没人广播」）。

**ROG 侧准备（先在家里 Wi-Fi 上做完，避免后面热点里没有网）**：

1. 打开 Quick Share（Google 的 Quick Share for Windows），**登录 Google 账号**（与手机同一个账号）。
2. 设置里把可见性改成 **「所有人 / Everyone」**（默认是"仅联系人"）。
3. 保持 Quick Share 窗口开着（前台）。
4. **Windows 防火墙的坑**：换网络后 Windows 会把新网络当「公用网络」并**静默拦掉入站**。
   控制面板 → 网络和共享中心 → 把当前网络改为 **「专用网络 / Private」**；弹出
   「是否允许此电脑可被发现」时选 **是**。不做这一步，抓包里会一个包都没有。

**Mac 侧抓取**（家里 Wi-Fi，不用开热点——mDNS 是组播，en0 直接看得见）：

```bash
sudo tcpdump -i en0 -U -w captures/qs-04-twodev.pcap 'udp port 5353 or udp port 1900'
```

**动作顺序（每步后在终端跑一次 `date +%H:%M:%S`）**：

1. ROG 的 Quick Share 窗口保持前台 → 等 20 秒。
2. 手机打开快速分享面板（设备列表）→ 等 20–30 秒（两台应当互相看见）。
3. 在手机上**改一次可见性**（所有人 ↔ 仅联系人），每档停 15 秒。
4. 关掉两边面板 → 结束抓取。

我会比对：两个广播者的 `n` 位域（`device_type` 位该不同）、`f` 取值、实例名长度/首字符、端口、
以及 ROG 是否广播 `_quickshare._tcp`（手机一直在查它）。

## 2c. 传输字节（在同一份操作里顺手做，需要热点）

发现做完后，若要拿 TCP 字节（UKEY2 与 4 字节长度前缀帧），把两台设备都放进 Mac 热点：

1. Mac：系统设置 → 通用 → 共享 → 互联网共享（来源：Wi-Fi，目标端口：Wi-Fi）→ 打开。
2. ROG 与手机都连这个热点；**ROG 上再次确认网络是「专用网络」**（换网络会重置）。
3. 抓网桥：`sudo tcpdump -i bridge100 -U -w captures/qs-05-transfer.pcap 'tcp or udp'`
4. 手机 → 快速分享 → 选 **ROG**（名字形如 `DESKTOP-XXXX` / 自定名）→ 发一个 **1–2 MB** 文件，
   在 ROG 上**接受**；然后**再发第二个文件**（两次传输 = 两段可对照的会话）。
5. 结束：关掉互联网共享，恢复原网络。

> 若 bridge100 上只有 mDNS 没有 TCP：说明这次传输没走 Wi-Fi LAN（可能走了 BLE/蓝牙/Wi-Fi Direct）——
> 那本身是 P-F02-1 的结论，记下来即可。

## 3. AirPlay：**iPhone/iPad（发）→ 库存 Apple TV（收）**（不需要 S1，约 20 分钟）

> 这条线**不用三星**（安卓没有 AirPlay 发送端），也别在这条线里开快速分享。

P-M01-2（`audioFormat`/`ct`/`spf`）与 features 位值一直缺的是**发送端真实取值**。用一个**真实接收端**
（Apple TV）就能拿到，不必运行 UxPlay（那是 S1 的事）：

1. Apple TV 开箱激活（**需要联网**——可以直接先连 Mac 的热点，让它顺便有网）。
2. 把 **Apple TV 与 iPhone/iPad 都连到 Mac 的热点**（同一子网，mDNS 才看得见彼此）。
3. 抓网桥（AirPlay 控制连接是 TCP 7000；媒体另开连接）：

```bash
sudo tcpdump -i bridge100 -U -w captures/ap-01-control.pcap 'tcp or udp'
```

4. iPhone 上做四件事，每件之间停 5–10 秒：① 打开控制中心的**屏幕镜像**列表（看 mDNS 交换）；
   ② 连上 Apple TV 开始镜像 20 秒；③ 调一次音量、切一次 App；④ 断开镜像。
5. 结束抓取。

抓包会告诉我们两件事：**发送端实际发的 `audioFormat`/`ct`/`spf` 取值**，以及**这条控制连接是否明文**
（若 Apple TV 走的是我们没预期的加密形态，那同样是一条要记的结论，而不是失败）。

## 4. WFD / Miracast（最难，可暂缓）

Samsung「Smart View」→ ROG 的「无线显示器 / MiracastReceiver」是 P-M05-1 的现成组合，但 **Wi-Fi Direct
不经过 LAN**，Mac 抓不到。要抓 M0–M16 与 WFD IE 得在 Ubuntu 上跑一个 sink + **支持监听模式的网卡**，
这是另一个量级的准备（等 §1–§3 有结论后再排）。P-M05-1 的"Windows receiver 能不能脚本化"可以**只看行为**
（不抓包）先在 ROG 上确认。

## 5. 交回与脱敏

```bash
python3 tools/capture_redact.py captures/qs-01-discovery.pcap \
    --out-dir evidence/quickshare-discovery-capture --label qs-01-discovery
```

产物：`*-redacted.json`（机器可读）、`*-redacted.txt`（人类可读）、`*-REDACTION.md`（删了什么/留了什么/
原文 sha256）。**原始 pcap 不入库**，也只有脱敏摘录会进 `evidence/`。

一并告诉我：设备清单（型号 + 系统版本 + 谁发谁收）、每步的大致时刻、以及 §1 的三种结论里是哪一种。

## 6. 安全与边界

- 只抓**你自己的**网段；原始 pcap 留或删由你定（lab 只需要脱敏摘录 + hash）。
- 脱敏是白名单式：只有服务类型、TXT **键名**、协议常量形状的取值、端口、长度、时序、TCP 首批载荷形状会留下；
  设备名/人名/文件名/IP/MAC/UUID 一律替换成稳定假名或 `<redacted len=N>`。
- 抓包不产生"设备认证材料"：UKEY2/TLS 之后的载荷是密文，本仓也不从中反推密钥或身份。
