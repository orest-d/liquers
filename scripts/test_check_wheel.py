"""Tests for check-wheel.py (liquers-py-package Phase 3), on synthetic wheels.

Run: python3 -m unittest scripts/test_check_wheel.py
"""

from __future__ import annotations

import importlib.util
import io
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("check_wheel", ROOT / "scripts" / "check-wheel.py")
assert _spec is not None and _spec.loader is not None
cw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cw)

LINUX = "liquers_py-0.1.0-cp314-abi3-manylinux_2_28_x86_64.whl"
AUDITWHEEL = """liquers_py-0.1.0-cp314-abi3-manylinux_2_28_x86_64.whl is consistent with the following
platform tag: "manylinux_2_28_x86_64".

The wheel references external versioned symbols in these system-provided shared libraries:
libgcc_s.so.1 with versions {'GCC_4.2.0'}, libc.so.6 with versions {'GLIBC_2.28'}, libm.so.6
with versions {'GLIBC_2.27'}
"""


def _wheel(directory: Path, name: str, files: dict[str, bytes], pad: int = 0) -> Path:
    path = directory / name
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_STORED) as archive:
        archive.writestr("liquers/_native.abi3.so", b"\0" * pad)
        for member, data in files.items():
            archive.writestr(member, data)
    return path


def _sdist(directory: Path, with_lock: bool) -> Path:
    path = directory / "liquers_py-0.1.0.tar.gz"
    with tarfile.open(path, "w:gz") as archive:
        names = ["liquers_py-0.1.0/pyproject.toml"]
        if with_lock:
            names.append("liquers_py-0.1.0/Cargo.lock")
        for name in names:
            info = tarfile.TarInfo(name)
            info.size = 0
            archive.addfile(info, io.BytesIO(b""))
    return path


class CheckWheelTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_clean_wheel_passes(self) -> None:
        wheel = _wheel(self.dir, LINUX, {})
        self.assertEqual(cw.check_wheel(wheel, AUDITWHEEL, 90, "2_28"), [])

    def test_grafted_library_is_refused(self) -> None:
        wheel = _wheel(self.dir, LINUX, {"liquers_py.libs/libssl-1a2b.so.3": b"x"})
        findings = cw.check_wheel(wheel, None, 90, "2_28")
        self.assertEqual(len(findings), 1, findings)
        self.assertIn("liquers_py.libs/libssl-1a2b.so.3", findings[0])

    def test_unlisted_dependency_is_refused(self) -> None:
        wheel = _wheel(self.dir, LINUX, {})
        findings = cw.check_wheel(wheel, AUDITWHEEL + " libssl.so.3 with versions {}\n", 90, "2_28")
        self.assertEqual(len(findings), 1, findings)
        self.assertIn("libssl.so.3", findings[0])
        windows = _wheel(self.dir, "liquers_py-0.1.0-cp314-abi3-win_arm64.whl", {})
        listing = "  Image has the following dependencies:\n\n    KERNEL32.dll\n    libcrypto-3.dll\n"
        findings = cw.check_wheel(windows, listing, 90, "2_28")
        self.assertEqual(findings, [
            "liquers_py-0.1.0-cp314-abi3-win_arm64.whl: links libcrypto-3.dll, which is not in the windows allow-list"
        ])

    def test_oversized_wheel_is_refused(self) -> None:
        wheel = _wheel(self.dir, LINUX, {}, pad=2_000_000)
        findings = cw.check_wheel(wheel, None, 1, "2_28")
        self.assertEqual(len(findings), 1, findings)
        self.assertIn("over the 1 MB limit", findings[0])

    def test_incomplete_dist_is_refused(self) -> None:
        platforms = {
            "linux_x86_64": "manylinux_2_28_x86_64",
            "linux_aarch64": "manylinux_2_28_aarch64",
            "macos_arm64": "macosx_11_0_arm64",
            "win_amd64": "win_amd64",
            "win_arm64": "win_arm64",
        }
        for tag in platforms.values():
            for python_tag, abi_tag in cw.BUILDS:
                _wheel(self.dir, f"liquers_py-0.1.0-{python_tag}-{abi_tag}-{tag}.whl", {})
        _sdist(self.dir, with_lock=True)
        self.assertEqual(cw.check_dist(self.dir), [])
        (self.dir / "liquers_py-0.1.0-cp315t-cp315t-win_arm64.whl").unlink()
        (self.dir / "liquers_py-0.1.0.tar.gz").unlink()
        _sdist(self.dir, with_lock=False)
        findings = cw.check_dist(self.dir)
        self.assertEqual(sorted(findings), sorted([
            "missing wheel for win_arm64 cp315t-cp315t",
            "liquers_py-0.1.0.tar.gz: no Cargo.lock in the sdist",
        ]))


if __name__ == "__main__":
    unittest.main()
