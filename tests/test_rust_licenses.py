"""Focused source-only tests for the candidate notice collector."""
import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "packaging/rust/collect-licenses.py"
spec = importlib.util.spec_from_file_location("rust_licenses", SCRIPT)
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)


class LicenseCollection(unittest.TestCase):
    def fixture(self, directory):
        root = pathlib.Path(directory)
        (root / "Cargo.toml").write_text("[workspace]\nmembers=[]\n")
        (root / "LICENSE").write_text("Fixture workspace MIT notice\n")
        packages = []
        for name in ("app", "normal", "build", "dev"):
            crate = root / name
            crate.mkdir()
            (crate / "Cargo.toml").write_text(f'[package]\nname="{name}"\nversion="1.0.0"\n')
            if name != "app":
                (crate / "LICENSE-MIT").write_text(f"Exact {name} license notice\n")
            packages.append({"id": name, "name": name, "version": "1.0.0", "source": None,
                             "manifest_path": str(crate / "Cargo.toml"), "license": "MIT", "license_file": None})
        nodes = [{"id": name, "deps": []} for name in ("app", "normal", "build", "dev")]
        nodes[0]["deps"] = [{"pkg": name, "dep_kinds": [{"kind": kind}]} for name, kind in (("normal", None), ("build", "build"), ("dev", "dev"))]
        return {"packages": packages, "workspace_root": str(root), "workspace_members": ["app", "normal", "build", "dev"], "resolve": {"nodes": nodes}}

    def test_non_dev_closure_workspace_fallback_and_deterministic_inventory(self):
        outputs = []
        for _ in range(2):
            with tempfile.TemporaryDirectory() as directory:
                metadata = self.fixture(directory)
                output = pathlib.Path(directory) / "notices"
                report = collector.collect(metadata, "app", "x86_64-unknown-linux-gnu", output)
                self.assertEqual([p["name"] for p in report["packages"]], ["app", "build", "normal"])
                self.assertEqual(report["missing"], [])
                self.assertIn("review remains required", report["status"])
                outputs.append((output / "inventory.json").read_bytes())
                for package in report["packages"]:
                    self.assertTrue(package["license_files"])
                    for notice in package["license_files"]:
                        self.assertEqual(collector.hashlib.sha256((output / notice["path"]).read_bytes()).hexdigest(), notice["sha256"])
        self.assertEqual(outputs[0], outputs[1])

    def test_missing_text_is_explicit_and_existing_output_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata = self.fixture(directory)
            metadata["workspace_members"] = ["app"]
            (pathlib.Path(directory) / "normal/LICENSE-MIT").unlink()
            metadata["packages"][1]["source"] = "registry+https://example.invalid/index"
            output = pathlib.Path(directory) / "notices"
            report = collector.collect(metadata, "app", "x86_64-unknown-linux-gnu", output)
            self.assertEqual(report["status"], "incomplete")
            self.assertEqual(len(report["missing"]), 1)
            original = (output / "inventory.json").read_bytes()
            with self.assertRaises(ValueError):
                collector.collect(metadata, "app", "x86_64-unknown-linux-gnu", output)
            self.assertEqual((output / "inventory.json").read_bytes(), original)

    def test_pinned_workspace_license_links_are_copied_but_escaping_links_fail(self):
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as outside:
            metadata = self.fixture(directory)
            path = pathlib.Path(directory) / "normal/LICENSE-MIT"
            path.unlink()
            path.symlink_to("../LICENSE")
            report = collector.collect(metadata, "app", "x86_64-unknown-linux-gnu", pathlib.Path(directory) / "safe")
            self.assertFalse(report["missing"])
            path.unlink()
            target = pathlib.Path(outside) / "LICENSE"
            target.write_text("Do not collect unrelated files\n")
            path.symlink_to(target)
            with self.assertRaises(ValueError):
                collector.collect(metadata, "app", "x86_64-unknown-linux-gnu", pathlib.Path(directory) / "unsafe")
            self.assertFalse((pathlib.Path(directory) / "unsafe").exists())

    def test_exact_supplement_identity_and_hash_are_required_before_output(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata = self.fixture(directory)
            root = pathlib.Path(directory)
            item = metadata["packages"][1]
            item["source"] = "registry+https://example.invalid/index"
            metadata["workspace_members"] = ["app"]
            (root / "normal/LICENSE-MIT").unlink()
            (root / "normal/.cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": "exact-revision"}}))
            (root / "Cargo.lock").write_text('version=4\n[[package]]\nname="normal"\nversion="1.0.0"\nsource="registry+https://example.invalid/index"\nchecksum="exact-archive"\n')
            data = b"Exact upstream copyright and license\n"
            (root / "source-license.txt").write_bytes(data)
            entry = {"name": "normal", "version": "1.0.0", "source": item["source"], "license": "MIT",
                     "registry_package_sha256": "exact-archive", "vcs_revision": "exact-revision",
                     "files": [{"path": "source-license.txt", "sha256": collector.hashlib.sha256(data).hexdigest(),
                                "upstream_revision": "exact-revision", "url": "https://example.invalid/exact-revision/LICENSE"}]}
            supplement = root / "supplement.json"
            supplement.write_text(json.dumps({"version": 1, "packages": [entry]}))
            report = collector.collect(metadata, "app", "linux", root / "good", supplement_file=supplement)
            self.assertFalse(report["missing"])
            normal = next(p for p in report["packages"] if p["name"] == "normal")
            self.assertEqual(normal["license_files"][0]["upstream_revision"], "exact-revision")
            entry["registry_package_sha256"] = "different-archive"
            supplement.write_text(json.dumps({"version": 1, "packages": [entry]}))
            with self.assertRaisesRegex(ValueError, "publisher identity"):
                collector.collect(metadata, "app", "linux", root / "wrong-identity", supplement_file=supplement)
            self.assertFalse((root / "wrong-identity").exists())
            entry["registry_package_sha256"] = "exact-archive"
            supplement.write_text(json.dumps({"version": 1, "packages": [entry]}))
            (root / "source-license.txt").write_text("tampered notice\n")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                collector.collect(metadata, "app", "linux", root / "wrong-text", supplement_file=supplement)
            self.assertFalse((root / "wrong-text").exists())

    def test_shared_git_notice_requires_same_declaring_expression(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata = self.fixture(directory)
            root = pathlib.Path(directory)
            (root / "Cargo.toml").write_text('[workspace]\nmembers=[]\n[package]\nname="upstream"\nversion="1.0.0"\nlicense="MIT"\n')
            metadata["workspace_members"] = ["app"]
            item = metadata["packages"][1]
            item["source"] = "git+https://example.invalid/repo#exact"
            (root / "normal/LICENSE-MIT").unlink()
            report = collector.collect(metadata, "app", "linux", root / "same", supplement_file=None)
            self.assertFalse(report["missing"])
            (root / "Cargo.toml").write_text('[workspace]\nmembers=[]\n[package]\nname="upstream"\nversion="1.0.0"\nlicense="GPL-3.0-only"\n')
            report = collector.collect(metadata, "app", "linux", root / "different", supplement_file=None)
            self.assertEqual(len(report["missing"]), 1)

    def test_local_candidate_is_explicit_while_strict_and_default_fail_gaps(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata = self.fixture(directory)
            root = pathlib.Path(directory)
            metadata["workspace_members"] = ["app"]
            metadata["packages"][1]["source"] = "registry+https://example.invalid/index"
            (root / "normal/LICENSE-MIT").unlink()
            saved = root / "metadata.json"
            saved.write_text(json.dumps(metadata))
            for flag, expected in ((None, 1), ("--require-complete", 1), ("--candidate", 0)):
                output = root / (flag or "default").lstrip("-")
                command = [sys.executable, str(SCRIPT), "--package", "app", "--metadata", str(saved), "--output", str(output)]
                if flag:
                    command.append(flag)
                result = subprocess.run(command, capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
                report = json.loads((output / "inventory.json").read_text())
                self.assertFalse(report["collection_complete"])
                self.assertEqual(len(report["missing"]), 1)
                if flag == "--candidate":
                    self.assertIn("LOCAL CANDIDATE ONLY", result.stdout)
                    self.assertIn("local candidate testing only", report["purpose"])
            (root / "RELEASE_KIND").write_text("stable\n")
            with self.assertRaisesRegex(ValueError, "not explicitly marked candidate"):
                collector.collect(metadata, "app", "linux", root / "stable-candidate", candidate=True)
            self.assertFalse((root / "stable-candidate").exists())
            (root / "RELEASE_KIND").write_text("candidate\n")
            report = collector.collect(metadata, "app", "linux", root / "marked-candidate", candidate=True)
            self.assertFalse(report["collection_complete"])


if __name__ == "__main__":
    unittest.main()
