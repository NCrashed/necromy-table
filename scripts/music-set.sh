#!/usr/bin/env bash
# Generate the whole music set listed in art/music.txt as seamless loops.
#
#   scripts/music-set.sh [-n TAKES] [NAME...]
#
# Each listed track gets TAKES variants (default 2) in art/audio/NAME-SEED.wav;
# tracks that already have a take are skipped unless named on the command
# line. Pick one per track and keep it as assets/music/NAME.ogg.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
takes=2
while getopts "n:h" opt; do
  case "$opt" in
    n) takes="$OPTARG" ;;
    *) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  esac
done
shift $((OPTIND - 1))

grep -v '^#' "$root/art/music.txt" | while IFS=$'\t' read -r name secs prompt; do
  [ -n "$name" ] || continue
  if [ $# -gt 0 ]; then
    [[ " $* " == *" $name "* ]] || continue
  elif ls "$root/art/audio/$name"-*.wav > /dev/null 2>&1; then
    continue
  fi
  echo "== $name"
  "$root/scripts/stable-audio.sh" -l -d "$secs" -n "$takes" "$name" "$prompt" < /dev/null
done
