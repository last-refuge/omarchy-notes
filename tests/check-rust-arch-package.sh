#!/usr/bin/env bash
# Read-only validation. The install/coexistence matrix belongs in disposable CI.
set -euo pipefail
package_dir="$(realpath "${1:?usage: check-rust-arch-package.sh PACKAGE_DIRECTORY}")"
shopt -s nullglob
packages=("$package_dir"/omarchy-notes-[0-9]*-x86_64.pkg.tar.zst)
[[ ${#packages[@]} == 1 ]] || { echo 'Expected one Rust application package' >&2; exit 1; }
package=${packages[0]}
identity=$(pacman -Qp "$package")
[[ $identity == omarchy-notes\ * ]] || { echo 'Wrong Rust package identity' >&2; exit 1; }
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
namcap "$package_dir/PKGBUILD" "$package" | tee "$work_dir/namcap.log"
if rg -q ' E: |^Error:|^Traceback ' "$work_dir/namcap.log"; then echo 'namcap reported Rust packaging errors' >&2; exit 1; fi
bsdtar -xf "$package" -C "$work_dir"
for path in usr/bin/omarchy-notes-rust usr/bin/omarchy-notes-preview \
  usr/share/licenses/omarchy-notes/LICENSE \
  usr/share/licenses/omarchy-notes/ASSET-NOTICES.md \
  usr/share/licenses/omarchy-notes/GPUI-LICENSE \
  usr/share/licenses/omarchy-notes/FONT-OFL.txt \
  usr/share/licenses/omarchy-notes/ACCESSKIT-LICENSE-APACHE \
  usr/share/licenses/omarchy-notes/ACCESSKIT-LICENSE-MIT \
  usr/share/licenses/omarchy-notes/dependencies/inventory.json \
  usr/share/applications/org.omarchy.Notes.Rust.desktop \
  usr/share/icons/hicolor/scalable/apps/org.omarchy.Notes.Rust.svg \
  usr/share/metainfo/org.omarchy.Notes.Rust.metainfo.xml \
  usr/share/man/man1/omarchy-notes-rust.1.gz \
  usr/share/doc/omarchy-notes/SOURCE_REVISION \
  usr/share/doc/omarchy-notes/RELEASE_KIND; do
  [[ -s $work_dir/$path ]] || { echo "Missing Rust package file: $path" >&2; exit 1; }
done
[[ ! -e $work_dir/usr/bin/omarchy-notes && ! -e $work_dir/usr/share/applications/org.omarchy.Notes.desktop ]]
if readelf -d "$work_dir/usr/bin/omarchy-notes-rust" | rg -q '\((RPATH|RUNPATH)\)'; then
  echo 'Unexpected Rust runtime library path' >&2; exit 1
fi
rg -q '^exec /usr/bin/omarchy-notes-rust "\$@"$' "$work_dir/usr/bin/omarchy-notes-preview"
desktop-file-validate "$work_dir/usr/share/applications/org.omarchy.Notes.Rust.desktop"
appstreamcli validate --no-net "$work_dir/usr/share/metainfo/org.omarchy.Notes.Rust.metainfo.xml"
python3 - "$work_dir/usr/share/doc/omarchy-notes" "$work_dir/usr/share/licenses/omarchy-notes/dependencies/inventory.json" <<'PY'
import json, re, sys
from pathlib import Path
doc = Path(sys.argv[1])
kind = (doc / "RELEASE_KIND").read_text().strip()
if kind not in ("candidate", "stable"):
    raise SystemExit("Unknown Rust package release kind")
revision = (doc / "SOURCE_REVISION").read_text().strip()
if not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", revision):
    raise SystemExit("Invalid immutable Rust source revision")
inventory = json.loads(Path(sys.argv[2]).read_text())
missing = inventory.get("missing")
if not isinstance(missing, list):
    raise SystemExit("No explicit dependency notice collection result")
if kind == "stable" and missing:
    raise SystemExit("Stable package has missing dependency license texts")
if missing:
    print(f"Candidate retains {len(missing)} explicitly recorded source-text gaps; license acceptance remains open")
PY
echo 'Rust package validation: ok'
