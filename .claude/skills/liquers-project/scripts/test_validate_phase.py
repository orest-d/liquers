"""Unit tests of validate_phase.py (standard library only).

Run from the repository root:
    python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'
"""
import importlib.util
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "validate_phase", Path(__file__).with_name("validate_phase.py")
)
validate_phase = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(validate_phase)

PHASE4_REQUIRED = ["Steps", "Validation"]


class EmptySectionsTests(unittest.TestCase):
    def test_name_in_earlier_section_text_is_not_empty(self):
        content = (
            "### Steps\n\n"
            "- [ ] 1. Do x — close STORE-CONFORMANCE-VALIDATION-TOOL — `cargo test`\n\n"
            "### Validation\n\nRun the tests.\n"
        )
        self.assertEqual(validate_phase.empty_sections(content, PHASE4_REQUIRED, "###"), [])

    def test_heading_followed_by_heading_is_empty(self):
        content = "### Steps\n\n### Validation\n\nRun the tests.\n"
        self.assertEqual(
            validate_phase.empty_sections(content, PHASE4_REQUIRED, "###"), ["Steps"]
        )

    def test_heading_at_end_is_empty(self):
        content = "### Steps\n\n- [ ] 1. Do VALIDATION work\n\n### Validation"
        self.assertEqual(
            validate_phase.empty_sections(content, PHASE4_REQUIRED, "###"), ["Validation"]
        )

    def test_deeper_subsection_counts_as_content(self):
        content = "### Examples\n\n#### Primary\n\nA worked example.\n\n### Tests\n\nt1\n"
        self.assertEqual(
            validate_phase.empty_sections(content, ["Examples", "Tests"], "###"), []
        )


class CompactSectionTests(unittest.TestCase):
    def test_fenced_phase_heading_is_ignored(self):
        text = (
            "# d\n\n## Phase 1: High-Level Design\n\nExample:\n\n"
            "```markdown\n## Phase 4: Implementation Plan\n\nfenced\n```\n\n"
            "### Purpose\n\nreal phase 1\n\n"
            "## Phase 4: Implementation Plan\n\n### Steps\n\nreal phase 4\n"
        )
        phase1 = validate_phase.compact_section(text, 1)
        phase4 = validate_phase.compact_section(text, 4)
        self.assertIn("real phase 1", phase1)
        self.assertIn("```markdown\n## Phase 4", phase1)
        self.assertTrue(phase4.startswith("## Phase 4: Implementation Plan\n\n### Steps"))
        self.assertIn("real phase 4", phase4)
        self.assertNotIn("fenced", phase4)

    def test_longer_fence_may_contain_a_shorter_one(self):
        text = (
            "# d\n\n## Phase 1: High-Level Design\n\n"
            "````markdown\n```rust\n## Phase 3: inside\n````\n\nreal phase 1\n\n"
            "## Phase 2: Architecture\n\n```rust\nfn f() {}\n```\n\nreal phase 2\n"
        )
        phase1 = validate_phase.compact_section(text, 1)
        phase2 = validate_phase.compact_section(text, 2)
        self.assertIn("real phase 1", phase1)
        self.assertIsNone(validate_phase.compact_section(text, 3))
        self.assertIsNotNone(phase2)
        self.assertIn("real phase 2", phase2)


class ApiDocsLineTests(unittest.TestCase):
    PHASE1 = "## Documentation Intent\n- Reference: none\n- Guide: none\n"

    def test_phase1_without_api_docs_line_fails_after_gate_date(self):
        problems = validate_phase.api_docs_line_problems(self.PHASE1, {"created": "2026-11-03"})
        self.assertEqual(len(problems), 1)
        self.assertIn("API docs", problems[0])

    def test_phase1_without_api_docs_line_passes_before_gate_date(self):
        self.assertEqual(
            validate_phase.api_docs_line_problems(self.PHASE1, {"created": "2026-11-02"}), []
        )

    def test_phase1_with_api_docs_line_passes(self):
        for line in ("- API docs: none — docs only\n", "- **API docs:** `liquers_core::commands`\n"):
            with self.subTest(line=line):
                problems = validate_phase.api_docs_line_problems(
                    self.PHASE1 + line, {"created": "2026-12-01"}
                )
                self.assertEqual(problems, [])

    def test_api_docs_line_inside_a_fence_does_not_count(self):
        text = self.PHASE1 + "```markdown\n- API docs: none\n```\n"
        self.assertEqual(
            len(validate_phase.api_docs_line_problems(text, {"created": "2026-12-01"})), 1
        )


if __name__ == "__main__":
    unittest.main()
