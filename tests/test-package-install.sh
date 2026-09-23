#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT
version="$(sed -n '/^project(omarchy-notes/,/)/s/^ *VERSION \([0-9.]*\).*/\1/p' "$project_dir/CMakeLists.txt")"
[[ -n "$version" ]]

cmake -S "$project_dir" -B "$work_dir/build" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr -DCMAKE_INSTALL_LIBDIR=lib -DBUILD_TESTING=OFF
cmake --build "$work_dir/build"
DESTDIR="$work_dir/root" cmake --install "$work_dir/build"

required=(
  usr/bin/omarchy-notes
  usr/share/applications/org.omarchy.Notes.desktop
  usr/share/icons/hicolor/scalable/apps/org.omarchy.Notes.svg
  usr/share/metainfo/org.omarchy.Notes.metainfo.xml
  usr/share/man/man1/omarchy-notes.1
  usr/share/doc/omarchy-notes/README.md
  usr/share/doc/omarchy-notes/CHANGELOG.md
  usr/share/doc/omarchy-notes/SECURITY.md
  usr/share/licenses/omarchy-notes/LICENSE
)
for path in "${required[@]}"; do
  [[ -f "$work_dir/root/$path" ]] || { echo "missing installed file: $path" >&2; exit 1; }
done

[[ "$(env -u QT_QPA_PLATFORMTHEME QT_QPA_PLATFORM=offscreen \
  "$work_dir/root/usr/bin/omarchy-notes" --version)" == "omarchy-notes $version" ]]
cmp "$project_dir/LICENSE" "$work_dir/root/usr/share/licenses/omarchy-notes/LICENSE"
if readelf -d "$work_dir/root/usr/bin/omarchy-notes" | grep -Eq '\((RPATH|RUNPATH)\)'; then
  echo 'unexpected runtime library path' >&2
  exit 1
fi

if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$work_dir/root/usr/share/applications/org.omarchy.Notes.desktop"
fi
if command -v appstreamcli >/dev/null; then
  appstreamcli validate --no-net "$work_dir/root/usr/share/metainfo/org.omarchy.Notes.metainfo.xml"
fi

echo "package install contract: ok"
