#!/usr/bin/env python3
"""Round-trip folder operations through Rust and the supported Qt NoteStore."""
import argparse
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("driver", type=Path)
args = parser.parse_args()
repo = Path(__file__).resolve().parents[1]
env = dict(os.environ, DISPLAY="", WAYLAND_DISPLAY="", DBUS_SESSION_BUS_ADDRESS="")


def run(arguments):
    result = subprocess.run(list(map(str, arguments)), env=env, capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    print(result.stdout, end="")


with tempfile.TemporaryDirectory(prefix="notes-folder-compatibility-") as tmp:
    root = Path(tmp)
    probe = root / "qt-organization-probe"
    flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "Qt6Core", "sqlite3"], text=True))
    sources = [repo / "tests/qt-organization-probe.cpp"] + [repo / "src/core" / (name + ".cpp")
        for name in ["NoteStore", "LibraryBackup", "LibraryPaths", "Database", "BlobStore", "RichDocument"]]
    run(["g++", "-std=c++20", "-fPIC", '-DONOTES_VERSION="folder-test"', f"-I{repo / 'src/core'}",
        *sources, "-o", probe, *flags])
    library = root / "library"
    run([args.driver.resolve(), "seed", library])
    run([probe, library])
    run([args.driver.resolve(), "verify", library])
