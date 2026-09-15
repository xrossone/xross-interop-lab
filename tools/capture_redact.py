"""S3 抓包脱敏：把原始 pcap 变成可入库的结构化摘录（stdlib only）。

**这件事的边界**（写在这里，也是给复核者看的）：

- 只读**包头与协议结构**：mDNS 的服务类型/键名/端口、SSDP 的方法与头名、TCP/UDP 流的形状与
  首批载荷（截断）。**不**解析、**不**输出任何设备名、人名、文件名、MAC/IP/UUID、载荷内容。
- 不做解密、不做协议语义校验、不产生"设备已认证"之类的结论。抓包只证明"网线上有什么字节"。
- 原始 pcap **永不入库**；本工具只把脱敏摘录与原文 sha256 写到你指定的目录（`evidence/` 下）。
- 脱敏是**白名单**式的：只有明确认识的字段会被保留（服务类型、TXT 键名、协议常量形状的取值、
  端口、长度、时序），其余一律替换成稳定假名或 `<redacted len=N>`。

用法：
    python3 tools/capture_redact.py <pcap> --out-dir evidence/<run-id> [--label capture-01]

产物：`<label>-redacted.json`、`<label>-redacted.txt`、`<label>-REDACTION.md`。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import struct
import sys
from collections import OrderedDict

PCAP_MAGICS = {
    0xA1B2C3D4: ("<", 1_000_000),  # 微秒，小端
    0xD4C3B2A1: (">", 1_000_000),  # 微秒，大端
    0xA1B23C4D: ("<", 1_000_000_000),  # 纳秒，小端
    0x4D3CB2A1: (">", 1_000_000_000),  # 纳秒，大端
}
LINKTYPE_ETHERNET = 1
LINKTYPE_NULL = 0
LINKTYPE_LINUX_SLL = 113
LINKTYPE_LINUX_SLL2 = 276

ETH_IPV4 = 0x0800
ETH_IPV6 = 0x86DD
ETH_VLAN = 0x8100
ETH_VLAN_QINQ = 0x88A8

IPPROTO_TCP = 6
IPPROTO_UDP = 17

PORT_MDNS = 5353
PORT_SSDP = 1900

# 只保留"协议常量形状"的取值；其余取值一律脱敏（设备名/人名/文件名都在其余里）
PROTOCOL_VALUE_PREFIXES = ("0x", "0X")
TXT_KEYS_ALWAYS_KEPT = {
    # mDNS/AirPlay/Cast/Quick Share 侧与协议语义直接相关的键名（键名本身不是隐私）
    "txtvers",
    "features",
    "flags",
    "deviceid",
    "pk",
    "pi",
    "srcvers",
    "vv",
    "ve",
    "ca",
    "st",
    "md",
    "model",
    "protovers",
    "serialnumber",
    "rpfl",
    "rpba",
    "gid",
    "gcgl",
    "rs",
    "psi",
    "df",
    "ft",
    "sf",
}

MAX_PAYLOAD_PREVIEW = 64
MAX_PAYLOADS_PER_DIRECTION = 8


class PcapError(Exception):
    pass


class Pseudonyms:
    """稳定假名：同一实体的每次出现都映射到同一个假名，便于跨包对照。"""

    def __init__(self, prefix: str):
        self.prefix = prefix
        self.mapping: "OrderedDict[str, str]" = OrderedDict()

    def of(self, value: str) -> str:
        if value not in self.mapping:
            self.mapping[value] = f"<{self.prefix}-{len(self.mapping) + 1}>"
        return self.mapping[value]

    def as_list(self) -> list[dict]:
        return [{"pseudonym": v, "occurrences": 0} for v in self.mapping.values()]


def read_pcap(path: pathlib.Path) -> tuple[int, list[tuple[float, bytes]]]:
    raw = path.read_bytes()
    if len(raw) < 24:
        raise PcapError("文件太短，不是 pcap")
    magic = struct.unpack("<I", raw[:4])[0]
    if magic not in PCAP_MAGICS:
        if raw[:4] == b"\x0a\x0d\x0d\x0a":
            raise PcapError("这是 pcapng；请用 tcpdump -w x.pcap（经典 pcap）或先转换")
        raise PcapError(f"未知的 pcap magic：0x{magic:08x}")
    endian, ts_div = PCAP_MAGICS[magic]
    linktype = struct.unpack(endian + "I", raw[20:24])[0]
    packets: list[tuple[float, bytes]] = []
    off = 24
    while off + 16 <= len(raw):
        ts_sec, ts_frac, incl_len, _orig_len = struct.unpack(endian + "IIII", raw[off : off + 16])
        off += 16
        if off + incl_len > len(raw):
            break
        packets.append((ts_sec + ts_frac / ts_div, raw[off : off + incl_len]))
        off += incl_len
    return linktype, packets


def read_name(data: bytes, off: int, depth: int = 0) -> tuple[str, int]:
    """DNS 名字（含压缩指针）。返回 (名字, 指针之后的下一个偏移)。"""
    labels: list[str] = []
    next_off = off
    jumped = False
    hops = 0
    while True:
        if off >= len(data):
            raise PcapError("DNS 名字越界")
        length = data[off]
        if length == 0:
            off += 1
            if not jumped:
                next_off = off
            break
        if length & 0xC0 == 0xC0:
            if off + 1 >= len(data):
                raise PcapError("DNS 压缩指针越界")
            ptr = ((length & 0x3F) << 8) | data[off + 1]
            if not jumped:
                next_off = off + 2
            off = ptr
            jumped = True
            hops += 1
            if hops > 32 or depth > 16:
                raise PcapError("DNS 压缩指针成环")
            continue
        off += 1
        labels.append(data[off : off + length].decode("utf-8", "replace"))
        off += length
        if not jumped:
            next_off = off
    return ".".join(labels), next_off


def parse_dns(payload: bytes) -> dict:
    if len(payload) < 12:
        raise PcapError("DNS 头太短")
    _id, flags, qdcount, ancount, nscount, arcount = struct.unpack(">HHHHHH", payload[:12])
    off = 12
    questions = []
    for _ in range(qdcount):
        name, off = read_name(payload, off)
        qtype, qclass = struct.unpack(">HH", payload[off : off + 4])
        off += 4
        questions.append({"name": name, "type": qtype, "class": qclass})
    records = []
    for _ in range(ancount + nscount + arcount):
        name, off = read_name(payload, off)
        rtype, rclass, _ttl, rdlen = struct.unpack(">HHIH", payload[off : off + 10])
        off += 10
        rdata = payload[off : off + rdlen]
        off += rdlen
        rec = {"name": name, "type": rtype, "class": rclass, "rdlen": rdlen}
        if rtype == 12:  # PTR
            rec["ptr"] = read_name(payload, off - rdlen)[0]
        elif rtype == 33:  # SRV
            prio, weight, port = struct.unpack(">HHH", rdata[:6])
            target = read_name(payload, off - rdlen + 6)[0]
            rec.update({"priority": prio, "weight": weight, "port": port, "target": target})
        elif rtype == 16:  # TXT
            values = []
            i = 0
            while i < len(rdata):
                ln = rdata[i]
                i += 1
                values.append(rdata[i : i + ln].decode("utf-8", "replace"))
                i += ln
            rec["txt"] = values
        elif rtype in (1, 28):
            rec["addr_len"] = rdlen
        records.append(rec)
    return {"flags": flags, "questions": questions, "records": records}


def parse_ssdp(payload: bytes) -> dict:
    text = payload.decode("utf-8", "replace")
    lines = [l for l in text.split("\r\n") if l]
    start = lines[0] if lines else ""
    headers = []
    for line in lines[1:]:
        if ":" in line:
            name, _, _value = line.partition(":")
            headers.append(name.strip())
    return {"start_line": start, "header_names": headers}


def is_protocol_constant(value: str) -> bool:
    """协议常量形状：`0x…` / 十进制 / 逗号分隔的这两者（如 AirPlay 的 `0x5A7FFFF7,0x1E`）。

    只有这种形状的取值才保留——设备名、人名、文件名、UUID 都不长这样。
    """
    if not value or len(value) > 48:
        return False
    v = value.strip()
    if not v:
        return False
    for part in (p.strip() for p in v.split(",")):
        if not part:
            return False
        if part.startswith(PROTOCOL_VALUE_PREFIXES):
            body = part[2:]
            if not body or not all(c in "0123456789abcdefABCDEF" for c in body):
                return False
        elif not part.isdigit():
            return False
    return True


def value_class(value: str) -> str:
    if is_protocol_constant(value):
        return "protocol-constant"
    if is_uuid(value):
        return "uuid"
    if all(c in "0123456789abcdefABCDEF" for c in value) and len(value) >= 16:
        return "hex-blob"
    if len(value) > 64:
        return "long-text"
    return "text"


def base64url_decode(value: str) -> bytes | None:
    v = value.strip()
    if not v or any(c not in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_" for c in v):
        return None
    pad = "=" * ((4 - len(v) % 4) % 4)
    try:
        import base64 as _b64

        return _b64.urlsafe_b64decode(v + pad)
    except Exception:  # noqa: BLE001 - 非法长度等一律视为"不是 base64"
        return None


def probe_endpoint_info(value: str) -> dict:
    """按 **F-02 的声明**读 `n` 的结构：位域 + 16 字节识别材料 + 名字长度 + 名字。

    只输出**结构与长度**（位域原始字节、各段字节数），**不输出**识别材料与设备名内容——
    这样既能做"抓包 ↔ 源码对照"，又不把身份材料带进仓库。读数与 F-02 不符时同样如实记录。
    """
    raw = base64url_decode(value)
    if raw is None:
        return {"declared_form": "base64url（F-02）", "decoded": False}
    info = {"declared_form": "base64url（F-02）", "decoded": True, "decoded_len": len(raw)}
    if len(raw) >= 17:
        info["bitfield_hex"] = raw[:1].hex()
        info["identity_bytes"] = 16
        rest = raw[17:]
        if rest:
            info["name_len_byte"] = rest[0]
            info["name_bytes_present"] = min(rest[0], max(0, len(rest) - 1))
            info["trailing_bytes"] = max(0, len(rest) - 1 - info["name_bytes_present"])
        else:
            info["name_len_byte"] = None
            info["name_bytes_present"] = 0
            info["trailing_bytes"] = 0
    return info


def is_mac_literal(value: str) -> bool:
    """`AA:BB:CC:DD:EE:FF` 形状（AirPlay 的 deviceid 是 MAC，不是 IP）。"""
    parts = value.strip().split(":")
    return len(parts) == 6 and all(len(x) == 2 and all(c in "0123456789abcdefABCDEF" for c in x) for x in parts)


def is_ip_literal(value: str) -> bool:
    v = value.strip()
    if is_mac_literal(v):
        return False
    parts = v.split(".")
    if len(parts) == 4 and all(p.isdigit() and 0 <= int(p) <= 255 for p in parts):
        return True
    # IPv6：≥3 组、每组 1..4 位十六进制，允许 `::` 压缩（MAC 已被上面排除）
    if v.count(":") < 2:
        return False
    if "::" in v and v.count("::") > 1:
        return False
    groups = [g for g in v.split(":") if g]
    if len(groups) < 2:
        return False
    return all(1 <= len(g) <= 4 and all(c in "0123456789abcdefABCDEF" for c in g) for g in groups)


def is_uuid(value: str) -> bool:
    parts = value.split("-")
    if [len(p) for p in parts] != [8, 4, 4, 4, 12]:
        return False
    return all(c in "0123456789abcdefABCDEF" for p in parts for c in p)


def strip_service_type(name: str) -> tuple[str, str]:
    """把 `_airplay._tcp.local` 与 `<instance>._airplay._tcp.local` 拆开。"""
    labels = name.split(".")
    for i, label in enumerate(labels):
        if label.startswith("_"):
            return ".".join(labels[:i]), ".".join(labels[i:])
    return name, ""


class Redactor:
    def __init__(self, label: str):
        self.label = label
        self.ips = Pseudonyms("ip")
        self.names = Pseudonyms("name")
        self.instances = Pseudonyms("instance")
        self.seen_services: "OrderedDict[str, dict]" = OrderedDict()
        self.flows: "OrderedDict[tuple, dict]" = OrderedDict()
        self.mdns_questions: "OrderedDict[tuple, int]" = OrderedDict()
        self.ssdp: list[dict] = []
        self.tcp_previews: list[dict] = []
        self.instance_times: "OrderedDict[str, list[float]]" = OrderedDict()
        self.instance_lens: dict[str, int] = {}
        self.instance_prefix: dict[str, str] = {}
        self.skipped_frames = 0
        self.non_ip_frames = 0

    # ---- 流记账 -------------------------------------------------------
    def flow(self, proto: str, src, sport, dst, dport, ts, length, family: str) -> dict:
        key = (proto, self.ips.of(src), sport, self.ips.of(dst), dport)
        entry = self.flows.get(key)
        if entry is None:
            entry = {
                "proto": proto,
                "src": key[1],
                "sport": sport,
                "dst": key[3],
                "dport": dport,
                "family": family,
                "packets": 0,
                "bytes": 0,
                "first_ts": ts,
                "last_ts": ts,
            }
            self.flows[key] = entry
        entry["packets"] += 1
        entry["bytes"] += length
        entry["last_ts"] = ts
        return entry

    # ---- mDNS ---------------------------------------------------------
    def mdns(self, payload: bytes, ts: float, src: str) -> None:
        dns = parse_dns(payload)
        asker = self.ips.of(src)
        for q in dns["questions"]:
            instance, service = strip_service_type(q["name"])
            shown = service or self.names.of(q["name"])
            key = (shown, q["type"])
            entry = self.mdns_questions.setdefault(key, {"count": 0, "askers": OrderedDict()})
            entry["count"] += 1
            entry["askers"][asker] = entry["askers"].get(asker, 0) + 1
            if instance:
                self.instances.of(instance)
        for rec in dns["records"]:
            instance, service = strip_service_type(rec["name"])
            if instance:
                self.instances.of(instance)
            if service in self.seen_services or rec["type"] in (12, 33, 16):
                # 应答里的服务类型：广播者就是这一包的源（只记假名）
                pass
            if rec["type"] == 12:
                target_instance, target_service = strip_service_type(rec.get("ptr", ""))
                if target_instance and target_service:
                    svc = self._service_instance(target_service, target_instance, ts)
                    self.note_advertiser(svc, src)
            elif rec["type"] == 33:
                svc = (
                    self._service_instance(service, instance, ts)
                    if instance
                    else self._service(service, ts)
                )
                self.note_advertiser(svc, src)
                svc["port"] = rec["port"]
                svc["target"] = self.names.of(rec["target"]) if rec.get("target") else None
            elif rec["type"] == 16:
                svc = (
                    self._service_instance(service, instance, ts)
                    if instance
                    else self._service(service, ts)
                )
                self.note_advertiser(svc, src)
                for item in rec.get("txt", []):
                    if "=" in item:
                        k, _, v = item.partition("=")
                        k = k.strip()
                        svc["txt_keys"][k] = True
                        if is_protocol_constant(v):
                            svc["txt_constants"][k] = v
                        else:
                            # 地址字面量单独归类：形状留下，"是否等于广播者自己的地址"留下
                            cls = (
                                "mac"
                                if is_mac_literal(v)
                                else "ip"
                                if is_ip_literal(v)
                                else value_class(v)
                            )
                            info = {"class": cls, "len": len(v)}
                            if cls in ("ip", "mac"):
                                # 原文只在内存里用于比较（双栈设备不能用"本包源地址"判）
                                svc.setdefault("txt_ip_values", {})[k] = v
                            elif k == "n":
                                # F-02 声明 `n` 是 base64(endpoint info)：只读结构，不读内容
                                info["structure"] = probe_endpoint_info(v)
                                probe = info["structure"]
                                series = svc.setdefault("n_series", [])
                                entry = {
                                    "ts": None,  # 摘要时填相对时刻
                                    "bitfield_hex": probe.get("bitfield_hex"),
                                    "decoded_len": probe.get("decoded_len"),
                                    "name_len_byte": probe.get("name_len_byte"),
                                }
                                if not series or any(
                                    series[-1][k2] != entry[k2] for k2 in entry if k2 != "ts"
                                ):
                                    entry["ts"] = ts
                                    series.append(entry)
                            svc["txt_redacted"][k] = info
                    else:
                        svc["txt_keys"][item] = True
            elif rec["type"] in (1, 28):
                svc = self._service(service, ts) if service else None
                if svc is not None:
                    svc["addr_records"] = svc.get("addr_records", 0) + 1

    def _service_instance(self, service: str, instance: str, ts: float) -> dict:
        svc = self._service(service, ts)
        pseudonym = self.instances.of(instance)
        if pseudonym not in svc["instances"]:
            svc["instances"].append(pseudonym)
        self.note_instance_time(pseudonym, ts)
        self.instance_lens.setdefault(pseudonym, len(instance))
        # 只留首字符：用来核对来源声明的**固定前缀**（F-01：0x23 → base64url 首字符恒为 'I'），
        # 其余字符（含随机 endpoint id）不出现
        self.instance_prefix.setdefault(pseudonym, instance[:1])
        return svc

    def _service(self, service: str, ts: float) -> dict:
        if not service:
            service = "<no-service-type>"
        svc = self.seen_services.get(service)
        if svc is None:
            svc = {
                "service_type": service,
                "instances": [],
                "advertisers": [],
                "advertiser": None,
                "port": None,
                "target": None,
                "txt_keys": OrderedDict(),
                "txt_constants": OrderedDict(),
                "txt_redacted": OrderedDict(),
                "first_ts": ts,
                "last_ts": ts,
            }
            self.seen_services[service] = svc
        svc["last_ts"] = ts
        return svc

    def note_instance_time(self, pseudonym: str, ts: float) -> None:
        window = self.instance_times.setdefault(pseudonym, [ts, ts])
        window[0] = min(window[0], ts)
        window[1] = max(window[1], ts)

    def note_advertiser(self, svc: dict, src: str) -> None:
        """谁广播了这条服务（只记假名；用于回答"手机到底广没广播"）。"""
        pseudonym = self.ips.of(src)
        if svc["advertiser"] is None:
            svc["advertiser"] = pseudonym
        if pseudonym not in svc["advertisers"]:
            svc["advertisers"].append(pseudonym)

    # ---- SSDP ---------------------------------------------------------
    def ssdp_message(self, payload: bytes, ts: float) -> None:
        parsed = parse_ssdp(payload)
        entry = {
            "start_line": parsed["start_line"],
            "header_names": parsed["header_names"],
            "ts": round(ts, 3),
        }
        self.ssdp.append(entry)
        if len(self.ssdp) > 40:
            del self.ssdp[0]

    # ---- TCP 预览 -----------------------------------------------------
    def tcp_preview(self, flow: dict, payload: bytes, ts: float, direction: str) -> None:
        label = f"{flow['src']}:{flow['sport']}->{flow['dst']}:{flow['dport']}"
        per_dir = [p for p in self.tcp_previews if p["flow"] == label]
        if len(per_dir) >= MAX_PAYLOADS_PER_DIRECTION:
            return
        self.tcp_previews.append(
            {
                "flow": label,
                "ts": round(ts, 3),
                "direction": direction,
                "len": len(payload),
                "head_hex": payload[:MAX_PAYLOAD_PREVIEW].hex(),
            }
        )

    def summary(self, meta: dict) -> dict:
        real_by_pseudonym = {p: real for real, p in self.ips.mapping.items()}
        observed = set(self.ips.mapping.keys())
        for svc in self.seen_services.values():
            advertiser_addrs = {
                real_by_pseudonym[p] for p in svc["advertisers"] if p in real_by_pseudonym
            }
            for k, v in svc.pop("txt_ip_values", {}).items():
                svc["txt_redacted"][k]["matches_advertiser"] = v in advertiser_addrs
                svc["txt_redacted"][k]["matches_any_observed"] = v in observed
            svc["first_ts"] = round(svc["first_ts"] - meta["first_ts"], 3)
            if "n_series" in svc:
                svc["n_series"] = [
                    {**e, "ts": round(e["ts"] - meta["first_ts"], 3)} for e in svc["n_series"]
                ]
            svc["last_ts"] = round(svc["last_ts"] - meta["first_ts"], 3)
            svc["instance_windows"] = [
                {
                    "pseudonym": p,
                    "name_len": self.instance_lens.get(p),
                    "first_char": self.instance_prefix.get(p),
                    "first_ts": round(self.instance_times[p][0] - meta["first_ts"], 3),
                    "last_ts": round(self.instance_times[p][1] - meta["first_ts"], 3),
                }
                for p in svc["instances"]
                if p in self.instance_times
            ]
            svc["txt_keys"] = list(svc["txt_keys"].keys())
            svc["txt_constants"] = dict(svc["txt_constants"])
            svc["txt_redacted"] = dict(svc["txt_redacted"])
        return {
            "meta": meta,
            "flows": [
                {k: v for k, v in f.items() if k not in ("first_ts", "last_ts")}
                | {
                    "first_ts_rel": round(f["first_ts"] - meta["first_ts"], 3),
                    "last_ts_rel": round(f["last_ts"] - meta["first_ts"], 3),
                }
                for f in self.flows.values()
            ],
            "mdns": {
                "questions": [
                    {
                        "name": name,
                        "type": qtype,
                        "count": info["count"],
                        "askers": [
                            {"pseudonym": who, "count": n} for who, n in info["askers"].items()
                        ],
                    }
                    for (name, qtype), info in self.mdns_questions.items()
                ],
                "services": list(self.seen_services.values()),
            },
            "ssdp": self.ssdp,
            "tcp_payload_previews": self.tcp_previews,
            "redaction": {
                "text_pseudonyms": len(self.names.mapping),
                "ip_pseudonyms": len(self.ips.mapping),
                "instance_pseudonyms": len(self.instances.mapping),
                "redacted_txt_values": sum(
                    len(s["txt_redacted"]) for s in self.seen_services.values()
                ),
                "skipped_frames": self.skipped_frames,
                "non_ip_frames": self.non_ip_frames,
            },
        }


def iter_l3(frame: bytes, linktype: int) -> tuple[int, bytes] | None:
    if linktype == LINKTYPE_ETHERNET:
        if len(frame) < 14:
            return None
        ethertype = struct.unpack(">H", frame[12:14])[0]
        off = 14
        while ethertype in (ETH_VLAN, ETH_VLAN_QINQ):
            if len(frame) < off + 4:
                return None
            ethertype = struct.unpack(">H", frame[off + 2 : off + 4])[0]
            off += 4
        return ethertype, frame[off:]
    if linktype == LINKTYPE_NULL:
        if len(frame) < 4:
            return None
        family = struct.unpack("<I", frame[:4])[0]
        return (ETH_IPV4 if family == 2 else ETH_IPV6), frame[4:]
    if linktype in (LINKTYPE_LINUX_SLL, LINKTYPE_LINUX_SLL2):
        header = 16 if linktype == LINKTYPE_LINUX_SLL else 20
        if len(frame) < header:
            return None
        ethertype = struct.unpack(">H", frame[header - 2 : header])[0]
        return ethertype, frame[header:]
    return None


def parse_ip(l3: bytes, ethertype: int) -> tuple[str, str, int, bytes] | None:
    if ethertype == ETH_IPV4:
        if len(l3) < 20:
            return None
        ihl = (l3[0] & 0x0F) * 4
        proto = l3[9]
        total = struct.unpack(">H", l3[2:4])[0]
        src = ".".join(str(b) for b in l3[12:16])
        dst = ".".join(str(b) for b in l3[16:20])
        return src, dst, proto, l3[ihl:total]
    if ethertype == ETH_IPV6:
        if len(l3) < 40:
            return None
        proto = l3[6]
        payload_len = struct.unpack(">H", l3[4:6])[0]
        src = l3[8:24].hex(":")
        dst = l3[24:40].hex(":")
        return src, dst, proto, l3[40 : 40 + payload_len]
    return None


def run(path: pathlib.Path, label: str) -> tuple[dict, str]:
    linktype, packets = read_pcap(path)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    red = Redactor(label)
    for ts, frame in packets:
        l3 = iter_l3(frame, linktype)
        if l3 is None:
            red.non_ip_frames += 1
            continue
        parsed = parse_ip(l3[1], l3[0])
        if parsed is None:
            red.non_ip_frames += 1
            continue
        src, dst, proto, payload = parsed
        family = "v6" if l3[0] == ETH_IPV6 else "v4"
        try:
            if proto == IPPROTO_UDP and len(payload) >= 8:
                sport, dport, ulen, _cksum = struct.unpack(">HHHH", payload[:8])
                body = payload[8:ulen] if 8 <= ulen <= len(payload) else payload[8:]
                flow = red.flow("udp", src, sport, dst, dport, ts, len(frame), family)
                if PORT_MDNS in (sport, dport):
                    flow["protocol_hint"] = "mdns"
                    red.mdns(body, ts, src)
                elif PORT_SSDP in (sport, dport):
                    flow["protocol_hint"] = "ssdp"
                    red.ssdp_message(body, ts)
            elif proto == IPPROTO_TCP and len(payload) >= 20:
                sport, dport = struct.unpack(">HH", payload[:4])
                data_off = (payload[12] >> 4) * 4
                body = payload[data_off:]
                flow = red.flow("tcp", src, sport, dst, dport, ts, len(frame), family)
                if body:
                    red.tcp_preview(flow, body, ts, f"{flow['src']}->{flow['dst']}")
        except PcapError:
            red.skipped_frames += 1
            continue
    first = packets[0][0] if packets else 0.0
    last = packets[-1][0] if packets else 0.0
    meta = {
        "source_sha256": digest,
        "source_bytes": path.stat().st_size,
        "linktype": linktype,
        "packet_count": len(packets),
        "first_ts": first,
        "last_ts": last,
        "duration_s": round(last - first, 3) if packets else 0.0,
        "redactor": "tools/capture_redact.py",
    }
    return red.summary(meta), digest


def render_text(summary: dict) -> str:
    out = []
    meta = summary["meta"]
    out.append(f"# 脱敏摘录（{meta['packet_count']} 包 / {meta['duration_s']} s）")
    out.append(f"原文 sha256: {meta['source_sha256']}")
    out.append("")
    out.append("## 流")
    for f in summary["flows"]:
        hint = f.get("protocol_hint", "")
        out.append(
            f"- {f['proto']} {f['src']}:{f['sport']} → {f['dst']}:{f['dport']} "
            f"{f['packets']} 包 / {f['bytes']} 字节 [{f['first_ts_rel']}s..{f['last_ts_rel']}s] {hint}"
        )
    out.append("")
    out.append("## mDNS 服务")
    for svc in summary["mdns"]["services"]:
        out.append(
            f"- {svc['service_type']} port={svc['port']} 实例数={len(svc['instances'])} "
            f"广播者={svc['advertisers']} [{svc['first_ts']}s..{svc['last_ts']}s]"
        )
        for w in svc.get("instance_windows", []):
            out.append(
                f"  实例 {w['pseudonym']}: 首字符={w['first_char']!r} 长度={w['name_len']} "
                f"[{w['first_ts']}s..{w['last_ts']}s]"
            )
        if svc["txt_keys"]:
            out.append(f"  TXT 键: {', '.join(svc['txt_keys'])}")
        for k, v in svc["txt_constants"].items():
            out.append(f"  TXT 常量: {k}={v}")
        for k, meta_v in svc["txt_redacted"].items():
            out.append(f"  TXT 已脱敏: {k} <{meta_v['class']} len={meta_v['len']}>")
        for e in svc.get("n_series", []):
            names = "无名字段" if e["name_len_byte"] is None else f"名字长度={e['name_len_byte']}"
            out.append(
                f"  n 位域序列 @{e['ts']}s: bitfield={e['bitfield_hex']} "
                f"解码长度={e['decoded_len']} {names}"
            )
    if summary["mdns"]["questions"]:
        out.append("")
        out.append("## mDNS 查询")
        for q in summary["mdns"]["questions"]:
            askers = ", ".join(f"{a['pseudonym']}×{a['count']}" for a in q["askers"])
            out.append(f"- {q['name']} type={q['type']} ×{q['count']}（询问者：{askers}）")
    if summary["ssdp"]:
        out.append("")
        out.append("## SSDP")
        for s in summary["ssdp"][:10]:
            out.append(f"- {s['start_line']} | 头: {', '.join(s['header_names'])}")
    if summary["tcp_payload_previews"]:
        out.append("")
        out.append("## TCP 首批载荷（每方向最多 8 条，截断 64 字节）")
        for p in summary["tcp_payload_previews"]:
            out.append(f"- {p['flow']} len={p['len']} head={p['head_hex']}")
    out.append("")
    out.append("## 脱敏统计")
    for k, v in summary["redaction"].items():
        out.append(f"- {k}: {v}")
    return "\n".join(out) + "\n"


def render_report(summary: dict) -> str:
    meta = summary["meta"]
    red = summary["redaction"]
    return f"""# 脱敏报告

- 原文：`{meta['source_sha256']}`（{meta['source_bytes']} 字节，**未入库**）
- 包数/时长：{meta['packet_count']} / {meta['duration_s']} s（linktype={meta['linktype']}）
- 工具：`tools/capture_redact.py`（stdlib only，白名单式保留）

## 删掉了什么

- 所有 MAC/IP：替换为稳定假名 `<ip-N>`（同址同假名，可跨包对照）
- 所有实例名/主机名/设备名/人名/文件名：替换为 `<name-N>` / `<instance-N>`
- TXT 取值：只保留"协议常量形状"（十六进制/十进制/数字列表，长度 ≤48），
  其余一律记成 `<redacted len=N>` 并标注类别；共脱敏 {red['redacted_txt_values']} 条
- DNS 名字：只保留服务类型部分（`_xxx._tcp` 之类），实例部分脱敏
- SSDP：只保留起始行与头名，取值一律不出现
- 载荷：只保留 TCP 每方向前 {MAX_PAYLOADS_PER_DIRECTION} 条、每条前 {MAX_PAYLOAD_PREVIEW} 字节的十六进制

## 保留了哪些协议事实

服务类型、TXT 键名、协议常量取值、SRV 端口、SSDP 方法/头名、流的方向与包数字节数、TCP 首批载荷形状。

## 这个摘录**不能**证明什么

- 不能证明协议语义正确、不能替代字段级来源；抓包只证明"网线上出现了这些字节"。
- 不含任何设备身份：本文件不得用于"设备已认证/已配对"之类的结论。
"""


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="pcap → 脱敏摘录（S3）")
    parser.add_argument("pcap", type=pathlib.Path)
    parser.add_argument("--out-dir", type=pathlib.Path, required=True)
    parser.add_argument("--label", default="capture")
    args = parser.parse_args(argv)

    try:
        summary, _digest = run(args.pcap, args.label)
    except PcapError as exc:
        print(f"错误：{exc}", file=sys.stderr)
        return 2
    args.out_dir.mkdir(parents=True, exist_ok=True)
    (args.out_dir / f"{args.label}-redacted.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )
    (args.out_dir / f"{args.label}-redacted.txt").write_text(
        render_text(summary), encoding="utf-8"
    )
    (args.out_dir / f"{args.label}-REDACTION.md").write_text(
        render_report(summary), encoding="utf-8"
    )
    print(f"ok：{args.out_dir}/{args.label}-redacted.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
