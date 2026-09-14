"""T04 验收 case 的可运行测试（plans/01-foundation.md §T04）。

- T04-01 只有README写supported → 状态最多 catalogued，不是 device-verified。
- T04-02 只有sender demo可运行 → receiver 保持未验证。
- T04-03 规范有未确定必需握手字段 → 新增 probe 而非 AI 补常量。

dossier.md 必须以 ```json xross-dossier … ``` 机器可读头开始（角色/方向/
证据等级/来源/未知字段/probe），本测试解析该头并与 evidence/index.json、
references/sources.lock.json 交叉校验。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import json
import re
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent

PROFILE_SLUGS = [
    "f01-localsend",
    "f02-quickshare-lan",
    "m01-airplay-legacy-mirror",
    "m05-miracast-wfd",
    "m07-dlna-upnp-av",
    "m08-cast-media-control",
]

# 证据等级阶梯（docs/00 §4）
LEVEL_ORDER = [
    "catalogued",
    "source-reviewed",
    "build-verified",
    "simulated",
    "device-verified",
    "release-qualified",
]
# 只能支撑 catalogued/source-reviewed 的来源种类
DESK_KINDS = {"source-tree", "page-review", "user-observation"}
# 能支撑 simulated/device-verified 的来源种类
RUN_KINDS = {"run-manifest", "device-run"}


# 每种证据可支撑的最高等级（方向等级必须 ≤ 证据中的最强支撑，T04-01/02）
KIND_MAX_LEVEL = {
    "page-review": "source-reviewed",
    "source-tree": "source-reviewed",
    "user-observation": "catalogued",  # unpinned 观察不得升级
    "run-manifest": "device-verified",  # release-qualified 需专门证据，暂无 kind 可支撑
    "device-run": "device-verified",
}

def load_json(p: Path):
    return json.loads(p.read_text(encoding="utf-8"))


def parse_dossier_header(md: str) -> dict:
    m = re.search(r"```json xross-dossier\n(.*?)\n```", md, re.S)
    if not m:
        raise AssertionError("dossier 缺少 ```json xross-dossier 机器可读头")
    return json.loads(m.group(1))


class T04Dossiers(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.lock = {
            e["id"]: e
            for e in load_json(LAB / "references/sources.lock.json")["repositories"]
        }
        idx_path = LAB / "evidence/index.json"
        if not idx_path.is_file():
            raise AssertionError("evidence/index.json 不存在：T04 产出缺失（red 状态）")
        cls.index = {
            e["id"]: e for e in load_json(idx_path)["entries"]
        }
        cls.dossiers = {}
        for slug in PROFILE_SLUGS:
            p = LAB / f"research/{slug}/dossier.md"
            if not p.is_file():
                raise AssertionError(f"{p} 不存在：T04 产出缺失（red 状态）")
            cls.dossiers[slug] = parse_dossier_header(p.read_text(encoding="utf-8"))

    def _refs_of(self, d: dict, direction: str) -> list[str]:
        role = d["directions"][direction]
        return role.get("evidence_refs", [])

    def test_specs_reviewed_present(self):
        for slug in PROFILE_SLUGS:
            p = LAB / f"specs-reviewed/{slug}.md"
            self.assertTrue(p.is_file(), f"缺少 specs-reviewed/{slug}.md")

    def test_sources_pinned_to_lock(self):
        for slug, d in self.dossiers.items():
            for s in d["sources"]:
                if s["id"].startswith("R"):
                    self.assertIn(s["id"], self.lock, f"{slug}: {s['id']} 不在 lock")
                    locked = self.lock[s["id"]]["resolved_commit"] or ""
                    self.assertTrue(
                        locked.startswith(s.get("commit", "")) and len(s.get("commit", "")) >= 12,
                        f"{slug}: {s['id']} commit 前缀与 lock 不一致"
                        f"（dossier={s.get('commit')} lock={locked[:12]}）",
                    )

    def test_t04_01_readme_claim_cannot_be_device_verified(self):
        for slug, d in self.dossiers.items():
            for direction, role in d["directions"].items():
                lvl = role["evidence_level"]
                if lvl not in ("simulated", "device-verified", "release-qualified"):
                    continue
                refs = self._refs_of(d, direction)
                self.assertTrue(
                    refs, f"{slug}/{direction}: 声称 {lvl} 却无任何证据引用"
                )
                for rid in refs:
                    entry = self.index.get(rid)
                    self.assertIsNotNone(
                        entry, f"{slug}/{direction}: 证据 {rid} 未登记在 evidence/index.json"
                    )
                    self.assertIn(
                        entry["kind"],
                        RUN_KINDS,
                        f"{slug}/{direction}: {lvl} 的证据 {rid} 种类是 "
                        f"{entry['kind']}（README/页面审查不能支撑运行级结论，T04-01）",
                    )
                if role.get("capability_status") == "blocked":
                    self.assertTrue(
                        role.get("obstacles"),
                        f"{slug}/{direction}: blocked 必须列障碍与重评条件",
                    )

    def test_t04_02_directions_are_independent(self):
        for slug, d in self.dossiers.items():
            self.assertTrue(
                d["directions"], f"{slug}: 必须按方向（send/receive/control…）单列"
            )
            for direction, role in d["directions"].items():
                refs = self._refs_of(d, direction)
                kinds = [self.index[r]["kind"] for r in refs if r in self.index]
                if not kinds:
                    continue
                max_allowed = max(
                    LEVEL_ORDER.index(KIND_MAX_LEVEL[k]) for k in kinds
                )
                allowed = set(LEVEL_ORDER[: max_allowed + 1])
                self.assertIn(
                    role["evidence_level"],
                    allowed,
                    f"{slug}/{direction}: 证据种类 {kinds} 不足以支撑 "
                    f"{role['evidence_level']}（T04-02：方向独立验证，"
                    "README/页面审查/未固定观察不得升级）",
                )

    def test_t04_03_unknown_fields_require_probes(self):
        for slug, d in self.dossiers.items():
            probe_ids = {p["id"] for p in d.get("probes", [])}
            self.assertTrue(
                d.get("probes"),
                f"{slug}: 无 probe——catalogued 状态至少要有首个可判定 probe",
            )
            for uf in d.get("unknown_fields", []):
                if uf.get("probe") is None:
                    continue  # null = 明确的范围排除项（正文必须已说明边界），不需要 probe
                self.assertIn(
                    uf["probe"],
                    probe_ids,
                    f"{slug}: 未知字段 {uf['field']} 引用的 probe {uf['probe']} 不存在"
                    "（T04-03：未知→probe，不允许 AI 补常量）",
                )
            for p in d.get("probes", []):
                self.assertTrue(p.get("question"), f"{slug}: probe {p['id']} 无问题")
                self.assertTrue(p.get("method"), f"{slug}: probe {p['id']} 无方法")


if __name__ == "__main__":
    unittest.main()
