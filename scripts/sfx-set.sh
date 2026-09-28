#!/usr/bin/env bash
# Generate the sound effects listed in art/sfx.txt with ElevenLabs.
#
#   scripts/sfx-set.sh [NAME...]
#
# Sounds that already have takes in art/audio/sfx/ are skipped unless named
# on the command line (named ones get their variants added). Then keep them
# with scripts/sfx-keep.sh.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
grep -v '^#' "$root/art/sfx.txt" | while IFS=$'\t' read -r name secs count prompt; do
  [ -n "$name" ] || continue
  if [ $# -gt 0 ]; then
    [[ " $* " == *" $name "* ]] || continue
  elif ls "$root/art/audio/sfx/$name"-*.wav > /dev/null 2>&1; then
    continue
  fi
  echo "== $name"
  "$root/scripts/elevenlabs-sfx.sh" -d "$secs" -n "$count" "$name" "$prompt" < /dev/null
done
