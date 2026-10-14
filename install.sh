#!/usr/bin/env bash
# Omarchy Notes installer. Run as your normal user; pacman handles system files.
# Published releases carry this script as install.sh, alongside SHA256SUMS.
set -Eeuo pipefail

usage() {
  cat <<'USAGE'
Omarchy Notes installer. A place for your thoughts.

  bash install.sh [options]

Installs a verified release package on Omarchy or Arch Linux.
Run again to update. Your notes and running application are kept.

  --version TAG       select a published release (for example rust-v1.0.0)
  --source            build the selected release with makepkg as your user
  --yes, -y           accept package installation without confirmation
  --dry-run           inspect the release and show the plan without installing
  --quick-note        show manual Quick Note setup instructions
  --no-quick-note     skip Quick Note setup
  --help, -h          show this help

Without a terminal, installation requires --yes and noninteractive sudo access.
--yes alone never enables Quick Note. NO_COLOR disables color and animation.
Other Linux systems: see the README for building with Cargo.
USAGE
}

presentation() {
  ACCENT=''; GREEN=''; BOLD=''; DIM=''; RESET=''; ANIMATE=0
  GOOD='+'; BAD='x'; BULLET='-'; FRAMES='|\-/'
  local charmap
  charmap=$(locale charmap 2>/dev/null || true)
  if [[ ${charmap^^} == UTF-8 || ${charmap^^} == UTF8 ]]; then
    GOOD='✔'; BAD='✘'; BULLET='·'; FRAMES='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
  fi
  if [[ -t 1 && ${TERM:-dumb} != dumb && ! ${NO_COLOR+x} ]]; then
    local columns
    columns=$(tput cols 2>/dev/null || printf '80')
    # Long animated lines wrap in small terminals and cannot be cleared reliably.
    if [[ $columns =~ ^[0-9]+$ ]] && (( columns >= 80 )); then ANIMATE=1; fi
    ACCENT=$'\033[34m'; GREEN=$'\033[32m'
    if [[ ${COLORTERM:-} == *truecolor* || ${COLORTERM:-} == *24bit* ]]; then
      ACCENT=$'\033[38;2;126;156;216m'; GREEN=$'\033[38;2;152;187;108m'
      # Terminals exposing a light background use the website's darker accent.
      local background=${COLORFGBG:-}
      case ${background##*;} in
        7|15) ACCENT=$'\033[38;2;45;79;142m'; GREEN=$'\033[38;2;61;105;71m' ;;
      esac
    fi
    BOLD=$'\033[1m'; DIM=$'\033[2m'; RESET=$'\033[0m'
  fi
}

ok() { printf '  %s%s%s  %s\n' "$GREEN" "$GOOD" "$RESET" "$*"; }
note() { printf '  %s%s  %s%s\n' "$DIM" "$BULLET" "$*" "$RESET"; }
warn() { printf '  %s!%s  %s\n' "$ACCENT" "$RESET" "$*"; }
die() {
  printf '\n  %s%s%s  %s\n' "$ACCENT" "$BAD" "$RESET" "$1" >&2
  if [[ -n ${LOG:-} && -f $LOG ]]; then
    if [[ -s $LOG ]]; then tail -n 12 "$LOG" | sed 's/^/     /' >&2; fi
    printf '%s\n' "$1" >>"$LOG"
    printf '     Full log: %s\n' "$LOG" >&2
  fi
  printf '\n' >&2
  exit "${2:-1}"
}

cleanup() {
  local status=$?
  trap - ERR INT TERM
  if [[ -n ${ACTIVE_PID:-} ]]; then
    # Each quiet step has its own process group, including build subprocesses.
    kill -TERM -- "-$ACTIVE_PID" 2>/dev/null || true
    kill -TERM "$ACTIVE_PID" 2>/dev/null || true
    sleep 0.2
    kill -KILL -- "-$ACTIVE_PID" 2>/dev/null || true
    wait "$ACTIVE_PID" 2>/dev/null || true
  fi
  if (( ANIMATE )); then printf '\033[?25h'; fi
  if [[ -n ${WORK:-} ]]; then rm -rf -- "$WORK"; fi
  if (( status == 0 && ! RETAIN_LOG )) && [[ -n ${LOG:-} ]]; then rm -f -- "$LOG"; fi
  return "$status"
}

step() {
  local message=$1 status=0 frame=0
  shift
  if (( ! ANIMATE )); then note "$message"; fi
  setsid --wait "$@" </dev/null >>"$LOG" 2>&1 &
  ACTIVE_PID=$!
  if (( ANIMATE )); then
    printf '\033[?25l'
    while kill -0 "$ACTIVE_PID" 2>/dev/null; do
      printf '\r\033[K  %s%s%s  %s' "$ACCENT" "${FRAMES:frame++%${#FRAMES}:1}" "$RESET" "$message"
      sleep 0.1
    done
    printf '\r\033[K\033[?25h'
  fi
  wait "$ACTIVE_PID" || status=$?
  ACTIVE_PID=''
  if (( status == 0 )); then ok "$message"; fi
  return "$status"
}

confirm() {
  local message=$1 default=$2 reply=''
  (( YES )) && return 0
  (( HAVE_TTY )) || die 'No interactive terminal. Use --yes to install unattended.'
  printf '  ?  %s [%s] ' "$message" "$default" >/dev/tty
  read -r reply </dev/tty || return 1
  [[ $reply == [yY] || $reply == [yY][eE][sS] || ( -z $reply && $default == Y/n ) ]]
}

privileged() {
  # The caller intentionally opens the user's terminal before sudo runs.
  # shellcheck disable=SC2024
  if (( HAVE_TTY )); then sudo "$@" </dev/tty
  else sudo -n "$@" </dev/null; fi
}

read_manifest() {
  local digest name extra
  ASSET_NAMES=(); ASSET_HASHES=()
  while read -r digest name extra || [[ -n $digest ]]; do
    [[ -n $digest ]] || continue
    [[ $digest =~ ^[[:xdigit:]]{64}$ && -n $name && -z $extra ]] || die 'Malformed release checksum manifest.'
    name=${name#\*}; name=${name#./}
    [[ $name =~ ^[A-Za-z0-9._-]+$ ]] || die 'Unsupported filename in release checksum manifest.'
    ASSET_NAMES+=("$name"); ASSET_HASHES+=("${digest,,}")
  done <"$WORK/SHA256SUMS"
  (( ${#ASSET_NAMES[@]} )) || die 'The release checksum manifest is empty.'
}

select_asset() {
  local pattern=$1 description=$2 count=0 i
  ASSET=''; HASH=''
  for i in "${!ASSET_NAMES[@]}"; do
    if [[ ${ASSET_NAMES[i]} =~ $pattern ]]; then
      ASSET=${ASSET_NAMES[i]}; HASH=${ASSET_HASHES[i]}; count=$((count + 1))
    fi
  done
  if (( count == 0 )); then
    for i in "${!ASSET_NAMES[@]}"; do
      if [[ ${ASSET_NAMES[i]} == omarchy-notes-rust-*.pkg.tar.zst ]]; then
        die "$TAG uses the retired package name. Select a newer release containing omarchy-notes; --source cannot repair missing renamed release artifacts. Nothing was installed."
      fi
    done
    die "No $description in $TAG. Try --source for a missing binary; downloads never trigger an automatic build."
  fi
  (( count == 1 )) || die "Ambiguous release: found $count assets for $description."
}

download_verified() {
  local name=$1 digest=$2 destination=$3
  step "Downloading $name" "${CURL[@]}" --output "$destination" "$BASE/$name" ||
    die "Could not download $name. Check the release URL and network, then retry."
  printf '%s  %s\n' "$digest" "$destination" >"$WORK/checksum"
  step 'Verifying checksum' sha256sum --check --status "$WORK/checksum" ||
    die "Checksum failed for $name. Nothing was installed."
}

verify_package() {
  local path=$1 expected=$2 metadata architecture
  metadata=$(LC_ALL=C pacman -Qp "$path" </dev/null 2>>"$LOG") || die 'Could not read package identity.'
  [[ $metadata == "$PACKAGE_NAME $expected" ]] || die "Unexpected package identity: $metadata. Nothing was installed."
  LC_ALL=C pacman -Qip "$path" </dev/null >"$WORK/package-info" 2>>"$LOG" || die 'Could not read package architecture.'
  architecture=$(awk '/^Architecture[[:space:]]*:/ {print $3}' "$WORK/package-info")
  [[ $architecture == "$ARCH" ]] || die "Unexpected package architecture: $architecture. Nothing was installed."
  ok "Package identity verified $BULLET $expected $BULLET $ARCH"
}

build_source() {
  local recipe=$1 recipe_hash=$2 source=$3 source_hash=$4 entry
  download_verified "$recipe" "$recipe_hash" "$WORK/$recipe"
  download_verified "$source" "$source_hash" "$WORK/$source"
  tar -tzf "$WORK/$recipe" </dev/null >"$WORK/recipe-files" 2>>"$LOG" || die 'Could not inspect the AUR archive.'
  while IFS= read -r entry; do
    case "$entry" in aur/|aur/PKGBUILD|aur/.SRCINFO) ;; *) die 'Unexpected path in the AUR archive.' ;; esac
  done <"$WORK/recipe-files"
  tar -xzf "$WORK/$recipe" -C "$WORK" --no-same-owner --no-same-permissions </dev/null >>"$LOG" 2>&1 || die 'Could not extract the AUR recipe.'
  [[ -f $WORK/aur/PKGBUILD && ! -L $WORK/aur/PKGBUILD && -f $WORK/aur/.SRCINFO && ! -L $WORK/aur/.SRCINFO ]] || die 'The AUR recipe is incomplete.'
  mv -- "$WORK/$source" "$WORK/aur/$source"
  note "Build dependencies: ${BUILD_DEPS[*]}"
  note 'Administrator access may be requested to install build dependencies.'
  privileged pacman -S --needed "${PACMAN_FLAGS[@]}" "${BUILD_DEPS[@]}" || die 'Could not install build dependencies.'
  command -v makepkg >/dev/null || die 'makepkg is missing after installing base-devel.'
  mkdir -p "$WORK/packages" "$WORK/build"
  # The child shell expands its own positional arguments.
  # shellcheck disable=SC2016
  step 'Building Omarchy Notes (this may take a few minutes)' bash -c '
    cd "$1" || exit 1
    exec env SRCDEST="$1" PKGDEST="$2" BUILDDIR="$3" makepkg --cleanbuild --noconfirm
  ' _ "$WORK/aur" "$WORK/packages" "$WORK/build" || die 'Source build failed.'
  local candidates=() path
  for path in "$WORK/packages/"*.pkg.tar.zst; do
    [[ -f $path ]] || continue
    if [[ ${path##*/} =~ $PACKAGE_PATTERN ]]; then candidates+=("$path"); fi
  done
  (( ${#candidates[@]} == 1 )) || die 'The build did not produce exactly one matching application package.'
  PACKAGE=${candidates[0]}
  PACKAGE_VERSION=${PACKAGE##*/}; PACKAGE_VERSION=${PACKAGE_VERSION#"$PACKAGE_NAME"-}; PACKAGE_VERSION=${PACKAGE_VERSION%-"$ARCH".pkg.tar.zst}
}

finish() {
  local binary='' preview_alias='' package_name path count=0 alias_count=0 output shadow data
  pacman -Ql "$PACKAGE_NAME" </dev/null >"$WORK/installed-files" 2>>"$LOG" || die 'Installed package file list is unavailable.'
  while read -r package_name path; do
    if [[ $package_name == "$PACKAGE_NAME" && $path == /*/bin/"$EXECUTABLE" ]]; then
      binary=$path; count=$((count + 1))
    fi
    if [[ $package_name == "$PACKAGE_NAME" && $path == /*/bin/omarchy-notes-preview ]]; then
      preview_alias=$path; alias_count=$((alias_count + 1))
    fi
  done <"$WORK/installed-files"
  (( count == 1 )) && [[ -x $binary ]] || die 'The installed application executable is missing or ambiguous.'
  output=$("$binary" --version </dev/null 2>>"$LOG") ||
    die 'Package installed, but application verification failed. See the log before launching it.'
  [[ $output == "$EXECUTABLE $VERSION" ]] || die "Installed application reports an unexpected version: $output."
  ok "Application verified $BULLET $output"
  shadow=$(command -v "$EXECUTABLE" || true)
  if [[ -n $shadow && $(readlink -f -- "$shadow") != "$(readlink -f -- "$binary")" ]]; then
    warn "An older or separate copy at $shadow takes precedence on PATH."
    note "Use $binary for the package version; move the older copy aside when ready."
  fi
  (( alias_count == 1 )) && [[ -x $preview_alias ]] || die 'The package-owned preview compatibility launcher is missing or ambiguous.'
  output=$("$preview_alias" --version </dev/null 2>>"$LOG") || die 'The preview compatibility launcher could not execute Omarchy Notes.'
  [[ $output == "$EXECUTABLE $VERSION" ]] || die "The preview compatibility launcher reports an unexpected version: $output."
  shadow=$(command -v omarchy-notes-preview || true)
  if [[ -n $shadow && $(readlink -f -- "$shadow") != "$(readlink -f -- "$preview_alias")" ]]; then
    warn "An older preview at $shadow takes precedence on PATH; move that specific copy aside when ready."
  fi
  if [[ $QUICK == yes ]]; then
    note 'Quick Note is available in the desktop launcher or with omarchy-notes-rust --quick-note.'
    note 'Automatic compositor setup is not implemented; use the distinct-key instructions in docs/rust-release.md. No bindings were changed.'
  fi
  data=${XDG_DATA_HOME:-$HOME/.local/share}/omarchy-notes-gpui
  [[ $data == /* ]] || data=$PWD/$data
  printf '\n  %sReady to write%s\n' "$BOLD" "$RESET"
  if (( IS_OMARCHY )); then printf '     Super + Space, then type "%s"\n' "$DISPLAY_NAME"; fi
  printf '     Or run %s\n' "$binary"
  note 'If Notes is open, close and reopen it normally to use the installed version.'
  printf '\n  %sYour notes%s\n     %s\n' "$BOLD" "$RESET" "$data"
  printf '\n  %sUpdate%s\n     Run this installer again.\n' "$BOLD" "$RESET"
  printf '\n  %sRemove the app%s\n     sudo pacman -R %s\n' "$BOLD" "$RESET" "$PACKAGE_NAME"
  note 'Removing the package keeps your notes.'
  printf '\n'
}

main() {
  presentation
  YES=0; DRY=0; SOURCE=0; QUICK=no; TAG=''; WORK=''; LOG=''; ACTIVE_PID=''; RETAIN_LOG=0
  local quick_option='' option
  while (( $# )); do
    option=$1
    case "$option" in
      --help|-h) usage; return 0 ;;
      --yes|-y) YES=1 ;;
      --dry-run) DRY=1 ;;
      --source) SOURCE=1 ;;
      --version) shift; (( $# )) && [[ -n $1 ]] || die '--version requires a release tag.'; TAG=$1 ;;
      --version=*) TAG=${option#*=}; [[ -n $TAG ]] || die '--version requires a release tag.' ;;
      --quick-note|--no-quick-note)
        [[ -z $quick_option || $quick_option == "$option" ]] || die 'Choose either --quick-note or --no-quick-note.'
        quick_option=$option
        if [[ $option == --quick-note ]]; then QUICK=yes; else QUICK=no; fi ;;
      *) die "Unknown option: $option (try --help)." ;;
    esac
    shift
  done
  PACKAGE_NAME=omarchy-notes; EXECUTABLE=omarchy-notes-rust; TAG_PREFIX=rust-v; DISPLAY_NAME='Omarchy Notes'
  [[ -z $TAG || $TAG =~ ^${TAG_PREFIX}[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "--version must be a stable release tag such as ${TAG_PREFIX}1.0.0."
  HAVE_TTY=0
  if ( : </dev/tty ) 2>/dev/null; then HAVE_TTY=1; fi
  if (( ! DRY )); then
    (( EUID != 0 )) || die 'Run this installer as your normal user, without sudo.'
    (( HAVE_TTY || YES )) || die 'No interactive terminal. Use --yes to install unattended.'
  fi
  [[ $(uname -s) == Linux ]] || die 'This installer supports Linux. See the README for build instructions.'
  IS_OMARCHY=0
  if [[ -d /usr/share/omarchy ]] || command -v omarchy >/dev/null 2>&1; then IS_OMARCHY=1; fi
  [[ -f /etc/arch-release ]] || die 'This installer supports Omarchy and Arch Linux. See the README for a Cargo build.'
  ARCH=$(uname -m)
  [[ $ARCH == x86_64 ]] || die "Unsupported architecture: $ARCH. Omarchy Notes currently supports x86_64 only."
  local tool
  for tool in curl sha256sum pacman setsid mktemp awk sed tail readlink python3; do
    command -v "$tool" >/dev/null || die "Missing prerequisite: $tool. Install it and try again."
  done
  if (( ! DRY )); then command -v sudo >/dev/null || die 'sudo is required to install the package.'; fi
  if (( SOURCE )); then command -v tar >/dev/null || die 'tar is required to build from source.'; fi
  trap cleanup EXIT
  trap 'die "Interrupted." 130' INT
  trap 'die "Interrupted." 143' TERM
  trap 'die "Unexpected failure at installer line $LINENO."' ERR
  WORK=$(mktemp -d "${TMPDIR:-/tmp}/omarchy-notes-install.XXXXXX")
  LOG=$(mktemp "${TMPDIR:-/tmp}/omarchy-notes-install.XXXXXX.log")
  CURL=(curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https'
    --retry 2 --retry-delay 1 --connect-timeout 10 --max-time 180)
  local repo_url=https://github.com/last-refuge/omarchy-notes version_pattern
  printf '\n  %s%somarchy notes%s  %s%s  A place for your thoughts.%s\n\n' "$ACCENT" "$BOLD" "$RESET" "$DIM" "$BULLET" "$RESET"
  if (( IS_OMARCHY )); then ok "Omarchy detected $BULLET $ARCH"; else ok "Arch Linux detected $BULLET $ARCH"; fi
  if [[ -z $TAG ]]; then
    # Avoid a stale cached release list immediately after a release is published.
    step 'Finding the latest published release' "${CURL[@]}" --output "$WORK/releases.json" \
      "https://api.github.com/repos/last-refuge/omarchy-notes/releases?per_page=100&refresh=$(date +%s)" ||
      die 'Could not resolve the latest release. Check GitHub and your network, or select --version TAG.'
    TAG=$(python3 - "$WORK/releases.json" "$TAG_PREFIX" <<'PY'
import json, re, sys
with open(sys.argv[1], encoding="utf-8") as source:
    releases = json.load(source)
if not isinstance(releases, list):
    raise ValueError("GitHub did not return a release list")
pattern = re.compile(re.escape(sys.argv[2]) + r"[0-9]+\.[0-9]+\.[0-9]+")
matches = [release for release in releases if isinstance(release, dict)
           and not release.get("draft") and not release.get("prerelease")
           and isinstance(release.get("tag_name"), str)
           and pattern.fullmatch(release["tag_name"])
           and isinstance(release.get("published_at"), str)]
if not matches:
    sys.exit(1)
print(max(matches, key=lambda release: release["published_at"])["tag_name"])
PY
    ) ||
      die 'No supported published release was found. Select --version TAG or check GitHub Releases.'
  fi
  VERSION=${TAG#"$TAG_PREFIX"}; BASE=$repo_url/releases/download/$TAG
  version_pattern=${VERSION//./\\.}
  PACKAGE_PATTERN="^${PACKAGE_NAME}-${version_pattern}-[0-9]+(\\.[0-9]+)?-${ARCH}\\.pkg\\.tar\\.zst$"
  ok "Release $TAG selected"
  step 'Reading release checksums' "${CURL[@]}" --output "$WORK/SHA256SUMS" "$BASE/SHA256SUMS" ||
    die "Could not download checksums for $TAG. Check that the release exists and the network is available."
  read_manifest
  PACMAN_FLAGS=(); (( ! YES )) || PACMAN_FLAGS+=(--noconfirm)
  BUILD_DEPS=(base-devel rustup clang cmake pkgconf git vulkan-headers python glib2 cairo pango libxcb
    libxkbcommon libxkbcommon-x11 wayland vulkan-icd-loader hicolor-icon-theme noto-fonts-cjk noto-fonts-emoji xdg-desktop-portal xdg-utils)
  local recipe='' recipe_hash='' source='' source_hash='' installed=''
  if (( SOURCE )); then
    select_asset "^${PACKAGE_NAME}-${version_pattern}-aur\\.tar\\.gz$" 'AUR recipe'
    recipe=$ASSET; recipe_hash=$HASH
    select_asset "^${PACKAGE_NAME}-${version_pattern}\\.tar\\.gz$" 'source archive'
    source=$ASSET; source_hash=$HASH
    note "Build $TAG as your user, then install with pacman."
    note "Build dependencies: ${BUILD_DEPS[*]}"
  else
    select_asset "$PACKAGE_PATTERN" "$ARCH application package"
    PACKAGE=$WORK/$ASSET
    PACKAGE_VERSION=${ASSET#"$PACKAGE_NAME"-}; PACKAGE_VERSION=${PACKAGE_VERSION%-"$ARCH".pkg.tar.zst}
    installed=$(pacman -Q "$PACKAGE_NAME" </dev/null 2>/dev/null || true)
    if [[ $installed == "$PACKAGE_NAME $PACKAGE_VERSION" ]]; then note "Package $PACKAGE_VERSION is already installed."; fi
  fi
  if (( DRY )); then
    note 'Dry run: release inspected; no packages or configuration will be changed.'
    if (( SOURCE )); then
      note "Would download and verify $recipe and $source."
      note "Would run sudo pacman -S --needed ${BUILD_DEPS[*]}, then makepkg --cleanbuild."
    elif [[ $installed != "$PACKAGE_NAME $PACKAGE_VERSION" ]]; then
      note "Would download and verify $ASSET, and check its package identity."
    fi
    if (( SOURCE )) || [[ $installed != "$PACKAGE_NAME $PACKAGE_VERSION" ]]; then note 'Would run sudo pacman -U <verified-package>.'; fi
    note 'Would verify the package-owned executable with --version.'
    if [[ $QUICK == yes ]]; then note 'Would show the desktop Quick Note action and manual distinct-key setup instructions.'; fi
    printf '\n'; return 0
  fi
  if (( SOURCE )) || [[ $installed != "$PACKAGE_NAME $PACKAGE_VERSION" ]]; then
    if ! confirm "Install Omarchy Notes $TAG?" Y/n; then note 'Installation cancelled.'; return 0; fi
    if (( SOURCE )); then build_source "$recipe" "$recipe_hash" "$source" "$source_hash"
    else download_verified "$ASSET" "$HASH" "$PACKAGE"; fi
    verify_package "$PACKAGE" "$PACKAGE_VERSION"
    printf '\n  %sInstalling the package%s\n' "$BOLD" "$RESET"
    note 'Administrator access may be requested.'
    privileged pacman -U "${PACMAN_FLAGS[@]}" "$PACKAGE" || die 'pacman could not install the package. Check the transaction above.'
    ok 'Omarchy Notes installed'
  fi
  finish
}

main "$@"
