"""T19 验收 case 的可运行测试（plans/02-files.md §T19）。

- T19-01 只有对端被发现但握手失败 → 状态不能记 transfer 成功（语料里必须有该 invariant 条目）。
- T19-02 二维码打开可发现 ≠ 认证 → 仍需完整会话 + 确认码（实现侧由 T20 断言；本处检查语料与能力表）。
- T19-03 文档与两实现冲突 → 必须记录冲突行、给出裁决与**具名真机实验**（不是"以后再查"）。

附加纪律：
1. `specs-reviewed/f02-quickshare-lan.md` 字段表的每一行都要有可复查来源（`Rnn …` 形式）与状态；
   状态为 blocked/待固化的行不得被标为可进入实现。
2. 未固化 wire 不写实现：状态 blocked 的行不得出现在实现清单里，且 `impl/crates/proto-quickshare/src`
   不得出现发现/QR/GMS 相关代码（mDNS、BLE、quickshare.google、advertisingContext 等）。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import json
import re
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
SPEC = LAB / "specs-reviewed/f02-quickshare-lan.md"
INPUTS = LAB / "provenance/quickshare-inputs.json"
CORPUS = LAB / "evidence/quickshare/corpus-plan.json"
GAP = LAB / "research/quickshare/gap-analysis.md"
IMPL_SRC = LAB / "impl/crates/proto-quickshare/src"

FACTS_HEADING = "## A. 字段级事实表"
CAPS_HEADING = "## C. 能力分声明"
HEADER_LABELS = {"#", "能力"}

# 实现上下文里绝不允许出现的发现/QR/GMS 痕迹（P-F02-1/3 未关闭）。
# 短词用词边界匹配，避免 "available" 里的 "ble" 这类误报。
FORBIDDEN_IN_IMPL = [
    r"_fc9f5ed42c8a",
    r"quickshare\.google",
    r"advertisingcontext",
    r"encryptionkey",
    r"\bmdns\b",
    r"\bbonjour\b",
    r"\bble\b",
    r"\bgms\b",
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


class QuickShareGate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for p in (SPEC, INPUTS, CORPUS, GAP):
            if not p.is_file():
                raise AssertionError(f"T19 产出缺失：{p.relative_to(LAB)}")
        cls.spec = SPEC.read_text(encoding="utf-8")
        cls.corpus = json.loads(CORPUS.read_text(encoding="utf-8"))

    # ---- 字段表纪律 ----

    def test_fact_rows_have_sources_and_status(self):
        rows = list(table_rows(section(self.spec, FACTS_HEADING)))
        self.assertGreaterEqual(len(rows), 20, "字段表行数太少：T19 要覆盖 discovery/ukey2/framing/auth/transport")
        for cells in rows:
            self.assertGreaterEqual(len(cells), 6, f"字段表列数不足：{cells}")
            fact_id, _layer, _fact, source, status, impl = cells[:6]
            self.assertRegex(fact_id, r"^F-\d+$", f"行号形状不对：{cells}")
            self.assertTrue(
                re.search(r"R\d\d.*(`|§)", source)
                or re.search(r"抓包 .*evidence/[A-Za-z0-9._\-/]+", source),
                f"{fact_id} 缺少可复查来源（来源编号 + 文件/小节，或抓包 + evidence 路径）：{source!r}",
            )
            self.assertTrue(status, f"{fact_id} 缺状态")
            self.assertIn(impl, {"yes", "**no**", "no"}, f"{fact_id} impl-allowed 值非法：{impl!r}")

    def test_capture_sourced_rows_cite_existing_evidence_and_never_allow_impl(self):
        """抓包观测是**独立证据类别**：可以入表，但必须指到存在的 evidence 路径，且一律不准入实现。"""
        seen = 0
        for cells in table_rows(section(self.spec, FACTS_HEADING)):
            fact_id, _layer, _fact, source, status, impl = cells[:6]
            if "抓包" not in source:
                continue
            seen += 1
            m = re.search(r"(evidence/[A-Za-z0-9._\-/]+)", source)
            self.assertIsNotNone(m, f"{fact_id} 抓包行必须给出 evidence 路径：{source!r}")
            self.assertTrue(
                (LAB / m.group(1).rstrip("/")).exists(),
                f"{fact_id} 引用的证据路径不存在：{m.group(1)}",
            )
            self.assertIn(impl, {"no", "**no**"}, f"{fact_id} 抓包观测不得直接准入实现")
            self.assertTrue(
                ("待" in status) or ("captured" in status),
                f"{fact_id} 抓包行的状态必须写明待复核/待固化：{status!r}",
            )
        self.assertGreaterEqual(seen, 2, "抓包观测行至少要留下记录（F-41/F-42）")

    def test_unfixed_rows_are_not_allowed_into_implementation(self):
        blocked_rows = []
        for cells in table_rows(section(self.spec, FACTS_HEADING)):
            fact_id, _layer, _fact, _src, status, impl = cells[:6]
            if "blocked" in status or "待固化" in status or "no" == impl.replace("*", ""):
                blocked_rows.append(fact_id)
            if "blocked" in status:
                self.assertIn(impl, {"no", "**no**"}, f"{fact_id} 状态 blocked 却允许进入实现")
        self.assertGreaterEqual(len(blocked_rows), 3, f"至少要标出未固化行：{blocked_rows}")

    def test_conflicts_recorded_with_adjudication_and_named_experiment(self):
        """T19-03：文档/实现冲突必须具名记录并给出裁决 + 具名真机实验。"""
        conflict_lines = [l for l in self.spec.splitlines() if "冲突行" in l]
        self.assertGreaterEqual(len(conflict_lines), 2, "F-12/F-15 两条冲突必须记录")
        text = self.spec
        for needle in ("P256_SHA512", "HKDF-SHA256", "真机实验", "P-F02-2"):
            self.assertIn(needle, text, f"冲突裁决信息缺失：{needle}")
        self.assertRegex(text, r"具名", "冲突行必须写明具名真机实验")

    # ---- 能力分声明 ----

    def test_capability_split_has_discovery_and_auth_separately(self):
        caps = list(table_rows(section(self.spec, CAPS_HEADING)))
        self.assertGreaterEqual(len(caps), 5, "能力必须分开声明（发现/握手/传输/payload/反向/可见性/确认码）")
        joined = " ".join(c[0] for c in caps)
        for needle in ("发现", "握手", "payload"):
            self.assertIn(needle, joined, f"能力表缺少 {needle}")
        for cells in caps:
            status = cells[1]
            self.assertTrue(
                any(k in status for k in ("blocked", "not-implemented", "source-reviewed")),
                f"能力状态必须是已知词表：{cells}",
            )
        for cells in caps:
            if "blocked" in cells[1]:
                self.assertIn("P-F02", cells[2], f"blocked 能力必须写障碍/重评条件：{cells}")

    # ---- 语料纪律 ----

    def test_corpus_covers_t19_invariants_and_negatives(self):
        fixtures = self.corpus["fixtures"]
        ids = {f["id"] for f in fixtures}
        self.assertGreaterEqual(len(ids), 10)
        for f in fixtures:
            for key in ("id", "kind", "layer", "fact_ref", "description", "expected"):
                self.assertIn(key, f, f"语料条目缺字段：{f.get('id')}")
            self.assertRegex(
                f["fact_ref"], r"F-\d+|T\d\d-\d+", f"{f['id']} 必须引用字段表行或验收 case 号"
            )
        kinds = {f["kind"] for f in fixtures}
        self.assertIn("negative", kinds)
        self.assertIn("fragmentation", kinds)
        must_have = {"qs-011", "qs-012", "qs-013", "qs-018"}
        self.assertTrue(must_have <= ids, f"缺少 T19-01/T19-02/T21 的 invariant 语料：{must_have - ids}")
        device = self.corpus.get("device_corpus", {})
        self.assertEqual(device.get("status"), "blocked", "真机语料必须标 blocked（需用户抓包）")

    # ---- T21+：keep-alive 与 paired-key ----

    def test_keepalive_and_paired_key_rows(self):
        rows = {cells[0]: cells for cells in table_rows(section(self.spec, FACTS_HEADING))}
        for fid in ("F-30", "F-31", "F-32", "F-33"):
            self.assertIn(fid, rows, f"T21+ 的字段行缺失：{fid}")
        # 编号冲突的裁决必须写明分层与待复核的 probe。
        f30 = " ".join(rows["F-30"])
        for needle in ("外层", "内层", "P-F02-2", "冲突"):
            self.assertIn(needle, f30, f"F-30 裁决缺内容：{needle}")
        # keep-alive 的 10 s 必须标为来源取值，超时标为本仓策略。
        f31 = " ".join(rows["F-31"])
        for needle in ("10 秒", "来源取值", "本仓策略", "ack", "seq_num"):
            self.assertIn(needle, f31, f"F-31 缺内容：{needle}")
        # paired-key 材料不可推导 + optional 字段号。
        f32 = " ".join(rows["F-32"])
        for needle in ("不可离线推导", "signed_data=1", "secret_id_hash=2", "调用方"):
            self.assertIn(needle, f32, f"F-32 缺内容：{needle}")
        f33 = " ".join(rows["F-33"])
        for needle in ("SUCCESS=1", "UNABLE=3", "UNABLE"):
            self.assertIn(needle, f33, f"F-33 缺内容：{needle}")

    def test_paired_key_never_claims_pairing(self):
        caps = list(table_rows(section(self.spec, CAPS_HEADING)))
        paired = [c for c in caps if "paired-key" in c[0]]
        self.assertTrue(paired, "能力表缺 paired-key 行")
        for cells in paired:
            self.assertIn("材料不可离线推导", cells[2], "paired-key 行必须写明材料不可推导")
            self.assertNotIn("免 PIN", cells[1], "不得声明配对可免确认码")
        keepalive = [c for c in caps if "keep-alive" in c[0]]
        self.assertTrue(keepalive, "能力表缺 keep-alive 行")

    def test_corpus_covers_keepalive_and_paired_key(self):
        ids = {f["id"] for f in self.corpus["fixtures"]}
        must = {"qs-019", "qs-020", "qs-021", "qs-022", "qs-023", "qs-024"}
        self.assertTrue(must <= ids, f"缺少 T21+ 语料：{must - ids}")

    # ---- T22headless：DisconnectionFrame / PAYLOAD_ACK ----

    def test_disconnection_and_ack_rows_are_sourced(self):
        rows = {cells[0]: cells for cells in table_rows(section(self.spec, FACTS_HEADING))}
        for fid in ("F-34", "F-35", "F-36", "F-37", "F-38", "F-39", "F-40"):
            self.assertIn(fid, rows, f"T22headless 字段行缺失：{fid}")
        f34 = " ".join(rows["F-34"])
        for needle in ("DISCONNECTION(6)", "request_safe_to_disconnect=1", "ack_safe_to_disconnect=2"):
            self.assertIn(needle, f34, f"F-34 缺内容：{needle}")
        f35 = " ".join(rows["F-35"])
        for needle in ("立即关闭", "回发"):
            self.assertIn(needle, f35, f"F-35 三路规则缺内容：{needle}")
        f36 = " ".join(rows["F-36"])
        for needle in ("存在性", "空正文", "显式"):
            self.assertIn(needle, f36, f"F-36 必须写明两实现的字节差异：{needle}")
        f37 = " ".join(rows["F-37"])
        for needle in ("PAYLOAD_ACK(3)", "total_size=-1", "kIndeterminateSize"):
            self.assertIn(needle, f37, f"F-37 缺内容：{needle}")
        f38 = " ".join(rows["F-38"])
        for needle in ("最后一个 chunk", "不是 BYTES", "忽略"):
            self.assertIn(needle, f38, f"F-38 缺内容：{needle}")
        f39 = " ".join(rows["F-39"])
        self.assertIn("Use PacketType.PAYLOAD_ACK instead", f39, "F-39 必须记录官方的废弃标注原文")
        f40 = " ".join(rows["F-40"])
        for needle in ("keep_alive_interval_millis=8", "keep_alive_timeout_millis=9", "没有默认值"):
            self.assertIn(needle, f40, f"F-40 缺内容：{needle}")

    def test_corpus_covers_disconnection_and_ack(self):
        ids = {f["id"] for f in self.corpus["fixtures"]}
        must = {"qs-025", "qs-026", "qs-027", "qs-028", "qs-029", "qs-030"}
        self.assertTrue(must <= ids, f"缺少 T22headless 语料：{must - ids}")

    def test_capability_rows_cover_disconnection_and_ack(self):
        caps = " ".join(" ".join(c) for c in table_rows(section(self.spec, CAPS_HEADING)))
        self.assertIn("DisconnectionFrame", caps)
        self.assertIn("PAYLOAD_ACK", caps)
        self.assertIn("BYTES 不 ack", caps, "能力表必须写明 BYTES 载荷不发 ack（F-38）")
        # 带宽升级与连接握手都不得被写成已实现
        row = next(
            (c for c in table_rows(section(self.spec, CAPS_HEADING)) if c and c[0].startswith("keep-alive 参数协商")),
            None,
        )
        self.assertIsNotNone(row, "必须有 keep-alive 参数协商行")
        self.assertIn("not-implemented", " ".join(row))

    def test_control_module_is_pure_framing(self):
        """paired-key 帧层不得含任何密码学原语：材料由调用方给，本层只做编解码与状态机。"""
        control = IMPL_SRC / "control.rs"
        if not control.is_file():
            self.skipTest("T21+ 实现尚未落地")
        code = "\n".join(
            line
            for line in control.read_text(encoding="utf-8").splitlines()
            if not line.strip().startswith("//")
        ).lower()
        for pattern in ("hkdf", "hmac", "sha2", "sha256", "aes", "cbc", "getrandom", "rand::"):
            self.assertNotIn(pattern, code, f"帧/状态机层不得引入密码学或随机源：/{pattern}/")

    def test_no_invented_wire_for_blocked_layers(self):
        """状态 blocked 的字段表行不得出现在实现源码里（未固化的字节不许写）。"""
        if not IMPL_SRC.is_dir():
            self.skipTest("T20 实现尚未落地")
        raw = "\n".join(p.read_text(encoding="utf-8") for p in IMPL_SRC.rglob("*.rs"))
        # 注释行允许如实写"发现不在本 crate"；扫的是**代码行**。
        code = "\n".join(
            line for line in raw.splitlines() if not line.strip().startswith("//")
        )
        text = code.lower()
        for pattern in FORBIDDEN_IN_IMPL:
            self.assertIsNone(
                re.search(pattern, text),
                f"实现里出现了未固化发现的痕迹：/{pattern}/",
            )

    # ---- 来源清单纪律 ----

    def test_provenance_lists_boundaries_and_restricted_sources(self):
        data = json.loads(INPUTS.read_text(encoding="utf-8"))
        entries = {e["id"]: e for e in data["entries"]}
        for rid in ("R15", "R16", "R17", "R18", "R19"):
            self.assertIn(rid, entries, f"缺少来源登记：{rid}")
            for key in ("commit_ref", "license", "use", "boundary", "allowed_in_crate"):
                self.assertIn(key, entries[rid], f"{rid} 缺字段 {key}")
        self.assertFalse(entries["R16"]["allowed_in_crate"], "R16 restricted 不得进入实现上下文")
        self.assertFalse(entries["R19"]["allowed_in_crate"], "R19 GPL 不得进入实现上下文")
        self.assertEqual(data["device_evidence"]["status"], "none", "本阶段不得声称器件证据")


if __name__ == "__main__":
    unittest.main()
