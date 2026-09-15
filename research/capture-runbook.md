# 抓包 runbook（S3：lab 网段抓包 + 脱敏）

> 用途：把"只有真机才能看见的字节"变成可复查的 lab 证据。**抓包由用户执行**（本机 + 自有设备），
> 本仓负责脱敏、固化字段行与记 hash。原始 pcap **永不入库**（`.gitignore` 已覆盖 `/captures/` 与 `*.pcap`），
> 只有脱敏摘录进 `evidence/`。
>
> 批准状态：S3 = **已批准（2026-09-15，用户主动配合）**。S1/S2 未批准前，AirPlay 侧的接收端抓包不做（见 §4）。

## 0. 先决条件与预检

```bash
cd /Volumes/Portable2TB/ExtDev/xross-interop-lab
mkdir -p captures                       # 不入库
networksetup -listallhardwareports | grep -A2 Wi-Fi   # 取 Wi-Fi 的 Device（通常是 en0）
```

预检（10 秒，确认"手机的组播在 Mac 上看得见"）：

```bash
sudo tcpdump -i en0 -n -c 20 'udp port 5353'      # 同时在手机上打开一次分享面板
```

- 有包 → 直接进 §1。
- **0 个包** → 多半是 AP 的客户端隔离（或手机在 5G 上、Mac 在另一个 SSID）。
  两个办法：① 手机与 Mac 连同一个 SSID/频段重试；② 走 §2 的"Mac 热点"方案（那时抓 `bridge100`）。

## 1. 抓取 A：发现过程（P-F02-1 的发现半边 + P-F02-3）

```bash
sudo tcpdump -i en0 -U -w captures/qs-01-discovery.pcap 'udp port 5353 or udp port 1900'
```

`-U` 是边抓边写，Ctrl-C 不会丢掉尾巴。**只留 mDNS(5353) 与 SSDP(1900)**——发现字节都在这两条里，
同时把无关流量挡在外面（少一点隐私面）。

手机动作（**按顺序，每步之间停 5–10 秒**，便于按时间轴切段）：

1. 打开手机"Wi-Fi 设置"页（让系统刷新一次 mDNS）。
2. 进"快速分享 / Quick Share"（三星：快速分享；小米/OPPO/vivo：互传/附近分享）**设置页**，
   把可见性从"仅联系人"改成"所有人 / 附近的所有人"——**这一步才决定它会不会广播**。
   再改回"仅联系人"停 10 秒（差分！见下）。
3. 回桌面 → 选中一个文件 → 分享 → 快速分享，停在"选择设备"列表 20–30 秒不动。
4. 关掉分享面板，Ctrl-C 结束。

**差分法（不需要事先知道服务名，最稳）**：抓包期间开着这条旁证命令，

```bash
dns-sd -B _services._dns-sd._udp local.     # 列出局域网上正在宣告的服务类型（Ctrl-C 停）
```

把"可见性=所有人"那段与"仅联系人"那段对比：**只在开启时出现的服务类型**就是 Quick Share 的发现服务。
抓包里再留意形如 `_<12 位十六进制>._tcp` 的名字（Nearby Connections 系的常见形态）——这是**假设**，
抓包要么验证要么否证，**未验证前不会写进 spec 或实现**。

## 2. 抓取 B：传输过程（真正的 wire 字节；可选，需要改一次系统网络设置）

unicast TCP 在别人的 AP 上看不见（除非监听模式或中间人，那些都不做）。干净的办法是把**两台设备都
放进 Mac 的热点**，让它们的流量从 Mac 的网桥过：

1. 系统设置 → 通用 → 共享 → **互联网共享**（来源：以太网/Wi-Fi；目标端口：Wi-Fi）→ 打开。
   ⚠️ 这是**真实系统网络变更**，做完记得关掉。
2. 手机与第二台设备都连上这个热点。
3. 抓网桥：

```bash
sudo tcpdump -i bridge100 -U -w captures/qs-02-transfer.pcap 'tcp or udp'
```

4. 手机往第二台设备传一个 **2–5 MB** 的文件；传输到一半时**停 2 秒再继续**（便于区分分块与 ack）。
5. 结束后关闭互联网共享，恢复原网络。

## 3. 交回给 lab

- 文件放 `captures/`（或任何仓外路径，只要告诉我路径）。**不要**把 pcap 提交进 git。
- 附一句设备清单（型号 + 系统版本 + 谁发谁收），例如写进 `captures/device-notes.md`：
  `A: Pixel 8 / Android 15（发）；B: Windows 11 + Quick Share（收）；Mac: macOS 15 (en0)`
- lab 侧的处理（由本仓执行，可复查）：
  1. 脱敏：丢 MAC/IP/主机名/文件名/任何载荷，只留协议结构（服务类型、TXT 键、端口、帧形状与长度）。
  2. 摘录进 `evidence/quickshare-discovery-capture/`（人类可读 + JSON），记**原始 pcap 的 sha256**。
  3. 据此固化 `specs-reviewed/f02` 的发现行（每条带"抓包 2026-xx-xx + 设备"来源），并把 gate 的
     blocked 行解除；**不**据此推断任何 crypto/凭据材料。

## 4. 现在不做的抓包（诚实边界）

| 目标 | 为什么现在不做 |
|---|---|
| P-M01-2 AirPlay `audioFormat`/`ct`/`spf` | 要一个**真的 AirPlay 接收端**在线，iPhone 才会发 SETUP；接收端 = UxPlay（S1）+ 真机（S2） |
| AirPlay `features` 位值（TXT） | 同上，需要接收端广播；别人的 Apple TV 广播只能补观测，补不了 sender 侧取值 |
| 真机画质/长跑矩阵 | S2：需要你的设备与连续在场时间（见 §1.4 的 A1–A10） |

## 5. 安全与边界

- 只抓**你自己的**网段；抓完原始 pcap 留或删由你定（lab 只需要脱敏摘录 + hash）。
- 脱敏前 lab **不**引用任何设备名/人名/文件名；`captures/` 与 `*.pcap` 已被 `.gitignore` 挡住。
- 抓包不产生"设备认证材料"：UKEY2/TLS 之后的载荷是密文，本仓也不从中反推密钥。
