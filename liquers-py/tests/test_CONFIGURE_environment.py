"""Configuring an environment after its commands are registered (``py-default-environment-config``).

The default environment is process-wide and cannot be configured again once built, so every test
that uses ``lq.configure`` runs its scenario in a fresh interpreter (``run_script``, like
``test_GUIDE_examples.py``'s ``run_block``).
"""

import inspect
import subprocess
import sys
import textwrap
import threading
import time

import pytest

import liquers as lq
from conftest import Opaque


def run_script(tmp_path, source, name="scenario"):
    """Runs ``source`` in its own interpreter, from ``tmp_path``; fails with its output."""
    script = tmp_path / f"{name}.py"
    script.write_text(textwrap.dedent(source))
    done = subprocess.run(
        [sys.executable, str(script)], cwd=tmp_path, capture_output=True, text=True, timeout=120
    )
    assert done.returncode == 0, done.stdout + done.stderr
    return done.stdout


def test_CONFIGURE01_configure_after_import_time_registration(tmp_path):
    # AC-1: a command registered at import, then the default configured from a file.
    (tmp_path / "commands_module.py").write_text(
        "import liquers as lq\n\n@lq.first_command\ndef hello() -> str:\n    return 'hello'\n"
    )
    data = tmp_path / "data"
    data.mkdir()
    (tmp_path / "liquers.yaml").write_text(
        "store:\n  stores:\n"
        f"    - {{type: filesystem, prefix: data, config: {{path: {data}}}}}\n"
    )
    run_script(
        tmp_path,
        """
        import pathlib
        import liquers as lq
        import commands_module                     # the default environment exists from here on

        lq.configure(config=pathlib.Path("liquers.yaml"))
        assert lq.evaluate("hello") == "hello"
        lq.default_environment().store_set("data/note.txt", "kept")
        assert (pathlib.Path("data") / "data" / "note.txt").read_text() == "kept"
        """,
    )


def test_CONFIGURE02_keywords_apply_to_earlier_registrations(tmp_path):
    # AC-2: conversion, async loops and job capacity set after the commands were registered.
    run_script(
        tmp_path,
        """
        import threading, time
        import liquers as lq

        class Opaque:
            pass

        @lq.first_command
        def make():
            return Opaque()

        @lq.first_command(volatile=True)
        async def where(i: int = 0) -> int:
            return threading.get_ident()

        running, overlap, lock = [], [], threading.Lock()

        @lq.first_command(volatile=True)
        def exclusive(i: int = 0) -> int:
            with lock:
                running.append(i)
                overlap.append(len(running))
            time.sleep(0.3)
            with lock:
                running.remove(i)
            return i

        lq.configure(conversion="strict", async_loops=2, job_capacity=1)
        try:
            lq.evaluate("make")
            raise AssertionError("strict conversion did not apply to an earlier command")
        except lq.ConversionError:
            pass
        idents = {lq.evaluate(f"where-{i}") for i in range(3)}
        assert len(idents) == 2, idents
        threads = [threading.Thread(target=lq.evaluate, args=(f"exclusive-{i}",)) for i in (1, 2)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(10)
        assert max(overlap) == 1, "job_capacity=1 did not apply"
        """,
    )


CMR = """commands:
  - name: {name}
    label: {label}
    module: python
    state_argument: null
    arguments: []
"""


def test_CONFIGURE03_registry_file_then_import_metadata(tmp_path):
    # AC-3: the configuration's registry file over the decorator, import_metadata over both.
    (tmp_path / "cmr.yaml").write_text(CMR.format(name="greet", label="From the file") + CMR.format(name="later", label="Later").split("commands:\n", 1)[1])
    run_script(
        tmp_path,
        """
        import liquers as lq

        @lq.first_command(label="From the decorator")
        def greet() -> str:
            return "hi"

        env = lq.default_environment()
        env.import_metadata("commands:\\n  - name: greet\\n    label: From import_metadata\\n    module: python\\n    state_argument: null\\n    arguments: []\\n")
        lq.configure(config={"command_registry": "cmr.yaml"})

        @lq.first_command
        def later() -> str:                     # registered after configure; the file describes it
            return "later"

        assert lq.evaluate("greet") == "hi"
        assert env.command_metadata("greet").label == "From import_metadata"
        assert env.command_metadata("later").label == "Later"
        """,
    )
    run_script(
        tmp_path,
        """
        import liquers as lq

        @lq.first_command(label="From the decorator")
        def greet() -> str:
            return "hi"

        @lq.first_command
        def later() -> str:
            return "later"

        lq.configure(config={"command_registry": "cmr.yaml"})
        assert lq.evaluate("greet") == "hi"
        assert lq.default_environment().command_metadata("greet").label == "From the file"
        """,
        name="file_only",
    )


def test_CONFIGURE04_configure_after_build_is_refused():
    # AC-4: refused once built; the environment keeps working, late registrations included.
    env = lq.Environment(job_capacity=2)
    try:
        @env.first_command
        def one() -> int:
            return 1

        assert env.evaluate("one") == 1
        with pytest.raises(lq.ConfigurationError, match="already built"):
            env.configure(pickle=True)
        assert env.evaluate("one") == 1

        @env.first_command
        def two() -> int:
            return 2

        assert env.evaluate("two") == 2
        with pytest.raises(lq.ConfigurationError, match="already built"):
            env.configure()
    finally:
        env.shutdown()


def test_CONFIGURE05_last_configure_wins(tmp_path):
    # AC-5: each configure replaces every setting.
    env = lq.Environment()
    try:
        @env.first_command
        def make():
            return Opaque(5)

        env.configure(config="python: {conversion: strict}\n")
        env.configure(pickle=True)                       # drops the strict configuration
        assert env.evaluate("make") == Opaque(5)
        env.store_set("x/opaque", Opaque(6))             # pickle on: an opaque value can be stored
    finally:
        env.shutdown()


@pytest.mark.parametrize(
    "keywords, error",
    [
        ({"config": "store: [unclosed\n"}, lq.ParseError),
        ({"config": "sever: {port: 1}\n"}, lq.ConfigurationError),
        ({"async_loops": 0}, lq.ConfigurationError),
        ({"config": {"command_registry": "/nonexistent/cmr.yaml"}}, lq.ConfigurationError),
    ],
    ids=["unparsable", "unknown-section", "zero-loops", "missing-registry"],
)
def test_CONFIGURE06_invalid_configure_changes_nothing(keywords, error):
    # AC-6: a failed configure raises at the call and leaves the earlier settings.
    env = lq.Environment()
    try:
        @env.first_command
        def make():
            return Opaque(1)

        env.configure(conversion="strict")
        with pytest.raises(error):
            env.configure(**keywords)
        with pytest.raises(lq.ConversionError):
            env.evaluate("make")                         # still strict
    finally:
        env.shutdown()


def test_CONFIGURE07_env_configure_is_per_environment():
    # AC-7: configure acts on its own environment; lq.configure has the same keywords.
    first, second = lq.Environment(), lq.Environment()
    try:
        for env in (first, second):
            @env.first_command
            def make():
                return Opaque(2)

        first.configure(conversion="strict")
        with pytest.raises(lq.ConversionError):
            first.evaluate("make")
        assert second.evaluate("make") == Opaque(2)
    finally:
        first.shutdown()
        second.shutdown()
    native = list(inspect.signature(lq.Environment.configure).parameters)
    assert list(inspect.signature(lq.configure).parameters) == [p for p in native if p != "self"]


def test_CONFIGURE08_serve_and_ui_use_configured_settings(tmp_path):
    # AC-8: serve uses the configured server section (keywords still win), ui the configured store.
    data = tmp_path / "data"
    (data / "data").mkdir(parents=True)
    (data / "data" / "note.txt").write_text("from the file store")
    run_script(
        tmp_path,
        f"""
        import urllib.request
        import liquers as lq

        lq.configure(config={{
            "server": {{"port": 0, "prefix": "/x"}},
            "store": {{"stores": [{{"type": "filesystem", "prefix": "data", "config": {{"path": {str(data)!r}}}}}]}},
        }})
        server = lq.serve()
        with urllib.request.urlopen(f"{{server.url}}/x/q/-R/data/note.txt", timeout=10) as response:
            assert response.read() == b"from the file store"
        server.shutdown()
        with lq.serve(prefix="/y") as server:
            with urllib.request.urlopen(f"{{server.url}}/y/q/-R/data/note.txt", timeout=10) as response:
                assert response.status == 200
        ui = lq.ui()
        handle = ui.show("-R/data/note.txt")
        ui.wait_idle(10)
        assert ui.elements()[0].handle == handle
        assert lq.evaluate("-R/data/note.txt") == "from the file store"
        """,
    )


def test_CONFIGURE09_configure_after_failed_build():
    # AC-9: a failed first build leaves configure allowed, and the next build uses its settings.
    from test_STORE_registration import _build_error

    env = lq.Environment()
    try:
        def broken(prefix):
            raise RuntimeError("cannot open")

        env.register_store_factory("broken", broken)
        env.configure(config={"store": {"stores": [{"type": "broken", "prefix": "n"}]}})

        @env.first_command
        def one() -> int:
            return 1

        assert _build_error(env, "one") is not None
        env.configure()                                  # the memory store
        assert env.evaluate("one") == 1
    finally:
        env.shutdown()
