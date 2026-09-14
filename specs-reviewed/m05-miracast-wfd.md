# Reviewed Wire Spec — wfd.miracast（M05）

**状态**：review pending。本组仓库均未 source-review（T02 范围外）；平台能力未 probe。
**P-M05-1/2 关闭前本文件不是 wire spec**。

## 已登记的事实入口

| 主题 | 来源 | 备注 |
|---|---|---|
| Windows MiracastReceiver API | S04/S05（learn.microsoft.com，2026-09-15 读） | MediaSourceCreated 公开 MediaSource，非 raw packet 保证 |
| Linux sink（LGPL-2.1） | R32 miraclecast @0b7f1f1f | README：source 未实现；旧脚本只隔离机跑 |
| Linux source（GPL） | R33 gnome-network-displays @521caad0 | NetworkManager 交互模式 |
| WFD RTSP 拆分历史参考 | R34 @214d77ae | Apache-2.0 衍生声明 |
| 开放 WFD 组件（Apache-2.0） | R37 castengine @13ce3d3e | 系统服务依赖 ≠ 普通 App 权限 |
| （gated） | R35 universal-miracast-sink | 只记录 provenance，不进实现允许列表 |

## 待固化

1. WFD IE 协商 + RTSP M0-M16 实测序列（P-M05-2，隔离 Linux 机）。
2. Windows 平台 probe 结论（API 存在性/会话要求/headless 边界，P-M05-1）。
3. codec/profile 真机矩阵（P-M05-3）。
