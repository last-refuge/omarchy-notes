#!/usr/bin/env bash
# Run only in a disposable Arch container. Never install test packages on the host.
set -euo pipefail

if (( EUID != 0 )) || [[ ! -f /.dockerenv && ! -f /run/omarchy-notes-package-test ]]; then
  echo 'Package lifecycle checks require a disposable container running as root.' >&2
  exit 1
fi
if pacman -Q omarchy-notes &>/dev/null; then
  echo 'Refusing to replace an existing Omarchy Notes installation.' >&2
  exit 1
fi

package_dir="$(realpath "${1:?usage: test-package-lifecycle.sh PACKAGE_DIRECTORY [PREVIOUS_PACKAGE]}")"
shopt -s nullglob
packages=("$package_dir"/omarchy-notes-[0-9]*-*.pkg.tar.zst)
[[ ${#packages[@]} == 1 ]] || { echo 'Expected one application package' >&2; exit 1; }
package="${packages[0]}"
work_dir="$(mktemp -d)"
installed=false
cleanup() {
  if [[ "$installed" == true ]]; then
    pacman -R --noconfirm omarchy-notes >/dev/null || true
  fi
  rm -rf "$work_dir"
}
trap cleanup EXIT

# The last public application release is a fixed upgrade baseline, not a moving URL.
if [[ -n "${2:-}" ]]; then
  previous="$(realpath "$2")"
else
  previous="$work_dir/omarchy-notes-1.0.0-1-x86_64.pkg.tar.zst"
  curl -fsSL https://github.com/last-refuge/omarchy-notes/releases/download/v1.0.0/SHA256SUMS \
    -o "$work_dir/SHA256SUMS"
  curl -fsSL "https://github.com/last-refuge/omarchy-notes/releases/download/v1.0.0/$(basename "$previous")" \
    -o "$previous"
  checksum="$(awk -v name="$(basename "$previous")" '$2 == name || $2 == "./" name {print $1}' "$work_dir/SHA256SUMS")"
  [[ "$checksum" =~ ^[a-f0-9]{64}$ ]]
  printf '%s  %s\n' "$checksum" "$previous" | sha256sum --check
fi
previous_version="$(pacman -Qp "$previous" | awk '{print $2}')"
new_version="$(pacman -Qp "$package" | awk '{print $2}')"
[[ "$(vercmp "$new_version" "$previous_version")" == 1 ]] || {
  echo "Expected an upgrade from $previous_version to $new_version" >&2
  exit 1
}

id builder >/dev/null
mkdir -p "$work_dir/runtime" "$work_dir/config" "$work_dir/data" "$work_dir/cache"
chmod 700 "$work_dir/runtime"
chown -R builder:builder "$work_dir"
library="$work_dir/data/omarchy-notes"

launch() {
  runuser -u builder -- env -u QT_QPA_PLATFORMTHEME -u OMARCHY_NOTES_DATA_DIR \
    QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
    XDG_RUNTIME_DIR="$work_dir/runtime" XDG_CONFIG_HOME="$work_dir/config" \
    XDG_DATA_HOME="$work_dir/data" XDG_CACHE_HOME="$work_dir/cache" \
    ONOTES_EXIT_AFTER_FIRST_FRAME=1 timeout 30 /usr/bin/omarchy-notes "$@"
}

snapshot() {
  python3 - "$library" <<'PY'
import hashlib
import json
from pathlib import Path
import sqlite3
import sys

root = Path(sys.argv[1])
with sqlite3.connect(f"file:{root / 'library.sqlite3'}?mode=ro", uri=True) as db:
    assert db.execute('PRAGMA integrity_check').fetchone() == ('ok',)
    assert not db.execute('PRAGMA foreign_key_check').fetchall()
    notes = db.execute('SELECT id, title, body, body_schema FROM notes ORDER BY id').fetchall()
    assert any(title == 'Package upgrade check' for _, title, _, _ in notes)
    attachments = db.execute('SELECT id, note_id, blob_hash, file_name FROM attachments ORDER BY id').fetchall()
    assert attachments, 'Sample library must include attachments'
    blobs = {}
    for digest, size in db.execute('SELECT hash, size FROM blobs ORDER BY hash'):
        data = (root / 'blobs' / digest[:2] / digest).read_bytes()
        assert len(data) == size
        assert hashlib.sha256(data).hexdigest() == digest
        blobs[digest] = size
print(json.dumps({'notes': notes, 'attachments': attachments, 'blobs': blobs}, sort_keys=True))
PY
}

pacman -U --noconfirm "$previous"
installed=true
launch --samples > "$work_dir/first-launch.log"
grep -q 'first-frame' "$work_dir/first-launch.log"
launch --capture $'Package upgrade check\nKeep this note and its attachments.'
snapshot > "$work_dir/before.json"

pacman -U --noconfirm "$package"
[[ "$(pacman -Q omarchy-notes)" == "omarchy-notes $new_version" ]]
launch > "$work_dir/upgraded-launch.log"
grep -q 'first-frame' "$work_dir/upgraded-launch.log"
snapshot > "$work_dir/after.json"
cmp "$work_dir/before.json" "$work_dir/after.json"

pacman -R --noconfirm omarchy-notes
installed=false
[[ ! -e /usr/bin/omarchy-notes && ! -e /usr/share/applications/org.omarchy.Notes.desktop ]]
snapshot > "$work_dir/removed.json"
cmp "$work_dir/before.json" "$work_dir/removed.json"

pacman -U --noconfirm "$package"
installed=true
launch > "$work_dir/reinstalled-launch.log"
grep -q 'first-frame' "$work_dir/reinstalled-launch.log"
snapshot > "$work_dir/reinstalled.json"
cmp "$work_dir/before.json" "$work_dir/reinstalled.json"
echo "Package lifecycle $previous_version → $new_version: ok (notes and attachments preserved)"
