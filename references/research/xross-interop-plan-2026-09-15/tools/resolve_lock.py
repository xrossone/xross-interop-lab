#!/usr/bin/env python3
"""Read local Git metadata to record commits and top-level license-file digests.
No network requests, checkout, project builds, hooks or repository scripts are run.
This records provenance; it is not a source-code security or license audit.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlsplit
from clone_plan import validate_entries

def git_read(path: Path, *args: str) -> bytes:
    env = {k:v for k,v in os.environ.items() if not k.startswith('GIT_')}
    env.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull,
               GIT_TERMINAL_PROMPT='0', GIT_OPTIONAL_LOCKS='0', GIT_NO_LAZY_FETCH='1')
    cmd = ['git', '--no-replace-objects', '-c', 'core.hooksPath='+os.devnull,
           '-c', 'core.fsmonitor=false', '-c', 'protocol.file.allow=never',
           '-c', 'protocol.ext.allow=never', '-C', str(path), *args]
    p = subprocess.run(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                       check=False, timeout=20)
    if p.returncode:
        raise ValueError('Git read failed: '+p.stderr.decode('utf-8','replace')[:400])
    return p.stdout

def normalized_url(url: str) -> tuple[str,str]:
    u=urlsplit(url)
    path=u.path.rstrip('/')
    if path.endswith('.git'):path=path[:-4]
    return ((u.hostname or '').lower(),path.lower())

def inspect_repo(entry: dict, root: Path) -> dict:
    validate_entries([entry])
    root=root.resolve()
    path=root/entry['local_dir']
    base={'id':entry['id'],'clone_url':entry['clone_url'],'local_dir':entry['local_dir'],
          'resolved_commit':None,'production_approved':False,'license_audited':False}
    # A symlink must not redirect the metadata reader outside the quarantine root.
    resolved=path.resolve()
    if not resolved.is_relative_to(root):
        raise ValueError('repository destination escapes quarantine root')
    if not path.exists():
        return dict(base,status='missing',license_files=[])
    if path.is_symlink():
        raise ValueError('symlink repository directories require manual review')
    origin=git_read(path,'config','--local','--get','remote.origin.url').decode().strip()
    validate_entries([dict(entry,clone_url=origin)])
    if normalized_url(origin) != normalized_url(entry['clone_url']):
        raise ValueError('origin does not match approved inventory URL')
    commit=git_read(path,'rev-parse','--verify','HEAD^{commit}').decode().strip()
    if not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}',commit):
        raise ValueError('invalid commit object id')
    # Hash only plain top-level LICENSE/COPYING/NOTICE-like blobs. A full dependency
    # and per-file audit is deliberately outside the scope of this helper.
    raw_names=git_read(path,'ls-tree','--name-only','-z',commit).split(b'\0')
    names=[]
    for raw in raw_names:
        name=raw.decode('utf-8','replace')
        if re.fullmatch(r'(LICENSE|LICENCE|COPYING|NOTICE)([._-][A-Za-z0-9._-]+)?',name,re.I):
            names.append(name)
    licenses=[]
    for name in sorted(names)[:32]:
        obj=commit+':'+name
        kind=git_read(path,'cat-file','-t',obj).decode().strip()
        if kind!='blob':continue
        size=int(git_read(path,'cat-file','-s',obj).decode().strip())
        if size>1024*1024:
            licenses.append({'path':name,'status':'too-large-for-helper','size_bytes':size})
            continue
        data=git_read(path,'cat-file','blob',obj)
        licenses.append({'path':name,'size_bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
    return dict(base,status='resolved',resolved_commit=commit,origin=origin,license_files=licenses)

def main() -> int:
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--manifest',type=Path,default=Path(__file__).resolve().parents[1]/'manifests/repositories.json')
    p.add_argument('--root',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--overwrite',action='store_true')
    a=p.parse_args()
    try:
        entries=validate_entries(json.loads(a.manifest.read_text(encoding='utf-8'))['repositories'])
        if not a.root.is_dir():raise ValueError('root must be an existing quarantine directory')
        if a.output.exists() and not a.overwrite:raise ValueError('output exists; preserve it or explicitly use --overwrite')
        results=[inspect_repo(e,a.root) for e in entries]
        result={'schema_version':1,'recorded_at':datetime.now(timezone.utc).isoformat(),
                'scope':'LOCAL_GIT_SNAPSHOT_NOT_A_SECURITY_OR_LICENSE_AUDIT','repositories':results}
        a.output.parent.mkdir(parents=True,exist_ok=True)
        # Exclusive creation by default prevents silently replacing a historical lock.
        with a.output.open('w' if a.overwrite else 'x',encoding='utf-8') as f:
            json.dump(result,f,ensure_ascii=False,indent=2);f.write('\n')
    except (OSError,ValueError,KeyError,subprocess.TimeoutExpired) as exc:
        print(f'error: {exc}',file=sys.stderr);return 2
    print(f'Recorded {sum(r["status"]=="resolved" for r in results)} local repositories; others explicitly missing.')
    return 0

if __name__=='__main__':raise SystemExit(main())
