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

    def test_index_md_is_link_checked(self):
        with tempfile.TemporaryDirectory() as temporary:
            specs = Path(temporary) / "specs"
            specs.mkdir()
            (specs / "index.md").write_text(
                "[phase1](design/missing.md)", encoding="utf-8"
            )

            self.assertEqual(
                docs_index.relative_link_errors(specs),
                ["specs/index.md: dead link design/missing.md (§8.4)"],
            )

    def test_index_markdown_phase_links_resolve(self):
        text = docs_index.render_index_markdown(docs_index.collect())
        targets = [m.group(1) for m in docs_index.RELATIVE_LINK_RE.finditer(text)]

        self.assertTrue(any(t.startswith("design/") for t in targets))
        self.assertEqual(
            [t for t in targets if not (docs_index.SPECS / t).exists()], []
        )


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
        "autofix": "",
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


def _design(
    slug: str,
    issues: list[str],
    readiness: str = "ready",
    merged: str = "2026-10-05",
    autofix: str = "",
) -> dict:
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
    if autofix:
        fm["autofix"] = autofix
    if merged and len(issues) > 1:
        fm["merged"] = merged
    return {
        "id": slug.upper(),
        "kind": "design",
        "design": slug,
        "file": f"specs/design/{slug}/DESIGN.md",
        "priority": "",
        "complexity": "",
        "gh_issue": "",
        "readiness": "",
        "autofix": "",
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

    def test_several_sources_without_a_recorded_merge_are_refused(self):
        rows = [
            _issue("LEAD", "unmarked"),
            _issue("OTHER", "unmarked"),
            _design("unmarked", ["LEAD", "OTHER"], merged=""),
        ]

        errors = docs_index.readiness_errors(rows)

        self.assertEqual(
            errors,
            [
                "specs/design/unmarked/DESIGN.md: a readiness-labeled design with several "
                "sources must record the merge as `merged: YYYY-MM-DD` (§5.1.1)"
            ],
        )

    def test_a_merged_predecessor_must_be_superseded_without_readiness(self):
        predecessor = _design("old", ["LEAD"])
        predecessor["_fm"]["superseded_by"] = "merged"
        rows = [
            _issue("LEAD", "merged"),
            _issue("OTHER", "merged"),
            _design("merged", ["LEAD", "OTHER"]),
            predecessor,
        ]

        errors = [e for e in docs_index.readiness_errors(rows) if "old" in e.split(":")[0]]

        self.assertEqual(
            errors,
            [
                "specs/design/old/DESIGN.md: source 'LEAD' links to design 'merged', "
                "expected 'old' (§5.1.1)",
                "specs/design/old/DESIGN.md: source 'LEAD' is already owned by "
                "readiness-labeled design 'merged' (§5.1.1)",
                "specs/design/old/DESIGN.md: superseded_by 'merged' requires status "
                "'superseded' (§5.1.1)",
                "specs/design/old/DESIGN.md: merged into 'merged', so it must drop "
                "readiness (§5.1.1)",
            ],
        )


class BlankCodeTests(unittest.TestCase):
    """Link syntax inside code is text, not a link (DOCS-LINK-CHECK-READS-CODE-SPANS)."""

    LINK = "[" + "x](missing.md)"
    TICKS = "`" * 3

    def links(self, text):
        return [m.group(1) for m in docs_index.RELATIVE_LINK_RE.finditer(docs_index.blank_code(text))]

    def test_fenced_block_is_blanked(self):
        text = self.TICKS + "python\nlink = '[' + 'label](' + name + ')'\n" + self.LINK + "\n" + self.TICKS + "\n"
        self.assertEqual(self.links(text), [])

    def test_inline_code_span_is_blanked(self):
        self.assertEqual(self.links("see `" + self.LINK + "` here"), [])

    def test_double_backtick_span_with_inner_backtick(self):
        text = "a ``code ` " + self.LINK + "`` b " + self.LINK
        self.assertEqual(self.links(text), ["missing.md"])
        self.assertEqual(len(docs_index.blank_code(text)), len(text))

    def test_link_outside_code_is_kept(self):
        self.assertEqual(self.links("text " + self.LINK), ["missing.md"])

    def test_unclosed_backtick_is_literal(self):
        self.assertEqual(self.links("a ` stray " + self.LINK), ["missing.md"])

    def test_tilde_fence_is_blanked(self):
        text = "~~~\n" + self.LINK + "\n~~~\n" + self.LINK
        self.assertEqual(self.links(text), ["missing.md"])

    def test_shorter_fence_does_not_close(self):
        text = "````\n" + self.TICKS + "\n" + self.LINK + "\n````\nafter " + self.LINK
        self.assertEqual(self.links(text), ["missing.md"])

    def test_newlines_and_length_preserved(self):
        text = self.TICKS + "\na\nb\n" + self.TICKS + "\n`c`\n"
        blanked = docs_index.blank_code(text)
        self.assertEqual(len(blanked), len(text))
        self.assertEqual(blanked.count("\n"), text.count("\n"))

    def test_code_span_continues_across_lines(self):
        text = "a `span\n" + self.LINK + "\nends` b " + self.LINK
        self.assertEqual(self.links(text), ["missing.md"])

    def test_code_span_does_not_cross_a_blank_line(self):
        text = "a ` stray\n\n" + self.LINK + " `x`"
        self.assertEqual(self.links(text), ["missing.md"])

    def test_code_span_does_not_cross_into_a_list_item(self):
        text = "- a ` stray\n- " + self.LINK + " `x`"
        self.assertEqual(self.links(text), ["missing.md"])

    def test_escaped_backticks_do_not_open_a_span(self):
        text = "\\` " + self.LINK + " \\`"
        self.assertEqual(self.links(text), ["missing.md"])

    def test_escaped_backslash_leaves_backtick_active(self):
        text = "\\\\`" + self.LINK + "`"
        self.assertEqual(self.links(text), [])

    def test_backslash_inside_span_is_literal(self):
        text = "`a\\` " + self.LINK
        self.assertEqual(self.links(text), ["missing.md"])

    def test_relative_link_errors_ignores_code(self):
        with tempfile.TemporaryDirectory() as temporary:
            specs = Path(temporary) / "specs"
            (specs / "issues").mkdir(parents=True)
            (specs / "issues" / "CODE.md").write_text(
                "`" + self.LINK + "`\n" + self.TICKS + "\n" + self.LINK + "\n" + self.TICKS + "\n",
                encoding="utf-8",
            )
            self.assertEqual(docs_index.relative_link_errors(specs), [])


if __name__ == "__main__":
    unittest.main()


class AutofixTests(unittest.TestCase):
    def test_autofix_is_projected_onto_every_source(self):
        rows = [
            _issue("LEAD", "merged"),
            _issue("OTHER", "merged"),
            _design("merged", ["LEAD", "OTHER"], autofix="eligible"),
        ]

        docs_index.join_design_sources(rows)

        self.assertEqual([r["autofix"] for r in rows], ["eligible"] * 3)
        self.assertEqual(docs_index.autofix_errors(rows), [])

    def test_unknown_autofix_value_is_rejected(self):
        rows = [_issue("ONE", "one"), _design("one", ["ONE"], autofix="yes")]

        self.assertEqual(
            docs_index.autofix_errors(rows),
            ["specs/design/one/DESIGN.md: autofix 'yes' not in §5.1.1"],
        )

    def test_autofix_requires_readiness(self):
        rows = [_issue("ONE", "one"), _design("one", ["ONE"], readiness="", autofix="not-eligible")]

        self.assertEqual(
            docs_index.autofix_errors(rows),
            ["specs/design/one/DESIGN.md: autofix requires a readiness value (§5.1.1)"],
        )
        docs_index.join_design_sources(rows)
        self.assertEqual(rows[0]["autofix"], "")

    def test_eligible_requires_ready(self):
        rows = [
            _issue("ONE", "one"),
            _design("one", ["ONE"], readiness="needs-decision", autofix="eligible"),
        ]

        self.assertEqual(
            docs_index.autofix_errors(rows),
            [
                "specs/design/one/DESIGN.md: autofix 'eligible' requires readiness 'ready', "
                "not 'needs-decision' (§5.1.1)"
            ],
        )

    def test_eligible_requires_small_leading_source(self):
        issue = _issue("ONE", "one")
        issue["complexity"] = "L"
        rows = [issue, _design("one", ["ONE"], autofix="eligible")]

        self.assertEqual(
            docs_index.autofix_errors(rows),
            [
                "specs/design/one/DESIGN.md: autofix 'eligible' requires complexity S or M, "
                "but leading source 'ONE' is 'L' (§5.1.1)"
            ],
        )

    def test_not_eligible_needs_no_size_or_readiness_check(self):
        issue = _issue("ONE", "one")
        issue["complexity"] = "XL"
        rows = [issue, _design("one", ["ONE"], readiness="blocked", autofix="not-eligible")]

        self.assertEqual(docs_index.autofix_errors(rows), [])

    def test_board_orders_eligible_before_other_ready_work(self):
        rows = [
            _issue("B-LATER", "later"),
            _issue("A-FIRST", "first"),
            _design("later", ["B-LATER"], autofix="not-eligible"),
            _design("first", ["A-FIRST"], autofix="eligible"),
        ]
        rows[0]["id"], rows[1]["id"] = "A-NOT", "B-ELIGIBLE"
        rows[2]["_fm"]["issues"], rows[3]["_fm"]["issues"] = ["A-NOT"], ["B-ELIGIBLE"]

        docs_index.join_design_sources(rows)

        self.assertEqual(
            [r["id"] for r in docs_index.active_work_rows(rows)], ["B-ELIGIBLE", "A-NOT"]
        )
