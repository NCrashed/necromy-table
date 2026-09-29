#!/usr/bin/env bash
# Builds a champion's main menu sheet from a PixelLab character.
#
#   scripts/pixellab-menu.sh CHARACTER_ID GOD ANIMATION...
#
# Downloads the character and writes assets/sprites/<god>-menu.png: one row
# per named animation (facing south), frames in 96×96 cells left to right,
# 12 columns (unused cells empty). `menu_stage.rs` reads rows in the order
# it names them: wave first, then the god's own act.
set -euo pipefail
id="$1"
god="$2"
shift 2
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -sf -o "$work/char.zip" "https://api.pixellab.ai/mcp/characters/$id/download" \
  || { echo "download failed (HTTP 423 means jobs are still running)" >&2; exit 1; }
nix shell nixpkgs#unzip -c unzip -q "$work/char.zip" -d "$work/char"
anims="$(dirname "$(find "$work/char" -type d -name animations | head -1)")/animations"

args=()
for anim in "$@"; do
  dir="$anims/$anim/south"
  [ -d "$dir" ] || { echo "no $anim/south in the download" >&2; ls "$anims" >&2; exit 1; }
  args+=("(")
  for i in $(seq 0 11); do
    f="$dir/frame_$(printf %03d "$i").png"
    if [ -f "$f" ]; then
      args+=("(" "$f" -background none -gravity center -extent 96x96 ")")
    else
      args+=(-size 96x96 xc:none)
    fi
  done
  args+=(+append ")")
done

out="$root/assets/sprites/$god-menu.png"
nix shell nixpkgs#imagemagick -c magick -background none "${args[@]}" -append +repage "PNG32:$out"
echo "wrote $out"
