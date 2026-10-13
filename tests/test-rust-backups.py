#!/usr/bin/env python3
"""Rust backup/restore regressions using a frozen legacy library and scratch copies."""
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
parser.add_argument("--quote-driver", type=Path, help="Also verify schema-3 upgrade and rollback")
args = parser.parse_args()
binary = args.binary.resolve()
repo = Path(__file__).resolve().parents[1]
# A headless operation must not need the user's graphical/session services.
env = dict(os.environ, DISPLAY="", WAYLAND_DISPLAY="", DBUS_SESSION_BUS_ADDRESS="")


def run(*arguments, ok=True):
    result = subprocess.run([str(x) for x in arguments], env=env, capture_output=True, text=True, timeout=30)
    if ok and result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    if not ok:
        assert result.returncode != 0, result.stdout
    return result


def hashes(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in root.rglob("*") if p.is_file()}


def dump(root):
    with sqlite3.connect(root / "library.sqlite3") as db:
        names = [row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")]
        return {name: sorted(map(repr, db.execute('SELECT * FROM "' + name.replace('"', '""') + '"')))
                for name in names}


with tempfile.TemporaryDirectory(prefix="notes-backup-compatibility-") as temporary:
    root = Path(temporary)
    library = root / "library"
    shutil.copytree(repo / "tests/fixtures/qt-v1", library)
    with sqlite3.connect(library / "library.sqlite3") as db:
        db.executescript("""
            INSERT INTO folders VALUES('parent',NULL,'Parent',7,11,12,NULL);
            INSERT INTO folders VALUES('child','parent','Child',8,13,14,NULL);
            INSERT INTO smart_folders VALUES('smart','History','{"tags":["planning"]}',5,15,16);
            UPDATE notes SET pinned=1,deleted_at=17,folder_id='child' WHERE title='Groceries';
        """)

    qt_backup = root / "Legacy format backup"
    run(binary, "--data-dir", library, "--backup", qt_backup)
    # Exercise the legacy format-1 manifest, which omitted Rust's optional
    # integrity fields. The fixture manifest itself is not an app backup.
    manifest = json.loads((qt_backup / "manifest.json").read_text())
    manifest.pop("databaseSha256", None)
    manifest.pop("committedRevisions", None)
    (qt_backup / "manifest.json").write_text(json.dumps(manifest))
    original_backup = hashes(qt_backup)
    original_content = dump(qt_backup)
    original_library = dump(library)
    run(binary, "--inspect-backup", qt_backup)
    restored = root / "Rust restore"
    run(binary, "--restore", qt_backup, "--data-dir", restored)
    assert hashes(qt_backup) == original_backup
    assert dump(restored) == original_content
    assert hashes(restored / "blobs") == hashes(library / "blobs")
    run(binary, "--data-dir", restored, "--capture", "A note added after restore #backup")
    rust_backup = root / "Rust backup ? # ü"
    run(binary, "--data-dir", restored, "--backup", rust_backup)
    assert "databaseSha256" in json.loads((rust_backup / "manifest.json").read_text())
    rust_hashes = hashes(rust_backup)
    run(binary, "--inspect-backup", rust_backup)
    assert hashes(rust_backup) == rust_hashes
    roundtrip = root / "Rust roundtrip"
    run(binary, "--restore", rust_backup, "--data-dir", roundtrip)
    assert dump(roundtrip) == dump(rust_backup)
    assert hashes(roundtrip / "blobs") == hashes(rust_backup / "blobs")
    assert hashes(rust_backup) == rust_hashes
    assert dump(library) == original_library

    # Headless misuse must fail before creating/changing a library or destination.
    before = hashes(rust_backup)
    run(binary, "--backup", rust_backup, "--data-dir", restored, ok=False)
    run(binary, "--restore", rust_backup, "--data-dir", library, ok=False)
    run(binary, "--restore", rust_backup, ok=False)
    run(binary, "--restore", rust_backup, "--capture", "no", "--data-dir", root / "bad", ok=False)
    run(binary, "--demo", "--backup", root / "bad", ok=False)
    run(binary, "--inspect-backup", rust_backup, "--data-dir", root / "bad", ok=False)
    run(binary, "--data-dir", root / "missing", "--backup", root / "bad", ok=False)
    assert not (root / "bad").exists() and not (root / "missing").exists()
    assert before == hashes(rust_backup) and dump(library) == original_library

    # A legacy format-1 manifest with a schema-1 snapshot is restored without
    # migration; opening the restored copy performs the normal Rust migration.
    old = root / "Schema 1"
    shutil.copytree(qt_backup, old)
    with sqlite3.connect(old / "library.sqlite3") as db:
        db.executescript("""
            DROP TABLE note_tags; DROP TABLE note_attachment_refs; DROP TABLE smart_folders;
            ALTER TABLE notes DROP COLUMN thumbnail; PRAGMA user_version=1;
        """)
        total = db.execute("SELECT coalesce(sum(size),0) FROM blobs").fetchone()[0]
    manifest = json.loads((old / "manifest.json").read_text())
    manifest.update(schemaVersion=1, bytes=total + (old / "library.sqlite3").stat().st_size)
    (old / "manifest.json").write_text(json.dumps(manifest))
    old_before = hashes(old)
    old_restore = root / "Schema 1 restored"
    run(binary, "--restore", old, "--data-dir", old_restore)
    assert hashes(old) == old_before and dump(old_restore) == dump(old)
    with sqlite3.connect(old_restore / "library.sqlite3") as db:
        assert db.execute("PRAGMA user_version").fetchone()[0] == 1
    run(binary, "--data-dir", old_restore, "--capture", "After schema-1 migration")
    with sqlite3.connect(old_restore / "library.sqlite3") as db:
        assert db.execute("PRAGMA user_version").fetchone()[0] == 2
    assert hashes(old) == old_before

    if args.quote_driver:
        upgraded = root / "quotes"
        shutil.copytree(library, upgraded)
        before_upgrade = dump(upgraded)
        safety = root / "before quotes"
        run(args.quote_driver.resolve(), upgraded, safety)
        assert dump(safety) == before_upgrade
        run(binary, "--inspect-backup", safety)
        with sqlite3.connect(upgraded / "library.sqlite3") as db:
            assert db.execute("PRAGMA user_version").fetchone()[0] == 3
            assert db.execute("SELECT count(*) FROM schema_migrations WHERE version=3").fetchone()[0] == 1
            quoted = db.execute("SELECT body,body_schema FROM notes WHERE title='A quotation worth keeping.'").fetchone()
            assert quoted[1] == 2 and json.loads(quoted[0])["blocks"][0]["type"] == "quote"
        new_backup = root / "quoted backup"
        run(binary, "--data-dir", upgraded, "--backup", new_backup)
        run(binary, "--inspect-backup", new_backup)
        preserved = hashes(new_backup)
        modern_restore = root / "quoted restore"
        run(binary, "--restore", new_backup, "--data-dir", modern_restore)
        assert dump(modern_restore) == dump(upgraded) and hashes(new_backup) == preserved
        rollback = root / "quote rollback"
        run(binary, "--restore", safety, "--data-dir", rollback)
        assert dump(rollback) == before_upgrade
        run(binary, "--data-dir", rollback, "--capture", "After rollback")
        print("PASS: Rust schema-3 quote backup/restore and schema-2 rollback.")

print("PASS: Rust legacy-format backup/restore, full table/history/trash/blob preservation, schema-1 migration, immutable backups and headless CLI failure paths.")
