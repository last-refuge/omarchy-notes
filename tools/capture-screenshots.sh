#!/usr/bin/env bash
# Captures the website/README screenshots from the real app with the sample
# library, in stock Omarchy themes. Usage: tools/capture-screenshots.sh BUILD_DIR
set -euo pipefail

build_dir="$(realpath "${1:?usage: capture-screenshots.sh BUILD_DIR}")"
project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$project_dir/docs/images"
themes="${OMARCHY_PATH:-/usr/share/omarchy}/themes"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

shot() { # name theme gallery view
  local name="$1" theme="$2" gallery="$3" view="$4"
  local home="$work/$name"
  mkdir -p "$home/config/omarchy-notes" "$home/runtime"
  chmod 700 "$home/runtime"
  printf '[list]\ngallery=%s\nview=%s\n' "$gallery" "$view" > "$home/config/omarchy-notes/settings.ini"
  env -u QT_QPA_PLATFORMTHEME -u HYPRLAND_INSTANCE_SIGNATURE \
    QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
    XDG_CONFIG_HOME="$home/config" XDG_RUNTIME_DIR="$home/runtime" \
    OMARCHY_NOTES_THEME_DIR="$themes/$theme" \
    ONOTES_SCREENSHOT="$out/$name.png" \
    "$build_dir/src/app/omarchy-notes" --data-dir "$home/library" --samples >/dev/null
  echo "wrote docs/images/$name.png ($theme)"
}

shot screenshot-dark kanagawa false all
shot screenshot-light catppuccin-latte false all
shot screenshot-gallery rose-pine true all
