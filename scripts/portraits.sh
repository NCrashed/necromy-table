#!/usr/bin/env bash
# Cuts UI portraits out of the champion sheets.
#
#   scripts/portraits.sh
#
# For every assets/sprites/<god>-champion.png: takes the first idle frame
# facing the viewer and cuts a 32×48 bust (head and chest, the 2:3 shape
# every portrait slot in the UI has), centred on the head rather than the
# whole figure, so a staff or a cleaver does not pull it aside. Writes
# assets/portraits/<god>.png. Same pixels as the token, no resampling.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$root/assets/portraits"
cd "$root/assets/sprites"
for sheet in *-champion.png; do
  god="${sheet%-champion.png}"
  nix shell nixpkgs#imagemagick -c bash -c '
    set -euo pipefail
    sheet="$1"; out="$2"
    magick "$sheet" -crop 96x96+0+0 +repage /tmp/portrait-cell.png
    # Top of the figure.
    top=$(magick /tmp/portrait-cell.png -format "%@" info: | sed -E "s/.*\+[0-9]+\+([0-9]+)$/\1/")
    # The head: the first 14 rows of the figure, its centre column.
    head=$(magick /tmp/portrait-cell.png -crop 96x14+0+$top +repage -format "%@" info:)
    hw=$(echo "$head" | sed -E "s/^([0-9]+)x.*/\1/")
    hx=$(echo "$head" | sed -E "s/.*\+([0-9]+)\+[0-9]+$/\1/")
    cx=$(( hx + hw / 2 ))
    x=$(( cx - 16 )); y=$(( top - 2 ))
    [ $x -lt 0 ] && x=0; [ $y -lt 0 ] && y=0
    magick /tmp/portrait-cell.png -crop 32x48+$x+$y +repage -background none -extent 32x48 "PNG32:$out"
    rm -f /tmp/portrait-cell.png
  ' _ "$sheet" "$root/assets/portraits/$god.png"
  echo "wrote assets/portraits/$god.png"
done
