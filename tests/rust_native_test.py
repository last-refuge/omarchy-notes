"""Cleanup for a native test owner's exact private IPC socket after it exits."""
import hashlib
import os
from pathlib import Path
import stat
import tempfile


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
