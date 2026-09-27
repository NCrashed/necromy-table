#!/usr/bin/env bash
# Builds a champion sheet for the game from a PixelLab character.
#
#   scripts/pixellab-sheet.sh CHARACTER_ID GOD [--still-north]
#
# --still-north: the idle facing north is the still north rotation. From
# behind breathing hardly shows, and skeleton-v3 sometimes turns the
# figure to face the viewer there.
#
# Downloads the character (it must have finished `idle` and `walk`
# animations in south, east, north and west) and writes
# assets/sprites/<god>-champion.png: 96×96 frames, rows idle S, E, N, W, then
# walk S, E, N, W, six columns (idle's four frames padded). `token.rs`
# reads exactly this layout.
set -euo pipefail
id="$1"
god="$2"
still_north="${3:-}"
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -sf -o "$work/char.zip" "https://api.pixellab.ai/mcp/characters/$id/download" \
  || { echo "download failed (HTTP 423 means jobs are still running)" >&2; exit 1; }
nix shell nixpkgs#unzip -c unzip -q "$work/char.zip" -d "$work/char"
anims="$(dirname "$(find "$work/char" -type d -name animations | head -1)")/animations"

# One row per animation and direction, six 96×96 cells each. skeleton-v3
# frames are 96 px; 64 px frames are centred on that canvas, which puts
# their feet on the same row.
args=()
for anim in idle walk; do
  for dir in south east north west; do
    args+=("(")
    for i in 0 1 2 3 4 5; do
      f="$anims/$anim/$dir/frame_00$i.png"
      if [ "$still_north" = "--still-north" ] && [ "$anim$dir" = "idlenorth" ] && [ "$i" -lt 4 ]; then
        f="$(dirname "$anims")/rotations/north.png"
      fi
      if [ -f "$f" ]; then
        args+=("(" "$f" -background none -gravity center -extent 96x96 ")")
      else
        args+=(-size 96x96 xc:none)
      fi
    done
    args+=(+append ")")
  done
done

mkdir -p "$root/assets/sprites"
out="$root/assets/sprites/$god-champion.png"
nix shell nixpkgs#imagemagick -c magick -background none "${args[@]}" -append +repage "PNG32:$out"
echo "wrote $out"
