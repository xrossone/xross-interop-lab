"""T43/M08 gate 的可运行测试（plans/03-casting.md §T43 之前的 gate 纪律）。

- T43-01 TLS/auth 失败 → 不启动播放（语料 cast-012；实现侧由 controller 的 auth seam 断言）；
- T43-02 app launch 失败 → 明确错误、不是 media loading（语料 cast-007）；
- T43-03 停止或会话被 receiver 终止 → 清理 URL 与控制状态（语料 cast-016）；
- T43-04 只 URL 可用 → screen 能力保持 false（语料 cast-017）。

附加纪律：
1. `specs-reviewed/m08-cast-media-control.md` 字段表每行必须带可复查来源与状态；blocked 行不得准入实现。
2. F-18（DeviceAuth 握手）与 F-23（app 注册）不得出现在实现里：实现里不得含证书/注册材料关键词，
   也不得出现屏幕镜像能力声明。
3. heartbeat 周期（10 s/10 s）必须标为来源取值，不是协议常量。
4. R45（restricted）不得进入实现上下文。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import json
import re
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
SPEC = LAB / "specs-reviewed/m08-cast-media-control.md"
INPUTS = LAB / "provenance/m08-inputs.json"
CORPUS = LAB / "evidence/cast/corpus-plan.json"
IMPL_SRC = LAB / "impl/crates/proto-cast/src"

FACTS_HEADING = "## A. 字段级事实表"
CAPS_HEADING = "## C. 能力分声明"
HEADER_LABELS = {"#", "能力"}

# 实现里绝不允许出现的"未固化/越界"痕迹
FORBIDDEN_IN_IMPL = [
    r"deviceauth",           # 认证握手（F-18）不实现
    r"auth_challenge",
    r"certificate",
    r"client_auth",
    r"developer console",    # app 注册（F-23）
    r"screen[-_ ]?mirror",   # T43-04：Cast 不做屏幕镜像
    r"cast-web",             # R45 restricted 来源
]
# 允许出现 auth 词的地方：seam 名称与拒绝理由（必须同行情报"未实现/拒绝/vendor-gated"）
AUTH_DENIAL_MARKERS = ("不实现", "未实现", "拒绝", "vendor-gated", "unavailable", "unsupported")


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


class CastGate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for p in (SPEC, INPUTS, CORPUS):
            if not p.is_file():
                raise AssertionError(f"M08/T43 产出缺失：{p.relative_to(LAB)}")
        cls.spec = SPEC.read_text(encoding="utf-8")
        cls.corpus = json.loads(CORPUS.read_text(encoding="utf-8"))

    # ---- 字段表纪律 ----

    def test_fact_rows_have_sources_and_status(self):
        rows = list(table_rows(section(self.spec, FACTS_HEADING)))
        self.assertGreaterEqual(len(rows), 20, "字段表要覆盖 channel/receiver/media/auth/discovery/compat")
        for cells in rows:
            self.assertGreaterEqual(len(cells), 6, f"字段表列数不足：{cells}")
            fact_id, _layer, _fact, source, status, impl = cells[:6]
            self.assertRegex(fact_id, r"^F-\d+$")
            self.assertTrue(
                re.search(r"R\d\d.*(`|§|\d)", source)
                or re.search(r"P-M\d\d", source)
                or re.search(r"S\d\d", source)
                or (source.startswith("本仓策略") and re.search(r"F-\d+|T43-\d+", source)),
                f"{fact_id} 缺可复查来源或 probe 标记：{source!r}",
            )
            self.assertTrue(
                impl.startswith(("yes", "no", "**no**")), f"{fact_id} impl-allowed 非法：{impl!r}"
            )
            if "blocked" in status:
                self.assertIn("no", impl, f"{fact_id} blocked 却准入实现")

    def test_blocked_rows_and_probes(self):
        blocked = {
            cells[0]
            for cells in table_rows(section(self.spec, FACTS_HEADING))
            if "blocked" in cells[4]
        }
        self.assertGreaterEqual(len(blocked), 3, "真机/边界/发现矩阵必须显式 blocked")
        for needle in ("P-M08-1", "P-M08-2", "P-M08-3", "DeviceAuth", "app ID"):
            self.assertIn(needle, self.spec, f"边界说明缺失：{needle}")

    def test_auth_and_app_id_are_hard_boundaries(self):
        rows = {cells[0]: cells for cells in table_rows(section(self.spec, FACTS_HEADING))}
        f18 = " ".join(rows["F-18"])
        self.assertIn("不实现", f18, "F-18 必须写明认证握手不实现")
        f17 = " ".join(rows["F-17"])
        self.assertIn("不申请、不伪造", f17, "F-17 必须写明不伪造 app ID")

    def test_heartbeat_values_marked_as_source_taken(self):
        rows = {cells[0]: cells for cells in table_rows(section(self.spec, FACTS_HEADING))}
        self.assertIn("不是协议常量", rows["F-09"][2])

    # ---- 能力分声明 ----

    def test_capability_split_no_mirror_claim(self):
        caps = list(table_rows(section(self.spec, CAPS_HEADING)))
        joined = " ".join(c[0] for c in caps)
        for needle in ("CASTV2", "sender 控制会话", "DeviceAuth", "app 注册", "镜像"):
            self.assertIn(needle, joined, f"能力表缺少 {needle}")
        for cells in caps:
            if "镜像" in cells[0]:
                self.assertIn("not-implemented", cells[1], "Cast 不得声明屏幕镜像（T43-04）")
            if "blocked" in cells[1]:
                self.assertIn("P-M08", cells[2], f"blocked 能力必须写重评条件：{cells}")
            if "DeviceAuth" in cells[0] or "app 注册" in cells[0]:
                self.assertIn("not-implemented", cells[1], f"{cells[0]} 不得声明为已实现")

    # ---- 语料 ----

    def test_corpus_covers_t43_cases_and_negatives(self):
        fixtures = self.corpus["fixtures"]
        ids = {f["id"] for f in fixtures}
        for f in fixtures:
            for key in ("id", "kind", "layer", "fact_ref", "description", "expected"):
                self.assertIn(key, f, f"语料条目缺字段：{f.get('id')}")
            self.assertRegex(f["fact_ref"], r"F-\d+")
        self.assertIn("negative", {f["kind"] for f in fixtures})
        must = {"cast-012", "cast-007", "cast-016", "cast-017"}
        self.assertTrue(must <= ids, f"缺少 T43-01..04 的语料：{must - ids}")
        self.assertEqual(self.corpus["device_corpus"]["status"], "blocked")

    def test_corpus_has_no_credentials(self):
        blob = json.dumps(self.corpus, ensure_ascii=False).lower()
        for pattern in ("begin certificate", "private key", "client_auth_certificate"):
            self.assertNotIn(pattern, blob, f"语料里出现凭据类内容：{pattern}")

    # ---- 来源清单 ----

    def test_provenance_registers_facts_only_sources(self):
        data = json.loads(INPUTS.read_text(encoding="utf-8"))
        entries = {e["id"]: e for e in data["entries"]}
        for rid in ("R42", "R43", "R44", "R45"):
            self.assertIn(rid, entries)
            for key in ("commit_ref", "license", "use", "boundary", "allowed_in_crate"):
                self.assertIn(key, entries[rid], f"{rid} 缺字段 {key}")
        for rid in ("R42", "R43", "R44"):
            self.assertTrue(entries[rid]["allowed_in_crate"], f"{rid} 应作为 facts-source 放行")
        self.assertFalse(entries["R45"]["allowed_in_crate"], "R45（restricted）不得进入实现")
        self.assertEqual(data["device_evidence"]["status"], "none")

    # ---- T45：receiver 可行性 gate ----

    def test_t45_receiver_facts_are_sourced_and_verdict_is_honest(self):
        rows = {c[0]: c for c in table_rows(section(self.spec, FACTS_HEADING)) if len(c) >= 5}
        for fid in ("F-26", "F-27", "F-28", "F-29", "F-30", "F-31", "F-32", "F-33", "F-34", "F-35"):
            self.assertIn(fid, rows, f"T45 字段行缺失：{fid}")
            row = " ".join(rows[fid])
            self.assertIn("R42", row, f"{fid} 必须引用 R42 的行级位置")
        f34 = " ".join(rows["F-34"])
        for needle in ("blocked", "stock"):
            self.assertIn(needle, f34, f"F-34 结论缺内容：{needle}")
        # 结论行给出边界，演示路径在 F-35（两者必须分开：结论 blocked ≠ 实现准入）
        self.assertIn("test-root", " ".join(rows["F-35"]))
        # 门禁行不得被标成 yes（不能放行认证材料）：涉及证书/认证的三行必须写明只做拒绝，
        # test-root 行必须写明不得冒充厂商信任
        for fid in ("F-26", "F-27", "F-28"):
            self.assertIn("拒绝", " ".join(rows[fid]), f"{fid} 必须写明只做拒绝")
        self.assertIn("不得冒充", " ".join(rows["F-29"]), "F-29 必须写明 test-root 不得冒充厂商信任")
        # 结论行（F-34）必须是 blocked 且不准入实现；演示路径单独一行（F-35）
        self.assertIn("blocked", " ".join(rows["F-34"]).lower())
        self.assertEqual(rows["F-34"][-1].strip(), "**no**", f"F-34 结论行不得准入实现：{rows['F-34'][-1]}")
        self.assertIn("test-root", " ".join(rows["F-35"]))

    def test_t45_receiver_gate_doc_exists_and_states_the_block(self):
        gate = LAB / "research/cast/receiver-gate.md"
        self.assertTrue(gate.is_file(), "T45 必须产出 research/cast/receiver-gate.md")
        text = gate.read_text(encoding="utf-8")
        for needle in ("T45-01", "T45-02", "T45-03", "blocked", "test-root", "不获取", "不伪造"):
            self.assertIn(needle, text, f"receiver gate 缺内容：{needle}")
        # 不得把 stock 说成可用
        lowered = text.lower()
        for bad in ("stock 可用", "支持 stock", "可以直接投屏到本机"):
            self.assertNotIn(bad, lowered, f"不得声称 stock 可用：{bad}")

    def test_t45_capability_rows_keep_stock_blocked(self):
        caps = " ".join(" ".join(c) for c in table_rows(section(self.spec, CAPS_HEADING)))
        self.assertIn("stock sender", caps, "能力表必须有一行说明 stock sender 连通性")
        row = next(
            (c for c in table_rows(section(self.spec, CAPS_HEADING)) if "stock sender" in " ".join(c)),
            None,
        )
        self.assertIsNotNone(row)
        self.assertIn("blocked", " ".join(row), f"stock sender 行必须标 blocked：{row}")
        receiver_row = next(
            (c for c in table_rows(section(self.spec, CAPS_HEADING)) if c and c[0].startswith("**receiver 侧控制面")),
            None,
        )
        self.assertIsNotNone(receiver_row, "能力表必须有 receiver 侧控制面行")
        self.assertIn("test-root", " ".join(receiver_row), "必须写明只在 test-root 门禁后可用")
        caf = next((c for c in table_rows(section(self.spec, CAPS_HEADING)) if c and "CAF" in c[0]), None)
        self.assertIsNotNone(caf, "能力表必须有 CAF/托管 app 行（来源未固化 → blocked）")
        self.assertIn("blocked", " ".join(caf))

    def test_t45_corpus_covers_receiver_cases(self):
        ids = {f["id"] for f in self.corpus["fixtures"]}
        must = {"cast-018", "cast-019", "cast-020", "cast-021", "cast-022", "cast-023"}
        self.assertTrue(must <= ids, f"缺少 T45 语料：{must - ids}")

    def test_t45_impl_has_no_app_ids_and_no_screen_claim(self):
        """T45 实现纪律：不内置 app id、无镜像能力、无证书材料、门禁默认拒绝。"""
        src = IMPL_SRC / "receiver.rs"
        if not src.is_file():
            self.skipTest("T45 实现尚未落地")
        text = src.read_text(encoding="utf-8")
        code = "\n".join(
            line for line in text.splitlines() if not line.strip().startswith("//")
        ).lower()
        # 已知的 Cast app id 字面量一个都不许出现
        for app_id in ("cc1ad845", "e8c28d3c", "0f5096e8"):
            self.assertNotIn(app_id, code, f"实现里不得内置 app id：{app_id}")
        for pattern in ("certificate", "private key", "begin ", "client_auth"):
            self.assertIsNone(re.search(pattern, code), f"receiver 实现里出现凭据类痕迹：/{pattern}/")
        self.assertIn("vendorgated", code, "必须存在恒拒绝的生产门禁")
        self.assertIn("test-root", code, "演示门禁必须在代码里显式标为 test-root")

    # ---- 实现纪律 ----

    def test_no_auth_or_registration_material_in_implementation(self):
        if not IMPL_SRC.is_dir():
            self.skipTest("T43 实现尚未落地")
        raw = "\n".join(p.read_text(encoding="utf-8") for p in IMPL_SRC.rglob("*.rs"))
        code = "\n".join(
            line for line in raw.splitlines() if not line.strip().startswith("//")
        ).lower()
        for pattern in FORBIDDEN_IN_IMPL:
            self.assertIsNone(re.search(pattern, code), f"实现里出现越界材料：/{pattern}/")
        # 认证相关词只允许出现在 seam 的拒绝路径里
        for line in code.splitlines():
            if "auth" in line and not any(m in line for m in AUTH_DENIAL_MARKERS):
                self.fail(f"实现里出现认证实现痕迹：{line.strip()!r}（F-18）")


if __name__ == "__main__":
    unittest.main()
