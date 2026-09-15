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

    # ---- T34（2026-09-15）：AP1/AP2 音频 profile 与配对存储 ----

    def test_t34_audio_profile_rows_are_sourced(self):
        """音频 profile 的每一行都必须能指回 UxPlay/shaiplay-rust 的具体位置。"""
        facts = section(self.spec, FACTS_HEADING)
        ct_rows = [cells for cells in table_rows(facts) if cells and cells[0].startswith("ct=")]
        self.assertEqual(
            {cells[0] for cells in ct_rows},
            {"ct=1", "ct=2", "ct=4", "ct=8"},
            f"ct 四值表不完整：{ct_rows}",
        )
        for cells in ct_rows:
            self.assertTrue(
                "renderers/audio_renderer.c" in cells[-1],
                f"{cells[0]} 的来源必须落到 caps 定义行：{cells[-1]}",
            )
        packed = [c for c in table_rows(facts) if c and c[0].startswith("0x0004")]
        self.assertTrue(packed, "必须登记 AP2 打包 audioFormat 的 0x00040000 行")
        ssrc = [c for c in table_rows(facts) if c and c[0].startswith("0x0000FACE")]
        self.assertTrue(ssrc, "必须登记 RTP SSRC 魔数行")
        # 打包值与 SSRC 是两套：两行的来源必须指向不同文件
        self.assertIn("codec/alac.rs", packed[0][-1])
        self.assertIn("codec/aac.rs", ssrc[0][-1])
        joined = " ".join(" ".join(c) for c in table_rows(facts))
        for needle in ("103", "120", "130", "audioBufferSize", "OneTimePairingRequired", "bit 9"):
            self.assertIn(needle, joined, f"T34 字段表缺内容：{needle}")
        self.assertIn(
            "not RTP SSRC values",
            joined,
            "必须写明 AP2 打包值与 RTP SSRC 魔数不是一回事（避免把两套编号混用）",
        )

    def test_t34_pair_store_facts_and_endpoints_are_sourced(self):
        facts = section(self.spec, FACTS_HEADING)
        joined = " ".join(" ".join(c) for c in table_rows(facts))
        for needle in (
            "/pair-setup-pin",
            "/pair-verify",
            "/fp-setup",
            "pk,device_id,name",
            "MemoryPairingStore",
            "PairingStore",
            "470",
        ):
            self.assertIn(needle, joined, f"配对存储字段表缺内容：{needle}")
        self.assertIn("TLV8", joined, "必须写明配对**不是** TLV8（A 仓 grep 零命中）")
        # 禁止边界小节必须存在且写明不得引入加密实现（要点列表，不是表格）
        bounds = section(self.spec, "### T34 禁止边界")
        self.assertTrue(bounds, "必须有『T34 禁止边界』小节")
        for needle in ("禁止推断", "不实现", "SRP", "vendor"):
            self.assertIn(needle, bounds, f"禁止边界缺内容：{needle}")

    def test_t34_capability_rows_keep_unimplemented_parts_honest(self):
        caps = section(self.spec, CAPS_HEADING)
        rows = list(table_rows(caps))
        buffered = next((r for r in rows if r and r[0].startswith("AP2 buffered")), None)
        self.assertIsNotNone(buffered, "能力表必须有 AP2 buffered（103）行")
        self.assertIn("not-implemented", buffered[1], f"103 必须标 not-implemented：{buffered}")
        handshake = next((r for r in rows if r and r[0].startswith("配对握手")), None)
        self.assertIsNotNone(handshake, "能力表必须有配对握手行")
        self.assertIn("not-implemented", handshake[1], f"配对握手必须标 not-implemented：{handshake}")
        store = next((r for r in rows if r and r[0].startswith("配对存储")), None)
        self.assertIsNotNone(store, "能力表必须有配对存储行")
        self.assertIn("headless", store[1])
        self.assertIn("不得据此声称设备已认证", store[2], "存储行必须写明它不构成认证")
        fp = next((r for r in rows if r and "/fp-setup" in r[0]), None)
        self.assertIsNotNone(fp, "能力表必须有 /fp-setup 行")
        self.assertIn("vendor-gated", fp[1])

    def test_t34_corpus_covers_profiles_and_store(self):
        ids = {e["id"] for e in self.corpus["entries"]}
        must = {
            "fx-airplay-setup-audio-ap1",
            "fx-airplay-setup-audio-ap1-negative",
            "fx-airplay-audio-format-packed",
            "fx-airplay-audio-ssrc-magic",
            "fx-airplay-stream-type-unsupported",
            "fx-airplay-pairstore-roundtrip",
            "fx-airplay-pairstore-no-auth-claim",
            "fx-airplay-pairing-endpoints-policy",
        }
        self.assertTrue(must <= ids, f"缺少 T34 语料：{must - ids}")
        for e in self.corpus["entries"]:
            if e["id"] in must:
                self.assertEqual(e["authored"], "self", f"{e['id']} 必须自制")
                self.assertFalse(e["device_required"], f"{e['id']} 不是真机条目")

    @staticmethod
    def _code_only(text: str) -> str:
        """去掉字符串字面量与注释，只留下代码——用于判断"是否真的用了某个原语"。

        理由：本仓要在**文案**里写明"不实现 SRP/X25519/Ed25519、不走 ChaCha"（那是诚实边界），
        但不允许在**代码**里引入这些原语的实现或依赖。"""
        text = re.sub(r'//[^\n]*', '', text)           # 行注释
        text = re.sub(r'/\*.*?\*/', '', text, flags=re.S)  # 块注释
        text = re.sub(r'"(?:[^"\\]|\\.)*"', '""', text)   # 字符串字面量
        return text.lower()

    # ---- T36：音频路径控制请求（/audioMode、/feedback） ----

    def test_t36_audio_control_rows_are_sourced(self):
        facts = section(self.spec, FACTS_HEADING)
        joined = " ".join(" ".join(c) for c in table_rows(facts))
        for needle in ("/audioMode", "/feedback", "elapsed_ms", "application/x-apple-binary-plist"):
            self.assertIn(needle, joined, f"T36 字段表缺内容：{needle}")
        # 语义未固化必须写明，且不得把心跳写成已实现
        self.assertIn("待考", joined, "必须保留实测记录的『待考』标注")
        self.assertIn("语义待固化", joined, "必须显式标注语义待固化")
        self.assertIn("不实现", joined, "必须写明不实现心跳语义")
        # 能力表：这一行只能声称形状与观测
        caps = " ".join(" ".join(c) for c in table_rows(section(self.spec, CAPS_HEADING)))
        self.assertIn("/audioMode", caps)
        row = next(
            (c for c in table_rows(section(self.spec, CAPS_HEADING)) if c and "/audioMode" in c[0]),
            None,
        )
        self.assertIsNotNone(row, "能力表必须有音频控制请求行")
        self.assertIn("形状", " ".join(row), f"只能声称形状校验：{row}")
        self.assertIn("待考", " ".join(row), f"必须写明语义待考：{row}")

    def test_t36_corpus_covers_audio_control(self):
        ids = {e["id"] for e in self.corpus["entries"]}
        must = {"fx-airplay-audiomode-shape", "fx-airplay-feedback-shape", "fx-airplay-feedback-observation"}
        self.assertTrue(must <= ids, f"缺少 T36 语料：{must - ids}")

    def test_t36_impl_never_claims_heartbeat_semantics(self):
        """实现里不得把 /feedback 说成心跳或时钟：语义未固化（T36）。"""
        src = LAB / "impl/crates/proto-airplay/src/audio_control.rs"
        if not src.is_file():
            self.skipTest("T36 实现尚未落地")
        text = src.read_text(encoding="utf-8")
        code = "\n".join(
            line for line in text.splitlines() if not line.strip().startswith("//")
        )
        for bad in ("clock", "Clock", "playback_position"):
            self.assertNotIn(bad, code, f"不得据 elapsed_ms 推时钟（T36）：{bad}")
        self.assertIn("待考", text, "实现里必须保留语义待考的标注")

    def test_t34_impl_has_no_crypto_and_no_fairplay(self):
        """T34 的实现面：音频 profile 与配对存储都不得含加密原语的实现或依赖。"""
        src = LAB / "impl/crates/proto-airplay/src"
        if not src.is_dir():
            self.skipTest("proto-airplay 尚未落地")
        primitives = ("srp", "x25519", "curve25519", "ed25519", "sha1", "sha2", "hmac", "aes",
                      "chacha", "poly1305", "playfair", "fairplay")
        files = sorted(src.glob("*.rs"))
        self.assertTrue(files, "crate 必须有源文件")
        for path in files:
            code = self._code_only(path.read_text(encoding="utf-8"))
            for token in primitives:
                self.assertNotIn(
                    token, code, f"{path.name} 的代码里出现加密/厂商材料符号：{token}"
                )
        # T34 的两个模块必须存在，且分别只做解析与登记
        for name, must in (("audio_profile.rs", "parse_packed_audio_format"),
                           ("pairstore.rs", "PairingVerdict")):
            path = src / name
            self.assertTrue(path.is_file(), f"T34 模块缺失：{name}")
            self.assertIn(must, path.read_text(encoding="utf-8"))
        # 登记层的结论枚举不得出现"已认证"这类变体（注释里说明"没有它"当然可以）
        store_src = (src / "pairstore.rs").read_text(encoding="utf-8")
        self.assertIn("KnownButUnverified", store_src)
        self.assertNotIn("Authenticated", self._code_only(store_src))
        # 不得出现面向 /fp-setup 的实现路径
        for path in files:
            code = self._code_only(path.read_text(encoding="utf-8"))
            self.assertNotIn("fp_setup_handle", code)

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
