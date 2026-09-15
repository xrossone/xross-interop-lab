"""脱敏器的自测：用**自撰的合成 pcap** 证明"该保留的保留、该删的删"。

fixture 全部由本文件构造（不来自任何真实抓包）：里面故意埋了设备名、人名、IP、UUID、文件名，
断言脱敏产物里**一个都找不到**，而协议事实（服务类型、TXT 键名、协议常量、端口、SSDP 方法）都在。
"""

import json
import pathlib
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import capture_redact as redact  # noqa: E402

# 故意埋进 fixture 的隐私材料（不得出现在产物里）
PII_DEVICE_NAME = "张三的 MacBook Pro"
PII_INSTANCE = "Living-Room-TV-Of-Zhang"
PII_IP_A = "192.168.7.23"
PII_IP_B = "192.168.7.99"
PII_UUID = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
PII_FILENAME = "家庭照片-2026.zip"


def dns_name(name: str) -> bytes:
    out = b""
    for label in name.split("."):
        if not label:
            continue
        raw = label.encode()  # 长度按**字节**算（中文设备名不是 1 字符 1 字节）
        assert len(raw) <= 63, "DNS 标签上限 63 字节"
        out += bytes([len(raw)]) + raw
    return out + b"\x00"


def mdns_query(name: str, qtype: int = 12) -> bytes:
    header = struct.pack(">HHHHHH", 0, 0, 1, 0, 0, 0)
    return header + dns_name(name) + struct.pack(">HH", qtype, 1)


def mdns_response(records: list[tuple[str, int, bytes]]) -> bytes:
    rr = b""
    for name, rtype, rdata in records:
        rr += dns_name(name) + struct.pack(">HHIH", rtype, 1, 120, len(rdata)) + rdata
    header = struct.pack(">HHHHHH", 0, 0x8400, 0, len(records), 0, 0)
    return header + rr


def txt_rdata(pairs: list[str]) -> bytes:
    out = b""
    for p in pairs:
        b = p.encode()
        out += bytes([len(b)]) + b
    return out


def udp_packet(src: str, dst: str, sport: int, dport: int, payload: bytes) -> bytes:
    def ip(a):
        return bytes(int(x) for x in a.split("."))

    udp = struct.pack(">HHHH", sport, dport, 8 + len(payload), 0) + payload
    ip_hdr = struct.pack(
        ">BBHHHBBH4s4s",
        0x45,
        0,
        20 + len(udp),
        0,
        0,
        64,
        17,
        0,
        ip(src),
        ip(dst),
    )
    eth = b"\x02\x00\x00\x00\x00\x02" + b"\x02\x00\x00\x00\x00\x03" + struct.pack(">H", 0x0800)
    return eth + ip_hdr + udp


def tcp_packet(src: str, dst: str, sport: int, dport: int, payload: bytes, seq: int = 1) -> bytes:
    def ip(a):
        return bytes(int(x) for x in a.split("."))

    tcp = struct.pack(">HHIIBBHHH", sport, dport, seq, 0, 0x50, 0x18, 8192, 0, 0) + payload
    ip_hdr = struct.pack(
        ">BBHHHBBH4s4s", 0x45, 0, 20 + len(tcp), 0, 0, 64, 6, 0, ip(src), ip(dst)
    )
    eth = b"\x02\x00\x00\x00\x00\x02" + b"\x02\x00\x00\x00\x00\x03" + struct.pack(">H", 0x0800)
    return eth + ip_hdr + tcp


def write_pcap(path: pathlib.Path, frames: list[bytes]) -> None:
    header = struct.pack("<IHHiIII", 0xA1B2C3D4, 2, 4, 0, 0, 262144, 1)
    out = bytearray(header)
    ts = 1_760_000_000
    for i, frame in enumerate(frames):
        out += struct.pack("<IIII", ts + i, 0, len(frame), len(frame)) + frame
    path.write_bytes(bytes(out))


class RedactionTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        root = pathlib.Path(cls.tmp.name)
        cls.pcap = root / "synthetic.pcap"
        frames = [
            # mDNS 查询：服务类型是协议事实，必须保留
            udp_packet(PII_IP_A, "224.0.0.251", 5353, 5353, mdns_query("_airplay._tcp.local")),
            # mDNS 应答：实例名 + TXT（含设备名与人名 → 必须脱敏；含协议常量 → 必须保留）
            udp_packet(
                PII_IP_B,
                "224.0.0.251",
                5353,
                5353,
                mdns_response(
                    [
                        (
                            "_airplay._tcp.local",
                            12,
                            dns_name(f"{PII_INSTANCE}._airplay._tcp.local"),
                        ),
                        (
                            f"{PII_INSTANCE}._airplay._tcp.local",
                            33,
                            struct.pack(">HHH", 0, 0, 7000)
                            + dns_name(f"{PII_INSTANCE}.local"),
                        ),
                        (
                            f"{PII_INSTANCE}._airplay._tcp.local",
                            16,
                            txt_rdata(
                                [
                                    "txtvers=1",
                                    "features=0x5A7FFFF7,0x1E",
                                    f"deviceid={PII_UUID}",
                                    f"fn={PII_DEVICE_NAME}",
                                ]
                            ),
                        ),
                    ]
                ),
            ),
            # 设备名也出现在自己的 SRV/PTR target 里
            udp_packet(
                PII_IP_B,
                "224.0.0.251",
                5353,
                5353,
                mdns_response(
                    [
                        (
                            "_quickshare._tcp.local",
                            12,
                            dns_name(f"{PII_DEVICE_NAME}._quickshare._tcp.local"),
                        ),
                        (
                            "_quickshare._tcp.local",
                            33,
                            struct.pack(">HHH", 0, 0, 4321) + dns_name("nearby.local"),
                        ),
                        (
                            "_quickshare._tcp.local",
                            16,
                            # 自己的地址：脱敏后只保留"是否等于广播者地址"这个布尔
                            txt_rdata([f"ipv4={PII_IP_B}", "f=5200"]),
                        ),
                    ]
                ),
            ),
            # SSDP NOTIFY：只保留方法与头名
            udp_packet(
                PII_IP_B,
                "239.255.255.250",
                1900,
                1900,
                (
                    "NOTIFY * HTTP/1.1\r\n"
                    "HOST: 239.255.255.250:1900\r\n"
                    "NT: urn:schemas-upnp-org:device:MediaRenderer:1\r\n"
                    f"USN: uuid:{PII_UUID}::urn:schemas-upnp-org:device:MediaRenderer:1\r\n"
                    f"LOCATION: http://{PII_IP_B}:49152/{PII_FILENAME}\r\n"
                    "SERVER: Linux/6.8 UPnP/1.0 Demo/1.0\r\n\r\n"
                ).encode(),
            ),
            # TCP：首批载荷形状保留（模拟 UKEY2 ClientInit 的字节形状）
            tcp_packet(PII_IP_A, PII_IP_B, 51234, 4321, bytes.fromhex("08011001" + "aa" * 60)),
            tcp_packet(PII_IP_B, PII_IP_A, 4321, 51234, bytes.fromhex("120a0801" + "bb" * 20)),
        ]
        write_pcap(cls.pcap, frames)
        cls.summary, _ = redact.run(cls.pcap, "synthetic")
        cls.text = redact.render_text(cls.summary)
        cls.raw_json = json.dumps(cls.summary, ensure_ascii=False)
        cls.report = redact.render_report(cls.summary)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_pii_is_absent_everywhere(self):
        haystack = self.raw_json + self.text
        for secret in [
            PII_DEVICE_NAME,
            PII_INSTANCE,
            PII_IP_A,
            PII_IP_B,
            PII_UUID,
            PII_FILENAME,
            "Zhang",
            "张三",
            "Living-Room",
        ]:
            self.assertNotIn(secret, haystack, f"脱敏产物里泄露了：{secret}")
        # 连原始帧的字节也不该以十六进制形式出现在产品里
        self.assertNotIn(b"\x02\x00\x00\x00\x00\x02".hex(), haystack)

    def test_protocol_facts_are_kept(self):
        self.assertIn("_airplay._tcp.local", self.raw_json)
        self.assertIn("_quickshare._tcp.local", self.raw_json)
        self.assertIn("features=0x5A7FFFF7,0x1E", self.text)  # 人类可读摘要里保留常量
        self.assertIn("txtvers=1", self.text)
        services = {s["service_type"]: s for s in self.summary["mdns"]["services"]}
        self.assertEqual(services["_airplay._tcp.local"]["port"], 7000)
        self.assertEqual(services["_quickshare._tcp.local"]["port"], 4321)
        self.assertIn("deviceid", services["_airplay._tcp.local"]["txt_keys"])
        self.assertIn("fn", services["_airplay._tcp.local"]["txt_keys"])
        self.assertIn("fn", services["_airplay._tcp.local"]["txt_redacted"])
        self.assertNotIn("fn", services["_airplay._tcp.local"]["txt_constants"])
        self.assertEqual(
            services["_airplay._tcp.local"]["txt_constants"]["features"], "0x5A7FFFF7,0x1E"
        )
        self.assertTrue(any(q["name"] == "_airplay._tcp.local" for q in self.summary["mdns"]["questions"]))

    def test_redacted_values_carry_shape_not_content(self):
        svc = [s for s in self.summary["mdns"]["services"] if s["service_type"] == "_airplay._tcp.local"][0]
        fn = svc["txt_redacted"]["fn"]
        self.assertEqual(fn["len"], len(PII_DEVICE_NAME))
        self.assertEqual(fn["class"], "text")
        did = svc["txt_redacted"]["deviceid"]
        self.assertEqual(did["class"], "uuid")
        self.assertEqual(did["len"], len(PII_UUID))

    def test_ssdp_keeps_method_and_header_names_only(self):
        self.assertEqual(len(self.summary["ssdp"]), 1)
        entry = self.summary["ssdp"][0]
        self.assertEqual(entry["start_line"], "NOTIFY * HTTP/1.1")
        self.assertIn("USN", entry["header_names"])
        self.assertIn("LOCATION", entry["header_names"])
        self.assertNotIn(PII_IP_B, json.dumps(entry))

    def test_tcp_previews_keep_framing_shape(self):
        self.assertGreaterEqual(len(self.summary["tcp_payload_previews"]), 2)
        first = self.summary["tcp_payload_previews"][0]
        self.assertEqual(first["len"], 64)
        self.assertTrue(first["head_hex"].startswith("08011001"))

    def test_pseudonyms_are_stable(self):
        ips = {f["src"] for f in self.summary["flows"]}
        self.assertTrue(all(i.startswith("<ip-") for i in ips))
        self.assertLessEqual(len(ips), 2)

    def test_advertiser_attribution_and_self_address_flag(self):
        services = {s["service_type"]: s for s in self.summary["mdns"]["services"]}
        airplay = services["_airplay._tcp.local"]
        # 广播者必须是假名，且与"询问者"的假名体系一致
        self.assertEqual(len(airplay["advertisers"]), 1)
        self.assertTrue(airplay["advertisers"][0].startswith("<ip-"))
        quick = services["_quickshare._tcp.local"]
        self.assertRegex(quick["advertisers"][0], r"^<ip-\d+>$")

    def test_txt_ip_value_is_classified_and_compared_to_advertiser(self):
        services = {s["service_type"]: s for s in self.summary["mdns"]["services"]}
        # fixture 里 `_quickshare` 的 TXT 带自己的地址：只留 "是否等于广播者地址" 这个布尔
        txt = services["_quickshare._tcp.local"]["txt_redacted"]
        self.assertIn("ipv4", txt)
        self.assertEqual(txt["ipv4"]["class"], "ip")
        self.assertTrue(txt["ipv4"]["matches_advertiser"])

    def test_question_askers_are_recorded(self):
        q = [q for q in self.summary["mdns"]["questions"] if q["name"] == "_airplay._tcp.local"][0]
        self.assertEqual(q["count"], 1)
        self.assertEqual(len(q["askers"]), 1)
        self.assertRegex(q["askers"][0]["pseudonym"], r"^<ip-\d+>$")

    def test_report_records_hash_and_limits(self):
        self.assertIn(self.summary["meta"]["source_sha256"], self.report)
        self.assertIn("不能证明", self.report)
        self.assertEqual(len(self.summary["meta"]["source_sha256"]), 64)

    def test_pcapng_input_is_rejected_with_a_clear_message(self):
        with tempfile.TemporaryDirectory() as d:
            bad = pathlib.Path(d) / "x.pcapng"
            bad.write_bytes(b"\x0a\x0d\x0d\x0a" + b"\x00" * 32)
            with self.assertRaises(redact.PcapError) as ctx:
                redact.read_pcap(bad)
            self.assertIn("pcapng", str(ctx.exception))

    def test_empty_and_truncated_files_do_not_crash(self):
        with tempfile.TemporaryDirectory() as d:
            empty = pathlib.Path(d) / "empty.pcap"
            write_pcap(empty, [])
            summary, _ = redact.run(empty, "empty")
            self.assertEqual(summary["meta"]["packet_count"], 0)
            self.assertEqual(summary["flows"], [])
            truncated = pathlib.Path(d) / "trunc.pcap"
            write_pcap(truncated, [udp_packet(PII_IP_A, PII_IP_B, 1, 2, b"x")])
            raw = truncated.read_bytes()
            truncated.write_bytes(raw[: len(raw) - 3])
            summary, _ = redact.run(truncated, "trunc")
            self.assertGreaterEqual(summary["meta"]["packet_count"], 0)

    def test_malformed_dns_is_counted_not_fatal(self):
        # 压缩指针成环：解析必须被拒绝并记入 skipped，而不是崩或死循环
        bad = struct.pack(">HHHHHH", 0, 0x8400, 0, 1, 0, 0)
        bad += b"\xc0\x0c" + struct.pack(">HHIH", 12, 1, 10, 2) + b"\xc0\x0c"
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "loop.pcap"
            write_pcap(path, [udp_packet(PII_IP_A, "224.0.0.251", 5353, 5353, bad)])
            summary, _ = redact.run(path, "loop")
            self.assertEqual(summary["redaction"]["skipped_frames"], 1)


if __name__ == "__main__":
    unittest.main()
