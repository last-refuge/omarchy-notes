#!/usr/bin/env python3
"""Opt-in native launch forwarding using a disposable library and owned windows.
Every synthetic input/focus is checked against PID, app ID, address and workspace.
No desktop configuration, clipboard, external viewers or screenshots are used.
"""
import concurrent.futures
import json
import os
from pathlib import Path
import re
import socket
import sqlite3
import struct
import subprocess
import sys
import tempfile
import time
import uuid
from rust_native_test import InstanceSocket

REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / "target/release/omarchy-notes-preview"
APP_ID = "org.omarchy.Notes.Preview"


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def wait(predicate, label, timeout=12):
    from gi.repository import GLib
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        context = GLib.MainContext.default()
        while context.pending():
            context.iteration(False)
        value = predicate()
        if value:
            return value
        time.sleep(.04)
    raise AssertionError("Timed out: " + label)


def compositor(kind):
    return json.loads(subprocess.check_output(["hyprctl", kind, "-j"], timeout=3))


def run():
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, Gio, GLib
    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    bus.call_sync("org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", "IsEnabled", GLib.Variant("b", True))), None, 0, 3000, None)
    registry = subprocess.Popen(["/usr/lib/at-spi2-registryd"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    process = None
    endpoint = None
    try:
        with tempfile.TemporaryDirectory(prefix="notes-launch-native-") as temporary:
            root = Path(temporary) / "library"
            os.environ["XDG_CONFIG_HOME"] = str(Path(temporary) / "config")
            log = REPO / "target/launch-native-app.log"
            env = dict(os.environ, WAYLAND_DEBUG="client")
            env.pop("XDG_ACTIVATION_TOKEN", None)
            with log.open("w") as output:
                process = subprocess.Popen([str(BINARY), "--data-dir", str(root), "--reference", "--new"], env=env, stdout=output, stderr=output)

            def windows():
                assert process.poll() is None, log.read_text()[-4000:]
                return [w for w in compositor("clients") if w.get("pid") == process.pid and w.get("class") == APP_ID]

            def window(quick):
                return next((w for w in windows() if w["title"].startswith("Quick Note") == quick), None)

            def guard(address):
                active = compositor("activewindow")
                assert active.get("pid") == process.pid and active.get("class") == APP_ID and active.get("address") == address, "Refusing input: owned window lost focus"
                assert active["workspace"]["id"] == compositor("activeworkspace")["id"], "Refusing input: workspace changed"

            def focus(address):
                owned = next((w for w in windows() if w["address"] == address), None)
                assert owned and owned["workspace"]["id"] == compositor("activeworkspace")["id"], "Only owned windows on the active workspace may be focused"
                if compositor("activewindow").get("address") != address:
                    subprocess.run(["hyprctl", "dispatch", 'hl.dsp.focus({ window = "address:' + address + '" })'], capture_output=True, check=True, timeout=3)
                wait(lambda: compositor("activewindow").get("address") == address, "owned focus")
                guard(address)

            def key(address, key):
                assert key in ("s", "q")
                guard(address)
                subprocess.run(["wtype", "-M", "ctrl", "-k", key, "-m", "ctrl"], check=True, timeout=3)
                if key != "q":
                    guard(address)

            def descendants(node):
                if node is None:
                    return
                node.clear_cache()
                yield node
                for i in range(node.get_child_count()):
                    yield from descendants(node.get_child_at_index(i))

            def editor():
                desktop = Atspi.get_desktop(0)
                app = next((a for i in range(desktop.get_child_count()) if (a := desktop.get_child_at_index(i)).get_process_id() == process.pid), None)
                if app is None:
                    return None
                # Scope away from stale closed-window cache nodes, as in the
                # Quick Note oracle. The installed cache ABI emits warnings.
                active = compositor("activewindow")
                assert active.get("pid") == process.pid and active.get("class") == APP_ID
                app.clear_cache()
                children = [app.get_child_at_index(i) for i in range(app.get_child_count())]
                for frame in reversed(children):
                    frame.clear_cache()
                    if frame.get_name() == active["title"]:
                        return next((n for n in descendants(frame) if n.get_name() == "Note content" and n.get_state_set().contains(Atspi.StateType.FOCUSED)), None)
                return None

            def text_value():
                node = editor()
                return Atspi.Text.get_text(node.get_text_iface(), 0, -1) if node else None

            def type_text(address, text, expected):
                wait(lambda: text_value() == expected, "expected rendered editor")
                for char in text:
                    provider = wait(editor, "focused editor").get_text_iface()
                    before = Atspi.Text.get_text(provider, 0, -1)
                    caret = Atspi.Text.get_caret_offset(provider)
                    guard(address)
                    subprocess.run(["wtype", "--", char], check=True, timeout=3)
                    guard(address)
                    wait(lambda: text_value() == before[:caret] + char + before[caret:], "input acknowledgement")

            def stored():
                if not (root / "library.sqlite3").exists():
                    return []
                with sqlite3.connect(root / "library.sqlite3") as db:
                    return db.execute("SELECT id,plain_text FROM notes WHERE deleted_at IS NULL ORDER BY id").fetchall()

            main = wait(lambda: window(False), "initial main window")["address"]
            wait(lambda: len(stored()) == 1, "initial --new commit")
            original = stored()[0][0]
            endpoint = InstanceSocket(root, process)
            client_env = dict(os.environ)
            for name in ("WAYLAND_DISPLAY", "DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "AT_SPI_BUS_ADDRESS", "XDG_ACTIVATION_TOKEN", "WAYLAND_DEBUG"):
                client_env.pop(name, None)

            def cli(*args, ok=True, library=root, token=None):
                env = dict(client_env)
                if token is not None:
                    env["XDG_ACTIVATION_TOKEN"] = token
                result = subprocess.run([str(BINARY), "--data-dir", str(library), *args], env=env, capture_output=True, text=True, timeout=15)
                assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
                return result

            def request(intent, request_id=None, token=None, lost=False):
                message = {"version": 1, "request_id": request_id or str(uuid.uuid4()), "command": {"action": "launch", "intent": intent, "token": token}}
                data = json.dumps(message).encode()
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                    client.settimeout(12)
                    client.connect(str(endpoint.path))
                    client.sendall(struct.pack("!I", len(data)) + data)
                    if lost:
                        return None
                    def exact(count):
                        result = b""
                        while len(result) < count:
                            chunk = client.recv(count - len(result))
                            assert chunk, "Owner closed before reply"
                            result += chunk
                        return result
                    reply = json.loads(exact(struct.unpack("!I", exact(4))[0]))
                    assert reply["request_id"] == message["request_id"]
                    return reply["outcome"]

            focus(main)
            type_text(main, "Keep this draft", "")
            assert original in cli().stdout
            cli("--note", str(uuid.uuid4()), ok=False)
            focus(main)
            type_text(main, " safe", "Keep this draft")
            key(main, "s")
            wait(lambda: (original, "Keep this draft safe") in stored(), "default/missing target preserve draft and caret")
            alias = Path(temporary) / "alias"
            alias.symlink_to(root, target_is_directory=True)
            created = re.search(r"Opened note ([a-f0-9-]{36})", cli("--new", library=alias).stdout).group(1)
            assert len(stored()) == 2 and created != original
            assert len(windows()) == 1
            focus(main)
            type_text(main, "Another real note", "")
            key(main, "s")
            wait(lambda: (created, "Another real note") in stored(), "forwarded new editor")
            cli("--note", original)
            focus(main)
            type_text(main, " again", "Keep this draft safe")
            key(main, "s")
            wait(lambda: (original, "Keep this draft safe again") in stored(), "specific note and retained caret")
            for args in [("--new", "--quick-note"), ("--note", "invalid"), ("--new", "--capture", "invalid"), ("--new", "--simulate-save-failure")]:
                cli(*args, ok=False)
            cli("--note", str(uuid.uuid4()), library=Path(temporary) / "missing", ok=False)
            assert not (Path(temporary) / "missing").exists()
            assert len(stored()) == 2
            # Concurrent launches produce one Quick window and one draft ID.
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                replies = list(pool.map(lambda _: cli("--quick-note").stdout, range(2)))
            assert replies[0] == replies[1]
            q = wait(lambda: window(True), "forwarded Quick")["address"]
            assert len(windows()) == 2 and len(stored()) == 2
            quick_id = re.search(r"Opened note ([a-f0-9-]{36})", replies[0]).group(1)
            focus(q)
            type_text(q, "Quick from a second invocation", "")
            key(q, "s")
            wait(lambda: (quick_id, "Quick from a second invocation") in stored(), "forwarded Quick save")
            # Test-only invalid tokens prove target submission, NOT focus grants.
            for kind, label in [("main", "main"), ("quick", "quick")]:
                result = request({"kind": kind}, token="notes-native-test-" + label)
                assert result["state"] == "opened", result
                assert result["window"]["activation"] == "token_requested", result
                guard(q)  # Invalid authorization must not force main into focus.
            cli("--note", original, token="notes-native-test-cli")
            guard(q)
            trace = log.read_text()
            targets = []
            for label in ("main", "quick", "cli"):
                matches = re.findall(r'activate\("notes-native-test-' + label + r'", (wl_surface[@#][0-9]+)\)', trace)
                assert len(matches) == 1, "Expected exactly one token submission to " + label
                targets.extend(matches)
            assert targets[0] != targets[1] and targets[0] == targets[2], "Tokens must target the intended owned surface"
            # The caller disappears after sending New. Reusing the same protocol
            # ID reaches the owner-local replay guard and must not create twice.
            lost = str(uuid.uuid4())
            request({"kind": "new"}, request_id=lost, lost=True)
            result = request({"kind": "new"}, request_id=lost)
            assert result["state"] == "opened" and len(stored()) == 4, result
            assert request({"kind": "quick"}, request_id=lost)["state"] == "unconfirmed"
            # Main can close independently; forwarding recreates only main.
            focus(main)
            key(main, "q")
            wait(lambda: window(False) is None, "main close")
            assert len(windows()) == 1
            cli("--note", str(uuid.uuid4()), ok=False)
            assert len(windows()) == 1
            cli()
            main = wait(lambda: window(False), "main reopen")["address"]
            assert len(windows()) == 2
            cli("--note", quick_id)
            wait(lambda: window(True) is None, "Quick handoff by note ID")
            focus(main)
            type_text(main, " retained", "Quick from a second invocation")
            key(main, "s")
            wait(lambda: (quick_id, "Quick from a second invocation retained") in stored(), "handoff retained editor")
            assert len(stored()) == 4
            print("PASS: real --new startup; display-free default/new/note/Quick forwarding; canonical aliases; native draft/caret retention; CLI validation; concurrent Quick reuse; distinct Wayland token targets; lost-reply replay; main reopen; Quick handoff. Valid-token focus policy, real IME and Orca remain unverified.")
    finally:
        stop(process)
        if endpoint is not None:
            endpoint.cleanup()
        stop(registry)


if __name__ == "__main__":
    if "--isolated" not in sys.argv:
        os.execvp("dbus-run-session", ["dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()), "--isolated"])
    run()
