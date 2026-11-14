"""WEBSERV: serving an environment over the Liquers HTTP API (specs/design/liquers-py-web-server/).

Every test starts its own server on `port=0` and talks to it with the standard library, so the
tests run in parallel without collisions and need no HTTP client package.
"""

import asyncio
import base64
import http.client
import json
import os
import signal
import socket
import struct
import subprocess
import sys
import textwrap
import threading
import time
import urllib.error
import urllib.request

import pytest

import liquers as lq


def get(url, timeout=10):
    with urllib.request.urlopen(url, timeout=timeout) as response:
        return response.status, response.read()


def raw(server, method, path, body=None):
    """One request without following redirects: (status, headers, body)."""
    connection = http.client.HTTPConnection(server.address, server.port, timeout=10)
    try:
        connection.request(method, path, body=body)
        response = connection.getresponse()
        return response.status, dict(response.getheaders()), response.read()
    finally:
        connection.close()


def port_is_free(port):
    """Whether nothing listens on `port`; a connection the server closed may still be in TIME_WAIT."""
    with socket.socket() as probe:
        if sys.platform != "win32":
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            probe.bind(("127.0.0.1", port))
            return True
        except OSError:
            return False


CALLS = {"greet": 0}
"""How often the `env` fixture's `greet` ran (an `lq.Environment` takes no attributes)."""


@pytest.fixture
def env():
    e = lq.Environment()
    CALLS["greet"] = 0

    @e.first_command
    def greet(name: str = "world") -> str:
        CALLS["greet"] += 1
        return f"Hello, {name}!"

    yield e
    e.shutdown()


def test_WEBSERV01_start_on_ephemeral_port_and_reach_readiness(env):
    server = env.serve(port=0)
    try:
        assert server.running
        assert server.port != 0
        assert server.address == "127.0.0.1"
        assert server.url == f"http://127.0.0.1:{server.port}"
        assert get(f"{server.url}/liquer/q/greet-Ann") == (200, b"Hello, Ann!")
    finally:
        server.shutdown()
    assert not server.running


def test_WEBSERV02_standard_route_uses_configured_environment(env):
    env.store_set("data/x.txt", "stored from Python")
    with env.serve(port=0) as server:
        assert get(f"{server.url}/liquer/q/greet-Bob")[1] == b"Hello, Bob!"
        assert get(f"{server.url}/liquer/api/store/data/data/x.txt") == (200, b"stored from Python")
    calls = CALLS["greet"]
    assert env.evaluate("greet-Bob") == "Hello, Bob!"
    assert CALLS["greet"] == calls, "the request's asset is reused by evaluate"


def test_WEBSERV03_port_in_use_raises(env):
    with socket.socket() as taken:
        taken.bind(("127.0.0.1", 0))
        taken.listen()
        port = taken.getsockname()[1]
        with pytest.raises(lq.LiquersError, match=str(port)):
            env.serve(port=port)
    assert env.servers == []


def test_WEBSERV03_bad_host_raises(env):
    with pytest.raises(lq.LiquersError, match="no such host"):
        env.serve(host="no such host.invalid", port=0)


def test_WEBSERV04_graceful_shutdown(env):
    gate = threading.Event()
    started = threading.Event()

    @env.first_command(volatile=True)
    def slow() -> str:
        started.set()
        gate.wait(5)
        return "done"

    # With the gate opened in time, the request in flight completes.
    server = env.serve(port=0)
    result = {}
    request = threading.Thread(target=lambda: result.setdefault("r", get(f"{server.url}/liquer/q/slow")))
    request.start()
    assert started.wait(5)
    stopping = threading.Thread(target=server.shutdown, kwargs={"timeout": 5})
    stopping.start()
    time.sleep(0.2)
    gate.set()
    request.join(10)
    stopping.join(10)
    assert result["r"] == (200, b"done")
    assert not server.running and port_is_free(server.port)

    # Without it, the request is cut off at the deadline.
    gate.clear()
    started.clear()
    server = env.serve(port=0)
    failure = {}

    def doomed():
        try:
            get(f"{server.url}/liquer/q/slow")
        except (urllib.error.URLError, http.client.HTTPException, ConnectionError) as e:
            failure["e"] = e

    request = threading.Thread(target=doomed)
    request.start()
    assert started.wait(5)
    begin = time.monotonic()
    server.shutdown(timeout=0.3)
    assert time.monotonic() - begin < 3
    request.join(10)
    assert "e" in failure
    assert port_is_free(server.port)
    gate.set()


def test_WEBSERV05_background_handle_owns_server_lifetime(env):
    with env.serve(port=0) as server:
        assert server.running
    assert not server.running

    port = env.serve(port=0).port                    # the handle is dropped at once
    assert get(f"http://127.0.0.1:{port}/liquer/q/greet-Ann")[0] == 200
    assert [s.port for s in env.servers] == [port]
    env.shutdown()                                   # stops the servers first
    assert port_is_free(port)
    assert env.servers == []


def test_WEBSERV05_exit_stops_server(tmp_path):
    script = tmp_path / "serve_and_exit.py"
    script.write_text(textwrap.dedent("""
        import threading, time, urllib.request
        import liquers as lq
        env = lq.Environment()

        @env.first_command(volatile=True)
        def slow() -> str:
            time.sleep(0.5)
            return "done"

        server = env.serve(port=0)
        print(server.port, flush=True)
        threading.Thread(
            target=lambda: urllib.request.urlopen(f"{server.url}/liquer/q/slow").read(), daemon=True
        ).start()
        time.sleep(0.1)          # a command in flight when the interpreter exits
    """))
    done = subprocess.run([sys.executable, str(script)], capture_output=True, text=True, timeout=60)
    assert done.returncode == 0, done.stderr
    port = int(done.stdout.split()[0])
    assert port_is_free(port)
    assert "Fatal Python error" not in done.stderr


@pytest.mark.skipif(sys.platform == "win32", reason="SIGINT to a child process is POSIX-only here")
def test_WEBSERV_foreground_ctrl_c(tmp_path):
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    script = tmp_path / "foreground.py"
    script.write_text(textwrap.dedent(f"""
        import liquers as lq
        env = lq.Environment()
        try:
            env.serve(port={port}, background=False)
        except KeyboardInterrupt:
            print("interrupted", flush=True)
            raise
    """))
    child = subprocess.Popen([sys.executable, str(script)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.monotonic() + 30
        while True:
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=1):
                    break
            except OSError:
                assert time.monotonic() < deadline, "the foreground server never listened"
                time.sleep(0.05)
        child.send_signal(signal.SIGINT)
        out, err = child.communicate(timeout=30)
    finally:
        if child.poll() is None:
            child.kill()
    assert "interrupted" in out, err
    assert "KeyboardInterrupt" in err
    assert child.returncode != 0
    assert port_is_free(port)


def test_WEBSERV_late_registration_is_served(env):
    with env.serve(port=0) as server:
        assert get(f"{server.url}/liquer/q/greet-Ann")[0] == 200

        @env.first_command
        def report() -> str:
            return "rows: 3"

        assert get(f"{server.url}/liquer/q/report") == (200, b"rows: 3")


class TinyWebSocket:
    """A minimal RFC 6455 client: the handshake, masked text frames out, frames in."""

    def __init__(self, host, port, path):
        self.sock = socket.create_connection((host, port), timeout=10)
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.sendall(
            (
                f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n"
                f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
            ).encode()
        )
        response = b""
        while b"\r\n\r\n" not in response:
            chunk = self.sock.recv(4096)
            assert chunk, "the server closed the handshake"
            response += chunk
        head, self.buffer = response.split(b"\r\n\r\n", 1)
        assert head.startswith(b"HTTP/1.1 101"), head

    def _read(self, n):
        while len(self.buffer) < n:
            chunk = self.sock.recv(4096)
            assert chunk, "the connection closed"
            self.buffer += chunk
        data, self.buffer = self.buffer[:n], self.buffer[n:]
        return data

    def send_text(self, text):
        payload = text.encode()
        mask = os.urandom(4)
        header = bytes([0x81])
        if len(payload) < 126:
            header += bytes([0x80 | len(payload)])
        else:
            header += bytes([0x80 | 126]) + struct.pack("!H", len(payload))
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        self.sock.sendall(header + mask + masked)

    def receive(self):
        """(opcode, payload) of the next frame."""
        first, second = self._read(2)
        length = second & 0x7F
        if length == 126:
            (length,) = struct.unpack("!H", self._read(2))
        elif length == 127:
            (length,) = struct.unpack("!Q", self._read(8))
        return first & 0x0F, self._read(length)

    def close(self):
        self.sock.close()


def test_WEBSERV_open_websocket_keeps_its_build(env):
    env.store_set("data/a.txt", "a")
    env.store_set("data/b.txt", "b")
    with env.serve(port=0) as server:
        socket_ = TinyWebSocket(server.address, server.port, "/liquer/api/assets/ws/key")
        try:
            socket_.send_text(json.dumps({"action": "subscribe", "key": "data/a.txt"}))
            opcode, payload = socket_.receive()
            assert opcode == 1 and isinstance(json.loads(payload), dict)

            @env.first_command                    # a rebuild: new requests get the new build
            def late() -> str:
                return "late"

            assert get(f"{server.url}/liquer/q/late")[1] == b"late"
            # The open socket still answers, from the build it upgraded with.
            socket_.send_text(json.dumps({"action": "subscribe", "key": "data/b.txt"}))
            opcode, payload = socket_.receive()
            assert opcode == 1 and isinstance(json.loads(payload), dict)
        finally:
            socket_.close()


def test_WEBSERV_websocket_gets_close_frame_on_shutdown(env):
    server = env.serve(port=0)
    socket_ = TinyWebSocket(server.address, server.port, "/liquer/api/assets/ws/q")
    try:
        stopping = threading.Thread(target=server.shutdown)
        stopping.start()
        opcode, payload = socket_.receive()
        assert opcode == 8
        assert struct.unpack("!H", payload[:2])[0] == 1001
        stopping.join(10)
    finally:
        socket_.close()


def test_WEBSERV_settings_from_the_configuration(tmp_path):
    site = tmp_path / "site"
    (site / "web").mkdir(parents=True)                      # the mount keeps its prefix in the path
    (site / "web" / "index.html").write_text("<h1>reports</h1>")
    config = {
        "store": {"stores": [{"type": "filesystem", "prefix": "web", "read_only": True, "config": {"path": str(site)}}]},
        "server": {"port": 0, "prefix": "/api", "read_only": True},
    }
    configured = lq.Environment(config=config)
    try:
        with configured.serve(prefix="/v1") as server:      # the file's port and read_only, prefix /v1
            assert raw(server, "GET", "/v1/web/")[0] == 200
            assert raw(server, "GET", "/v1/api/store/data/web/index.html")[0] == 404
            assert raw(server, "GET", "/api/web/")[0] == 404
    finally:
        configured.shutdown()

    plain = lq.Environment()
    try:
        with plain.serve(port=0) as server:                  # writes enabled by default
            status, _, _ = raw(server, "PUT", "/liquer/api/store/data/data/x.txt", body=b"written")
            assert status == 200
            assert get(f"{server.url}/liquer/api/store/data/data/x.txt")[1] == b"written"
        plain.store_set("site/index.html", "<p>site</p>")
        with plain.serve(port=0, web="site") as server:
            assert get(f"{server.url}/liquer/web/")[1] == b"<p>site</p>"
    finally:
        plain.shutdown()


def test_WEBSERV_async_forms(env):
    async def main():
        task = asyncio.create_task(env.serve_async(port=0))
        for _ in range(200):
            if env.servers:
                break
            await asyncio.sleep(0.02)
        (server,) = env.servers
        status, body = await asyncio.to_thread(get, f"{server.url}/liquer/q/greet-Ann")
        assert (status, body) == (200, b"Hello, Ann!")
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        for _ in range(200):
            if not server.running:
                break
            await asyncio.sleep(0.02)
        assert not server.running

        background = env.serve(port=0)                    # from async code: returns when bound
        assert background.running
        await background.shutdown_async()
        assert not background.running

    asyncio.run(main())


def test_WEBSERV_web_folder_from_a_mount(tmp_path):
    site = tmp_path / "site"
    (site / "web" / "docs").mkdir(parents=True)             # key web/app.js is <site>/web/app.js
    (site / "web" / "index.html").write_text("<h1>app</h1>")
    (site / "web" / "app.js").write_text("console.log(1)")
    (site / "web" / "docs" / "index.html").write_text("<h1>docs</h1>")
    env = lq.Environment(config={"store": {"stores": [
        {"type": "filesystem", "prefix": "web", "read_only": True, "config": {"path": str(site)}},
        {"type": "memory", "prefix": ""},
    ]}})
    try:
        with env.serve(port=0) as server:
            status, headers, body = raw(server, "GET", "/liquer/web/app.js")
            assert (status, body) == (200, b"console.log(1)")
            assert headers["content-type"] == "text/javascript"
            assert raw(server, "GET", "/liquer/web/docs/")[2] == b"<h1>docs</h1>"
            assert raw(server, "GET", "/liquer/web/docs")[0] == 308
            assert raw(server, "GET", "/liquer/web/missing.js")[0] == 404
    finally:
        env.shutdown()


def test_WEBSERV_index_redirects_to_web(env):
    env.store_set("web/index.html", "<h1>Reports</h1>")
    with env.serve(port=0) as server:
        for path in ("/", "/index.html"):
            status, headers, _ = raw(server, "GET", path)
            assert (status, headers["location"]) == (307, "/liquer/web/"), path
        status, headers, _ = raw(server, "GET", "/liquer/web")
        assert (status, headers["location"]) == (308, "/liquer/web/")
        status, headers, body = raw(server, "GET", "/liquer/web/")
        assert (status, body) == (200, b"<h1>Reports</h1>")
        assert headers["content-type"] == "text/html"   # by the extension: the str is stored as txt


def test_WEBSERV_bad_settings_raise(env):
    with pytest.raises(lq.ConfigurationError, match="nope"):
        env.serve(port=0, apis=["query", "nope"])
    with pytest.raises(lq.ConfigurationError, match="prefix"):
        env.serve(port=0, prefix="api")
    with pytest.raises(lq.ConfigurationError):
        env.serve(port=0, web=3)
    assert env.servers == []


def test_WEBSERV_serve_after_shutdown():
    e = lq.Environment()
    e.shutdown()
    with pytest.raises(lq.LiquersError, match="shut down"):
        e.serve(port=0)


def test_WEBSERV_failed_rebuild_is_503(env):
    with env.serve(port=0) as server:
        env.import_metadata("commands:\n  - name: not_yet\n    label: Not yet\n")
        status, _, body = raw(server, "GET", "/liquer/q/greet-Ann")
        assert status == 503
        assert b"not registered" in body

        @env.first_command
        def not_yet() -> str:
            return "now"

        assert get(f"{server.url}/liquer/q/not_yet") == (200, b"now")


def test_WEBSERV_shutdown_on_loop_thread_raises(env):
    server = env.serve(port=0)

    @env.first_command(volatile=True)
    async def stop_it() -> str:
        try:
            server.shutdown()
        except RuntimeError as e:
            return f"raised: {e}"
        return "did not raise"

    try:
        assert env.evaluate("stop_it").startswith("raised: ")
        assert server.running
    finally:
        server.shutdown()


def test_WEBSERV_repr_and_module_functions(env):
    with env.serve(port=0) as server:
        assert repr(server) == f"<liquers.Server {server.url} running>"
    assert repr(server) == f"<liquers.Server {server.url} stopped>"
    assert callable(lq.serve) and callable(lq.serve_async)
    assert isinstance(server, lq.Server)
