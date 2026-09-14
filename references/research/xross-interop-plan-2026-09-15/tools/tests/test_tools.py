import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))
import clone_plan
import resolve_lock

def entry(id='R01', group='file-core'):
    return dict(id=id,name='example/demo',group=group,clone_url='https://github.com/example/demo.git',local_dir=id.lower()+'-demo',page_review='repository_or_primary_page_read')

class ClonePlanTests(unittest.TestCase):
    def test_safe_plan_is_no_checkout(self):
        text=clone_plan.generate_script([entry()],Path('/tmp/research'))
        self.assertIn('--no-checkout',text)
        self.assertIn('--no-recurse-submodules',text)
        self.assertIn('--depth=1',text)
        self.assertNotIn('cargo build',text)
        self.assertNotIn('npm install',text)
    def test_reject_bad_url(self):
        e=entry();e['clone_url']='ext::sh -c danger'
        with self.assertRaises(ValueError):clone_plan.validate_entries([e])
    def test_reject_path_escape(self):
        e=entry();e['local_dir']='../escape'
        with self.assertRaises(ValueError):clone_plan.validate_entries([e])
    def test_reject_duplicate(self):
        with self.assertRaises(ValueError):clone_plan.validate_entries([entry(),entry()])
    def test_default_excludes_large_and_gated(self):
        text=clone_plan.generate_script([entry(),entry('R02','large'),entry('R03','gated')],Path('/tmp/r'))
        self.assertIn('r01-demo',text);self.assertNotIn('r02-demo',text);self.assertNotIn('r03-demo',text)
    def test_all_includes_groups(self):
        text=clone_plan.generate_script([entry('R02','large')],Path('/tmp/r'),include_all=True)
        self.assertIn('r02-demo',text)
    def test_unverified_requires_extra_flag(self):
        e=entry();e['page_review']='candidate_requires_local_verification'
        self.assertNotIn('r01-demo',clone_plan.generate_script([e],Path('/tmp/r'),include_all=True))
        self.assertIn('r01-demo',clone_plan.generate_script([e],Path('/tmp/r'),include_all=True,include_unverified=True))
    def test_unknown_group(self):
        with self.assertRaises(ValueError):clone_plan.generate_script([entry()],Path('/tmp/r'),groups=['missing'])
    def test_script_shell_syntax_with_special_root(self):
        text=clone_plan.generate_script([entry()],Path('/tmp/s p;a\'ce'))
        p=subprocess.run(['bash','-n'],input=text,text=True,capture_output=True)
        self.assertEqual(p.returncode,0,p.stderr)
    def test_real_inventory(self):
        data=json.loads((TOOLS.parent/'manifests/repositories.json').read_text())
        self.assertEqual(len(clone_plan.validate_entries(data['repositories'])),64)

class ResolveLockTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
    def tearDown(self):self.temp.cleanup()
    def test_missing_is_explicit(self):
        self.assertEqual(resolve_lock.inspect_repo(entry(),self.root)['status'],'missing')
    def make_repo(self,origin='https://github.com/example/demo.git'):
        p=self.root/'r01-demo';p.mkdir()
        env=dict(os.environ,GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL=os.devnull)
        for cmd in [
            ['git','init','-q',str(p)],
            ['git','-C',str(p),'config','user.email','fixture@example.invalid'],
            ['git','-C',str(p),'config','user.name','Synthetic Test'],
            ['git','-C',str(p),'remote','add','origin',origin],
        ]:subprocess.run(cmd,env=env,check=True,capture_output=True)
        (p/'LICENSE').write_text('Synthetic fixture license, not a real project license.\n')
        subprocess.run(['git','-C',str(p),'add','LICENSE'],env=env,check=True,capture_output=True)
        subprocess.run(['git','-C',str(p),'-c','core.hooksPath='+os.devnull,'commit','-qm','fixture'],env=env,check=True,capture_output=True)
        return p
    def test_records_commit_and_license_hash_without_build(self):
        p=self.make_repo();result=resolve_lock.inspect_repo(entry(),self.root)
        self.assertEqual(result['status'],'resolved')
        self.assertRegex(result['resolved_commit'],r'^[0-9a-f]{40,64}$')
        self.assertEqual(result['license_files'][0]['path'],'LICENSE')
        self.assertRegex(result['license_files'][0]['sha256'],r'^[0-9a-f]{64}$')
        self.assertFalse(result['production_approved'])
    def test_wrong_origin_rejected(self):
        self.make_repo('https://github.com/other/repo.git')
        with self.assertRaises(ValueError):resolve_lock.inspect_repo(entry(),self.root)
    def test_symlink_escape_rejected(self):
        with tempfile.TemporaryDirectory() as outside:
            (self.root/'r01-demo').symlink_to(outside,target_is_directory=True)
            with self.assertRaises(ValueError):resolve_lock.inspect_repo(entry(),self.root)

if __name__=='__main__':unittest.main()
