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


if __name__ == "__main__":
    unittest.main()
