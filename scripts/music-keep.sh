#!/usr/bin/env bash
# Keep takes of the music set as assets/music/NAME.ogg.
#
#   scripts/music-keep.sh              # every track in art/music.txt not kept yet
#   scripts/music-keep.sh NAME [SEED]  # (re)keep one track: SEED's take, or the best
#   scripts/music-keep.sh --relevel    # re-encode every kept take (after a level change)
#
# The best take is picked by scripts/music_pick.py (clean loop point, no
# silence, instruments in tune). Every kept piece is brought to the same
# loudness (-23 LUFS) by one fixed gain, never above -1 dBFS peak: a gain
# that moves would break the loop. The take kept for each piece is written
# to art/music-kept.txt.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
sa3="${SA3_DIR:-$HOME/models/stable-audio-3}"
out="$root/assets/music"
kept="$root/art/music-kept.txt"
target=-23
mkdir -p "$out"
: "${NECROMY_PYLIBS:?run inside nix develop}"

keep() { # NAME [SEED]
  local name="$1" take
  if [ -n "${2:-}" ]; then
    take="$root/art/audio/$name-$2.wav"
  else
    ls "$root/art/audio/$name"-*.wav > /dev/null 2>&1 || { echo "$name: no takes"; return; }
    take="$(LD_LIBRARY_PATH="$NECROMY_PYLIBS" "$sa3/.venv/bin/python" \
      "$root/scripts/music_pick.py" "$root/art/audio/$name"-*.wav | head -1 | cut -d' ' -f2)"
  fi
  local stats lufs peak gain
  stats="$(ffmpeg -nostdin -hide_banner -nostats -i "$take" -af ebur128=peak=true -f null - 2>&1)"
  lufs="$(grep -E '^\s+I:' <<< "$stats" | tail -1 | awk '{print $2}')"
  peak="$(grep -E '^\s+Peak:' <<< "$stats" | tail -1 | awk '{print $2}')"
  gain="$(awk -v l="$lufs" -v p="$peak" -v t="$target" 'BEGIN { g = t - l; if (p + g > -1) g = -1 - p; printf "%.2f", g }')"
  ffmpeg -nostdin -loglevel error -y -i "$take" -af "volume=${gain}dB" -c:a libvorbis -q:a 5 "$out/$name.ogg"
  # Remember the take: one line per piece, sorted, the header kept on top.
  touch "$kept"
  { grep '^#' "$kept" || true
    { grep -v '^#' "$kept" | awk -F'\t' -v n="$name" '$1 != n' || true
      printf '%s\t%s\n' "$name" "$(basename "$take")"; } | sort; } > "$kept.tmp"
  mv "$kept.tmp" "$kept"
  echo "$name <- $(basename "$take")  (${lufs} LUFS, ${gain} dB)"
}

if [ "${1:-}" = --relevel ]; then
  grep -v '^#' "$kept" | while IFS=$'\t' read -r name take; do
    seed="${take#"$name"-}"
    keep "$name" "${seed%.wav}"
  done
  exit
fi
if [ $# -gt 0 ]; then
  keep "$@"
  exit
fi
grep -v '^#' "$root/art/music.txt" | cut -f1 | while read -r name; do
  [ -n "$name" ] && [ ! -e "$out/$name.ogg" ] && keep "$name"
done
