#!/usr/bin/env python3
"""Focused Rust identity/immutable preparation checks; no build or installation."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("prepare_release", ROOT / "packaging/prepare-release.py")
prepare_release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare_release)


class RustReleaseTests(unittest.TestCase):
    def test_metadata_names_and_current_preview_version_align(self):
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        component = ET.parse(ROOT / "packaging/metainfo/org.omarchy.Notes.Rust.metainfo.xml").getroot()
        self.assertEqual(component.findtext("id"), "org.omarchy.Notes.Rust")
        self.assertEqual(component.findtext("launchable"), "org.omarchy.Notes.Rust.desktop")
        self.assertEqual(component.findtext("provides/binary"), "omarchy-notes-rust")
        self.assertEqual(component.find("releases/release").get("version"), version)
        self.assertEqual(component.find("releases/release").get("type"), "development" if version.startswith("0.") else "stable")
        if os.environ.get("GITHUB_REF_TYPE") == "tag" and os.environ.get("GITHUB_REF_NAME", "").startswith("rust-v"):
            self.assertEqual(os.environ["GITHUB_REF_NAME"], f"rust-v{version}")
        self.assertIn(f"omarchy-notes-rust {version}", (ROOT / "packaging/man/omarchy-notes-rust.1").read_text())
        desktop = (ROOT / "packaging/desktop/org.omarchy.Notes.Rust.desktop").read_text()
        self.assertIn("Exec=/usr/bin/omarchy-notes-rust\n", desktop)
        self.assertIn("StartupWMClass=org.omarchy.Notes.Rust", desktop)
        self.assertIn("Exec=/usr/bin/omarchy-notes-rust --quick-note", desktop)
        recipe = (ROOT / "packaging/arch/PKGBUILD-rust.in").read_text()
        self.assertIn("pkgname=omarchy-notes-rust\n", recipe)
        self.assertNotIn("conflicts=", recipe)
        self.assertNotIn("provides=", recipe)
        self.assertIn("arch=('x86_64')", recipe)
        self.assertIn("--locked --offline", recipe)
        self.assertNotIn("qt6-", recipe)

    def test_preview_launcher_preserves_arguments_exit_and_stable_destination(self):
        launcher = (ROOT / "packaging/rust/omarchy-notes-preview").read_text()
        self.assertIn('exec /usr/bin/omarchy-notes-rust "$@"', launcher)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executable = root / "omarchy-notes-rust"
            executable.write_text("#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\nsys.exit(37)\n")
            executable.chmod(0o755)
            alias = root / "omarchy-notes-preview"
            alias.write_text(launcher.replace("/usr/bin/omarchy-notes-rust", str(executable)))
            args = ["--data-dir", "two words", "literal$(not-a-command)"]
            result = subprocess.run(["sh", str(alias), *args], capture_output=True, text=True)
            self.assertEqual(result.returncode, 37)
            self.assertEqual(json.loads(result.stdout), args)

    def test_prepare_fetches_only_the_locked_advertised_target_in_private_cargo_home(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "src"
            (source / "omarchy-notes-rust-@RUST_VERSION@").mkdir(parents=True)
            tools = root / "bin"
            tools.mkdir()
            cargo = tools / "cargo"
            cargo.write_text(
                "#!/usr/bin/env python3\n"
                "import json,os,sys\n"
                "from pathlib import Path\n"
                "Path(os.environ['RUST_RELEASE_FETCH_LOG']).write_text(json.dumps({"
                "'args':sys.argv[1:],'cwd':os.getcwd(),'cargo_home':os.environ.get('CARGO_HOME')}))\n"
            )
            cargo.chmod(0o755)
            rustup = tools / "rustup"
            rustup.write_text("#!/bin/sh\nexit 0\n")
            rustup.chmod(0o755)
            log = root / "fetch.json"
            env = dict(os.environ, PATH=str(tools) + ":" + os.environ["PATH"], RUST_RELEASE_FETCH_LOG=str(log))
            subprocess.run(
                ["bash", "-c", 'source "$1"; srcdir="$2"; prepare', "rust-recipe-prepare",
                 str(ROOT / "packaging/arch/PKGBUILD-rust.in"), str(source)],
                env=env, check=True, capture_output=True, text=True,
            )
            recorded = json.loads(log.read_text())
            self.assertEqual(recorded["args"], ["fetch", "--locked", "--target", "x86_64-unknown-linux-gnu"])
            self.assertEqual(recorded["cwd"], str(source / "omarchy-notes-rust-@RUST_VERSION@"))
            self.assertEqual(recorded["cargo_home"], str(source / "cargo-home"))

    def test_candidate_preparation_is_exact_repeatable_and_rejects_cross_edition_tags(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repository"
            repo.mkdir()
            (repo / "packaging/arch").mkdir(parents=True)
            (repo / "Cargo.toml").write_text('[workspace.package]\nversion="0.1.0"\n')
            (repo / "CMakeLists.txt").write_text("project(omarchy-notes VERSION 1.0.0)\n")
            for name in ("PKGBUILD.in", "PKGBUILD-rust.in"):
                (repo / "packaging/arch" / name).write_bytes((ROOT / "packaging/arch" / name).read_bytes())
            (repo / "tracked").write_text("committed source\n")
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
            subprocess.run(["git", "-C", str(repo), "-c", "user.name=Release test", "-c", "user.email=release-test@example.invalid", "commit", "-qm", "fixture"], check=True)
            revision = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
            subprocess.run(["git", "-C", str(repo), "tag", "v1.0.0"], check=True)
            subprocess.run(["git", "-C", str(repo), "tag", "rust-v0.1.0"], check=True)
            (repo / "tracked").write_text("uncommitted change must not leak\n")
            (repo / "untracked").write_text("not a release input\n")
            bin_dir = root / "bin"; bin_dir.mkdir()
            shim = bin_dir / "makepkg"
            shim.write_text("#!/bin/sh\nprintf 'pkgbase = omarchy-notes-rust\\n\\tpkgname = omarchy-notes-rust\\n'\n")
            shim.chmod(0o755)
            old_path = os.environ["PATH"]
            os.environ["PATH"] = str(bin_dir) + ":" + old_path
            try:
                with self.assertRaisesRegex(ValueError, "require --candidate"):
                    prepare_release.prepare(repo, "HEAD", root / "refused", "rust")
                with self.assertRaisesRegex(ValueError, "edition"):
                    prepare_release.prepare(repo, "v1.0.0", root / "wrong-rust", "rust", True)
                with self.assertRaisesRegex(ValueError, "edition"):
                    prepare_release.prepare(repo, "rust-v0.1.0", root / "wrong-qt")
                first, second = root / "first", root / "second"
                prepare_release.prepare(repo, revision, first, "rust", True)
                prepare_release.prepare(repo, revision, second, "rust", True)
                self.assertEqual((first / "SOURCE_REVISION").read_text().strip(), revision)
                self.assertEqual((first / "RELEASE_KIND").read_text().strip(), "candidate")
                self.assertEqual((first / "SHA256SUMS").read_bytes(), (second / "SHA256SUMS").read_bytes())
                self.assertNotIn("@SOURCE_B2SUM@", (first / "aur/PKGBUILD").read_text())
                archive = first / "omarchy-notes-rust-0.1.0.tar.gz"
                with tarfile.open(archive) as source:
                    prefix = "omarchy-notes-rust-0.1.0/"
                    self.assertEqual(source.extractfile(prefix + "tracked").read(), b"committed source\n")
                    self.assertEqual(source.extractfile(prefix + "SOURCE_REVISION").read().decode().strip(), revision)
                    self.assertNotIn(prefix + "untracked", source.getnames())
                for line in (first / "SHA256SUMS").read_text().splitlines():
                    digest, name = line.split()
                    self.assertEqual(hashlib.sha256((first / name).read_bytes()).hexdigest(), digest)
            finally:
                os.environ["PATH"] = old_path


if __name__ == "__main__":
    unittest.main()
