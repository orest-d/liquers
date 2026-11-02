"""Static checks of .github/workflows/liquers-py-release.yml (liquers-py-package AC-8's parts).

Publishing cannot be tested without publishing, so these check the workflow that does it: every
publishing job runs only on a `liquers-py-v*` tag, and PyPI comes only after TestPyPI, through the
protected `pypi` environment, with trusted publishing (`id-token: write`) and no token.

Run: python3 -m unittest scripts/test_release_workflow.py   (needs PyYAML)
"""

from __future__ import annotations

import unittest
from pathlib import Path

import yaml

WORKFLOW = Path(__file__).resolve().parent.parent / ".github" / "workflows" / "liquers-py-release.yml"
PUBLISHING = ("github-release", "testpypi", "pypi")
TAG_ONLY = "startsWith(github.ref, 'refs/tags/liquers-py-v')"


def _needs(job: dict) -> set[str]:
    needs = job.get("needs", [])
    return {needs} if isinstance(needs, str) else set(needs)


class ReleaseWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        cls.jobs = cls.workflow["jobs"]

    def test_publishing_jobs_are_tag_only(self) -> None:
        for name in PUBLISHING:
            with self.subTest(job=name):
                self.assertEqual(self.jobs[name].get("if"), TAG_ONLY)
        for name, job in self.jobs.items():
            if name in PUBLISHING:
                continue
            steps = job.get("steps", [])
            uses = " ".join(str(step.get("uses", "")) for step in steps)
            with self.subTest(job=name):
                self.assertNotIn("gh-action-pypi-publish", uses)
                self.assertNotIn("action-gh-release", uses)

    def test_pypi_waits_for_testpypi_and_its_environment(self) -> None:
        pypi = self.jobs["pypi"]
        self.assertEqual(_needs(pypi), {"testpypi"})
        self.assertEqual(pypi.get("environment"), "pypi")
        self.assertEqual(pypi.get("permissions"), {"id-token": "write"})
        testpypi = self.jobs["testpypi"]
        self.assertEqual(testpypi.get("environment"), "testpypi")
        self.assertTrue({"dist", "test", "test-sdist"} <= _needs(testpypi))
        for name in ("testpypi", "pypi"):
            for step in self.jobs[name]["steps"]:
                with self.subTest(job=name, step=step.get("uses") or step.get("name")):
                    self.assertNotIn("password", step.get("with", {}) or {})

    def test_only_tags_publish_and_the_default_permission_is_read(self) -> None:
        self.assertEqual(self.workflow["permissions"], {"contents": "read"})
        triggers = self.workflow[True] if True in self.workflow else self.workflow["on"]
        self.assertEqual(triggers["push"], {"tags": ["liquers-py-v*"]})


if __name__ == "__main__":
    unittest.main()
