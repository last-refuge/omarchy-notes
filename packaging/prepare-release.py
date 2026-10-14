#!/usr/bin/env python3
"""Prepare immutable source and AUR artifacts from a committed revision. No publishing."""

import argparse
import gzip
import hashlib
import io
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib


def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args])


def archive(path, root, names, epoch):
    # Stable metadata and gzip header make repeated preparation of a revision reproducible.
    def normalize(info):
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        info.mtime = epoch
        if info.isdir():
            info.mode = 0o755
        elif info.isfile():
            info.mode = 0o755 if info.mode & 0o111 else 0o644
        return info

    with path.open("wb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as tar:
            for name in names:
                tar.add(root / name, arcname=name, filter=normalize)


def prepare(repo, ref, output, candidate=False):
    revision = git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}").decode().strip()
    epoch = int(git(repo, "show", "-s", "--format=%ct", revision))
    if output.exists():
        raise ValueError("Output directory already exists; choose a new directory")
    with tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary)
        tree = work / "tree"
        tree.mkdir()
        with tarfile.open(fileobj=io.BytesIO(git(repo, "archive", revision))) as tar:
            tar.extractall(tree, filter="data")
        version = tomllib.loads((tree / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
            raise ValueError("Rust workspace must use a supported release version")
        if ref.startswith("v") or (ref.startswith("rust-v") and ref != f"rust-v{version}"):
            raise ValueError("Rust tag does not match the source version")
        if not candidate and (version.startswith("0.") or ref != f"rust-v{version}"):
            raise ValueError("Unreleased Rust sources require --candidate; stable preparation requires the exact rust-vVERSION tag")
        template = (tree / "packaging/arch/PKGBUILD-rust.in").read_text()
        if template.count("@RUST_VERSION@") != 1:
            raise ValueError("Rust PKGBUILD has no unique version placeholder")
        template = template.replace("@RUST_VERSION@", version)
        if not candidate:
            template = template.replace(" (early access; v1 acceptance pending)", "")
        source_name = f"omarchy-notes-{version}"
        kind = "candidate" if candidate else "stable"
        (tree / "SOURCE_REVISION").write_text(revision + "\n")
        (tree / "RELEASE_KIND").write_text(kind + "\n")
        if f"pkgver={version}\n" not in template or template.count("@SOURCE_B2SUM@") != 1:
            raise ValueError("PKGBUILD template does not match the source version")
        source = work / source_name
        tree.rename(source)
        artifacts = work / "artifacts"
        artifacts.mkdir()
        source_archive = artifacts / f"{source_name}.tar.gz"
        archive(source_archive, work, [source_name], epoch)
        checksum = hashlib.blake2b(source_archive.read_bytes()).hexdigest()
        aur = artifacts / "aur"
        aur.mkdir()
        (aur / "PKGBUILD").write_text(template.replace("@SOURCE_B2SUM@", checksum))
        srcinfo = subprocess.check_output(["makepkg", "--printsrcinfo"], cwd=aur)
        (aur / ".SRCINFO").write_bytes(srcinfo)
        archive(artifacts / f"{source_name}-aur.tar.gz", artifacts, ["aur"], epoch)
        # makepkg uses this exact archive locally before it is published at the source URL.
        shutil.copyfile(source_archive, aur / source_archive.name)
        (artifacts / "SOURCE_REVISION").write_text(revision + "\n")
        (artifacts / "RELEASE_KIND").write_text(kind + "\n")
        with (artifacts / "SHA256SUMS").open("w") as sums:
            for path in sorted(artifacts.iterdir()):
                if path.is_file() and path.name != "SHA256SUMS":
                    sums.write(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(artifacts, output)
    print(f"Prepared {source_name} from {revision} in {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--ref", required=True, help="Committed revision or release tag")
    parser.add_argument("--candidate", action="store_true", help="Prepare unannounced Rust candidate artifacts without claiming a stable release")
    parser.add_argument("--output", required=True, type=Path, help="New output directory")
    args = parser.parse_args()
    try:
        prepare(args.repo.resolve(), args.ref, args.output.resolve(), args.candidate)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release preparation failed: {error}\n")
