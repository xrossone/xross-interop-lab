# Reviewed Wire Spec — localsend.v2（F01）

**状态**：source-reviewed（骨架）。**来源**：r13-protocol @62bd3406（restricted，事实引用注明出处）、
xross-dev adapter @099b6c72、r14 @230fb692。**本文件不复制 r13 文档文本**，只固化互操作实现所需的事实骨架。
**开放项**：P-F01-1/2/3 关闭后补全字段级差异矩阵。

## 发现（Discovery）

- 传输：UDP 多播。默认多播组 `224.0.0.0/24`（注释原因：部分 Android 拒绝其他组），默认端口 `53317`。
- Announce JSON（字段骨架，可选位待 P-F01-1）：`alias`、`deviceModel`、`deviceType`、`fingerprint`、`port`、`protocol`、`download`。
- 多播不可达时：向候选地址逐一直接 `POST /api/localsend/v2/register`（TCP 探测旁路，adapter scan.rs 模式）。
- **禁止实现偏移**：不可把 UDP 发现改写成"仅 mDNS"（catalog F01 边界）。

## 传输与安全

- HTTPS（自签名证书）为主；证书 fingerprint 经 announce 分发供对端校验。
- 明文 HTTP 仅用于浏览器 Web Share 场景（显式降级，不作为默认）。

## 端点（v2，收方为 HTTP 服务端）

| 端点 | 方向 | 语义 |
|---|---|---|
| `POST /api/localsend/v2/register` | 发→收 | 显式登记/旁路发现；响应收方信息 |
| `POST /api/localsend/v2/prepare-upload` | 发→收 | 提交文件清单（元信息）；收方逐文件 accept/reject；返回 sessionId + 每文件 token |
| `POST /api/localsend/v2/upload?sessionId&fileId&token` | 发→收 | 逐文件字节流；token 一次性校验 |
| `POST /api/localsend/v2/cancel?sessionId` | 双向 | 会话取消；token 失效 |
| `GET /api/localsend/v2/info` | 探测 | 收方信息（下载页/旁路发现用） |

v2 不含 `prepare-download` 语义的强依赖（新版本草案中存在——不进入本 profile 实现范围）。

## 状态机投影（interop 侧）

```
discovered → offered(prepare-upload) → awaiting-consent(部分接受) → transferring(upload×N) → verifying → completed
                    │                                                    │
                    └── reject ──→ completed(拒绝，无文件)                └── cancel → cancelled
```

- 收方主导文件级决策；协议无 resume → 不宣称续传（FILE-07）。
- 400 vs 403 语义（adapter consent.rs）：400=永不接受（standing deny），403=本次人拒绝——投影进 ConsentTable 时保留区分。

## 元信息预算（docs/01 §8 产品约束，非协议值）

offer 元信息 ≤ 1 MiB 且 ≤ 10,000 entries（同时应用）；超出按预算拒绝或显式提额。

## 待固化（P-F01-*）

- 字段级兼容矩阵（各 stock 版本 announce/register 的可选位）。
- pin 流程（prepare-upload 401 → pin）与 token 生命周期的精确语义。
- Unicode/同名/目录（相对路径 components）在库存端的实际处理。
