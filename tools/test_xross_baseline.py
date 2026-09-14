"""T03 验收 case 的可运行测试（plans/01-foundation.md §T03）。

- T03-01 主仓已有LocalSend实现 → 先列可复用模块与回归，不直接写替代品。
- T03-02 媒体接口未稳定 → mock host先行、集成列blocked dependency。
- T03-03 旧MVP文档与用户目标冲突 → 按当前用户范围计划，不擅自砍协议。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
只读：git 元数据 + 文件存在性检查；不修改 xross-dev。
"""

import json
import subprocess
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
XROSS_DEV = Path("/Volumes/Portable2TB/ExtDev/xross-dev")


def git(args: list[str]) -> str:
    return subprocess.run(
        ["git", "-C", str(XROSS_DEV)] + args, capture_output=True, text=True, check=True
    ).stdout.strip()


class T03XrossBaseline(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = LAB / "decisions/xross-contract-baseline.json"
        if not path.is_file():
            raise AssertionError(
                "decisions/xross-contract-baseline.json 不存在：T03 产出缺失（red 状态）"
            )
        cls.b = json.loads(path.read_text(encoding="utf-8"))

    def test_baseline_commit_is_anchored_in_history(self):
        actual = git(["rev-parse", "HEAD"])
        pinned = self.b["xross_dev"]["commit"]
        # 基线允许落后于移动中的 HEAD（主仓活跃开发），但必须是可验证的真实历史 commit
        rc = subprocess.run(
            ["git", "-C", str(XROSS_DEV), "merge-base", "--is-ancestor", pinned, "HEAD"],
            capture_output=True,
        ).returncode
        self.assertEqual(
            rc, 0, f"baseline 记录的 commit {pinned[:12]} 不在 xross-dev 历史中"
        )
        if actual != pinned:
            print(
                f"NOTE: xross-dev HEAD 已前移到 {actual[:12]}（baseline 锚定 {pinned[:12]}）；"
                "集成实现前需复核映射"
            )
        self.assertTrue(self.b["xross_dev"]["branch"])

    def test_cited_anchors_exist(self):
        """映射中引用的关键 anchor 文件必须真实存在（防陈旧路径）。"""
        for anchor in self.b.get("anchors", []):
            p = XROSS_DEV / anchor
            self.assertTrue(p.is_file(), f"anchor 不存在: {anchor}")

    def test_t03_01_localsend_reuse_first(self):
        ls = self.b["localsend"]
        self.assertEqual(ls["status"], "reuse-first", "LocalSend 必须先复用，不写替代品")
        self.assertTrue(ls["reusable_modules"], "必须列出可复用模块")
        self.assertTrue(ls["regression_tests"], "必须列出现有回归测试位置")
        self.assertTrue(ls["vendored"]["submodule_path"], "必须记录主仓 vendored 事实")
        self.assertEqual(
            ls["vendored"]["origin"], ls["user_copy"]["origin"], "vendored 与用户副本应为同一 upstream"
        )

    def test_t03_02_media_absent_mock_host_first(self):
        media = self.b["media"]
        self.assertEqual(media["stability"], "absent", "媒体接口现状必须如实记录为 absent")
        integ = media["integration"]
        self.assertEqual(integ["status"], "blocked-dependency", "媒体集成必须标 blocked-dependency")
        self.assertTrue(integ["mock_host_first"], "mock host 必须先行")
        self.assertTrue(media["evidence"], "absent 结论必须附证据")

    def test_t03_03_scope_conflicts_keep_user_scope(self):
        conflicts = self.b["scope_conflicts"]
        self.assertTrue(conflicts, "MVP 文档与用户目标的冲突必须显式记录")
        for c in conflicts:
            self.assertIn(
                c["resolution"],
                {"keep-in-scope", "defer", "pending-user-review"},
                f"{c['id']}：不得擅自砍协议（resolution={c['resolution']}）",
            )
            self.assertEqual(c["scope_source"], "user-goal", "裁决依据必须是用户目标，不是旧 MVP 文档")

    def test_adapter_seam_is_single(self):
        seam = self.b["adapter_seam"]
        self.assertTrue(seam["name"], "必须提出唯一 adapter seam")
        self.assertIn("唯一", seam["rationale"], "rationale 必须说明唯一性（不建平行模型）")


if __name__ == "__main__":
    unittest.main()
