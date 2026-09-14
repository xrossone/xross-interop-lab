#!/usr/bin/env python3
"""Emit a Bash clone plan. Never download, checkout, build, or run repository code.
Use an isolated, patched environment to execute any generated plan.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import re
import shlex
import sys
from urllib.parse import urlsplit

DEFAULT_GROUPS = frozenset({'file-core', 'airplay'})
ALLOWED_HOSTS = frozenset({'github.com', 'chromium.googlesource.com', 'android.googlesource.com'})

def validate_entries(entries: list[dict]) -> list[dict]:
    if not isinstance(entries, list):
        raise ValueError('repositories must be a list')
    seen_ids: set[str] = set()
    seen_dirs: set[str] = set()
    for e in entries:
        if not isinstance(e, dict):
            raise ValueError('each repository must be an object')
        for field in ('id', 'name', 'group', 'clone_url', 'local_dir', 'page_review'):
            if not isinstance(e.get(field), str) or not e[field]:
                raise ValueError(f'missing/invalid field: {field}')
        if not re.fullmatch(r'R[0-9]{2,4}', e['id']):
            raise ValueError('invalid repository id')
        if not re.fullmatch(r'[a-z0-9][a-z0-9._-]{0,120}', e['local_dir']):
            raise ValueError('local_dir must be one safe path component')
        if e['local_dir'] in ('.', '..'):
            raise ValueError('path traversal')
        u = urlsplit(e['clone_url'])
        if u.scheme != 'https' or u.hostname not in ALLOWED_HOSTS:
            raise ValueError('only approved public HTTPS hosts are supported')
        if u.username or u.password or u.port or u.query or u.fragment:
            raise ValueError('credentials, ports, query strings and fragments are forbidden')
        if not re.fullmatch(r'/[A-Za-z0-9._/-]+', u.path) or '..' in u.path.split('/'):
            raise ValueError('invalid repository path')
        if e['id'] in seen_ids or e['local_dir'] in seen_dirs:
            raise ValueError('duplicate id or destination')
        seen_ids.add(e['id']); seen_dirs.add(e['local_dir'])
    return entries

def generate_script(entries: list[dict], root: Path, groups: list[str] | None = None,
                    include_all: bool = False, include_unverified: bool = False) -> str:
    validate_entries(entries)
    if include_all and groups:
        raise ValueError('--all and --group cannot be combined')
    known_groups = {e['group'] for e in entries}
    if groups and not set(groups).issubset(known_groups):
        raise ValueError('unknown group(s): ' + ', '.join(sorted(set(groups) - known_groups)))
    selected_groups = set(groups) if groups else DEFAULT_GROUPS
    root = root.expanduser().absolute()
    q = shlex.quote
    lines = ['#!/usr/bin/env bash', 'set -euo pipefail',
             '# Generated plan: review before executing in an isolated environment.',
             '# Fetches public Git sources only. No checkout, submodules, installs or builds.',
             '# Shallow HEAD is not a reproducible lock until resolve_lock.py records its commit.',
             'export GIT_CONFIG_NOSYSTEM=1', 'export GIT_CONFIG_GLOBAL=/dev/null',
             'export GIT_TERMINAL_PROMPT=0', f'mkdir -p -- {q(str(root))}', '']
    for e in entries:
        if not include_all and e['group'] not in selected_groups:
            continue
        if e['page_review'] != 'repository_or_primary_page_read' and not include_unverified:
            continue
        dest = str(root / e['local_dir'])
        lines.extend([
            f'# {e["id"]} | {e["group"]}',
            f'if test -e {q(dest)}; then',
            f'  printf "%s\\n" {q("SKIP existing path: " + dest + " (inspect origin manually)")}',
            'else',
            '  git -c core.hooksPath=/dev/null -c protocol.file.allow=never '
            '-c protocol.ext.allow=never clone --depth=1 --no-checkout '
            f'--no-recurse-submodules -- {q(e["clone_url"])} {q(dest)}',
            'fi', ''
        ])
    lines.append('# Next: resolve immutable commits and audit sources; do not run their build scripts yet.')
    return '\n'.join(lines) + '\n'

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, default=Path(__file__).resolve().parents[1] / 'manifests/repositories.json')
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--group', action='append', help='Repeat to select groups; default: file-core and airplay')
    parser.add_argument('--all', action='store_true', help='Include large/gated groups (still only generate commands)')
    parser.add_argument('--include-unverified', action='store_true')
    args = parser.parse_args()
    try:
        data = json.loads(args.manifest.read_text(encoding='utf-8'))
        text = generate_script(data['repositories'], args.root, args.group, args.all, args.include_unverified)
    except (OSError, ValueError, KeyError) as exc:
        print(f'error: {exc}', file=sys.stderr)
        return 2
    sys.stdout.write(text)
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
