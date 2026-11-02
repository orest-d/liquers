"""Tests for check-workspace-metadata.py (liquers-py-package Phase 3).

Run: python3 -m unittest scripts/test_check_workspace_metadata.py
"""

from __future__ import annotations

import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location(
    "check_workspace_metadata", ROOT / "scripts" / "check-workspace-metadata.py"
)
assert _spec is not None and _spec.loader is not None
cwm = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cwm)


def _copy_workspace(target: Path) -> None:
    """Copy just what the checker reads: the manifests and the two LICENSE files."""
    shutil.copy(ROOT / "Cargo.toml", target / "Cargo.toml")
    shutil.copy(ROOT / "LICENSE", target / "LICENSE")
    import tomllib

    members = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["members"]
    for member in members:
        (target / member).mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / member / "Cargo.toml", target / member / "Cargo.toml")
    shutil.copy(ROOT / "liquers-py" / "LICENSE", target / "liquers-py" / "LICENSE")


class CheckWorkspaceMetadataTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        _copy_workspace(self.root)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_workspace_is_consistent(self) -> None:
        findings, version = cwm.check(ROOT, None)
        self.assertEqual(findings, [])
        self.assertIsNotNone(version)

    def test_tag_mismatch_is_refused(self) -> None:
        findings, version = cwm.check(self.root, "liquers-py-v999.0.0")
        self.assertEqual(len(findings), 1, findings)
        self.assertIn("999.0.0", findings[0])
        self.assertIn(str(version), findings[0])

    def test_matching_tag_is_accepted(self) -> None:
        _, version = cwm.check(self.root, None)
        findings, _ = cwm.check(self.root, f"liquers-py-v{version}")
        self.assertEqual(findings, [])

    def test_no_tag_skips_the_tag_check(self) -> None:
        self.assertEqual(cwm.main(["--root", str(self.root)]), 0)

    def test_member_without_inheritance_is_refused(self) -> None:
        manifest = self.root / "liquers-records" / "Cargo.toml"
        text = manifest.read_text().replace("license.workspace = true", 'license = "MIT"', 1)
        manifest.write_text(text)
        findings, _ = cwm.check(self.root, None)
        self.assertEqual(len(findings), 1, findings)
        self.assertIn("liquers-records", findings[0])
        self.assertIn("license", findings[0])

    def test_license_copies_differ(self) -> None:
        (self.root / "liquers-py" / "LICENSE").write_text("not the license\n")
        findings, _ = cwm.check(self.root, None)
        self.assertEqual(len(findings), 1, findings)
        self.assertIn("liquers-py/LICENSE", findings[0])


if __name__ == "__main__":
    unittest.main()
