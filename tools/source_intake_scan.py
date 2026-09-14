#!/usr/bin/env python3
"""T02 来源 intake 扫描器：只读收集 file-core + airplay 两组仓库的事实。

对每个仓库收集：origin/HEAD（与 sources.lock.json 比对）、根许可文件、
构建入口（build.rs/proc-macro/Makefile/CMake/package.json scripts 等）、
submodules、CI 下载项、已提交 vendor 目录、不可信指令文件（AGENTS/CLAUDE 等）。

硬性约束：本工具只做文件系统与 git 元数据读取，**绝不执行**被扫描仓库中的
任何脚本、构建规则或指令性文字；扫描到的 AGENTS/CLAUDE/.kiro 等仅记录为
不可信材料。

用法（lab 根目录）：
    python3 tools/source_intake_scan.py
输出：
    evidence/source-intake/<slug>.json  （每仓库一份事实清单）
    evidence/source-intake/scan-log.md  （命令与结果摘要）
"""

from __future__ import annotations

import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
OTHERS = Path("/Volumes/Portable2TB/ExtDev/others")
IN_SCOPE_GROUPS = {"file-core", "airplay"}

LICENSE_GLOBS = ["LICENSE*", "LICENCE*", "COPYING*", "COPYRIGHT*", "NOTICE*", "UNLICENSE*"]

BUILD_ENTRY_FILES = [
    "build.rs",
    "Makefile",
    "CMakeLists.txt",
    "configure",
    "autogen.sh",
    "meson.build",
    "setup.py",
    "pyproject.toml",
    "package.json",
    "Cargo.toml",
    "go.mod",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "SConstruct",
    "justfile",
    "Taskfile.yml",
]

UNTRUSTED_NAMES = {
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    ".cursorrules",
    ".clinerules",
    "copilot-instructions.md",
}
UNTRUSTED_DIRS = {".cursor", ".kiro", ".windsurf", ".aider"}

VENDOR_SEGMENTS = {"vendor", "vendors", "third_party", "thirdparty", "node_modules", "external"}

GENERATED_PATTERNS = ["*.pb.go", "*_pb2.py", "*.pb.h", "*.pb.cc", "*_pb2_grpc.py"]


def run(cmd: list[str], cwd: Path | None = None) -> tuple[int, str]:
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=60)
    return p.returncode, (p.stdout + p.stderr).strip()


def git_ls_files(repo: Path) -> list[str] | None:
    rc, out = run(["git", "ls-files", "-z"], cwd=repo)
    if rc != 0:
        return None
    return [s for s in out.split("\0") if s]


def scan_repo(repo: Path) -> dict:
    facts: dict = {"slug": repo.name}

    rc, out = run(["git", "remote", "get-url", "origin"], cwd=repo)
    facts["origin_url"] = out if rc == 0 else None
    rc, out = run(["git", "rev-parse", "HEAD"], cwd=repo)
    facts["head_commit"] = out.splitlines()[0] if rc == 0 and out else None
    rc, out = run(["git", "log", "-1", "--format=%cI"], cwd=repo)
    facts["head_commit_time"] = out if rc == 0 else None

    license_files = []
    for pat in LICENSE_GLOBS:
        for p in sorted(repo.glob(pat)):
            if p.is_file():
                license_files.append(p.name)
    facts["license_files_root"] = license_files

    files = git_ls_files(repo) or []
    top_names = {Path(f).name for f in files}

    facts["build_entrypoints"] = sorted(n for n in BUILD_ENTRY_FILES if n in top_names)
    for spec in repo.glob("*.podspec"):
        facts["build_entrypoints"].append(spec.name)

    # Cargo 专项：build scripts / proc-macro
    cargo_notes = []
    for manifest in sorted(repo.rglob("Cargo.toml")):
        try:
            text = manifest.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        rel = manifest.relative_to(repo).as_posix()
        if "[build-dependencies]" in text:
            cargo_notes.append(f"{rel}: [build-dependencies]")
        for line in text.splitlines():
            s = line.strip()
            if s.startswith("build ") or s.startswith("build="):
                cargo_notes.append(f"{rel}: {s}")
        if "[lib]" in text and "proc-macro" in text:
            cargo_notes.append(f"{rel}: proc-macro lib")
    facts["cargo_build_surface"] = cargo_notes

    # package.json 生命周期脚本
    npm_scripts = []
    for pkg in sorted(repo.rglob("package.json")):
        if "node_modules" in pkg.parts:
            continue
        try:
            data = json.loads(pkg.read_text(encoding="utf-8", errors="replace"))
        except (OSError, json.JSONDecodeError):
            continue
        hooks = sorted(
            k for k in (data.get("scripts") or {})
            if k in ("postinstall", "preinstall", "prepare", "prepack", "postpack")
        )
        if hooks:
            npm_scripts.append(f"{pkg.relative_to(repo).as_posix()}: {','.join(hooks)}")
    facts["npm_lifecycle_hooks"] = npm_scripts

    # submodules
    rc, out = run(["git", "config", "--file", ".gitmodules", "--name-only", "--get-regexp", r"\.path$"], cwd=repo)
    facts["submodules"] = [line.rsplit(".", 1)[0] for line in out.splitlines() if rc == 0 and line]

    # CI 下载项（只记文件与行号，不复制内容）
    ci_downloads = []
    wf_dir = repo / ".github" / "workflows"
    if wf_dir.is_dir():
        for wf in sorted(wf_dir.glob("*.y*ml")):
            try:
                lines = wf.read_text(encoding="utf-8", errors="replace").splitlines()
            except OSError:
                continue
            hits = [
                str(i + 1)
                for i, l in enumerate(lines)
                if any(k in l.lower() for k in ("curl ", "wget ", "download-action", "actions/download", "install.sh"))
            ]
            if hits:
                ci_downloads.append({"file": wf.name, "lines": hits})
    facts["ci_download_surface"] = ci_downloads

    # 不可信指令文件（只记录路径，不执行）
    untrusted = []
    if files is not None:
        seen_dirs = set()
        for f in files:
            parts = Path(f).parts
            for i, seg in enumerate(parts):
                if seg in UNTRUSTED_DIRS:
                    key = "/".join(parts[: i + 1])
                    if key not in seen_dirs:
                        seen_dirs.add(key)
                        untrusted.append(key)
                    break
            name = parts[-1]
            if name in UNTRUSTED_NAMES:
                untrusted.append(f)
            if name == "copilot-instructions.md" and ".github" in parts:
                untrusted.append(f)
    facts["untrusted_instruction_files"] = sorted(set(untrusted))

    # 已提交 vendor 树 / 生成文件计数
    vendor_hits: dict[str, int] = {}
    gen_counts: dict[str, int] = {}
    for f in files:
        segs = Path(f).parts
        for v in VENDOR_SEGMENTS:
            if v in segs:
                vendor_hits[v] = vendor_hits.get(v, 0) + 1
        for pat in GENERATED_PATTERNS:
            if Path(f).match(pat):
                gen_counts[pat] = gen_counts.get(pat, 0) + 1
    facts["committed_vendor_dirs"] = vendor_hits
    facts["generated_file_counts"] = gen_counts

    facts["indexed_file_count"] = len(files)
    return facts


def main() -> int:
    repos = json.loads((LAB / "references/repositories.json").read_text(encoding="utf-8"))
    in_scope = [r for r in repos["repositories"] if r["group"] in IN_SCOPE_GROUPS]
    out_dir = LAB / "evidence/source-intake"
    out_dir.mkdir(parents=True, exist_ok=True)

    log: list[str] = [
        "# T02 source-intake 扫描日志",
        "",
        f"- 时间：{datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "- 范围：" + ", ".join(r["id"] for r in in_scope),
        "- 方式：只读（git 元数据 + 文件系统存在性检查）；未执行任何第三方内容",
        "",
    ]

    failures = 0
    for r in in_scope:
        slug = r["local_dir"]
        repo = OTHERS / slug
        if not repo.is_dir():
            log.append(f"- FAIL {slug}: 本地克隆缺失")
            failures += 1
            continue
        try:
            facts = scan_repo(repo)
        except Exception as exc:  # noqa: BLE001 —— 记录后继续
            log.append(f"- FAIL {slug}: 扫描异常 {exc!r}")
            failures += 1
            continue
        (out_dir / f"{slug}.json").write_text(
            json.dumps(facts, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
        )
        n_untrusted = len(facts["untrusted_instruction_files"])
        log.append(
            f"- OK {slug}: HEAD={facts['head_commit'][:12] if facts['head_commit'] else '?'} "
            f"许可文件={facts['license_files_root'] or '无'} "
            f"构建入口={len(facts['build_entrypoints'])} submodules={len(facts['submodules'])} "
            f"不可信指令文件={n_untrusted}"
        )

    (out_dir / "scan-log.md").write_text("\n".join(log) + "\n", encoding="utf-8")
    print("\n".join(log))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
