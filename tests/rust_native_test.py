"""Cleanup for a native test owner's exact private IPC socket after it exits."""
import hashlib
import json
import os
from pathlib import Path
import socket
import stat
import struct
import subprocess
import tempfile


def process_start_ticks(pid):
    """Distinguish an owned process from a later reuse of its PID."""
    return int((Path('/proc') / str(pid) / 'stat').read_text().rsplit(')', 1)[1].split()[19])


_nested_contract = None


def guard_nested_desktop():
    """Optional extra guard; the calling driver still checks its exact inner window."""
    if not __debug__:
        raise RuntimeError('Native ownership guards require Python assertions')
    path = os.environ.get('NOTES_NATIVE_QA_NESTED_GUARD')
    if not path:
        return
    global _nested_contract
    if _nested_contract is None or _nested_contract[0] != path:
        metadata = Path(path).lstat()
        assert stat.S_ISREG(metadata.st_mode) and metadata.st_uid == os.getuid()
        assert metadata.st_mode & 0o077 == 0, 'Nested guard receipt is not private'
        contract = json.loads(Path(path).read_text())
        assert contract['schema'] == 1 and contract['uid'] == os.getuid()
        _nested_contract = path, contract
    contract = _nested_contract[1]
    runtime = Path(os.environ['XDG_RUNTIME_DIR']).resolve(strict=True)
    assert runtime == Path(contract['runtime']).resolve(strict=True), 'Private runtime changed'
    assert os.environ['HYPRLAND_INSTANCE_SIGNATURE'] == contract['instance'], 'Private compositor identity changed'
    private = Path(os.environ['WAYLAND_DISPLAY'])
    if not private.is_absolute():
        private = runtime / private
    private = private.resolve(strict=True)
    host = Path(contract['host_socket']).resolve(strict=True)
    assert private == Path(contract['private_socket']) and private.is_relative_to(runtime)
    assert private != host and not os.path.samefile(private, host), 'Private socket became host socket'
    assert [private.stat().st_dev, private.stat().st_ino] == contract['private_inode'], 'Private socket replaced'
    assert stat.S_ISSOCK(private.stat().st_mode)
    assert process_start_ticks(contract['nested_pid']) == contract['nested_start_ticks'], 'Owned compositor PID changed'
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(2)
        client.connect(str(private))
        pid, uid, _ = struct.unpack('3i', client.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
    assert pid == contract['nested_pid'] and uid == contract['uid'], 'Private peer is not the owned compositor'
    host_env = dict(os.environ, XDG_RUNTIME_DIR=contract['host_runtime'],
                    HYPRLAND_INSTANCE_SIGNATURE=contract['host_instance'], WAYLAND_DISPLAY=str(host))
    clients = json.loads(subprocess.check_output(['hyprctl', 'clients', '-j'], env=host_env, text=True, timeout=3))
    parent = next((c for c in clients if c.get('pid') == pid and c.get('address') == contract['outer_address']), None)
    assert parent and parent.get('mapped', False) and not parent.get('hidden', False), 'Owned nested parent disappeared'
    assert parent['workspace']['id'] == contract['outer_workspace'], 'Owned nested parent workspace changed'


class InstanceSocket:
    def __init__(self, library, process):
        self.process = process
        runtime = Path(os.environ.get("XDG_RUNTIME_DIR", tempfile.gettempdir() + f"/omarchy-notes-runtime-{os.geteuid()}"))
        key = hashlib.sha256(os.fsencode(Path(library).resolve())).hexdigest()[:32]
        self.path = runtime / "omarchy-notes-rust" / (key + ".sock")
        metadata = self.path.lstat()
        assert stat.S_ISSOCK(metadata.st_mode) and metadata.st_uid == os.geteuid()
        self.identity = (metadata.st_dev, metadata.st_ino)

    def cleanup(self):
        # SIGTERM/SIGKILL cannot run Rust destructors. Never remove a live
        # owner's endpoint, or a replacement file/socket with another inode.
        assert self.process.poll() is not None
        try:
            metadata = self.path.lstat()
            if stat.S_ISSOCK(metadata.st_mode) and (metadata.st_dev, metadata.st_ino) == self.identity:
                self.path.unlink()
        except FileNotFoundError:
            pass
