#!/usr/bin/env bash
# Builds a champion's fight sheet from a PixelLab character.
#
#   scripts/pixellab-fight.sh CHARACTER_ID GOD [ATTACK_A ATTACK_B HURT_A HURT_B DEATH]
#
# The five names are the character's animations (east only), by default
# attack-a attack-b hurt-punch hurt-stagger death. Writes
# assets/sprites/<god>-fight.png: 96×96 cells, one row per animation in
# that order, up to ten frames each, the rest of a row empty. `fight.rs`
# counts the frames of a row from the image.
set -euo pipefail
id="$1"
god="$2"
shift 2
names=("${@:-}")
if [ "${#names[@]}" -ne 5 ]; then
  names=(attack-a attack-b hurt-punch hurt-stagger death)
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# PIXELLAB_ZIP: a character zip already downloaded (the download is
# refused while any job of the character runs).
if [ -n "${PIXELLAB_ZIP:-}" ]; then
  cp "$PIXELLAB_ZIP" "$work/char.zip"
else
  curl -sf -o "$work/char.zip" "https://api.pixellab.ai/mcp/characters/$id/download" \
    || { echo "download failed (HTTP 423 means jobs are still running)" >&2; exit 1; }
fi
nix shell nixpkgs#unzip -c unzip -q "$work/char.zip" -d "$work/char"
anims="$(dirname "$(find "$work/char" -type d -name animations | head -1)")/animations"

args=()
for anim in "${names[@]}"; do
  dir="$anims/$anim/east"
  [ -d "$dir" ] || { echo "no east frames for $anim in $anims" >&2; ls "$anims" >&2; exit 1; }
  args+=("(")
  for i in 0 1 2 3 4 5 6 7 8 9; do
    f="$dir/frame_$(printf %03d "$i").png"
    if [ -f "$f" ]; then
      # Frames of any size, centred on the 96 px cell (feet stay level
      # within one animation).
      args+=("(" "$f" -background none -gravity center -extent 96x96 ")")
    else
      args+=(-size 96x96 xc:none)
    fi
  done
  args+=(+append ")")
done

mkdir -p "$root/assets/sprites"
out="$root/assets/sprites/$god-fight.png"
nix shell nixpkgs#imagemagick -c magick -background none "${args[@]}" -append +repage "PNG32:$out"
echo "wrote $out"
