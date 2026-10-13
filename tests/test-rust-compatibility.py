#!/usr/bin/env python3
"""Rust edit and lock regressions against a frozen legacy library (scratch copies only)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("storage_driver", type=Path)
args = parser.parse_args()
binary, driver = args.binary.resolve(), args.storage_driver.resolve()
repo = Path(__file__).resolve().parents[1]
env = dict(os.environ, DISPLAY="", WAYLAND_DISPLAY="", DBUS_SESSION_BUS_ADDRESS="")


def snapshot(library):
    with sqlite3.connect(library / "library.sqlite3") as db:
        return {id: (json.loads(body), revision) for id, body, revision in
                db.execute("SELECT id,body,revision FROM notes")}


with tempfile.TemporaryDirectory(prefix="omarchy-rust-compatibility-") as temporary:
    library = Path(temporary) / "library"
    shutil.copytree(repo / "tests/fixtures/qt-v1", library)
    original = snapshot(library)
    before = {p.relative_to(library): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in (library / "blobs").rglob("*") if p.is_file()}
    subprocess.run([str(binary), "--data-dir", str(library), "--capture",
                    "A Rust note\nRich words #compatibility"], env=env, check=True, timeout=30)
    captured = snapshot(library)
    assert len(captured) == len(original) + 1
    assert all(captured[id] == note for id, note in original.items())
    subprocess.run([str(driver), str(library)], env=env, check=True, timeout=30)
    edited = snapshot(library)
    assert len(edited) == len(captured) + 2  # Imported media and its duplicate.
    for id, (body, revision) in captured.items():
        expected = json.loads(json.dumps(body))
        expected["blocks"][0]["spans"].append({"text": " · Rust", "marks": ["italic"]})
        assert edited[id] == (expected, revision + 1), id
    assert all(hashlib.sha256((library / p).read_bytes()).hexdigest() == digest
               for p, digest in before.items())

    # The Rust storage owner must exclude both another storage owner and the CLI.
    child = subprocess.Popen([str(driver), str(library), "hold"], env=env,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    try:
        assert child.stdout.readline().strip() == "LOCKED"
        locked = snapshot(library)
        for contender in ([str(binary), "--data-dir", str(library), "--capture", "Must not be written"],
                          [str(driver), str(library)]):
            result = subprocess.run(contender, env=env, capture_output=True, text=True, timeout=10)
            assert result.returncode != 0, "Two Rust owners acquired the same library"
            assert snapshot(library) == locked
    finally:
        child.communicate("close\n", timeout=5)
        assert child.returncode == 0
    subprocess.run([str(binary), "--data-dir", str(library), "--capture", "After lock release"],
                   env=env, check=True, timeout=30)
    assert len(snapshot(library)) == len(edited) + 1
    print("PASS: Rust edits preserve frozen legacy documents and blob hashes; library exclusion and lock release.")
