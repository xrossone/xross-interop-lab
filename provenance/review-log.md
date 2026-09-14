# T05 审查日志（review log）

**日期**：2026-09-15　**决议人**：AI agent（初步路线）；**生产放行权**：保留给用户
**输入**：T02 source-intake、T04 dossiers、docs/06（四条复用路线）、airplay-research.md

## 决议摘要

| provider | route | 发布单元 | 组合审查 | 放行状态 |
|---|---|---|---|---|
| UxPlay | worker（外部二进制 oracle/fallback） | external-binary（GPL-3.0 义务保留） | pending | pending-user-review |
| shairplay-rust | worker（库进独立 airplay-worker） | airplay-worker-lgpl | pending（Rust×LGPL 链接方式待定） | pending-user-review |
| LocalSend | reuse（自有实现 + 主仓 adapter） | interop 核心 | 不适用（宽松许可） | pending（localsend-rs LICENSE 文件缺失，待用户补/确认） |
| QuickShare 参考 | independent（事实/规范 → 独立 Rust 实现） | interop 核心 | 不适用 | pending（wire spec 未固化） |
| GStreamer | reuse（运行时依赖，media-worker 内部） | media-worker | pending（插件许可矩阵未审） | pending-user-review |
| WinRT Miracast | independent（系统 API 直接用） | windows native provider | 不适用 | pending（windows-rs 依赖 intake 未做） |
| Apple FairPlay | vendor | — | — | **永久 vendor-gated**（T05-03 真实锚点） |

## 逐项理由（含否决项）

1. **UxPlay**：GPL-3.0。任何形式的表达复制/翻译都会传染许可；只作为未修改二进制的外部
   oracle + fallback 进程。其 `raop_handlers.h`/`srp.c` 允许行级事实引用（注明 commit+行），
   不允许把逻辑翻译成 Rust——`/pair-setup-pin` 的实现将按协议事实独立编写（specs-reviewed/m01
   已固化 plist/SRP 步骤事实），这属于"规范/事实 → 独立实现"路线，与逐行翻译的区别在
   review 时按 docs/06 §1 标准复核。
2. **shairplay-rust**：LGPL-3.0 是四条路线里最干净的强能力选项（纯 Rust、可审计、Gate 1 已过）。
   隔离方式选 worker 进程 + 独立发布单元，而不是链接进主 daemon。**不把 socket 包装当成许可
   隔离定理**（T05-02）：composition_review 留 pending，等 T15+ 定了链接形态（动态加载/独立
   进程参数化）再按 GNU FAQ 因素（通信机制、语义耦合、组合紧密度）完成审查。
3. **LocalSend**：唯一接近放行项。阻碍不是许可实质而是形式：用户自有 localsend-rs 的
   Cargo.toml/README 声明 MIT，但仓库里没有 LICENSE 文件。已列为 pending-user-confirmation
   （用户补文件即可 approved）。R13 协议文档仓无许可 → restricted，只用事实。
4. **QuickShare 参考**：全组未形成 wire spec（P-F02-1/2 未关）。此刻任何"复用"都无从谈起；
   走 independent 路线积累事实。rquickshare（GPL）只允许结构思想参考——T05-01 否决情景
   （把 GPL Rust 库改改就说自有）在此预设禁止。
5. **GStreamer**：UxPlay 同款的成熟管线依赖，但插件许可矩阵参差（GPL 插件、非免费插件）。
   只允许出现在 media-worker 独立单元内，最小插件集审计完成前不放行。
6. **WinRT**：Windows 系统 API 随 OS 授权使用，无需厂商 SDK 分发权（不是 vendor-gated）；
   但 windows-rs 绑定 crate 是新依赖 → SEC-07 intake 未做 → 暂不放行。若未来涉及真厂商
   SDK（认证/私有框架），必须另立 vendor-gated 条目并永久锁定。
7. **Apple FairPlay**：无授权 → vendor 永久锁定。release manifest 构建器会永远拒绝它
   （tools/test_provider_adoption.py 的真实数据断言）。

## 放行流程（后续）

`production_approved=true` 需要：approval.status=approved（用户裁决）→ composition_review=passed
（worker 路线）→ release manifest 构建器放行该单元。任何一步缺失，构建被拒绝
（"无许可结论的 profile 构建被 release feature manifest 拒绝"——已由测试固化）。

## 诚实记录

- 本轮所有决议由 AI agent 作出，全部 production_approved=false；用户尚未裁决。
- localsend-rs 缺 LICENSE 文件是本次审查的发现之一。
- "独立实现"路线的法律边界按 docs/06 §1 理解：本记录是工程风险管理，不构成法律意见。
