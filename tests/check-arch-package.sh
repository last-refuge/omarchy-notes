#!/usr/bin/env bash
set -euo pipefail

package_dir="$(realpath "${1:?usage: check-arch-package.sh PACKAGE_DIRECTORY}")"
shopt -s nullglob
packages=("$package_dir"/omarchy-notes-[0-9]*-*.pkg.tar.zst)
[[ ${#packages[@]} == 1 ]] || { echo 'Expected one application package' >&2; exit 1; }
package="${packages[0]}"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

# namcap can report errors with exit status zero; inspect its diagnostics as well.
namcap "$package_dir/PKGBUILD" "$package" | tee "$work_dir/namcap.log"
if grep -q ' E: ' "$work_dir/namcap.log"; then
  echo 'namcap reported packaging errors' >&2
  exit 1
fi
bsdtar -xf "$package" -C "$work_dir"
for path in usr/bin/omarchy-notes \
            usr/share/licenses/omarchy-notes/LICENSE \
            usr/share/applications/org.omarchy.Notes.desktop \
            usr/share/icons/hicolor/scalable/apps/org.omarchy.Notes.svg \
            usr/share/metainfo/org.omarchy.Notes.metainfo.xml \
            usr/share/man/man1/omarchy-notes.1.gz \
            usr/share/doc/omarchy-notes/README.md; do
  [[ -s "$work_dir/$path" ]] || { echo "Missing package file: $path" >&2; exit 1; }
done
if readelf -d "$work_dir/usr/bin/omarchy-notes" | grep -Eq '\((RPATH|RUNPATH)\)'; then
  echo 'Unexpected runtime library path in omarchy-notes' >&2
  exit 1
fi
grep -q '^Exec=/usr/bin/omarchy-notes$' "$work_dir/usr/share/applications/org.omarchy.Notes.desktop"
echo 'Arch package validation: ok'
