#!/usr/bin/env python3
"""Capture synthetic Qt data for the Rust compatibility suite, never a user's library."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time


def capture(binary: Path, destination: Path) -> None:
    if destination.exists():
        raise SystemExit(f"Refusing to overwrite {destination}")
    with tempfile.TemporaryDirectory(prefix="omarchy-notes-qt-fixture-") as temporary:
        root = Path(temporary)
        library = root / "library"
        runtime = root / "runtime"
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software", QT_QPA_PLATFORMTHEME="",
                   XDG_RUNTIME_DIR=str(runtime))
        with (root / "app.log").open("w+") as log:
            process = subprocess.Popen([str(binary.resolve()), "--data-dir", str(library), "--samples"],
                                       env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 20
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        log.seek(0)
                        raise RuntimeError("Qt exited before completing its sample library:\n" + log.read())
                    try:
                        with sqlite3.connect(f"file:{library / 'library.sqlite3'}?mode=ro", uri=True) as db:
                            ready = db.execute("SELECT COUNT(*) FROM notes WHERE title <> ''").fetchone()[0] == 3
                        if ready:
                            break
                    except sqlite3.Error:
                        pass
                    time.sleep(0.1)
                else:
                    raise RuntimeError("Timed out waiting for committed sample notes")

                staged = root / "fixture"
                staged.mkdir()
                with sqlite3.connect(f"file:{library / 'library.sqlite3'}?mode=ro", uri=True) as source:
                    with sqlite3.connect(staged / "library.sqlite3") as snapshot:
                        source.backup(snapshot)
                        snapshot.execute("PRAGMA journal_mode=DELETE")
                        rows = snapshot.execute("SELECT id, title, body FROM notes ORDER BY title").fetchall()
                        blobs = snapshot.execute("SELECT hash, size FROM blobs ORDER BY hash").fetchall()
                        assert snapshot.execute("PRAGMA user_version").fetchone()[0] == 2
                        assert snapshot.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                        assert not snapshot.execute("PRAGMA foreign_key_check").fetchall()
                (staged / "documents.json").write_text(json.dumps([
                    {"id": note_id, "title": title, "body": json.loads(body)} for note_id, title, body in rows
                ], indent=2, ensure_ascii=False) + "\n")
                for digest, size in blobs:
                    relative = Path("blobs") / digest[:2] / digest
                    data = (library / relative).read_bytes()
                    assert len(data) == size and hashlib.sha256(data).hexdigest() == digest
                    (staged / relative).parent.mkdir(parents=True, exist_ok=True)
                    (staged / relative).write_bytes(data)
                (staged / "manifest.json").write_text(json.dumps({
                    "fixture": "Qt sample library, not the application backup format",
                    "database_schema": 2, "document_schema": 1,
                    "notes": len(rows), "blobs": len(blobs),
                    "files": {str(p.relative_to(staged)): hashlib.sha256(p.read_bytes()).hexdigest()
                              for p in sorted(staged.rglob("*")) if p.is_file()},
                }, indent=2) + "\n")
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copytree(staged, destination)
                print(f"Captured {len(rows)} notes and {len(blobs)} blobs in {destination}")
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    capture(args.binary, args.destination)
