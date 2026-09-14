from pathlib import Path
import sys
import unittest
TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))
import validate_pack

class PackValidationTests(unittest.TestCase):
    def test_real_pack(self):
        r=validate_pack.validate(TOOLS.parent)
        self.assertEqual(r['repositories'],64)
        self.assertEqual(r['profiles'],38)
        self.assertEqual(r['tasks'],80)
        self.assertEqual(r['official_sources'],31)
    def test_cycle_rejected(self):
        with self.assertRaises(ValueError):
            validate_pack.check_graph({'T01':['T02'],'T02':['T01']})
    def test_missing_dependency_rejected(self):
        with self.assertRaises(ValueError):
            validate_pack.check_graph({'T01':['T09']})
    def test_valid_graph(self):
        validate_pack.check_graph({'T01':[], 'T02':['T01']})
    def test_duplicate_ids_rejected(self):
        with self.assertRaises(ValueError):
            validate_pack.unique_ids([{'id':'A'},{'id':'A'}], 'fixture')

if __name__=='__main__':unittest.main()
