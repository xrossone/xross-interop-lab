# Gap 分析 — Quick Share / UKEY2（T19，2026-09-15）

**结论一句话**：UKEY2 握手这一层已经可以在 headless 下真做（规范+proto 有字段号、两实现一致的部分已固化），
但**发现载体与可见性矩阵仍是空洞**——它们是 P-F02-1/P-F02-3，需要用户抓包与真机。

## 1. 已知 vs 未知

| 层 | 已知（可写实现） | 未知（blocked） | 备注 |
|---|---|---|---|
| discovery | 服务类型字符串、名称/ TXT `n` 布局、service id 来源（SHA-256 常量）来自 R15 逆向文档 | 当前 stock 版本的实际广播行为、可见性对发现的影响、BLE 触发条件 | P-F02-1（抓包）/ P-F02-3（真机矩阵）未关闭 → **不实现发现** |
| framing | 4 字节大端长度前缀 + 读前长度检查 | 真实上限（R15 的 5 MiB 是实现选择） | 我们的上限是本仓保守值，文档里标明 |
| handshake | UKEY2 封装/Alert 码/三条消息字段/commitment（SHA-512）/认证串与 next secret（HKDF）/cipher 选择 | 规范与实现在 HKDF 哈希与 cipher 选择描述上**冲突**（F-12/F-15） | 以实现侧为准 + 具名真机实验裁决 |
| auth | 4 位确认码算法（R15 实现） | 与 stock Android 的一致性 | 标注为兼容启发式 |
| transport | 密钥链与 SecureMessage 结构（F-19/F-20） | 分块/序号/重放窗口的真机行为 | T21 范围，本次不实现 |
| payload | introduction/response/payload transfer 字段（F-22） | 10 GiB 级流式与断点续传语义 | T21/T22 |

## 2. 本次（P3-T6）关闭了什么

- **P-F02-2 → source-reviewed**：UKEY2 绑定链（消息字段、commitment、DH 哈希、认证串/next secret、
  next_protocol 取值、4 位确认码算法）逐行带来源固化，足以支撑 T20 的握手状态机与 framing。
  仍未关闭的部分：真机互通、以及两处规范/实现冲突的最终裁决。
- 产出：`specs-reviewed/f02-quickshare-lan.md` 字段表（F-01..F-24）、`provenance/quickshare-inputs.json`、
  `evidence/quickshare/corpus-plan.json`、纪律测试 `tools/test_quickshare_gate.py`。

## 3. 仍然 blocked（障碍 + 重评条件）

1. **P-F02-1 发现载体**：障碍＝只能在用户批准的网段上抓一份真机 mDNS/BLE 记录（抓包属 S3）；
   重评条件＝抓包结果与 R15/R17 发现模块源码一致。
2. **P-F02-3 可见性矩阵**：障碍＝需要两台 Android 真机 + 本节点；重评条件＝矩阵跑完并记录。
3. **F-12/F-15 冲突裁决**：障碍＝需要一次与 stock Android 的成功握手来确认 HKDF 哈希与 cipher 选择；
   重评条件＝PIN 一致 + 握手成功。
4. **4 位确认码一致性**：障碍＝同上真机比对。

## 4. 为什么这样切分是安全的

- 握手层可以完全离线自证（两实例对跑 + 篡改/重放/截断/分片负向用例），且不依赖任何未固化字节。
- 发现层一旦先写，会把"逆向文档推测"当成协议事实固化成代码，后面真机一测就得推倒——按 P-F02-1 的
  重评条件先挂起更便宜。
- payload/传输层需要 F-19/F-20 的真机行为（序号/重放窗口）才能判对错，留给 T21。
