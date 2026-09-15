# 基础阶段期末报告（T01–T14）

**日期**：2026-09-15　**范围**：docs/goal-phase-1.md 全部任务　**格式**：docs/12 六项汇报格式
**结论**：基础阶段完成定义 4 项全部满足；无未记录的阻塞项。

---

## 1. 当前 task 与 commit，改动文件列表

各 task 单独提交（`Txx: <摘要>`），全部已 push origin（github.com/xrossone/xross-interop-lab）：

| task | commit | 主要内容 |
|---|---|---|
| T01 | `05d52e7` | impl/ workspace 骨架 + 卫生测试（依赖图分层/索引扫描） |
| T02 | `45eb5b3` | file-core+airplay 19 项来源 intake + allowlist + 一致性测试 |
| T03 | `a11fe51` | xross-dev 集成映射（锚定 099b6c72）+ baseline + 测试 |
| T04 | `01484ce` | F01/F02/M01/M05/M07/M08 六 dossier + evidence index + specs-reviewed |
| T05 | `760d8a4`→`19f5379` | provider-adoption（7 项决议）+ approved-inputs + review-log；`19f5379` 按用户裁决修正：localsend-rs 移出输入、shairplay-rust 降为仅协议参考 |
| T06 | `41211e5` | interop-contract：ids/capability/offer/media/error + golden + JSON Schema |
| T07 | `078115b` | interop-policy grant + interop-runtime FakeHost + standalone-host |
| T08 | `a537af6` | interop-file：path/spool/integrity |
| T09 | `d4957d0` | interop-ipc frame/auth/server + apps/interopd 骨架 |
| T10 | `7aeff2f` | interop-runtime session/events/limits |
| T14 | `922aee3` | interop-testkit + evidence/run.schema.json |
| 顶层设计 | `6ab018e` | xross-dev 顶层设计对齐简报（读于 a483bd14） |

新增/改动文件共 **70+**（逐 task 见各 commit 与 `evidence/<run-id>/run-manifest.json`）。

## 2. 实际验证命令及 exit code

| 命令 | exit | 结果 |
|---|---|---|
| `cargo test --manifest-path impl/Cargo.toml --workspace` | 0 | **46 tests 全绿**（T01 3 + T06 13 + T07 7 + T08 6 + T09 7 + T10 4 + T14 6） |
| `cargo clippy --manifest-path impl/Cargo.toml --workspace --all-targets` | 0 | 0 warnings |
| `python3 -m unittest discover -s tools -p 'test_*.py'` | 0 | **24 tests OK**（T02 4 + T03 6 + T04 5 + T05 5 + run manifest schema 4） |
| （说明）| — | 上表为最终状态；各 task 的 red→green 过程命令与中途 fail 见对应 run manifest 的 commands 数组 |

**pass**：全部上表；**fail**：无遗留；**blocked**：见 §5 清单（协议级，不阻塞基础阶段）；
**not-run**：真机互通矩阵（T15+ 范围）。

## 3. 使用的 source/fixture IDs 与新引入依赖

- **Source IDs**：R01–R19（intake 与 dossier）、xross-dev@099b6c72（映射）；全部与 `references/sources.lock.json` 一致（测试强制）。（用户自有 `~/Dev/localsend-rs` 经 2026-09-15 澄清属另一项目，已移出输入清单。）
- **Fixture**：`impl/crates/interop-testkit/tests/fixtures/sample.bin`（自制，算法与 SHA-256 记录在案）；
  其余测试 fixture 均为测试内人工构造（无第三方测试向量复制）。
- **新引入依赖**（全部记录在 run manifest，SEC-07）：`serde`/`serde_json`（序列化）、
  `sha2 0.10`（RustCrypto，完整性）。无 git 来源依赖、无浮动版本（Cargo.lock 锁定；
  卫生测试禁止 git 依赖与逃逸 impl/ 的路径依赖）。
- **未引入**：windows-rs（待 intake）、tokio（T11+ 编排再评估）、任何 GPL/LGPL 库（按 T05 路线）。

## 4. 功能达到的证据层级

**build-verified**：impl/ 全部 7 crate + adapter + app 的本机测试（46 tests）与静态检查。
**source-reviewed**：T02 intake、T03 映射、T04 dossier、T05 决议。
**device-verified（局部，且有明确障碍）**：仅 M01 AirPlay transient 链路（airplay-research.md
真机记录，iPad Pro 12.9 ↔ MacBook Pro macOS 26.6）——`capability_status=blocked`（画质/音频/PIN）。
**没有真机互通的部分就是没有**：F01/F02/M05/M07/M08 全部 ≤ source-reviewed/catalogued；
xross-dev 媒体接口 absent（mock host 先行）；无任何 release-qualified 项。

## 5. 尚未确认的 wire/平台条件与下一次 probe（blocked 清单）

| 项 | 障碍 | 下一次 probe | 重评条件 |
|---|---|---|---|
| M01 视频画质 / 音频无声 / PIN 配对 | 根因未定（解码器 vs 数据；音频链路；SRP 未实现） | P-M01-1（.h264 落盘 ffplay A/B）、P-M01-2（音频计数日志）、P-M01-3（pair-setup-pin 移植） | 三项关闭后 → simulated；再真机矩阵 → device |
| F02 wire spec 未固化 | P-F02-1/2/3 未做（发现/ukey2/可见性） | 批准 lab 网段抓包（脱敏）+ r15/r17/r18/r19 对照 | specs-reviewed/f02 固化 → 才可谈实现 |
| M05 Windows/Linux 能力 | 无平台 probe；网卡 P2P 未验 | P-M05-1（Windows API probe）、P-M05-2（隔离 Linux sink） | probe 后定路线 |
| M07/M08 | SSDP/SOAP/CASTV2 字段未实测 | P-M07-2（安全预算）、P-M08-1（lab Chromecast 实测） | 同上 |
| xross-dev 媒体接口 | absent（north-star 划出）；集成 blocked-dependency | 监控主仓媒体接口出现；interop 侧 native player 先行 | MediaHostPort 有真实对接物 |
| GStreamer 插件许可矩阵 | 未审计 | 最小插件集 + 许可清单 | composition_review=passed |
| F01 LocalSend profile 范围 | 复用目标=主仓 adapter；用户自有 localsend-rs 已移出（属另一项目） | 用户确认 F01 是否保持 P0 与 reuse 路线 | → 更新 dossier/allowlist |
| FairPlay/设备认证 | 无授权 | 不 probe（永久 vendor-gated） | 仅 Apple 授权 + 用户书面批准 |

**不是完成报告**：以上均为待办；基础阶段的"完成"只指 §2 的契约/底座测试面。

## 6. 主仓回归/接口影响与 contract owner 裁决项

- **xross-dev 未被修改**（全程只读）。注意其为活跃工作区：基线锚定 `099b6c72`，
  期末时 HEAD 已前移至 `a483bd14`（映射测试按"基线是 HEAD 祖先 + anchors 存在"校验，
  集成实现开始前需复核映射）。
- **接口影响**：无——interop 不依赖主仓任何私有路径；唯一 seam 是 XrossHostAdapter
  （ControlServiceClient gRPC/UDS）。interop 契约（T06–T10）是独立版本面（`interop.api/0.1`）。
- **2026-09-15 已由用户裁决两项**：① `~/Dev/localsend-rs` 属另一项目，不作为本项目输入（已移出
  复用候选与 approved-inputs）；② shairplay-rust 仅作协议事实参考，不链接、不复用（实测质量不达标），
  AirPlay 接收改为独立实现 + UxPlay oracle/fallback。
- **仍需用户裁决**：**是否批准 T05 的 provider-adoption 放行**（当前全部 `production_approved=false`）——
  核心是 F01 LocalSend profile 是否保持 P0 与 reuse（主仓 adapter）路线；其余为 GStreamer 插件矩阵、
  windows-rs intake 等按计划推进项。

## 下一阶段建议（T11–T13 + 首批 provider 接入）

1. **T13 worker supervisor + 真实隔离 probe**：为 AirPlay 独立实现 worker 铺路；
   canary 测试（假秘密）验证 process-only 边界如实标注。
2. **M01 probe 三连**（P-M01-1/2/3）：成本低、直接解锁第一个 device-verified 媒体链路。
3. **T11 endpoint registry**：接 LocalSend 发现（主仓 adapter 模式），先用
   testkit 的确定性 peer 做模拟闭环（simulated），不冒充真机。
4. **T12 platform probe（只读）**：macOS/Win/Linux 网络与媒体能力矩阵，RadioLease 模型落地。
5. **F02 wire spec**（P-F02-1/2）：若优先 Quick Share 方向则先做；否则排在 M01 后。
6. **集成形态裁决**（读 xross-dev 顶层设计后新增）：interop 进产品时是作为第一方角色
   （用 `xross.control.v1`）还是独立进程（用 `xross.client.v1` 公开 seam）——需用户裁决；
   详见 [research/xross-top-level-design.md](../research/xross-top-level-design.md)。
