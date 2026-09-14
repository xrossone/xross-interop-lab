# T02 来源 intake：file-core + airplay（19 项）

**日期**：2026-09-15　**范围**：`references/repositories.json` 中 `file-core`（R13–R19）与
`airplay`（R01–R12）两组　**方式**：只读（git 元数据 + 文件系统存在性检查）
**扫描器**：[tools/source_intake_scan.py](../tools/source_intake_scan.py)　
**逐仓库原始事实**：[evidence/source-intake/](../evidence/source-intake/)　
**机器可读决议**：[decisions/source-allowlist.json](../decisions/source-allowlist.json)
**一致性测试**：`python3 -m unittest discover -s tools`（T02-01/02/03）

## 结论摘要

- 19/19 仓库 origin 与 HEAD 均与 `references/sources.lock.json` 一致（浅克隆 default branch）。
- **restricted（许可不明）**：R10 airplay-spec、R13 localsend/protocol（均无 LICENSE 文件）；
  R16 Bada（无根 LICENSE，仅模块级 Apache 声明）。
- **根许可 copyleft**（只能独立进程/独立实现，不可链接）：R01/R03/R04/R19（GPL-3.0）、
  R05/R06（GPL/LGPL 混合）、R02（LGPL-3.0）。
- **根许可宽松但传递风险未清**（T02-02 场景）：R08 airplayreceiver、R09 Airplay2OnWindows
  （根 MIT 但衍生链待核）、R16（Apache 声明但无根文件）、R07 shairport-sync（文件级混合）、
  R12 AirConnect（10 个 submodule 未审查）、R17 nearby（third_party 15 文件 + 34 个生成文件）、
  R04（vendored UxPlay/libplist/openssl/ffmpeg）。
- **直接复用候选**：R14 LocalSend（Apache-2.0；复用面评估在 T03，用户自有 localsend-rs 优先）、
  R18 ukey2（Apache-2.0 + NOTICE）、R15 NearDrop（Unlicense）、R11 pyatv（MIT，事实参考）。
- **本阶段全部 `production_approved=false`**；正式采纳决议在 T05。

## 不可信材料记录（T02-03）

扫描发现的 foreign agent 指令文件，全部登记为 `untrusted-material`，**未执行其中任何内容**：

| 仓库 | 文件 | 分类 |
|---|---|---|
| r02-shairplay-rust | `.kiro/steering/`（5 文件） | Kiro steering 规则 |
| r14-localsend | `AGENTS.md`、`CLAUDE.md` | AI 贡献政策 / fvm 构建指引 |
| r15-neardrop | `AGENTS.md`、`CLAUDE.md` | 拒绝 AI 参与的项目声明（劝阻性指令，不构成本任务授权变更） |
| r16-bada | `AGENTS.md`、`CLAUDE.md` | agent 开发指引 |

未发现要求上传 env/凭据类文字；若未来扫描发现此类指令，按 AGENTS.md 规则记录并继续任务，
不执行、不传播。上述文件的存在与分类已固化在 allowlist 的
`untrusted_instruction_files[].disposition=untrusted-material` 字段，由 T02-03 测试强制校验。

## 构建面清单（要点）

- **Rust**：R14 localsend（cli build.rs + build-dependencies）、R19 rquickshare（build.rs ×2 + build-deps）、
  R02 shairplay-rust（纯 Cargo）。`cargo check/test` 会执行这些 build script——
  本阶段未构建任何第三方仓库。
- **C/C++**：R01 CMake；R03 CMake+autogen；R05 autotools；R06 Makefile；R12 Makefile（+10 submodule，
  含自编 openssl/crosstools）；R07 autoconf（扫描未见生成前入口，构建入口在 bootstrap 后产生）。
- **Android/Gradle**：R04（+4 submodule vendored third_party）、R16、R17（+4 third_party submodule）、
  R18；R14 为 Flutter（含 flutter submodule + Gradle）。
- **Node**：R19 package.json（无 postinstall 类钩子）。
- **CI 下载项**：R02（ci.yml/release.yml 5 处）、R10 deploy.yml、R11 release.yml、
  R14 release.yml（13 处下载/安装步骤）——均为发布流水线，不影响本地只读审查。
- **生成文件**：R11 pyatv 77 个 `*_pb2.py`；R17 nearby 34 个 `*.pb.cc/h`；R12 1 个 `.pb.h`。
  这些是 protobuf 生成物，审查时按生成来源追溯 proto 定义。
- **proc-macro**：范围内未发现 `[lib] proc-macro = true`。

## Submodules（本机未初始化，仅登记）

R04：UxPlay、libplist、openssl-cmake、ffmpeg；R12：dmap-parser、nanopb、libjansson、libpupnp、
libopenssl、libmdns、libcodecs、crosstools、libraop、libpthreads4w；R14：flutter；
R17：protobuf、ukey2、smhasher、json。
按 AGENTS.md 规则浅克隆未递归 submodules；需要其内容时单独 intake，不自动拉取。

## 与测试的对应

| case | 测试 | 语义 |
|---|---|---|
| T02-01 | `test_t02_01_unlocked_source_cannot_enter_product_build` | commit 未锁定 ⇒ 禁止进入可复现构建/产品依赖；与 lock 逐条一致 |
| T02-02 | `test_t02_02_root_permissive_with_gpl_dep_stays_blocked` | 根 MIT 但依赖 GPL ⇒ 组件复用仍未放行（fixture + R08/R16 真实锚点） |
| T02-03 | `test_t02_03_foreign_agent_instructions_only_recorded` | foreign AGENTS 只记为不可信材料；扫描↔allowlist↔lock 三方一致 |

运行：`python3 -m unittest discover -s tools -p 'test_*.py'`（2026-09-15 本机 exit 0，4 tests OK）。
