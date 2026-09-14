"""T14 支撑测试：lab 全部 run manifest 必须符合 evidence/run.schema.json。

- kind=simulated 的 run 永远只能 evidence_level=simulated（不可升级，T14-02）
- kind=device 必须携带 device_attestation（指向 device-run 证据 ID）
- commands 必须有 command+exit_code；required 字段齐全

运行（lab 根目录）：python3 -m unittest discover -s tools -p 'test_*.py'
"""

import json
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
SCHEMA = LAB / "evidence/run.schema.json"

REQUIRED = [
    "schema_version",
    "run_id",
    "kind",
    "evidence_level",
    "result",
    "commands",
    "credentials_redacted",
]
KINDS = {"simulated", "device", "manual"}
LEVELS = {
    "catalogued", "source-reviewed", "build-verified", "simulated",
    "device-verified", "release-qualified", "blocked", "manual", "not-run",
}
RESULTS = {"pass", "fail", "blocked", "not-run"}


class RunManifests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
        cls.paths = sorted((LAB / "evidence").glob("*/run-manifest.json"))
        if not cls.paths:
            raise AssertionError("没有任何 run-manifest.json（red 状态）")

    def test_every_manifest_is_schema_conformant(self):
        for p in self.paths:
            doc = json.loads(p.read_text(encoding="utf-8"))
            where = p.parent.name
            for field in REQUIRED:
                self.assertIn(field, doc, f"{where}: 缺 {field}")
            self.assertIn(doc["kind"], KINDS, where)
            self.assertIn(doc["evidence_level"], LEVELS, where)
            self.assertIn(doc["result"], RESULTS, where)
            for i, c in enumerate(doc["commands"]):
                self.assertIn("command", c, f"{where}: commands[{i}]")
                self.assertIn("exit_code", c, f"{where}: commands[{i}]")

    def test_simulated_never_upgraded(self):
        for p in self.paths:
            doc = json.loads(p.read_text(encoding="utf-8"))
            if doc["kind"] == "simulated":
                self.assertEqual(
                    doc["evidence_level"],
                    "simulated",
                    f"{p.parent.name}: simulated run 不得升级（T14-02）",
                )

    def test_device_requires_attestation(self):
        for p in self.paths:
            doc = json.loads(p.read_text(encoding="utf-8"))
            if doc["kind"] == "device":
                self.assertTrue(
                    doc.get("device_attestation"),
                    f"{p.parent.name}: device run 必须携带 device_attestation",
                )

    def test_no_unredacted_secret_shapes(self):
        import re
        # 高置信形状（与 testkit check_redaction 对应；此处为 JSON 文本级 tripwire）
        patterns = [
            ("-----BEGIN" + " PRIVATE KEY-----", "PEM"),
            (r"AKIA[0-9A-Z]{16}", "AWS"),
        ]
        for p in self.paths:
            text = p.read_text(encoding="utf-8")
            for pat, name in patterns:
                self.assertIsNone(
                    re.search(pat, text) if pat.startswith("AKIA") else (pat if pat in text else None),
                    f"{p.parent.name}: 发现 {name} 形状（T14-03 脱敏失败）",
                )


if __name__ == "__main__":
    unittest.main()
