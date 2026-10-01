#!/usr/bin/env bash
# Builds a row of frames for the menu's world from a PixelLab
# `animate_image` job (a loose picture animated, no object needed).
#
#   scripts/pixellab-image-strip.sh JOB_ID STEM [ROWS]
#
# Downloads every frame of the job (frame 0 is the input picture) and writes
# assets/props/menu/<STEM>.png (say `tree-oak`) as one row. ROWS keeps only
# the top ROWS rows of each frame, the rest cleared: the new trees came with a
# band of soil across the whole bottom, cut off their stills the same way.
set -euo pipefail
job="$1"
stem="$2"
rows="${3:-}"
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
magick() { nix shell nixpkgs#imagemagick -c magick "$@"; }

i=0
while curl -sf -o "$work/$(printf %03d $i).png" \
  "https://api.pixellab.ai/mcp/images/$job/download?index=$i"; do
  i=$((i + 1))
done
[ "$i" -gt 1 ] || { echo "no frames for $job (still running?)" >&2; exit 1; }

frames=("$work"/*.png)
if [ -n "$rows" ]; then
  read -r w h < <(magick "${frames[0]}" -format "%w %h\n" info:)
  for f in "${frames[@]}"; do
    magick "$f" -crop "${w}x${rows}+0+0" +repage -background none -extent "${w}x${h}" "PNG32:$f"
  done
fi
out="$root/assets/props/menu/$stem.png"
magick -background none "${frames[@]}" +repage +append "PNG32:$out"
echo "wrote $out ($i frames)"
