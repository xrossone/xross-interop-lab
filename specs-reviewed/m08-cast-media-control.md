# Reviewed Wire Spec — cast.media-control（M08）

**状态**：review pending。CASTV2 通道细节未 source-review；**P-M08-1/2 关闭前本文件不是 wire spec**。

## 已登记的事实入口

| 主题 | 来源 |
|---|---|
| 公开 libcast/CASTV2 实现（BSD 系混合，逐文件核） | R42 openscreen @29634014（cast/ 目录） |
| CAF receiver 语义（Apache-2.0） | R43 CastReceiver @ddfb06c7 |
| sender 行为对照（MIT） | R44 pychromecast @5cfdb607 |
| CASTV2 交叉参考（restricted） | R45 cast-web/protocol @c906d210 |
| 平台文档（sender/receiver 区别、注册、Web Receiver） | S07/S08/S09 |

## 骨架（catalogued）

- 发现：mDNS `_googlecast._tcp`，TXT 携带 UUID/型号/能力（字段矩阵待 P-M08-3）。
- 通道：TLS + protobuf（CASTV2）；namespace 消息（CONNECT/CLOSE、media 控制、GET_STATUS）。
- payload：receiver 按 URL 拉取；本节点签发短时/单资源/可撤销 URL lease。
- 认证边界：第三方可通信 ≠ 官方认证；不做认证绕过、不伪造 app ID（S08）。

## 待固化

1. lab Chromecast（具名型号）无认证 sender 的接受条件与 media 命令集（P-M08-1）。
2. openscreen cast/ 组件边界（sender/streaming/receiver 与 M09 的分界）（P-M08-2）。
3. mDNS TXT 字段矩阵（P-M08-3）。
