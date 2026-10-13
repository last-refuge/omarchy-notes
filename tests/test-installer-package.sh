#!/usr/bin/env bash
# Real pacman installation, restricted to CI's disposable Arch container.
set -euo pipefail
[[ ${CI:-} == true && -f /.dockerenv && $EUID != 0 ]] || {
  echo 'Run this test as the build user in the disposable CI container.' >&2
  exit 1
}
if pacman -Q omarchy-notes >/dev/null 2>&1; then
  echo 'Expected a fresh container without Omarchy Notes installed.' >&2
  exit 1
fi
project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
package_dir="$(realpath "${1:?usage: test-installer-package.sh PACKAGE_DIRECTORY}")"
shopt -s nullglob
packages=("$package_dir"/omarchy-notes-[0-9]*-*.pkg.tar.zst)
[[ ${#packages[@]} == 1 ]]
identity=$(pacman -Qp "${packages[0]}")
version=${identity#omarchy-notes }; version=${version%-*}
test_workspace=$(mktemp -d)
trap 'rm -rf "$test_workspace"' EXIT
mkdir -p "$test_workspace/bin" "$test_workspace/assets" "$test_workspace/data" "$test_workspace/config"
cp "${packages[0]}" "$test_workspace/assets/"
(cd "$test_workspace/assets" && sha256sum ./*.pkg.tar.zst >SHA256SUMS)

# Only transport is replaced; identity checks, pacman, and the app are real.
cat >"$test_workspace/bin/curl" <<'CURL'
#!/usr/bin/env bash
set -euo pipefail
output=''; url=''; latest=0
while (( $# )); do
  case "$1" in
    --output) output=$2; shift ;;
    --write-out) latest=1; shift ;;
    https://github.com/last-refuge/omarchy-notes/releases/*|https://api.github.com/repos/last-refuge/omarchy-notes/releases*) url=$1 ;;
  esac
  shift
done
if [[ $url == https://api.github.com/* ]]; then
  printf '[{"tag_name":"%s","published_at":"2026-10-13T00:00:00Z","draft":false,"prerelease":false}]' "$NOTES_INSTALLER_TEST_TAG" >"$output"
elif (( latest )); then
  printf 'https://github.com/last-refuge/omarchy-notes/releases/tag/%s' "$NOTES_INSTALLER_TEST_TAG"
else
  [[ $url == "https://github.com/last-refuge/omarchy-notes/releases/download/$NOTES_INSTALLER_TEST_TAG/"* ]]
  cp -- "$NOTES_INSTALLER_TEST_ASSETS/${url##*/}" "$output"
fi
CURL
chmod +x "$test_workspace/bin/curl"
export NOTES_INSTALLER_TEST_ASSETS="$test_workspace/assets"
export NOTES_INSTALLER_TEST_TAG="v$version"
export XDG_DATA_HOME="$test_workspace/data" XDG_CONFIG_HOME="$test_workspace/config"
export PATH="$test_workspace/bin:$PATH"
printf 'keep my notes\n' >"$XDG_DATA_HOME/sentinel"
bash "$project_dir/install.sh" --dry-run
if pacman -Q omarchy-notes >/dev/null 2>&1; then
  echo 'Dry run unexpectedly installed the package.' >&2
  exit 1
fi
bash "$project_dir/install.sh" --yes --no-quick-note
[[ $(pacman -Q omarchy-notes) == "$identity" ]]
[[ $(env -u QT_QPA_PLATFORMTHEME QT_QPA_PLATFORM=offscreen /usr/bin/omarchy-notes --version) == "omarchy-notes $version" ]]
test -s /usr/share/applications/org.omarchy.Notes.desktop
bash "$project_dir/install.sh" --yes --no-quick-note >"$test_workspace/repeat.log"
cat "$test_workspace/repeat.log"
rg -q 'already installed' "$test_workspace/repeat.log"
[[ $(cat "$XDG_DATA_HOME/sentinel") == 'keep my notes' ]]
echo 'real installer package contract: ok'
