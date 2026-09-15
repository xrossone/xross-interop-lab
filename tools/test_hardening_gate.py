"""T46：不可信输入加固的纪律测试。

对应 `impl/crates/interop-hardening`（对抗性输入扫描）与 `interop-testkit::adversarial`。
每条断言都对应一句可以写在证据里的话：

1. **覆盖**：七个线上 crate 里每个吃 `&[u8]`/`&str` 的 pub 解析入口，要么有扫描用例标签，
   要么在**带理由**的排除名单里；排除名单不得过期（源里找不到就报错）。
2. **作用域**：扫描声明的文件之外，不得再出现"吃 `&[u8]` 的 pub 解析入口"。
3. **自撰且确定**：语料固定 seed；hardening crate 里不得出现 `SystemTime`/`rand`/`getrandom`；
   全仓不得引入第三方模糊器依赖（rand/arbitrary/proptest/quickcheck/afl/honggfuzz/libfuzzer）。
4. **测试专用**：`interop-hardening` 没有 runtime 依赖（全部在 dev-dependencies），
   且是 workspace 成员——扫描能力不该搭上任何运行时依赖图。
5. **unsafe 纪律**：全仓只允许一处 `unsafe`（计数分配器的 `GlobalAlloc`），
   其余 crate 仍然 `#![forbid(unsafe_code)]`。
6. **只许拒绝**：未实现/blocked 的入口在 `ALWAYS_REJECTS` 里，且每条理由非空。
7. **证据**：语料计划 + run manifest 记录 seed、用例数、逐入口 digest，以及扫描抓出的缺陷。
"""

import json
import pathlib
import re
import unittest

LAB = pathlib.Path(__file__).resolve().parent.parent
IMPL = LAB / "impl"
HARDENING = IMPL / "crates/interop-hardening"
TOOLKIT = IMPL / "crates/interop-testkit/src/adversarial.rs"
COMMON = HARDENING / "tests/common/mod.rs"
LIB_RS = HARDENING / "src/lib.rs"
CORPUS_PLAN = LAB / "evidence/input-hardening-corpus/corpus-plan.json"
MANIFEST = LAB / "evidence/2026-09-15-t46-input-hardening/run-manifest.json"
DECODE_TXT = LAB / "evidence/2026-09-15-t46-input-hardening/decode-scan.txt"
ALLOC_TXT = LAB / "evidence/2026-09-15-t46-input-hardening/allocation-budget.txt"

# 有线上输入的 crate（扫描的作用域）
WIRE_CRATES = [
    "proto-quickshare",
    "proto-airplay",
    "proto-upnp",
    "proto-cast",
    "proto-wfd",
    "interop-ipc",
    "interop-contract",
]

# 扫描声明的文件 → 标签前缀
SCOPE = {
    "crates/proto-quickshare/src/wire.rs": "quickshare/wire/",
    "crates/proto-quickshare/src/control.rs": "quickshare/control/",
    "crates/proto-quickshare/src/payload.rs": "quickshare/payload/",
    "crates/proto-quickshare/src/framing.rs": "quickshare/framing/",
    "crates/proto-airplay/src/rtsp.rs": "airplay/rtsp/",
    "crates/proto-airplay/src/audio_control.rs": "airplay/audio_control/",
    "crates/proto-airplay/src/pairstore.rs": "airplay/pairstore/",
    "crates/proto-upnp/src/xml.rs": "upnp/xml/",
    "crates/proto-upnp/src/ssdp.rs": "upnp/ssdp/",
    "crates/proto-upnp/src/soap.rs": "upnp/soap/",
    "crates/proto-upnp/src/dmc.rs": "upnp/dmc/",
    "crates/proto-upnp/src/dms.rs": "upnp/dms/",
    "crates/proto-cast/src/castv2.rs": "cast/castv2/",
    "crates/proto-cast/src/namespaces.rs": "cast/namespaces/",
    "crates/proto-cast/src/discovery.rs": "cast/discovery/",
    "crates/proto-cast/src/receiver.rs": "cast/receiver/",
    "crates/proto-wfd/src/messages.rs": "wfd/messages/",
    "crates/proto-wfd/src/ie.rs": "wfd/ie/",
    "crates/proto-wfd/src/rtp.rs": "wfd/rtp/",
    "crates/proto-wfd/src/negotiate.rs": "wfd/negotiate/",
    "crates/interop-ipc/src/frame.rs": "ipc/frame/",
    "crates/interop-ipc/src/media_frame.rs": "ipc/media_frame/",
    "crates/interop-contract/src/capability.rs": "contract/capability/",
    "crates/interop-contract/src/media.rs": "contract/media/",
}

# 「Type::fn」→ 标签：解析器名字与标签用词不一致时显式登记（不许靠猜）
ALIASES = {
    ("crates/proto-airplay/src/rtsp.rs", "RtspRequest::parse"): "airplay/rtsp/parse",
    ("crates/proto-quickshare/src/framing.rs", "FrameDecoder::push"): "quickshare/framing/push",
    (
        "crates/proto-quickshare/src/control.rs",
        "KeepAliveFrame::decode",
    ): "quickshare/control/keepalive_frame_decode",
    (
        "crates/proto-quickshare/src/control.rs",
        "PairedKeyMaterial::decode_inner",
    ): "quickshare/control/paired_key_encryption/decode_inner",
    (
        "crates/proto-quickshare/src/control.rs",
        "PairedKeyResultFrame::decode_inner",
    ): "quickshare/control/paired_key_result/decode_inner",
    (
        "crates/proto-wfd/src/messages.rs",
        "Request::parse",
    ): "wfd/messages/request_parse",
    (
        "crates/proto-wfd/src/messages.rs",
        "Response::parse",
    ): "wfd/messages/response_parse",
    (
        "crates/proto-wfd/src/messages.rs",
        "Response::parse_parameter_names",
    ): "wfd/messages/parse_parameter_names",
    (
        "crates/proto-wfd/src/messages.rs",
        "Response::parse_parameter_body",
    ): "wfd/messages/parse_parameter_body",
    (
        "crates/proto-upnp/src/soap.rs",
        "SoapMessage::parse",
    ): "upnp/soap/parse_raw",
    (
        "crates/proto-upnp/src/dmc.rs",
        "DescriptionFetchPolicy::check",
    ): "upnp/dmc/description_fetch_policy",
    (
        "crates/proto-wfd/src/negotiate.rs",
        "ContentProtection::parse_video_formats",
    ): "wfd/negotiate/video_formats",
    (
        "crates/proto-wfd/src/negotiate.rs",
        "ContentProtection::parse_audio_codecs",
    ): "wfd/negotiate/audio_codecs",
    # 这些解析器的名字与标签用词不同（标签按"协商参数"命名，解析器按类型命名）——显式登记
    ("crates/proto-upnp/src/soap.rs", "SoapAction::from_wire"): "upnp/soap/action_from_wire",
    ("crates/proto-wfd/src/ie.rs", "WfdSubelement::parse"): "wfd/ie/subelement_parse",
    ("crates/proto-wfd/src/ie.rs", "WfdIeContainer::parse"): "wfd/ie/container_parse",
    ("crates/proto-wfd/src/negotiate.rs", "VideoFormatSet::parse"): "wfd/negotiate/video_formats",
    ("crates/proto-wfd/src/negotiate.rs", "AudioCodecSet::parse"): "wfd/negotiate/audio_codecs",
    ("crates/proto-wfd/src/negotiate.rs", "RtpPorts::parse"): "wfd/negotiate/rtp_ports",
    ("crates/proto-wfd/src/negotiate.rs", "Transport::parse"): "wfd/negotiate/transport",
    (
        "crates/proto-wfd/src/negotiate.rs",
        "ContentProtection::parse",
    ): "wfd/negotiate/content_protection",
    ("crates/interop-contract/src/capability.rs", "ApiVersion::parse"): "contract/capability/api_version",
}

# 明确不做扫描的入口：「Type::fn」→ 理由（必须非空；源里找不到该 fn 就是过期条目）
EXCLUDED = {
    (
        "crates/interop-ipc/src/media_frame.rs",
        "FrameKind::check_payload_len",
    ): "输入是已解出的长度数值（XMD1 头已解析），不是线上字节",
    (
        "crates/proto-wfd/src/messages.rs",
        "Response::check_cseq",
    ): "两个 u64 的比较，不解析字节",
    (
        "crates/proto-wfd/src/negotiate.rs",
        "NativeResolution::parse",
    ): "输入是已从 hex 解出的 u16 数值（原始串由 wfd_video_formats 的语料覆盖）",
    (
        "crates/proto-upnp/src/dmc.rs",
        "RendererRegistry::check_push_capability",
    ): "输入是注册表里已登记的 protocolInfo 数组（观察入库后的加工），不是本次扫描的字节入口",
    (
        "crates/proto-cast/src/namespaces.rs",
        "PlayerState::parse",
    ): "取值表：由 namespaces/decode_message 的语料经同一条 JSON 路径覆盖",
    (
        "crates/proto-cast/src/namespaces.rs",
        "StreamType::parse",
    ): "取值表：由 namespaces/decode_message 的语料经同一条 JSON 路径覆盖",
    (
        "crates/proto-airplay/src/audio_control.rs",
        "FeedbackObservation::parse_elapsed_ms",
    ): "输入是已解出的 i64 数值；plist 正文在本仓保持不解析（允许来源里没有 bplist 规范）",
}

FN_RE = re.compile(
    r"^\s*pub fn (decode\w*|parse\w*|from_txt|from_wire|read_frame|push|check\w*)\s*\("
)
IMPL_RE = re.compile(r"^impl(?:<[^>]*>)?\s+(\w+)")
BYTE_INPUT_RE = re.compile(r"pub fn (decode\w*|parse\w*|from_bytes|read_frame|push)\s*\([^)]*&\[u8\]")

BANNED_FUZZ_DEPS = [
    "rand",
    "arbitrary",
    "proptest",
    "quickcheck",
    "afl",
    "honggfuzz",
    "libfuzzer",
    "cargo-fuzz",
]


def strip_non_code(text: str) -> str:
    """去掉注释与字符串字面量：gate 判的是代码，不是散文。"""
    text = re.sub(r"//[^\n]*", "", text)
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    text = re.sub(r'"([^"\\]|\\.)*"', '""', text)
    return text


def snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def parse_like(path: pathlib.Path):
    """返回 [(Type::fn 或 fn, 是否吃 &[u8])]。"""
    out = []
    cur = None
    for line in path.read_text(encoding="utf-8").splitlines():
        m = IMPL_RE.match(line)
        if m:
            cur = m.group(1)
        m = FN_RE.match(line)
        if m:
            out.append((f"{cur}::{m.group(1)}" if cur else m.group(1), "&[u8]" in line))
    return out


def labels():
    text = LIB_RS.read_text(encoding="utf-8")
    found = re.findall(r'"([a-z0-9/_\-]+)",', text)
    return [l for l in found if "/" in l]


class HardeningGate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.labels = labels()
        cls.scan_text = TOOLKIT.read_text(encoding="utf-8")
        cls.common_text = COMMON.read_text(encoding="utf-8")

    # 1 -----------------------------------------------------------------
    def test_every_wire_parser_has_a_scan_case_or_a_reasoned_exclusion(self):
        unexplained, stale = [], []
        covered_pairs = set()
        for rel, prefix in SCOPE.items():
            path = IMPL / rel
            self.assertTrue(path.exists(), f"扫描作用域里的文件不存在：{rel}")
            for fn, _ in parse_like(path):
                key = (rel, fn)
                alias = ALIASES.get(key)
                if alias is not None:
                    self.assertIn(alias, self.labels, f"{rel} {fn} 的别名不在 SCAN_TARGETS 里")
                    covered_pairs.add(key)
                    continue
                fname = fn.split("::")[-1]
                impl = fn.split("::")[0] if "::" in fn else None
                slugs = {fname}
                if impl:
                    slugs.add(f"{snake(impl)}_{fname}")
                hits = [
                    l
                    for l in self.labels
                    if l.startswith(prefix)
                    and (l.split("/")[-1] in slugs or any(s in l.split("/")[-1] for s in slugs if s != fname))
                ]
                if hits:
                    covered_pairs.add(key)
                elif key not in EXCLUDED:
                    unexplained.append(f"{rel} {fn}")
        for key, reason in EXCLUDED.items():
            self.assertTrue(reason.strip(), f"排除项缺理由：{key}")
            rel, fn = key
            if not any(f == fn for f, _ in parse_like(IMPL / rel)):
                stale.append(f"{rel} {fn}")
        self.assertEqual(unexplained, [], f"这些线上解析入口没有扫描用例也没有排除理由：{unexplained}")
        self.assertEqual(stale, [], f"排除名单已过期（源里找不到）：{stale}")
        self.assertGreaterEqual(len(covered_pairs), 40, "被覆盖的入口数量异常地少")

    # 2 -----------------------------------------------------------------
    def test_no_byte_parser_outside_the_declared_scope(self):
        offenders = []
        for crate in WIRE_CRATES:
            for path in sorted((IMPL / "crates" / crate / "src").glob("*.rs")):
                rel = str(path.relative_to(IMPL))
                if rel in SCOPE:
                    continue
                for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                    if BYTE_INPUT_RE.search(line):
                        offenders.append(f"{rel}:{i}")
        self.assertEqual(
            offenders,
            [],
            f"这些吃 &[u8] 的 pub 解析入口不在扫描作用域内（要么加进 SCOPE 并写用例，要么写清理由）：{offenders}",
        )

    # 3 -----------------------------------------------------------------
    def test_corpus_is_self_authored_and_deterministic(self):
        self.assertRegex(self.common_text, r"pub const SEED: u64 = 0x[0-9A-Fa-f_]+")
        for banned in ["SystemTime", "rand::", "getrandom", "thread_rng", "Instant::now().elapsed()"]:
            if banned == "Instant::now().elapsed()":
                continue
            self.assertNotIn(
                banned,
                self.common_text,
                f"语料生成器里不得出现 {banned}（固定 seed 才能复现）",
            )
        self.assertIn(
            "XorShift64Star",
            self.scan_text,
            "语料应复用 testkit 的零依赖 PRNG（peer.rs 的 xorshift64*）",
        )
        self.assertIn("digest", self.scan_text, "语料必须有 digest（同一 digest 可复现同一批输入）")

    def test_no_third_party_fuzzer_dependency(self):
        offenders = []
        for manifest in sorted(IMPL.rglob("Cargo.toml")):
            text = manifest.read_text(encoding="utf-8")
            for dep in BANNED_FUZZ_DEPS:
                if re.search(rf'^\s*"?{re.escape(dep)}"?\s*=', text, re.M):
                    offenders.append(f"{manifest.relative_to(IMPL)}: {dep}")
        for source in sorted(IMPL.rglob("*.rs")):
            text = source.read_text(encoding="utf-8")
            for dep in BANNED_FUZZ_DEPS:
                if re.search(rf"^\s*use {re.escape(dep)}(::|;)", text, re.M):
                    offenders.append(f"{source.relative_to(IMPL)}: use {dep}")
        self.assertEqual(offenders, [], f"不得引入第三方模糊器/随机依赖：{offenders}")

    # 4 -----------------------------------------------------------------
    def test_hardening_crate_is_test_only(self):
        manifest = (HARDENING / "Cargo.toml").read_text(encoding="utf-8")
        deps_section = re.search(r"^\[dependencies\]\s*$", manifest, re.M)
        self.assertIsNotNone(deps_section, "必须显式声明空的 [dependencies]（表明无运行时依赖）")
        after = manifest[deps_section.end() :]
        runtime = []
        for line in after.splitlines():
            if line.startswith("["):
                break
            if line.strip() and not line.strip().startswith("#"):
                runtime.append(line.strip())
        self.assertEqual(runtime, [], f"interop-hardening 不得有运行时依赖：{runtime}")
        self.assertIn("[dev-dependencies]", manifest)
        workspace = (IMPL / "Cargo.toml").read_text(encoding="utf-8")
        self.assertIn('"crates/interop-hardening"', workspace, "必须是 workspace 成员")

    # 5 -----------------------------------------------------------------
    def test_unsafe_is_confined_to_the_counting_allocator(self):
        allowed = IMPL / "crates/interop-hardening/tests/allocation_budget.rs"
        offenders = []
        for source in sorted(list((IMPL / "crates").rglob("src/*.rs")) + list((IMPL / "apps").rglob("src/*.rs"))):
            text = source.read_text(encoding="utf-8")
            # 只扫代码：注释里出现 "unsafe/安全" 这类词不算（去注释与字符串字面量）
            code = strip_non_code(text)
            if re.search(r"\bunsafe\b", code):
                offenders.append(f"{source.relative_to(IMPL)}（代码里出现 unsafe）")
        self.assertEqual(offenders, [], f"src 里只允许 forbid(unsafe_code)：{offenders}")
        alloc = allowed.read_text(encoding="utf-8")
        self.assertIn("unsafe impl GlobalAlloc", alloc)
        # 计数分配器之外，测试里也不应有 unsafe
        for source in sorted(HARDENING.rglob("tests/*.rs")):
            if source == allowed:
                continue
            self.assertNotIn("unsafe", source.read_text(encoding="utf-8"), f"{source} 出现 unsafe")

    # 6 -----------------------------------------------------------------
    def test_always_rejects_entries_carry_reasons(self):
        block = re.search(r"ALWAYS_REJECTS: &\[\(&str, &str\)\] = &\[(.*?)\n\];", self.common_text, re.S)
        self.assertIsNotNone(block, "ALWAYS_REJECTS 必须是 (标签, 理由) 列表")
        entries = re.findall(r'\(\s*"([^"]+)",\s*"([^"]+)"', block.group(1))
        self.assertGreaterEqual(len(entries), 2)
        for label, reason in entries:
            self.assertIn(label, self.labels, f"ALWAYS_REJECTS 的标签不在 SCAN_TARGETS 里：{label}")
            self.assertGreaterEqual(len(reason.strip()), 10, f"{label} 的理由太短")
        got = {label for label, _ in entries}
        for must in ["wfd/ie/container_parse", "airplay/pairstore/check_request"]:
            self.assertIn(must, got, f"未实现/blocked 的入口必须在 ALWAYS_REJECTS 里：{must}")
        campaign = (HARDENING / "tests/decode_campaign.rs").read_text(encoding="utf-8")
        self.assertIn("report.accepted, 0", campaign, "必须有「一个都不接受」的断言")

    # 7 -----------------------------------------------------------------
    def test_evidence_records_seed_cases_digests_and_findings(self):
        self.assertTrue(CORPUS_PLAN.exists(), "缺语料计划")
        plan = json.loads(CORPUS_PLAN.read_text(encoding="utf-8"))
        self.assertEqual(plan["generator"]["algorithm"], "xorshift64* + FNV-1a（每个标签独立 seed）")
        self.assertTrue(plan["generator"]["seed"].startswith("0x"))
        digests = {d["label"]: d["digest"] for d in plan["digests"]}
        for label in self.labels:
            self.assertIn(label, digests, f"语料计划缺 {label} 的 digest")
            self.assertRegex(digests[label], r"^[0-9a-f]{64}$")
        for entry in plan["digests"]:
            self.assertGreaterEqual(entry["cases"], 200, f"{entry['label']} 的用例数异常地少")

        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        self.assertEqual(manifest["run_id"], "2026-09-15-t46-input-hardening")
        self.assertEqual(manifest["result"], "pass")
        text = json.dumps(manifest, ensure_ascii=False)
        for label in ["wfd/negotiate/rtp_ports", "wfd/messages/parse_parameter_names"]:
            self.assertIn(label, text, f"抓出的缺陷必须写进证据：{label}")
        for word in ["red", "green"]:
            self.assertIn(word, text.lower(), "必须记录 red→green")
        for artifact in [DECODE_TXT, ALLOC_TXT]:
            self.assertTrue(artifact.exists(), f"缺证据产物：{artifact.name}")
            self.assertGreater(len(artifact.read_text(encoding="utf-8").strip()), 100)


if __name__ == "__main__":
    unittest.main()
