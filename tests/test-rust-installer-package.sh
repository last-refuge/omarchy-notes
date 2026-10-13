#!/usr/bin/env bash
# Real package mutations are permitted only in CI's disposable Arch container.
set -euo pipefail
[[ ${CI:-} == true && -f /.dockerenv && $EUID != 0 ]] || {
  echo 'Run this test as the build user in the disposable CI container.' >&2; exit 1;
}
if pacman -Q omarchy-notes-rust >/dev/null 2>&1; then echo 'Expected a fresh Rust installation.' >&2; exit 1; fi
project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
package_dir="$(realpath "${1:?usage: test-rust-installer-package.sh PACKAGE_DIRECTORY}")"
shopt -s nullglob
packages=("$package_dir"/omarchy-notes-rust-[0-9]*-x86_64.pkg.tar.zst)
[[ ${#packages[@]} == 1 ]]
identity=$(pacman -Qp "${packages[0]}")
[[ $identity == omarchy-notes-rust\ * ]]
version=${identity#omarchy-notes-rust }; version=${version%-*}
test_workspace=$(mktemp -d)
trap 'rm -rf "$test_workspace"' EXIT
mkdir -p "$test_workspace/bin" "$test_workspace/assets" "$test_workspace/data/omarchy-notes" "$test_workspace/config" "$test_workspace/runtime"
chmod 700 "$test_workspace/runtime"
cp "${packages[0]}" "$test_workspace/assets/"
(cd "$test_workspace/assets" && sha256sum ./*.pkg.tar.zst > SHA256SUMS)
cat >"$test_workspace/bin/curl" <<'CURL'
#!/usr/bin/env bash
set -euo pipefail
output=''; url=''
while (( $# )); do
  case "$1" in
    --output) output=$2; shift ;;
    https://github.com/last-refuge/omarchy-notes/releases/*) url=$1 ;;
  esac
  shift
done
[[ $url == "https://github.com/last-refuge/omarchy-notes/releases/download/$NOTES_INSTALLER_TEST_TAG/"* ]]
cp -- "$NOTES_INSTALLER_TEST_ASSETS/${url##*/}" "$output"
CURL
chmod +x "$test_workspace/bin/curl"
export NOTES_INSTALLER_TEST_ASSETS="$test_workspace/assets" NOTES_INSTALLER_TEST_TAG="rust-v$version"
export XDG_DATA_HOME="$test_workspace/data" XDG_CONFIG_HOME="$test_workspace/config" XDG_RUNTIME_DIR="$test_workspace/runtime"
export PATH="$test_workspace/bin:$PATH"
unset DISPLAY WAYLAND_DISPLAY
printf 'Qt library retained\n' > "$XDG_DATA_HOME/omarchy-notes/sentinel"
bash "$project_dir/install.sh" --version "$NOTES_INSTALLER_TEST_TAG" --dry-run
! pacman -Q omarchy-notes-rust >/dev/null 2>&1
bash "$project_dir/install.sh" --version "$NOTES_INSTALLER_TEST_TAG" --yes --no-quick-note
[[ $(pacman -Q omarchy-notes-rust) == "$identity" ]]
[[ $(/usr/bin/omarchy-notes-rust --version) == "omarchy-notes-rust $version" ]]
[[ $(/usr/bin/omarchy-notes-preview --version) == "omarchy-notes-rust $version" ]]
request=01234567-89ab-4cde-8fab-0123456789ab
/usr/bin/omarchy-notes-preview --capture 'Rust package capture without a display' --request-id "$request"
/usr/bin/omarchy-notes-rust --capture-status "$request"
test -s "$XDG_DATA_HOME/omarchy-notes-gpui/library.sqlite3"
before=$(sha256sum "$XDG_DATA_HOME/omarchy-notes-gpui/library.sqlite3")
bash "$project_dir/install.sh" --version "$NOTES_INSTALLER_TEST_TAG" --yes --no-quick-note > "$test_workspace/repeat.log"
rg -q 'already installed' "$test_workspace/repeat.log"
[[ $(cat "$XDG_DATA_HOME/omarchy-notes/sentinel") == 'Qt library retained' ]]
sudo -n pacman -R --noconfirm omarchy-notes-rust
[[ $(sha256sum "$XDG_DATA_HOME/omarchy-notes-gpui/library.sqlite3") == "$before" ]]
[[ $(cat "$XDG_DATA_HOME/omarchy-notes/sentinel") == 'Qt library retained' ]]
echo 'Rust disposable install/repeat/alias/headless capture/removal contract: ok'
