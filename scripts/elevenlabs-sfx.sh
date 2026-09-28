#!/usr/bin/env bash
# Sound effects with ElevenLabs text-to-sound (eleven_text_to_sound_v2).
#
#   scripts/elevenlabs-sfx.sh [options] NAME "prompt"
#
#   -d SECONDS  length 0.5..30 (default: the model guesses; costs more)
#   -p INFLUENCE  prompt influence 0..1 (default 0.5: literal, less drift)
#   -n COUNT    number of variants (default 3)
#   -l          seamless loop
#   -S          skip the shared style prefix from art/sfx-style.txt
#
# Writes art/audio/sfx/NAME-K.wav (gitignored, 44.1 kHz stereo) and prints the
# credits spent. Keep a take with scripts/sfx-keep.sh. Needs ELEVENLABS_API_KEY.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out_dir="$root/art/audio/sfx"
style_file="$root/art/sfx-style.txt"

duration=""
influence=0.5
count=3
loop=false
use_style=1

usage() { sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

while getopts "d:p:n:lSh" opt; do
  case "$opt" in
    d) duration="$OPTARG" ;;
    p) influence="$OPTARG" ;;
    n) count="$OPTARG" ;;
    l) loop=true ;;
    S) use_style=0 ;;
    h) usage 0 ;;
    *) usage 1 ;;
  esac
done
shift $((OPTIND - 1))
[ $# -ge 2 ] || usage 1
name="$1"; shift
prompt="$*"
: "${ELEVENLABS_API_KEY:?put ELEVENLABS_API_KEY in .env}"

if [ "$use_style" = 1 ] && [ -s "$style_file" ]; then
  prompt="$prompt. $(tr '\n' ' ' < "$style_file" | sed 's/ *$//')"
fi

used() {
  curl -sf -H "xi-api-key: $ELEVENLABS_API_KEY" \
    https://api.elevenlabs.io/v1/user/subscription | jq -r .character_count || echo 0
}

mkdir -p "$out_dir"
body="$(jq -n --arg t "$prompt" --arg d "$duration" --argjson p "$influence" --argjson l "$loop" \
  '{text: $t, prompt_influence: $p, loop: $l, model_id: "eleven_text_to_sound_v2"}
   + (if $d == "" then {} else {duration_seconds: ($d | tonumber)} end)')"
before="$(used)"
echo "prompt: $prompt"

# Number variants after the ones already there, so reruns add rather than replace.
k=1
while [ -e "$out_dir/$name-$k.wav" ]; do k=$((k + 1)); done
for _ in $(seq "$count"); do
  raw="$(mktemp)"
  code="$(curl -s -o "$raw" -w '%{http_code}' \
    -X POST 'https://api.elevenlabs.io/v1/sound-generation?output_format=pcm_44100' \
    -H "xi-api-key: $ELEVENLABS_API_KEY" -H 'Content-Type: application/json' \
    -d "$body")"
  if [ "$code" != 200 ]; then
    echo "HTTP $code: $(head -c 400 "$raw")" >&2
    rm -f "$raw"
    exit 1
  fi
  ffmpeg -nostdin -loglevel error -y -f s16le -ar 44100 -ac 2 -i "$raw" "$out_dir/$name-$k.wav"
  rm -f "$raw"
  echo "$out_dir/$name-$k.wav"
  k=$((k + 1))
done
echo "credits: $(( $(used) - before ))"
