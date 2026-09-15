"""T41/T42 M07 gate 的可运行测试（plans/03-casting.md §T41/§T42 之前的 gate 纪律）。

- T41-01 设备描述指向恶意内网目标 → fetch policy 拒绝（语料 dl-008 覆盖；实现侧由 crate 测试断言）；
- T41-02 未知/不支持 codec → **禁止假装 screen mirror**（语料 dl-009 + 能力表里 mirror=not-implemented）；
- T41-03 ContentDirectory 越权 objectID → 拒绝（语料 dl-010）；
- T41-04 stop 播放 → URL lease 按策略撤销（语料 dl-011）；
- T42-01 `SetAVTransportURI` 为 `file://` → 拒绝（语料 dl-019）；
- T42-02 重复 `Stop` → 幂等结束（语料 dl-020）；
- T42-03 live 流 `Seek` 不支持 → 710（语料 dl-021）；
- T42-04 外来 event 订阅 callback 越界 → 拒绝（语料 dl-022）。

附加纪律：
1. `specs-reviewed/m07-dlna-upnp-av.md` 字段表每行必须带可复查来源与状态；blocked 行不得准入实现。
2. 未固化的 F-13/F-14（真实 TV 矩阵/时序）不得出现在实现源码里：实现中不得出现品牌名，
   也不得出现 mirror/screen-mirroring 能力声明。
3. 解析预算（深度/体积）必须标为本仓策略值。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import json
import re
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
SPEC = LAB / "specs-reviewed/m07-dlna-upnp-av.md"
INPUTS = LAB / "provenance/m07-inputs.json"
CORPUS = LAB / "evidence/dlna/corpus-plan.json"
IMPL_SRC = LAB / "impl/crates/proto-upnp/src"

FACTS_HEADING = "## A. 字段级事实表"
CAPS_HEADING = "## C. 能力分声明"
HEADER_LABELS = {"#", "能力"}

# 实现里绝不允许出现的"未固化能力/品牌"痕迹（F-13/F-14 blocked）
FORBIDDEN_IN_IMPL = [
    r"\bsamsung\b",
    r"\blg\b",
    r"\bsony\b",
    r"screen[-_ ]?mirror",
    r"\bmiracast\b",
]


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


class DlnaGate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for p in (SPEC, INPUTS, CORPUS):
            if not p.is_file():
                raise AssertionError(f"M07/T41 产出缺失：{p.relative_to(LAB)}")
        cls.spec = SPEC.read_text(encoding="utf-8")
        cls.corpus = json.loads(CORPUS.read_text(encoding="utf-8"))

    # ---- 字段表纪律 ----

    def test_fact_rows_have_sources_and_status(self):
        rows = list(table_rows(section(self.spec, FACTS_HEADING)))
        self.assertGreaterEqual(len(rows), 12, "字段表至少要覆盖 discovery/control/payload/events/xml")
        for cells in rows:
            self.assertGreaterEqual(len(cells), 6, f"字段表列数不足：{cells}")
            fact_id, _layer, _fact, source, status, impl = cells[:6]
            self.assertRegex(fact_id, r"^F-\d+$")
            # 来源要么是可复查引用（来源编号 + 文件/小节），要么是"由某个 probe 关闭"的 blocked 标记
            self.assertTrue(
                re.search(r"R\d\d.*(`|§|\d)", source)
                or re.search(r"P-M\d\d", source)
                or (
                    source.startswith(("本仓策略", "本仓范围声明"))
                    and re.search(r"F-\d+", source)
                ),
                f"{fact_id} 缺可复查来源或 probe 标记：{source!r}",
            )
            self.assertIn(impl, {"yes", "no", "**no**"}, f"{fact_id} impl-allowed 非法：{impl!r}")
            if "blocked" in status:
                self.assertIn(impl, {"no", "**no**"}, f"{fact_id} blocked 却准入实现")

    def test_blocked_rows_are_the_device_dependent_ones(self):
        blocked = [
            cells[0]
            for cells in table_rows(section(self.spec, FACTS_HEADING))
            if "blocked" in cells[4]
        ]
        self.assertGreaterEqual(len(blocked), 2, "真实 TV 矩阵/时序必须显式 blocked")
        for needle in ("P-M07-1", "protocolInfo", "时序"):
            self.assertIn(needle, self.spec, f"blocked 说明缺失：{needle}")

    def test_parse_budgets_are_marked_as_our_policy(self):
        self.assertIn("本仓策略", self.spec, "解析预算必须标明是本仓策略而非协议常量")
        self.assertRegex(self.spec, r"P-M07-2")

    # ---- 能力分声明 ----

    def test_capability_split_and_no_mirror_claim(self):
        caps = list(table_rows(section(self.spec, CAPS_HEADING)))
        joined = " ".join(c[0] for c in caps)
        for needle in ("SSDP", "DMC", "DMS", "镜像"):
            self.assertIn(needle, joined, f"能力表缺少 {needle}")
        for cells in caps:
            if "镜像" in cells[0]:
                self.assertIn("not-implemented", cells[1], "DLNA 不得声明屏幕镜像能力（T41-02）")
            if "blocked" in cells[1]:
                self.assertIn("P-M07", cells[2], f"blocked 能力必须写重评条件：{cells}")

    # ---- 语料 ----

    def test_corpus_covers_t41_cases_and_negatives(self):
        fixtures = self.corpus["fixtures"]
        ids = {f["id"] for f in fixtures}
        for f in fixtures:
            for key in ("id", "kind", "layer", "fact_ref", "description", "expected"):
                self.assertIn(key, f, f"语料条目缺字段：{f.get('id')}")
            self.assertRegex(f["fact_ref"], r"F-\d+|T41-\d+")
        self.assertIn("negative", {f["kind"] for f in fixtures})
        must = {"dl-008", "dl-009", "dl-010", "dl-011", "dl-012"}
        self.assertTrue(must <= ids, f"缺少 T41-01..04 的语料：{must - ids}")
        self.assertEqual(self.corpus["device_corpus"]["status"], "blocked")

    # ---- T42：renderer 侧 ----

    def test_renderer_fact_rows_and_states(self):
        rows = {cells[0]: cells for cells in table_rows(section(self.spec, FACTS_HEADING))}
        for fid in ("F-15", "F-16", "F-17", "F-18", "F-19", "F-20", "F-21", "F-22"):
            self.assertIn(fid, rows, f"T42 的字段行缺失：{fid}")
        # 状态值/错误码来自来源，不是我们自己编的。
        joined = " ".join(rows["F-15"] + rows["F-16"])
        for needle in ("NO_MEDIA_PRESENT", "PAUSED_PLAYBACK", "710", "711", "712", "701"):
            self.assertIn(needle, joined, f"F-15/F-16 缺少来源固定值：{needle}")

    def test_capability_split_covers_renderer_and_gena(self):
        caps = list(table_rows(section(self.spec, CAPS_HEADING)))
        joined = " ".join(c[0] for c in caps)
        self.assertIn("renderer", joined, "能力表缺 DLNA renderer 行（T42）")
        self.assertIn("GENA 订阅校验", joined, "能力表要把 GENA 的校验与投递分开")
        for cells in caps:
            if cells[0].startswith("GENA 事件回调投递"):
                self.assertIn("not-implemented", cells[1], "事件投递不得声明为已实现")

    def test_corpus_covers_t42_cases(self):
        ids = {f["id"] for f in self.corpus["fixtures"]}
        must = {"dl-019", "dl-020", "dl-021", "dl-022"}
        self.assertTrue(must <= ids, f"缺少 T42-01..04 的语料：{must - ids}")

    def test_renderer_implementation_does_no_network_io(self):
        dmr = IMPL_SRC / "dmr.rs"
        if not dmr.is_file():
            self.skipTest("T42 实现尚未落地")
        code = "\n".join(
            line for line in dmr.read_text(encoding="utf-8").splitlines()
            if not line.strip().startswith("//")
        ).lower()
        for pattern in ("reqwest", "hyper::", "tcpstream", "udpsocket", "std::process", "curl ", "http_client"):
            self.assertNotIn(pattern, code, f"renderer 控制路径不得做网络 I/O：/{pattern}/")

    def test_no_unfixed_capability_in_implementation(self):
        if not IMPL_SRC.is_dir():
            self.skipTest("T41 实现尚未落地")
        raw = "\n".join(p.read_text(encoding="utf-8") for p in IMPL_SRC.rglob("*.rs"))
        code = "\n".join(
            line for line in raw.splitlines() if not line.strip().startswith("//")
        ).lower()
        for pattern in FORBIDDEN_IN_IMPL:
            self.assertIsNone(re.search(pattern, code), f"实现里出现未固化能力/品牌：/{pattern}/")
        # 镜像能力不得被**声明**：允许在"拒绝镜像替代"的说明行里出现（那是 T41-02 要求的拒绝理由）
        denial_markers = ("不提供", "替代", "不假装")
        for line in code.splitlines():
            if re.search(r"\bmirror\b", line) and not any(m in line for m in denial_markers):
                self.fail(f"实现里出现未授权能力的声明：{line.strip()!r}（T41-02）")

    # ---- 来源清单 ----

    def test_provenance_registers_facts_only_sources(self):
        data = json.loads(INPUTS.read_text(encoding="utf-8"))
        entries = {e["id"]: e for e in data["entries"]}
        for rid in ("R46", "R47", "R48", "R49"):
            self.assertIn(rid, entries)
            for key in ("commit_ref", "license", "use", "boundary", "allowed_in_crate"):
                self.assertIn(key, entries[rid], f"{rid} 缺字段 {key}")
        self.assertTrue(entries["R46"]["allowed_in_crate"])
        self.assertTrue(entries["R49"]["allowed_in_crate"])
        self.assertFalse(entries["R47"]["allowed_in_crate"], "Platinum（GPL/商业双轨）不得进入实现")
        self.assertFalse(entries["R48"]["allowed_in_crate"], "gerbera（GPL-2.0）不得进入实现")
        self.assertEqual(data["device_evidence"]["status"], "none")


if __name__ == "__main__":
    unittest.main()
