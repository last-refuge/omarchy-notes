#!/usr/bin/env python3
"""Native startup/IPC checks with disposable libraries; no synthetic input.
All spawned processes are tracked and stopped individually. Requires a display.
"""
import concurrent.futures
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time
import uuid
from rust_native_test import InstanceSocket

REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / "target/release/omarchy-notes-preview"


def wait(predicate, label, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(.04)
    raise AssertionError("Timed out: " + label)


def stop(p):
    if p.poll() is None:
        p.terminate()
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait(timeout=5)


def run():
    children = []
    endpoints = []
    outputs = []
    client_env = dict(os.environ)
    for name in ("WAYLAND_DISPLAY", "DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "AT_SPI_BUS_ADDRESS", "XDG_ACTIVATION_TOKEN"):
        client_env.pop(name, None)
    try:
        with tempfile.TemporaryDirectory(prefix="notes-launch-startup-") as temporary:
            os.environ["XDG_CONFIG_HOME"] = str(Path(temporary) / "config")
            def start(root, name, *args):
                output = (REPO / ("target/launch-startup-" + name + ".log")).open("w")
                outputs.append(output)
                p = subprocess.Popen([str(BINARY), "--data-dir", str(root), "--reference", *args], stdout=output, stderr=output)
                children.append(p)
                return p

            def rows(root):
                try:
                    with sqlite3.connect(f"file:{root / 'library.sqlite3'}?mode=ro", uri=True) as db:
                        return db.execute("SELECT id,plain_text FROM notes").fetchall()
                except sqlite3.OperationalError:
                    return []

            def cli(root, *args, ok=True):
                p = subprocess.run([str(BINARY), "--data-dir", str(root), *args], env=client_env, text=True, capture_output=True, timeout=15)
                assert (p.returncode == 0) == ok, (p.stdout, p.stderr)
                return p

            root = Path(temporary) / "raced"
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                pair = list(pool.map(lambda n: start(root, "race-" + str(n), "--new"), range(2)))
            wait(lambda: len(rows(root)) == 2 and sum(p.poll() is None for p in pair) == 1, "two startup intents, one surviving owner")
            owner = next(p for p in pair if p.poll() is None)
            client = next(p for p in pair if p.poll() is not None)
            assert client.returncode == 0
            endpoints.append(InstanceSocket(root, owner))
            clients = json.loads(subprocess.check_output(["hyprctl", "clients", "-j"], timeout=3))
            owned = [w for w in clients if w.get("pid") in [p.pid for p in pair]]
            assert len(owned) == 1 and owned[0]["pid"] == owner.pid
            assert owned[0]["class"] == "org.omarchy.Notes.Preview"
            cli(root, "--new")
            assert len(rows(root)) == 3
            stop(owner)
            endpoints[-1].cleanup()
            # Reopen an explicit existing ID, including a killed owner's stale
            # private socket; it must neither create nor duplicate a note.
            id = rows(root)[0][0]
            owner = start(root, "specific", "--note", id)
            wait(lambda: any(w.get("pid") == owner.pid and w.get("class") == "org.omarchy.Notes.Preview" for w in json.loads(subprocess.check_output(["hyprctl", "clients", "-j"], timeout=3))), "specific note startup")
            endpoints.append(InstanceSocket(root, owner))
            assert id in cli(root).stdout and len(rows(root)) == 3
            stop(owner)
            endpoints[-1].cleanup()
            cli(root, "--note", str(uuid.uuid4()), ok=False)
            assert len(rows(root)) == 3

            failed = Path(temporary) / "failure"
            owner = start(failed, "failure", "--simulate-save-failure")
            wait(lambda: any(w.get("pid") == owner.pid and w.get("class") == "org.omarchy.Notes.Preview" for w in json.loads(subprocess.check_output(["hyprctl", "clients", "-j"], timeout=3))), "failure owner")
            endpoints.append(InstanceSocket(failed, owner))
            reply = cli(failed, "--new", ok=False)
            assert "Couldn’t create" in reply.stderr and not rows(failed)
            cli(failed)  # Still responsive after a failed creation.
            assert owner.poll() is None
            print("PASS: raced first --new launches produce two notes/one GUI owner; display-free forwarding; explicit-note startup; unknown startup target; failed creation never reports Opened and owner remains responsive.")
    finally:
        for p in children:
            stop(p)
        for endpoint in endpoints:
            endpoint.cleanup()
        for output in outputs:
            output.close()


if __name__ == "__main__":
    run()
