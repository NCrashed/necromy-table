#!/usr/bin/env bash
# Builds a menu animal's strip from an animated PixelLab object.
#
#   scripts/pixellab-strip.sh OBJECT_ID NAME [CELL]
#
# Downloads the object and writes assets/props/menu/animal-<NAME>.png: its
# still picture, then every frame of its (first) animation, in one row of
# CELL px cells (32 unless given). `menu_world.rs` shows frame 0 standing
# and the rest moving.
set -euo pipefail
id="$1"
name="$2"
cell="${3:-32}"
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -sf -o "$work/obj.zip" "https://api.pixellab.ai/mcp/objects/$id/download" \
  || { echo "download failed (HTTP 423 means jobs are still running)" >&2; exit 1; }
nix shell nixpkgs#unzip -c unzip -q "$work/obj.zip" -d "$work/obj"
frames="$(find "$work/obj/animations" -name 'frame_*.png' | sort)"
[ -n "$frames" ] || { echo "no animation frames in $id" >&2; exit 1; }
args=("$work/obj/rotations/unknown.png")
# The animation's frame 0 is the still picture again: skip it.
for f in $(echo "$frames" | tail -n +2); do
  args+=("$f")
done
out="$root/assets/props/menu/animal-$name.png"
nix shell nixpkgs#imagemagick -c magick -background none "${args[@]}" \
  -gravity center -extent "${cell}x${cell}" +append +repage "PNG32:$out"
echo "wrote $out"
