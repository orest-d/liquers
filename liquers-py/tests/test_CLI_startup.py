"""Starting an application from a configuration file with command-line overrides
(``py-command-line-config``).

Most tests configure their own ``lq.Environment`` through ``env=``. The ones that need the default
environment, or a real ``sys.argv``, run in a fresh interpreter (``run_script``), since the
default can be configured only once per process.
"""

import argparse
import pathlib
import re
import urllib.error
import urllib.request

import pytest

import liquers as lq
from test_CONFIGURE_environment import run_script

GUIDE = pathlib.Path(__file__).resolve().parents[2] / "docs" / "guides" / "python"

CMR = """commands:
  - name: greet
    label: {label}
    module: python
    state_argument: null
    arguments: []
"""


def greet_environment():
    env = lq.Environment()

    @env.first_command
    def greet() -> str:
        return "hi"

    return env


def put(server, path, body=b"written"):
    request = urllib.request.Request(f"{server.url}{path}", data=body, method="PUT")
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code


def test_CLI01_add_arguments_adds_a_group():
    # AC-1: one "Liquers" group; the program's options are untouched; clashes are reported.
    parser = argparse.ArgumentParser(prog="app")
    parser.add_argument("--title", default="My app")
    group = lq.add_arguments(parser)
    assert group.title == "Liquers"
    text = parser.format_help()
    for option in ("--config", "--command-registry", "--default-namespace", "--host", "--port",
                   "--prefix", "--server-read-only", "--job-capacity", "--async-loops",
                   "--conversion", "--pickle"):
        assert option in text, option
    args = parser.parse_args([])
    assert args.title == "My app"
    assert all(getattr(args, name) is None for name in vars(args) if name.startswith("liquers_")
               and name != "liquers_default_config")

    clashing = argparse.ArgumentParser()
    clashing.add_argument("--port")
    with pytest.raises(argparse.ArgumentError):
        lq.add_arguments(clashing)

    only = argparse.ArgumentParser()
    lq.add_arguments(only, options=["config"])
    assert "--config" in only.format_help() and "--port" not in only.format_help()
    with pytest.raises(ValueError, match="unknown Liquers option"):
        lq.add_arguments(argparse.ArgumentParser(), options=["colour"])


def test_CLI02_configure_from_args_overrides_the_file(tmp_path, monkeypatch):
    # AC-2: given options win; options not given keep the file's values, booleans included.
    monkeypatch.chdir(tmp_path)
    (tmp_path / "app.yaml").write_text("server: {port: 8123, prefix: /app, read_only: false}\n")
    (tmp_path / "cmr.yaml").write_text(CMR.format(label="From the command line"))

    parser = argparse.ArgumentParser()
    lq.add_arguments(parser)
    args = parser.parse_args(["--config", "app.yaml", "--port", "0",
                              "--command-registry", "cmr.yaml", "--server-read-only"])
    env = greet_environment()
    try:
        lq.configure_from_args(args, env=env)
        with env.serve() as server:
            assert server.port != 8123
            assert put(server, "/app/api/store/data/data/x.txt") != 200
        assert env.command_metadata("greet").label == "From the command line"
    finally:
        env.shutdown()

    args = parser.parse_args(["--config", "app.yaml"])
    env = greet_environment()
    try:
        lq.configure_from_args(args, env=env)
        with env.serve(port=0) as server:               # the keyword: the file's port may be taken
            assert put(server, "/app/api/store/data/data/x.txt") == 200
        assert env.evaluate("-R/data/x.txt") == "written"
    finally:
        env.shutdown()


@pytest.mark.parametrize(
    "case", ["default-file", "no-file", "default-disabled", "missing-config", "port-without-file"]
)
def test_CLI03_default_file_and_missing_file(tmp_path, monkeypatch, case):
    # AC-3: liquers.yaml in the working directory is the default file; a named file must exist.
    monkeypatch.chdir(tmp_path)
    if case in ("default-file", "default-disabled"):
        (tmp_path / "liquers.yaml").write_text("python: {conversion: strict}\n")
    parser = argparse.ArgumentParser()
    lq.add_arguments(parser, default_config=None if case == "default-disabled" else "liquers.yaml")
    argv = {"missing-config": ["--config", "absent.yaml"], "port-without-file": ["--port", "0"]}
    args = parser.parse_args(argv.get(case, []))

    env = lq.Environment()
    try:
        @env.first_command
        def make():
            return object()

        if case == "missing-config":
            with pytest.raises(lq.ConfigurationError, match="absent.yaml"):
                lq.configure_from_args(args, env=env)
            return
        lq.configure_from_args(args, env=env)
        if case == "default-file":
            with pytest.raises(lq.ConversionError):
                env.evaluate("make")                    # strict, from liquers.yaml
        else:
            env.evaluate("make")                        # the defaults: permissive
        if case == "port-without-file":
            with env.serve() as server:
                assert server.port != 8080
    finally:
        env.shutdown()


def test_CLI04_paths_keep_their_meaning(tmp_path, monkeypatch):
    # AC-4: the file's relative command_registry joins the file's folder; a relative
    # --command-registry joins the working directory.
    config_dir = tmp_path / "conf"
    config_dir.mkdir()
    (config_dir / "cmr.yaml").write_text(CMR.format(label="Beside the file"))
    (config_dir / "app.yaml").write_text("command_registry: cmr.yaml\n")
    work = tmp_path / "work"
    work.mkdir()
    (work / "cmr.yaml").write_text(CMR.format(label="In the working directory"))
    monkeypatch.chdir(work)

    parser = argparse.ArgumentParser()
    lq.add_arguments(parser)
    for argv, label in (
        (["--config", str(config_dir / "app.yaml")], "Beside the file"),
        (["--config", str(config_dir / "app.yaml"), "--command-registry", "cmr.yaml"],
         "In the working directory"),
    ):
        env = greet_environment()
        try:
            lq.configure_from_args(parser.parse_args(argv), env=env)
            assert env.evaluate("greet") == "hi"
            assert env.command_metadata("greet").label == label
        finally:
            env.shutdown()


def test_CLI05_configure_from_command_line(tmp_path):
    # AC-5: one call parses, configures the default environment and returns the namespace;
    # argv=None reads sys.argv[1:].
    (tmp_path / "liquers.yaml").write_text("server: {port: 8123, prefix: /cli}\n")
    run_script(
        tmp_path,
        """
        import argparse, sys
        import liquers as lq

        @lq.first_command
        def hello() -> str:
            return "hello"

        sys.argv = ["app", "--title", "Shown", "--port", "0"]
        parser = argparse.ArgumentParser()
        parser.add_argument("--title")
        args = lq.configure_from_command_line(parser)
        assert args.title == "Shown" and args.liquers_port == 0
        import urllib.request
        with lq.serve() as server:                      # liquers.yaml's prefix, the port from argv
            assert server.port != 8123
            with urllib.request.urlopen(f"{server.url}/cli/q/hello", timeout=10) as response:
                assert response.read() == b"hello"
        """,
    )
    run_script(
        tmp_path,
        """
        import liquers as lq

        args = lq.configure_from_command_line(argv=["--conversion", "strict"])
        assert args.liquers_conversion == "strict"

        @lq.first_command
        def make():
            return object()

        try:
            lq.evaluate("make")
            raise AssertionError("--conversion strict did not apply")
        except lq.ConversionError:
            pass
        """,
        name="no_parser",
    )


def blocks(chapter):
    text = chapter.read_text()
    return re.findall(r"```python\n(.*?)```", text, flags=re.S)


@pytest.mark.parametrize("name", ["11-ui.md", "12-serving.md"])
def test_CLI06_guide_examples_load_yaml(name):
    # AC-6: every complete server or UI example of chapters 11 and 12 loads a YAML file first.
    matches = [GUIDE / name]
    assert matches[0].is_file(), f"no chapter {name} in {GUIDE}"
    # The complete examples, as test_GUIDE_examples.py runs them; a fragment continues one.
    starting = [block for block in blocks(matches[0])
                if "import liquers" in block and re.search(r"\b(lq|env)\.(serve|ui)\(", block)]
    assert starting, f"{matches[0].name} has no server or UI example"
    for block in starting:
        assert re.search(r"configure(_from_command_line|_from_args)?\(|Environment\(config=pathlib", block), (
            f"{matches[0].name}: an example starts a server or UI without loading a configuration:\n"
            + block
        )
