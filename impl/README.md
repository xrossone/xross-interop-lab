# impl/ — Xross Interop 实现 workspace

设计包 plans/ 中的相对代码路径（`crates/`、`apps/`、`tests/`、`schemas/`、
`adapters/`、`workers/`、`policies/`）一律映射到本目录之下，例如
`crates/interop-contract` → `impl/crates/interop-contract`。

本 workspace 是未来可能独立开源的实现仓候选内容：**只放许可可清理的实现代码与测试**，
不放 lab 研究材料（dossier/证据/抓包），不引用 `references/external/` 或任何
仓库外路径。仓库卫生规则由 `crates/interop-contract/tests/t01_workspace_hygiene.rs`
强制执行（T01-01 依赖图 / T01-02 lab 索引扫描；复制出去后 lab 扫描自动跳过）。

## 成员

| crate | 职责 | 引入于 |
|---|---|---|
| `crates/interop-contract` | 领域对象、统一错误、能力声明、JSON 契约（ids/capability/offer/media/error + golden + JSON Schema） | T01/T06 |
| `crates/interop-policy` | scoped grant：绑定 subject/session/scope/direction/预算/expiry，默认拒绝跨 scope | T07 |
| `crates/interop-runtime` | HostPorts + 内存 fake host；会话注册表（单 authority 终态）、events（4096/16MiB + gap）、limits | T07/T10 |
| `crates/interop-file` | 受限流式存储：路径形状拒绝、独占临时文件、预算、no-follow 原子发布、SHA-256 完整性 | T08 |
| `crates/interop-ipc` | 本地控制面：u32BE 长度帧（256KiB 上限）、JSON-RPC 单对象、hello 认证、版本协商 | T09 |
| `crates/interop-platform` | 平台侧输入：发现源观察（shape 校验、明文候选如实标注、fake 源）；只读平台 probe/radio（T12） | T11 |
| `crates/interop-testkit` | fake clock、确定性分片/重排、fixture hash 登记、run manifest 分级、脱敏检查 | T14 |
| `adapters/standalone-host` | standalone 显式 policy（profile 默认关闭、60s TTL、只签发 Entry scope） | T07 |
| `apps/interopd` | headless daemon 骨架：UDS 0600 + 帧循环（Windows pipe 载体后续） | T09 |
| `apps/interop-cli`（bin `xinterop`） | 诊断 CLI：`xinterop doctor [--json]` 输出只读平台报告 | T12 |
| `workers/mock-provider` | T13 测试 worker（真实子进程 + 最小行协议；canary/exec/崩溃/listener 模式） | T13 |

`interop-runtime` 另含 **worker supervisor**（T13）：profile 来自 `impl/policies/worker-profiles.json`
（binary hash pin、固定参数、环境白名单、有限重启 4 次/5 分钟、关闭无孤儿），并提供 canary
隔离探针——只有 OS 沙箱真的拒绝读取时才报 `platform-sandbox`（本机 macOS 走 `sandbox-exec`），
否则如实报 `process-only` 并限制可发布 profile。worker 隔离测试需要 `target/debug/mock-provider`：
`cargo test --workspace` 会构建全部成员，单独跑某个测试前先 `cargo build --workspace`。

`interop-runtime` 另含 **endpoint registry + 路由**（T11）：去重键含来源与 identity claim，
同名/同 IP/同地址都不合并；地址候选带 TTL 与接口，接口断开即失效；用户 alias 只影响
`presentation_groups` 派生视图，不参与授权；路由检查 purpose/方向、能力与媒体形态、安全策略、
平台状态（拒绝而非降级）。

后续按任务需要创建（docs/04 §2：不一次创建所有空 crate）：
阶段 3 的媒体/协议 worker 与 `adapters/` 集成侧。

## 依赖分层（T01-01）

- **core**（`interop-contract` / `interop-policy` / `interop-file`）：禁止网络栈、
  异步运行时、GUI、Xross 账号栈；只允许基本序列化/标识类型。
- **orchestrator**（runtime/ipc/testkit/apps/adapters）：GUI 与 Xross 账号栈仍禁止；
  网络栈（如 tokio）受控允许。
- 所有成员：禁止 git 来源依赖与逃逸 `impl/` 的路径依赖。
- 工具链锁 `rust-toolchain.toml`：stable 具体版本；升版必须显式改对应断言。

## 构建与测试

```sh
cargo test --manifest-path impl/Cargo.toml --workspace
```
