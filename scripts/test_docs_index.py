import tempfile
import unittest
from pathlib import Path

from scripts import docs_index


class RelativeLinkTests(unittest.TestCase):
    def test_stable_paths_ignore_host_case_ordering(self):
        paths = [
            Path("specs/guides/UNITTEST_GUIDE.md"),
            Path("specs/guides/autonomous_issue_fixing.md"),
            Path("specs/guides/COMMAND_REGISTRATION_GUIDE.md"),
        ]

        self.assertEqual(
            [path.as_posix() for path in docs_index.stable_paths(reversed(paths))],
            [
                "specs/guides/autonomous_issue_fixing.md",
                "specs/guides/COMMAND_REGISTRATION_GUIDE.md",
                "specs/guides/UNITTEST_GUIDE.md",
            ],
        )

    def test_valid_links_fragments_and_urls(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            specs = root / "specs"
            (specs / "guides").mkdir(parents=True)
            (specs / "reference" / "api").mkdir(parents=True)
            (root / "liquers-core" / "src").mkdir(parents=True)
            (specs / "reference" / "X.md").write_text("target", encoding="utf-8")
            (root / "liquers-core" / "src" / "context.rs").write_text("", encoding="utf-8")
            (specs / "guides" / "G.md").write_text(
                "[reference](../reference/X.md#heading) [anchor](#local) "
                "[remote](https://example.com/missing)",
                encoding="utf-8",
            )
            (specs / "reference" / "api" / "A.md").write_text(
                "[source](../../../liquers-core/src/context.rs)", encoding="utf-8"
            )

            self.assertEqual(docs_index.relative_link_errors(specs), [])

    def test_missing_link_identifies_source_and_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            specs = Path(temporary) / "specs"
            (specs / "issues").mkdir(parents=True)
            (specs / "issues" / "BROKEN.md").write_text(
                "[missing](../reference/MISSING.md)", encoding="utf-8"
            )

            self.assertEqual(
                docs_index.relative_link_errors(specs),
                ["specs/issues/BROKEN.md: dead link ../reference/MISSING.md (§8.4)"],
            )

    def test_archive_is_excluded(self):
        with tempfile.TemporaryDirectory() as temporary:
            specs = Path(temporary) / "specs"
            (specs / "archive").mkdir(parents=True)
            (specs / "archive" / "OLD.md").write_text(
                "[missing](missing.md)", encoding="utf-8"
            )

            self.assertEqual(docs_index.relative_link_errors(specs), [])


def _issue(issue_id: str, design: str, priority: str = "P2") -> dict:
    return {
        "id": issue_id,
        "kind": "issue",
        "design": design,
        "file": f"specs/issues/{issue_id}.md",
        "priority": priority,
        "complexity": "S",
        "gh_issue": "",
        "readiness": "",
        "status": "draft",
        "_path": Path(f"specs/issues/{issue_id}.md"),
        "_fm": {
            "id": issue_id,
            "kind": "issue",
            "status": "draft",
            "priority": priority,
            "complexity": "S",
            "area": ["docs"],
            "design": design,
        },
    }


def _design(slug: str, issues: list[str], readiness: str = "ready") -> dict:
    fm = {
        "id": slug.upper(),
        "kind": "design",
        "status": "in_review",
        "phase": "implementation",
        "area": ["docs"],
        "issues": issues,
    }
    if readiness:
        fm["readiness"] = readiness
    return {
        "id": slug.upper(),
        "kind": "design",
        "design": slug,
        "file": f"specs/design/{slug}/DESIGN.md",
        "priority": "",
        "complexity": "",
        "gh_issue": "",
        "readiness": "",
        "status": "in_review",
        "_path": Path(f"specs/design/{slug}/DESIGN.md"),
        "_fm": fm,
    }


class MergedDesignTests(unittest.TestCase):
    def test_merged_readiness_design_may_own_several_sources(self):
        rows = [
            _issue("LEAD", "merged", priority="P1"),
            _issue("OTHER", "merged", priority="P3"),
            _design("merged", ["LEAD", "OTHER"]),
        ]

        errors = docs_index.readiness_errors(rows)

        self.assertEqual(errors, [])

    def test_merged_design_inherits_the_leading_source_and_projects_readiness(self):
        rows = [
            _issue("LEAD", "merged", priority="P1"),
            _issue("OTHER", "merged", priority="P3"),
            _design("merged", ["LEAD", "OTHER"]),
        ]

        docs_index.join_design_sources(rows)

        self.assertEqual(rows[2]["priority"], "P1")
        self.assertEqual([rows[0]["readiness"], rows[1]["readiness"]], ["ready", "ready"])

    def test_every_source_of_a_merged_design_must_link_back(self):
        rows = [
            _issue("LEAD", "merged"),
            _issue("OTHER", "elsewhere"),
            _design("merged", ["LEAD", "OTHER"]),
            _design("elsewhere", [], readiness=""),
        ]

        errors = docs_index.readiness_errors(rows)

        self.assertEqual(
            errors,
            [
                "specs/design/merged/DESIGN.md: source 'OTHER' links to design "
                "'elsewhere', expected 'merged' (§5.1.1)"
            ],
        )

    def test_a_readiness_design_needs_a_source(self):
        rows = [_design("orphan", [])]

        errors = docs_index.readiness_errors(rows)

        self.assertEqual(
            errors,
            [
                "specs/design/orphan/DESIGN.md: readiness-labeled design must name at least "
                "one source issue or feature (§5.1.1)"
            ],
        )


if __name__ == "__main__":
    unittest.main()
