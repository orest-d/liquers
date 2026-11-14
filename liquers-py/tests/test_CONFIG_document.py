"""One configuration document for the Python host (specs/design/configuration-document/, Phase 3).

`lq.Environment(config=...)` reads a file (`pathlib.Path`), YAML text (`str`) or a dict; the host
takes the `server` and `python` sections, loads the `command_registry` file, and refuses a section
or key it does not know. Keywords override the file (Decision 2).
"""

import http.client
import pathlib
import urllib.request

import pytest

import liquers as lq


class Payload:
    """A class with no structural mapping: storing it needs the pickle codec."""

    def __init__(self, n):
        self.n = n


CMR = """commands:
  - name: greet
    label: Greet someone
    doc: Returns a greeting for a name.
    module: python
    state_argument: null
    arguments:
      - {name: name, label: Name, default: !Value world, argument_type: string, gui_info: !TextField 20}
default_namespaces: ['', root]
global_enums: {}
"""


def write(path, text):
    path.write_text(text)
    return path


def register_greet(env):
    @env.first_command
    def greet(name: str = "world") -> str:
        return f"Hello, {name}!"


def get(url):
    with urllib.request.urlopen(url, timeout=10) as response:
        return response.status, response.read()


def test_CONFIG_one_file_configures_the_host(tmp_path):
    site = tmp_path / "site"
    (site / "web").mkdir(parents=True)                      # key web/index.html is <site>/web/index.html
    (site / "web" / "index.html").write_text("<h1>site</h1>")
    write(tmp_path / "commands.yaml", CMR)
    config = write(
        tmp_path / "liquers.yaml",
        f"""store:
  stores:
    - {{type: filesystem, prefix: web, read_only: true, config: {{path: {site}}}}}
    - {{type: memory, prefix: data}}
command_registry: commands.yaml
python:
  pickle: true
server:
  port: 0
""",
    )
    env = lq.Environment(config=config)
    try:
        register_greet(env)
        env.store_set("data/payload", Payload(3))          # pickle on, from the file
        assert env.command_metadata("greet").label == "Greet someone"
        with env.serve() as server:                         # the file's port 0
            assert server.port != 0
            assert get(f"{server.url}/liquer/q/greet-Ann") == (200, b"Hello, Ann!")
            assert get(f"{server.url}/liquer/web/") == (200, b"<h1>site</h1>")
    finally:
        env.shutdown()


def test_CONFIG_unknown_section_is_refused(tmp_path):
    config = write(tmp_path / "liquers.yaml", "server: {port: 8080}\nsever: {port: 9000}\n")
    with pytest.raises(lq.ConfigurationError) as raised:
        lq.Environment(config=config)
    message = str(raised.value)
    assert "unknown section 'sever'" in message
    for known in ("store", "recipes", "assets", "default_namespaces", "command_registry", "server", "python"):
        assert known in message


@pytest.mark.parametrize(
    "document, key",
    [({"server": {"prot": 1}}, "prot"), ({"python": {"pikle": True}}, "pikle")],
)
def test_CONFIG_unknown_key_in_a_section(document, key):
    with pytest.raises(lq.ConfigurationError, match=key):
        lq.Environment(config=document)


def test_CONFIG_command_registry_file(tmp_path):
    write(tmp_path / "commands.yaml", CMR)
    config = write(tmp_path / "analysis.yaml", "command_registry: commands.yaml\n")
    env = lq.Environment(config=config)
    try:
        register_greet(env)
        assert env.evaluate("greet-Ann") == "Hello, Ann!"
        assert env.command_metadata("greet").label == "Greet someone"
    finally:
        env.shutdown()


def test_CONFIG_missing_command_registry_file(tmp_path):
    config = write(tmp_path / "liquers.yaml", "command_registry: missing.yaml\n")
    with pytest.raises(lq.LiquersError, match="missing.yaml"):
        lq.Environment(config=config)
    write(tmp_path / "bad.yaml", "commands: 3\n")
    config = write(tmp_path / "liquers.yaml", "command_registry: bad.yaml\n")
    with pytest.raises(lq.ConfigurationError, match="bad.yaml"):
        lq.Environment(config=config)


def test_CONFIG_keywords_override_the_file(tmp_path):
    config = write(
        tmp_path / "deploy.yaml",
        "assets: {job_capacity: 3}\npython: {pickle: true}\nserver: {port: 8000}\n",
    )
    env = lq.Environment(config=config, pickle=False, job_capacity=2)   # no conflict error
    try:
        with pytest.raises(lq.LiquersError):
            env.store_set("payload", Payload(1))           # the keyword turned pickle off
        with env.serve(port=0) as server:                   # instead of the file's 8000
            assert server.port not in (0, 8000)
    finally:
        env.shutdown()
    # The former conflict case of test_ASYNC_loops.py: the keyword wins.
    lq.Environment(job_capacity=2, config={"assets": {"job_capacity": 3}}).shutdown()


def test_CONFIG_str_file_name_is_explained():
    with pytest.raises(lq.ConfigurationError) as raised:
        lq.Environment(config="liquers.yaml")
    assert "pathlib.Path" in str(raised.value)


EXAMPLE_1 = """
store:
  stores: []
recipes: default
assets:
  job_capacity: 4
  dependency_audit: explicit
  verify_versions: on_read
  external_change: user_input
  recipe_cache_strategy: all
  query_cache_strategy: all
  cut_predecessors: true
python:
  conversion: permissive
  pickle: false
  async_loops: 1
server:
  host: 127.0.0.1
  port: 8080
  prefix: /liquer
  apis: [query, store, assets, recipes]
  read_only: false
  web: true
"""


def observed(env):
    """What a user can see of an environment's configuration, with the server on port 0."""
    register_greet(env)
    env.store_set("web/index.html", "<p>x</p>")
    with env.serve(port=0) as server:
        host, port = server.address, server.port
        connection = http.client.HTTPConnection(host, port, timeout=10)
        connection.request("GET", "/")
        redirect = connection.getresponse()
        redirect.read()
        seen = {
            "redirect": (redirect.status, redirect.getheader("Location")),
            "query": get(f"{server.url}/liquer/q/greet-Ann"),
            "store": get(f"{server.url}/liquer/api/store/data/web/index.html")[0],
            "recipes": get(f"{server.url}/liquer/api/recipes/listdir")[0],
        }
    try:
        env.store_set("data/payload", Payload(1))
        seen["pickle"] = True
    except lq.LiquersError:
        seen["pickle"] = False
    seen["evaluate"] = env.evaluate("greet-Bob")
    return seen


def test_CONFIG_written_defaults_equal_empty():
    written = lq.Environment(config=EXAMPLE_1)
    empty = lq.Environment(config={})
    try:
        assert observed(written) == observed(empty)
    finally:
        written.shutdown()
        empty.shutdown()


def test_CONFIG_toml_needs_the_feature(tmp_path):
    config = write(tmp_path / "liquers.toml", "[assets]\njob_capacity = 2\n")
    with pytest.raises(lq.ConfigurationError, match="toml"):
        lq.Environment(config=config)


def test_CONFIG_unknown_extension(tmp_path):
    config = write(tmp_path / "liquers.conf", "assets: {}\n")
    with pytest.raises(lq.ConfigurationError, match=r"\.yaml"):
        lq.Environment(config=config)


def test_CONFIG_async_loops_zero():
    with pytest.raises(lq.ConfigurationError):
        lq.Environment(config={"python": {"async_loops": 0}})


def test_CONFIG_syntax_error():
    with pytest.raises(lq.ParseError):
        lq.Environment(config="store: [unclosed")


def test_CONFIG_empty_file_is_defaults(tmp_path):
    for config in (write(tmp_path / "empty.yaml", ""), "", {}, None):
        env = lq.Environment(config=config)
        try:
            register_greet(env)
            assert env.evaluate("greet-Ann") == "Hello, Ann!"
        finally:
            env.shutdown()


def test_CONFIG_unreadable_document(tmp_path):
    with pytest.raises(lq.LiquersError, match="absent.yaml"):
        lq.Environment(config=tmp_path / "absent.yaml")


def test_CONFIG_json_file_and_pathlike(tmp_path):
    class Where:
        def __init__(self, path):
            self.path = path

        def __fspath__(self):
            return str(self.path)

    config = write(tmp_path / "liquers.json", '{"python": {"conversion": "strict"}, "server": {"port": 0}}')
    for form in (config, Where(config)):
        env = lq.Environment(config=form)
        try:
            with env.serve() as server:
                assert server.port != 0
        finally:
            env.shutdown()


def test_CONFIG_wrong_config_type():
    with pytest.raises(lq.ConfigurationError, match="pathlib.Path"):
        lq.Environment(config=3)


def test_CONFIG_relative_paths_resolve_against_the_document(tmp_path, monkeypatch):
    folder = tmp_path / "app"
    folder.mkdir()
    write(folder / "commands.yaml", CMR)
    write(folder / "liquers.yaml", "command_registry: commands.yaml\n")
    monkeypatch.chdir(tmp_path)
    env = lq.Environment(config=pathlib.Path("app/liquers.yaml"))
    try:
        register_greet(env)
        assert env.command_metadata("greet").label == "Greet someone"
    finally:
        env.shutdown()
