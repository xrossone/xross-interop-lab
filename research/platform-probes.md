# 平台 probe 记录（T12）

**日期**：2026-09-15　**机器**：macOS 26.6 / aarch64（本 lab 开发机，唯一实测环境）
**命令**：`cargo run -p interop-cli --bin xinterop -- doctor --json`
**性质**：**只读**。白名单命令（`impl/crates/interop-platform/src/probe.rs` 的 `PROBE_COMMANDS`）：
固定程序 + 固定参数、非 root、不经 shell；网络部分只保留接口名，地址/MAC 不采集。

## 实测输出（原文，2026-09-15）

```json
{
  "os": "macos",
  "arch": "aarch64",
  "commands": [
    { "program": "/sbin/ifconfig", "args": ["-l"], "exit_code": 0, "lines": 1 }
  ],
  "network_interfaces": {
    "status": "observed",
    "value": ["lo0","gif0","stf0","anpi2","anpi1","anpi0","en4","en5","en6","en1","en2","en3",
              "en7","bridge0","ap1","en0","awdl0","llw0","utun0","utun1","utun2","utun3",
              "vmenet0","bridge100","vmenet1","bridge101","utun4","utun5","utun6"]
  },
  "wifi_p2p": {
    "status": "unavailable",
    "why": "macOS 无公开 Wi-Fi Direct (P2P) API；AWDL 为 Apple 私有，不探测、不假设"
  },
  "wifi_display": {
    "status": "unavailable",
    "why": "macOS 无公开 WFD/Sink API——plans/01 T12：没有实现的 Mac WFD 明确不可用"
  },
  "media_outputs": {
    "status": "not-run",
    "why": "native/file sink 属阶段 3（T29）；T07 mock host 对媒体显式拒绝——不得声称可用"
  },
  "interactive_session": { "status": "observed", "value": false },
  "permissions": {
    "status": "unavailable",
    "why": "无公开 API 读取 TCC/权限状态；授权路径见 docs/07，不猜测"
  }
}
```

（`interactive_session=false` 是因为本次由管道运行、stdin 非终端——在交互终端下会观察到 `true`；
该字段用于判断能否向用户发起 radio lease 审批。）

## 结论（进入路由/调度的字段）

| 字段 | 本机结论 | 对实现的影响 |
|---|---|---|
| `network_interfaces` | 观察到 29 个接口（含 `en0`、`awdl0`、多个 `utun*`/`bridge*`） | T11 registry 的地址候选带接口名；接口断开即失效 |
| `wifi_p2p` | **无公开 API** | 走 P2P 的 profile（M05 WFD、QuickShare P2P 升级）在本平台一律 `hardware-unavailable`，不降级、不假装 |
| `wifi_display` | **无公开 API（Mac WFD 明确不可用）** | Miracast sink 在 macOS 只能走 Class C（平台/厂商路径）或直接标不可用 |
| `media_outputs` | **未实现**（T29 之前） | 媒体首批只做本地呈现；本阶段不声称任何媒体输出可用 |
| `interactive_session` | 可观察 | radio 独占 lease 的审批入口（`Approval::Pending` 由 UI 收集） |
| `permissions` | **无 API，不猜测** | 权限状态只能由用户授权流程产生（docs/07），probe 不伪造 |

## 未运行 / 待补

- **Linux / Windows**：本阶段无目标机 → 报告中为 `not-run`，不得代填（探针形状已就位，字段与
  macOS 相同）。
- **P2P 与 WFD 的真实能力**：需要目标机 + 目标 OS 版本才有意义；本机没有公开 API，属平台边界
  而非"待实现"。
- **媒体输出能力矩阵**：T29（null/file/native sink）之后回来补测。
