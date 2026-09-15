"""T31 验收 case 的可运行测试（plans/03-casting.md §T31）。

- T31-01 视频能解密但音频未验证 → 两个能力必须分开（不能合并成"支持 AirPlay"）。
- T31-02 FairPlay 材料来源不能说明 → 独立 permissive 产物 blocked，外部合规 provider 仍可研究。
- T31-03 只比较 selftest 不替代真机 → 语料里必须有 device_required 条目且不得宣称 device-verified。

附加纪律：`specs-reviewed/m01-airplay-legacy-mirror.md` 的字段级事实表**每一行都要有可复查来源**
（"来源"列非空）；没有来源的字段必须写"待固化"，不许填猜测值。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import json
import re
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
SPEC = LAB / "specs-reviewed/m01-airplay-legacy-mirror.md"
INPUTS = LAB / "provenance/airplay-inputs.json"
CORPUS = LAB / "evidence/airplay-corpus/corpus-plan.json"
GAP = LAB / "research/airplay/gap-analysis.md"

FACTS_HEADING = "## 字段级事实表"
CAPS_HEADING = "## 能力声明"


def section(text: str, heading: str) -> str:
    """取某个 `## ` 小节到下一个 `## ` 之间的内容。"""
    start = text.find(heading)
    if start < 0:
        return ""
    rest = text[start + len(heading):]
    nxt = rest.find("\n## ")
    return rest if nxt < 0 else rest[:nxt]


HEADER_LABELS = {"项", "步", "能力", "模块", "字段", "检查", "来源"}


def table_rows(block: str):
    """解析 markdown 表格行（跳过表头与分隔行）。"""
    for line in block.splitlines():
        line = line.strip()
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if all(set(c) <= set("-: ") for c in cells):
            continue
        if cells and cells[0] in HEADER_LABELS:
            continue
        yield cells


class AirPlayGate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for p in (SPEC, INPUTS, CORPUS, GAP):
            if not p.is_file():
                raise AssertionError(f"T31 产出缺失：{p.relative_to(LAB)}")
        cls.spec = SPEC.read_text(encoding="utf-8")
        cls.inputs = json.loads(INPUTS.read_text(encoding="utf-8"))
        cls.corpus = json.loads(CORPUS.read_text(encoding="utf-8"))
        cls.gap = GAP.read_text(encoding="utf-8")

    def test_t31_01_video_and_audio_capabilities_are_separate(self):
        caps = section(self.spec, CAPS_HEADING)
        self.assertTrue(caps, "spec 必须有『能力声明』小节")
        video = next((r for r in table_rows(caps) if r and r[0].startswith("视频")), None)
        audio = next((r for r in table_rows(caps) if r and r[0].startswith("音频")), None)
        self.assertIsNotNone(video, "必须有独立的『视频』能力行")
        self.assertIsNotNone(audio, "必须有独立的『音频』能力行")
        v_status, a_status = video[1], audio[1]
        self.assertIn("blocked", v_status, f"视频状态必须如实标 blocked：{v_status}")
        self.assertIn("blocked", a_status, f"音频状态必须如实标 blocked：{a_status}")
        self.assertNotEqual(
            v_status, a_status, "视频与音频的证据状态不同，不得合并成同一句话"
        )
        for row in table_rows(caps):
            if not row:
                continue
            self.assertNotIn(
                "支持可用",
                row[1],
                f"{row[0]}: 能力状态不得写成笼统的『支持可用』",
            )

    def test_t31_02_fairplay_material_is_blocked_but_external_provider_ok(self):
        fairplay = [
            e
            for e in self.inputs["entries"]
            if "fairplay" in e["id"].lower() or "playfair" in e["name"].lower()
        ]
        self.assertTrue(fairplay, "必须登记 FairPlay/PlayFair 来源（R06）")
        for e in fairplay:
            self.assertFalse(e["allowed_in_crate"], "FairPlay 材料不得进入实现上下文")
            self.assertIn("vendor-gated", e["gate"])
        # 独立实现：密钥获取环节 blocked（gap 分析里写明）
        self.assertIn("key setup", self.gap)
        key_row = next(
            (r for r in table_rows(section(self.gap, "## 1. 模块 gap 表")) if r and "key setup" in r[0]),
            None,
        )
        self.assertIsNotNone(key_row, "gap 表必须有 key setup 行")
        close_cell = " ".join(key_row[3:])
        self.assertTrue(
            any(marker in close_cell for marker in ("blocked", "不可关闭", "vendor-gated", "不实现")),
            f"密钥环节必须标 blocked/不可关闭：{close_cell}",
        )
        # 外部合规 provider 仍可研究
        uxplay = next(e for e in self.inputs["entries"] if e["id"] == "R01")
        self.assertIn("compatibility-oracle", uxplay["use"])
        self.assertIn("S1", uxplay["gate"], "外部引擎接入需用户 scope S1")

    def test_t31_03_corpus_requires_real_device_and_never_claims_device_verified(self):
        entries = self.corpus["entries"]
        device = [e for e in entries if e["device_required"]]
        self.assertTrue(device, "语料必须包含真机条目（selftest 不替代真机）")
        for e in device:
            self.assertEqual(e["status"], "blocked", "真机条目在 S1/S2 批准前必须 blocked")
        for e in entries:
            declared = {str(e.get("status", "")), str(e.get("level", ""))}
            self.assertNotIn(
                "device-verified",
                declared,
                f"{e['id']}: 语料不得把 status/level 声明为 device-verified",
            )
        self.assertGreaterEqual(
            len([e for e in entries if e["authored"] == "self"]),
            5,
            "自制 fixture 至少覆盖 discovery/control/pairing/frame 四类",
        )

    def test_spec_field_tables_all_have_resolvable_sources(self):
        facts = section(self.spec, FACTS_HEADING)
        self.assertTrue(facts, "spec 必须有字段级事实表小节")
        checked = 0
        for cells in table_rows(facts):
            if len(cells) < 3:
                continue
            source = cells[-1]
            self.assertTrue(
                source and source not in {"-", "—", "来源"},
                f"字段行缺少来源：{cells}",
            )
            is_known_source = (
                "实测" in source
                or "UxPlay@" in source
                or "shairplay-rust@" in source
                or "待固化" in source
                or "decisions/" in source
                or "计划" in source
            )
            self.assertTrue(
                is_known_source,
                f"来源必须可复查（实测/UxPlay@/shairplay-rust@/待固化/decisions/计划）：{source}",
            )
            checked += 1
        self.assertGreaterEqual(checked, 12, "字段级事实表至少覆盖 12 行")


if __name__ == "__main__":
    unittest.main()
