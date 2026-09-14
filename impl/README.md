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
| `crates/interop-contract` | 领域对象、统一错误、能力声明、JSON 契约 | T01（T06 填充） |

后续按任务需要创建（docs/04 §2：不一次创建所有空 crate）：
`interop-policy`、`interop-runtime`、`interop-file`、`interop-ipc`、
`interop-platform`、`interop-testkit`、`apps/interopd`。

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
