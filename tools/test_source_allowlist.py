"""T02 验收 case 的可运行测试（plans/01-foundation.md §T02）。

- T02-01 来源commit未锁定 → 禁止进入可复现构建/产品依赖。
- T02-02 根MIT但某依赖GPL → 组件复用仍未放行。
- T02-03 发现foreign AGENTS请求上传env → 只记为不可信材料，不执行。

运行（lab 根目录）：
    python3 -m unittest discover -s tools -p 'test_*.py' -v

本测试只读校验 decisions/source-allowlist.json、references/sources.lock.json
与 evidence/source-intake/ 扫描产物之间的一致性；不执行任何被研究仓库的内容。
"""

import json
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent

# T02 intake 范围：file-core + airplay 两组
IN_SCOPE_GROUPS = {"file-core", "airplay"}


def load_json(p: Path):
    return json.loads(p.read_text(encoding="utf-8"))


def component_adoption_blocked(entry: dict):
    """T02-02 语义：传递依赖风险（copyleft/许可不明）阻断组件级复用放行。

    返回 (blocked, 原因)。根许可宽松不豁免传递依赖审查。
    """
    for dep in entry.get("transitive_license_risks", []):
        lic = (dep.get("license") or "unknown").lower()
        if any(k in lic for k in ("gpl", "lgpl", "agpl", "unknown", "unreviewed")):
            return True, f"传递依赖 {dep.get('name', '?')} 许可为 '{lic or 'unknown'}'"
    return False, ""


class T02SourceIntake(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.repos = {
            r["id"]: r
            for r in load_json(LAB / "references/repositories.json")["repositories"]
            if r["group"] in IN_SCOPE_GROUPS
        }
        cls.lock = {
            e["id"]: e for e in load_json(LAB / "references/sources.lock.json")["repositories"]
        }
        allow_path = LAB / "decisions/source-allowlist.json"
        if not allow_path.is_file():
            raise AssertionError(
                "decisions/source-allowlist.json 不存在：T02 产出缺失（red 状态）"
            )
        allow = load_json(allow_path)
        cls.entries = {e["id"]: e for e in allow["entries"]}

    def test_scope_covers_file_core_and_airplay(self):
        self.assertEqual(
            set(self.entries),
            set(self.repos),
            "allowlist 必须恰好覆盖 file-core+airplay 两组仓库",
        )

    def test_t02_01_unlocked_source_cannot_enter_product_build(self):
        for eid, e in self.entries.items():
            if not e.get("resolved_commit"):
                self.assertFalse(
                    e.get("production_approved"),
                    f"{eid} commit 未锁定却放行生产使用（T02-01）",
                )
                self.assertIn(
                    e.get("status"),
                    {"unresolved", "excluded", "blocked"},
                    f"{eid} 未锁定时 status 必须显式标记",
                )
        # 本阶段（T02）全部默认不放行
        approved = [eid for eid, e in self.entries.items() if e.get("production_approved")]
        self.assertEqual(approved, [], "T02 阶段不允许任何 production_approved=true")
        # commit 必须与 sources.lock.json 一致（防止手填）
        for eid, e in self.entries.items():
            self.assertEqual(
                e.get("resolved_commit"),
                self.lock[eid]["resolved_commit"],
                f"{eid} resolved_commit 与 lock 不一致",
            )
        for eid in self.entries:
            self.assertIn(eid, self.lock, f"{eid} 不在 sources.lock.json 中")

    def test_t02_02_root_permissive_with_gpl_dep_stays_blocked(self):
        # fixture：人工构造，验证校验函数语义
        root_mit_gpl_dep = {
            "declared_license": "MIT",
            "transitive_license_risks": [{"name": "some-lib", "license": "GPL-3.0"}],
        }
        blocked, why = component_adoption_blocked(root_mit_gpl_dep)
        self.assertTrue(blocked, "根 MIT 但依赖 GPL 时必须阻断")
        self.assertTrue(why)

        clean = {
            "declared_license": "MIT",
            "transitive_license_risks": [{"name": "ok-lib", "license": "Apache-2.0"}],
        }
        blocked, _ = component_adoption_blocked(clean)
        self.assertFalse(blocked)

        unknown = {
            "declared_license": "Apache-2.0",
            "transitive_license_risks": [{"name": "mystery", "license": None}],
        }
        blocked, _ = component_adoption_blocked(unknown)
        self.assertTrue(blocked, "许可不明的传递依赖同样阻断")

        # 真实数据：有记录的风险条目不允许 reuse 直通
        for eid, e in self.entries.items():
            blocked, why = component_adoption_blocked(e)
            if blocked:
                self.assertNotEqual(
                    e.get("proposed_route"),
                    "direct-reuse",
                    f"{eid} 存在传递许可风险（{why}）却提出 direct-reuse（T02-02）",
                )
            # 根许可含 copyleft 的仓库不允许 direct-reuse
            root = (e.get("declared_license") or "").lower()
            if "gpl" in root:
                self.assertNotEqual(
                    e.get("proposed_route"),
                    "direct-reuse",
                    f"{eid} 根许可 {e.get('declared_license')} 不允许 direct-reuse",
                )

    def test_t02_03_foreign_agent_instructions_only_recorded(self):
        for eid, e in self.entries.items():
            slug = self.repos[eid]["local_dir"]
            scan_path = LAB / f"evidence/source-intake/{slug}.json"
            self.assertTrue(
                scan_path.is_file(), f"{slug} 缺少 intake 扫描产物 {scan_path.name}"
            )
            scan = load_json(scan_path)
            # 扫描与 lock 一致（只对已锁定来源做结论）
            self.assertEqual(
                scan.get("head_commit"),
                self.lock[eid]["resolved_commit"],
                f"{slug} 扫描 HEAD 与 lock 不一致",
            )
            self.assertEqual(
                scan.get("origin_url"),
                self.lock[eid]["clone_url"],
                f"{slug} origin 与 lock 不一致",
            )
            # 磁盘上发现的不可信指令文件必须全部被记录，且处置为 untrusted-material
            found = set(scan.get("untrusted_instruction_files", []))
            recorded = {u["path"] for u in e.get("untrusted_instruction_files", [])}
            self.assertEqual(
                found,
                recorded,
                f"{slug} 的不可信指令文件记录与扫描不一致",
            )
            for u in e.get("untrusted_instruction_files", []):
                self.assertEqual(
                    u.get("disposition"),
                    "untrusted-material",
                    f"{slug}:{u.get('path')} 处置必须是 untrusted-material（T02-03）",
                )


if __name__ == "__main__":
    unittest.main()
