#!/usr/bin/env python3
"""Rust edits of frozen table fixtures, undo/redo and exact SQLite reopen checks."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("driver", type=Path)
args = parser.parse_args()
env = dict(os.environ, DISPLAY="", WAYLAND_DISPLAY="", DBUS_SESSION_BUS_ADDRESS="")

with tempfile.TemporaryDirectory(prefix="notes-table-regression-") as tmp:
    output = Path(tmp) / "roundtrips"
    # The Rust driver loads the frozen merged-table fixture, exercises allowed
    # structural edits and undo/redo, then checks every body after Store reopen.
    subprocess.run([str(args.driver.resolve()), str(output)], env=env, check=True, timeout=60)
    files = sorted((output / "documents").glob("*.json"))
    assert files and (output / "documents/created-in-rust.json") in files
    for action in ("RowBefore", "RowAfter", "ColumnBefore", "ColumnAfter",
                   "DeleteRow", "DeleteColumn", "SplitCell"):
        assert any(p.stem.endswith("-" + action) for p in files), action
    expected = sorted(json.dumps(json.loads(p.read_text()), sort_keys=True) for p in files)
    with sqlite3.connect(output / "library/library.sqlite3") as db:
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        saved = sorted(json.dumps(json.loads(body), sort_keys=True)
                       for (body,) in db.execute("SELECT body FROM notes"))
    assert saved == expected, "Persisted documents differ from Rust edit output"
    print(f"PASS: {len(files)} Rust table documents survive structural edits, undo/redo and exact SQLite reopen.")
