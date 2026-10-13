#!/usr/bin/env python3
"""Round-trip Rust table edits through SQLite and the actual Qt editable document.
Only scratch directories are used. Requires Qt6Core/Qt6Gui, Python and g++.
"""
import argparse
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("driver",type=Path)
args=parser.parse_args()
repo=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="notes-table-compatibility-") as tmp:
    root=Path(tmp)
    probe=root/"qt-table-probe"
    flags=shlex.split(subprocess.check_output(["pkg-config","--cflags","--libs","Qt6Core","Qt6Gui"],text=True))
    compilation=subprocess.run(["g++","-std=c++20","-fPIC",f"-I{repo/'src/core'}",str(repo/"tests/qt-table-probe.cpp"),str(repo/"src/core/RichDocument.cpp"),str(repo/"src/core/DocumentConverter.cpp"),"-o",str(probe),*flags],capture_output=True,text=True)
    if compilation.returncode: raise RuntimeError(compilation.stdout+compilation.stderr)
    output=root/"roundtrips"
    subprocess.run([str(args.driver.resolve()),str(output)],check=True)
    runtime=root/"runtime";runtime.mkdir(mode=0o700)
    env=dict(os.environ,QT_QPA_PLATFORM="offscreen",QT_QPA_PLATFORMTHEME="",XDG_RUNTIME_DIR=str(runtime))
    files=[repo/"tests/fixtures/qt-merged-table.json",*sorted((output/"documents").glob("*.json"))]
    for file in files:
        result=subprocess.run([str(probe),"verify",str(file)],env=env,capture_output=True,text=True)
        if result.returncode: raise RuntimeError(result.stdout+result.stderr)
    print(f"PASS: {len(files)} documents retain exact known schema content, cell spans, formatting and identities through Qt load/read; Rust edits also survive SQLite reopen.")
