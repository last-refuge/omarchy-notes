#!/usr/bin/env python3
"""Cross-version checks using scratch libraries and the real Qt document/lock APIs."""
import argparse
import hashlib
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("storage_driver", type=Path)
args = parser.parse_args()
binary, driver = args.binary.resolve(), args.storage_driver.resolve()
repo = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="omarchy-rust-compatibility-") as temporary:
    root = Path(temporary)
    probe = root / "qt-probe"
    flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "Qt6Core", "sqlite3"], text=True))
    compilation = subprocess.run(["g++", "-std=c++20", "-fPIC", f"-I{repo / 'src/core'}", str(repo / "tests/rust-compatibility-probe.cpp"), str(repo / "src/core/RichDocument.cpp"), "-o", str(probe), *flags], capture_output=True, text=True)
    if compilation.returncode:
        raise RuntimeError(compilation.stdout + compilation.stderr)
    library = root / "library"
    shutil.copytree(repo / "tests/fixtures/qt-v1", library)
    before = {p.relative_to(library): hashlib.sha256(p.read_bytes()).hexdigest() for p in (library / "blobs").rglob("*") if p.is_file()}
    subprocess.run([str(binary), "--data-dir", str(library), "--capture", "A Rust note\nRich words #compatibility"], check=True)
    subprocess.run([str(driver), str(library)], check=True)
    subprocess.run([str(probe), "verify", str(library)], check=True)
    assert all(hashlib.sha256((library / p).read_bytes()).hexdigest() == digest for p, digest in before.items())
    for owner, contender in [([str(probe), "hold", str(library)], [str(binary), "--data-dir", str(library), "--capture", "Must not be written"]),
                              ([str(driver), str(library), "hold"], [str(probe), "verify", str(library)])]:
        child = subprocess.Popen(owner, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        try:
            assert child.stdout.readline().strip() == "LOCKED"
            result = subprocess.run(contender, capture_output=True, text=True, timeout=10)
            assert result.returncode != 0, "Both editions acquired the same library"
        finally:
            child.communicate("close\n", timeout=5)
            assert child.returncode == 0
    print("Passed Qt → Rust edit → Qt validation, unchanged blob hashes, and mutual library exclusion.")
