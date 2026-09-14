```json xross-dossier
{
  "profile_id": "wfd.miracast",
  "catalog_id": "M05",
  "priority": "P1→P2",
  "status": "catalogued",
  "capability_status": "not-implemented",
  "directions": {
    "receive": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r32-miraclecast", "page-r34-miracast-sink", "page-r37-castengine", "page-s04-win-miracastreceiver"],
      "notes": "Windows 优先（系统 MiracastReceiver API）；Linux sink 参考 miraclecast（sink-only）"
    },
    "send": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r33-gnome-network-displays", "page-s04-win-miracastreceiver"],
      "notes": "MiracleCast README 明确 source 未实现 → source 不从 MiracleCast 推（catalog 边界）；Linux source 看 gnome-network-displays；macOS 无公开 WFD API"
    },
    "control": {
      "evidence_level": "catalogued",
      "capability_status": "not-implemented",
      "evidence_refs": ["page-r37-castengine"],
      "notes": "WFD RTSP 控制会话（M0-M16）作为 sink/source 共用控制面参考"
    }
  },
  "sources": [
    {"id": "R32", "name": "albfan/miraclecast", "commit": "0b7f1f1f6586", "license": "LGPL-2.1 为主", "usage": "sink/无线协商对照（source 未实现）"},
    {"id": "R33", "name": "GNOME/gnome-network-displays", "commit": "521caad089ca", "license": "GPL（版本待核）", "usage": "source 与 NetworkManager 交互参考"},
    {"id": "R34", "name": "ivygroup/miracast-sink", "commit": "214d77aec1d1", "license": "Apache-2.0 衍生声明", "usage": "WFD RTSP/媒体拆分历史参考"},
    {"id": "R37", "name": "openharmony/castengine_wifi_display", "commit": "13ce3d3ebc82", "license": "Apache-2.0（根确认）", "usage": "开放 WFD 组件优先参考（系统服务依赖≠App 可用）"},
    {"id": "S04", "name": "Windows MiracastReceiver API 文档", "commit": "learn.microsoft.com 页面（2026-09-15 读）", "license": "微软文档条款", "usage": "平台入口事实"},
    {"id": "S05", "name": "Windows MediaSourceCreated 文档", "commit": "同上", "license": "同上", "usage": "MediaSource ≠ raw packet 导出保证"}
  ],
  "unknown_fields": [
    {"field": "Windows MiracastReceiver 在 headless/服务会话的可用性（用户会话/UI owner 要求边界）", "probe": "P-M05-1"},
    {"field": "WFD IE 协商与 RTSP M0-M16 序列的实测字节（本组仓库均未 source-review）", "probe": "P-M05-2"},
    {"field": "典型 TV/手机 sink 的 codec/profile 组合（H.264 profile/level、LPCM/AAC 音频）", "probe": "P-M05-3"},
    {"field": "R41 AOSP frameworks/av WFD 栈（入口未核验，本组外排除项）", "probe": null}
  ],
  "probes": [
    {"id": "P-M05-1", "question": "Windows native receiver 在本产品形态能拿到什么（MediaSource 播放 vs raw）？", "method": "Windows 测试机 platform probe（只读检查 API 存在性 + 权限，不自动改系统配置——CORE-09）", "status": "planned"},
    {"id": "P-M05-2", "question": "WFD 控制面实际序列？", "method": "隔离 Linux 测试机跑 miraclecast sink 抓 RTSP 序列（不执行其旧脚本，NetworkManager 操作隔离机）", "status": "planned"},
    {"id": "P-M05-3", "question": "目标设备 codec 矩阵？", "method": "Samsung/Huawei/Windows+K 真机矩阵（catalog 验收）", "status": "planned"}
  ]
}
```

# Protocol Dossier — wfd.miracast（M05）

## A. Scope

- **Profile**：Miracast / Wi-Fi Display（WFD）；receive（Windows native 优先）+ send（Linux/Windows 路径分别验证）。
- **系统入口**：Windows Win+K / "无线显示屏"；Android 智能投屏；电视端 sink。
- **场景**：手机/PC → 本节点画面；本节点 → TV。**排除**：Miracast over Infrastructure（M06 独立 profile）；无受支持网卡时的伪可用（必须返回 `platform-unavailable`，U5 场景）。
- **品牌辨析**：Smart View（三星）= WFD 实现，不另建栈；DLNA（M07）非实时镜像，不混同。

## B. Sources and provenance

见机器头；commit 以 lock 为准（本组未做逐文件 source-review，证据等级 catalogued）。R35 universal-miracast-sink 为 **gated**（固件/反编译来源风险），只允许 provenance 记录，不允许进入任何实现允许列表。R41 AOSP 排除（巨仓入口未核验）。

## C. Wire layers

**catalogued 级事实（来源：docs/02/03 编目 + 平台文档）：**

| 层 | 已知 | 待证 |
|---|---|---|
| 介质 | Wi-Fi Alliance Wi-Fi Direct（P2P），WPA2；独占/组管理影响当前联网（RadioLease 模型，docs/04 §6） | 本机网卡 P2P 能力 probe（P-M05-1 前置） |
| 控制 | WFD RTSP 会话（M0-M16 消息序号语义） | 实测序列（P-M05-2） |
| payload | 典型 H.264 + 音频；`MediaSourceCreated` 公开对象是 **MediaSource**（S05）——系统做解码呈现，**不保证 raw packet 导出**（MEDIA-02：native-only 不宣称录制/转发） | 真机 codec 矩阵（P-M05-3） |
| 平台 | Windows receiver=系统 API（S04）；MiracleCast **source 未实现**（R32 README 声明）；OpenHarmony 组件系统依赖≠普通 App 权限（R37 边界） | Windows headless 边界（P-M05-1） |

## D. Message/state map

投影：requested → authorizing（RadioLease + 用户确认）→ negotiating（RTSP）→ buffering → active → draining → ended。字段级 map 待 P-M05-2。

## E. Platform contract

- **Windows**：MiracastReceiver API（WinRT）——需实际平台 probe 确认 app 容器/会话要求；系统已有栈不重复实现（docs/04 §3）。
- **macOS**：无公开 WFD API → `platform-unavailable` 是合法终态，不 root、不私有框架。
- **Linux**：NetworkManager P2P 交互（R33 模式）；需要用户会话与网络管理权限。
- 所有平台：probe 只检查报告，不自动改网络配置（CORE-09）；RadioLease 显式审批（docs/04 §6：resource/shared/exclusive/disruption/deadline/rollback）。

## F. Compatibility matrix

| 对端 | 状态 |
|---|---|
| Samsung/Huawei 手机 → Windows native source | **not-run**（P-M05-3） |
| Windows +K → 本节点 | **not-run**（P-M05-1 后） |
| 无 P2P 网卡环境 | 预期 `platform-unavailable` + remedy（probe 设计的一部分） |

## G. Security review

- P2P 组管理是网络层特权操作：默认拒绝自动建组（docs/00 §3.10）；RadioLease 显式授权 + rollback 在真实硬件验证。
- RTSP 解析：长度/超时/deadline 预算（docs/07 §4）；H.264 裸流 decoder 隔离（原生路径则无 decoder 暴露——MediaSource 模式）。
- 不从 MiracleCast 旧脚本继承任何系统修改行为（其脚本可能停 NetworkManager——只隔离机验证）。

## H. Decision

**路线：Windows = native API provider（系统栈复用）；Linux = WFD core 独立实现候选（R34/R37 事实参考）；macOS = blocked（无 API，如实返回不可用）。** 正式化在 T05。重评条件：P-M05-1 平台 probe 结论。

## I. Implementation input release

- 允许：本 dossier、（P-M05-2 后的）specs-reviewed/m05。
- 禁止：R35 gated 材料；MiracleCast 脚本执行；未经 probe 的平台假设。
- 未关闭阻塞：P-M05-1/2/3。
- 验收 case：catalog M05（Samsung/Huawei/Win+K→Windows native source；Linux 收发分别；无网卡清晰不可用）。
- 负责 task：T15+ media 分卷。
