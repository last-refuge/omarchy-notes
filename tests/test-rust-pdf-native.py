#!/usr/bin/env python3
"""Opt-in Hyprland release smoke test; owned-window input only, no screenshots.
Confirms the export shortcut takes focus and Escape restores writing, then saves.
Does not automate the portal or open a viewer. No clipboard access.
"""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time
from rust_native_test import InstanceSocket

repo = Path(__file__).resolve().parents[1]
binary = repo / "target/release/omarchy-notes-rust"
with tempfile.TemporaryDirectory(prefix="notes-pdf-native-") as temporary:
    root = Path(temporary)
    os.environ["XDG_CONFIG_HOME"] = str(root / "config")
    library = root / "library"
    subprocess.run([str(binary), "--data-dir", str(library), "--capture", "PDF workflow"], check=True, capture_output=True)
    with (root / "app.log").open("w") as log:
        process = subprocess.Popen([str(binary), "--data-dir", str(library), "--reference"], stdout=log, stderr=log)
    address = None
    instance_socket = None

    def state(kind):
        return json.loads(subprocess.check_output(["hyprctl", kind, "-j"], timeout=3))

    def guard():
        assert process.poll() is None, "Owned process exited"
        window, workspace = state("activewindow"), state("activeworkspace")
        assert window.get("pid") == process.pid and window.get("address") == address and window.get("class") == "org.omarchy.Notes.Rust", "Refusing input: test window lost focus"
        assert window["workspace"]["id"] == workspace["id"], "Refusing input: workspace changed"

    def key(name, ctrl=False, shift=False):
        assert name in ["k", "p", "s", "Return", "Escape", "End"]
        args = ["wtype"] + (["-M", "ctrl"] if ctrl else []) + (["-M", "shift"] if shift else []) + ["-k", name] + (["-m", "shift"] if shift else []) + (["-m", "ctrl"] if ctrl else [])
        guard(); subprocess.run(args, check=True, timeout=3); guard()

    def text(value):
        for char in value:
            assert char.isprintable()
            guard(); subprocess.run(["wtype", "--", char], check=True, timeout=3); guard()

    try:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            owned = [w for w in state("clients") if w.get("pid") == process.pid and w.get("class") == "org.omarchy.Notes.Rust"]
            if owned:
                address = owned[0]["address"]
                guard()
                break
            assert process.poll() is None, (root / "app.log").read_text()
            time.sleep(.1)
        assert address, "Owned window did not appear"
        instance_socket = InstanceSocket(library, process)
        time.sleep(.2)
        key("k", ctrl=True); text("PDF workflow"); key("Return")
        key("p", ctrl=True, shift=True); text("x")  # Sheet has focus; must not edit the note.
        key("Escape"); key("End"); text("!"); key("s", ctrl=True)
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                saved = db.execute("SELECT plain_text FROM notes").fetchone()[0]
            if saved == "PDF workflow!":
                break
            time.sleep(.05)
        assert saved == "PDF workflow!", saved
        print("PASS: release window, PDF shortcut focus, Escape restores editor, durable save; no portal/viewer claim")
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait(timeout=5)

        if instance_socket is not None:
            instance_socket.cleanup()
