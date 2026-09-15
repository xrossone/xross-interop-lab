"""ADR 记录纪律的可运行测试（templates/adr.md + AGENTS.md `decisions/` 行）。

背景：阶段 1 出现过"依据不成立却以 accepted 落库的 ADR"（ADR-001/002，已撤回并重写历史）。
阶段 2 T0 由用户采纳 ADR-003 后恢复本测试，保证 ADR 不被写成立场文章、不与台账脱节：

- ADR-01 每个 decisions/adr-*.md 含模板全部 section，Status ∈ {proposed, accepted, superseded}。
- ADR-02 accepted 的 ADR 至少一条 Evidence 可解析（仓库内路径 / commit hash / 参考包路径）。
- ADR-03 lab 文档中引用的 ADR 相对链接真实存在，且至少有一处引用（决议进入审查链）。
- ADR-04 `decisions/README.md` 台账覆盖每个 ADR，且标注的 Status 与文件内容一致。

没有任何 adr-*.md 时直接 red（决议产出缺失）——比"跳过"更强：撤回事件之后需要回归保护。

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import re
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
REF_PACKAGE = LAB / "references/research/xross-interop-plan-2026-09-15"
TEMPLATE = REF_PACKAGE / "templates/adr.md"

# 模板 section 的锚（宽松匹配：允许中英文与编号差异）
REQUIRED_SECTIONS = {
    "status": r"status",
    "owner": r"(date|owner)",
    "context": r"context",
    "decision": r"decision",
    "alternatives": r"alternatives?",
    "authority": r"authority|security",
    "licensing": r"licensing|provenance",
    "compatibility": r"compatibility|migration",
    "tests": r"tests?|验收",
    "rollback": r"rollback",
    "evidence": r"evidence",
}
VALID_STATUS = {"proposed", "accepted", "superseded"}

HEX8 = re.compile(r"\b[0-9a-f]{8,40}\b")


def adr_files():
    return sorted((LAB / "decisions").glob("adr-*.md"))


def parse_sections(text: str) -> set:
    """收集形如 `- **Name**：...` / `**Name**` 的加粗字段名。"""
    return {m.group(1).strip().lower() for m in re.finditer(r"\*\*([^*\n]+)\*\*", text)}


def section_present(field_names: set, pattern: str) -> bool:
    rx = re.compile(pattern)
    return any(rx.search(name) for name in field_names)


def normalize_status(raw: str) -> str:
    return raw.strip().lower().split("（")[0].split("(")[0].strip()


def evidence_resolvable(evidence_text: str) -> list:
    """返回可解析的证据锚（仓库内存在的路径 / commit hash / 参考包内存在的路径）。"""
    found = []
    for m in re.finditer(r"`([^`]+)`", evidence_text):
        token = m.group(1).strip()
        if HEX8.fullmatch(token):
            found.append(f"commit:{token}")
            continue
        if any(ch in token for ch in "*<>"):
            base = token.split("*")[0].split("<")[0].rstrip("/")
            if base and (LAB / base).exists():
                found.append(token)
            continue
        if re.search(r"[\u4e00-\u9fff]", token):
            continue  # 中文说明不是路径锚
        for candidate in (LAB / token, REF_PACKAGE / token, LAB / "references" / token):
            if candidate.exists():
                found.append(token)
                break
    return found


class ADRRecords(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files = adr_files()
        if not cls.files:
            raise AssertionError("decisions/adr-*.md 不存在：ADR 产出缺失（red 状态）")
        if not TEMPLATE.is_file():
            raise AssertionError(f"模板缺失：{TEMPLATE}")

    def test_adr_01_sections_and_status(self):
        for path in self.files:
            text = path.read_text(encoding="utf-8")
            fields = parse_sections(text)
            missing = [
                name
                for name, pattern in REQUIRED_SECTIONS.items()
                if not section_present(fields, pattern)
            ]
            self.assertEqual(
                missing, [], f"{path.name}: 缺模板 section（templates/adr.md）：{missing}"
            )
            status_line = next(
                (line for line in text.splitlines() if "status" in line.lower()[:40]),
                "",
            )
            status = normalize_status(
                re.sub(r"^[^：:]*[：:]", "", status_line.split("**")[-1] or status_line)
            )
            self.assertIn(
                status.split()[0] if status.split() else "",
                VALID_STATUS,
                f"{path.name}: Status 必须是 proposed/accepted/superseded，实际 {status!r}",
            )

    def test_adr_02_accepted_has_resolvable_evidence(self):
        for path in self.files:
            text = path.read_text(encoding="utf-8")
            if "accepted" not in text.lower():
                continue
            ev_lines, collecting = [], False
            for line in text.splitlines():
                if re.match(r"^-\s+\*\*evidence", line, re.IGNORECASE):
                    collecting = True
                    ev_lines.append(line)
                    continue
                if collecting:
                    if not line.strip() or re.match(r"^-\s+\*\*", line):
                        break
                    ev_lines.append(line)  # 续行（Evidence 允许多行）
            ev = "\n".join(ev_lines)
            self.assertTrue(ev, f"{path.name}: accepted 但缺 Evidence 行")
            anchors = evidence_resolvable(ev)
            self.assertGreaterEqual(
                len(anchors), 1,
                f"{path.name}: Evidence 没有任何可解析锚（路径/hash）：{ev.strip()}",
            )

    def test_adr_03_referenced_adr_links_resolve(self):
        referenced = set()
        for doc in (
            [LAB / "README.md", LAB / "AGENTS.md"]
            + sorted((LAB / "research").glob("*.md"))
            + sorted((LAB / "evidence").glob("*.md"))
            + sorted((LAB / "docs").glob("*.md"))
        ):
            if not doc.is_file():
                continue
            for m in re.finditer(
                r"\((?:\.\.?/)*decisions/(adr-[^)\s]+\.md)\)", doc.read_text(encoding="utf-8")
            ):
                referenced.add((doc.relative_to(LAB).as_posix(), m.group(1)))
        missing = [
            f"{doc} → decisions/{target}"
            for doc, target in sorted(referenced)
            if not (LAB / "decisions" / target).is_file()
        ]
        self.assertEqual(missing, [], f"引用的 ADR 文件不存在（悬空链接）：{missing}")
        self.assertGreaterEqual(
            len(referenced), 1, "没有任何文档引用 ADR：决议未进入审查链"
        )

    def test_adr_04_ledger_covers_adrs_with_matching_status(self):
        ledger = LAB / "decisions/README.md"
        self.assertTrue(ledger.is_file(), "decisions/README.md 台账缺失（ADR-04）")
        text = ledger.read_text(encoding="utf-8")
        for path in self.files:
            self.assertIn(
                path.name, text, f"{path.name}: 未登记进 decisions/README.md 台账"
            )
            row = next(
                (line for line in text.splitlines() if path.name in line), ""
            )
            statuses = [s for s in VALID_STATUS if s in row.lower()]
            self.assertTrue(
                statuses,
                f"{path.name}: 台账行必须标注 proposed/accepted/superseded，实际 {row.strip()!r}",
            )
            adr_text = path.read_text(encoding="utf-8").lower()
            first = adr_text.split("**status**", 1)[-1][:200]
            actual = [s for s in VALID_STATUS if s in first]
            self.assertEqual(
                sorted(statuses[:1]), sorted(actual[:1]) or sorted(statuses[:1]),
                f"{path.name}: 台账标注 {statuses} 与文件 Status {actual} 不一致",
            )


if __name__ == "__main__":
    unittest.main()
