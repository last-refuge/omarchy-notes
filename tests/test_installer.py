#!/usr/bin/env python3
"""Exercise the installer with isolated release assets and command shims.

No command reaches the network, sudo, pacman, or the user's notes/configuration.
PTY cases exercise the actual spinner, prompts, and interrupt cleanup.
"""

import hashlib
import fcntl
import io
import json
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import struct
import sys
import tarfile
import termios
import tempfile
import time
import unittest

INSTALLER = Path(sys.argv.pop(1)).resolve()
PACKAGE = "omarchy-notes-1.0.0-1-x86_64.pkg.tar.zst"
REPO = "https://github.com/last-refuge/omarchy-notes"

SHIM = r'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys, time
root = pathlib.Path(os.environ["MOCK_ROOT"])
cfg = json.loads((root / "config.json").read_text())
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with (root / "calls.jsonl").open("a") as calls:
    calls.write(json.dumps([name, args]) + "\n")
def fail(message, code=1):
    print(message, file=sys.stderr)
    sys.exit(code)
if name == "uname":
    print(cfg.get("arch", "x86_64") if "-m" in args else cfg.get("os", "Linux"))
elif name == "curl":
    url = args[-1]
    asset = url.rsplit("/", 1)[-1]
    if cfg.get("slow_asset") == asset:
        child = subprocess.Popen(["sleep", "120"])
        (root / "child.pid").write_text(str(child.pid))
        (root / "worker.pid").write_text(str(os.getpid()))
        child.wait()
    if cfg.get("fail_asset") == asset:
        fail("curl: simulated HTTP 404 / network failure", 22)
    if "--write-out" in args:
        print(cfg.get("latest_url", "https://github.com/last-refuge/omarchy-notes/releases/tag/v1.0.0"), end="")
    else:
        source = root / "assets" / asset
        if not source.exists():
            fail("curl: asset missing", 22)
        destination = pathlib.Path(args[args.index("--output") + 1])
        data = source.read_bytes()
        destination.write_bytes(b"corrupted" if cfg.get("corrupt_asset") == asset else data)
elif name == "sudo":
    if cfg.get("deny_sudo"):
        fail("sudo: a password is required")
    if args and args[0] == "-n":
        args = args[1:]
    sys.exit(subprocess.call(args, stdin=sys.stdin))
elif name == "pacman":
    if args[0] == "-Q":
        state = root / "installed"
        if not state.exists(): sys.exit(1)
        print("omarchy-notes " + state.read_text())
    elif args[0] == "-Qp":
        print(cfg.get("identity", "omarchy-notes 1.0.0-1"))
    elif args[0] == "-Qip":
        print("Architecture    : " + cfg.get("package_arch", "x86_64"))
    elif args[0] == "-Ql":
        print("omarchy-notes " + str(root / "system/bin/omarchy-notes"))
    elif args[0] == "-U":
        if cfg.get("deny_pacman"): fail("pacman: simulated transaction failure")
        (root / "installed").write_text("1.0.0-1")
        print("pacman transaction visible")
    elif args[0] == "-S":
        print("build dependency transaction visible")
    else: fail("unexpected pacman arguments: " + repr(args))
elif name == "makepkg":
    if cfg.get("fail_build"): fail("simulated compiler failure")
    destination = pathlib.Path(os.environ["PKGDEST"])
    (destination / "omarchy-notes-1.0.0-1-x86_64.pkg.tar.zst").write_bytes(b"built package")
    (destination / "omarchy-notes-debug-1.0.0-1-x86_64.pkg.tar.zst").write_bytes(b"debug")
elif name == "omarchy-notes":
    if args == ["--version"]:
        if os.environ.get("QT_QPA_PLATFORM") != "offscreen": fail("not off-screen")
        if cfg.get("fail_verify"): fail("simulated Qt failure")
        print(cfg.get("app_version", "omarchy-notes 1.0.0"))
    elif args == ["--setup-hyprland"]:
        if cfg.get("quick_conflict"):
            print("Super + Alt + N is already bound, so nothing was changed.")
            sys.exit(1)
        print("Added Quick Note; backups saved.")
    else: fail("unexpected app arguments")
elif name == "omarchy":
    fail("installer must not run omarchy commands")
else: fail("unexpected shim " + name)
'''


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="notes-installer-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for directory in ("bin", "assets", "tmp", "home", "system/bin", "config/hypr", "data/omarchy-notes"):
            (self.root / directory).mkdir(parents=True)
        shim = self.root / "shim"
        shim.write_text(SHIM)
        shim.chmod(0o755)
        for name in ("curl", "sudo", "pacman", "uname", "makepkg", "omarchy"):
            (self.root / "bin" / name).symlink_to(shim)
        (self.root / "system/bin/omarchy-notes").symlink_to(shim)
        self.config = {}
        (self.root / "config/hypr/bindings.lua").write_text("-- sentinel binding\n")
        (self.root / "config/hypr/hyprland.lua").write_text("-- sentinel rule\n")
        (self.root / "data/omarchy-notes/library.sqlite3").write_bytes(b"untouched notes")
        (self.root / "assets" / PACKAGE).write_bytes(b"release package")
        (self.root / "assets" / PACKAGE.replace("omarchy-notes-", "omarchy-notes-debug-")).write_bytes(b"debug")
        self.archive("omarchy-notes-1.0.0-aur.tar.gz", {"aur/PKGBUILD": "pkgname=omarchy-notes\n", "aur/.SRCINFO": "pkgname = omarchy-notes\n"})
        self.archive("omarchy-notes-1.0.0.tar.gz", {"omarchy-notes-1.0.0/CMakeLists.txt": "source"})
        self.manifest()
        self.env = dict(os.environ, MOCK_ROOT=str(self.root), HOME=str(self.root / "home"),
                        XDG_CONFIG_HOME=str(self.root / "config"), XDG_DATA_HOME=str(self.root / "data"),
                        TMPDIR=str(self.root / "tmp"), PATH=str(self.root / "bin") + ":" + os.environ["PATH"],
                        TERM="xterm-256color", COLORTERM="truecolor", LC_ALL="C.UTF-8")
        self.env.pop("NO_COLOR", None)
        self.env.pop("COLORFGBG", None)
        self.env.pop("OMARCHY_NOTES_DATA_DIR", None)

    def archive(self, name, files):
        with tarfile.open(self.root / "assets" / name, "w:gz") as archive:
            for path, content in files.items():
                data = content.encode()
                info = tarfile.TarInfo(path)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))

    def manifest(self):
        assets = self.root / "assets"
        (assets / "SHA256SUMS").write_text("".join(
            f"{hashlib.sha256(path.read_bytes()).hexdigest()}  ./{path.name}\n"
            for path in sorted(assets.iterdir()) if path.name != "SHA256SUMS"))

    def prepare(self):
        (self.root / "config.json").write_text(json.dumps(self.config))

    def run_installer(self, *args, piped=False):
        self.prepare()
        command = ["bash", "-s", "--", *args] if piped else ["bash", str(INSTALLER), *args]
        result = subprocess.run(command, input=INSTALLER.read_text() if piped else "",
                                env=self.env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                start_new_session=True, timeout=20)
        self.output = result.stdout
        return result

    def calls(self, name=None):
        file = self.root / "calls.jsonl"
        values = [json.loads(line) for line in file.read_text().splitlines()] if file.exists() else []
        return [args for command, args in values if name is None or command == name]

    def mutations(self):
        return [args for args in self.calls("pacman") if args[0] in ("-S", "-U")]

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stdout)

    def assert_failure(self, result, message):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(message, result.stdout)

    def test_fresh_install_and_preservation(self):
        result = self.run_installer("--yes")
        self.assert_success(result)
        self.assertIn("Application verified", result.stdout)
        self.assertIn("pacman transaction visible", result.stdout)
        self.assertIn(str(self.root / "data/omarchy-notes"), result.stdout)
        self.assertEqual(len(self.mutations()), 1)
        self.assertIn("--noconfirm", self.mutations()[0])
        self.assertEqual(self.calls("omarchy-notes"), [["--version"]])
        self.assertEqual((self.root / "data/omarchy-notes/library.sqlite3").read_bytes(), b"untouched notes")
        self.assertEqual((self.root / "config/hypr/bindings.lua").read_text(), "-- sentinel binding\n")
        self.assertEqual(list((self.root / "tmp").iterdir()), [])
        self.assertNotIn("\x1b", result.stdout)
        urls = [args[-1] for args in self.calls("curl")]
        self.assertEqual(urls.count(REPO + "/releases/latest"), 1)
        self.assertTrue(all("/download/v1.0.0/" in url for url in urls[1:]))
        self.assertFalse(any("debug" in url for url in urls))

    def test_upgrade_and_already_installed(self):
        (self.root / "installed").write_text("0.9.0-1")
        self.assert_success(self.run_installer("--yes", "--version", "v1.0.0"))
        self.assertEqual(len(self.mutations()), 1)
        (self.root / "calls.jsonl").unlink()
        result = self.run_installer("--yes", "--version=v1.0.0")
        self.assert_success(result)
        self.assertIn("already installed", result.stdout)
        self.assertEqual(self.mutations(), [])
        self.assertEqual(len(self.calls("curl")), 1)

    def test_dry_run(self):
        for args in (("--dry-run",), ("--dry-run", "--source", "--quick-note")):
            self.assert_success(self.run_installer(*args))
        self.assertEqual(self.mutations(), [])
        self.assertEqual(self.calls("sudo"), [])
        self.assertEqual(self.calls("omarchy-notes"), [])
        self.assertEqual(self.calls("makepkg"), [])
        self.assertEqual(list((self.root / "tmp").iterdir()), [])

    def test_arguments_and_unattended_input(self):
        for args, message in (((), "No interactive terminal"),
                              (("--version",), "requires a release tag"),
                              (("--version=", "--yes"), "requires a release tag"),
                              (("--version", "../bad", "--yes"), "stable release tag"),
                              (("--quick-note", "--no-quick-note"), "Choose either"),
                              (("--wat",), "Unknown option")):
            self.assert_failure(self.run_installer(*args), message)
        self.assertEqual(self.calls(), [])
        self.assert_success(self.run_installer("--help"))

    def test_architecture_and_os(self):
        self.config["arch"] = "riscv64"
        self.assert_failure(self.run_installer("--yes"), "Unsupported architecture")
        self.config["arch"] = "aarch64"
        self.assert_failure(self.run_installer("--yes"), "No aarch64 application package")
        self.config["os"] = "Darwin"
        self.assert_failure(self.run_installer("--yes"), "supports Linux")
        self.assertEqual(self.mutations(), [])

    def test_missing_release_and_network(self):
        for asset in ("latest", "SHA256SUMS", PACKAGE):
            self.config["fail_asset"] = asset
            self.assert_failure(self.run_installer("--yes"), "Could not")
        self.assertEqual(self.mutations(), [])
        self.assertEqual(self.calls("makepkg"), [])

    def test_unsupported_release_redirect(self):
        self.config["latest_url"] = REPO + "/releases/tag/v1.1.0-beta"
        self.assert_failure(self.run_installer("--yes"), "No supported published release")
        self.assertEqual(self.mutations(), [])

    def test_bad_checksum_retains_log(self):
        self.config["corrupt_asset"] = PACKAGE
        result = self.run_installer("--yes")
        self.assert_failure(result, "Checksum failed")
        self.assertEqual(self.mutations(), [])
        logs = list((self.root / "tmp").glob("*.log"))
        self.assertEqual(len(logs), 1)
        self.assertIn(str(logs[0]), result.stdout)
        self.assertIn("Checksum failed", logs[0].read_text())
        self.assertEqual(list((self.root / "tmp").iterdir()), logs)

    def test_missing_ambiguous_and_malformed_manifest(self):
        sums = self.root / "assets/SHA256SUMS"
        original = sums.read_text()
        sums.write_text("\n".join(line for line in original.splitlines() if PACKAGE not in line) + "\n")
        self.assert_failure(self.run_installer("--yes"), "No x86_64 application package")
        sums.write_text(original + next(line for line in original.splitlines() if PACKAGE in line) + "\n")
        self.assert_failure(self.run_installer("--yes"), "Ambiguous release")
        sums.write_text("broken checksum\n")
        self.assert_failure(self.run_installer("--yes"), "Malformed release checksum")
        self.assertEqual(self.mutations(), [])

    def test_package_identity_and_architecture(self):
        for config in ({"identity": "other-package 1.0.0-1"}, {"package_arch": "aarch64"}):
            self.config = config
            self.assert_failure(self.run_installer("--yes"), "Unexpected package")
        self.assertEqual(self.mutations(), [])

    def test_denied_privilege_and_transaction(self):
        self.config["deny_sudo"] = True
        self.assert_failure(self.run_installer("--yes"), "pacman could not install")
        self.assertEqual(self.mutations(), [])
        self.assertEqual(self.calls("sudo")[0][0], "-n")
        self.config = {"deny_pacman": True}
        self.assert_failure(self.run_installer("--yes"), "pacman could not install")
        self.assertFalse((self.root / "installed").exists())

    def test_post_install_verification_failure(self):
        self.config["fail_verify"] = True
        self.assert_failure(self.run_installer("--yes"), "application verification failed")
        self.assertTrue((self.root / "installed").exists())
        self.assertEqual(self.calls("omarchy-notes"), [["--version"]])

    def test_quick_note_explicit_opt_in_and_conflict(self):
        self.config["quick_conflict"] = True
        result = self.run_installer("--yes", "--quick-note")
        self.assert_success(result)
        self.assertIn("Quick Note setup needs attention", result.stdout)
        self.assertIn("already bound", result.stdout)
        self.assertEqual(self.calls("omarchy-notes"), [["--version"], ["--setup-hyprland"]])
        logs = list((self.root / "tmp").glob("*.log"))
        self.assertEqual(len(logs), 1)
        self.assertIn("already bound", logs[0].read_text())

    def test_quick_note_skipped_and_incompatible(self):
        self.assert_success(self.run_installer("--yes", "--no-quick-note"))
        (self.root / "config/hypr/bindings.lua").unlink()
        result = self.run_installer("--yes", "--quick-note")
        self.assert_success(result)
        self.assertIn("requires compatible", result.stdout)
        self.assertFalse(any("--setup-hyprland" in args for args in self.calls("omarchy-notes")))

    def test_shadow_copy_and_data_override(self):
        # Different real path, rather than a symlink to the package-owned shim.
        (self.root / "bin/omarchy-notes").write_text("#!/bin/sh\nexit 99\n")
        (self.root / "bin/omarchy-notes").chmod(0o755)
        self.env["OMARCHY_NOTES_DATA_DIR"] = str(self.root / "custom-library")
        result = self.run_installer("--yes")
        self.assert_success(result)
        self.assertIn("takes precedence on PATH", result.stdout)
        self.assertIn(str(self.root / "custom-library"), result.stdout)
        self.assertTrue((self.root / "bin/omarchy-notes").exists())

    def test_source_build(self):
        result = self.run_installer("--yes", "--source")
        self.assert_success(result)
        self.assertEqual([args[0] for args in self.mutations()], ["-S", "-U"])
        self.assertEqual(len(self.calls("makepkg")), 1)
        self.assertFalse(any(PACKAGE == args[-1].rsplit("/", 1)[-1] for args in self.calls("curl")))

    def test_source_failures(self):
        self.config["corrupt_asset"] = "omarchy-notes-1.0.0-aur.tar.gz"
        self.assert_failure(self.run_installer("--yes", "--source"), "Checksum failed")
        self.assertEqual(self.mutations(), [])
        self.config = {"fail_build": True}
        self.assert_failure(self.run_installer("--yes", "--source"), "Source build failed")
        self.assertEqual([args[0] for args in self.mutations()], ["-S"])

    def test_unexpected_recipe_path(self):
        self.archive("omarchy-notes-1.0.0-aur.tar.gz", {"../escaped": "bad"})
        self.manifest()
        self.assert_failure(self.run_installer("--yes", "--source"), "Unexpected path")
        self.assertEqual(self.mutations(), [])

    def test_piped_execution_and_ascii(self):
        self.env["LC_ALL"] = "C"
        result = self.run_installer("--yes", piped=True)
        self.assert_success(result)
        result.stdout.encode("ascii")
        self.assertNotIn("\x1b", result.stdout)

    def run_pty(self, args, replies=(), interrupt=False, columns=100):
        self.prepare()
        pid, master = pty.fork()
        if pid == 0:
            fcntl.ioctl(1, termios.TIOCSWINSZ, struct.pack("HHHH", 40, columns, 0, 0))
            os.environ.update(self.env)
            os.execvpe("bash", ["bash", str(INSTALLER), *args], self.env)
        output = bytearray()
        deadline = time.monotonic() + 20
        replies = list(replies)
        interrupted = False
        status = None
        try:
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError:
                        break
                    if not chunk: break
                    output.extend(chunk)
                if replies and replies[0][0].encode() in output:
                    _, reply = replies.pop(0)
                    os.write(master, reply.encode() + b"\n")
                if interrupt and not interrupted and (self.root / "child.pid").exists():
                    os.kill(pid, signal.SIGTERM)
                    interrupted = True
                done, status = os.waitpid(pid, os.WNOHANG)
                if done:
                    pid = None
                    # Drain remaining terminal output, including cursor restoration.
                    while select.select([master], [], [], 0.1)[0]:
                        try: output.extend(os.read(master, 65536))
                        except OSError: break
                    break
            if pid:
                done, status = os.waitpid(pid, os.WNOHANG)
                if not done:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
                    self.fail("PTY installer timed out: " + output.decode(errors="replace"))
                pid = None
        finally:
            os.close(master)
            if pid:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
        return os.waitstatus_to_exitcode(status), output.decode(errors="replace")

    def test_interactive_prompts_and_narrow_terminal(self):
        code, output = self.run_pty([], (("[Y/n]", "y"), ("[y/N]", "n")))
        self.assertEqual(code, 0, output)
        self.assertIn("\x1b[38;2;126;156;216m", output)
        self.assertIn("\x1b[?25h", output)
        self.assertFalse(any("--setup-hyprland" in args for args in self.calls("omarchy-notes")))
        self.assertNotIn("--noconfirm", self.mutations()[0])
        self.assertNotEqual(self.calls("sudo")[0][0], "-n")
        code, output = self.run_pty(["--yes"], columns=40)
        self.assertEqual(code, 0, output)
        self.assertNotIn("\x1b[?25l", output)

    def test_interactive_cancel_and_plain_terminal(self):
        self.env["NO_COLOR"] = ""
        code, output = self.run_pty([], (("[Y/n]", "n"),))
        self.assertEqual(code, 0, output)
        self.assertIn("cancelled", output)
        self.assertNotIn("\x1b", output)
        self.assertEqual(self.mutations(), [])
        self.env.pop("NO_COLOR")
        self.env["TERM"] = "dumb"
        code, output = self.run_pty(["--yes"])
        self.assertEqual(code, 0, output)
        self.assertNotIn("\x1b", output)

    def test_light_terminal_and_ansi_fallback(self):
        self.env["COLORFGBG"] = "0;15"
        code, output = self.run_pty(["--yes"])
        self.assertEqual(code, 0, output)
        self.assertIn("\x1b[38;2;45;79;142m", output)
        self.env.pop("COLORTERM")
        code, output = self.run_pty(["--yes"])
        self.assertEqual(code, 0, output)
        self.assertIn("\x1b[34m", output)
        self.assertNotIn("\x1b[38;2;", output)

    def test_interrupt_stops_children_and_restores_cursor(self):
        self.config["slow_asset"] = PACKAGE
        code, output = self.run_pty(["--yes"], interrupt=True)
        self.assertEqual(code, 143, output)
        self.assertIn("Interrupted", output)
        self.assertIn("\x1b[?25h", output)
        for file in ("worker.pid", "child.pid"):
            pid = int((self.root / file).read_text())
            stat = Path(f"/proc/{pid}/stat")
            self.assertTrue(not stat.exists() or stat.read_text().split()[2] == "Z", f"process {pid} survived cleanup")
        self.assertEqual(self.mutations(), [])
        self.assertTrue(list((self.root / "tmp").glob("*.log")))


if __name__ == "__main__":
    unittest.main(verbosity=2)
