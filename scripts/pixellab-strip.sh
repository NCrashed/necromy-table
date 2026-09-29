#!/usr/bin/env bash
# Builds a row of frames for the menu's world from an animated PixelLab
# object.
#
#   scripts/pixellab-strip.sh OBJECT_ID STEM [CELL] [MATCH] [ANIM]
#
# Downloads the object and writes assets/props/menu/<STEM>.png (say
# `animal-hare`, `tree-fir`): its still picture, then every frame of its
# (first) animation, in one row of CELL px cells (32 unless given).
# `menu_world.rs` shows frame 0 still and the rest moving. MATCH is a
# picture the still was moved within its canvas to (a trimmed and
# re-seated one): every frame moves by the same amount ("" for none).
# ANIM picks the animation whose description contains it (its folder in
# the download), for objects with more than one.
set -euo pipefail
id="$1"
stem="$2"
cell="${3:-32}"
match="${4:-}"
anim="${5:-}"
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
magick() { nix shell nixpkgs#imagemagick -c magick "$@"; }

curl -sf -o "$work/obj.zip" "https://api.pixellab.ai/mcp/objects/$id/download" \
  || { echo "download failed (HTTP 423 means jobs are still running)" >&2; exit 1; }
nix shell nixpkgs#unzip -c unzip -q "$work/obj.zip" -d "$work/obj"
frames="$(find "$work/obj/animations" -path "*${anim}*" -name 'frame_*.png' | sort)"
[ -n "$frames" ] || { echo "no animation frames in $id" >&2; exit 1; }
still="$work/obj/rotations/unknown.png"

# How far MATCH sits from the still: the offset of their opaque boxes.
dx=0
dy=0
if [ -n "$match" ]; then
  read -r ox oy < <(magick "$still" -format "%[fx:page.x] %[fx:page.y]\n" -trim info:)
  read -r mx my < <(magick "$match" -format "%[fx:page.x] %[fx:page.y]\n" -trim info:)
  dx=$((mx - ox))
  dy=$((my - oy))
fi

args=()
# The animation's frame 0 is the still picture again: skip it.
for f in "$still" $(echo "$frames" | tail -n +2); do
  args+=("(" -size "${cell}x${cell}" xc:none "(" "$f" -repage "$(printf "%+d%+d" "$dx" "$dy")" ")" -flatten ")")
done
out="$root/assets/props/menu/$stem.png"
magick -background none "${args[@]}" +append +repage "PNG32:$out"
echo "wrote $out"
