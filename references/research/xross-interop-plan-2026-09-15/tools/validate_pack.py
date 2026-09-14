#!/usr/bin/env python3
"""Validate this planning pack. Offline, read-only; never builds reference code."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
from typing import Any
from urllib.parse import unquote, urlsplit


def unique_ids(items: list[dict[str, Any]], label: str) -> set[str]:
    ids: set[str] = set()
    for item in items:
        value = item.get("id")
        if not isinstance(value, str) or not value or value in ids:
            raise ValueError(f"{label}: missing or duplicate id {value!r}")
        ids.add(value)
    return ids


def check_graph(graph: dict[str, list[str]]) -> None:
    """Reject unknown targets and cycles, including selected-profile dependencies."""
    done: set[str] = set()
    active: set[str] = set()

    def visit(node: str) -> None:
        if node not in graph:
            raise ValueError(f"Unknown dependency: {node}")
        if node in active:
            raise ValueError(f"Dependency cycle at {node}")
        if node in done:
            return
        active.add(node)
        for dependency in graph[node]:
            visit(dependency)
        active.remove(node)
        done.add(node)

    for node in graph:
        visit(node)


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def _read_json(root: Path, name: str) -> Any:
    return json.loads((root / "manifests" / name).read_text(encoding="utf-8"))


def validate(root: Path) -> dict[str, Any]:
    root = root.resolve(strict=True)
    repositories = _read_json(root, "repositories.json")["repositories"]
    sources = _read_json(root, "official-sources.json")["sources"]
    profiles = _read_json(root, "protocols.json")["profiles"]
    task_document = _read_json(root, "tasks.json")
    tasks = task_document["tasks"]
    requirement_map = _read_json(root, "requirements-map.json")
    profile_map = _read_json(root, "profile-task-map.json")
    repository_ids = unique_ids(repositories, "repositories")
    source_ids = unique_ids(sources, "official sources")
    profile_ids = unique_ids(profiles, "profiles")
    task_ids = unique_ids(tasks, "tasks")
    reference_ids = repository_ids | source_ids
    _require((len(repositories), len(profiles), len(tasks)) == (64, 38, 80),
             "Unexpected inventory/profile/task count; update expectations when revising pack")
    _require(task_document["task_count"] == len(tasks), "Stale task_count")
    _require(set(profile_map) == profile_ids, "Profile coverage is incomplete")
    specification = (root / "docs/01-functional-spec.md").read_text(encoding="utf-8")
    requirement_ids = set(re.findall(r"\|\s*((?:CORE|FILE|MEDIA|SEC|APP|INT)-\d{2})\s*\|", specification))
    _require(set(requirement_map) == requirement_ids, "Requirement map differs from functional spec")
    actual_map = {key: [] for key in requirement_ids}
    graph: dict[str, list[str]] = {}
    all_cases: list[dict[str, Any]] = []
    for task in tasks:
        _require(task["status"] == "planned", f"Unexpected completed claim: {task['id']}")
        _require(bool(task["cases"]), f"Missing acceptance cases: {task['id']}")
        _require(set(task["references"]) <= reference_ids, f"Unknown source in {task['id']}")
        _require(set(task["requirements"]) <= requirement_ids, f"Unknown requirement in {task['id']}")
        for key in task["requirements"]:
            actual_map[key].append(task["id"])
        graph[task["id"]] = task["depends_on"] + task.get("conditional_dependencies", [])
        for field in ("files", "consumes", "produces", "implementation", "verification_command"):
            _require(bool(task[field]), f"Missing {field} in {task['id']}")
        all_cases.extend(task["cases"])
    check_graph(graph)
    unique_ids(all_cases, "acceptance cases")
    for key in requirement_ids:
        _require(bool(actual_map[key]), f"Uncovered requirement: {key}")
        _require(set(actual_map[key]) == set(requirement_map[key]), f"Stale coverage: {key}")
    for key, values in profile_map.items():
        _require(bool(values) and set(values) <= task_ids, f"Uncovered/invalid profile: {key}")
    for profile in profiles:
        _require(set(profile["reference_ids"]) <= reference_ids, f"Unknown profile source: {profile['id']}")
        _require(profile["production_approved"] is False, f"Unintended profile approval: {profile['id']}")
    for repository in repositories:
        _require(repository["production_approved"] is False, f"Unintended repository approval: {repository['id']}")
        _require(repository["license_audited"] is False, f"Unintended license audit claim: {repository['id']}")
        _require(repository["resolved_commit"] is None, f"Inventory must not pretend to be lock: {repository['id']}")
    # Follow document paths only; anchors are not checked. Ignore examples inside fences.
    link_count = 0
    documents = sorted(root.rglob("*.md"))
    for path in documents:
        text = path.read_text(encoding="utf-8")
        unfenced = re.sub(r"^(```|~~~).*?^\1[^\n]*$", "", text, flags=re.M | re.S)
        for target in re.findall(r"\]\(([^)\s]+)\)", unfenced):
            target = target.strip("<>")
            parsed = urlsplit(target)
            if parsed.scheme or target.startswith("#"):
                continue
            relative_path = unquote(parsed.path)
            if not relative_path:
                continue
            resolved = (path.parent / relative_path).resolve()
            _require(resolved.is_relative_to(root), f"Link leaves pack: {path.name}: {target}")
            _require(resolved.exists(), f"Broken local link: {path.relative_to(root)}: {target}")
            link_count += 1
    for filename in ("run-manifest.example.json", "provenance-record.example.json"):
        example = json.loads((root / "templates" / filename).read_text(encoding="utf-8"))
        _require(example.get("synthetic") is True, f"Unmarked synthetic fixture: {filename}")
    return dict(status="PASS", repositories=len(repositories), official_sources=len(sources),
                profiles=len(profiles), requirements=len(requirement_ids), tasks=len(tasks),
                acceptance_cases=len(all_cases), markdown_documents=len(documents),
                local_links_checked=link_count, dependency_graph="acyclic",
                actual_protocol_tests_run=False,
                note="Offline structural validation only; not a safety, license or device-compatibility certification.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        print(json.dumps(validate(args.root), ensure_ascii=False, indent=2))
        return 0
    except (ValueError, OSError, KeyError, TypeError) as error:
        print(f"Validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
