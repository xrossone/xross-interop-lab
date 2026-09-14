"""T05 验收 case 的可运行测试（plans/01-foundation.md §T05）。

- T05-01 GPL C 自动翻译 Rust → 不得默认标 MIT。
- T05-02 socket 包装内部函数 → 需要组合性质审查，不自动放行。
- T05-03 未获得厂商 SDK 分发权 → 只能保留 vendor-gated。

附加（step 4 验证）：无许可结论的 provider 必须被 release feature manifest 拒绝。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import json
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent

PERMISSIVE = {"mit", "apache", "bsd", "unlicense", "zlib"}
COPYLEFT = {"gpl", "lgpl", "agpl"}


def license_class_of(s: str) -> str:
    s = (s or "").lower()
    if any(k in s for k in COPYLEFT):
        return "copyleft"
    if any(k in s for k in PERMISSIVE):
        return "permissive"
    return "unknown"


def route_is_license_sound(entry: dict):
    """T05-01：copyleft 来源的任何派生/翻译，输出不得标宽松许可。"""
    src = license_class_of(entry.get("source_license", ""))
    out = license_class_of(entry.get("output_license", ""))
    if src == "copyleft" and entry.get("derivation", {}).get("translated_or_ported"):
        if out in ("permissive", "unknown"):
            return False, (
                f"{entry.get('id')}: copyleft 来源（{entry.get('source_license')}）"
                f"派生物不得标 {entry.get('output_license')}（T05-01）"
            )
    return True, ""


def production_allowed(entry: dict):
    """汇总放行条件：许可路线结论 + 组合审查 + 非 vendor-gated。"""
    if entry.get("vendor_gated"):
        return False, "vendor-gated：未取得厂商分发权（T05-03）"
    if entry.get("approval", {}).get("status") != "approved":
        return False, "许可路线无结论（approval.status != approved）"
    if entry.get("route") == "worker":
        cr = entry.get("composition_review")
        if not cr or cr.get("status") != "passed":
            return False, "worker 路线缺独立程序组合性质审查（T05-02）"
    if entry.get("production_approved") is not True:
        return False, "production_approved 未显式置 true"
    return True, ""


def build_release_feature_manifest(entries: list[dict]):
    """step 4：无许可结论的 profile 构建被 release feature manifest 拒绝。"""
    approved, rejected = [], []
    for e in entries:
        ok, why = production_allowed(e)
        (approved if ok else rejected).append(
            e if ok else {"id": e.get("id"), "provider": e.get("provider"), "reason": why}
        )
    return {"approved": approved, "rejected": rejected}


class T05ProviderAdoption(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = LAB / "decisions/provider-adoption.json"
        if not path.is_file():
            raise AssertionError(
                "decisions/provider-adoption.json 不存在：T05 产出缺失（red 状态）"
            )
        cls.doc = json.loads(path.read_text(encoding="utf-8"))
        cls.entries = {e["id"]: e for e in cls.doc["entries"]}
        # goal 规定的六个裁决对象
        required = {
            "uxplay", "shairplay-shairplay-rust", "localsend",
            "quickshare-references", "gstreamer", "winrt-miracast",
        }
        missing = required - {e["key"] for e in cls.doc["entries"]}
        if missing:
            raise AssertionError(f"缺少对 {sorted(missing)} 的决议")

    def test_fixture_t05_01_translated_gpl_not_mit(self):
        bad = {
            "id": "X",
            "source_license": "GPL-3.0",
            "derivation": {"translated_or_ported": True},
            "output_license": "MIT",
        }
        ok, why = route_is_license_sound(bad)
        self.assertFalse(ok, "GPL 翻译成 Rust 标 MIT 必须被拒绝")
        good = {
            "id": "Y",
            "source_license": "MIT",
            "derivation": {"translated_or_ported": True},
            "output_license": "MIT",
        }
        ok, _ = route_is_license_sound(good)
        self.assertTrue(ok)

    def test_fixture_t05_02_socket_wrap_needs_composition_review(self):
        e = {
            "id": "W",
            "route": "worker",
            "vendor_gated": False,
            "approval": {"status": "approved"},
            "composition_review": {"status": "pending", "note": "socket 包装内部函数，独立性未证"},
            "production_approved": True,
        }
        ok, why = production_allowed(e)
        self.assertFalse(ok, "组合性质未审查不得自动放行（T05-02）")

    def test_fixture_t05_03_vendor_gated_stays_locked(self):
        e = {
            "id": "V",
            "route": "vendor",
            "vendor_gated": True,
            "approval": {"status": "approved"},
            "production_approved": True,
        }
        ok, why = production_allowed(e)
        self.assertFalse(ok)
        self.assertIn("vendor-gated", why)

    def test_release_manifest_rejects_unapproved(self):
        entries = list(self.entries.values())
        # 现实语义检查：fixture 已在其余测试覆盖，这里验证真实决议表
        manifest = build_release_feature_manifest(entries)
        approved_ids = {e["id"] for e in manifest["approved"]}
        self.assertEqual(
            approved_ids, set(),
            "本阶段所有 provider 都未完成许可放行；release manifest 不得包含任何 approved"
        )
        for r in manifest["rejected"]:
            self.assertTrue(r["reason"], "拒绝必须给理由")

    def test_real_entries_license_consistency(self):
        for e in self.entries.values():
            ok, why = route_is_license_sound(e)
            self.assertTrue(ok, why)
            if e.get("route") == "worker":
                self.assertIn(
                    "composition_review", e,
                    f"{e['id']}: worker 路线必须显式记录组合审查状态（哪怕是 pending）",
                )
            ok, why = production_allowed(e)
            if ok:
                self.assertTrue(e["production_approved"])


if __name__ == "__main__":
    unittest.main()
