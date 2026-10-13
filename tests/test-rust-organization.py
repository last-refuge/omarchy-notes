#!/usr/bin/env python3
"""Rust folder-operation persistence checks in a scratch library."""
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

with tempfile.TemporaryDirectory(prefix="notes-folder-regression-") as tmp:
    library = Path(tmp) / "library"
    # seed performs Rust create/rename/move operations and asserts that a stale
    # content save cannot undo filing. Its verify mode requires Qt mutations,
    # so validate the persisted Rust result directly instead.
    subprocess.run([str(args.driver.resolve()), "seed", str(library)], env=env,
                   check=True, timeout=60)
    expected = json.loads((library / "expected.json").read_text())
    with sqlite3.connect(library / "library.sqlite3") as db:
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert db.execute("PRAGMA user_version").fetchone()[0] == 2
        folders = {id: {"id": id, "name": name, "parent": parent}
                   for id, name, parent in db.execute("SELECT id,name,parent_id FROM folders")}
        assert folders == {f["id"]: f for f in expected["folders"]}
        assert len(folders) == 5
        subject = folders[expected["subject"]]
        assert subject["name"] == "Projects"
        assert folders[subject["parent"]]["name"] == "Personal"
        assert folders[expected["work"]]["name"] == "Work"
        clients = [f for f in folders.values() if f["name"] == "Clients"]
        assert len(clients) == 2 and len({f["parent"] for f in clients}) == 2
        note = expected["note"]
        body, revision, updated, folder = db.execute(
            "SELECT body,revision,updated_at,folder_id FROM notes WHERE id=?", (note["id"],)
        ).fetchone()
        assert json.loads(body) == note["body"]
        assert revision == note["revision"] and updated == note["updated_at"]
        assert folder == expected["subject"]
        assert db.execute("SELECT count(*) FROM notes").fetchone()[0] == 2
    print("PASS: Rust folder identities, nesting, rename/move and stale-save protection persist in SQLite.")
