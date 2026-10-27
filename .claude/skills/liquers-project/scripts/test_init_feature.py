"""Unit tests of init_feature.py (standard library only).

Run from the repository root:
    python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'
"""
import contextlib
import importlib.util
import io
import os
import re
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "init_feature", Path(__file__).with_name("init_feature.py")
)
init_feature = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(init_feature)


class SkeletonTests(unittest.TestCase):
    def test_full_form_has_no_ticked_box(self):
        previous = os.getcwd()
        with tempfile.TemporaryDirectory() as temporary:
            (Path(temporary) / "specs").mkdir()
            os.chdir(temporary)
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    folder = init_feature.init_feature("demo", compact=False, workflow=True)
            finally:
                os.chdir(previous)
            files = sorted(folder.glob("*.md"))
            self.assertIn("phase5-documentation.md", [f.name for f in files])
            for f in files:
                self.assertNotIn("- [x]", f.read_text(encoding="utf-8"), f.name)

    def test_phase5_template_preconditions_unticked(self):
        text = init_feature.skeleton("phase5-documentation.md")
        section = re.search(r"(?ms)^## Completion Preconditions\n(.*?)(?=^## |\Z)", text).group(1)
        self.assertEqual(len(re.findall(r"(?m)^- \[ \] ", section)), 5)
        self.assertNotIn("- [x]", section)
        reference = (init_feature.REFERENCES / "phase5-documentation.md").read_text(encoding="utf-8")
        before_block = reference[: reference.index("```markdown\n# Phase 5")]
        self.assertIn("evidence", before_block.split("## Summary Template", 1)[1])


if __name__ == "__main__":
    unittest.main()
