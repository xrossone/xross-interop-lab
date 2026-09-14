# Reviewed Wire Spec — quickshare.lan.v1（F02）

**状态**：review pending。发现/握手/传输的字段级事实尚未固化——**P-F02-1/2 关闭前本文件不是 wire spec**，
实现任务不得据此写字节格式（plans/01 执行方式红线）。

## 已登记的事实入口（固定 commit，待审查）

| 主题 | 来源 | 路径建议 |
|---|---|---|
| Nearby 官方参考（含 ukey2 submodule、生成 pb 文件） | R17 nearby @2ea517ea | connections/ 模块、proto 目录 |
| UKEY2 握手规范与实现 | R18 ukey2 @10fc737a | docs/ + go/java 实现 |
| macOS 侧实现（Unlicense） | R15 NearDrop @8f1fdc7d | 接收路径 |
| Rust 结构参考（GPL，结构 only） | R19 rquickshare @378d8ae9 | core_lib wire 结构 |
| Android standalone（restricted） | R16 Bada @9fba4ee6 | 事实引用 only |

## 待固化（关闭对应 probe 后补全本文件）

1. LAN 发现载体与消息（P-F02-1 抓包 + 源码对照）。
2. UKEY2 → 传输加密绑定链、端点/端口（P-F02-2）。
3. 可见性模式矩阵与确认码语义（P-F02-3）。
