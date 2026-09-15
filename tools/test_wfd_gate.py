"""T37/M05 gate 的可运行测试（plans/03-casting.md §T37 之前的 gate 纪律）。

- T37-01 媒体协商：M3/M4 参数体解析与交集选择（语料 wfd-009..wfd-012 覆盖；实现侧由 crate 测试断言）；
- T37-02 WFD IE：**容器 OUI 未固化 → 拒绝构造完整 IE**（语料 wfd-015）；子元素可往返（wfd-014）；
- T37-03 顺序与角色：方法/方向/`CSeq`/`Session` 约束（语料 wfd-002、wfd-018）；
- T37-04 媒体边界：RTP 记账与按本仓策略请求关键帧（语料 wfd-016、wfd-017）。

附加纪律：
1. `specs-reviewed/m05-miracast-wfd.md` 字段表每行必须带可复查来源与状态；blocked 行不得准入实现。
2. F-24/F-25/F-30 的 blocked 内容不得出现在实现里：实现不得含 OUI 字节、平台 P2P API 名与品牌名。
3. HDCP 只允许以“未实现/拒绝”的形式出现（不得出现密钥交换实现）。
4. keep-alive 取值（30 s/25 s）必须标注为来源取值而非规范常量。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import json
import re
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
SPEC = LAB / "specs-reviewed/m05-miracast-wfd.md"
INPUTS = LAB / "provenance/m05-inputs.json"
CORPUS = LAB / "evidence/wfd/corpus-plan.json"
IMPL_SRC = LAB / "impl/crates/proto-wfd/src"

FACTS_HEADING = "## A. 字段级事实表"
CAPS_HEADING = "## C. 能力分声明"
HEADER_LABELS = {"#", "能力"}

# blocked 行禁止进入实现：OUI 字节、平台 P2P API、品牌
FORBIDDEN_IN_IMPL = [
    r"0x0*0*a0*0\b",          # 任何 00:0A:00 形式的 OUI 常量
    r"00[:-]0a[:-]00",
    r"0x506f9a|50[:-]6f[:-]9a",
    r"networkmanager",
    r"wpa_supplicant",
    r"wifip2p",
    r"\bsamsung\b",
    r"\bhuawei\b",
    r"\bsony\b",
]
HDCP_DENIAL_MARKERS = ("不实现", "未实现", "拒绝", "unsupported", "not-implemented")
# HDCP **密钥交换**的实现痕迹（F-22 只允许"解析取值 + 明确拒绝"）：
# 出现这些词说明有人开始写握手/密钥逻辑，而不是在拒绝它。
HDCP_IMPL_MARKERS = ("key", "aes", "cipher", "handshake", "derive", "km(", "rtx", "rx_pair")


def section(text: str, heading: str) -> str:
    start = text.find(heading)
    if start < 0:
        return ""
    rest = text[start + len(heading):]
    nxt = rest.find("\n## ")
    return rest if nxt < 0 else rest[:nxt]


def table_rows(block: str):
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


class WfdGate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for p in (SPEC, INPUTS, CORPUS):
            if not p.is_file():
                raise AssertionError(f"M05/T37 产出缺失：{p.relative_to(LAB)}")
        cls.spec = SPEC.read_text(encoding="utf-8")
        cls.corpus = json.loads(CORPUS.read_text(encoding="utf-8"))

    # ---- 字段表纪律 ----

    def test_fact_rows_have_sources_and_status(self):
        rows = list(table_rows(section(self.spec, FACTS_HEADING)))
        self.assertGreaterEqual(len(rows), 25, "字段表要覆盖 control/payload/discovery/compat 四层")
        for cells in rows:
            self.assertGreaterEqual(len(cells), 6, f"字段表列数不足：{cells}")
            fact_id, _layer, _fact, source, status, impl = cells[:6]
            self.assertRegex(fact_id, r"^F-\d+$")
            # 来源列要么是可复查引用（来源编号 + 文件/行），要么指向 probe，要么是"本仓策略 + 字段表交叉引用"
            self.assertTrue(
                re.search(r"R\d\d.*(`|§|\d)", source)
                or re.search(r"P-M\d\d", source)
                or (source.startswith("本仓策略") and re.search(r"F-\d+", source)),
                f"{fact_id} 缺可复查来源或 probe 标记：{source!r}",
            )
            self.assertTrue(impl.startswith(("yes", "no", "**no**")), f"{fact_id} impl-allowed 非法：{impl!r}")
            if "blocked" in status:
                self.assertIn("no", impl, f"{fact_id} blocked 却准入实现")

    def test_blocked_rows_are_the_unfixed_ones(self):
        blocked = {
            cells[0]
            for cells in table_rows(section(self.spec, FACTS_HEADING))
            if "blocked" in cells[4]
        }
        for fid in ("F-24", "F-25", "F-30"):
            self.assertIn(fid, blocked, f"{fid} 必须显式 blocked")
        for needle in ("P-M05-1", "P-M05-2", "P-M05-3", "OUI"):
            self.assertIn(needle, self.spec, f"blocked 说明缺失：{needle}")

    def test_hdcp_is_refused_not_implemented(self):
        rows = {cells[0]: cells for cells in table_rows(section(self.spec, FACTS_HEADING))}
        hdcp = rows["F-22"]
        self.assertIn("不实现", hdcp[2], "F-22 必须写明 HDCP 握手不实现")
        self.assertIn("yes", hdcp[5], "F-22 只允许“解析并拒绝”这一种实现形态")

    def test_keepalive_values_marked_as_source_taken(self):
        rows = {cells[0]: cells for cells in table_rows(section(self.spec, FACTS_HEADING))}
        self.assertIn("不是规范常量", rows["F-27"][2])

    # ---- 能力分声明 ----

    def test_capability_split(self):
        caps = list(table_rows(section(self.spec, CAPS_HEADING)))
        joined = " ".join(c[0] for c in caps)
        for needle in ("控制面", "媒体协商", "WFD IE", "P2P", "HDCP", "解复用"):
            self.assertIn(needle, joined, f"能力表缺少 {needle}")
        for cells in caps:
            if "blocked" in cells[1]:
                self.assertIn("P-M05", cells[2], f"blocked 能力必须写重评条件：{cells}")
            if "HDCP" in cells[0]:
                self.assertIn("not-implemented", cells[1], "HDCP 不得声明为已实现")

    # ---- 语料 ----

    def test_corpus_covers_t37_cases_and_negatives(self):
        fixtures = self.corpus["fixtures"]
        ids = {f["id"] for f in fixtures}
        for f in fixtures:
            for key in ("id", "kind", "layer", "fact_ref", "description", "expected"):
                self.assertIn(key, f, f"语料条目缺字段：{f.get('id')}")
            self.assertRegex(f["fact_ref"], r"F-\d+")
        self.assertIn("negative", {f["kind"] for f in fixtures})
        must = {"wfd-009", "wfd-012", "wfd-014", "wfd-015", "wfd-016", "wfd-018"}
        self.assertTrue(must <= ids, f"缺少 T37-01..04 的语料：{must - ids}")
        self.assertEqual(self.corpus["device_corpus"]["status"], "blocked")

    def test_corpus_has_no_oui_guess(self):
        blob = json.dumps(self.corpus, ensure_ascii=False).lower()
        for pattern in ("506f9a", "50:6f:9a", "00:0a:00", "0x000a00"):
            self.assertNotIn(pattern, blob, f"语料里出现 OUI 猜测值：{pattern}")

    # ---- 来源清单 ----

    def test_provenance_registers_facts_only_sources(self):
        data = json.loads(INPUTS.read_text(encoding="utf-8"))
        entries = {e["id"]: e for e in data["entries"]}
        for rid in ("R32", "R33", "R34", "R35", "R36", "R37"):
            self.assertIn(rid, entries)
            for key in ("commit_ref", "license", "use", "boundary", "allowed_in_crate"):
                self.assertIn(key, entries[rid], f"{rid} 缺字段 {key}")
        for rid in ("R32", "R33", "R34", "R37"):
            self.assertTrue(entries[rid]["allowed_in_crate"], f"{rid} 应作为 facts-source 放行")
        self.assertFalse(entries["R35"]["allowed_in_crate"], "R35（固件/反编译来源）不得进入实现")
        self.assertFalse(entries["R36"]["allowed_in_crate"], "R36（许可未审计）不得进入实现")
        self.assertEqual(data["device_evidence"]["status"], "none")

    # ---- 实现纪律 ----

    def test_no_blocked_material_in_implementation(self):
        if not IMPL_SRC.is_dir():
            self.skipTest("T37 实现尚未落地")
        raw = "\n".join(p.read_text(encoding="utf-8") for p in IMPL_SRC.rglob("*.rs"))
        code = "\n".join(
            line for line in raw.splitlines() if not line.strip().startswith("//")
        ).lower()
        for pattern in FORBIDDEN_IN_IMPL:
            self.assertIsNone(re.search(pattern, code), f"实现里出现 blocked 材料：/{pattern}/")
        # HDCP 允许出现在解析/拒绝路径里；一旦同行出现密钥/握手类词汇就是越界（F-22）。
        for line in code.splitlines():
            if "hdcp" not in line:
                continue
            if any(m in line for m in HDCP_DENIAL_MARKERS):
                continue
            for marker in HDCP_IMPL_MARKERS:
                if marker in line:
                    self.fail(
                        f"实现里出现 HDCP 密钥/握手痕迹（{marker}）：{line.strip()!r}（F-22）"
                    )


if __name__ == "__main__":
    unittest.main()
