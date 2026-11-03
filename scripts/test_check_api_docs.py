"""Unit tests of check_api_docs.py (standard library only).

Run from the repository root:
    python3 -m unittest -v scripts.test_check_api_docs
"""

import io
import tempfile
import textwrap
import unittest
from pathlib import Path

from scripts import check_api_docs as cad

GOOD_DOC = textwrap.dedent(
    """\
    //! Widgets: the things a frobnicator frobs.
    //!
    //! # Main components
    //! - [`Widget`] — one widget.
    //! - [`WidgetSet`] — many.
    //!
    //! # Where it is used
    //! - [`crate::frob`] frobs them.
    //! - every test that builds a set.
    //!
    //! # Related concepts
    //! [`crate::gadget`] is a widget with a handle.
    //!
    //! # Examples
    //! ```
    //! # fn main() {
    //! let w = Widget::new();
    //! let set = WidgetSet::from([w]);
    //! assert_eq!(set.len(), 1);
    //! # }
    //! ```
    //! - [`lib.rs`](https://github.com/orest-d/liquers/blob/main/liquers-foo/src/lib.rs) — the crate root.
    """
)
TWO_ITEMS = "pub struct Widget;\npub struct WidgetSet;\n"


def make_repo(root: Path, lib_rs: str, modules: dict | None = None) -> Path:
    """A minimal workspace: Cargo.toml + specs/ at the root, one crate `liquers-foo`."""
    (root / "specs").mkdir(exist_ok=True)
    (root / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
    crate = root / "liquers-foo"
    (crate / "src").mkdir(parents=True, exist_ok=True)
    (crate / "Cargo.toml").write_text("[package]\nname = \"liquers-foo\"\n", encoding="utf-8")
    (crate / "src" / "lib.rs").write_text(lib_rs, encoding="utf-8")
    for name, text in (modules or {}).items():
        path = crate / "src" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    return crate


def run(root: Path, *argv: str) -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    code = cad.run(list(argv), root=root, out=out, err=err)
    return code, out.getvalue()


class RustModuleTests(unittest.TestCase):
    def test_public_module_without_doc_warns(self):
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": TWO_ITEMS})
            code, out = run(Path(tmp))
        self.assertEqual(code, 0)
        self.assertIn("liquers-foo/src/widgets.rs:1: warning: no module doc", out)

    def test_module_under_private_parent_is_not_public(self):
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(
                Path(tmp),
                "//! Crate.\nmod inner;\n",
                {"inner.rs": "//! Inner.\npub mod widgets;\n", "inner/widgets.rs": TWO_ITEMS},
            )
            code, out = run(Path(tmp))
        self.assertEqual(code, 0)
        self.assertIn("inner/widgets.rs:1: warning: no module doc", out)
        self.assertNotIn("missing heading", out)

    def test_single_item_module_needs_only_purpose(self):
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod one;\n", {"one.rs": "//! One thing.\npub struct One;\n"})
            _, out = run(Path(tmp))
        self.assertEqual(out, "")

    def test_missing_section_warns(self):
        doc = GOOD_DOC.replace("//! # Where it is used\n//! - [`crate::frob`] frobs them.\n//! - every test that builds a set.\n//!\n", "")
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": doc + TWO_ITEMS})
            _, out = run(Path(tmp))
        self.assertIn("missing heading `# Where it is used`", out)
        self.assertNotIn("missing heading `# Examples`", out)

    def test_headings_out_of_order_warn(self):
        doc = GOOD_DOC.replace("# Related concepts", "# TEMP").replace("# Examples", "# Related concepts").replace("# TEMP", "# Examples")
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": doc + TWO_ITEMS})
            _, out = run(Path(tmp))
        self.assertIn("headings out of order", out)

    def test_more_than_five_use_links_warn(self):
        uses = "".join(f"//! - [`crate::u{i}`] uses it.\n" for i in range(6))
        doc = GOOD_DOC.replace("//! - [`crate::frob`] frobs them.\n", uses)
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": doc + TWO_ITEMS})
            _, out = run(Path(tmp))
        self.assertIn("more than 5 links under `# Where it is used`", out)

    def test_more_than_five_example_links_warn(self):
        links = "".join(f"//! - [t{i}](https://github.com/orest-d/liquers/blob/main/liquers-foo/src/lib.rs) — t.\n" for i in range(6))
        doc = GOOD_DOC + links
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": doc + TWO_ITEMS})
            _, out = run(Path(tmp))
        self.assertIn("more than 5 example links", out)

    def test_example_length_counts_visible_lines_only(self):
        # Three visible lines plus hidden `# ` lines: fine. Twelve visible lines: a warning.
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": GOOD_DOC + TWO_ITEMS})
            _, out = run(Path(tmp))
            self.assertNotIn("visible lines", out)
            long = GOOD_DOC.replace("//! let w = Widget::new();\n", "//! let w = Widget::new();\n" * 10)
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": long + TWO_ITEMS})
            _, out = run(Path(tmp))
        self.assertIn("the example has 12 visible lines; 3-10 expected", out)

    def test_no_example_warns(self):
        doc = GOOD_DOC.split("//! ```\n")[0] + "//! See the tests.\n"
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": doc + TWO_ITEMS})
            _, out = run(Path(tmp))
        self.assertIn("no example under `# Examples`", out)

    def test_github_main_link_to_missing_path_is_error(self):
        doc = GOOD_DOC.replace("liquers-foo/src/lib.rs", "liquers-foo/tests/widgets.rs")
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": doc + TWO_ITEMS})
            code, out = run(Path(tmp))
        self.assertEqual(code, 1)
        self.assertIn("widgets.rs:22: error: GitHub link to a path that does not exist: liquers-foo/tests/widgets.rs", out)

    def test_github_main_link_to_existing_path_is_fine(self):
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": GOOD_DOC + TWO_ITEMS})
            code, out = run(Path(tmp))
        self.assertEqual((code, out), (0, ""))

    def test_strict_turns_warnings_into_exit_1(self):
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": TWO_ITEMS})
            self.assertEqual(run(Path(tmp))[0], 0)
            self.assertEqual(run(Path(tmp), "--strict")[0], 1)

    def test_json_format_lists_findings(self):
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod widgets;\n", {"widgets.rs": TWO_ITEMS})
            _, out = run(Path(tmp), "--format", "json")
        import json

        findings = json.loads(out)
        self.assertEqual(findings[0]["level"], "warning")
        self.assertEqual(findings[0]["path"], "liquers-foo/src/widgets.rs")

    def test_path_argument_limits_the_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            make_repo(Path(tmp), "//! Crate.\npub mod a;\npub mod b;\n", {"a.rs": TWO_ITEMS, "b.rs": TWO_ITEMS})
            _, out = run(Path(tmp), str(Path(tmp) / "liquers-foo" / "src" / "a.rs"))
        self.assertIn("a.rs", out)
        self.assertNotIn("b.rs", out)


class PythonAndJavaScriptTests(unittest.TestCase):
    def test_python_module_docstring_is_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_repo(root, "//! Crate.\n")
            package = root / "liquers-py" / "python" / "liquers"
            package.mkdir(parents=True)
            (package / "__init__.py").write_text(
                '"""Liquers from Python.\n\n# Main components\n- `evaluate`\n\n# Where it is used\n'
                "- the tests\n\n# Related concepts\n- none\n\n# Examples\n>>> 1 + 1\n2\n>>> 2 + 2\n4\n\n"
                '"""\nX = 1\n',
                encoding="utf-8",
            )
            (package / "_private.py").write_text('"""Helpers."""\n', encoding="utf-8")
            docs = cad.python_docs(package)
            code, out = run(root)
        self.assertEqual([d.full for d in docs], [True, False])
        self.assertEqual((code, out), (0, ""))

    def test_python_docstring_example_too_short_warns(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_repo(root, "//! Crate.\n")
            package = root / "liquers-py" / "python" / "liquers"
            package.mkdir(parents=True)
            (package / "__init__.py").write_text(
                '"""P.\n\n# Main components\n- a\n\n# Where it is used\n- b\n\n# Related concepts\n- c\n\n'
                '# Examples\n>>> 1\n1\n"""\n',
                encoding="utf-8",
            )
            _, out = run(root)
        self.assertIn("the example has 2 visible lines", out)

    def test_javascript_package_documentation_is_read(self):
        block = textwrap.dedent(
            """\
            #[wasm_bindgen(typescript_custom_section)]
            const DOC: &'static str = r#"
            /**
             * Liquers in the browser.
             *
             * # Main components
             * - {@link evaluate}
             *
             * # Where it is used
             * - a page
             *
             * # Related concepts
             * - the Rust crate
             *
             * # Examples
             * @example
             * ```js
             * await init();
             * const x = await liquers.evaluate("commands_doc");
             * assert(x.startsWith("# Commands"));
             * ```
             * @packageDocumentation
             */
            "#;
            """
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_repo(root, "//! Crate.\n")
            ts = root / "liquers-web" / "src" / "typescript.rs"
            ts.parent.mkdir(parents=True)
            ts.write_text(block, encoding="utf-8")
            doc = cad.javascript_doc(ts)
            self.assertTrue(doc.text.startswith("Liquers in the browser."))
            self.assertEqual(doc.line, 3)
            self.assertEqual(cad.check_doc(doc, root), [])
            ts.write_text("const X: &str = \"\";\n", encoding="utf-8")
            self.assertEqual(cad.check_doc(cad.javascript_doc(ts), root)[0].message, "no @packageDocumentation block")


    def test_javascript_example_ends_at_its_closing_fence(self):
        """Links and prose after the fenced `@example` are not example lines (Codex review of
        PR #254): a three-line example followed by six links warns about the links, not the
        length, and a ten-line example followed by a sentence is within the limit."""
        def block(code_lines, trailer):
            code = "\n".join(f" * {line}" for line in code_lines)
            return textwrap.dedent(
                f"""\
                #[wasm_bindgen(typescript_custom_section)]
                const DOC: &'static str = r##"
                /**
                 * P.
                 *
                 * # Main components
                 * - a
                 *
                 * # Where it is used
                 * - b
                 *
                 * # Related concepts
                 * - c
                 *
                 * # Examples
                 * @example
                 * ```js
                {code}
                 * ```
                 *
                {trailer}
                 * @packageDocumentation
                 */
                "##;
                """
            )

        links = "\n".join(
            f" * - [t{i}](https://github.com/orest-d/liquers/blob/main/liquers-foo/src/lib.rs) — t." for i in range(6)
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            make_repo(root, "//! Crate.\n")
            ts = root / "liquers-web" / "src" / "typescript.rs"
            ts.parent.mkdir(parents=True)
            ts.write_text(block(["a();", "b();", "c();"], links), encoding="utf-8")
            messages = [f.message for f in cad.check_doc(cad.javascript_doc(ts), root)]
            self.assertEqual(messages, ["more than 5 example links under `# Examples`"])
            ts.write_text(block([f"l{i}();" for i in range(10)], " * More are under `tests/`."), encoding="utf-8")
            self.assertEqual(cad.check_doc(cad.javascript_doc(ts), root), [])
            ts.write_text(block([f"l{i}();" for i in range(11)], " * More are under `tests/`."), encoding="utf-8")
            messages = [f.message for f in cad.check_doc(cad.javascript_doc(ts), root)]
            self.assertEqual(messages, ["the example has 11 visible lines; 3-10 expected"])


class RepositoryTests(unittest.TestCase):
    ROOT = Path(__file__).resolve().parent.parent

    def test_repository_worked_examples_are_clean(self):
        paths = [
            "liquers-core/src/commands.rs",
            "liquers-py/python/liquers/__init__.py",
            "liquers-web/src/lib.rs",
            "liquers-web/src/typescript.rs",
            "liquers-docs/src/lib.rs",
        ]
        code, out = run(self.ROOT, *[str(self.ROOT / p) for p in paths])
        self.assertEqual((code, out), (0, ""))

    def test_repository_has_no_error(self):
        code, _ = run(self.ROOT)
        self.assertEqual(code, 0)


if __name__ == "__main__":
    unittest.main()
