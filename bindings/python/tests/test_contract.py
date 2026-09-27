import json
import sys
import unittest
from pathlib import Path

BINDING_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BINDING_ROOT))

from structural_api.client import API_VERSION, SCHEMA_VERSION  # noqa: E402


class ContractTests(unittest.TestCase):
    def test_version_constants(self):
        self.assertEqual(API_VERSION, "1.1")
        self.assertEqual(SCHEMA_VERSION, "structural-automation/1.0")

    def test_catalog_matches_binding_major(self):
        root = Path(__file__).resolve().parents[3]
        catalog = json.loads((root / "api" / "operations.json").read_text())
        self.assertEqual(catalog["api_version"].split(".")[0], API_VERSION.split(".")[0])


if __name__ == "__main__":
    unittest.main()
