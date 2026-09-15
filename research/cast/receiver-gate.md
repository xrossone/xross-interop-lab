# T45 · 通用 Google Cast receiver 可行性 gate（2026-09-15）

**问题**：能不能在本机（桌面）做一个 Google Cast **接收端**，让 iPhone/Android 的系统投屏或
Chrome 的 Cast 按钮直接连上来？

**结论（先给答案）**：**部分可行、stock 路径 blocked**。

- **可 headless 自动化的部分**（本 task 实现）：发现广播的 TXT 形状、CONNECT/CONNECTED、
  心跳 PING/PONG、`RECEIVER_STATUS` 记账、LAUNCH 的 app 白名单与拒绝语义。
- **blocked 的部分**：让**厂商默认信任的 sender**（Pixel/Android 系统投屏、Chrome 默认）接受我们。
  依据是 F-26..F-33——认证在**消息层**，sender 把 `AuthResponse` 的证书链走到**它的信任库**；
  默认 sender 只信任 Google 根（F-28），而拿到 Google 签发的设备证书材料不在本仓允许范围
  （`provenance/*` 的 vendor-gated 边界：不获取、不伪造）。
- **唯一可闭环的自动化路径**：调用方**显式信任我们的 test-root**（参考实现自己的
  `--generate-credentials` + `--developer-certificate` 就是这个用法，F-29）——这是"我们自己的
  sender/receiver 对跑"，**不等于** stock 兼容。

## 1. 事实与出处

字段级事实见 [`specs-reviewed/m08-cast-media-control.md`](../specs-reviewed/m08-cast-media-control.md)
的 `F-26`..`F-34`（每行带 R42 的 `文件:行`）。这里只复述判定所依赖的三条：

1. **认证不在 TLS 层**：sender 侧恒定 `unsafely_skip_certificate_validation = true`
   （R42 `cast/sender/channel/sender_socket_factory.cc:69`），POSIX 侧即 `SSL_VERIFY_NONE`；
   真正的检查是把 `AuthResponse` 里的证书链走到信任库（`VerifyDeviceCert`，
   `cast/sender/channel/cast_auth_util.cc:380-382`），不可信链 → `kCastV2CertNotSignedByTrustedCa=66`
   （`platform/base/error.h:147`）。
2. **默认只信 Google 根**：`CastTrustStore::Create()`（`sender_socket_factory.cc:31-36`）；
   非 Google 证书必须由调用方显式配置信任库才可能通过（`cast/standalone_sender/main.cc:250-258`；
   Chrome 用 `--cast-developer-certificate-path`，`cast/docs/USING.md:154-161`）。
3. **receiver 侧不校验 sender**（`cast/receiver/` 下无 `VerifyDeviceCert`）：因此"能不能连上"这个
   问题**完全由 sender 的信任库**决定——这正是 stock 路径 blocked 的机制原因。

## 2. 三个验收 case 的判定

| case | 场景 | 本仓判定 | 依据 |
|---|---|---|---|
| T45-01 | 自有 test-root 被信任，但 Pixel 不认可 | **stock receiver = blocked**；自配对闭环可用（test-root 门禁显式开启时） | F-28/F-29/F-33 |
| T45-02 | 只能被 discover、不能 launch | **不记为 cast 成功**：发现与 CONNECT 可达，但 LAUNCH 需要 sender 已通过认证与 app 可用；本仓不内置任何 app id | F-30/F-32/F-34 |
| T45-03 | 认证/授权路径不明 | **停止产品集成、保留合法 demo**：产品门禁恒 `vendor-gated`，仅 test-root 演示路径可开 | F-28/F-34 |

## 3. 本仓实现的边界（写进代码里的硬约束）

- 生产门禁 `VendorDeviceAuth`：任何 LAUNCH/媒体命令一律 `vendor-gated`，理由是"证书链必须根到
  厂商信任的 CA，而本仓没有、也不会获取该材料"。
- 演示门禁 `TestRootAuth`：**只在调用方显式构造时**存在，名字里带 `test-root`，且实现里
  **不含任何证书/密钥**（只做开关与记账）——它是"我们知道自己在用测试根"的显式标记，不是绕过。
- **不内置任何 app id**：LAUNCH 只接受调用方配置过的 app id（F-17/F-32）；未配置 → 明确拒绝。
- 发现只构造/解析 TXT（`id`/`ve`/`ca`/`st`/`fn`/`md`），**不做 mDNS 组播**（与 T43 同）。
- 任何"屏幕镜像"能力声明都不存在：本 profile 只做 URL/控制面（T43-04 立场不变）。

## 4. 未关闭的 probe（需要用户/真机，不在本 task）

- **P-M08-1**：库存 Chromecast/Google TV 与 Pixel 系统投屏的实际认证强制点（哪些默认接受）。
- **P-M08-3**：各代设备实际广播的 TXT 键矩阵（本仓只按 R42 的键实现）。
- **P-M08-4（新增）**：Chrome 的 `--cast-developer-certificate-path` 在用户机器上的实际行为
  （能否让桌面 Chrome 连上我们的 receiver）——**需用户手动**，涉及在真实浏览器里加载我们生成的
  test-root 证书；即使可用也只能算"开发者路径"，不得写成 stock 支持。
