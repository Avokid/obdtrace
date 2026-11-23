"""Package a native build and verify it after extraction."""

import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import queue
import re
import socket
import ssl
import subprocess
import tarfile
import tempfile
import threading
import time
import tomllib
import urllib.request
import urllib.error
import zipfile


ROOT = Path(__file__).resolve().parents[1]
PACKAGE = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]
LICENSES = {
    "LICENSE": ROOT / "LICENSE",
    "LICENSE-i18next": ROOT / "assets/vendor/LICENSE",
}


def smoke_test(exe, scratch):
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    env = dict(os.environ, OBDTRACE_DATA_DIR=str(scratch / "data"), OBDTRACE_PORT=str(port))
    if os.name == "nt":
        env["LOCALAPPDATA"] = str(scratch / "profile")
    env.pop("OBDTRACE_CONNECTION", None)
    env.pop("OBDTRACE_ENABLE_CLEAR", None)
    version = subprocess.check_output([exe, "--version"], env=env, text=True, timeout=15)
    assert version.strip() == f"obdtrace {PACKAGE['version']}", version
    subprocess.run([exe, "doctor"], env=env, check=True, timeout=30)

    with subprocess.Popen([exe, "demo", "--no-browser"], env=env) as app:
        try:
            url = f"http://127.0.0.1:{port}"
            for attempt in range(60):
                if app.poll() is not None:
                    raise RuntimeError(f"Demo exited with {app.returncode}")
                try:
                    with urllib.request.urlopen(f"{url}/api/ping", timeout=1) as response:
                        assert json.load(response)["app"] == "obdtrace"
                    break
                except OSError:
                    if attempt == 59:
                        raise
                    time.sleep(0.25)
            with urllib.request.urlopen(url, timeout=5) as response:
                assert b"<title>OBDTrace</title>" in response.read()
            runtime = (Path(env["LOCALAPPDATA"]) / "OBDTrace" if os.name == "nt"
                       else Path(env["OBDTRACE_DATA_DIR"])) / ".obdtrace-runtime"
            launch = (runtime / f"open-{port}.html").read_text()
            token = re.search(r"/#([0-9a-f]{32})", launch).group(1)
            try:
                urllib.request.urlopen(f"{url}/api/status", timeout=5)
            except urllib.error.HTTPError as error:
                assert error.code == 401
            else:
                raise AssertionError("Private status available without browser authentication")
            req = urllib.request.Request(f"{url}/api/status", headers={"Authorization": f"Bearer {token}"})
            with urllib.request.urlopen(req, timeout=5) as response:
                assert json.load(response)["adapter"]["connected"]
            for credential, expected in [("invalid", 401), (token, 101)]:
                ws = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
                try:
                    ws.request("GET", "/ws", headers={
                        "Origin": url, "Upgrade": "websocket", "Connection": "Upgrade",
                        "Sec-WebSocket-Version": "13", "Sec-WebSocket-Key": "c21va2UtdGVzdC1ub25jZQ==",
                        "Sec-WebSocket-Protocol": f"obdtrace, auth.{credential}",
                    })
                    assert ws.getresponse().status == expected
                finally:
                    ws.close()
            check_mcp(exe, dict(env, OBDTRACE_READ_ONLY="1"))

            path = runtime / f"peer-{port}.json"
            peer = json.loads(path.read_text())
            unrelated_ca = next(iter(ssl.create_default_context().get_ca_certs(binary_form=True)), b"")
            for field, invalid in [("token", "invalid"), ("cert", list(unrelated_ca))]:
                path.write_text(json.dumps({**peer, field: invalid}))
                denied = subprocess.run([exe, "mcp"], env=env, input=b"", capture_output=True, timeout=30)
                assert denied.returncode != 0, f"Accepted invalid bridge {field}"
        finally:
            app.terminate()
            app.wait(timeout=10)


def check_mcp(exe, env):
    with subprocess.Popen(
        [exe, "mcp"], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        text=True, encoding="utf-8",
    ) as proc:
        replies = queue.Queue()

        def read_replies():
            for line in proc.stdout:
                replies.put(line)
            replies.put(None)

        threading.Thread(target=read_replies, daemon=True).start()
        request_id = 0

        def rpc(method, params=None, notify=False):
            nonlocal request_id
            request_id += 1
            message = {"jsonrpc": "2.0", "method": method, "params": params or {}}
            if not notify:
                message["id"] = request_id
            proc.stdin.write(json.dumps(message) + "\n")
            proc.stdin.flush()
            if not notify:
                line = replies.get(timeout=20)
                assert line is not None, f"MCP exited during {method}"
                response = json.loads(line)
                assert response["id"] == request_id and "result" in response, response
                return response["result"]

        def call(name, **args):
            result = rpc("tools/call", {"name": name, "arguments": args})
            assert not result["isError"], result
            return json.loads(result["content"][0]["text"])

        try:
            init = rpc("initialize", {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "release-smoke-test", "version": "1"},
            })
            assert init["serverInfo"]["name"] == "obdtrace", init
            rpc("notifications/initialized", notify=True)
            tools = {t["name"] for t in rpc("tools/list")["tools"]}
            assert "query_session" in tools and "connect" not in tools
            denied = rpc("tools/call", {"name": "set_ring_buffer", "arguments": {"seconds": 300}})
            assert denied["isError"] and "disabled" in denied["content"][0]["text"]
            status = call("connection_status")
            assert status["adapter"]["connected"] and status["target"] == "mock:audi_demo", status
            events = call("get_session_events", session_id=status["session_id"])
            seed = next(e["data"] for e in events["events"] if e["kind"] == "demo_seed")
            history = call("query_session", session_id=status["session_id"], pids=["rpm"],
                           from_ms=seed["from_ms"], to_ms=seed["to_ms"], limit=1)
            series = history["series"]
            assert history["rows"] == 1 and len(series) == 1 and series[0]["pid"] == "rpm", history
            assert series[0]["source"] == "demo_seed" and len(series[0]["points"]) == 1, history
            proc.stdin.close()
            assert proc.wait(timeout=15) == 0
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait(timeout=10)


def build_archive(exe, target, out):
    version = PACKAGE["version"]
    stem = f"obdtrace-v{version}-{target}"
    files = {
        exe.name: exe,
        **LICENSES,
        "README.md": ROOT / "README.md",
        "SECURITY.md": ROOT / "SECURITY.md",
        "AGENTS.md": ROOT / "AGENTS.md",
        **{path.relative_to(ROOT).as_posix(): path for path in (ROOT / "docs").rglob("*.md")},
    }
    if target == "x86_64-unknown-linux-gnu":
        from linux_dependencies import collect
        files.update(collect(out))
    if target.endswith("-windows-msvc"):
        with zipfile.ZipFile(out / f"{stem}.zip", "w", zipfile.ZIP_DEFLATED) as archive:
            for name, path in files.items():
                archive.write(path, name)
        return out / f"{stem}.zip"
    with tarfile.open(out / f"{stem}.tar.gz", "w:gz") as archive:
        for name, path in files.items():
            archive.add(path, name, recursive=False)
    return out / f"{stem}.tar.gz"


def build_mcpb(exe, target, out):
    platform = {
        "aarch64-apple-darwin": "darwin",
        "x86_64-pc-windows-msvc": "win32",
        "x86_64-unknown-linux-gnu": "linux",
    }[target]
    version = PACKAGE["version"]
    repo = PACKAGE["repository"]
    description = "Local OBD-II diagnostics, session history and MCP for ELM327 adapters."
    entry = f"server/{exe.name}"
    manifest = {
        "manifest_version": "0.3", "name": "obdtrace", "display_name": "OBDTrace",
        "version": version, "description": description, "author": {"name": "Avokid"},
        "license": "MIT", "repository": {"type": "git", "url": repo},
        "compatibility": {"platforms": [platform]},
        "server": {"type": "binary", "entry_point": entry,
                   "mcp_config": {"command": "${__dirname}/" + entry, "args": ["mcp"]}},
        "tools_generated": True, "prompts_generated": True,
    }
    out.mkdir(parents=True, exist_ok=True)
    bundle = out / f"obdtrace-v{version}-{target}.mcpb"
    with zipfile.ZipFile(bundle, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest, indent=2) + "\n")
        for name, path in {entry: exe, **LICENSES}.items():
            archive.write(path, name)
    with bundle.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    metadata = {
        "$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
        "name": "io.github.avokid/obdtrace", "title": "OBDTrace",
        "description": description, "version": version,
        "repository": {"url": repo, "source": "github"},
        "packages": [{
            "registryType": "mcpb", "identifier": f"{repo}/releases/download/v{version}/{bundle.name}",
            "fileSha256": digest, "transport": {"type": "stdio"},
        }],
    }
    (out / "server.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return bundle


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target")
    parser.add_argument("executable", type=Path)
    parser.add_argument("--mcpb", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.mcpb:
        bundle = build_mcpb(args.executable, args.target, args.output / "mcpb")
        print(f"Prepared {bundle}; install in a compatible desktop client before publishing.")
        return
    archive_path = build_archive(args.executable, args.target, args.output)
    with tempfile.TemporaryDirectory(prefix="obdtrace-release-") as directory:
        scratch = Path(directory)
        if archive_path.suffix == ".zip":
            with zipfile.ZipFile(archive_path) as archive:
                archive.extractall(scratch)
        else:
            with tarfile.open(archive_path) as archive:
                archive.extractall(scratch, filter="data")
        smoke_test(scratch / args.executable.name, scratch)
    print(f"Verified {archive_path}")


if __name__ == "__main__":
    main()
