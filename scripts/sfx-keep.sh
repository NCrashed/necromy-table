#!/usr/bin/env bash
# Keep the sound effect takes as assets/sfx/NAME-K.ogg.
#
#   scripts/sfx-keep.sh [NAME...]   # default: every sound in art/sfx.txt
#
# Every take of a sound is kept (the game picks among them at random), numbered
# afresh; delete a bad take from art/audio/sfx/ and rerun. Leading silence is
# cut (a late sound feels laggy), the tail below -50 dB trimmed with a short
# fade, and the peak brought to -1 dBFS: loudness between sounds is set in code.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/art/audio/sfx"
out="$root/assets/sfx"
mkdir -p "$out"

names=("$@")
if [ ${#names[@]} -eq 0 ]; then
  mapfile -t names < <(grep -v '^#' "$root/art/sfx.txt" | cut -f1 | grep .)
fi

trim="silenceremove=start_periods=1:start_threshold=-50dB,areverse,silenceremove=start_periods=1:start_threshold=-50dB,afade=t=in:d=0.02,areverse"
for name in "${names[@]}"; do
  rm -f "$out/$name"-*.ogg
  k=1
  for take in $(ls "$src/$name"-*.wav 2>/dev/null | sort -V); do
    tmp="$(mktemp --suffix=.wav)"
    ffmpeg -nostdin -loglevel error -y -i "$take" -af "$trim" "$tmp"
    peak="$(ffmpeg -nostdin -hide_banner -i "$tmp" -af astats=metadata=0 -f null - 2>&1 \
      | grep 'Peak level dB' | tail -1 | awk '{print $NF}')"
    gain="$(awk -v p="$peak" 'BEGIN { printf "%.2f", -1 - p }')"
    ffmpeg -nostdin -loglevel error -y -i "$tmp" -af "volume=${gain}dB" -c:a libvorbis -q:a 5 "$out/$name-$k.ogg"
    rm -f "$tmp"
    k=$((k + 1))
  done
  echo "$name: $((k - 1))"
done
