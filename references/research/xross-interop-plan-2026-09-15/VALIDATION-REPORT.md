# 交付包验证记录

**日期：2026-09-15。范围：本包文档、JSON 清单与本地来源管理辅助工具。**

## 已执行

`python -m unittest discover -s tools/tests -v`：**19 个测试通过，exit code 0**。包括 clone 命令参数/来源/路径校验、默认排除受限组、Bash 语法、读取临时合成 Git 仓库、origin 与符号链接拒绝、文档清单与任务依赖图检查。

`python tools/validate_pack.py`：**PASS，exit code 0**。检查范围是 JSON 一致性、ID 唯一性、每个需求/profile 有任务、任务含验收case、无循环依赖（含条件依赖）、已引用的 R/S 来源存在、相对文件链接可解析、模板明确为 synthetic。

```json
{
  "status": "PASS",
  "repositories": 64,
  "official_sources": 31,
  "profiles": 38,
  "requirements": 56,
  "tasks": 80,
  "acceptance_cases": 278,
  "markdown_documents": 27,
  "local_links_checked": 768,
  "dependency_graph": "acyclic",
  "actual_protocol_tests_run": false,
  "note": "Offline structural validation only; not a safety, license or device-compatibility certification."
}
```

## 测试过程与边界

工具测试包含真实的失败→修复→通过过程：在模块未创建时先观察导入失败；来源清单初次校验发现目录含大写与路径策略不一致，修复为统一小写 slug 后重新运行。测试临时创建本地合成 Git 仓库；未连接真实远端、未 checkout 或构建第三方项目。

这些结果**不证明**任何 AirPlay、Miracast、Quick Share、Google Cast、华为协议或设备已互通。278 个协议/产品验收case是计划中待实现的定义，不是已运行的测试数量。

未执行：参考仓库64项的本地clone/build、安全或完整依赖审计；应用签名/商店审核；Tauri/协议实现编译；Windows/macOS/手机/电视互通；独立版权法律审查；所有外部URL的实时存活检查。当前引用的许可是标注级/初步来源核查，不是生产放行。

压缩包附带 `CHECKSUMS.sha256`，用于检查下载后文件是否变化，不代表第三方真实性或许可认证。

## 测试输出

```text
test_all_includes_groups (test_tools.ClonePlanTests.test_all_includes_groups) ... ok
test_default_excludes_large_and_gated (test_tools.ClonePlanTests.test_default_excludes_large_and_gated) ... ok
test_real_inventory (test_tools.ClonePlanTests.test_real_inventory) ... ok
test_reject_bad_url (test_tools.ClonePlanTests.test_reject_bad_url) ... ok
test_reject_duplicate (test_tools.ClonePlanTests.test_reject_duplicate) ... ok
test_reject_path_escape (test_tools.ClonePlanTests.test_reject_path_escape) ... ok
test_safe_plan_is_no_checkout (test_tools.ClonePlanTests.test_safe_plan_is_no_checkout) ... ok
test_script_shell_syntax_with_special_root (test_tools.ClonePlanTests.test_script_shell_syntax_with_special_root) ... ok
test_unknown_group (test_tools.ClonePlanTests.test_unknown_group) ... ok
test_unverified_requires_extra_flag (test_tools.ClonePlanTests.test_unverified_requires_extra_flag) ... ok
test_missing_is_explicit (test_tools.ResolveLockTests.test_missing_is_explicit) ... ok
test_records_commit_and_license_hash_without_build (test_tools.ResolveLockTests.test_records_commit_and_license_hash_without_build) ... ok
test_symlink_escape_rejected (test_tools.ResolveLockTests.test_symlink_escape_rejected) ... ok
test_wrong_origin_rejected (test_tools.ResolveLockTests.test_wrong_origin_rejected) ... ok
test_cycle_rejected (test_validate.PackValidationTests.test_cycle_rejected) ... ok
test_duplicate_ids_rejected (test_validate.PackValidationTests.test_duplicate_ids_rejected) ... ok
test_missing_dependency_rejected (test_validate.PackValidationTests.test_missing_dependency_rejected) ... ok
test_real_pack (test_validate.PackValidationTests.test_real_pack) ... ok
test_valid_graph (test_validate.PackValidationTests.test_valid_graph) ... ok

----------------------------------------------------------------------
Ran 19 tests in 0.093s

OK
```
