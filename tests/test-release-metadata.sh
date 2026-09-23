#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
version="$(sed -n '/^project(omarchy-notes/,/)/s/^ *VERSION \([0-9.]*\).*/\1/p' CMakeLists.txt)"
[[ -n "$version" ]]

grep -q "^pkgver=$version$" packaging/arch/PKGBUILD.in
grep -q "release version=\"$version\"" packaging/metainfo/org.omarchy.Notes.metainfo.xml
grep -q "omarchy-notes $version" packaging/man/omarchy-notes.1
grep -q "## \[$version\]" CHANGELOG.md
grep -q 'set(ONOTES_APP_ID "org.omarchy.Notes"' CMakeLists.txt
grep -q 'https://lastrefuge.ai/projects/omarchy-notes' packaging/metainfo/org.omarchy.Notes.metainfo.xml
grep -q 'GitHub Security Advisories' SECURITY.md
appstreamcli validate --no-net packaging/metainfo/org.omarchy.Notes.metainfo.xml >/dev/null
if [[ -n "${GITHUB_REF_NAME:-}" && "${GITHUB_REF_TYPE:-}" == "tag" ]]; then
  [[ "$GITHUB_REF_NAME" == "v$version" ]]
fi

echo "release metadata $version: ok"
